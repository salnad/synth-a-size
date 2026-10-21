// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Feedback` and `cr::Feedback`: one-block delays that break cycles.
//!
//! Ports `vital/src/synthesis/framework/feedback.h` and `feedback.cpp`.
//! Routers call [`Processor::refresh_output`] on their feedback nodes before
//! running the order, and `process` after it.

use sas_simd::common::MAX_BUFFER_SIZE;
use sas_simd::{PolyFloat, PolyMask};

use crate::processor::{Processor, ProcessorImpl, ProcessorRef};

/// `vital::Feedback` (audio rate) and `cr::Feedback` (control rate).
#[derive(Clone)]
pub struct Feedback {
    control_rate: bool,
    buffer: Box<[PolyFloat; MAX_BUFFER_SIZE]>,
    buffer_index: usize,
    /// `cr::Feedback::last_value_`.
    last_value: PolyFloat,
}

impl Feedback {
    fn kernel(control_rate: bool) -> Feedback {
        Feedback {
            control_rate,
            buffer: Box::new([PolyFloat::zero(); MAX_BUFFER_SIZE]),
            buffer_index: 0,
            last_value: PolyFloat::zero(),
        }
    }

    /// `new Feedback(control_rate)`. With `control_rate`, this is the base
    /// class behaviour on a control-rate node, not `cr::Feedback`.
    pub fn create(control_rate: bool) -> ProcessorRef {
        let mut kernel = Self::kernel(false);
        kernel.control_rate = false;
        Processor::new(kernel, 1, 1, control_rate, 1)
    }

    /// `new cr::Feedback()`.
    pub fn create_control_rate() -> ProcessorRef {
        Processor::new(Self::kernel(true), 1, 1, true, 1)
    }

    /// `tick(i)`: copies sample `i` of the input into the delay buffer.
    pub fn tick(&mut self, this: &Processor, i: usize) {
        self.buffer[i] = this.input(0).at(i);
    }
}

impl ProcessorImpl for Feedback {
    crate::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        if self.control_rate {
            self.last_value = this.input(0).at(0);
            return;
        }

        debug_assert!(this.input_matches_buffer_size(0));
        let audio_in = this.input_buffer(0);
        for sample in audio_in.iter().take(num_samples) {
            self.buffer[self.buffer_index] = sample.get();
            self.buffer_index = (self.buffer_index + 1) % MAX_BUFFER_SIZE;
        }
    }

    fn refresh_output(&mut self, this: &Processor, num_samples: usize) {
        if self.control_rate {
            this.output(0).set_at(0, self.last_value);
            return;
        }

        let audio_out = this.output_buffer(0);
        let mut index = (MAX_BUFFER_SIZE + self.buffer_index - num_samples) % MAX_BUFFER_SIZE;
        for sample in audio_out.iter().take(num_samples) {
            sample.set(self.buffer[index]);
            index = (index + 1) % MAX_BUFFER_SIZE;
        }
    }

    fn reset(&mut self, this: &Processor, _reset_mask: PolyMask) {
        if self.control_rate {
            self.last_value = PolyFloat::zero();
            this.output(0).set_at(0, self.last_value);
        }
    }
}
