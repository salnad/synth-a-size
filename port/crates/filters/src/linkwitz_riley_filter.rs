// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::LinkwitzRileyFilter`: a fixed-cutoff 4th-order Linkwitz-Riley
//! crossover (two cascaded Butterworth biquads per band), used by the
//! multiband compressor.
//!
//! Ports `vital/src/synthesis/filters/linkwitz_riley_filter.h` and
//! `linkwitz_riley_filter.cpp`.

use sas_dsp_core::{Buffer, Processor, ProcessorImpl, ProcessorRef};
use sas_simd::common::{PI, SQRT2};
use sas_simd::poly_utils as utils;
use sas_simd::{PolyFloat, PolyMask};

/// Input index `kAudio`.
pub const AUDIO: usize = 0;
/// `kNumInputs`.
pub const NUM_INPUTS: usize = 1;
/// Output index `kAudioLow`.
pub const AUDIO_LOW: usize = 0;
/// Output index `kAudioHigh`.
pub const AUDIO_HIGH: usize = 1;
/// `kNumOutputs`.
pub const NUM_OUTPUTS: usize = 2;

/// Biquad coefficients of one band.
#[derive(Clone, Copy, Debug, Default)]
struct Coefficients {
    in_0: f32,
    in_1: f32,
    in_2: f32,
    out_1: f32,
    out_2: f32,
}

/// `vital::LinkwitzRileyFilter`.
#[derive(Clone, Debug)]
pub struct LinkwitzRileyFilter {
    cutoff: f32,
    low: Coefficients,
    high: Coefficients,
    past_in_1a: [PolyFloat; NUM_OUTPUTS],
    past_in_2a: [PolyFloat; NUM_OUTPUTS],
    past_out_1a: [PolyFloat; NUM_OUTPUTS],
    past_out_2a: [PolyFloat; NUM_OUTPUTS],
    past_in_1b: [PolyFloat; NUM_OUTPUTS],
    past_in_2b: [PolyFloat; NUM_OUTPUTS],
    past_out_1b: [PolyFloat; NUM_OUTPUTS],
    past_out_2b: [PolyFloat; NUM_OUTPUTS],
}

impl LinkwitzRileyFilter {
    /// The kernel of `LinkwitzRileyFilter(cutoff)`. Coefficients stay zero
    /// until the sample rate is set, as upstream.
    pub fn new(cutoff: f32) -> Self {
        let zero = [PolyFloat::splat(0.0); NUM_OUTPUTS];
        let mut filter = LinkwitzRileyFilter {
            cutoff,
            low: Coefficients::default(),
            high: Coefficients::default(),
            past_in_1a: zero,
            past_in_2a: zero,
            past_out_1a: zero,
            past_out_2a: zero,
            past_in_1b: zero,
            past_in_2b: zero,
            past_out_1b: zero,
            past_out_2b: zero,
        };
        filter.reset_state(PolyMask::full());
        filter
    }

    /// `new LinkwitzRileyFilter(cutoff)`: `Processor(kNumInputs, kNumOutputs)`.
    pub fn create(cutoff: f32) -> ProcessorRef {
        Processor::new(Self::new(cutoff), NUM_INPUTS, NUM_OUTPUTS, false, 1)
    }

    /// `computeCoefficients()`.
    pub fn compute_coefficients(&mut self, this: &Processor) {
        let warp = 1.0f32 / (PI * self.cutoff / this.sample_rate() as f32).tan();
        let warp2 = warp * warp;
        let mult = 1.0f32 / (1.0 + SQRT2 * warp + warp2);

        self.low.in_0 = mult;
        self.low.in_1 = 2.0 * mult;
        self.low.in_2 = mult;
        self.low.out_1 = -2.0 * (1.0 - warp2) * mult;
        self.low.out_2 = -(1.0 - SQRT2 * warp + warp2) * mult;

        self.high.in_0 = warp2 * mult;
        self.high.in_1 = -2.0 * self.high.in_0;
        self.high.in_2 = self.high.in_0;
        self.high.out_1 = self.low.out_1;
        self.high.out_2 = self.low.out_2;
    }

    /// `reset(reset_mask)`.
    pub fn reset_state(&mut self, reset_mask: PolyMask) {
        for i in 0..NUM_OUTPUTS {
            for state in [
                &mut self.past_in_1a,
                &mut self.past_in_2a,
                &mut self.past_out_1a,
                &mut self.past_out_2a,
                &mut self.past_in_1b,
                &mut self.past_in_2b,
                &mut self.past_out_1b,
                &mut self.past_out_2b,
            ] {
                state[i] = utils::mask_load(state[i], 0.0, reset_mask);
            }
        }
    }

    /// `processWithInput(audio_in, num_samples)`.
    pub fn process_with_input_impl(
        &mut self,
        this: &Processor,
        audio_in: &Buffer,
        num_samples: usize,
    ) {
        let dest_low = this.output_buffer(AUDIO_LOW);
        let low = self.low;
        self.run_stage(AUDIO_LOW, &low, Stage::A, audio_in, &dest_low, num_samples);
        self.run_stage(AUDIO_LOW, &low, Stage::B, &dest_low, &dest_low, num_samples);

        let dest_high = this.output_buffer(AUDIO_HIGH);
        let high = self.high;
        self.run_stage(
            AUDIO_HIGH,
            &high,
            Stage::A,
            audio_in,
            &dest_high,
            num_samples,
        );
        self.run_stage(
            AUDIO_HIGH,
            &high,
            Stage::B,
            &dest_high,
            &dest_high,
            num_samples,
        );
    }

    /// One of the four loops of `processWithInput`.
    fn run_stage(
        &mut self,
        band: usize,
        c: &Coefficients,
        stage: Stage,
        source: &Buffer,
        dest: &Buffer,
        num_samples: usize,
    ) {
        let (past_in_1, past_in_2, past_out_1, past_out_2) = match stage {
            Stage::A => (
                &mut self.past_in_1a[band],
                &mut self.past_in_2a[band],
                &mut self.past_out_1a[band],
                &mut self.past_out_2a[band],
            ),
            Stage::B => (
                &mut self.past_in_1b[band],
                &mut self.past_in_2b[band],
                &mut self.past_out_1b[band],
                &mut self.past_out_2b[band],
            ),
        };
        for i in 0..num_samples {
            let audio = source[i].get();
            let in01 = utils::mul_add(audio * c.in_0, *past_in_1, c.in_1);
            let input = utils::mul_add(in01, *past_in_2, c.in_2);
            let in_out1 = utils::mul_add(input, *past_out_1, c.out_1);
            let out = utils::mul_add(in_out1, *past_out_2, c.out_2);
            *past_in_2 = *past_in_1;
            *past_in_1 = audio;
            *past_out_2 = *past_out_1;
            *past_out_1 = out;
            dest[i].set(out);
        }
    }
}

#[derive(Clone, Copy)]
enum Stage {
    A,
    B,
}

impl ProcessorImpl for LinkwitzRileyFilter {
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
        self.compute_coefficients(this);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.processor_set_oversample_amount(oversample);
        self.compute_coefficients(this);
    }

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.reset_state(reset_mask);
    }
}
