// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::IirHalfbandDecimator`: a polyphase allpass halfband filter that
//! halves the sample rate (9 or 25 taps).
//!
//! Ports `vital/src/synthesis/filters/iir_halfband_decimator.h` and
//! `iir_halfband_decimator.cpp`.

use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_simd::poly_utils as utils;
use sas_simd::{PolyFloat, PolyMask};

/// Input index `kAudio`.
pub const AUDIO: usize = 0;
/// `kNumInputs`.
pub const NUM_INPUTS: usize = 1;

/// `kNumTaps9`.
pub const NUM_TAPS_9: usize = 2;
/// `kNumTaps25`.
pub const NUM_TAPS_25: usize = 6;

/// `kTaps9`: `poly_float(even, odd)` pairs.
#[allow(clippy::excessive_precision)] // upstream's literals, verbatim
pub const TAPS_9: [[f32; 2]; NUM_TAPS_9] = [
    [0.167135116548925, 0.0413554705262319],
    [0.742130012538075, 0.3878932830211427],
];

/// `kTaps25`: `poly_float(even, odd)` pairs.
#[allow(clippy::excessive_precision)] // upstream's literals, verbatim
pub const TAPS_25: [[f32; 2]; NUM_TAPS_25] = [
    [0.093022421467960, 0.024388383731296],
    [0.312318050871736, 0.194029987625265],
    [0.548379093159427, 0.433855675727187],
    [0.737198546150414, 0.650124972769370],
    [0.872234992057129, 0.810418671775866],
    [0.975497791832324, 0.925979700943193],
];

/// `vital::IirHalfbandDecimator`.
#[derive(Debug)]
pub struct IirHalfbandDecimator {
    sharp_cutoff: bool,
    in_memory: [PolyFloat; NUM_TAPS_25],
    out_memory: [PolyFloat; NUM_TAPS_25],
}

impl Default for IirHalfbandDecimator {
    fn default() -> Self {
        Self::new()
    }
}

impl IirHalfbandDecimator {
    /// The kernel of `IirHalfbandDecimator()`.
    pub fn new() -> Self {
        let mut decimator = IirHalfbandDecimator {
            sharp_cutoff: false,
            in_memory: [PolyFloat::splat(0.0); NUM_TAPS_25],
            out_memory: [PolyFloat::splat(0.0); NUM_TAPS_25],
        };
        decimator.reset_state();
        decimator
    }

    /// `new IirHalfbandDecimator()`: `Processor(kNumInputs, 1)`.
    pub fn create() -> ProcessorRef {
        Processor::new(Self::new(), NUM_INPUTS, 1, false, 1)
    }

    pub fn set_sharp_cutoff(&mut self, sharp_cutoff: bool) {
        self.sharp_cutoff = sharp_cutoff;
    }

    /// `reset(reset_mask)`: clears every lane regardless of the mask.
    pub fn reset_state(&mut self) {
        for i in 0..NUM_TAPS_25 {
            self.in_memory[i] = PolyFloat::splat(0.0);
            self.out_memory[i] = PolyFloat::splat(0.0);
        }
    }
}

impl ProcessorImpl for IirHalfbandDecimator {
    /// Upstream's `clone()` is `VITAL_ASSERT(false); return nullptr;`: this
    /// processor only lives in the mono engine path and is never cloned.
    fn clone_box(&self) -> Box<dyn ProcessorImpl> {
        panic!("IirHalfbandDecimator cannot be cloned (upstream asserts in clone())")
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// Writes `num_samples` outputs from `2 * num_samples` inputs.
    fn process(&mut self, this: &Processor, num_samples: usize) {
        let taps: &[[f32; 2]] = if self.sharp_cutoff { &TAPS_25 } else { &TAPS_9 };

        let audio = this.input_buffer(AUDIO);
        let audio_out = this.output_buffer(0);
        for i in 0..num_samples {
            let audio_in_index = 2 * i;
            let mut result = utils::consolidate_audio(
                audio[audio_in_index].get(),
                audio[audio_in_index + 1].get(),
            );
            for (tap_index, tap) in taps.iter().enumerate() {
                let tap = PolyFloat::stereo(tap[0], tap[1]);
                let delta = result - self.out_memory[tap_index];
                let new_result = utils::mul_add(self.in_memory[tap_index], tap, delta);
                self.in_memory[tap_index] = result;
                self.out_memory[tap_index] = new_result;
                result = new_result;
            }
            audio_out[i].set(utils::sum_split_audio(result) * 0.5);
        }
    }

    fn reset(&mut self, _this: &Processor, _reset_mask: PolyMask) {
        self.reset_state();
    }
}
