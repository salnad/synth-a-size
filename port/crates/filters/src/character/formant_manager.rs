// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::FormantManager`: a router of parallel `DigitalSvf` formant
//! stages summed into one output.
//!
//! Ports `vital/src/synthesis/filters/formant_manager.h` and
//! `formant_manager.cpp`.

use sas_dsp_core::operators::VariableAdd;
use sas_dsp_core::sas_simd::PolyMask;
use sas_dsp_core::{processor_boilerplate, Processor, ProcessorImpl, ProcessorRef};

use crate::digital_svf::DigitalSvf;

/// `vital::FormantManager`.
#[derive(Clone)]
pub struct FormantManager {
    /// `formants_`: the global (original) stages; clones reach theirs
    /// through `getLocalProcessor`.
    formants: Vec<ProcessorRef>,
}

impl FormantManager {
    pub const MIN_RESONANCE: f32 = 4.0;
    pub const MAX_RESONANCE: f32 = 30.0;

    /// `new FormantManager(num_formants)`: `ProcessorRouter(0, 1)`.
    pub fn create(num_formants: usize) -> ProcessorRef {
        let formants: Vec<ProcessorRef> = (0..num_formants)
            .map(|_| {
                let mut formant = DigitalSvf::new();
                formant.set_resonance_bounds(Self::MIN_RESONANCE, Self::MAX_RESONANCE);
                DigitalSvf::create_with(formant)
            })
            .collect();
        let router = Processor::new_router(
            FormantManager {
                formants: formants.clone(),
            },
            0,
            1,
            false,
        );
        for formant in &formants {
            router.add_processor(formant);
        }
        router
    }

    /// `getFormant(index)`.
    pub fn get_formant(&self, index: usize) -> ProcessorRef {
        self.formants[index].clone()
    }

    /// `numFormants()`.
    pub fn num_formants(&self) -> usize {
        self.formants.len()
    }
}

impl ProcessorImpl for FormantManager {
    processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let total = VariableAdd::create(self.formants.len());
        for formant in &self.formants {
            total.plug_next(formant);
        }

        this.add_processor(&total);
        total.use_output(&this.output(0), 0);

        this.router_init();
    }

    fn reset(&mut self, this: &Processor, reset_mask: PolyMask) {
        for formant in &self.formants {
            if let Some(local) = this.get_local_processor(formant) {
                local.reset(reset_mask);
            }
        }
    }

    fn hard_reset(&mut self, this: &Processor) {
        for formant in &self.formants {
            if let Some(local) = this.get_local_processor(formant) {
                local.hard_reset();
            }
        }
    }
}
