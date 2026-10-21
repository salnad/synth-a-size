// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::DigitalSvf`: the trapezoidal state-variable filter (model
//! "digital"), also used by the formant filters and the equalizer.
//!
//! Ports `vital/src/synthesis/filters/digital_svf.h` and `digital_svf.cpp`.

use std::sync::LazyLock;

use sas_dsp_core::{Buffer, OneDimLookup, Processor, ProcessorImpl, ProcessorRef};
use sas_simd::common::PI;
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyMask};

use crate::synth_filter::{input, std_min, style, FilterState, SynthFilter};

/// `DigitalSvf::computeSvfOnePoleFilterCoefficient`.
#[inline]
pub fn compute_svf_one_pole_filter_coefficient(frequency_ratio: f32) -> f32 {
    const MAX_RATIO: f32 = 0.499;
    (std_min(MAX_RATIO, frequency_ratio) * PI).tan()
}

/// `DigitalSvf::SvfCoefficientLookup`
/// (`OneDimLookup<computeSvfOnePoleFilterCoefficient, 2048>`).
pub type SvfCoefficientLookup = OneDimLookup;

static SVF_COEFFICIENT_LOOKUP: LazyLock<SvfCoefficientLookup> =
    LazyLock::new(|| OneDimLookup::new(compute_svf_one_pole_filter_coefficient, 2048, 1.0));

/// `DigitalSvf::getSvfCoefficientLookup()`.
pub fn svf_coefficient_lookup() -> &'static SvfCoefficientLookup {
    &SVF_COEFFICIENT_LOOKUP
}

/// `DigitalSvf::FilterValues`: the output mix of input, band and low.
#[derive(Clone, Copy, Debug, Default)]
pub struct FilterValues {
    pub v0: PolyFloat,
    pub v1: PolyFloat,
    pub v2: PolyFloat,
}

impl FilterValues {
    pub fn hard_reset(&mut self) {
        self.v0 = PolyFloat::splat(0.0);
        self.v1 = PolyFloat::splat(0.0);
        self.v2 = PolyFloat::splat(0.0);
    }

    pub fn reset(&mut self, reset_mask: PolyMask, other: &FilterValues) {
        self.v0 = utils::mask_load(self.v0, other.v0, reset_mask);
        self.v1 = utils::mask_load(self.v1, other.v1, reset_mask);
        self.v2 = utils::mask_load(self.v2, other.v2, reset_mask);
    }

    /// `getDelta(target, increment)`.
    pub fn get_delta(&self, target: &FilterValues, increment: f32) -> FilterValues {
        FilterValues {
            v0: (target.v0 - self.v0) * increment,
            v1: (target.v1 - self.v1) * increment,
            v2: (target.v2 - self.v2) * increment,
        }
    }

    #[inline(always)]
    pub fn increment(&mut self, delta: &FilterValues) {
        self.v0 += delta.v0;
        self.v1 += delta.v1;
        self.v2 += delta.v2;
    }
}

/// Which per-sample tick a block runs.
#[derive(Clone, Copy)]
enum Tick {
    Normal12,
    Basic12,
    Normal24,
}

/// `vital::DigitalSvf`.
#[derive(Clone, Debug)]
pub struct DigitalSvf {
    filter_state: FilterState,
    midi_cutoff: PolyFloat,
    resonance: PolyFloat,
    blends1: FilterValues,
    blends2: FilterValues,
    drive: PolyFloat,
    post_multiply: PolyFloat,
    low_amount: PolyFloat,
    band_amount: PolyFloat,
    high_amount: PolyFloat,
    ic1eq_pre: PolyFloat,
    ic2eq_pre: PolyFloat,
    ic1eq: PolyFloat,
    ic2eq: PolyFloat,
    min_resonance: f32,
    max_resonance: f32,
    basic: bool,
    drive_compensation: bool,
}

impl Default for DigitalSvf {
    fn default() -> Self {
        Self::new()
    }
}

impl DigitalSvf {
    pub const DEFAULT_MIN_RESONANCE: f32 = 0.5;
    pub const DEFAULT_MAX_RESONANCE: f32 = 16.0;
    pub const MIN_CUTOFF: f32 = 1.0;
    pub const MAX_GAIN: f32 = 15.0;
    pub const MIN_GAIN: f32 = -15.0;

