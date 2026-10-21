// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Delay<MemoryType>`: a feedback delay with a band-pass (or
//! damping) filter in the loop, in mono, stereo and ping-pong styles.
//!
//! Ports `vital/src/synthesis/effects/delay.h` and `delay.cpp`.
//! [`StereoDelay`] is `Delay<StereoMemory>` and [`MultiDelay`] is
//! `Delay<Memory>`. Reads go through `Memory::get`, whose read index uses
//! `utils::toInt` (site A1a of `migration/decisions/reference-arm.md`).

use sas_dsp_core::output::Buffer;
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::sas_simd::common::{MIN_NYQUIST_MULT, NOTES_PER_OCTAVE, SQRT2};
use sas_dsp_core::sas_simd::poly_utils as utils;
use sas_dsp_core::sas_simd::{futils, PolyFloat};
use sas_dsp_core::{constants, Memory, OnePoleFilter, StereoMemory};

/// The delay-line storage a [`Delay`] reads and writes (`MemoryType`).
pub trait DelayMemory: Clone + 'static {
    fn create(size: i32) -> Self;
    fn get(&self, past: PolyFloat) -> PolyFloat;
    fn push(&mut self, sample: PolyFloat);
    fn clear_all(&mut self);
    fn max_period(&self) -> i32;
}

impl DelayMemory for StereoMemory {
    fn create(size: i32) -> Self {
        StereoMemory::new(size)
    }
    fn get(&self, past: PolyFloat) -> PolyFloat {
        StereoMemory::get(self, past)
    }
    fn push(&mut self, sample: PolyFloat) {
        StereoMemory::push(self, sample)
    }
    fn clear_all(&mut self) {
        StereoMemory::clear_all(self)
    }
    fn max_period(&self) -> i32 {
        StereoMemory::max_period(self)
    }
}

impl DelayMemory for Memory {
    fn create(size: i32) -> Self {
        Memory::new(size)
    }
    fn get(&self, past: PolyFloat) -> PolyFloat {
        Memory::get(self, past)
    }
    fn push(&mut self, sample: PolyFloat) {
        Memory::push(self, sample)
    }
    fn clear_all(&mut self) {
        Memory::clear_all(self)
    }
    fn max_period(&self) -> i32 {
        Memory::max_period(self)
    }
}

/// `Delay::Style`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Mono = 0,
    Stereo = 1,
    PingPong = 2,
    MidPingPong = 3,
    NumStyles = 4,
    ClampedDampened = 5,
    ClampedUnfiltered = 6,
    UnclampedUnfiltered = 7,
}

impl Style {
    /// `static_cast<Style>(value)`; values without an enumerator take the
    /// `default:` branch of the style switch, as `ClampedUnfiltered` does.
    fn from_index(value: i32) -> Option<Style> {
        Some(match value {
            0 => Style::Mono,
            1 => Style::Stereo,
            2 => Style::PingPong,
            3 => Style::MidPingPong,
            4 => Style::NumStyles,
            5 => Style::ClampedDampened,
            6 => Style::ClampedUnfiltered,
            7 => Style::UnclampedUnfiltered,
            _ => return None,
        })
    }
}

#[inline(always)]
fn saturate(value: PolyFloat) -> PolyFloat {
    futils::hard_tanh(value)
}

#[inline(always)]
fn saturate_large(value: PolyFloat) -> PolyFloat {
    const RATIO: f32 = 8.0;
    const MULT: f32 = 1.0 / RATIO;
    futils::hard_tanh(value * MULT) * RATIO
}

/// `vital::Delay<MemoryType>`.
#[derive(Clone)]
pub struct Delay<M: DelayMemory> {
    memory: M,
    last_frequency: PolyFloat,
    feedback: PolyFloat,
    wet: PolyFloat,
    dry: PolyFloat,
    period: PolyFloat,

    low_coefficient: PolyFloat,
    high_coefficient: PolyFloat,
    filter_gain: PolyFloat,

    low_pass: OnePoleFilter,
    high_pass: OnePoleFilter,
}

/// `vital::StereoDelay`.
pub type StereoDelay = Delay<StereoMemory>;
/// `vital::MultiDelay`.
pub type MultiDelay = Delay<Memory>;

/// Per-sample values ramped across a block.
#[derive(Clone, Copy)]
struct Ramp {
    period: PolyFloat,
    feedback: PolyFloat,
    filter_gain: PolyFloat,
    low_coefficient: PolyFloat,
    high_coefficient: PolyFloat,
    wet: PolyFloat,
    dry: PolyFloat,
}

