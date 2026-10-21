// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::CombModule`: the filter section's comb model, a `SynthModule`
//! wrapping one `CombFilter(kMaxFeedbackSamples)`.
//!
//! Ports `vital/src/synthesis/modules/comb_module.h` and `comb_module.cpp`.

use sas_dsp_core::sas_simd::PolyMask;
use sas_dsp_core::{processor_boilerplate, Processor, ProcessorImpl, ProcessorRef, SynthModule};

use super::comb_filter::CombFilter;
use crate::synth_filter::input as filter_input;

/// `vital::CombModule`.
#[derive(Clone, Default)]
pub struct CombModule {
    /// `comb_filter_`: the global (original) filter node; clones reach their
    /// own copy through `getLocalProcessor`.
    comb_filter: Option<ProcessorRef>,
}

impl CombModule {
    pub const MAX_FEEDBACK_SAMPLES: i32 = 25000;

    pub const AUDIO: usize = 0;
    pub const RESET: usize = 1;
    pub const MIDI_CUTOFF: usize = 2;
    pub const MIDI_BLEND_TRANSPOSE: usize = 3;
    pub const FILTER_CUTOFF_BLEND: usize = 4;
    pub const STYLE: usize = 5;
    pub const RESONANCE: usize = 6;
    pub const MIDI: usize = 7;
    pub const NUM_INPUTS: usize = 8;

    /// `new CombModule()`: `SynthModule(kNumInputs, 1)`. Call `init()`
    /// before processing.
    pub fn create() -> ProcessorRef {
        SynthModule::create_with(CombModule::default(), Self::NUM_INPUTS, 1, false)
    }

    fn local_filter(&self, this: &Processor) -> Option<ProcessorRef> {
        let global = self.comb_filter.as_ref()?;
        this.get_local_processor(global)
    }
}

impl ProcessorImpl for CombModule {
    processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let comb_filter = CombFilter::create(Self::MAX_FEEDBACK_SAMPLES);
        this.add_processor(&comb_filter);

        comb_filter.use_input(&this.input(Self::AUDIO), filter_input::AUDIO);
        comb_filter.use_input(&this.input(Self::MIDI_CUTOFF), filter_input::MIDI_CUTOFF);
        comb_filter.use_input(&this.input(Self::STYLE), filter_input::STYLE);
        comb_filter.use_input(
            &this.input(Self::MIDI_BLEND_TRANSPOSE),
            filter_input::TRANSPOSE,
        );
        comb_filter.use_input(
            &this.input(Self::FILTER_CUTOFF_BLEND),
            filter_input::PASS_BLEND,
        );
        comb_filter.use_input(&this.input(Self::RESONANCE), filter_input::RESONANCE);
        comb_filter.use_input(&this.input(Self::RESET), filter_input::RESET);
        comb_filter.use_output(&this.output(0), 0);
        self.comb_filter = Some(comb_filter);

        this.router_init();
    }

    fn reset(&mut self, this: &Processor, reset_mask: PolyMask) {
        if let Some(filter) = self.local_filter(this) {
            filter.reset(reset_mask);
        }
    }

    fn hard_reset(&mut self, this: &Processor) {
        if let Some(filter) = self.local_filter(this) {
            filter.hard_reset();
        }
    }
}
