// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Compressor` (an upward/downward RMS compressor on two bands at
//! once, one per voice) and `vital::MultibandCompressor` (three bands split
//! by 120 Hz and 2500 Hz [`LinkwitzRileyFilter`]s).
//!
//! Ports `vital/src/synthesis/effects/compressor.h` and `compressor.cpp`.
//! As upstream, the multiband compressor owns its filters and compressors
//! (members, not graph children) and forwards sample rate, oversampling and
//! resets to them itself.

use std::rc::Rc;

use sas_dsp_core::constants;
use sas_dsp_core::output::{Buffer, Output};
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::sas_simd::common::MS_PER_SEC;
use sas_dsp_core::sas_simd::poly_utils as utils;
use sas_dsp_core::sas_simd::{futils, PolyFloat, PolyMask};
use sas_filters::linkwitz_riley_filter::{self as lr, LinkwitzRileyFilter};

const RMS_TIME: f32 = 0.025;
const MAX_EXPAND_MULT: f32 = 32.0;
const LOW_ATTACK_MS: f32 = 2.8;
const BAND_ATTACK_MS: f32 = 1.4;
const HIGH_ATTACK_MS: f32 = 0.7;
const LOW_RELEASE_MS: f32 = 40.0;
const BAND_RELEASE_MS: f32 = 28.0;
const HIGH_RELEASE_MS: f32 = 15.0;

const MIN_GAIN: f32 = -30.0;
const MAX_GAIN: f32 = 30.0;
const MIN_THRESHOLD: f32 = -100.0;
const MAX_THRESHOLD: f32 = 12.0;
const MIN_SAMPLE_ENVELOPE: f32 = 5.0;

/// `vital::Compressor`.
#[derive(Clone)]
pub struct Compressor {
    input_mean_squared: PolyFloat,
    output_mean_squared: PolyFloat,
    high_enveloped_mean_squared: PolyFloat,
    low_enveloped_mean_squared: PolyFloat,

    mix: PolyFloat,

    base_attack_ms: PolyFloat,
    base_release_ms: PolyFloat,

    output_mult: PolyFloat,
}

impl Compressor {
    pub const AUDIO: usize = 0;
    pub const UPPER_THRESHOLD: usize = 1;
    pub const LOWER_THRESHOLD: usize = 2;
    pub const UPPER_RATIO: usize = 3;
    pub const LOWER_RATIO: usize = 4;
    pub const OUTPUT_GAIN: usize = 5;
    pub const ATTACK: usize = 6;
    pub const RELEASE: usize = 7;
    pub const MIX: usize = 8;
    pub const NUM_INPUTS: usize = 9;

    pub const AUDIO_OUT: usize = 0;
    pub const NUM_OUTPUTS: usize = 1;

    /// The kernel of `Compressor(attack_first, release_first, attack_second, release_second)`:
    /// voice 0 uses the first times, voice 1 the second.
    pub fn new(
        base_attack_ms_first: f32,
        base_release_ms_first: f32,
        base_attack_ms_second: f32,
        base_release_ms_second: f32,
    ) -> Self {
        Compressor {
            input_mean_squared: PolyFloat::zero(),
            output_mean_squared: PolyFloat::zero(),
            high_enveloped_mean_squared: PolyFloat::zero(),
            low_enveloped_mean_squared: PolyFloat::zero(),
            mix: PolyFloat::zero(),
            base_attack_ms: utils::mask_load(
                PolyFloat::splat(base_attack_ms_second),
                base_attack_ms_first,
                constants::first_mask(),
            ),
            base_release_ms: utils::mask_load(
                PolyFloat::splat(base_release_ms_second),
                base_release_ms_first,
                constants::first_mask(),
            ),
            output_mult: PolyFloat::zero(),
        }
    }

