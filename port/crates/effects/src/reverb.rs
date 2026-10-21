// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Reverb`: a 16-line feedback delay network with allpass diffusion,
//! shelving filters, chorus drift and a pre-delay.
//!
//! Ports `vital/src/synthesis/effects/reverb.h` and `reverb.cpp`. The
//! feedback matrix's `poly_float::sum()` is site A4 of
//! `migration/decisions/reference-arm.md` (horizontal sum order, in
//! `sas-simd`), and the pre-delay read is `StereoMemory::get` (A1a).

use sas_dsp_core::output::Buffer;
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::sas_simd::common::{MAX_SAMPLE_RATE, PI};
use sas_dsp_core::sas_simd::poly_utils as utils;
use sas_dsp_core::sas_simd::utils as mono;
use sas_dsp_core::sas_simd::{futils, PolyFloat, PolyInt};
use sas_dsp_core::{constants, OnePoleFilter, StereoMemory};

const MAX_CHORUS_DRIFT: f32 = 2500.0;
const MIN_DECAY_TIME: f32 = 0.1;
const MAX_DECAY_TIME: f32 = 100.0;
const MAX_CHORUS_FREQUENCY: f32 = 16.0;
const SAMPLE_DELAY_MULTIPLIER: f32 = 0.05;
const SAMPLE_INCREMENT_MULTIPLIER: f32 = 0.05;

const POLY_SIZE: usize = 4;

/// `vital::Reverb`.
#[derive(Clone)]
pub struct Reverb {
    memory: StereoMemory,

    /// `allpass_lookups_[n]`: `max_allpass_size_` poly_floats, stored as
    /// `4 * max_allpass_size_` floats (read lane by lane upstream).
    allpass_lookups: [Vec<f32>; Reverb::NETWORK_CONTAINERS],
    /// `feedback_memories_[n]`; `feedback_lookups_[n]` is this plus one.
    feedback_memories: [Vec<f32>; Reverb::NETWORK_SIZE],
    decays: [PolyFloat; Reverb::NETWORK_CONTAINERS],

    low_shelf_filters: [OnePoleFilter; Reverb::NETWORK_CONTAINERS],
    high_shelf_filters: [OnePoleFilter; Reverb::NETWORK_CONTAINERS],

    low_pre_filter: OnePoleFilter,
    high_pre_filter: OnePoleFilter,

    low_pre_coefficient: PolyFloat,
    high_pre_coefficient: PolyFloat,
    low_coefficient: PolyFloat,
    low_amplitude: PolyFloat,
    high_coefficient: PolyFloat,
    high_amplitude: PolyFloat,

    chorus_phase: f32,
    chorus_amount: PolyFloat,
    sample_delay: PolyFloat,
    sample_delay_increment: PolyFloat,
    dry: PolyFloat,
    wet: PolyFloat,
    write_index: i32,

    max_allpass_size: i32,
    max_feedback_size: i32,
    feedback_mask: i32,
    allpass_mask: PolyInt,
    poly_allpass_mask: i32,
}

impl Reverb {
    pub const T60_AMPLITUDE: f32 = 0.001;
    pub const ALLPASS_FEEDBACK: f32 = 0.6;
    pub const MIN_DELAY: f32 = 3.0;

    pub const BASE_SAMPLE_RATE: i32 = 44100;
    pub const DEFAULT_SAMPLE_RATE: i32 = 88200;
    pub const NETWORK_SIZE: usize = 16;
    pub const BASE_FEEDBACK_BITS: i32 = 14;
    pub const EXTRA_LOOKUP_SAMPLE: i32 = 4;
    pub const BASE_ALLPASS_BITS: i32 = 10;
    pub const NETWORK_CONTAINERS: usize = Self::NETWORK_SIZE / POLY_SIZE;
    pub const MIN_SIZE_POWER: i32 = -3;
    pub const MAX_SIZE_POWER: i32 = 1;
    pub const SIZE_POWER_RANGE: f32 = (Self::MAX_SIZE_POWER - Self::MIN_SIZE_POWER) as f32;

