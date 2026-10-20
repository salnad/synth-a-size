// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::ValueSwitch`: a `cr::Value` whose `kSwitch` output points at one
//! of its inputs' buffers, chosen by the value; also enables/disables the
//! processors that only matter for non-zero choices.
//!
//! Ports `vital/src/synthesis/utilities/value_switch.h` and
//! `value_switch.cpp`.

use sas_simd::conversions;
use sas_simd::utils::iclamp;
use sas_simd::PolyFloat;

use crate::processor::{Processor, ProcessorImpl, ProcessorRef};
use crate::value::{ValueCore, ValueKernel, NUM_INPUTS};

/// `vital::ValueSwitch`.
#[derive(Clone)]
pub struct ValueSwitch {
    pub core: ValueCore,
    processors: Vec<ProcessorRef>,
}

impl ValueSwitch {
    /// `kValue` output.
    pub const VALUE: usize = 0;
    /// `kSwitch` output.
    pub const SWITCH: usize = 1;
    pub const NUM_OUTPUTS: usize = 2;

    /// `new ValueSwitch(value)`.
    pub fn create(value: f32) -> ProcessorRef {
        let core = ValueCore::new(PolyFloat::splat(value));
        let processor = Processor::new(
            ValueSwitch {
                core,
                processors: Vec::new(),
            },
            NUM_INPUTS,
            1,
            true,
            1,
        );
        core.fill_output(&processor);
        while processor.num_outputs() < Self::NUM_OUTPUTS {
            processor.add_output(1);
        }
        processor.processor_enable(false);
        processor
    }

    /// `addProcessor(processor)`: enabled only while the choice is non-zero.
    pub fn add_processor(switch: &Processor, processor: &ProcessorRef) {
        switch.with::<ValueSwitch, _>(|kernel, _| kernel.processors.push(processor.clone()));
    }

    /// `setBuffer(source)`.
    fn set_buffer(&self, this: &Processor, source: i32) {
        let source = iclamp(source, 0, this.num_inputs() as i32 - 1) as usize;
        let input = this.input(source);
        let output = this.output(Self::SWITCH);
        output.set_buffer(input.buffer());
        output.set_buffer_size(input.buffer_size());
    }

    /// `setSource(source)`.
    fn set_source(&self, this: &Processor, source: i32) {
        self.set_buffer(this, source);
        let enable_processors = source != 0;
        for processor in &self.processors {
            processor.enable(enable_processors);
        }
    }
}

impl ValueKernel for ValueSwitch {
    fn set(&mut self, this: &Processor, value: PolyFloat) {
        self.core.set(this, value);
        self.set_source(this, conversions::float_to_i32(value.get(0)));
    }

    fn value(&self) -> f32 {
        self.core.value.get(0)
    }
}

impl ProcessorImpl for ValueSwitch {
    crate::processor_boilerplate!();

    fn process(&mut self, _this: &Processor, _num_samples: usize) {}

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        self.core.set_oversample_amount(this, oversample);
        for i in 0..this.num_inputs() {
            if let Some(owner) = this.input(i).source_owner() {
                owner.set_oversample_amount(oversample);
            }
        }
        self.set_buffer(this, conversions::float_to_i32(self.core.value.get(0)));
    }

    fn as_value(&mut self) -> Option<&mut dyn ValueKernel> {
        Some(self)
    }
}