    /// `new Compressor(...)`.
    pub fn create(
        base_attack_ms_first: f32,
        base_release_ms_first: f32,
        base_attack_ms_second: f32,
        base_release_ms_second: f32,
    ) -> ProcessorRef {
        Processor::new(
            Self::new(
                base_attack_ms_first,
                base_release_ms_first,
                base_attack_ms_second,
                base_release_ms_second,
            ),
            Self::NUM_INPUTS,
            Self::NUM_OUTPUTS,
            false,
            1,
        )
    }

    /// `getInputMeanSquared()`.
    pub fn input_mean_squared(&self) -> PolyFloat {
        self.input_mean_squared
    }

    /// `getOutputMeanSquared()`.
    pub fn output_mean_squared(&self) -> PolyFloat {
        self.output_mean_squared
    }

    /// `processWithInput(audio_in, num_samples)`.
    pub fn process_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        self.process_rms(this, audio_in, num_samples);

        self.input_mean_squared =
            Self::compute_mean_squared(this, audio_in, num_samples, self.input_mean_squared);
        self.output_mean_squared = Self::compute_mean_squared(
            this,
            &this.output_buffer(Self::AUDIO_OUT),
            num_samples,
            self.output_mean_squared,
        );
        self.scale_output(this, audio_in, num_samples);
    }

    /// `processRms(audio_in, num_samples)`.
    fn process_rms(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        let audio_out = this.output_buffer(Self::AUDIO_OUT);

        let samples_per_ms = (1.0 * this.sample_rate() as f32) / MS_PER_SEC as f32;
        let attack_mult = self.base_attack_ms * samples_per_ms;
        let release_mult = self.base_release_ms * samples_per_ms;
        let attack_exponent = utils::clamp(this.input(Self::ATTACK).at(0), 0.0, 1.0) * 8.0 - 4.0;
        let release_exponent = utils::clamp(this.input(Self::RELEASE).at(0), 0.0, 1.0) * 8.0 - 4.0;
        let mut envelope_attack_samples = futils::exp(attack_exponent) * attack_mult;
        let mut envelope_release_samples = futils::exp(release_exponent) * release_mult;
        envelope_attack_samples = utils::max(
            envelope_attack_samples,
            PolyFloat::splat(MIN_SAMPLE_ENVELOPE),
        );
        envelope_release_samples = utils::max(
            envelope_release_samples,
            PolyFloat::splat(MIN_SAMPLE_ENVELOPE),
        );

        let attack_scale = PolyFloat::splat(1.0) / (envelope_attack_samples + 1.0);
        let release_scale = PolyFloat::splat(1.0) / (envelope_release_samples + 1.0);

        let mut upper_threshold = utils::clamp(
            this.input(Self::UPPER_THRESHOLD).at(0),
            MIN_THRESHOLD,
            MAX_THRESHOLD,
        );
        upper_threshold = futils::db_to_magnitude(upper_threshold);
        upper_threshold *= upper_threshold;
        let mut lower_threshold = utils::clamp(
            this.input(Self::LOWER_THRESHOLD).at(0),
            MIN_THRESHOLD,
            MAX_THRESHOLD,
        );
        lower_threshold = futils::db_to_magnitude(lower_threshold);
        lower_threshold *= lower_threshold;

        let upper_ratio = utils::clamp(this.input(Self::UPPER_RATIO).at(0), 0.0, 1.0) * 0.5;
        let lower_ratio = utils::clamp(this.input(Self::LOWER_RATIO).at(0), -1.0, 1.0) * 0.5;

        let mut low_enveloped_mean_squared = self.low_enveloped_mean_squared;
        let mut high_enveloped_mean_squared = self.high_enveloped_mean_squared;

        for i in 0..num_samples {
            let sample = audio_in[i].get();
            let sample_squared = sample * sample;

            let high_attack_mask =
                PolyFloat::greater_than(sample_squared, high_enveloped_mean_squared);
            let high_samples = utils::mask_load(
                envelope_release_samples,
                envelope_attack_samples,
                high_attack_mask,
            );
            let high_scale = utils::mask_load(release_scale, attack_scale, high_attack_mask);

            high_enveloped_mean_squared =
                (sample_squared + high_enveloped_mean_squared * high_samples) * high_scale;
            high_enveloped_mean_squared = utils::max(high_enveloped_mean_squared, upper_threshold);

            let upper_mag_delta = upper_threshold / high_enveloped_mean_squared;
            let upper_mult = futils::pow(upper_mag_delta, upper_ratio);

            let low_attack_mask =
                PolyFloat::greater_than(sample_squared, low_enveloped_mean_squared);
            let low_samples = utils::mask_load(
                envelope_release_samples,
                envelope_attack_samples,
                low_attack_mask,
            );
            let low_scale = utils::mask_load(release_scale, attack_scale, low_attack_mask);

            low_enveloped_mean_squared =
                (sample_squared + low_enveloped_mean_squared * low_samples) * low_scale;
            low_enveloped_mean_squared = utils::min(low_enveloped_mean_squared, lower_threshold);

            let lower_mag_delta = lower_threshold / low_enveloped_mean_squared;
            let lower_mult = futils::pow(lower_mag_delta, lower_ratio);

            let gain_compression = utils::clamp(upper_mult * lower_mult, 0.0, MAX_EXPAND_MULT);
            audio_out[i].set(gain_compression * sample);
        }

        self.low_enveloped_mean_squared = low_enveloped_mean_squared;
        self.high_enveloped_mean_squared = high_enveloped_mean_squared;
    }

    /// `scaleOutput(audio_input, num_samples)`.
    fn scale_output(&mut self, this: &Processor, audio_input: &Buffer, num_samples: usize) {
        let audio_out = this.output_buffer(Self::AUDIO_OUT);

        let mut current_output_mult = self.output_mult;
        let gain = utils::clamp(this.input(Self::OUTPUT_GAIN).at(0), MIN_GAIN, MAX_GAIN);
        self.output_mult = futils::db_to_magnitude(gain);
        let delta_output_mult =
            (self.output_mult - current_output_mult) * (1.0 / num_samples as f32);

        let mut current_mix = self.mix;
        self.mix = utils::clamp(this.input(Self::MIX).at(0), 0.0, 1.0);
        let delta_mix = (self.mix - current_mix) * (1.0 / num_samples as f32);

        for i in 0..num_samples {
            current_output_mult += delta_output_mult;
            current_mix += delta_mix;
            audio_out[i].set(utils::interpolate(
                audio_input[i].get(),
                audio_out[i].get() * current_output_mult,
                current_mix,
            ));
        }
    }

    /// `computeMeanSquared(audio_in, num_samples, mean_squared)`.
    fn compute_mean_squared(
        this: &Processor,
        audio_in: &Buffer,
        num_samples: usize,
        mut mean_squared: PolyFloat,
    ) -> PolyFloat {
        let rms_samples = (RMS_TIME * this.sample_rate() as f32) as i32;
        let rms_adjusted = rms_samples as f32 - 1.0;
        let input_scale = 1.0 / rms_samples as f32;

        for sample in audio_in.iter().take(num_samples) {
            let sample = sample.get();
            let sample_squared = sample * sample;
            mean_squared = (mean_squared * rms_adjusted + sample_squared) * input_scale;
        }
        mean_squared
    }
}

