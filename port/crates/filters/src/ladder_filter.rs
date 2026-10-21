// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::LadderFilter`: a four-stage saturating ladder (model "ladder").
//!
//! Ports `vital/src/synthesis/filters/ladder_filter.h` and
//! `ladder_filter.cpp`.

use sas_dsp_core::{OnePoleFilter, Processor, ProcessorImpl, ProcessorRef};
use sas_simd::common::PI;
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyMask};

use crate::synth_filter::{coefficient_lookup, input, style, FilterState, SynthFilter};

/// `kNumStages`.
pub const NUM_STAGES: usize = 4;

const LOW_PASS_24: [f32; NUM_STAGES + 1] = [0.0, 0.0, 0.0, 0.0, 1.0];
const BAND_PASS_24: [f32; NUM_STAGES + 1] = [0.0, 0.0, -1.0, 2.0, -1.0];
const HIGH_PASS_24: [f32; NUM_STAGES + 1] = [1.0, -4.0, 6.0, -4.0, 1.0];
const LOW_PASS_12: [f32; NUM_STAGES + 1] = [0.0, 0.0, 1.0, 0.0, 0.0];
const BAND_PASS_12: [f32; NUM_STAGES + 1] = [0.0, 1.0, -1.0, 0.0, 0.0];
const HIGH_PASS_12: [f32; NUM_STAGES + 1] = [1.0, -2.0, 1.0, 0.0, 0.0];

/// `vital::LadderFilter`.
#[derive(Clone, Debug)]
pub struct LadderFilter {
    filter_state: FilterState,
    resonance: PolyFloat,
    drive: PolyFloat,
    post_multiply: PolyFloat,
    /// Zero until the first `setupFilter` with a style it handles (0 to 4),
    /// as upstream (`poly_float()` zero-initializes).
    stage_scales: [PolyFloat; NUM_STAGES + 1],
    stages: [OnePoleFilter; NUM_STAGES],
    filter_input: PolyFloat,
}

impl Default for LadderFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl LadderFilter {
    pub const RESONANCE_TUNING: f32 = 1.66;
    pub const MIN_RESONANCE: f32 = 0.001;
    pub const MAX_RESONANCE: f32 = 4.1;
    pub const MAX_COEFFICIENT: f32 = 0.35;
    pub const DRIVE_RESONANCE_BOOST: f32 = 5.0;
    pub const MIN_CUTOFF: f32 = 1.0;
    pub const MAX_CUTOFF: f32 = 20000.0;

    /// The kernel of `LadderFilter()`.
    pub fn new() -> Self {
        let zero = PolyFloat::splat(0.0);
        let mut filter = LadderFilter {
            filter_state: FilterState::default(),
            resonance: zero,
            drive: zero,
            post_multiply: zero,
            stage_scales: [zero; NUM_STAGES + 1],
            stages: [OnePoleFilter::with_saturate(futils::algebraic_sat); NUM_STAGES],
            filter_input: zero,
        };
        filter.hard_reset_state();
        filter
    }

    /// `new LadderFilter()`: `Processor(kNumInputs, 1)`.
    pub fn create() -> ProcessorRef {
        Processor::new(Self::new(), input::NUM_INPUTS, 1, false, 1)
    }

    pub fn get_drive(&self) -> PolyFloat {
        self.drive
    }

    pub fn get_resonance(&self) -> PolyFloat {
        self.resonance
    }

    pub fn get_stage_scale(&self, index: usize) -> PolyFloat {
        self.stage_scales[index]
    }

    /// `reset(reset_mask)`.
    pub fn reset_state(&mut self, reset_mask: PolyMask) {
        self.filter_input = utils::mask_load(self.filter_input, 0.0, reset_mask);
        for stage in &mut self.stages {
            stage.reset(reset_mask);
        }
    }

    /// `hardReset()`.
    pub fn hard_reset_state(&mut self) {
        self.reset_state(PolyMask::full());
        self.resonance = PolyFloat::splat(0.0);
        self.drive = PolyFloat::splat(0.0);
        self.post_multiply = PolyFloat::splat(0.0);
    }

