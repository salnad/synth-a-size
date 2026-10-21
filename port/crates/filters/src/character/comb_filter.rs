// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::CombFilter`: a feedback comb (or positive / negative flanger)
//! with a low/high-blend or band-spread filter pair in the loop.
//!
//! Ports `vital/src/synthesis/filters/comb_filter.h` and `comb_filter.cpp`.
//! The tick template parameter is a function pointer. The delay read
//! (`Memory::get`) goes through `utils::toInt` (site A1a of
//! `migration/decisions/reference-arm.md`), inside `sas-dsp-core`.
//!
//! `CombFilter(1)` (what upstream's `SynthFilter::createFilter` builds) has
//! a one-sample memory, and upstream's `Memory::get` then reads past the
//! buffer; here the same read panics, and `create_filter(Comb)` returns
//! `None`. `CombModule` uses `CombFilter(kMaxFeedbackSamples)`.

use sas_dsp_core::sas_simd::common::{MIN_NYQUIST_MULT, NOTES_PER_OCTAVE};
use sas_dsp_core::sas_simd::{futils, poly_utils as utils, utils as mono, PolyFloat, PolyMask};
use sas_dsp_core::{
    processor_boilerplate, Memory, OnePoleFilter, Processor, ProcessorImpl, ProcessorRef,
};

use crate::synth_filter::{input, FilterState, SynthFilter};

#[allow(clippy::excessive_precision, clippy::approx_constant)]
const FLANGE_SCALE: f32 = 0.707_106_781_19;

/// `CombFilter::FeedbackStyle`. A negative style gives a negative
/// remainder upstream, which matches no enumerator: `setupFilter` treats it
/// like a flanger and `process` writes nothing (`VITAL_ASSERT(false)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackStyle {
    Comb,
    PositiveFlange,
    NegativeFlange,
    Other(i32),
}

/// `CombFilter::FilterStyle`. Upstream casts `style / kNumFeedbackStyles`
/// to the enum without a range check; only `BandSpread` is tested for, so
/// every other value behaves as `LowHighBlend`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterStyle {
    LowHighBlend,
    BandSpread,
    Other(i32),
}

type Tick = fn(
    PolyFloat,
    &mut Memory,
    &mut OnePoleFilter,
    &mut OnePoleFilter,
    PolyFloat,
    PolyFloat,
    PolyFloat,
    PolyFloat,
    PolyFloat,
    PolyFloat,
    PolyFloat,
) -> PolyFloat;

#[inline(always)]
fn get_low_gain(blend: PolyFloat) -> PolyFloat {
    utils::clamp(-blend + 2.0, 0.0, 1.0)
}