    /// `kAllpassDelays`.
    pub const ALLPASS_DELAYS: [[u32; 4]; Self::NETWORK_CONTAINERS] = [
        [1001, 799, 933, 876],
        [895, 807, 907, 853],
        [957, 1019, 711, 567],
        [833, 779, 663, 997],
    ];

    /// `kFeedbackDelays`.
    #[allow(clippy::excessive_precision)]
    pub const FEEDBACK_DELAYS: [[f32; 4]; Self::NETWORK_CONTAINERS] = [
        [6753.2, 9278.4, 7704.5, 11328.5],
        [9701.12, 5512.5, 8480.45, 5638.65],
        [3120.73, 3429.5, 3626.37, 7713.52],
        [4521.54, 6518.97, 5265.56, 5630.25],
    ];

    pub const AUDIO: usize = 0;
    pub const DECAY_TIME: usize = 1;
    pub const PRE_LOW_CUTOFF: usize = 2;
    pub const PRE_HIGH_CUTOFF: usize = 3;
    pub const LOW_CUTOFF: usize = 4;
    pub const LOW_GAIN: usize = 5;
    pub const HIGH_CUTOFF: usize = 6;
    pub const HIGH_GAIN: usize = 7;
    pub const CHORUS_AMOUNT: usize = 8;
    pub const CHORUS_FREQUENCY: usize = 9;
    pub const STEREO_WIDTH: usize = 10;
    pub const SIZE: usize = 11;
    pub const DELAY: usize = 12;
    pub const WET: usize = 13;
    pub const NUM_INPUTS: usize = 14;

    /// The kernel of `Reverb()`.
    pub fn new() -> Self {
        let mut reverb = Reverb {
            memory: StereoMemory::new(MAX_SAMPLE_RATE),
            allpass_lookups: Default::default(),
            feedback_memories: Default::default(),
            decays: [PolyFloat::zero(); Self::NETWORK_CONTAINERS],
            low_shelf_filters: [OnePoleFilter::new(); Self::NETWORK_CONTAINERS],
            high_shelf_filters: [OnePoleFilter::new(); Self::NETWORK_CONTAINERS],
            low_pre_filter: OnePoleFilter::new(),
            high_pre_filter: OnePoleFilter::new(),
            low_pre_coefficient: PolyFloat::splat(0.1),
            high_pre_coefficient: PolyFloat::splat(0.1),
            low_coefficient: PolyFloat::splat(0.1),
            low_amplitude: PolyFloat::zero(),
            high_coefficient: PolyFloat::splat(0.1),
            high_amplitude: PolyFloat::zero(),
            chorus_phase: 0.0,
            chorus_amount: PolyFloat::zero(),
            sample_delay: PolyFloat::splat(Self::MIN_DELAY),
            sample_delay_increment: PolyFloat::zero(),
            dry: PolyFloat::zero(),
            wet: PolyFloat::zero(),
            write_index: 0,
            max_allpass_size: 0,
            max_feedback_size: 0,
            feedback_mask: 0,
            allpass_mask: PolyInt::zero(),
            poly_allpass_mask: 0,
        };
        reverb.setup_buffers_for_sample_rate(Self::DEFAULT_SAMPLE_RATE);
        reverb
    }

    /// `new Reverb()`.
    pub fn create() -> ProcessorRef {
        Processor::new(Self::new(), Self::NUM_INPUTS, 1, false, 1)
    }

    /// `getSampleRateRatio(sample_rate)`.
    #[inline(always)]
    pub fn get_sample_rate_ratio(sample_rate: i32) -> f32 {
        sample_rate as f32 / (1.0 * Self::BASE_SAMPLE_RATE as f32)
    }

    /// `getBufferScale(sample_rate)`.
    #[inline(always)]
    pub fn get_buffer_scale(sample_rate: i32) -> i32 {
        let mut scale = 1;
        let ratio = Self::get_sample_rate_ratio(sample_rate);
        while (scale as f32) < ratio {
            scale *= 2;
        }
        scale
    }

