// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::DirtyFilter`: a two-pole filter inside a saturated feedback loop
//! (model "dirty").
//!
//! Ports `vital/src/synthesis/filters/dirty_filter.h` and
//! `dirty_filter.cpp`.

use sas_dsp_core::{OnePoleFilter, Processor, ProcessorImpl, ProcessorRef};
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
    drive_boost: PolyFloat,
    drive_blend: PolyFloat,
    drive_mult: PolyFloat,
    low: PolyFloat,
    band: PolyFloat,
    high: PolyFloat,
}

/// `vital::DirtyFilter`.
#[derive(Clone, Debug)]
pub struct DirtyFilter {
    filter_state: FilterState,
    coefficient: PolyFloat,
    resonance: PolyFloat,
    drive: PolyFloat,
    drive_boost: PolyFloat,
    drive_blend: PolyFloat,
    drive_mult: PolyFloat,
    low_pass_amount: PolyFloat,
    band_pass_amount: PolyFloat,
    high_pass_amount: PolyFloat,
    pre_stage1: OnePoleFilter,
    pre_stage2: OnePoleFilter,
    stage1: OnePoleFilter,
    stage2: OnePoleFilter,
    stage3: OnePoleFilter,
    stage4: OnePoleFilter,
}

impl Default for DirtyFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl DirtyFilter {
    pub const MIN_RESONANCE: f32 = 0.1;
    pub const MAX_RESONANCE: f32 = 2.15;
    pub const SATURATION_BOOST: f32 = 1.4;
    pub const MAX_VISIBLE_RESONANCE: f32 = 2.0;
    pub const DRIVE_RESONANCE_BOOST: f32 = 0.05;
    pub const MIN_CUTOFF: f32 = 1.0;
    pub const MIN_DRIVE: f32 = 0.1;
    pub const FLAT_RESONANCE: f32 = 1.0;

    /// `tuneResonance(resonance, coefficient)`.
    #[inline(always)]
    pub fn tune_resonance(resonance: PolyFloat, coefficient: PolyFloat) -> PolyFloat {
        resonance / utils::max(PolyFloat::splat(1.0), coefficient * 0.25 + 0.97)
    }

    /// The kernel of `DirtyFilter()`.
    pub fn new() -> Self {
        let zero = PolyFloat::splat(0.0);
        let mut filter = DirtyFilter {
            filter_state: FilterState::default(),
            coefficient: zero,
            resonance: zero,
            drive: zero,
            drive_boost: zero,
            drive_blend: zero,
            drive_mult: zero,
            low_pass_amount: zero,
            band_pass_amount: zero,
            high_pass_amount: zero,
            pre_stage1: OnePoleFilter::new(),
            pre_stage2: OnePoleFilter::new(),
            stage1: OnePoleFilter::new(),
            stage2: OnePoleFilter::new(),
            stage3: OnePoleFilter::with_saturate(futils::quick_tanh),
            stage4: OnePoleFilter::with_saturate(futils::quick_tanh),
        };
        filter.hard_reset_state();
        filter
    }

    /// `new DirtyFilter()`: `Processor(kNumInputs, 1)`.
    pub fn create() -> ProcessorRef {
        Processor::new(Self::new(), input::NUM_INPUTS, 1, false, 1)
    }

    /// `getResonance()`.
    pub fn get_resonance(&self) -> PolyFloat {
        let resonance_in = utils::clamp(
            Self::tune_resonance(self.resonance, self.coefficient * 2.0),
            0.0,
            1.0,
        );
        utils::interpolate(Self::MIN_RESONANCE, Self::MAX_RESONANCE, resonance_in)
            + self.drive_boost
    }

    /// `getDrive()`.
    pub fn get_drive(&self) -> PolyFloat {
        let resonance = self.get_resonance();
        let scaled_drive = utils::max(PolyFloat::splat(Self::MIN_DRIVE), self.drive)
            / (resonance * resonance * 0.5 + 1.0);
        utils::interpolate(self.drive, scaled_drive, self.drive_blend)
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
        self.pre_stage1.reset(reset_mask);
        self.pre_stage2.reset(reset_mask);
        self.stage1.reset(reset_mask);
        self.stage2.reset(reset_mask);
        self.stage3.reset(reset_mask);
        self.stage4.reset(reset_mask);
    }