    /// `tick(audio_in, coefficient, resonance, drive)`.
    #[inline(always)]
    pub fn tick(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        drive: PolyFloat,
    ) {
        let g1 = coefficient * Self::RESONANCE_TUNING;
        let g2 = g1 * g1;
        let g3 = g1 * g2;

        let filter_state1 = utils::mul_add(
            self.stages[3].next_sat_state(),
            g1,
            self.stages[2].next_sat_state(),
        );
        let filter_state2 = utils::mul_add(filter_state1, g2, self.stages[1].next_sat_state());
        let filter_state = utils::mul_add(filter_state2, g3, self.stages[0].next_sat_state());

        let filter_input = audio_in * drive - resonance * filter_state;
        self.filter_input = futils::tanh(filter_input);
        let mut stage_out = self.stages[0].tick(self.filter_input, coefficient);
        stage_out = self.stages[1].tick(stage_out, coefficient);
        stage_out = self.stages[2].tick(stage_out, coefficient);
        self.stages[3].tick(stage_out, coefficient);
    }

    /// `setStageScales(filter_state)`.
    fn set_stage_scales(&mut self, filter_state: &FilterState) {
        let one = PolyFloat::splat(1.0);
        let blend = utils::clamp(filter_state.pass_blend - 1.0, -1.0, 1.0);
        let mut band_pass = utils::sqrt(-blend * blend + 1.0);
        let blend_mask = PolyFloat::less_than(blend, PolyFloat::splat(0.0));
        let low_pass = (-blend) & blend_mask;
        let high_pass = blend & !blend_mask;

        let resonance_percent = utils::clamp(filter_state.resonance_percent, 0.0, 1.0);
        let mut drive_mult = resonance_percent + 1.0;
        if filter_state.style != 0 {
            drive_mult = utils::sin(resonance_percent) + 1.0;
        }

        let resonance_scale = utils::interpolate(drive_mult, 1.0, high_pass);
        self.drive = filter_state.drive * resonance_scale;
        self.post_multiply = one / utils::sqrt((filter_state.drive - 1.0) * 0.5 + 1.0);

        let st = filter_state.style;
        if st == style::K12_DB {
            for i in 0..=NUM_STAGES {
                self.stage_scales[i] = low_pass * LOW_PASS_12[i]
                    + band_pass * BAND_PASS_12[i]
                    + high_pass * HIGH_PASS_12[i];
            }
        } else if st == style::K24_DB {
            band_pass = -PolyFloat::abs(blend) + 1.0;
            self.post_multiply = one / utils::sqrt((filter_state.drive - 1.0) * 0.25 + 1.0);

            for i in 0..=NUM_STAGES {
                self.stage_scales[i] = low_pass * LOW_PASS_24[i]
                    + band_pass * BAND_PASS_24[i]
                    + high_pass * HIGH_PASS_24[i];
            }
        } else if st == style::DUAL_NOTCH_BAND {
            self.drive = filter_state.drive;
            let low_pass_fade = utils::min(blend + 1.0, one);
            let high_pass_fade = utils::min(-blend + 1.0, one);
            self.stage_scales[0] = low_pass_fade;
            self.stage_scales[1] = low_pass_fade * -4.0;
            self.stage_scales[2] = high_pass_fade * 4.0 + low_pass_fade * 8.0;
            self.stage_scales[3] = high_pass_fade * -8.0 - low_pass_fade * 8.0;
            self.stage_scales[4] = high_pass_fade * 4.0 + low_pass_fade * 4.0;
        } else if st == style::NOTCH_PASS_SWAP {
            self.post_multiply = one / utils::sqrt((filter_state.drive - 1.0) * 0.5 + 1.0);
            let low_pass_fade = utils::min(blend + 1.0, one);
            let low_pass_fade2 = low_pass_fade * low_pass_fade;
            let high_pass_fade = utils::min(-blend + 1.0, one);
            let high_pass_fade2 = high_pass_fade * high_pass_fade;
            let low_high_pass_fade = low_pass_fade * high_pass_fade;
            self.stage_scales[0] = low_pass_fade2;
            self.stage_scales[1] = low_pass_fade2 * -4.0;
            self.stage_scales[2] = low_pass_fade2 * 6.0 + low_high_pass_fade * 2.0;
            self.stage_scales[3] = low_pass_fade2 * -4.0 - low_high_pass_fade * 4.0;
            self.stage_scales[4] = low_pass_fade2 + high_pass_fade2 + low_high_pass_fade * 2.0;
        } else if st == style::BAND_PEAK_NOTCH {
            let drive_t = PolyFloat::min(-blend + 1.0, one);
            self.drive = utils::interpolate(filter_state.drive, self.drive, drive_t);

            let drive_inv_t = -drive_t + 1.0;
            let mult = utils::sqrt((drive_inv_t * drive_inv_t) * 0.5 + 0.5);
            let peak_band_value = -utils::max(-blend, PolyFloat::splat(0.0));
            let low_high = mult * (peak_band_value + 1.0);
            let band = mult * (peak_band_value - blend + 1.0) * 2.0;

            for i in 0..=NUM_STAGES {
                self.stage_scales[i] =
                    low_high * LOW_PASS_12[i] + band * BAND_PASS_12[i] + low_high * HIGH_PASS_12[i];
            }
        }
    }
}