    /// `setupBuffersForSampleRate(sample_rate)`.
    pub fn setup_buffers_for_sample_rate(&mut self, sample_rate: i32) {
        let buffer_scale = Self::get_buffer_scale(sample_rate);
        let max_feedback_size =
            buffer_scale * (1 << (Self::BASE_FEEDBACK_BITS + Self::MAX_SIZE_POWER));
        if self.max_feedback_size == max_feedback_size {
            return;
        }

        self.max_feedback_size = max_feedback_size;
        self.feedback_mask = self.max_feedback_size - 1;
        for memory in &mut self.feedback_memories {
            *memory = vec![0.0; (self.max_feedback_size + Self::EXTRA_LOOKUP_SAMPLE) as usize];
        }

        self.max_allpass_size = buffer_scale * (1 << Self::BASE_ALLPASS_BITS);
        self.poly_allpass_mask = self.max_allpass_size - 1;
        self.allpass_mask = PolyInt::splat((self.max_allpass_size * POLY_SIZE as i32 - 1) as u32);
        for lookup in &mut self.allpass_lookups {
            *lookup = vec![0.0; self.max_allpass_size as usize * POLY_SIZE];
        }

        self.write_index &= self.feedback_mask;
    }

    /// `readFeedback(lookups, offset)` on the four lines
    /// `feedback_lookups_[first..first + 4]`.
    #[inline(always)]
    fn read_feedback(&self, first: usize, offset: PolyFloat) -> PolyFloat {
        let write_offset = PolyFloat::splat(self.write_index as f32) - offset;
        let floored_offset = utils::floor(write_offset);
        let t = write_offset - floored_offset;
        let interpolation_matrix = utils::get_polynomial_interpolation_matrix(t);
        let indices = utils::to_int(floored_offset) & PolyInt::splat(self.feedback_mask as u32);
        let lookups: [&[f32]; 4] = std::array::from_fn(|i| &self.feedback_memories[first + i][1..]);
        let mut value_matrix = utils::get_value_matrix_separate(lookups, indices);
        value_matrix.transpose();
        interpolation_matrix.multiply_and_sum_rows(&value_matrix)
    }

    /// `readAllpass(lookup, offset)`.
    #[inline(always)]
    fn read_allpass(&self, container: usize, offset: PolyInt) -> PolyFloat {
        let indices = (PolyInt::splat((self.write_index * POLY_SIZE as i32) as u32) - offset)
            & self.allpass_mask;
        let lookup = &self.allpass_lookups[container];
        let i = indices.to_array();
        PolyFloat::new(
            lookup[i[0] as usize],
            lookup[i[1] as usize],
            lookup[i[2] as usize],
            lookup[i[3] as usize],
        )
    }

    /// `wrapFeedbackBuffer(buffer)`.
    #[inline(always)]
    fn wrap_feedback_buffer(buffer: &mut [f32], max_feedback_size: usize) {
        buffer[0] = buffer[max_feedback_size];
        buffer[max_feedback_size + 1] = buffer[1];
        buffer[max_feedback_size + 2] = buffer[2];
        buffer[max_feedback_size + 3] = buffer[3];
    }

    #[inline(always)]
    fn write_allpass(&mut self, container: usize, index: usize, value: PolyFloat) {
        let lanes = value.to_array();
        self.allpass_lookups[container][index * POLY_SIZE..index * POLY_SIZE + POLY_SIZE]
            .copy_from_slice(&lanes);
    }

    #[inline(always)]
    fn write_feedback(&mut self, first: usize, value: PolyFloat) {
        let index = self.write_index as usize + 1;
        for (lane, value) in value.to_array().into_iter().enumerate() {
            self.feedback_memories[first + lane][index] = value;
        }
    }

    /// `processWithInput(audio_in, num_samples)`.
    pub fn process_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        let max_feedback_size = self.max_feedback_size as usize;
        for memory in &mut self.feedback_memories {
            Self::wrap_feedback_buffer(memory, max_feedback_size);
        }

        let audio_out = this.output_buffer(0);
        let tick_increment = 1.0 / num_samples as f32;

        let mut current_dry = self.dry;
        let mut current_wet = self.wet;
        let current_low_pre_coefficient = self.low_pre_coefficient;
        let current_high_pre_coefficient = self.high_pre_coefficient;
        let current_low_coefficient = self.low_coefficient;
        let current_low_amplitude = self.low_amplitude;
        let mut current_high_coefficient = self.high_coefficient;
        let mut current_high_amplitude = self.high_amplitude;