    /// `hardReset()`.
    pub fn hard_reset_state(&mut self) {
        self.reset_state(PolyMask::full());
        self.coefficient = PolyFloat::splat(0.1);
        self.resonance = PolyFloat::splat(0.0);
        self.drive = PolyFloat::splat(0.0);
        self.drive_boost = PolyFloat::splat(0.0);
        self.drive_blend = PolyFloat::splat(0.0);
        self.drive_mult = PolyFloat::splat(0.0);
        self.low_pass_amount = PolyFloat::splat(0.0);
        self.band_pass_amount = PolyFloat::splat(0.0);
        self.high_pass_amount = PolyFloat::splat(0.0);
    }

    /// `process12`, `process24` and `processDual`. `processDual` neither
    /// smooths nor uses the band amount, and, as upstream, computes but never
    /// applies `delta_drive_mult`: it uses the previous block's `drive_mult_`
    /// (the new one only on lanes reset this block).
    fn process_mode(&mut self, this: &Processor, mode: Mode, num_samples: usize, current: Current) {
        let Current {
            resonance: mut current_resonance,
            drive: mut current_drive,
            drive_boost: mut current_drive_boost,
            drive_blend: mut current_drive_blend,
            drive_mult: current_drive_mult,
            low: mut current_low,
            band: mut current_band,
            high: mut current_high,
        } = current;

        let audio_in = this.input_buffer(input::AUDIO);
        let audio_out = this.output_buffer(0);
        let tick_increment = 1.0f32 / num_samples as f32;
        let delta_resonance = (self.resonance - current_resonance) * tick_increment;
        let delta_drive = (self.drive - current_drive) * tick_increment;
        let delta_drive_boost = (self.drive_boost - current_drive_boost) * tick_increment;
        let delta_drive_blend = (self.drive_blend - current_drive_blend) * tick_increment;
        let delta_low = (self.low_pass_amount - current_low) * tick_increment;
        let delta_band = (self.band_pass_amount - current_band) * tick_increment;
        let delta_high = (self.high_pass_amount - current_high) * tick_increment;

        let lookup = coefficient_lookup();
        let base_midi = self.filter_state.midi_cutoff_at(num_samples - 1);
        let base_frequency =
            utils::midi_note_to_frequency(base_midi) * (1.0f32 / this.sample_rate() as f32);
        let one = PolyFloat::splat(1.0);

        for i in 0..num_samples {
            current_drive_boost += delta_drive_boost;
            current_resonance += delta_resonance;
            let midi_delta = self.filter_state.midi_cutoff_at(i) - base_midi;
            let frequency = utils::min(
                base_frequency * futils::midi_offset_to_ratio(midi_delta),
                one,
            );
            let coefficient = lookup.cubic_lookup(frequency);
            let coefficient_squared = coefficient * coefficient;
            let coefficient2 = coefficient * 2.0;

            let resonance_in = utils::clamp(
                Self::tune_resonance(current_resonance, coefficient2),
                0.0,
                1.0,
            );
            let resonance =
                utils::interpolate(Self::MIN_RESONANCE, Self::MAX_RESONANCE, resonance_in)
                    + current_drive_boost;
            let resonance_squared = resonance * resonance;
            let normalizer = PolyFloat::splat(Self::SATURATION_BOOST) / (resonance_squared + 1.0);

            let input = audio_in[i].get();
            let out = if mode == Mode::K12 {
                let compute = -resonance * (coefficient - coefficient_squared) + 1.0;
                let feed_mult = one / (compute * (coefficient + 1.0));

                current_drive += delta_drive;
                current_drive_blend += delta_drive_blend;
                let scaled_drive = utils::max(PolyFloat::splat(Self::MIN_DRIVE), current_drive)
                    / (resonance_squared * 0.5 + 1.0);
                let drive = utils::interpolate(current_drive, scaled_drive, current_drive_blend);

                current_low += delta_low;
                current_band += delta_band;
                current_high += delta_high;

                self.tick(
                    input,
                    coefficient,
                    resonance,
                    drive,
                    feed_mult,
                    normalizer,
                    current_low,
                    current_band,
                    current_high,
                )
            } else {
                let coefficient_diff = coefficient_squared - coefficient;
                let compute = resonance * coefficient_diff + 1.0;
                let feed_mult = one / (compute * (coefficient + 1.0));
                let pre_feedback = coefficient2 - coefficient_squared - 1.0;
                let pre_normalizer = one / (coefficient_diff * Self::FLAT_RESONANCE + 1.0);

                current_drive += delta_drive;
                current_drive_blend += delta_drive_blend;
                let scaled_drive = utils::max(PolyFloat::splat(Self::MIN_DRIVE), current_drive)
                    / (resonance_squared * 0.5 + 1.0);

                if mode == Mode::Dual {
                    let drive = utils::interpolate(
                        current_drive,
                        scaled_drive * current_drive_mult,
                        current_drive_blend,
                    );
                    current_low += delta_low;
                    current_high += delta_high;
                    self.tick_dual(
                        input,
                        coefficient,
                        resonance,
                        drive,
                        feed_mult,
                        normalizer,
                        pre_feedback,
                        pre_normalizer,
                        current_low,
                        current_high,
                    )
                } else {
                    let drive =
                        utils::interpolate(current_drive, scaled_drive, current_drive_blend);
                    current_low += delta_low;
                    current_band += delta_band;
                    current_high += delta_high;
                    self.tick24(
                        input,
                        coefficient,
                        resonance,
                        drive,
                        feed_mult,
                        normalizer,
                        pre_feedback,
                        pre_normalizer,
                        current_low,
                        current_band,
                        current_high,
                    )
                }
            };
            audio_out[i].set(out);
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
        drive: PolyFloat,
        feed_mult: PolyFloat,
        normalizer: PolyFloat,
        pre_feedback_mult: PolyFloat,
        pre_normalizer: PolyFloat,
        low: PolyFloat,
        band: PolyFloat,
        high: PolyFloat,
    ) -> PolyFloat {
        let mult_stage2 = -coefficient + 1.0;
        let mut feedback = pre_feedback_mult * self.pre_stage1.next_sat_state()
            + mult_stage2 * self.pre_stage2.next_sat_state();
        feedback *= Self::FLAT_RESONANCE;
        let stage1_input = (audio_in - feedback) * pre_normalizer;
        let stage1_out = self.pre_stage1.tick_basic(stage1_input, coefficient);
        let stage2_out = self.pre_stage2.tick_basic(stage1_out, coefficient);

        let band_pass = stage1_out - stage2_out;
        let high_pass = stage1_input - stage1_out - band_pass;
        let pre_out = band * band_pass + high * high_pass + low * stage2_out;
        self.tick(
            pre_out,
            coefficient,
            resonance,
            drive,
            feed_mult,
            normalizer,
            low,
            band,
            high,
        )
    }

    /// `tickDual(...)`.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn tick_dual(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        drive: PolyFloat,
        feed_mult: PolyFloat,
        normalizer: PolyFloat,
        pre_feedback_mult: PolyFloat,
        pre_normalizer: PolyFloat,
        low: PolyFloat,
        high: PolyFloat,
    ) -> PolyFloat {
        let mult_stage2 = -coefficient + 1.0;
        let mut feedback = pre_feedback_mult * self.pre_stage1.next_sat_state()
            + mult_stage2 * self.pre_stage2.next_sat_state();
        feedback *= Self::FLAT_RESONANCE;
        let stage1_input = (audio_in - feedback) * pre_normalizer;
        let stage1_out = self.pre_stage1.tick_basic(stage1_input, coefficient);
        let stage2_out = self.pre_stage2.tick_basic(stage1_out, coefficient);

        let band_pass = stage1_out - stage2_out;
        let high_pass = stage1_input - stage1_out - band_pass;
        let pre_out = low * high_pass + high * stage2_out;
        self.tick(
            pre_out,
            coefficient,
            resonance,
            drive,
            feed_mult,
            normalizer,
            low,
            PolyFloat::splat(0.0),
            high,
        )
    }

