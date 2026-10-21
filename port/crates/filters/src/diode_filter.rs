// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::DiodeFilter`: a four-stage diode ladder with a two-pole input
//! high-pass (model "diode").
//!
//! Ports `vital/src/synthesis/filters/diode_filter.h` and
//! `diode_filter.cpp`.

use sas_dsp_core::{OnePoleFilter, Processor, ProcessorImpl, ProcessorRef};
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyMask};

use crate::synth_filter::{coefficient_lookup, input, style, FilterState, SynthFilter};

/// `DiodeFilter::saturate`.
fn saturate(value: PolyFloat) -> PolyFloat {
    futils::tanh(value)
}

/// `DiodeFilter::saturate2`.
fn saturate2(value: PolyFloat) -> PolyFloat {
    utils::clamp(value, -1.0, 1.0)
}

/// `vital::DiodeFilter`.
#[derive(Clone, Debug)]
pub struct DiodeFilter {
    filter_state: FilterState,
    resonance: PolyFloat,
    drive: PolyFloat,
    post_multiply: PolyFloat,
    /// Upstream leaves `high_pass_ratio_` and `high_pass_amount_`
    /// uninitialized until the first `setupFilter`; here they start at zero.
    high_pass_ratio: PolyFloat,
    high_pass_amount: PolyFloat,
    high_pass_1: OnePoleFilter,
    high_pass_2: OnePoleFilter,
    high_pass_feedback: OnePoleFilter,
    stage1: OnePoleFilter,
    stage2: OnePoleFilter,
    stage3: OnePoleFilter,
    stage4: OnePoleFilter,
}

impl Default for DiodeFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl DiodeFilter {
    pub const MIN_RESONANCE: f32 = 0.7;
    pub const MAX_RESONANCE: f32 = 17.0;
    pub const MIN_CUTOFF: f32 = 1.0;
    pub const HIGH_PASS_FREQUENCY: f32 = 20.0;

    /// The kernel of `DiodeFilter()`.
    pub fn new() -> Self {
        let zero = PolyFloat::splat(0.0);
        let mut filter = DiodeFilter {
            filter_state: FilterState::default(),
            resonance: zero,
            drive: zero,
            post_multiply: zero,
            high_pass_ratio: zero,
            high_pass_amount: zero,
            high_pass_1: OnePoleFilter::new(),
            high_pass_2: OnePoleFilter::new(),
            high_pass_feedback: OnePoleFilter::new(),
            stage1: OnePoleFilter::with_saturate(saturate),
            stage2: OnePoleFilter::new(),
            stage3: OnePoleFilter::new(),
            stage4: OnePoleFilter::with_saturate(saturate2),
        };
        filter.hard_reset_state();
        filter
    }

    /// `new DiodeFilter()`: `Processor(kNumInputs, 1)`.
    pub fn create() -> ProcessorRef {
        Processor::new(Self::new(), input::NUM_INPUTS, 1, false, 1)
    }

    pub fn get_resonance(&self) -> PolyFloat {
        self.resonance
    }

    pub fn get_drive(&self) -> PolyFloat {
        self.drive
    }

    pub fn get_high_pass_ratio(&self) -> PolyFloat {
        self.high_pass_ratio
    }

    pub fn get_high_pass_amount(&self) -> PolyFloat {
        self.high_pass_amount
    }

    /// `reset(reset_mask)`. As upstream, `high_pass_feedback_` is not reset.
    pub fn reset_state(&mut self, reset_mask: PolyMask) {
        self.high_pass_1.reset(reset_mask);
        self.high_pass_2.reset(reset_mask);
        self.stage1.reset(reset_mask);
        self.stage2.reset(reset_mask);
        self.stage3.reset(reset_mask);
        self.stage4.reset(reset_mask);
    }

    /// `hardReset()`.
    pub fn hard_reset_state(&mut self) {
        self.reset_state(PolyMask::full());
        self.resonance = PolyFloat::splat(0.0);
        self.drive = PolyFloat::splat(0.0);
        self.post_multiply = PolyFloat::splat(0.0);
    }