        let wet_in = utils::clamp(this.input(Self::WET).at(0), 0.0, 1.0);
        self.wet = futils::equal_power_fade(wet_in);
        self.dry = futils::equal_power_fade_inverse(wet_in);
        let delta_wet = (self.wet - current_wet) * tick_increment;
        let delta_dry = (self.dry - current_dry) * tick_increment;

        let sample_rate = this.sample_rate();
        let buffer_scale = Self::get_buffer_scale(sample_rate);
        let sample_rate_ratio = Self::get_sample_rate_ratio(sample_rate);
        let low_pre_cutoff_midi = utils::clamp(this.input(Self::PRE_LOW_CUTOFF).at(0), 0.0, 130.0);
        let low_pre_cutoff_frequency = utils::midi_note_to_frequency(low_pre_cutoff_midi);
        self.low_pre_coefficient =
            OnePoleFilter::compute_coefficient(low_pre_cutoff_frequency, sample_rate);

        let high_pre_cutoff_midi =
            utils::clamp(this.input(Self::PRE_HIGH_CUTOFF).at(0), 0.0, 130.0);
        let high_pre_cutoff_frequency = utils::midi_note_to_frequency(high_pre_cutoff_midi);
        self.high_pre_coefficient =
            OnePoleFilter::compute_coefficient(high_pre_cutoff_frequency, sample_rate);

        let low_cutoff_midi = utils::clamp(this.input(Self::LOW_CUTOFF).at(0), 0.0, 130.0);
        let low_cutoff_frequency = utils::midi_note_to_frequency(low_cutoff_midi);
        self.low_coefficient =
            OnePoleFilter::compute_coefficient(low_cutoff_frequency, sample_rate);

        let high_cutoff_midi = utils::clamp(this.input(Self::HIGH_CUTOFF).at(0), 0.0, 130.0);
        let high_cutoff_frequency = utils::midi_note_to_frequency(high_cutoff_midi);
        self.high_coefficient =
            OnePoleFilter::compute_coefficient(high_cutoff_frequency, sample_rate);
        let delta_high_coefficient =
            (self.high_coefficient - current_high_coefficient) * tick_increment;

        let low_gain = utils::clamp(this.input(Self::LOW_GAIN).at(0), -24.0, 0.0);
        self.low_amplitude = PolyFloat::splat(1.0) - utils::db_to_magnitude(low_gain);
        let high_gain = utils::clamp(this.input(Self::HIGH_GAIN).at(0), -24.0, 0.0);
        self.high_amplitude = utils::db_to_magnitude(high_gain);
        let delta_high_amplitude = (self.high_amplitude - current_high_amplitude) * tick_increment;

        let size = utils::clamp(this.input(Self::SIZE).at(0), 0.0, 1.0);
        let size_mult = futils::pow(
            PolyFloat::splat(2.0),
            size * Self::SIZE_POWER_RANGE + Self::MIN_SIZE_POWER as f32,
        );

        let decay_samples = utils::clamp(
            this.input(Self::DECAY_TIME).at(0),
            MIN_DECAY_TIME,
            MAX_DECAY_TIME,
        ) * Self::BASE_SAMPLE_RATE as f32;
        let decay_period = size_mult / decay_samples;
        let mut current_decays = self.decays;
        for (decay, delays) in self.decays.iter_mut().zip(Self::FEEDBACK_DELAYS) {
            *decay = utils::pow(
                PolyFloat::splat(Self::T60_AMPLITUDE),
                PolyFloat::from_array(delays) * decay_period,
            );
        }
        let delta_decays: [PolyFloat; Self::NETWORK_CONTAINERS] =
            std::array::from_fn(|i| (self.decays[i] - current_decays[i]) * tick_increment);

        let mut delay_offset = PolyInt::new(0, (-1i32) as u32, (-2i32) as u32, (-3i32) as u32);
        if buffer_scale != 0 {
            delay_offset += POLY_SIZE as u32;
        }