impl ProcessorImpl for Compressor {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio_in = this.input_buffer(Self::AUDIO);
        self.process_input(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        self.process_input(this, audio_in, num_samples);
    }

    fn reset(&mut self, _this: &Processor, _reset_mask: PolyMask) {
        self.input_mean_squared = PolyFloat::zero();
        self.output_mean_squared = PolyFloat::zero();
        self.output_mult = PolyFloat::zero();
        self.mix = PolyFloat::zero();

        self.high_enveloped_mean_squared = PolyFloat::zero();
        self.low_enveloped_mean_squared = PolyFloat::zero();
    }
}

/// `MultibandCompressor::BandOptions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandOptions {
    Multiband = 0,
    LowBand = 1,
    HighBand = 2,
    SingleBand = 3,
}

impl BandOptions {
    pub const NUM_BAND_OPTIONS: i32 = 4;
}

/// `vital::MultibandCompressor`.
#[derive(Clone)]
pub struct MultibandCompressor {
    was_low_enabled: bool,
    was_high_enabled: bool,

    low_band_upper_ratio: Rc<Output>,
    band_high_upper_ratio: Rc<Output>,
    low_band_lower_ratio: Rc<Output>,
    band_high_lower_ratio: Rc<Output>,
    low_band_upper_threshold: Rc<Output>,
    band_high_upper_threshold: Rc<Output>,
    low_band_lower_threshold: Rc<Output>,
    band_high_lower_threshold: Rc<Output>,

