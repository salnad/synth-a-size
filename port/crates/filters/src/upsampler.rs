// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Upsampler`: repeats each input sample `oversample` times.
//!
//! Ports `vital/src/synthesis/filters/upsampler.h` and `upsampler.cpp`.
//! Upstream derives it from `ProcessorRouter` (with no children); the
//! effects engine holds it as an idle processor and calls
//! `processWithInput` with the base-rate sample count.

use sas_dsp_core::{Buffer, Processor, ProcessorImpl, ProcessorRef};

/// Input index `kAudio`.
pub const AUDIO: usize = 0;
/// `kNumInputs`.
pub const NUM_INPUTS: usize = 1;

/// `vital::Upsampler`.
#[derive(Debug, Default)]
pub struct Upsampler;

impl Upsampler {
    /// `new Upsampler()`: `ProcessorRouter(kNumInputs, 1)`.
    pub fn create() -> ProcessorRef {
        Processor::new_router(Upsampler, NUM_INPUTS, 1, false)
    }

    /// `processWithInput(audio_in, num_samples)`: `num_samples` input
    /// samples, `num_samples * oversample` output samples.
    pub fn process_with_input_impl(this: &Processor, audio_in: &Buffer, num_samples: usize) {
        let destination = this.output_buffer(0);
        let oversample_amount = this.oversample_amount() as usize;
        for i in 0..num_samples {
            let offset = i * oversample_amount;
            let value = audio_in[i].get();
            for s in 0..oversample_amount {
                destination[offset + s].set(value);
            }
        }
    }
}

impl ProcessorImpl for Upsampler {
    /// Upstream's `clone()` is `VITAL_ASSERT(false); return nullptr;`: this
    /// processor only lives in the mono engine path and is never cloned.
    fn clone_box(&self) -> Box<dyn ProcessorImpl> {
        panic!("Upsampler cannot be cloned (upstream asserts in clone())")
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio_in = this.input_buffer(AUDIO);
        Self::process_with_input_impl(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        Self::process_with_input_impl(this, audio_in, num_samples);
    }
}