        let allpass_offsets: [PolyInt; Self::NETWORK_CONTAINERS] = std::array::from_fn(|i| {
            utils::swap_stereo_int(
                PolyInt::from_array(Self::ALLPASS_DELAYS[i])
                    * (buffer_scale as u32)
                    * (POLY_SIZE as u32)
                    + delay_offset,
            )
        });

        let chorus_frequency = mono::clamp(
            this.input(Self::CHORUS_FREQUENCY).at(0).get(0),
            0.0,
            MAX_CHORUS_FREQUENCY,
        );
        let chorus_phase_increment = chorus_frequency / sample_rate as f32;

        let network_offset = 2.0 * PI / Self::NETWORK_SIZE as f32;
        let phase_offset = PolyFloat::new(0.0, 1.0, 2.0, 3.0) * network_offset;
        let container_phase = phase_offset + self.chorus_phase * 2.0 * PI;
        self.chorus_phase += num_samples as f32 * chorus_phase_increment;
        self.chorus_phase -= self.chorus_phase.floor();

        let chorus_increment_real =
            utils::cos(PolyFloat::splat(chorus_phase_increment * (2.0 * PI)));
        let chorus_increment_imaginary =
            utils::sin(PolyFloat::splat(chorus_phase_increment * (2.0 * PI)));
        let mut current_chorus_real = utils::cos(container_phase);
        let mut current_chorus_imaginary = utils::sin(container_phase);

        let delays: [PolyFloat; Self::NETWORK_CONTAINERS] = std::array::from_fn(|i| {
            size_mult * PolyFloat::from_array(Self::FEEDBACK_DELAYS[i]) * sample_rate_ratio
        });

        let mut current_chorus_amount = self.chorus_amount;
        self.chorus_amount = PolyFloat::splat(
            mono::clamp(this.input(Self::CHORUS_AMOUNT).at(0).get(0), 0.0, 1.0)
                * MAX_CHORUS_DRIFT
                * sample_rate_ratio,
        );
        for delay in &delays {
            self.chorus_amount = utils::min(self.chorus_amount, *delay - (8 * POLY_SIZE) as f32);
        }
        let delta_chorus_amount = (self.chorus_amount - current_chorus_amount) * tick_increment;
        current_chorus_amount *= size_mult;

        let mut current_sample_delay = self.sample_delay;
        let mut current_delay_increment = self.sample_delay_increment;
        let end_target = current_sample_delay + current_delay_increment * num_samples as f32;
        let mut target_delay = utils::clamp(
            this.input(Self::DELAY).at(0) * this.sample_rate() as f32,
            Self::MIN_DELAY,
            MAX_SAMPLE_RATE as f32,
        );
        target_delay = utils::interpolate(self.sample_delay, target_delay, SAMPLE_DELAY_MULTIPLIER);
        let makeup_delay = target_delay - end_target;
        let delta_delay_increment = makeup_delay / (0.5 * num_samples as f32 * num_samples as f32)
            * SAMPLE_INCREMENT_MULTIPLIER;

