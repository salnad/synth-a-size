// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Value` and `cr::Value`: constant sources that a trigger on their
//! `kSet` input can change per voice.
//!
//! Ports `vital/src/synthesis/framework/value.h` and `value.cpp`.

use sas_simd::poly_utils as utils;
use sas_simd::PolyFloat;

use crate::processor::{Processor, ProcessorImpl, ProcessorRef};

/// The `Value` interface (`set`, `value`) shared by `Value`, `cr::Value`,
/// `SmoothValue`, `cr::SmoothValue` and `ValueSwitch`. Reach it through
/// [`Processor::set_value`] and [`Processor::value`].
pub trait ValueKernel {
    /// `set(value)` (virtual).
    fn set(&mut self, this: &Processor, value: PolyFloat);
    /// `value()`: lane 0 of the stored value.
    fn value(&self) -> f32;
}

/// `Value::kSet`.
pub const SET: usize = 0;
/// `Value::kNumInputs`.
pub const NUM_INPUTS: usize = 1;

/// Shared data and behaviour of the `Value` family (`value_` and the base
/// methods).
#[derive(Clone, Copy, Debug)]
pub struct ValueCore {
    pub value: PolyFloat,
}

impl ValueCore {
    pub fn new(value: PolyFloat) -> ValueCore {
        ValueCore { value }
    }

    /// Fills the whole output buffer with the value (constructor and `set`).
    pub fn fill_output(&self, this: &Processor) {
        let output = this.output(0);
        let buffer = output.buffer();
        for sample in buffer.iter().take(output.buffer_size()) {
            sample.set(self.value);
        }
    }

    /// `Value::set`.
    pub fn set(&mut self, this: &Processor, value: PolyFloat) {
        self.value = value;
        self.fill_output(this);
    }

    /// Applies a trigger on `kSet` to the stored value.
    pub fn apply_set_trigger(&mut self, this: &Processor) {
        let input = this.input(SET);
        let trigger_mask = input.trigger_mask();
        if trigger_mask.any_mask() != 0 {
            let trigger_value = input.trigger_value();
            self.value = utils::mask_load(self.value, trigger_value, trigger_mask);
        }
    }

    /// `Value::process`.
    pub fn process_audio(&mut self, this: &Processor, num_samples: usize) {
        self.apply_set_trigger(this);
        let dest = this.output_buffer(0);
        for sample in dest.iter().take(num_samples) {
            sample.set(self.value);
        }
    }

    /// `cr::Value::process`.
    pub fn process_control(&mut self, this: &Processor) {
        self.apply_set_trigger(this);
        this.output(0).set_at(0, self.value);
    }

    /// `Value::setOversampleAmount`.
    pub fn set_oversample_amount(&self, this: &Processor, oversample: i32) {
        this.processor_set_oversample_amount(oversample);
        self.fill_output(this);
    }
}

/// `vital::Value` (audio rate) and `cr::Value` (control rate).
#[derive(Clone, Debug)]
pub struct Value {
    pub core: ValueCore,
    control_rate: bool,
}

impl Value {
    /// `new Value(value, control_rate)`.
    pub fn create(value: impl Into<PolyFloat>, control_rate: bool) -> ProcessorRef {
        let core = ValueCore::new(value.into());
        let processor = Processor::new(
            Value {
                core,
                control_rate: false,
            },
            NUM_INPUTS,
            1,
            control_rate,
            1,
        );
        core.fill_output(&processor);
        processor
    }

    /// `new cr::Value(value)`.
    pub fn create_control_rate(value: impl Into<PolyFloat>) -> ProcessorRef {
        let core = ValueCore::new(value.into());
        let processor = Processor::new(
            Value {
                core,
                control_rate: true,
            },
            NUM_INPUTS,
            1,
            true,
            1,
        );
        core.fill_output(&processor);
        processor
    }
}

impl ValueKernel for Value {
    fn set(&mut self, this: &Processor, value: PolyFloat) {
        self.core.set(this, value);
    }

    fn value(&self) -> f32 {
        self.core.value.get(0)
    }
}

impl ProcessorImpl for Value {
    crate::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        if self.control_rate {
            self.core.process_control(this);
        } else {
            self.core.process_audio(this, num_samples);
        }
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        self.core.set_oversample_amount(this, oversample);
    }

    fn as_value(&mut self) -> Option<&mut dyn ValueKernel> {
        Some(self)
    }
}