impl SynthFilter for LadderFilter {
    fn setup_filter(&mut self, _this: &Processor, filter_state: &FilterState) {
        let resonance_percent = utils::clamp(filter_state.resonance_percent, 0.0, 1.0);
        let mut resonance_adjust = resonance_percent;
        if filter_state.style != 0 {
            resonance_adjust = utils::sin(resonance_percent * (0.5 * PI));
        }

        self.resonance =
            utils::interpolate(Self::MIN_RESONANCE, Self::MAX_RESONANCE, resonance_adjust);
        self.resonance += filter_state.drive_percent
            * filter_state.resonance_percent
            * Self::DRIVE_RESONANCE_BOOST;

        self.set_stage_scales(filter_state);
    }
}

impl ProcessorImpl for LadderFilter {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let mut current_resonance = self.resonance;
        let mut current_drive = self.drive;
        let mut current_post_multiply = self.post_multiply;
        let mut current_stage_scales = self.stage_scales;

        let mut filter_state = std::mem::take(&mut self.filter_state);
        filter_state.load_settings(this);
        self.setup_filter(this, &filter_state);
        self.filter_state = filter_state;

        let reset_mask = this.get_reset_mask(input::RESET);
        if reset_mask.any_mask() != 0 {
            self.reset_state(reset_mask);

            current_resonance = utils::mask_load(current_resonance, self.resonance, reset_mask);
            current_drive = utils::mask_load(current_drive, self.drive, reset_mask);
            current_post_multiply =
                utils::mask_load(current_post_multiply, self.post_multiply, reset_mask);

            for i in 0..=NUM_STAGES {
                current_stage_scales[i] =
                    utils::mask_load(current_stage_scales[i], self.stage_scales[i], reset_mask);
            }
        }

        let tick_increment = 1.0f32 / num_samples as f32;
        let delta_resonance = (self.resonance - current_resonance) * tick_increment;
        let delta_drive = (self.drive - current_drive) * tick_increment;
        let delta_post_multiply = (self.post_multiply - current_post_multiply) * tick_increment;
        let mut delta_stage_scales = [PolyFloat::splat(0.0); NUM_STAGES + 1];
        for i in 0..=NUM_STAGES {
            delta_stage_scales[i] =
                (self.stage_scales[i] - current_stage_scales[i]) * tick_increment;
        }

        let audio_in = this.input_buffer(input::AUDIO);
        let audio_out = this.output_buffer(0);
        let lookup = coefficient_lookup();
        let base_midi = self.filter_state.midi_cutoff_at(num_samples - 1);
        let base_frequency =
            utils::midi_note_to_frequency(base_midi) * (1.0f32 / this.sample_rate() as f32);
        let max_frequency = PolyFloat::splat(Self::MAX_CUTOFF / this.sample_rate() as f32);

        for i in 0..num_samples {
            let midi_delta = self.filter_state.midi_cutoff_at(i) - base_midi;
            let frequency = utils::min(
                base_frequency * futils::midi_offset_to_ratio(midi_delta),
                max_frequency,
            );
            let coefficient = lookup.cubic_lookup(frequency);

            current_resonance += delta_resonance;
            current_drive += delta_drive;
            current_post_multiply += delta_post_multiply;
            for stage in 0..=NUM_STAGES {
                current_stage_scales[stage] += delta_stage_scales[stage];
            }

            self.tick(
                audio_in[i].get(),
                coefficient,
                current_resonance,
                current_drive,
            );
            let mut total = current_stage_scales[0] * self.filter_input;

            for stage in 0..NUM_STAGES {
                total += current_stage_scales[stage + 1] * self.stages[stage].current_state();
            }

            audio_out[i].set(total * current_post_multiply);
        }
    }

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.reset_state(reset_mask);
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.hard_reset_state();
    }
}