    low_band_output_gain: Rc<Output>,
    band_high_output_gain: Rc<Output>,

    low_band_filter: ProcessorRef,
    band_high_filter: ProcessorRef,

    low_band_compressor: ProcessorRef,
    band_high_compressor: ProcessorRef,
}

impl MultibandCompressor {
    pub const AUDIO: usize = 0;
    pub const LOW_UPPER_RATIO: usize = 1;
    pub const BAND_UPPER_RATIO: usize = 2;
    pub const HIGH_UPPER_RATIO: usize = 3;
    pub const LOW_LOWER_RATIO: usize = 4;
    pub const BAND_LOWER_RATIO: usize = 5;
    pub const HIGH_LOWER_RATIO: usize = 6;
    pub const LOW_UPPER_THRESHOLD: usize = 7;
    pub const BAND_UPPER_THRESHOLD: usize = 8;
    pub const HIGH_UPPER_THRESHOLD: usize = 9;
    pub const LOW_LOWER_THRESHOLD: usize = 10;
    pub const BAND_LOWER_THRESHOLD: usize = 11;
    pub const HIGH_LOWER_THRESHOLD: usize = 12;
    pub const LOW_OUTPUT_GAIN: usize = 13;
    pub const BAND_OUTPUT_GAIN: usize = 14;
    pub const HIGH_OUTPUT_GAIN: usize = 15;
    pub const ATTACK: usize = 16;
    pub const RELEASE: usize = 17;
    pub const ENABLED_BANDS: usize = 18;
    pub const MIX: usize = 19;
    pub const NUM_INPUTS: usize = 20;

    pub const AUDIO_OUT: usize = 0;
    pub const LOW_INPUT_MEAN_SQUARED: usize = 1;
    pub const BAND_INPUT_MEAN_SQUARED: usize = 2;
    pub const HIGH_INPUT_MEAN_SQUARED: usize = 3;
    pub const LOW_OUTPUT_MEAN_SQUARED: usize = 4;
    pub const BAND_OUTPUT_MEAN_SQUARED: usize = 5;
    pub const HIGH_OUTPUT_MEAN_SQUARED: usize = 6;
    pub const NUM_OUTPUTS: usize = 7;