impl<M: DelayMemory> Delay<M> {
    pub const SPREAD_OCTAVE_RANGE: f32 = 8.0;
    pub const DEFAULT_PERIOD: f32 = 100.0;
    pub const DELAY_HALF_LIFE: f32 = 0.02;
    pub const MIN_DAMP_NOTE: f32 = 60.0;
    pub const MAX_DAMP_NOTE: f32 = 136.0;

    pub const AUDIO: usize = 0;
    pub const WET: usize = 1;
    pub const FREQUENCY: usize = 2;
    pub const FREQUENCY_AUX: usize = 3;
    pub const FEEDBACK: usize = 4;
    pub const DAMPING: usize = 5;
    pub const STYLE: usize = 6;
    pub const FILTER_CUTOFF: usize = 7;
    pub const FILTER_SPREAD: usize = 8;
    pub const NUM_INPUTS: usize = 9;

    /// `getFilterRadius(spread)`.
    pub fn get_filter_radius(spread: PolyFloat) -> PolyFloat {
        utils::max(
            spread * Self::SPREAD_OCTAVE_RANGE * NOTES_PER_OCTAVE as f32,
            PolyFloat::zero(),
        )
    }

    /// The kernel of `Delay(size)`.
    pub fn new(size: i32) -> Self {
        let mut delay = Delay {
            memory: M::create(size),
            last_frequency: PolyFloat::splat(2.0),
            feedback: PolyFloat::zero(),
            wet: PolyFloat::zero(),
            dry: PolyFloat::zero(),
            period: utils::min(
                PolyFloat::splat(Self::DEFAULT_PERIOD),
                PolyFloat::splat((size - 1) as f32),
            ),
            low_coefficient: PolyFloat::zero(),
            high_coefficient: PolyFloat::zero(),
            filter_gain: PolyFloat::zero(),
            low_pass: OnePoleFilter::new(),
            high_pass: OnePoleFilter::new(),
        };
        delay.reset_state();
        delay
    }

    /// `new Delay<MemoryType>(size)`.
    pub fn create(size: i32) -> ProcessorRef {
        Processor::new(Self::new(size), Self::NUM_INPUTS, 1, false, 1)
    }

    /// `hardReset()`.
    fn reset_state(&mut self) {
        self.memory.clear_all();
        self.filter_gain = PolyFloat::zero();
        self.low_pass.reset(constants::full_mask());
        self.high_pass.reset(constants::full_mask());
    }

    /// `setMaxSamples(max_samples)`.
    pub fn set_max_samples(&mut self, max_samples: i32) {
        self.memory = M::create(max_samples);
        self.period = utils::min(self.period, PolyFloat::splat((max_samples - 1) as f32));
    }

    fn ramp_deltas(&self, current: &Ramp, tick_increment: f32) -> Ramp {
        Ramp {
            period: (self.period - current.period) * tick_increment,
            feedback: (self.feedback - current.feedback) * tick_increment,
            filter_gain: (self.filter_gain - current.filter_gain) * tick_increment,
            low_coefficient: (self.low_coefficient - current.low_coefficient) * tick_increment,
            high_coefficient: (self.high_coefficient - current.high_coefficient) * tick_increment,
            wet: (self.wet - current.wet) * tick_increment,
            dry: (self.dry - current.dry) * tick_increment,
        }
    }

