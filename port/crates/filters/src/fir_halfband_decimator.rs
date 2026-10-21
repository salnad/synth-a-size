// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::FirHalfbandDecimator`: a 32-tap FIR halfband filter that halves
//! the sample rate. Upstream builds it but no engine path uses it.
//!
//! Ports `vital/src/synthesis/filters/fir_halfband_decimator.h` and
//! `fir_halfband_decimator.cpp`.

use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_simd::poly_utils as utils;
use sas_simd::{PolyFloat, PolyMask};

/// Input index `kAudio`.
pub const AUDIO: usize = 0;
/// `kNumInputs`.
pub const NUM_INPUTS: usize = 1;
/// `kNumTaps`.
pub const NUM_TAPS: usize = 32;

#[allow(clippy::excessive_precision)] // upstream's literals, verbatim
const COEFFICIENTS: [f32; NUM_TAPS] = [
    0.000088228877315364,
    0.000487010018128278,
    0.000852264975437944,
    -0.001283563593466774,
    -0.010130591831925894,
    -0.025688727779244691,
    -0.036346596505004387,
    -0.024088355516718698,
    0.012246773417129486,
    0.040021434054637831,
    0.017771298164062477,
    -0.046866403416502632,
    -0.075597513455990611,
    0.013331126342402619,
    0.202889888191404910,
    0.362615173769444080,
    0.362615173769444080,
    0.202889888191404910,
    0.013331126342402619,
    -0.075597513455990611,
    -0.046866403416502632,
    0.017771298164062477,
    0.040021434054637831,
    0.012246773417129486,
    -0.024088355516718698,
    -0.036346596505004387,
    -0.025688727779244691,
    -0.010130591831925894,
    -0.001283563593466774,
    0.000852264975437944,
    0.000487010018128278,
    0.000088228877315364,
];

/// `vital::FirHalfbandDecimator`.
#[derive(Clone, Debug)]
pub struct FirHalfbandDecimator {
    memory: [PolyFloat; NUM_TAPS / 2 - 1],
    taps: [PolyFloat; NUM_TAPS / 2],
}

impl Default for FirHalfbandDecimator {
    fn default() -> Self {
        Self::new()
    }
}

impl FirHalfbandDecimator {
    /// The kernel of `FirHalfbandDecimator()`.
    pub fn new() -> Self {
        let mut taps = [PolyFloat::splat(0.0); NUM_TAPS / 2];
        for (i, tap) in taps.iter_mut().enumerate() {
            *tap = PolyFloat::stereo(COEFFICIENTS[2 * i], COEFFICIENTS[2 * i + 1]);
        }
        let mut decimator = FirHalfbandDecimator {
            memory: [PolyFloat::splat(0.0); NUM_TAPS / 2 - 1],
            taps,
        };
        decimator.reset_state();
        decimator
    }

    /// `new FirHalfbandDecimator()`: `Processor(kNumInputs, 1)`.
    pub fn create() -> ProcessorRef {
        Processor::new(Self::new(), NUM_INPUTS, 1, false, 1)
    }

    /// `reset(reset_mask)`: clears every lane regardless of the mask.
    pub fn reset_state(&mut self) {
        for value in &mut self.memory {
            *value = PolyFloat::splat(0.0);
        }
    }

    /// `saveMemory(num_samples)`.
    pub fn save_memory(&mut self, this: &Processor, num_samples: usize) {
        let input_buffer_size = 2 * num_samples;
        let audio = this.input_buffer(AUDIO);
        let start_audio_index = input_buffer_size + 2 - NUM_TAPS;
        for i in 0..NUM_TAPS / 2 - 1 {
            let audio_index = start_audio_index + 2 * i;
            self.memory[i] =
                utils::consolidate_audio(audio[audio_index].get(), audio[audio_index + 1].get());
        }
    }
}

impl ProcessorImpl for FirHalfbandDecimator {
    sas_dsp_core::processor_boilerplate!();

    /// Writes `num_samples` outputs from `2 * num_samples` inputs
    /// (`num_samples > kNumTaps / 2`).
    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio = this.input_buffer(AUDIO);
        let output_buffer_size = num_samples;
        let audio_out = this.output_buffer(0);

        for memory_start in 0..NUM_TAPS / 2 - 1 {
            let mut sum = PolyFloat::splat(0.0);
            let mut tap_index = 0;
            let num_memory = NUM_TAPS / 2 - memory_start - 1;
            while tap_index < num_memory {
                sum = utils::mul_add(
                    sum,
                    self.memory[tap_index + memory_start],
                    self.taps[tap_index],
                );
                tap_index += 1;
            }
            let mut audio_index = 0;
            while tap_index < NUM_TAPS / 2 {
                let consolidated = utils::consolidate_audio(
                    audio[audio_index].get(),
                    audio[audio_index + 1].get(),
                );
                sum = utils::mul_add(sum, consolidated, self.taps[tap_index]);
                audio_index += 2;
                tap_index += 1;
            }
            audio_out[memory_start].set(utils::sum_split_audio(sum));
        }

        let mut audio_start = 0;
        for out_index in NUM_TAPS / 2 - 1..output_buffer_size {
            let mut sum = PolyFloat::splat(0.0);
            let mut audio_index = audio_start;
            for tap in &self.taps {
                let consolidated = utils::consolidate_audio(
                    audio[audio_index].get(),
                    audio[audio_index + 1].get(),
                );
                sum = utils::mul_add(sum, consolidated, *tap);
                audio_index += 2;
            }
            audio_start += 2;
            audio_out[out_index].set(utils::sum_split_audio(sum));
        }

        self.save_memory(this, num_samples);
    }

    fn reset(&mut self, _this: &Processor, _reset_mask: PolyMask) {
        self.reset_state();
    }
}