    /// `new MultibandCompressor()`.
    pub fn create() -> ProcessorRef {
        let kernel = MultibandCompressor {
            was_low_enabled: false,
            was_high_enabled: false,
            low_band_upper_ratio: Output::new_control_rate(),
            band_high_upper_ratio: Output::new_control_rate(),
            low_band_lower_ratio: Output::new_control_rate(),
            band_high_lower_ratio: Output::new_control_rate(),
            low_band_upper_threshold: Output::new_control_rate(),
            band_high_upper_threshold: Output::new_control_rate(),
            low_band_lower_threshold: Output::new_control_rate(),
            band_high_lower_threshold: Output::new_control_rate(),
            low_band_output_gain: Output::new_control_rate(),
            band_high_output_gain: Output::new_control_rate(),
            low_band_filter: LinkwitzRileyFilter::create(120.0),
            band_high_filter: LinkwitzRileyFilter::create(2500.0),
            low_band_compressor: Compressor::create(
                LOW_ATTACK_MS,
                LOW_RELEASE_MS,
                BAND_ATTACK_MS,
                BAND_RELEASE_MS,
            ),
            band_high_compressor: Compressor::create(
                BAND_ATTACK_MS,
                BAND_RELEASE_MS,
                HIGH_ATTACK_MS,
                HIGH_RELEASE_MS,
            ),
        };
        let wiring = kernel.clone();
        let node = Processor::new(kernel, Self::NUM_INPUTS, Self::NUM_OUTPUTS, false, 1);

        for (compressor, upper_threshold, lower_threshold, upper_ratio, lower_ratio, gain) in [
            (
                &wiring.low_band_compressor,
                &wiring.low_band_upper_threshold,
                &wiring.low_band_lower_threshold,
                &wiring.low_band_upper_ratio,
                &wiring.low_band_lower_ratio,
                &wiring.low_band_output_gain,
            ),
            (
                &wiring.band_high_compressor,
                &wiring.band_high_upper_threshold,
                &wiring.band_high_lower_threshold,
                &wiring.band_high_upper_ratio,
                &wiring.band_high_lower_ratio,
                &wiring.band_high_output_gain,
            ),
        ] {
            compressor.plug(upper_threshold, Compressor::UPPER_THRESHOLD);
            compressor.plug(lower_threshold, Compressor::LOWER_THRESHOLD);
            compressor.plug(upper_ratio, Compressor::UPPER_RATIO);
            compressor.plug(lower_ratio, Compressor::LOWER_RATIO);
            compressor.plug(gain, Compressor::OUTPUT_GAIN);
            compressor.use_input(&node.input(Self::ATTACK), Compressor::ATTACK);
            compressor.use_input(&node.input(Self::RELEASE), Compressor::RELEASE);
            compressor.use_input(&node.input(Self::MIX), Compressor::MIX);
        }
        node
    }

    fn compressor_mean_squares(compressor: &Processor) -> (PolyFloat, PolyFloat) {
        compressor.with::<Compressor, _>(|c, _| (c.input_mean_squared(), c.output_mean_squared()))
    }

    /// `packFilterOutput(filter, num_samples, dest)`.
    fn pack_filter_output(filter: &Processor, num_samples: usize, dest: &Buffer) {
        let low_output = filter.output(lr::AUDIO_LOW).buffer();
        let high_output = filter.output(lr::AUDIO_HIGH).buffer();

        for i in 0..num_samples {
            let low_sample = low_output[i].get();
            let high_sample = utils::swap_voices(high_output[i].get());
            dest[i].set(utils::mask_load(
                high_sample,
                low_sample,
                constants::first_mask(),
            ));
        }
    }

    /// `packLowBandCompressor(num_samples, dest)`.
    fn pack_low_band_compressor(&self, num_samples: usize, dest: &Buffer) {
        let low_output = self.band_high_filter.output(lr::AUDIO_LOW).buffer();
        let high_output = self.band_high_filter.output(lr::AUDIO_HIGH).buffer();

        for i in 0..num_samples {
            let low_band_sample = low_output[i].get();
            let low_high_sample = high_output[i].get() & constants::first_mask();
            dest[i].set(low_band_sample + low_high_sample);
        }
    }

    /// `writeAllCompressorOutputs(num_samples, dest)`.
    fn write_all_compressor_outputs(&self, num_samples: usize, dest: &Buffer) {
        let low_band_output = self
            .low_band_compressor
            .output(Compressor::AUDIO_OUT)
            .buffer();
        let high_output = self
            .band_high_compressor
            .output(Compressor::AUDIO_OUT)
            .buffer();

        for i in 0..num_samples {
            let mut low_band_sample = low_band_output[i].get();
            low_band_sample += utils::swap_voices(low_band_sample);
            let high_sample = utils::swap_voices(high_output[i].get());
            dest[i].set(low_band_sample + high_sample);
        }
    }