    /// `tick(...)`.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn tick(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        drive: PolyFloat,
        feed_mult: PolyFloat,
        normalizer: PolyFloat,
        low: PolyFloat,
        band: PolyFloat,
        high: PolyFloat,
    ) -> PolyFloat {
        let stage1_in = normalizer * audio_in;
        let stage1_out = self.stage1.tick_basic(stage1_in, coefficient);
        let stage2_out = self.stage2.tick_basic(stage1_out, coefficient);

        let band_pass = stage1_out - stage2_out;
        let high_pass = stage1_in - stage1_out - band_pass;
        let pass_output = utils::mul_add(
            utils::mul_add(low * stage2_out, band, band_pass),
            high,
            high_pass,
        );
        let feedback = self.stage4.next_sat_state()
            + utils::mul_add(
                pass_output,
                coefficient,
                pass_output - self.stage3.next_sat_state(),
            );

        let loop_input = futils::tanh(utils::mul_add(
            drive * pass_output,
            resonance,
            feed_mult * feedback,
        ));
        let stage3_out = self.stage3.tick(loop_input, coefficient);
        let stage4_in = loop_input - stage3_out;
        self.stage4.tick(stage4_in, coefficient);
        loop_input * (1.0f32 / Self::SATURATION_BOOST)
    }
}