    /// The kernel of `DigitalSvf()`.
    pub fn new() -> Self {
        let zero = PolyFloat::splat(0.0);
        let mut svf = DigitalSvf {
            filter_state: FilterState::default(),
            midi_cutoff: zero,
            resonance: zero,
            blends1: FilterValues::default(),
            blends2: FilterValues::default(),
            drive: zero,
            post_multiply: zero,
            low_amount: zero,
            band_amount: zero,
            high_amount: zero,
            ic1eq_pre: zero,
            ic2eq_pre: zero,
            ic1eq: zero,
            ic2eq: zero,
            min_resonance: Self::DEFAULT_MIN_RESONANCE,
            max_resonance: Self::DEFAULT_MAX_RESONANCE,
            basic: false,
            drive_compensation: true,
        };
        svf.hard_reset_state();
        svf
    }

    /// `new DigitalSvf()`: `Processor(kNumInputs, 1)`.
    pub fn create() -> ProcessorRef {
        Self::create_with(Self::new())
    }

    /// A node around a configured kernel (for example after
    /// [`DigitalSvf::set_resonance_bounds`]).
    pub fn create_with(svf: DigitalSvf) -> ProcessorRef {
        Processor::new(svf, input::NUM_INPUTS, 1, false, 1)
    }

    pub fn set_resonance_bounds(&mut self, min: f32, max: f32) {
        self.min_resonance = min;
        self.max_resonance = max;
    }

    pub fn set_basic(&mut self, basic: bool) {
        self.basic = basic;
    }

    pub fn set_drive_compensation(&mut self, drive_compensation: bool) {
        self.drive_compensation = drive_compensation;
    }

    pub fn get_drive(&self) -> PolyFloat {
        self.drive * self.post_multiply
    }

    pub fn get_midi_cutoff(&self) -> PolyFloat {
        self.midi_cutoff
    }

    pub fn get_resonance(&self) -> PolyFloat {
        self.resonance
    }

    pub fn get_low_amount(&self) -> PolyFloat {
        self.low_amount
    }

    pub fn get_band_amount(&self) -> PolyFloat {
        self.band_amount
    }

    pub fn get_high_amount(&self) -> PolyFloat {
        self.high_amount
    }

    pub fn get_low_amount24(&self, style: i32) -> PolyFloat {
        if style == style::DUAL_NOTCH_BAND {
            return self.high_amount;
        }
        self.low_amount
    }

    pub fn get_high_amount24(&self, style: i32) -> PolyFloat {
        if style == style::DUAL_NOTCH_BAND {
            return self.low_amount;
        }
        self.high_amount
    }

    /// `reset(reset_mask)`.
    pub fn reset_state(&mut self, reset_mask: PolyMask) {
        self.ic1eq_pre = utils::mask_load(self.ic1eq_pre, 0.0, reset_mask);
        self.ic2eq_pre = utils::mask_load(self.ic2eq_pre, 0.0, reset_mask);
        self.ic1eq = utils::mask_load(self.ic1eq, 0.0, reset_mask);
        self.ic2eq = utils::mask_load(self.ic2eq, 0.0, reset_mask);
    }

    /// `hardReset()`.
    pub fn hard_reset_state(&mut self) {
        self.reset_state(PolyMask::full());
        self.resonance = PolyFloat::splat(1.0);
        self.blends1.hard_reset();
        self.blends2.hard_reset();

        self.low_amount = PolyFloat::splat(0.0);
        self.band_amount = PolyFloat::splat(0.0);
        self.high_amount = PolyFloat::splat(0.0);

        self.drive = PolyFloat::splat(0.0);
        self.post_multiply = PolyFloat::splat(0.0);
    }