    /// `writeCompressorOutputs(compressor, num_samples, dest)`.
    fn write_compressor_outputs(compressor: &Processor, num_samples: usize, dest: &Buffer) {
        let compressor_output = compressor.output(Compressor::AUDIO_OUT).buffer();

        for i in 0..num_samples {
            let sample = compressor_output[i].get();
            dest[i].set(sample + utils::swap_voices(sample));
        }
    }

    fn reset_members(&self, reset_mask: PolyMask) {
        self.low_band_filter.reset(reset_mask);
        self.band_high_filter.reset(reset_mask);
        self.low_band_compressor.reset(reset_mask);
        self.band_high_compressor.reset(reset_mask);
    }

    /// `processWithInput(audio_in, num_samples)`.
    pub fn process_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        let enabled_bands = this.input(Self::ENABLED_BANDS).at(0).get(0) as i32;
        let low_enabled = enabled_bands == BandOptions::Multiband as i32
            || enabled_bands == BandOptions::LowBand as i32;
        let high_enabled = enabled_bands == BandOptions::Multiband as i32
            || enabled_bands == BandOptions::HighBand as i32;

        let first = constants::first_mask();
        let pair = |output: &Rc<Output>, low: usize, high: usize| {
            output.set_at(
                0,
                utils::mask_load(this.input(high).at(0), this.input(low).at(0), first),
            );
        };
        pair(
            &self.low_band_upper_threshold,
            Self::LOW_UPPER_THRESHOLD,
            Self::BAND_UPPER_THRESHOLD,
        );
        pair(
            &self.band_high_upper_threshold,
            Self::BAND_UPPER_THRESHOLD,
            Self::HIGH_UPPER_THRESHOLD,
        );
        pair(
            &self.low_band_lower_threshold,
            Self::LOW_LOWER_THRESHOLD,
            Self::BAND_LOWER_THRESHOLD,
        );
        pair(
            &self.band_high_lower_threshold,
            Self::BAND_LOWER_THRESHOLD,
            Self::HIGH_LOWER_THRESHOLD,
        );
        pair(
            &self.low_band_upper_ratio,
            Self::LOW_UPPER_RATIO,
            Self::BAND_UPPER_RATIO,
        );
        pair(
            &self.band_high_upper_ratio,
            Self::BAND_UPPER_RATIO,
            Self::HIGH_UPPER_RATIO,
        );
        pair(
            &self.low_band_lower_ratio,
            Self::LOW_LOWER_RATIO,
            Self::BAND_LOWER_RATIO,
        );
        pair(
            &self.band_high_lower_ratio,
            Self::BAND_LOWER_RATIO,
            Self::HIGH_LOWER_RATIO,
        );
        pair(
            &self.low_band_output_gain,
            Self::LOW_OUTPUT_GAIN,
            Self::BAND_OUTPUT_GAIN,
        );
        pair(
            &self.band_high_output_gain,
            Self::BAND_OUTPUT_GAIN,
            Self::HIGH_OUTPUT_GAIN,
        );

        if low_enabled != self.was_low_enabled || high_enabled != self.was_high_enabled {
            self.reset_members(constants::full_mask());
            self.was_low_enabled = low_enabled;
            self.was_high_enabled = high_enabled;
        }

        let audio_out = this.output_buffer(Self::AUDIO_OUT);