impl SynthFilter for DirtyFilter {
    fn setup_filter(&mut self, this: &Processor, filter_state: &FilterState) {
        const MAX_MIDI: f32 = 150.0;
        let cutoff = utils::clamp(filter_state.midi_cutoff, 0.0, MAX_MIDI);
        let base_frequency =
            utils::midi_note_to_frequency(cutoff) * (1.0f32 / this.sample_rate() as f32);
        self.coefficient = coefficient_lookup().cubic_lookup(base_frequency);
        self.resonance = utils::sqrt(utils::clamp(filter_state.resonance_percent, 0.0, 1.0));
        self.drive = (filter_state.drive - 1.0) * 2.0 + 1.0;
        self.drive_boost = filter_state.drive_percent * Self::DRIVE_RESONANCE_BOOST;
        self.drive_blend = PolyFloat::splat(1.0);
        self.drive_mult = PolyFloat::splat(1.0);

        let one = PolyFloat::splat(1.0);
        let blend = utils::clamp(filter_state.pass_blend - 1.0, -1.0, 1.0);

        if filter_state.style == style::DUAL_NOTCH_BAND {
            let t = blend * 0.5 + 0.5;
            self.drive_blend = PolyFloat::min(-blend + 1.0, one);
            self.drive_mult = -t + 2.0;

            self.low_pass_amount = t;
            self.band_pass_amount = PolyFloat::splat(0.0);
            self.high_pass_amount = one;
        } else if filter_state.style == style::NOTCH_PASS_SWAP {
            self.drive_blend = PolyFloat::abs(blend);
            self.low_pass_amount = utils::min(-blend + 1.0, one);
            self.band_pass_amount = PolyFloat::splat(0.0);
            self.high_pass_amount = utils::min(blend + 1.0, one);
        } else if filter_state.style == style::BAND_PEAK_NOTCH {
            self.drive_blend = PolyFloat::min(-blend + 1.0, one);
            let drive_inv_t = -self.drive_blend + 1.0;
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
    }
}

impl ProcessorImpl for DirtyFilter {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let mut current = Current {
            resonance: self.resonance,
            drive: self.drive,
            drive_boost: self.drive_boost,
            drive_blend: self.drive_blend,
            drive_mult: self.drive_mult,
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
            current.drive_boost =
                utils::mask_load(current.drive_boost, self.drive_boost, reset_mask);
            current.drive_blend =
                utils::mask_load(current.drive_blend, self.drive_blend, reset_mask);
            current.drive_mult = utils::mask_load(current.drive_mult, self.drive_mult, reset_mask);
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
        self.process_mode(this, mode, num_samples, current);
    }

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.reset_state(reset_mask);
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.hard_reset_state();
    }
}