    /// `processWithInput(audio_in, num_samples)`.
    pub fn process_with_input_impl(
        &mut self,
        this: &Processor,
        audio_in: &Buffer,
        num_samples: usize,
    ) {
        let mut blends1 = self.blends1;
        let mut blends2 = self.blends2;
        let mut current_resonance = self.resonance;
        let mut current_drive = self.drive;
        let mut current_post_multiply = self.post_multiply;

        let mut filter_state = std::mem::take(&mut self.filter_state);
        filter_state.load_settings(this);
        self.setup_filter(this, &filter_state);
        self.filter_state = filter_state;

        let reset_mask = this.get_reset_mask(input::RESET);

        if reset_mask.any_mask() != 0 {
            self.reset_state(reset_mask);
            blends1.reset(reset_mask, &self.blends1);
            blends2.reset(reset_mask, &self.blends2);
            current_resonance = utils::mask_load(current_resonance, self.resonance, reset_mask);
            current_drive = utils::mask_load(current_drive, self.drive, reset_mask);
            current_post_multiply =
                utils::mask_load(current_post_multiply, self.post_multiply, reset_mask);
        }

        let current = Current {
            resonance: current_resonance,
            drive: current_drive,
            post_multiply: current_post_multiply,
        };
        let style = self.filter_state.style;
        if style == style::SHELVING || self.basic {
            self.process_single(
                this,
                Tick::Basic12,
                audio_in,
                num_samples,
                current,
                &mut blends1,
            );
        } else if style == style::DUAL_NOTCH_BAND {
            self.process_dual(
                this,
                audio_in,
                num_samples,
                current,
                &mut blends1,
                &mut blends2,
            );
        } else if style == style::K12_DB {
            self.process_single(
                this,
                Tick::Normal12,
                audio_in,
                num_samples,
                current,
                &mut blends1,
            );
        } else {
            self.process_single(
                this,
                Tick::Normal24,
                audio_in,
                num_samples,
                current,
                &mut blends1,
            );
        }
    }

    /// `process12`, `processBasic12` and `process24`, which differ only in
    /// the tick and in `process12` clamping the cutoff buffer at 0.
    /// (`processBasic24` has no caller upstream.)
    fn process_single(
        &mut self,
        this: &Processor,
        tick: Tick,
        audio_in: &Buffer,
        num_samples: usize,
        current: Current,
        blends: &mut FilterValues,
    ) {
        let Current {
            resonance: mut current_resonance,
            drive: mut current_drive,
            post_multiply: mut current_post_multiply,
        } = current;
        let sample_inc = 1.0f32 / num_samples as f32;
        let delta_blends = blends.get_delta(&self.blends1, sample_inc);
        let delta_resonance = (self.resonance - current_resonance) * sample_inc;
        let delta_drive = (self.drive - current_drive) * sample_inc;
        let delta_post_multiply = (self.post_multiply - current_post_multiply) * sample_inc;

        let audio_out = this.output_buffer(0);
        let coefficient_lookup = svf_coefficient_lookup();
        let midi_cutoff_buffer = self
            .filter_state
            .midi_cutoff_buffer
            .clone()
            .expect("load_settings ran");
        let base_midi = midi_cutoff_buffer[num_samples - 1].get();
        let base_frequency =
            utils::midi_note_to_frequency(base_midi) * (1.0f32 / this.sample_rate() as f32);

        for i in 0..num_samples {
            let midi = midi_cutoff_buffer[i].get();
            let midi_delta = match tick {
                Tick::Normal12 => utils::max(midi, PolyFloat::splat(0.0)) - base_midi,
                _ => midi - base_midi,
            };
            let frequency = utils::min(
                base_frequency * futils::midi_offset_to_ratio(midi_delta),
                PolyFloat::splat(1.0),
            );
            let coefficient = coefficient_lookup.cubic_lookup(frequency);

            blends.increment(&delta_blends);
            current_resonance += delta_resonance;
            current_drive += delta_drive;
            current_post_multiply += delta_post_multiply;

            let input = audio_in[i].get();
            let result = match tick {
                Tick::Normal12 => {
                    self.tick(input, coefficient, current_resonance, current_drive, blends)
                }
                Tick::Basic12 => {
                    self.tick_basic(input, coefficient, current_resonance, current_drive, blends)
                }
                Tick::Normal24 => {
                    self.tick24(input, coefficient, current_resonance, current_drive, blends)
                }
            };
            audio_out[i].set(result * current_post_multiply);
        }
    }

