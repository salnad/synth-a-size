// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::SallenKeyFilter`: a two-pole Sallen-Key filter with a `tanh`
//! input stage, optionally preceded by a second two-pole stage (model
//! "analog").
//!
//! Ports `vital/src/synthesis/filters/sallen_key_filter.h` and
//! `sallen_key_filter.cpp`.

use sas_dsp_core::{Buffer, OnePoleFilter, Processor, ProcessorImpl, ProcessorRef};
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyMask};

use crate::synth_filter::{coefficient_lookup, input, style, FilterState, SynthFilter};

/// Which `process*` loop a block runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    K12,
    K24,
    Dual,
}

/// The smoothed per-block values passed into the `process*` loops.
#[derive(Clone, Copy)]
struct Current {
    resonance: PolyFloat,
    drive: PolyFloat,
    post_multiply: PolyFloat,
    low: PolyFloat,
    band: PolyFloat,
    high: PolyFloat,
}

/// `vital::SallenKeyFilter`.
#[derive(Clone, Debug)]
pub struct SallenKeyFilter {
    filter_state: FilterState,
    resonance: PolyFloat,
    drive: PolyFloat,
    post_multiply: PolyFloat,
    low_pass_amount: PolyFloat,
    band_pass_amount: PolyFloat,
    high_pass_amount: PolyFloat,
    stage1_input: PolyFloat,
    pre_stage1: OnePoleFilter,
    pre_stage2: OnePoleFilter,
    stage1: OnePoleFilter,
    stage2: OnePoleFilter,
}

impl Default for SallenKeyFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl SallenKeyFilter {
    pub const MIN_RESONANCE: f32 = 0.0;
    pub const MAX_RESONANCE: f32 = 2.15;
    pub const DRIVE_RESONANCE_BOOST: f32 = 1.1;
    pub const MAX_VISIBLE_RESONANCE: f32 = 2.0;
    pub const MIN_CUTOFF: f32 = 1.0;

    /// `tuneResonance(resonance, coefficient)`.
    #[inline(always)]
    pub fn tune_resonance(resonance: PolyFloat, coefficient: PolyFloat) -> PolyFloat {
        resonance / utils::max(PolyFloat::splat(1.0), coefficient * 0.09 + 0.97)
    }

    /// The kernel of `SallenKeyFilter()`.
    pub fn new() -> Self {
        let zero = PolyFloat::splat(0.0);
        let mut filter = SallenKeyFilter {
            filter_state: FilterState::default(),
            resonance: zero,
            drive: zero,
            post_multiply: zero,
            low_pass_amount: zero,
            band_pass_amount: zero,
            high_pass_amount: zero,
            stage1_input: zero,
            pre_stage1: OnePoleFilter::new(),
            pre_stage2: OnePoleFilter::new(),
            stage1: OnePoleFilter::new(),
            stage2: OnePoleFilter::new(),
        };
        filter.hard_reset_state();
        filter
    }

    /// `new SallenKeyFilter()`: `Processor(kNumInputs, 1)`.
    pub fn create() -> ProcessorRef {
        Processor::new(Self::new(), input::NUM_INPUTS, 1, false, 1)
    }

    pub fn get_resonance(&self) -> PolyFloat {
        self.resonance
    }

    pub fn get_drive(&self) -> PolyFloat {
        self.drive
    }

    pub fn get_low_amount(&self) -> PolyFloat {
        self.low_pass_amount
    }

    pub fn get_band_amount(&self) -> PolyFloat {
        self.band_pass_amount
    }

    pub fn get_high_amount(&self) -> PolyFloat {
        self.high_pass_amount
    }

    pub fn get_low_amount24(&self, style: i32) -> PolyFloat {
        if style == style::DUAL_NOTCH_BAND {
            return self.high_pass_amount;
        }
        self.low_pass_amount
    }

    pub fn get_high_amount24(&self, style: i32) -> PolyFloat {
        if style == style::DUAL_NOTCH_BAND {
            return self.low_pass_amount;
        }
        self.high_pass_amount
    }

    /// `reset(reset_mask)`.
    pub fn reset_state(&mut self, reset_mask: PolyMask) {
        self.stage1_input = utils::mask_load(self.stage1_input, 0.0, reset_mask);
        self.pre_stage1.reset(reset_mask);
        self.pre_stage2.reset(reset_mask);
        self.stage1.reset(reset_mask);
        self.stage2.reset(reset_mask);
    }

    /// `hardReset()`.
    pub fn hard_reset_state(&mut self) {
        self.reset_state(PolyMask::full());
        self.resonance = PolyFloat::splat(0.0);
        self.drive = PolyFloat::splat(0.0);
        self.post_multiply = PolyFloat::splat(0.0);
        self.low_pass_amount = PolyFloat::splat(0.0);
        self.band_pass_amount = PolyFloat::splat(0.0);
        self.high_pass_amount = PolyFloat::splat(0.0);
    }