#[inline(always)]
fn get_high_gain(blend: PolyFloat) -> PolyFloat {
    utils::clamp(blend, 0.0, 1.0)
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn tick_comb(
    audio_in: PolyFloat,
    memory: &mut Memory,
    filter1: &mut OnePoleFilter,
    filter2: &mut OnePoleFilter,
    period: PolyFloat,
    feedback: PolyFloat,
    scale: PolyFloat,
    filter_coefficient: PolyFloat,
    filter2_coefficient: PolyFloat,
    low_gain: PolyFloat,
    high_gain: PolyFloat,
) -> PolyFloat {
    let read = memory.get(period);
    let combine = utils::mul_add(scale * audio_in, read, feedback);

    let low_output = filter1.tick_basic(combine, filter_coefficient);
    let high_output = combine - low_output;
    let stage1_output = utils::mul_add(low_gain * low_output, high_gain, high_output);
    let stage2_output = filter2.tick_basic(stage1_output, filter2_coefficient);
    let result = stage1_output - stage2_output;
    memory.push(futils::hard_tanh(result));
    result
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn tick_positive_flange(
    audio_in: PolyFloat,
    memory: &mut Memory,
    filter1: &mut OnePoleFilter,
    filter2: &mut OnePoleFilter,
    period: PolyFloat,
    feedback: PolyFloat,
    scale: PolyFloat,
    filter_coefficient: PolyFloat,
    filter2_coefficient: PolyFloat,
    low_gain: PolyFloat,
    high_gain: PolyFloat,
) -> PolyFloat {
    let read = memory.get(period);
    let low_output = filter1.tick_basic(read, filter_coefficient);
    let high_output = read - low_output;
    let stage1_output = utils::mul_add(low_gain * low_output, high_gain, high_output);
    let stage2_output = filter2.tick_basic(stage1_output, filter2_coefficient);
    let filter_output = stage1_output - stage2_output;

    let scaled_input = audio_in * FLANGE_SCALE;
    memory.push(scaled_input + futils::hard_tanh(filter_output * feedback));
    scaled_input * scale + filter_output
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn tick_negative_flange(
    audio_in: PolyFloat,
    memory: &mut Memory,
    filter1: &mut OnePoleFilter,
    filter2: &mut OnePoleFilter,
    period: PolyFloat,
    feedback: PolyFloat,
    scale: PolyFloat,
    filter_coefficient: PolyFloat,
    filter2_coefficient: PolyFloat,
    low_gain: PolyFloat,
    high_gain: PolyFloat,
) -> PolyFloat {
    let read = memory.get(period * 0.5);
    let low_output = filter1.tick_basic(read, filter_coefficient);
    let high_output = read - low_output;
    let stage1_output = utils::mul_add(low_gain * low_output, high_gain, high_output);
    let stage2_output = filter2.tick_basic(stage1_output, filter2_coefficient);
    let filter_output = stage1_output - stage2_output;

    let scaled_input = audio_in * FLANGE_SCALE;
    memory.push(scaled_input - futils::hard_tanh(filter_output * feedback));
    scaled_input * scale - filter_output
}

/// `vital::CombFilter`.
pub struct CombFilter {
    filter_state: FilterState,
    memory: Memory,
    feedback_style: FeedbackStyle,
    max_period: PolyFloat,
    feedback: PolyFloat,
    filter_coefficient: PolyFloat,
    filter2_coefficient: PolyFloat,
    low_gain: PolyFloat,
    high_gain: PolyFloat,
    scale: PolyFloat,
    filter_midi_cutoff: PolyFloat,
    filter2_midi_cutoff: PolyFloat,
    feedback_filter: OnePoleFilter,
    feedback_filter2: OnePoleFilter,
}

impl Clone for CombFilter {
    /// Upstream's user-defined copy constructor: copies the filter state
    /// and feedback style, zeroed memory (`Memory`'s copy constructor), the
    /// settings it assigns back to their initial values, `feedback_filter_`
    /// reset. Members it does not mention are default-initialized:
    /// `low_gain_`, `high_gain_` (0) and `feedback_filter2_` (a reset
    /// `OnePoleFilter`).
    fn clone(&self) -> Self {
        let mut feedback_filter = self.feedback_filter;
        feedback_filter.reset(PolyMask::full());
        CombFilter {
            filter_state: self.filter_state.clone(),
            memory: self.memory.clone(),
            feedback_style: self.feedback_style,
            max_period: PolyFloat::splat(Memory::MIN_PERIOD),
            feedback: PolyFloat::zero(),
            filter_coefficient: PolyFloat::zero(),
            filter2_coefficient: PolyFloat::zero(),
            low_gain: PolyFloat::zero(),
            high_gain: PolyFloat::zero(),
            scale: PolyFloat::zero(),
            filter_midi_cutoff: PolyFloat::zero(),
            filter2_midi_cutoff: PolyFloat::zero(),
            feedback_filter,
            feedback_filter2: OnePoleFilter::new(),
        }
    }
}

impl CombFilter {
    pub const NUM_FEEDBACK_STYLES: i32 = 3;
    pub const NUM_FILTER_STYLES: i32 = 2;
    pub const NUM_FILTER_TYPES: i32 = Self::NUM_FILTER_STYLES * Self::NUM_FEEDBACK_STYLES;
    pub const BAND_OCTAVE_RANGE: f32 = 8.0;
    pub const BAND_OCTAVE_MIN: f32 = 0.0;
    pub const MIN_PERIOD: i32 = 2;
    pub const INPUT_SCALE: f32 = 0.5;
    pub const MAX_FEEDBACK: f32 = 1.0;

    /// `getFeedbackStyle(style)`: `style % kNumFeedbackStyles`.
    pub fn get_feedback_style(style: i32) -> FeedbackStyle {
        match style % Self::NUM_FEEDBACK_STYLES {
            0 => FeedbackStyle::Comb,
            1 => FeedbackStyle::PositiveFlange,
            2 => FeedbackStyle::NegativeFlange,
            other => FeedbackStyle::Other(other),
        }
    }

    /// `getFilterStyle(style)`: `style / kNumFeedbackStyles`.
    pub fn get_filter_style(style: i32) -> FilterStyle {
        match style / Self::NUM_FEEDBACK_STYLES {
            0 => FilterStyle::LowHighBlend,
            1 => FilterStyle::BandSpread,
            other => FilterStyle::Other(other),
        }
    }

    /// The kernel of `CombFilter(size)`.
    pub fn new(size: i32) -> CombFilter {
        CombFilter {
            filter_state: FilterState::default(),
            memory: Memory::new(size),
            feedback_style: FeedbackStyle::Comb,
            max_period: PolyFloat::splat(Memory::MIN_PERIOD),
            feedback: PolyFloat::zero(),
            filter_coefficient: PolyFloat::zero(),
            filter2_coefficient: PolyFloat::zero(),
            low_gain: PolyFloat::zero(),
            high_gain: PolyFloat::zero(),
            scale: PolyFloat::zero(),
            filter_midi_cutoff: PolyFloat::zero(),
            filter2_midi_cutoff: PolyFloat::zero(),
            feedback_filter: OnePoleFilter::new(),
            feedback_filter2: OnePoleFilter::new(),
        }
    }

    /// `new CombFilter(size)`: `Processor(kNumInputs, 1)`.
    pub fn create(size: i32) -> ProcessorRef {
        Processor::new(CombFilter::new(size), input::NUM_INPUTS, 1, false, 1)
    }

    pub fn drive(&self) -> PolyFloat {
        self.scale
    }

    pub fn resonance(&self) -> PolyFloat {
        self.feedback
    }

    pub fn low_amount(&self) -> PolyFloat {
        self.low_gain
    }

    pub fn high_amount(&self) -> PolyFloat {
        self.high_gain
    }

    pub fn filter_midi_cutoff(&self) -> PolyFloat {
        self.filter_midi_cutoff
    }

    pub fn filter2_midi_cutoff(&self) -> PolyFloat {
        self.filter2_midi_cutoff
    }

    fn reset_state(&mut self, reset_mask: PolyMask) {
        let lanes = self.max_period.to_array();
        let mut max_period = lanes[0];
        for &lane in &lanes[1..] {
            max_period = mono::max(max_period, lane);
        }

        let clear_samples = (self.memory.size() - 1).min(max_period as i32 + 1);
        self.memory.clear_memory(clear_samples, reset_mask);

        self.scale = utils::mask_load(self.scale, 0.0, reset_mask);
        self.low_gain = utils::mask_load(self.low_gain, 0.0, reset_mask);
        self.high_gain = utils::mask_load(self.high_gain, 0.0, reset_mask);

        self.feedback_filter.reset(reset_mask);
        self.feedback_filter2.reset(reset_mask);
    }

    fn process_filter(&mut self, this: &Processor, num_samples: usize, tick: Tick) {
        let mut current_feedback = self.feedback;
        let mut current_filter_coefficient = self.filter_coefficient;
        let mut current_filter2_coefficient = self.filter2_coefficient;
        let mut current_scale = self.scale;
        let mut current_low_gain = self.low_gain;
        let mut current_high_gain = self.high_gain;

        let filter_state = self.filter_state.clone();
        self.setup_filter(this, &filter_state);

        let sample_rate_int = this.sample_rate();
        let min_midi_cutoff = utils::min(
            filter_state.midi_cutoff_at(0),
            filter_state.midi_cutoff_at(num_samples - 1),
        );
        let min_frequency = utils::midi_note_to_frequency(min_midi_cutoff);
        let min_nyquist = sample_rate_int as f32 * MIN_NYQUIST_MULT;
        self.max_period = PolyFloat::splat(sample_rate_int as f32)
            / utils::clamp(min_frequency, 1.0, min_nyquist);
        let mut min_period = PolyFloat::splat(Memory::MIN_PERIOD);
        if self.feedback_style == FeedbackStyle::NegativeFlange {
            min_period *= 2.0;
        }
        self.max_period = utils::clamp(
            self.max_period,
            min_period,
            self.memory.max_period() as f32 - 5.0,
        );

        let reset_mask = this.get_reset_mask(input::RESET);
        if reset_mask.any_mask() != 0 {
            self.reset_state(reset_mask);

            current_feedback = utils::mask_load(current_feedback, self.feedback, reset_mask);
            current_filter_coefficient = utils::mask_load(
                current_filter_coefficient,
                self.filter_coefficient,
                reset_mask,
            );
            current_filter2_coefficient = utils::mask_load(
                current_filter2_coefficient,
                self.filter2_coefficient,
                reset_mask,
            );
            current_scale = utils::mask_load(current_scale, self.scale, reset_mask);
            current_low_gain = utils::mask_load(current_low_gain, self.low_gain, reset_mask);
            current_high_gain = utils::mask_load(current_high_gain, self.high_gain, reset_mask);
        }

        let audio_in = this.input_buffer(input::AUDIO);
        let audio_out = this.output_buffer(0);

        let tick_increment = 1.0f32 / num_samples as f32;
        let delta_feedback = (self.feedback - current_feedback) * tick_increment;
        let delta_coefficient =
            (self.filter_coefficient - current_filter_coefficient) * tick_increment;
        let delta_scale = (self.scale - current_scale) * tick_increment;
        let delta_coefficient2 =
            (self.filter2_coefficient - current_filter2_coefficient) * tick_increment;
        let delta_low_gain = (self.low_gain - current_low_gain) * tick_increment;
        let delta_high_gain = (self.high_gain - current_high_gain) * tick_increment;

        let base_midi = filter_state.midi_cutoff_at(num_samples - 1);
        let base_frequency = utils::midi_note_to_frequency(base_midi);

        let sample_rate = PolyFloat::splat(sample_rate_int as f32);
        let max_period = PolyFloat::splat(self.memory.max_period() as f32 - 5.0);
        for i in 0..num_samples {
            let midi_offset = filter_state.midi_cutoff_at(i) - base_midi;
            let frequency = base_frequency * futils::midi_offset_to_ratio(midi_offset);
            let mut period = sample_rate / frequency;
            period = utils::clamp(period, min_period, max_period);

            current_feedback += delta_feedback;
            current_filter_coefficient += delta_coefficient;
            current_filter2_coefficient += delta_coefficient2;
            current_scale += delta_scale;
            current_low_gain += delta_low_gain;
            current_high_gain += delta_high_gain;

            audio_out[i].set(tick(
                audio_in[i].get(),
                &mut self.memory,
                &mut self.feedback_filter,
                &mut self.feedback_filter2,
                period,
                current_feedback,
                current_scale,
                current_filter_coefficient,
                current_filter2_coefficient,
                current_low_gain,
                current_high_gain,
            ));
        }
    }
}

impl SynthFilter for CombFilter {
    fn setup_filter(&mut self, this: &Processor, filter_state: &FilterState) {
        let sample_rate = this.sample_rate();
        self.feedback_style = Self::get_feedback_style(filter_state.style);
        let resonance = utils::clamp(filter_state.resonance_percent, 0.0, 1.0);

        if self.feedback_style == FeedbackStyle::Comb {
            self.feedback = utils::interpolate(-Self::MAX_FEEDBACK, Self::MAX_FEEDBACK, resonance);
            self.feedback /= utils::sqrt(PolyFloat::abs(self.feedback) + 0.00001);
            self.scale = -self.feedback * self.feedback * Self::INPUT_SCALE + 1.0;
        } else {
            self.feedback = utils::interpolate(0.0, Self::MAX_FEEDBACK, resonance);
            self.scale = PolyFloat::splat(1.0) / (self.feedback + 1.0);
        }

        let midi_cutoff = filter_state.midi_cutoff;
        let min_nyquist = sample_rate as f32 * MIN_NYQUIST_MULT;

        let blend = filter_state.pass_blend;
        let min_cutoff = midi_cutoff - (4 * NOTES_PER_OCTAVE) as f32;

        let filter_style = Self::get_filter_style(filter_state.style);
        if filter_style == FilterStyle::BandSpread {
            let midi_blend_transpose = filter_state.transpose;
            let center_midi_cutoff = midi_cutoff + midi_blend_transpose;
            let midi_band_range = (blend * 0.5 * Self::BAND_OCTAVE_RANGE + Self::BAND_OCTAVE_MIN)
                * NOTES_PER_OCTAVE as f32;

            self.filter_midi_cutoff = center_midi_cutoff + midi_band_range;
            self.filter2_midi_cutoff = utils::max(min_cutoff, center_midi_cutoff - midi_band_range);
            let mut filter1_cutoff = utils::midi_note_to_frequency(self.filter_midi_cutoff);
            let mut filter2_cutoff = utils::midi_note_to_frequency(self.filter2_midi_cutoff);

            filter1_cutoff = utils::clamp(filter1_cutoff, 1.0, sample_rate as f32 / 2.1);
            filter2_cutoff = utils::clamp(filter2_cutoff, 1.0, sample_rate as f32 / 2.1);
            self.filter_midi_cutoff = utils::frequency_to_midi_note(filter1_cutoff);
            self.filter2_midi_cutoff = utils::frequency_to_midi_note(filter2_cutoff);
            self.low_gain = filter2_cutoff / filter1_cutoff + 1.0;
            self.high_gain = PolyFloat::zero();

            self.filter_coefficient =
                OnePoleFilter::compute_coefficient(filter1_cutoff, sample_rate);
            self.filter2_coefficient =
                OnePoleFilter::compute_coefficient(filter2_cutoff, sample_rate);
        } else {
            self.low_gain = get_low_gain(blend);
            self.high_gain = get_high_gain(blend);

            let midi_blend_transpose = filter_state.transpose;
            self.filter_midi_cutoff = midi_cutoff + midi_blend_transpose;
            self.filter2_midi_cutoff = min_cutoff;

            let mut filter_cutoff = utils::midi_note_to_frequency(self.filter_midi_cutoff);
            let mut filter2_cutoff = utils::midi_note_to_frequency(self.filter2_midi_cutoff);
            filter_cutoff = utils::clamp(filter_cutoff, 1.0, min_nyquist);
            filter2_cutoff = utils::clamp(filter2_cutoff, 1.0, min_nyquist);

            self.filter_coefficient =
                OnePoleFilter::compute_coefficient(filter_cutoff, sample_rate);
            self.filter2_coefficient =
                OnePoleFilter::compute_coefficient(filter2_cutoff, sample_rate);
        }
    }
}

impl ProcessorImpl for CombFilter {
    processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        self.filter_state.load_settings(this);
        let style = Self::get_feedback_style(self.filter_state.style);

        match style {
            FeedbackStyle::Comb => self.process_filter(this, num_samples, tick_comb),
            FeedbackStyle::PositiveFlange => {
                self.process_filter(this, num_samples, tick_positive_flange)
            }
            FeedbackStyle::NegativeFlange => {
                self.process_filter(this, num_samples, tick_negative_flange)
            }
            FeedbackStyle::Other(_) => {}
        }
    }

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.reset_state(reset_mask);
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.reset_state(PolyMask::full());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_default_initializes_what_the_copy_constructor_does_not_mention() {
        let mut filter = CombFilter::new(64);
        filter.low_gain = PolyFloat::splat(0.7);
        filter.high_gain = PolyFloat::splat(0.3);
        filter.scale = PolyFloat::splat(0.5);
        filter.feedback_style = FeedbackStyle::NegativeFlange;
        filter
            .feedback_filter2
            .tick_basic(PolyFloat::splat(1.0), PolyFloat::splat(0.5));
        filter
            .feedback_filter
            .tick_basic(PolyFloat::splat(1.0), PolyFloat::splat(0.5));

        let copy = filter.clone();
        let zero = PolyFloat::zero().to_array();
        assert_eq!(copy.low_gain.to_array(), zero);
        assert_eq!(copy.high_gain.to_array(), zero);
        assert_eq!(copy.scale.to_array(), zero);
        assert_eq!(copy.feedback_filter2.next_state().to_array(), zero);
        assert_eq!(copy.feedback_filter.next_state().to_array(), zero);
        assert_eq!(copy.feedback_style, FeedbackStyle::NegativeFlange);
        assert_eq!(copy.max_period.to_array(), [Memory::MIN_PERIOD; 4]);
    }
}