        if low_enabled && high_enabled {
            self.low_band_filter
                .process_with_input(audio_in, num_samples);
            Self::pack_filter_output(&self.low_band_filter, num_samples, &audio_out);
            self.band_high_filter
                .process_with_input(&audio_out, num_samples);
            self.pack_low_band_compressor(num_samples, &audio_out);

            self.low_band_compressor
                .process_with_input(&audio_out, num_samples);
            let band_high_buffer = self.band_high_filter.output(lr::AUDIO_HIGH).buffer();
            self.band_high_compressor
                .process_with_input(&band_high_buffer, num_samples);
            self.write_all_compressor_outputs(num_samples, &audio_out);
        } else if low_enabled {
            self.low_band_filter
                .process_with_input(audio_in, num_samples);
            Self::pack_filter_output(&self.low_band_filter, num_samples, &audio_out);
            self.low_band_compressor
                .process_with_input(&audio_out, num_samples);
            Self::write_compressor_outputs(&self.low_band_compressor, num_samples, &audio_out);
        } else if high_enabled {
            self.band_high_filter
                .process_with_input(audio_in, num_samples);
            Self::pack_filter_output(&self.band_high_filter, num_samples, &audio_out);
            self.band_high_compressor
                .process_with_input(&audio_out, num_samples);
            Self::write_compressor_outputs(&self.band_high_compressor, num_samples, &audio_out);
        } else {
            self.band_high_compressor
                .process_with_input(audio_in, num_samples);
            let compressed = self
                .band_high_compressor
                .output(Compressor::AUDIO_OUT)
                .buffer();
            for i in 0..num_samples {
                audio_out[i].set(compressed[i].get());
            }
        }

        let (low_band_input_ms, low_band_output_ms) =
            Self::compressor_mean_squares(&self.low_band_compressor);
        let (band_high_input_ms, band_high_output_ms) =
            Self::compressor_mean_squares(&self.band_high_compressor);

        this.output(Self::LOW_INPUT_MEAN_SQUARED)
            .set_at(0, low_band_input_ms);
        this.output(Self::LOW_OUTPUT_MEAN_SQUARED)
            .set_at(0, low_band_output_ms);

        if low_enabled {
            this.output(Self::BAND_INPUT_MEAN_SQUARED)
                .set_at(0, utils::swap_voices(low_band_input_ms));
            this.output(Self::BAND_OUTPUT_MEAN_SQUARED)
                .set_at(0, utils::swap_voices(low_band_output_ms));
        } else {
            this.output(Self::BAND_INPUT_MEAN_SQUARED)
                .set_at(0, band_high_input_ms);
            this.output(Self::BAND_OUTPUT_MEAN_SQUARED)
                .set_at(0, band_high_output_ms);
        }

        this.output(Self::HIGH_INPUT_MEAN_SQUARED)
            .set_at(0, utils::swap_voices(band_high_input_ms));
        this.output(Self::HIGH_OUTPUT_MEAN_SQUARED)
            .set_at(0, utils::swap_voices(band_high_output_ms));
    }
}

impl ProcessorImpl for MultibandCompressor {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio_in = this.input_buffer(Self::AUDIO);
        self.process_input(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        self.process_input(this, audio_in, num_samples);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.processor_set_oversample_amount(oversample);
        self.low_band_filter.set_oversample_amount(oversample);
        self.band_high_filter.set_oversample_amount(oversample);
        self.low_band_compressor.set_oversample_amount(oversample);
        self.band_high_compressor.set_oversample_amount(oversample);
    }

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.processor_set_sample_rate(sample_rate);
        self.low_band_filter.set_sample_rate(sample_rate);
        self.band_high_filter.set_sample_rate(sample_rate);
        self.low_band_compressor.set_sample_rate(sample_rate);
        self.band_high_compressor.set_sample_rate(sample_rate);
    }

    fn reset(&mut self, this: &Processor, reset_mask: PolyMask) {
        self.reset_members(reset_mask);

        for output in [
            Self::LOW_INPUT_MEAN_SQUARED,
            Self::LOW_OUTPUT_MEAN_SQUARED,
            Self::BAND_INPUT_MEAN_SQUARED,
            Self::BAND_OUTPUT_MEAN_SQUARED,
            Self::HIGH_INPUT_MEAN_SQUARED,
            Self::HIGH_OUTPUT_MEAN_SQUARED,
        ] {
            this.output(output).set_at(0, PolyFloat::zero());
        }
    }
}