    /// `processWithInput(audio_in, num_samples)`.
    pub fn process_with_input_impl(
        &mut self,
        this: &Processor,
        audio_in: &Buffer,
        num_samples: usize,
    ) {
        let mut current = Current {
            resonance: self.resonance,
            drive: self.drive,
            post_multiply: self.post_multiply,
            low: self.low_pass_amount,
            band: self.band_pass_amount,
            high: self.high_pass_amount,
        };

        let mut filter_state = std::mem::take(&mut self.filter_state);
        filter_state.load_settings(this);
        self.setup_filter(this, &filter_state);
        self.filter_state = filter_state;

        let reset_mask = this.get_reset_mask(input::RESET);
        if reset_mask.any_mask() != 0 {
            self.reset_state(reset_mask);
            current.resonance = utils::mask_load(current.resonance, self.resonance, reset_mask);
            current.drive = utils::mask_load(current.drive, self.drive, reset_mask);
            current.post_multiply =
                utils::mask_load(current.post_multiply, self.post_multiply, reset_mask);
            current.low = utils::mask_load(current.low, self.low_pass_amount, reset_mask);
            current.band = utils::mask_load(current.band, self.band_pass_amount, reset_mask);
            current.high = utils::mask_load(current.high, self.high_pass_amount, reset_mask);
        }

        let mode = if self.filter_state.style == style::K12_DB {
            Mode::K12
        } else if self.filter_state.style == style::DUAL_NOTCH_BAND {
            Mode::Dual
        } else {
            Mode::K24
        };
        self.process_mode(this, mode, audio_in, num_samples, current);
    }

    /// `process12`, `process24` and `processDual`. `processDual` neither
    /// smooths nor uses the band amount and mixes its outputs differently.
    fn process_mode(
        &mut self,
        this: &Processor,
        mode: Mode,
        audio_in: &Buffer,
        num_samples: usize,
        current: Current,
    ) {
        let Current {
            resonance: mut current_resonance,
            drive: mut current_drive,
            post_multiply: mut current_post_multiply,
            low: mut current_low,
            band: mut current_band,
            high: mut current_high,
        } = current;
        let tick_increment = 1.0f32 / num_samples as f32;
        let delta_resonance = (self.resonance - current_resonance) * tick_increment;
        let delta_drive = (self.drive - current_drive) * tick_increment;
        let delta_post_multiply = (self.post_multiply - current_post_multiply) * tick_increment;
        let delta_low = (self.low_pass_amount - current_low) * tick_increment;
        let delta_band = (self.band_pass_amount - current_band) * tick_increment;
        let delta_high = (self.high_pass_amount - current_high) * tick_increment;

        let lookup = coefficient_lookup();
        let base_midi = self.filter_state.midi_cutoff_at(num_samples - 1);
        let base_frequency =
            utils::midi_note_to_frequency(base_midi) * (1.0f32 / this.sample_rate() as f32);
        let audio_out = this.output_buffer(0);
        let one = PolyFloat::splat(1.0);

        for i in 0..num_samples {
            let midi_delta = self.filter_state.midi_cutoff_at(i) - base_midi;
            let frequency = utils::min(
                base_frequency * futils::midi_offset_to_ratio(midi_delta),
                one,
            );
            let coefficient = lookup.cubic_lookup(frequency);
            let coefficient_squared = coefficient * coefficient;
            let coefficient2 = coefficient * 2.0;

            current_resonance += delta_resonance;
            current_drive += delta_drive;
            current_post_multiply += delta_post_multiply;
            current_low += delta_low;
            if mode != Mode::Dual {
                current_band += delta_band;
            }
            current_high += delta_high;

            let resonance = Self::tune_resonance(current_resonance, coefficient2);
            let normalizer = one / (resonance * (coefficient_squared - coefficient) + 1.0);
            let input = audio_in[i].get();

            if mode == Mode::K12 {
                let stage1_feedback_mult = coefficient2 - coefficient * coefficient - 1.0;
                self.tick(
                    input,
                    coefficient,
                    resonance,
                    stage1_feedback_mult,
                    current_drive,
                    normalizer,
                );
            } else {
                let stage1_feedback_mult = coefficient2 - coefficient_squared - 1.0;
                let pre_normalizer = one / ((coefficient_squared - coefficient) + 1.0);
                let band = if mode == Mode::Dual {
                    PolyFloat::splat(0.0)
                } else {
                    current_band
                };
                self.tick24(
                    input,
                    coefficient,
                    resonance,
                    stage1_feedback_mult,
                    current_drive,
                    pre_normalizer,
                    normalizer,
                    current_low,
                    band,
                    current_high,
                );
            }

            let stage2_input = self.stage1.current_state();
            let low_pass = self.stage2.current_state();
            let out = if mode == Mode::Dual {
                let high_pass = self.stage1_input - stage2_input - stage2_input + low_pass;
                let low = current_high * low_pass;
                utils::mul_add(low, current_low, high_pass)
            } else {
                let band_pass = stage2_input - low_pass;
                let high_pass = self.stage1_input - stage2_input - band_pass;
                let low = current_low * low_pass;
                let band_low = utils::mul_add(low, current_band, band_pass);
                utils::mul_add(band_low, current_high, high_pass)
            };
            audio_out[i].set(out * current_post_multiply);
        }
    }