    /// `tick(...)`.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn tick(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        high_pass_ratio: PolyFloat,
        high_pass_amount: PolyFloat,
        high_pass_feedback_coefficient: PolyFloat,
        resonance: PolyFloat,
        drive: PolyFloat,
    ) {
        let high_pass_coefficient = coefficient * high_pass_ratio;
        let high_pass_coefficient2 = high_pass_coefficient * 2.0;
        let high_pass_coefficient_squared = high_pass_coefficient * high_pass_coefficient;
        let high_pass_coefficient_diff = high_pass_coefficient_squared - high_pass_coefficient;
        let high_pass_feedback_mult = high_pass_coefficient2 - high_pass_coefficient_squared - 1.0;
        let high_pass_normalizer = PolyFloat::splat(1.0) / (high_pass_coefficient_diff + 1.0);
        let high_pass_mult_stage2 = -high_pass_coefficient + 1.0;

        let high_pass_feedback = high_pass_feedback_mult * self.high_pass_1.next_state()
            + high_pass_mult_stage2 * self.high_pass_2.next_state();
        let high_pass_input = (audio_in - high_pass_feedback) * high_pass_normalizer;
        let high_pass_1_out = self
            .high_pass_1
            .tick_basic(high_pass_input, high_pass_coefficient);
        let high_pass_2_out = self
            .high_pass_2
            .tick_basic(high_pass_1_out, high_pass_coefficient);
        let mut high_pass_out = high_pass_input - high_pass_1_out * 2.0 + high_pass_2_out;
        high_pass_out = utils::interpolate(audio_in, high_pass_out, high_pass_amount);

        let filter_state = self.stage4.next_sat_state();
        let filter_input = (drive * high_pass_out - resonance * filter_state) * 0.5;
        let sat_input = futils::tanh(filter_input);

        let feedback_input = sat_input + self.stage2.next_sat_state();
        let feedback = self
            .high_pass_feedback
            .tick_basic(feedback_input, high_pass_feedback_coefficient);
        self.stage1.tick(feedback_input - feedback, coefficient);
        self.stage2.tick(
            (self.stage1.current_state() + self.stage3.next_sat_state()) * 0.5,
            coefficient,
        );
        self.stage3.tick(
            (self.stage2.current_state() + self.stage4.next_sat_state()) * 0.5,
            coefficient,
        );
        self.stage4.tick(self.stage3.current_state(), coefficient);
    }
}

impl SynthFilter for DiodeFilter {
    fn setup_filter(&mut self, _this: &Processor, filter_state: &FilterState) {
        const HIGH_PASS_START: f32 = -9.0;
        const HIGH_PASS_END: f32 = -1.0;
        const HIGH_PASS_RANGE: f32 = HIGH_PASS_END - HIGH_PASS_START;

        let mut resonance_percent = utils::clamp(filter_state.resonance_percent, 0.0, 1.0);
        resonance_percent = resonance_percent * (resonance_percent * resonance_percent);
        self.resonance =
            utils::interpolate(Self::MIN_RESONANCE, Self::MAX_RESONANCE, resonance_percent);
        self.drive = (self.resonance * 0.5 + 1.0) * filter_state.drive;
        self.post_multiply = PolyFloat::splat(1.0) / utils::sqrt(filter_state.drive);

        let blend_amount = filter_state.pass_blend * 0.5;

        if filter_state.style == style::K12_DB {
            self.high_pass_ratio = futils::exp2(PolyFloat::splat(HIGH_PASS_END));
            self.high_pass_amount = blend_amount * blend_amount;
        } else {
            self.high_pass_ratio = futils::exp2(blend_amount * HIGH_PASS_RANGE + HIGH_PASS_START);
            self.high_pass_amount = PolyFloat::splat(1.0);
        }
    }
}

impl ProcessorImpl for DiodeFilter {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let mut current_resonance = self.resonance;
        let mut current_drive = self.drive;
        let mut current_post_multiply = self.post_multiply;
        let mut current_high_pass_ratio = self.high_pass_ratio;
        let mut current_high_pass_amount = self.high_pass_amount;

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
            current_high_pass_ratio =
                utils::mask_load(current_high_pass_ratio, self.high_pass_ratio, reset_mask);
            current_high_pass_amount =
                utils::mask_load(current_high_pass_amount, self.high_pass_amount, reset_mask);
        }

        let tick_increment = 1.0f32 / num_samples as f32;
        let delta_resonance = (self.resonance - current_resonance) * tick_increment;
        let delta_drive = (self.drive - current_drive) * tick_increment;
        let delta_post_multiply = (self.post_multiply - current_post_multiply) * tick_increment;
        let delta_high_pass_ratio =
            (self.high_pass_ratio - current_high_pass_ratio) * tick_increment;
        let delta_high_pass_amount =
            (self.high_pass_amount - current_high_pass_amount) * tick_increment;

        let audio_in = this.input_buffer(input::AUDIO);
        let audio_out = this.output_buffer(0);
        let lookup = coefficient_lookup();
        let base_midi = self.filter_state.midi_cutoff_at(num_samples - 1);
        let base_frequency =
            utils::midi_note_to_frequency(base_midi) * (1.0f32 / this.sample_rate() as f32);
        let high_pass_frequency_ratio =
            PolyFloat::splat(Self::HIGH_PASS_FREQUENCY * (1.0f32 / this.sample_rate() as f32));
        let high_pass_feedback_coefficient = lookup.cubic_lookup(high_pass_frequency_ratio);

        for i in 0..num_samples {
            let midi_delta = self.filter_state.midi_cutoff_at(i) - base_midi;
            let frequency = utils::min(
                base_frequency * futils::midi_offset_to_ratio(midi_delta),
                PolyFloat::splat(1.0),
            );
            let coefficient = lookup.cubic_lookup(frequency);

            current_resonance += delta_resonance;
            current_drive += delta_drive;
            current_post_multiply += delta_post_multiply;
            current_high_pass_ratio += delta_high_pass_ratio;
            current_high_pass_amount += delta_high_pass_amount;

            self.tick(
                audio_in[i].get(),
                coefficient,
                current_high_pass_ratio,
                current_high_pass_amount,
                high_pass_feedback_coefficient,
                current_resonance,
                current_drive,
            );
            audio_out[i].set(self.stage4.current_state() * current_post_multiply);
        }
    }

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.reset_state(reset_mask);
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.hard_reset_state();
    }
}
