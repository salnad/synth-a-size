// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::PeakMeter`: level (RMS of voice 1 in voice 0's lanes, peak in the
//! others) and a held peak, for the UI.
//!
//! Ports `vital/src/synthesis/utilities/peak_meter.h` and `peak_meter.cpp`.

use sas_simd::poly_utils as utils;
use sas_simd::{conversions, PolyFloat, PolyInt};

use crate::constants;
use crate::processor::{Processor, ProcessorImpl, ProcessorRef};

const SAMPLE_DECAY: f32 = 8096.0;
const REMEMBERED_DECAY: f32 = 20000.0;
const REMEMBERED_HOLD: f32 = 50000.0;

/// `vital::PeakMeter`.
#[derive(Clone)]
pub struct PeakMeter {
    current_peak: PolyFloat,
    current_square_sum: PolyFloat,
    remembered_peak: PolyFloat,
    samples_since_remembered: PolyInt,
}

impl PeakMeter {
    pub const MAX_REMEMBERED_PEAKS: usize = 16;

    pub const LEVEL: usize = 0;
    pub const MEMORY_PEAK: usize = 1;
    pub const NUM_OUTPUTS: usize = 2;

    pub fn create() -> ProcessorRef {
        Processor::new(
            PeakMeter {
                current_peak: PolyFloat::zero(),
                current_square_sum: PolyFloat::zero(),
                remembered_peak: PolyFloat::zero(),
                samples_since_remembered: PolyInt::zero(),
            },
            1,
            Self::NUM_OUTPUTS,
            false,
            1,
        )
    }
}

impl ProcessorImpl for PeakMeter {
    crate::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio_in = this.input_buffer(0);
        // `utils::peak(audio_in, num_samples)`.
        let mut peak = PolyFloat::zero();
        for sample in audio_in.iter().take(num_samples) {
            peak = PolyFloat::max(peak, sample.get());
            peak = PolyFloat::max(peak, -sample.get());
        }

        let oversample = this.oversample_amount() as f32;
        let samples = oversample * SAMPLE_DECAY;
        let mult = (samples - 1.0) / samples;
        let mut current_peak = self.current_peak;
        let remembered_samples = oversample * REMEMBERED_DECAY;
        let remembered_mult = (remembered_samples - 1.0) / remembered_samples;
        let mut current_remembered_peak = self.remembered_peak;
        let mut current_square_sum = self.current_square_sum;

        for sample in audio_in.iter().take(num_samples) {
            let sample = sample.get();
            current_peak *= mult;
            current_remembered_peak *= remembered_mult;
            current_square_sum *= mult;
            current_square_sum += sample * sample;
        }

        self.current_peak = utils::max(current_peak, peak);
        self.samples_since_remembered += num_samples as u32;
        self.samples_since_remembered &=
            PolyFloat::less_than(self.current_peak, current_remembered_peak);

        let remembered_hold_samples = oversample * REMEMBERED_HOLD;
        let hold_mask = PolyInt::less_than(
            self.samples_since_remembered,
            PolyInt::splat(conversions::float_to_u32(remembered_hold_samples)),
        );
        current_remembered_peak =
            utils::mask_load(current_remembered_peak, self.remembered_peak, hold_mask);
        self.remembered_peak = utils::max(self.current_peak, current_remembered_peak);
        self.current_square_sum = current_square_sum;

        let rms = utils::sqrt(self.current_square_sum * (1.0 / samples));
        let prepped_rms = utils::swap_voices(rms);
        this.output(Self::LEVEL).set_at(
            0,
            utils::mask_load(prepped_rms, self.current_peak, constants::first_mask()),
        );
        this.output(Self::MEMORY_PEAK)
            .set_at(0, self.remembered_peak);
    }
}