    /// `processDual`.
    fn process_dual(
        &mut self,
        this: &Processor,
        audio_in: &Buffer,
        num_samples: usize,
        current: Current,
        blends1: &mut FilterValues,
        blends2: &mut FilterValues,
    ) {
        let Current {
            resonance: mut current_resonance,
            drive: mut current_drive,
            post_multiply: mut current_post_multiply,
        } = current;
        let sample_inc = 1.0f32 / num_samples as f32;
        let delta_blends1 = blends1.get_delta(&self.blends1, sample_inc);
        let delta_blends2 = blends2.get_delta(&self.blends2, sample_inc);
        let delta_resonance = (self.resonance - current_resonance) * sample_inc;
        let delta_drive = (self.drive - current_drive) * sample_inc;
        let delta_post_multiply = (self.post_multiply - current_post_multiply) * sample_inc;

        let audio_out = this.output_buffer(0);
        let coefficient_lookup = svf_coefficient_lookup();
        let midi_cutoff_buffer = self
            .filter_state
            .midi_cutoff_buffer
            .clone()
            .expect("load_settings ran");
        let base_midi = midi_cutoff_buffer[num_samples - 1].get();
        let base_frequency =
            utils::midi_note_to_frequency(base_midi) * (1.0f32 / this.sample_rate() as f32);

        for i in 0..num_samples {
            let midi_delta = midi_cutoff_buffer[i].get() - base_midi;
            let frequency = utils::min(
                base_frequency * futils::midi_offset_to_ratio(midi_delta),
                PolyFloat::splat(1.0),
            );
            let coefficient = coefficient_lookup.cubic_lookup(frequency);

            blends1.increment(&delta_blends1);
            blends2.increment(&delta_blends2);
            current_resonance += delta_resonance;
            current_drive += delta_drive;
            current_post_multiply += delta_post_multiply;

            let result = self.tick_dual(
                audio_in[i].get(),
                coefficient,
                current_resonance,
                current_drive,
                blends1,
                blends2,
            );
            audio_out[i].set(result * current_post_multiply);
        }
    }

    /// `tick`.
    #[inline(always)]
    pub fn tick(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        drive: PolyFloat,
        blends: &FilterValues,
    ) -> PolyFloat {
        futils::hard_tanh(self.tick_basic(audio_in, coefficient, resonance, drive, blends))
    }

    /// `tickBasic`.
    #[inline(always)]
    pub fn tick_basic(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        drive: PolyFloat,
        blends: &FilterValues,
    ) -> PolyFloat {
        let coefficient_squared = coefficient * coefficient;
        let coefficient_0 =
            PolyFloat::splat(1.0) / (coefficient_squared + coefficient * resonance + 1.0);
        let coefficient_1 = coefficient_0 * coefficient;
        let coefficient_2 = coefficient_0 * coefficient_squared;
        let input = drive * audio_in;

        let v3 = input - self.ic2eq;
        let v1 = utils::mul_add(coefficient_0 * self.ic1eq, coefficient_1, v3);
        let v2 = utils::mul_add(
            utils::mul_add(self.ic2eq, coefficient_1, self.ic1eq),
            coefficient_2,
            v3,
        );
        self.ic1eq = v1 * 2.0 - self.ic1eq;
        self.ic2eq = v2 * 2.0 - self.ic2eq;

        utils::mul_add(
            utils::mul_add(blends.v0 * input, blends.v1, v1),
            blends.v2,
            v2,
        )
    }

    /// `tick24`.
    #[inline(always)]
    pub fn tick24(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        drive: PolyFloat,
        blends: &FilterValues,
    ) -> PolyFloat {
        let coefficient_squared = coefficient * coefficient;
        let pre_coefficient_0 = PolyFloat::splat(1.0) / (coefficient_squared + coefficient + 1.0);
        let pre_coefficient_1 = pre_coefficient_0 * coefficient;
        let pre_coefficient_2 = pre_coefficient_0 * coefficient_squared;

        let input = drive * audio_in;

        let v3_pre = input - self.ic2eq_pre;
        let v1_pre = utils::mul_add(
            pre_coefficient_0 * self.ic1eq_pre,
            pre_coefficient_1,
            v3_pre,
        );
        let v2_pre = utils::mul_add(
            utils::mul_add(self.ic2eq_pre, pre_coefficient_1, self.ic1eq_pre),
            pre_coefficient_2,
            v3_pre,
        );
        self.ic1eq_pre = v1_pre * 2.0 - self.ic1eq_pre;
        self.ic2eq_pre = v2_pre * 2.0 - self.ic2eq_pre;
        let out_pre = utils::mul_add(
            utils::mul_add(blends.v0 * input, blends.v1, v1_pre),
            blends.v2,
            v2_pre,
        );

        let distort = futils::hard_tanh(out_pre);

        self.tick(
            distort,
            coefficient,
            resonance,
            PolyFloat::splat(1.0),
            blends,
        )
    }

