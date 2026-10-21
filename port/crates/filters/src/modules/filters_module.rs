// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::FiltersModule`: a voice's two filter slots, run in parallel or in
//! series (filter 1 into filter 2, or 2 into 1) by the
//! `filter_<n>_filter_input` controls.
//!
//! Ports `vital/src/synthesis/modules/filters_module.h` and
//! `filters_module.cpp`.

use std::rc::Rc;

use sas_dsp_core::output::Output;
use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef, SynthModule};
use sas_simd::common::MAX_BUFFER_SIZE;

use super::filter_module::FilterModule;

/// `vital::FiltersModule`.
#[derive(Clone)]
pub struct FiltersModule {
    filter_1: Option<ProcessorRef>,
    filter_2: Option<ProcessorRef>,
    filter_1_filter_input: Option<ProcessorRef>,
    filter_2_filter_input: Option<ProcessorRef>,
    filter_1_input: Rc<Output>,
    filter_2_input: Rc<Output>,
}

impl FiltersModule {
    pub const FILTER_1_INPUT: usize = 0;
    pub const FILTER_2_INPUT: usize = 1;
    pub const KEYTRACK: usize = 2;
    pub const MIDI: usize = 3;
    pub const RESET: usize = 4;
    pub const NUM_INPUTS: usize = 5;

    /// `new FiltersModule()`: `SynthModule(kNumInputs, 1)`. Call `init()`
    /// before processing.
    pub fn create() -> ProcessorRef {
        let kernel = FiltersModule {
            filter_1: None,
            filter_2: None,
            filter_1_filter_input: None,
            filter_2_filter_input: None,
            filter_1_input: Output::new_default(),
            filter_2_input: Output::new_default(),
        };
        SynthModule::create_with(kernel, Self::NUM_INPUTS, 1, false)
    }

    /// `getFilter1OnValue()`.
    pub fn get_filter_1_on_value(&self) -> Option<ProcessorRef> {
        Self::on_value(self.filter_1.as_ref())
    }

    /// `getFilter2OnValue()`.
    pub fn get_filter_2_on_value(&self) -> Option<ProcessorRef> {
        Self::on_value(self.filter_2.as_ref())
    }

    fn on_value(filter: Option<&ProcessorRef>) -> Option<ProcessorRef> {
        filter?.with::<FilterModule, _>(|filter, _| filter.get_on_value().cloned())
    }

    fn filters(&self) -> (&ProcessorRef, &ProcessorRef) {
        (
            self.filter_1.as_ref().expect("init ran"),
            self.filter_2.as_ref().expect("init ran"),
        )
    }

    fn local(this: &Processor, filter: &ProcessorRef) -> ProcessorRef {
        this.get_local_processor(filter)
            .expect("filter module is in this router")
    }

    /// `processParallel(num_samples)`.
    fn process_parallel(&self, this: &Processor, num_samples: usize) {
        let (filter_1, filter_2) = self.filters();
        self.filter_1_input
            .set_buffer(this.input_buffer(Self::FILTER_1_INPUT));
        self.filter_2_input
            .set_buffer(this.input_buffer(Self::FILTER_2_INPUT));
        Self::local(this, filter_1).process(num_samples);
        Self::local(this, filter_2).process(num_samples);

        let output_buffer = this.output_buffer(0);
        let filter_1_buffer = filter_1.output_buffer(0);
        let filter_2_buffer = filter_2.output_buffer(0);
        for i in 0..num_samples {
            output_buffer[i].set(filter_1_buffer[i].get() + filter_2_buffer[i].get());
        }
    }