        let allpass_feedback = Self::ALLPASS_FEEDBACK;
        for i in 0..num_samples {
            current_chorus_amount += delta_chorus_amount;
            current_chorus_real = current_chorus_real * chorus_increment_real
                - current_chorus_imaginary * chorus_increment_imaginary;
            current_chorus_imaginary = current_chorus_imaginary * chorus_increment_real
                + current_chorus_real * chorus_increment_imaginary;
            let feedback_offset1 = delays[0] + current_chorus_real * current_chorus_amount;
            let feedback_offset2 = delays[1] - current_chorus_real * current_chorus_amount;
            let feedback_offset3 = delays[2] + current_chorus_imaginary * current_chorus_amount;
            let feedback_offset4 = delays[3] - current_chorus_imaginary * current_chorus_amount;
            let feedback_read1 = self.read_feedback(0, feedback_offset1);
            let feedback_read2 = self.read_feedback(4, feedback_offset2);
            let feedback_read3 = self.read_feedback(8, feedback_offset3);
            let feedback_read4 = self.read_feedback(12, feedback_offset4);

            let mut input = audio_in[i].get() & constants::first_mask();
            input += utils::swap_voices(input);
            let mut filtered_input = self
                .high_pre_filter
                .tick_basic(input, current_high_pre_coefficient);
            filtered_input = self
                .low_pre_filter
                .tick_basic(input, current_low_pre_coefficient)
                - filtered_input;
            let scaled_input = filtered_input * 0.25;

            let allpass_read1 = self.read_allpass(0, allpass_offsets[0]);
            let allpass_read2 = self.read_allpass(1, allpass_offsets[1]);
            let allpass_read3 = self.read_allpass(2, allpass_offsets[2]);
            let allpass_read4 = self.read_allpass(3, allpass_offsets[3]);

            let allpass_delay_input1 = feedback_read1 - allpass_read1 * allpass_feedback;
            let allpass_delay_input2 = feedback_read2 - allpass_read2 * allpass_feedback;
            let allpass_delay_input3 = feedback_read3 - allpass_read3 * allpass_feedback;
            let allpass_delay_input4 = feedback_read4 - allpass_read4 * allpass_feedback;

            let allpass_write_index = (self.write_index & self.poly_allpass_mask) as usize;
            self.write_allpass(0, allpass_write_index, scaled_input + allpass_delay_input1);
            self.write_allpass(1, allpass_write_index, scaled_input + allpass_delay_input2);
            self.write_allpass(2, allpass_write_index, scaled_input + allpass_delay_input3);
            self.write_allpass(3, allpass_write_index, scaled_input + allpass_delay_input4);

            let mut allpass_output1 = allpass_read1 + allpass_delay_input1 * allpass_feedback;
            let mut allpass_output2 = allpass_read2 + allpass_delay_input2 * allpass_feedback;
            let mut allpass_output3 = allpass_read3 + allpass_delay_input3 * allpass_feedback;
            let mut allpass_output4 = allpass_read4 + allpass_delay_input4 * allpass_feedback;

            let total_rows = allpass_output1 + allpass_output2 + allpass_output3 + allpass_output4;
            let other_feedback = PolyFloat::mul_add(
                PolyFloat::splat(total_rows.sum() * 0.25),
                total_rows,
                PolyFloat::splat(-0.5),
            );

            let mut write1 = other_feedback + allpass_output1;
            let mut write2 = other_feedback + allpass_output2;
            let mut write3 = other_feedback + allpass_output3;
            let mut write4 = other_feedback + allpass_output4;

            PolyFloat::transpose(
                &mut allpass_output1,
                &mut allpass_output2,
                &mut allpass_output3,
                &mut allpass_output4,
            );
            let adjacent_feedback =
                (allpass_output1 + allpass_output2 + allpass_output3 + allpass_output4) * -0.5;

            write1 += adjacent_feedback.get(0);
            write2 += adjacent_feedback.get(1);
            write3 += adjacent_feedback.get(2);
            write4 += adjacent_feedback.get(3);
            let high_filtered1 =
                self.high_shelf_filters[0].tick_basic(write1, current_high_coefficient);
            let high_filtered2 =
                self.high_shelf_filters[1].tick_basic(write2, current_high_coefficient);
            let high_filtered3 =
                self.high_shelf_filters[2].tick_basic(write3, current_high_coefficient);
            let high_filtered4 =
                self.high_shelf_filters[3].tick_basic(write4, current_high_coefficient);
            write1 = high_filtered1 + current_high_amplitude * (write1 - high_filtered1);
            write2 = high_filtered2 + current_high_amplitude * (write2 - high_filtered2);
            write3 = high_filtered3 + current_high_amplitude * (write3 - high_filtered3);
            write4 = high_filtered4 + current_high_amplitude * (write4 - high_filtered4);

            let low_filtered1 =
                self.low_shelf_filters[0].tick_basic(write1, current_low_coefficient);
            let low_filtered2 =
                self.low_shelf_filters[1].tick_basic(write2, current_low_coefficient);
            let low_filtered3 =
                self.low_shelf_filters[2].tick_basic(write3, current_low_coefficient);
            let low_filtered4 =
                self.low_shelf_filters[3].tick_basic(write4, current_low_coefficient);
            write1 -= low_filtered1 * current_low_amplitude;
            write2 -= low_filtered2 * current_low_amplitude;
            write3 -= low_filtered3 * current_low_amplitude;
            write4 -= low_filtered4 * current_low_amplitude;

            for (decay, delta) in current_decays.iter_mut().zip(&delta_decays) {
                *decay += *delta;
            }
            let mut store1 = current_decays[0] * write1;
            let mut store2 = current_decays[1] * write2;
            let mut store3 = current_decays[2] * write3;
            let mut store4 = current_decays[3] * write4;
            self.write_feedback(0, store1);
            self.write_feedback(4, store2);
            self.write_feedback(8, store3);
            self.write_feedback(12, store4);

            self.write_index = (self.write_index + 1) & self.feedback_mask;

            let total_allpass = store1 + store2 + store3 + store4;
            let other_feedback_allpass = PolyFloat::mul_add(
                PolyFloat::splat(total_allpass.sum() * 0.25),
                total_allpass,
                PolyFloat::splat(-0.5),
            );

            let mut feed_forward1 = other_feedback_allpass + store1;
            let mut feed_forward2 = other_feedback_allpass + store2;
            let mut feed_forward3 = other_feedback_allpass + store3;
            let mut feed_forward4 = other_feedback_allpass + store4;

            PolyFloat::transpose(&mut store1, &mut store2, &mut store3, &mut store4);
            let adjacent_feedback_allpass = (store1 + store2 + store3 + store4) * -0.5;

            feed_forward1 += adjacent_feedback_allpass.get(0);
            feed_forward2 += adjacent_feedback_allpass.get(1);
            feed_forward3 += adjacent_feedback_allpass.get(2);
            feed_forward4 += adjacent_feedback_allpass.get(3);

            let mut total = write1 + write2 + write3 + write4;
            total += (feed_forward1 * current_decays[0]
                + feed_forward2 * current_decays[1]
                + feed_forward3 * current_decays[2]
                + feed_forward4 * current_decays[3])
                * 0.125;

            self.memory.push(total + utils::swap_voices(total));
            audio_out[i]
                .set(current_wet * self.memory.get(current_sample_delay) + current_dry * input);

            current_delay_increment += delta_delay_increment;
            current_sample_delay += current_delay_increment;
            current_sample_delay = utils::clamp(
                current_sample_delay,
                Self::MIN_DELAY,
                MAX_SAMPLE_RATE as f32,
            );
            current_dry += delta_dry;
            current_wet += delta_wet;
            current_high_coefficient += delta_high_coefficient;
            current_high_amplitude += delta_high_amplitude;
        }

