// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Distortion`: soft clip, hard clip, linear fold, sine fold, bit
//! crush and downsample, processed on voice 0 only.
//!
//! Ports `vital/src/synthesis/effects/distortion.h` and `distortion.cpp`.
//! Like upstream, the block is compacted (two frames of voice 0 per
//! `poly_float`), distorted, and expanded again with voice 1 a copy of
//! voice 0. The drive output keeps the compacted drive values.

use sas_dsp_core::constants;
use sas_dsp_core::output::Buffer;
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::sas_simd::poly_utils as utils;
use sas_dsp_core::sas_simd::{futils, PolyFloat};

/// `Distortion::Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistortionType {
    SoftClip = 0,
    HardClip = 1,
    LinearFold = 2,
    SinFold = 3,
    BitCrush = 4,
    DownSample = 5,
}

impl DistortionType {
    pub const NUM_TYPES: i32 = 6;

    fn from_index(value: i32) -> Option<DistortionType> {
        Some(match value {
            0 => DistortionType::SoftClip,
            1 => DistortionType::HardClip,
            2 => DistortionType::LinearFold,
            3 => DistortionType::SinFold,
            4 => DistortionType::BitCrush,
            5 => DistortionType::DownSample,
            _ => return None,
        })
    }
}

#[inline(always)]
fn linear_fold(value: PolyFloat, drive: PolyFloat) -> PolyFloat {
    let adjust = value * drive * 0.25 + 0.75;
    let range = utils::modulo(adjust);
    PolyFloat::abs(range * -4.0 + 2.0) - 1.0
}

#[inline(always)]
fn sin_fold(value: PolyFloat, drive: PolyFloat) -> PolyFloat {
    let adjust = value * drive * -0.25 + 0.5;
    let range = utils::modulo(adjust);
    futils::sin1(range)
}

#[inline(always)]
fn soft_clip(value: PolyFloat, drive: PolyFloat) -> PolyFloat {
    futils::tanh(value * drive)
}

#[inline(always)]
fn hard_clip(value: PolyFloat, drive: PolyFloat) -> PolyFloat {
    utils::clamp(value * drive, -1.0, 1.0)
}

#[inline(always)]
fn bit_crush(value: PolyFloat, drive: PolyFloat) -> PolyFloat {
    utils::round(value / drive) * drive
}

/// `compactAudio(audio_out, audio_in, num_samples)`; returns the number of
/// compacted frames.
fn compact_audio(audio_out: &Buffer, audio_in: &Buffer, num_samples: usize) -> usize {
    let num_full = num_samples / 2;
    for i in 0..num_full {
        let in_index = 2 * i;
        audio_out[i].set(utils::compact_first_voices(
            audio_in[in_index].get(),
            audio_in[in_index + 1].get(),
        ));
    }

    let num_remaining = num_samples % 2;
    if num_remaining != 0 {
        audio_out[num_full].set(audio_in[num_samples - 1].get());
    }
    num_full + num_remaining
}

/// `expandAudio(audio_out, audio_in, num_samples)`.
fn expand_audio(audio_out: &Buffer, audio_in: &Buffer, num_samples: usize) {
    let num_full = num_samples / 2;
    if !num_samples.is_multiple_of(2) {
        audio_out[num_samples - 1].set(audio_in[num_full].get());
    }

    for i in (0..num_full).rev() {
        let out_index = 2 * i;
        let value = audio_in[i].get();
        audio_out[out_index].set(value);
        audio_out[out_index + 1].set(utils::swap_voices(value));
    }
}

/// `vital::Distortion`.
#[derive(Clone)]
pub struct Distortion {
    last_distorted_value: PolyFloat,
    current_samples: PolyFloat,
    distortion_type: i32,
}

impl Default for Distortion {
    fn default() -> Self {
        Self::new()
    }
}

impl Distortion {
    pub const MAX_DRIVE: f32 = 30.0;
    pub const MIN_DRIVE: f32 = -30.0;
    pub const PERIOD_SCALE: f32 = 1.0 / 88200.0;
    pub const MIN_DISTORTION_MULT: f32 = 32.0 / i32::MAX as f32;

    pub const AUDIO: usize = 0;
    pub const TYPE: usize = 1;
    pub const DRIVE: usize = 2;
    pub const NUM_INPUTS: usize = 3;

    pub const AUDIO_OUT: usize = 0;
    pub const DRIVE_OUT: usize = 1;
    pub const NUM_OUTPUTS: usize = 2;

    /// The kernel of `Distortion()`.
    pub fn new() -> Self {
        Distortion {
            last_distorted_value: PolyFloat::zero(),
            current_samples: PolyFloat::zero(),
            distortion_type: DistortionType::NUM_TYPES,
        }
    }

    /// `new Distortion()`.
    pub fn create() -> ProcessorRef {
        Processor::new(Self::new(), Self::NUM_INPUTS, Self::NUM_OUTPUTS, false, 1)
    }

    /// `driveDbScale(db)`.
    #[inline(always)]
    pub fn drive_db_scale(db: PolyFloat) -> PolyFloat {
        futils::db_to_magnitude(utils::clamp(db, Self::MIN_DRIVE, Self::MAX_DRIVE))
    }

    /// `bitCrushScale(db)`.
    #[inline(always)]
    pub fn bit_crush_scale(db: PolyFloat) -> PolyFloat {
        const DRIVE_SCALE: f32 = 1.0 / (Distortion::MAX_DRIVE - Distortion::MIN_DRIVE);
        let drive = utils::max(db - Self::MIN_DRIVE, PolyFloat::zero()) * DRIVE_SCALE;
        utils::clamp(drive * drive, Self::MIN_DISTORTION_MULT, 1.0)
    }