    /// `tick24(...)`.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn tick24(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        stage1_feedback_mult: PolyFloat,
        drive: PolyFloat,
        pre_normalizer: PolyFloat,
        normalizer: PolyFloat,
        low: PolyFloat,
        band: PolyFloat,
        high: PolyFloat,
    ) {
        let mult_stage2 = -coefficient + 1.0;
        let feedback = utils::mul_add(
            stage1_feedback_mult * self.pre_stage1.next_state(),
            mult_stage2,
            self.pre_stage2.next_state(),
        );
        let stage1_input = (audio_in - feedback) * pre_normalizer;
        let stage1_out = self.pre_stage1.tick_basic(stage1_input, coefficient);
        let stage2_out = self.pre_stage2.tick_basic(stage1_out, coefficient);

        let band_pass_out = stage1_out - stage2_out;
        let high_pass_out = stage1_input - stage1_out - band_pass_out;
        let low_out = low * stage2_out;
        let band_low_out = utils::mul_add(low_out, band, band_pass_out);
        let audio_out = utils::mul_add(band_low_out, high, high_pass_out);
        self.tick(
            audio_out,
            coefficient,
            resonance,
            stage1_feedback_mult,
            drive,
            normalizer,
        );
    }

    /// `tick(...)`.
    #[inline(always)]
    pub fn tick(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        stage1_feedback_mult: PolyFloat,
        drive: PolyFloat,
        normalizer: PolyFloat,
    ) {
        let mult_stage2 = -coefficient + 1.0;
        let feedback = utils::mul_add(
            stage1_feedback_mult * self.stage1.next_state(),
            mult_stage2,
            self.stage2.next_state(),
        );
        self.stage1_input = futils::tanh((drive * audio_in - resonance * feedback) * normalizer);
        let stage1_out = self.stage1.tick_basic(self.stage1_input, coefficient);
        self.stage2.tick_basic(stage1_out, coefficient);
    }
}

impl SynthFilter for SallenKeyFilter {
    fn setup_filter(&mut self, _this: &Processor, filter_state: &FilterState) {
        let mut resonance_percent = utils::clamp(filter_state.resonance_percent, 0.0, 1.0);
        resonance_percent = utils::sqrt(resonance_percent);
        self.resonance =
            utils::interpolate(Self::MIN_RESONANCE, Self::MAX_RESONANCE, resonance_percent);
        self.resonance += filter_state.drive_percent
            * filter_state.resonance_percent
            * Self::DRIVE_RESONANCE_BOOST;

        let blend = utils::clamp(filter_state.pass_blend - 1.0, -1.0, 1.0);

        let one = PolyFloat::splat(1.0);
        let resonance_scale = resonance_percent * resonance_percent * 2.0 + 1.0;
        self.drive = filter_state.drive / resonance_scale;

        if filter_state.style == style::DUAL_NOTCH_BAND {
            let t = blend * 0.5 + 0.5;
            let drive_t = PolyFloat::min(-blend + 1.0, one);
            let drive_mult = -t + 2.0;
            self.drive = utils::interpolate(filter_state.drive, self.drive * drive_mult, drive_t);

            self.low_pass_amount = t;
            self.band_pass_amount = PolyFloat::splat(0.0);
            self.high_pass_amount = one;
        } else if filter_state.style == style::NOTCH_PASS_SWAP {
            let drive_t = PolyFloat::abs(blend);
            self.drive = utils::interpolate(filter_state.drive, self.drive, drive_t);

            self.low_pass_amount = utils::min(-blend + 1.0, one);
            self.band_pass_amount = PolyFloat::splat(0.0);
            self.high_pass_amount = utils::min(blend + 1.0, one);
        } else if filter_state.style == style::BAND_PEAK_NOTCH {
            let drive_t = PolyFloat::min(-blend + 1.0, one);
            self.drive = utils::interpolate(filter_state.drive, self.drive, drive_t);

            let drive_inv_t = -drive_t + 1.0;
            let mult = utils::sqrt((drive_inv_t * drive_inv_t) * 0.5 + 0.5);
            let peak_band_value = -utils::max(-blend, PolyFloat::splat(0.0));
            self.low_pass_amount = mult * (peak_band_value + 1.0);
            self.band_pass_amount = mult * (peak_band_value - blend + 1.0) * 2.0;
            self.high_pass_amount = self.low_pass_amount;
        } else {
            self.band_pass_amount = utils::sqrt(-blend * blend + 1.0);
            let blend_mask = PolyFloat::less_than(blend, PolyFloat::splat(0.0));
            self.low_pass_amount = (-blend) & blend_mask;
            self.high_pass_amount = blend & !blend_mask;
        }

        self.post_multiply = one / utils::sqrt(resonance_scale * self.drive);
    }
}

impl ProcessorImpl for SallenKeyFilter {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio_in = this.input_buffer(input::AUDIO);
        self.process_with_input_impl(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        self.process_with_input_impl(this, audio_in, num_samples);
    }

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.reset_state(reset_mask);
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.hard_reset_state();
    }
}