        self.sample_delay_increment = current_delay_increment;
        self.sample_delay = current_sample_delay;
    }
}

impl Default for Reverb {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessorImpl for Reverb {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        debug_assert!(this.input_matches_buffer_size(Self::AUDIO));
        let audio_in = this.input_buffer(Self::AUDIO);
        self.process_input(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        self.process_input(this, audio_in, num_samples);
    }

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.processor_set_sample_rate(sample_rate);
        self.setup_buffers_for_sample_rate(this.sample_rate());
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.processor_set_oversample_amount(oversample);
        self.setup_buffers_for_sample_rate(this.sample_rate());
    }

    fn hard_reset(&mut self, this: &Processor) {
        self.wet = PolyFloat::zero();
        self.dry = PolyFloat::zero();
        self.low_pre_filter.reset(constants::full_mask());
        self.high_pre_filter.reset(constants::full_mask());
        self.chorus_amount = PolyFloat::splat(
            mono::clamp(this.input(Self::CHORUS_AMOUNT).at(0).get(0), 0.0, 1.0) * MAX_CHORUS_DRIFT,
        );

        for i in 0..Self::NETWORK_CONTAINERS {
            self.low_shelf_filters[i].reset(constants::full_mask());
            self.high_shelf_filters[i].reset(constants::full_mask());
            self.decays[i] = PolyFloat::zero();
        }

        for lookup in &mut self.allpass_lookups {
            lookup.fill(0.0);
        }
        for memory in &mut self.feedback_memories {
            memory.fill(0.0);
        }
    }
}
