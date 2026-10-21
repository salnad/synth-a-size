// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::FilterFxModule`: the filter effect of the effect chain, a mono
//! [`FilterModule`] with the `filter_fx_*` controls and no `filter_fx_on`
//! control of its own (the chain owns the `_on` switch).
//!
//! Ports the `FilterFxModule` class defined in
//! `vital/src/synthesis/modules/reorderable_effect_chain.cpp`.

use std::rc::Rc;

use sas_dsp_core::output::{Buffer, Output};
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::SynthModule;
use sas_filters::modules::FilterModule;
use sas_simd::common::MAX_BUFFER_SIZE;

/// `vital::FilterFxModule`.
#[derive(Clone)]
pub struct FilterFxModule {
    filter: Option<ProcessorRef>,
    input: Rc<Output>,
}

impl FilterFxModule {
    pub const AUDIO: usize = 0;
    pub const KEYTRACK: usize = 1;
    pub const NUM_INPUTS: usize = 2;

    /// `new FilterFxModule(keytrack)`: builds and wires the filter, as the
    /// upstream constructor does.
    pub fn create(keytrack: &Rc<Output>) -> ProcessorRef {
        let input = Output::new_default();
        let module = SynthModule::create_with(
            FilterFxModule {
                filter: None,
                input: input.clone(),
            },
            Self::NUM_INPUTS,
            1,
            false,
        );
        let filter = FilterModule::create_with("filter_fx", |filter| {
            filter.set_create_on_value(false);
            filter.set_mono(true);
        });
        module.add_submodule(&filter);
        module.add_processor(&filter);
        filter.use_output(&module.output(0), 0);
        filter.plug(&input, FilterModule::AUDIO);
        filter.plug(keytrack, FilterModule::KEYTRACK);
        module.with::<FilterFxModule, _>(|kernel, _| kernel.filter = Some(filter));
        module
    }

    fn filter(&self) -> &ProcessorRef {
        self.filter
            .as_ref()
            .expect("FilterFxModule::create sets the filter")
    }
}

impl ProcessorImpl for FilterFxModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        this.router_init();
    }

    fn process_with_input(&mut self, _this: &Processor, audio_in: &Buffer, num_samples: usize) {
        let input = self.input.buffer();
        for i in 0..num_samples {
            input[i].set(audio_in[i].get());
        }
        self.filter().process(num_samples);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        self.input
            .ensure_buffer_size(MAX_BUFFER_SIZE * oversample as usize);
        this.router_set_oversample_amount(oversample);
    }
}