    /// `downSampleScale(db)`.
    #[inline(always)]
    pub fn down_sample_scale(db: PolyFloat) -> PolyFloat {
        const DRIVE_SCALE: f32 = 1.0 / (Distortion::MAX_DRIVE - Distortion::MIN_DRIVE);
        let mut drive = utils::max(db - Self::MIN_DRIVE, PolyFloat::zero()) * DRIVE_SCALE;
        drive = -drive + 1.0;
        drive = PolyFloat::splat(1.0) / utils::clamp(drive * drive, Self::MIN_DISTORTION_MULT, 1.0);
        utils::max(drive * 0.99, PolyFloat::splat(1.0)) * Self::PERIOD_SCALE
    }

    /// `getDriveValue(type, input_drive)`.
    pub fn get_drive_value(distortion_type: i32, input_drive: PolyFloat) -> PolyFloat {
        match DistortionType::from_index(distortion_type) {
            Some(DistortionType::BitCrush) => Self::bit_crush_scale(input_drive),
            Some(DistortionType::DownSample) => Self::down_sample_scale(input_drive),
            _ => Self::drive_db_scale(input_drive),
        }
    }

    /// `getDrivenValue(type, value, drive)`.
    pub fn get_driven_value(distortion_type: i32, value: PolyFloat, drive: PolyFloat) -> PolyFloat {
        match DistortionType::from_index(distortion_type) {
            Some(DistortionType::SoftClip) => soft_clip(value, drive),
            Some(DistortionType::HardClip) => hard_clip(value, drive),
            Some(DistortionType::LinearFold) => linear_fold(value, drive),
            Some(DistortionType::SinFold) => sin_fold(value, drive),
            Some(DistortionType::BitCrush) => bit_crush(value, drive),
            Some(DistortionType::DownSample) => bit_crush(
                value,
                PolyFloat::splat(1.001) - PolyFloat::splat(Self::PERIOD_SCALE) / drive,
            ),
            None => value,
        }
    }

    /// `processTimeInvariant<distort, scale>`.
    fn process_time_invariant(
        num_samples: usize,
        audio_in: &Buffer,
        drive: &Buffer,
        audio_out: &Buffer,
        distort: fn(PolyFloat, PolyFloat) -> PolyFloat,
        scale: fn(PolyFloat) -> PolyFloat,
    ) {
        for i in 0..num_samples {
            let current_drive = scale(drive[i].get());
            let sample = audio_in[i].get();
            audio_out[i].set(distort(sample, current_drive));
        }
    }

    /// `processDownSample`.
    fn process_down_sample(
        &mut self,
        this: &Processor,
        num_samples: usize,
        audio_in: &Buffer,
        drive: &Buffer,
        audio_out: &Buffer,
    ) {
        let sample_rate = this.sample_rate() as f32;
        let mut current_samples = self.current_samples;

        for i in 0..num_samples {
            let current_period = Self::down_sample_scale(drive[i].get()) * sample_rate;
            current_samples += 1.0;

            let current_sample = audio_in[i].get();
            let mut current_downsample = current_sample & constants::first_mask();
            current_downsample += utils::swap_voices(current_downsample);

            let update = PolyFloat::greater_than_or_equal(current_samples, current_period);
            self.last_distorted_value =
                utils::mask_load(self.last_distorted_value, current_downsample, update);
            current_samples =
                utils::mask_load(current_samples, current_samples - current_period, update);
            audio_out[i].set(self.last_distorted_value);
        }

        self.current_samples = current_samples;
    }

    /// `processWithInput(audio_in, num_samples)`.
    pub fn process_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        let distortion_type = this.input(Self::TYPE).at(0).get(0) as i32;
        let audio_out = this.output_buffer(Self::AUDIO_OUT);
        let drive_out = this.output_buffer(Self::DRIVE_OUT);

        let compact_samples = compact_audio(&audio_out, audio_in, num_samples);
        compact_audio(&drive_out, &this.input_buffer(Self::DRIVE), num_samples);

        if distortion_type != self.distortion_type {
            self.distortion_type = distortion_type;
            self.last_distorted_value = PolyFloat::zero();
            self.current_samples = PolyFloat::zero();
        }

        let invariant = |distort, scale| {
            Self::process_time_invariant(
                compact_samples,
                &audio_out,
                &drive_out,
                &audio_out,
                distort,
                scale,
            )
        };
        match DistortionType::from_index(distortion_type) {
            Some(DistortionType::SoftClip) => invariant(soft_clip, Self::drive_db_scale),
            Some(DistortionType::HardClip) => invariant(hard_clip, Self::drive_db_scale),
            Some(DistortionType::LinearFold) => invariant(linear_fold, Self::drive_db_scale),
            Some(DistortionType::SinFold) => invariant(sin_fold, Self::drive_db_scale),
            Some(DistortionType::BitCrush) => invariant(bit_crush, Self::bit_crush_scale),
            Some(DistortionType::DownSample) => {
                self.process_down_sample(this, compact_samples, &audio_out, &drive_out, &audio_out)
            }
            None => {
                for i in 0..num_samples {
                    audio_out[i].set(audio_in[i].get());
                }
                return;
            }
        }

        expand_audio(&audio_out, &audio_out, num_samples);
    }
}

impl ProcessorImpl for Distortion {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        debug_assert!(this.input_matches_buffer_size(Self::AUDIO));
        let audio_in = this.input_buffer(Self::AUDIO);
        self.process_input(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        self.process_input(this, audio_in, num_samples);
    }
}