    /// `tickBasic24`.
    #[inline(always)]
    pub fn tick_basic24(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        drive: PolyFloat,
        blends: &FilterValues,
    ) -> PolyFloat {
        let coefficient_squared = coefficient * coefficient;
        let pre_coefficient_0 = PolyFloat::splat(1.0) / (coefficient_squared + coefficient + 1.0);
        let pre_coefficient_1 = pre_coefficient_0 * coefficient;
        let pre_coefficient_2 = pre_coefficient_0 * coefficient_squared;

        let v3_pre = audio_in - self.ic2eq_pre;
        let v1_pre = utils::mul_add(
            pre_coefficient_0 * self.ic1eq_pre,
            pre_coefficient_1,
            v3_pre,
        );
        let v2_pre = utils::mul_add(
            utils::mul_add(self.ic2eq_pre, pre_coefficient_1, self.ic1eq_pre),
            pre_coefficient_2,
            v3_pre,
        );
        self.ic1eq_pre = v1_pre * 2.0 - self.ic1eq_pre;
        self.ic2eq_pre = v2_pre * 2.0 - self.ic2eq_pre;
        let out_pre = utils::mul_add(
            utils::mul_add(blends.v0 * audio_in, blends.v1, v1_pre),
            blends.v2,
            v2_pre,
        );

        self.tick_basic(out_pre, coefficient, resonance, drive, blends)
    }

    /// `tickDual`.
    #[inline(always)]
    pub fn tick_dual(
        &mut self,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        drive: PolyFloat,
        blends1: &FilterValues,
        blends2: &FilterValues,
    ) -> PolyFloat {
        let coefficient_squared = coefficient * coefficient;
        let pre_coefficient_0 = PolyFloat::splat(1.0) / (coefficient_squared + coefficient + 1.0);
        let pre_coefficient_1 = pre_coefficient_0 * coefficient;
        let pre_coefficient_2 = pre_coefficient_0 * coefficient_squared;
        let coefficient_0 =
            PolyFloat::splat(1.0) / (coefficient_squared + coefficient * resonance + 1.0);
        let coefficient_1 = coefficient_0 * coefficient;
        let coefficient_2 = coefficient_0 * coefficient_squared;

        let input = drive * audio_in;

        let v3_pre = input - self.ic2eq_pre;
        let v1_pre = utils::mul_add(
            pre_coefficient_0 * self.ic1eq_pre,
            pre_coefficient_1,
            v3_pre,
        );
        let v2_pre = utils::mul_add(
            utils::mul_add(self.ic2eq_pre, pre_coefficient_1, self.ic1eq_pre),
            pre_coefficient_2,
            v3_pre,
        );
        self.ic1eq_pre = v1_pre * 2.0 - self.ic1eq_pre;
        self.ic2eq_pre = v2_pre * 2.0 - self.ic2eq_pre;
        let out_pre = utils::mul_add(
            utils::mul_add(blends1.v0 * input, blends1.v1, v1_pre),
            blends1.v2,
            v2_pre,
        );

        let distort = futils::hard_tanh(out_pre);

        let v3 = distort - self.ic2eq;
        let v1 = utils::mul_add(coefficient_0 * self.ic1eq, coefficient_1, v3);
        let v2 = utils::mul_add(
            utils::mul_add(self.ic2eq, coefficient_1, self.ic1eq),
            coefficient_2,
            v3,
        );
        self.ic1eq = v1 * 2.0 - self.ic1eq;
        self.ic2eq = v2 * 2.0 - self.ic2eq;

        futils::hard_tanh(utils::mul_add(
            utils::mul_add(blends2.v0 * distort, blends2.v1, v1),
            blends2.v2,
            v2,
        ))
    }
}

/// The smoothed per-block values passed into the `process*` loops
/// (`current_resonance`, `current_drive`, `current_post_multiply`).
#[derive(Clone, Copy, Debug)]
struct Current {
    resonance: PolyFloat,
    drive: PolyFloat,
    post_multiply: PolyFloat,
}

