// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::DcFilter`: a one-pole DC blocker.
//!
//! Ports `vital/src/synthesis/filters/dc_filter.h` and `dc_filter.cpp`.

use sas_dsp_core::{Buffer, Processor, ProcessorImpl, ProcessorRef};
use sas_simd::poly_utils as utils;
use sas_simd::{PolyFloat, PolyMask};

/// Input index `kAudio`.
pub const AUDIO: usize = 0;
/// Input index `kReset`.
pub const RESET: usize = 1;
/// `kNumInputs`.
pub const NUM_INPUTS: usize = 2;

/// `vital::DcFilter`.
#[derive(Clone, Debug)]
pub struct DcFilter {
    coefficient: f32,
    past_in: PolyFloat,
    past_out: PolyFloat,
}

impl Default for DcFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl DcFilter {
    /// `kCoefficientToSrConstant`.
    pub const COEFFICIENT_TO_SR_CONSTANT: f32 = 1.0;

    /// The kernel of `DcFilter()`.
    pub fn new() -> Self {
        let mut filter = DcFilter {
            coefficient: 0.0,
            past_in: PolyFloat::splat(0.0),
            past_out: PolyFloat::splat(0.0),
        };
        filter.reset_state(PolyMask::full());
        filter
    }

    /// `new DcFilter()`: `Processor(kNumInputs, 1)`.
    pub fn create() -> ProcessorRef {
        Processor::new(Self::new(), NUM_INPUTS, 1, false, 1)
    }

    /// `tick(audio_in, audio_out)`.
    #[inline(always)]
    pub fn tick(&mut self, audio_in: PolyFloat) -> PolyFloat {
        let audio_out = utils::mul_add(audio_in - self.past_in, self.past_out, self.coefficient);
        self.past_out = audio_out;
        self.past_in = audio_in;
        audio_out
    }

    /// `reset(reset_mask)`. Upstream loads `past_out_` from `past_in_`
    /// (already reset), so lanes outside the mask take `past_in_`.
    pub fn reset_state(&mut self, reset_mask: PolyMask) {
        self.past_in = utils::mask_load(self.past_in, 0.0, reset_mask);
        self.past_out = utils::mask_load(self.past_in, 0.0, reset_mask);
    }

    /// `processWithInput(audio_in, num_samples)`.
    pub fn process_with_input_impl(
        &mut self,
        this: &Processor,
        audio_in: &Buffer,
        num_samples: usize,
    ) {
        let reset_mask = this.get_reset_mask(RESET);
        if reset_mask.any_mask() != 0 {
            self.reset_state(reset_mask);
        }

        let audio_out = this.output_buffer(0);
        for i in 0..num_samples {
            audio_out[i].set(self.tick(audio_in[i].get()));
        }
    }
}

impl ProcessorImpl for DcFilter {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio_in = this.input_buffer(AUDIO);
        self.process_with_input_impl(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        self.process_with_input_impl(this, audio_in, num_samples);
    }

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.processor_set_sample_rate(sample_rate);
        self.coefficient = 1.0 - Self::COEFFICIENT_TO_SR_CONSTANT / this.sample_rate() as f32;
    }

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.reset_state(reset_mask);
    }
}