    /// `processSerialForward(num_samples)`: filter 1 into filter 2.
    fn process_serial_forward(&self, this: &Processor, num_samples: usize) {
        let (filter_1, filter_2) = self.filters();
        self.filter_1_input
            .set_buffer(this.input_buffer(Self::FILTER_1_INPUT));
        self.filter_2_input
            .set_buffer(self.filter_2_input.owned_buffer());
        Self::local(this, filter_1).process(num_samples);

        let filter_2_input_buffer = self.filter_2_input.buffer();
        let filter_1_output_buffer = filter_1.output_buffer(0);
        let filter_2_straight_input = this.input_buffer(Self::FILTER_2_INPUT);
        for i in 0..num_samples {
            filter_2_input_buffer[i]
                .set(filter_1_output_buffer[i].get() + filter_2_straight_input[i].get());
        }

        Self::local(this, filter_2).process(num_samples);
        let output_buffer = this.output_buffer(0);
        let filter_2_output = filter_2.output_buffer(0);
        for i in 0..num_samples {
            output_buffer[i].set(filter_2_output[i].get());
        }
    }

    /// `processSerialBackward(num_samples)`: filter 2 into filter 1.
    fn process_serial_backward(&self, this: &Processor, num_samples: usize) {
        let (filter_1, filter_2) = self.filters();
        self.filter_1_input
            .set_buffer(self.filter_1_input.owned_buffer());
        self.filter_2_input
            .set_buffer(this.input_buffer(Self::FILTER_2_INPUT));
        Self::local(this, filter_2).process(num_samples);

        let filter_1_input_buffer = self.filter_1_input.buffer();
        let filter_2_output_buffer = filter_2.output_buffer(0);
        let filter_1_straight_input = this.input_buffer(Self::FILTER_1_INPUT);
        for i in 0..num_samples {
            filter_1_input_buffer[i]
                .set(filter_2_output_buffer[i].get() + filter_1_straight_input[i].get());
        }

        Self::local(this, filter_1).process(num_samples);
        let output_buffer = this.output_buffer(0);
        let filter_1_output = filter_1.output_buffer(0);
        for i in 0..num_samples {
            output_buffer[i].set(filter_1_output[i].get());
        }
    }
}

impl ProcessorImpl for FiltersModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        self.filter_1_filter_input =
            Some(this.create_base_control("filter_1_filter_input", false, false));
        let filter_1 = FilterModule::create("filter_1");
        this.add_submodule(&filter_1);
        this.add_processor(&filter_1);
        filter_1.plug(&self.filter_1_input, FilterModule::AUDIO);
        filter_1.use_input(&this.input(Self::RESET), FilterModule::RESET);
        filter_1.use_input(&this.input(Self::KEYTRACK), FilterModule::KEYTRACK);
        filter_1.use_input(&this.input(Self::MIDI), FilterModule::MIDI);

        self.filter_2_filter_input =
            Some(this.create_base_control("filter_2_filter_input", false, false));
        let filter_2 = FilterModule::create("filter_2");
        this.add_submodule(&filter_2);
        this.add_processor(&filter_2);
        filter_2.plug(&self.filter_2_input, FilterModule::AUDIO);
        filter_2.use_input(&this.input(Self::RESET), FilterModule::RESET);
        filter_2.use_input(&this.input(Self::KEYTRACK), FilterModule::KEYTRACK);
        filter_2.use_input(&this.input(Self::MIDI), FilterModule::MIDI);

        self.filter_1 = Some(filter_1);
        self.filter_2 = Some(filter_2);
        this.router_init();
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.router_set_oversample_amount(oversample);
        self.filter_1_input
            .ensure_buffer_size(oversample as usize * MAX_BUFFER_SIZE);
        self.filter_2_input
            .ensure_buffer_size(oversample as usize * MAX_BUFFER_SIZE);
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let value = |control: &Option<ProcessorRef>| control.as_ref().expect("init ran").value();
        let filter_1_on = self
            .get_filter_1_on_value()
            .expect("filter 1 creates its on value");
        let filter_2_on = self
            .get_filter_2_on_value()
            .expect("filter 2 creates its on value");
        if value(&self.filter_1_filter_input) != 0.0 && filter_1_on.value() != 0.0 {
            self.process_serial_backward(this, num_samples);
        } else if value(&self.filter_2_filter_input) != 0.0 && filter_2_on.value() != 0.0 {
            self.process_serial_forward(this, num_samples);
        } else {
            self.process_parallel(this, num_samples);
        }
    }
}