    /// `processCleanUnfiltered` and `processUnfiltered`.
    fn process_unfiltered(
        &mut self,
        this: &Processor,
        audio_in: &Buffer,
        num_samples: usize,
        mut current: Ramp,
        clean: bool,
    ) {
        let delta = self.ramp_deltas(&current, 1.0 / num_samples as f32);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            current.feedback += delta.feedback;
            current.wet += delta.wet;
            current.dry += delta.dry;

            let value = if clean {
                self.tick_clean_unfiltered(audio_in[i].get(), &current)
            } else {
                self.tick_unfiltered(audio_in[i].get(), &current)
            };
            dest[i].set(value);
            current.period += delta.period;
        }
    }

    /// `process(audio_in, ...)`, `processPingPong` and
    /// `processMonoPingPong`: the filtered styles.
    fn process_filtered(
        &mut self,
        this: &Processor,
        audio_in: &Buffer,
        num_samples: usize,
        mut current: Ramp,
        tick: fn(&mut Self, PolyFloat, &Ramp) -> PolyFloat,
    ) {
        let delta = self.ramp_deltas(&current, 1.0 / num_samples as f32);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            current.feedback += delta.feedback;
            current.wet += delta.wet;
            current.dry += delta.dry;
            current.filter_gain += delta.filter_gain;
            current.low_coefficient += delta.low_coefficient;
            current.high_coefficient += delta.high_coefficient;

            dest[i].set(tick(self, audio_in[i].get(), &current));
            current.period += delta.period;
        }
    }

    /// `processDamped`.
    fn process_damped(
        &mut self,
        this: &Processor,
        audio_in: &Buffer,
        num_samples: usize,
        mut current: Ramp,
    ) {
        let delta = self.ramp_deltas(&current, 1.0 / num_samples as f32);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            current.feedback += delta.feedback;
            current.wet += delta.wet;
            current.dry += delta.dry;
            current.low_coefficient += delta.low_coefficient;

            dest[i].set(self.tick_damped(audio_in[i].get(), &current));
            current.period += delta.period;
        }
    }

    #[inline(always)]
    fn tick_clean_unfiltered(&mut self, audio_in: PolyFloat, r: &Ramp) -> PolyFloat {
        let read = self.memory.get(r.period);
        self.memory.push(audio_in + read * r.feedback);
        r.dry * audio_in + r.wet * read
    }

    #[inline(always)]
    fn tick_unfiltered(&mut self, audio_in: PolyFloat, r: &Ramp) -> PolyFloat {
        let read = self.memory.get(r.period);
        self.memory.push(saturate(audio_in + read * r.feedback));
        r.dry * audio_in + r.wet * read
    }

    #[inline(always)]
    fn tick(&mut self, audio_in: PolyFloat, r: &Ramp) -> PolyFloat {
        let read = self.memory.get(r.period);
        let write_raw_value = saturate_large(audio_in + read * r.feedback);
        let low_pass_result = self
            .low_pass
            .tick_basic(write_raw_value * r.filter_gain, r.low_coefficient);
        let second_pass_result = self
            .high_pass
            .tick_basic(low_pass_result, r.high_coefficient);
        self.memory.push(low_pass_result - second_pass_result);
        r.dry * audio_in + r.wet * read
    }

    #[inline(always)]
    fn tick_damped(&mut self, audio_in: PolyFloat, r: &Ramp) -> PolyFloat {
        let read = self.memory.get(r.period);
        let write_raw_value = saturate_large(audio_in + read * r.feedback);
        let low_pass_result = self.low_pass.tick_basic(write_raw_value, r.low_coefficient);
        self.memory.push(low_pass_result);
        r.dry * audio_in + r.wet * read
    }

    #[inline(always)]
    fn tick_ping_pong(&mut self, audio_in: PolyFloat, r: &Ramp) -> PolyFloat {
        let read = self.memory.get(r.period);
        let write_raw_value = utils::swap_stereo(saturate_large(audio_in + read * r.feedback));
        let low_pass_result = self
            .low_pass
            .tick_basic(write_raw_value * r.filter_gain, r.low_coefficient);
        let second_pass_result = self
            .high_pass
            .tick_basic(low_pass_result, r.high_coefficient);
        self.memory.push(low_pass_result - second_pass_result);
        r.dry * audio_in + r.wet * read
    }

    #[inline(always)]
    fn tick_mono_ping_pong(&mut self, audio_in: PolyFloat, r: &Ramp) -> PolyFloat {
        let read = self.memory.get(r.period);
        let mono_in =
            ((audio_in + utils::swap_stereo(audio_in)) * (1.0 / SQRT2)) & constants::left_mask();
        let write_raw_value = utils::swap_stereo(saturate_large(mono_in + read * r.feedback));
        let low_pass_result = self
            .low_pass
            .tick_basic(write_raw_value * r.filter_gain, r.low_coefficient);
        let second_pass_result = self
            .high_pass
            .tick_basic(low_pass_result, r.high_coefficient);
        self.memory.push(low_pass_result - second_pass_result);
        r.dry * audio_in + r.wet * read
    }

    /// `processWithInput(audio_in, num_samples)`.
    pub fn process_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        let mut current = Ramp {
            period: self.period,
            feedback: self.feedback,
            filter_gain: self.filter_gain,
            low_coefficient: self.low_coefficient,
            high_coefficient: self.high_coefficient,
            wet: self.wet,
            dry: self.dry,
        };

        let mut target_frequency = this.input(Self::FREQUENCY).at(0);

        let style = Style::from_index(this.input(Self::STYLE).at(0).get(0) as i32);
        if matches!(
            style,
            Some(Style::Stereo | Style::PingPong | Style::MidPingPong)
        ) {
            target_frequency = utils::mask_load(
                target_frequency,
                this.input(Self::FREQUENCY_AUX).at(0),
                constants::right_mask(),
            );
        }

        let sample_rate = this.sample_rate();
        let decay = futils::exp_half(PolyFloat::splat(
            num_samples as f32 / (Self::DELAY_HALF_LIFE * sample_rate as f32),
        ));
        self.last_frequency = utils::interpolate(target_frequency, self.last_frequency, decay);

        let wet = utils::clamp(this.input(Self::WET).at(0), 0.0, 1.0);
        self.wet = futils::equal_power_fade(wet);
        self.dry = futils::equal_power_fade_inverse(wet);
        self.feedback = utils::clamp(this.input(Self::FEEDBACK).at(0), -1.0, 1.0);

        let mut samples = PolyFloat::splat(sample_rate as f32) / self.last_frequency;
        if style == Some(Style::MidPingPong) {
            samples += utils::swap_stereo(samples) & constants::left_mask();
        }
        if style == Some(Style::PingPong) {
            current.feedback = utils::mask_load(current.feedback, 1.0, constants::right_mask());
            self.feedback = utils::mask_load(self.feedback, 1.0, constants::right_mask());
        }

        self.period = utils::clamp(samples, 3.0, self.memory.max_period() as f32);
        self.period = utils::interpolate(current.period, self.period, 0.5);

        let filter_cutoff = this.input(Self::FILTER_CUTOFF).at(0);
        let filter_radius = Self::get_filter_radius(this.input(Self::FILTER_SPREAD).at(0));

        let mut low_frequency = utils::midi_note_to_frequency(filter_cutoff + filter_radius);
        let min_nyquist = sample_rate as f32 * MIN_NYQUIST_MULT;
        low_frequency = utils::clamp(low_frequency, 1.0, min_nyquist);
        self.low_coefficient = OnePoleFilter::compute_coefficient(low_frequency, sample_rate);

        let mut high_frequency = utils::midi_note_to_frequency(filter_cutoff - filter_radius);
        high_frequency = utils::clamp(high_frequency, 1.0, min_nyquist);
        self.high_coefficient = OnePoleFilter::compute_coefficient(high_frequency, sample_rate);

        self.filter_gain = high_frequency / low_frequency + 1.0;
        let damping = utils::clamp(this.input(Self::DAMPING).at(0), 0.0, 1.0);
        let damping_note = utils::interpolate(Self::MIN_DAMP_NOTE, Self::MAX_DAMP_NOTE, damping);
        let mut damping_frequency = utils::midi_note_to_frequency(damping_note);

        match style {
            Some(Style::Mono | Style::Stereo) => {
                self.process_filtered(this, audio_in, num_samples, current, Self::tick)
            }
            Some(Style::PingPong) => self.process_filtered(
                this,
                audio_in,
                num_samples,
                current,
                Self::tick_mono_ping_pong,
            ),
            Some(Style::MidPingPong) => {
                self.process_filtered(this, audio_in, num_samples, current, Self::tick_ping_pong)
            }
            Some(Style::ClampedDampened) => {
                damping_frequency = utils::clamp(damping_frequency, 1.0, min_nyquist);
                self.low_coefficient =
                    OnePoleFilter::compute_coefficient(damping_frequency, sample_rate);
                self.process_damped(this, audio_in, num_samples, current);
            }
            Some(Style::UnclampedUnfiltered) => {
                self.process_unfiltered(this, audio_in, num_samples, current, true)
            }
            _ => self.process_unfiltered(this, audio_in, num_samples, current, false),
        }
    }
}

impl<M: DelayMemory> ProcessorImpl for Delay<M> {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        debug_assert!(this.input_matches_buffer_size(Self::AUDIO));
        let audio_in = this.input_buffer(Self::AUDIO);
        self.process_input(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        self.process_input(this, audio_in, num_samples);
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.reset_state();
    }
}

/// `delay->setMaxSamples(max_samples)` on a [`Delay`] node.
pub fn set_max_samples<M: DelayMemory>(delay: &Processor, max_samples: i32) {
    delay.with::<Delay<M>, _>(|delay, _| delay.set_max_samples(max_samples));
}