impl SynthFilter for DigitalSvf {
    fn setup_filter(&mut self, _this: &Processor, filter_state: &FilterState) {
        self.midi_cutoff = filter_state.midi_cutoff;

        let gain_decibels = utils::clamp(filter_state.gain, Self::MIN_GAIN, Self::MAX_GAIN);
        let gain_amplitude = utils::db_to_magnitude(gain_decibels);

        let resonance_percent = utils::clamp(filter_state.resonance_percent, 0.0, 1.0);
        let resonance_adjust = resonance_percent * resonance_percent * resonance_percent;
        let resonance =
            utils::interpolate(self.min_resonance, self.max_resonance, resonance_adjust);
        if self.drive_compensation {
            self.drive = filter_state.drive / (resonance_adjust * 2.0 + 1.0);
        } else {
            self.drive = filter_state.drive;
        }

        self.post_multiply = gain_amplitude / utils::sqrt(filter_state.drive);
        self.resonance = PolyFloat::splat(1.0) / resonance;

        let blend = utils::clamp(filter_state.pass_blend - 1.0, -1.0, 1.0);
        let one = PolyFloat::splat(1.0);

        if filter_state.style == style::DUAL_NOTCH_BAND {
            let t = blend * 0.5 + 0.5;
            let drive_t = PolyFloat::min(-blend + 1.0, one);
            let drive_mult = -t + 2.0;
            self.drive = utils::interpolate(filter_state.drive, self.drive * drive_mult, drive_t);

            self.low_amount = t;
            self.band_amount = PolyFloat::splat(0.0);
            self.high_amount = one;
        } else if filter_state.style == style::NOTCH_PASS_SWAP {
            let drive_t = PolyFloat::abs(blend);
            self.drive = utils::interpolate(filter_state.drive, self.drive, drive_t);

            self.low_amount = utils::min(-blend + 1.0, one);
            self.band_amount = PolyFloat::splat(0.0);
            self.high_amount = utils::min(blend + 1.0, one);
        } else if filter_state.style == style::BAND_PEAK_NOTCH {
            let drive_t = PolyFloat::min(-blend + 1.0, one);
            self.drive = utils::interpolate(filter_state.drive, self.drive, drive_t);

            let drive_inv_t = -drive_t + 1.0;
            let mult = utils::sqrt((drive_inv_t * drive_inv_t) * 0.5 + 0.5);
            let peak_band_value = -utils::max(-blend, PolyFloat::splat(0.0));
            self.low_amount = mult * (peak_band_value + 1.0);
            self.band_amount = mult * (peak_band_value - blend + 1.0) * 2.0;
            self.high_amount = self.low_amount;
        } else if filter_state.style == style::SHELVING {
            self.drive = one;
            self.post_multiply = one;
            let low_bell_t = utils::clamp(blend + 1.0, 0.0, 1.0);
            let bell_high_t = utils::clamp(blend, 0.0, 1.0);
            let band_t = one - blend * blend;

            let amplitude_sqrt = utils::sqrt(gain_amplitude);
            let amplitude_quartic = utils::sqrt(amplitude_sqrt);
            let mult_adjust = futils::pow(amplitude_quartic, blend);

            self.low_amount = utils::interpolate(gain_amplitude, 1.0, low_bell_t);
            self.high_amount = utils::interpolate(1.0, gain_amplitude, bell_high_t);
            self.band_amount =
                self.resonance * amplitude_sqrt * utils::interpolate(1.0, amplitude_sqrt, band_t);
            self.midi_cutoff += utils::ratio_to_midi_transpose(mult_adjust);
        } else {
            self.band_amount = utils::sqrt(-blend * blend + 1.0);
            let blend_mask = PolyFloat::less_than(blend, PolyFloat::splat(0.0));
            self.low_amount = (-blend) & blend_mask;
            self.high_amount = blend & !blend_mask;
        }

        self.blends1.v0 = PolyFloat::splat(0.0);
        self.blends1.v1 = self.band_amount;
        self.blends1.v2 = self.low_amount;

        self.blends2.v0 = PolyFloat::splat(0.0);
        self.blends2.v1 = self.band_amount;
        self.blends2.v2 = self.high_amount;

        self.blends1.v0 += self.high_amount;
        self.blends1.v1 += -self.resonance * self.high_amount;
        self.blends1.v2 += -self.high_amount;

        self.blends2.v0 += self.low_amount;
        self.blends2.v1 += -self.resonance * self.low_amount;
        self.blends2.v2 += -self.low_amount;
    }
}

impl ProcessorImpl for DigitalSvf {
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
