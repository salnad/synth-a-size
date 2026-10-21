// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::CompressorModule`: the compressor effect, a
//! [`MultibandCompressor`] with its `compressor_*` controls.
//!
//! Ports `vital/src/synthesis/modules/compressor_module.h` and
//! `compressor_module.cpp`.

use sas_dsp_core::output::Buffer;
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::{constants, SynthModule};

use crate::compressor::MultibandCompressor;

/// `vital::CompressorModule`.
#[derive(Clone, Default)]
pub struct CompressorModule {
    compressor: Option<ProcessorRef>,
}

impl CompressorModule {
    pub const AUDIO: usize = 0;
    pub const LOW_INPUT_MEAN_SQUARED: usize = 1;
    pub const BAND_INPUT_MEAN_SQUARED: usize = 2;
    pub const HIGH_INPUT_MEAN_SQUARED: usize = 3;
    pub const LOW_OUTPUT_MEAN_SQUARED: usize = 4;
    pub const BAND_OUTPUT_MEAN_SQUARED: usize = 5;
    pub const HIGH_OUTPUT_MEAN_SQUARED: usize = 6;
    pub const NUM_OUTPUTS: usize = 7;

    /// `new CompressorModule()`.
    pub fn create() -> ProcessorRef {
        SynthModule::create_with(CompressorModule::default(), 0, Self::NUM_OUTPUTS, false)
    }

    fn compressor(&self) -> &ProcessorRef {
        self.compressor
            .as_ref()
            .expect("CompressorModule::init creates the compressor")
    }
}

impl ProcessorImpl for CompressorModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let compressor = MultibandCompressor::create();
        for (module_output, compressor_output) in [
            (Self::AUDIO, MultibandCompressor::AUDIO_OUT),
            (
                Self::LOW_INPUT_MEAN_SQUARED,
                MultibandCompressor::LOW_INPUT_MEAN_SQUARED,
            ),
            (
                Self::BAND_INPUT_MEAN_SQUARED,
                MultibandCompressor::BAND_INPUT_MEAN_SQUARED,
            ),
            (
                Self::HIGH_INPUT_MEAN_SQUARED,
                MultibandCompressor::HIGH_INPUT_MEAN_SQUARED,
            ),
            (
                Self::LOW_OUTPUT_MEAN_SQUARED,
                MultibandCompressor::LOW_OUTPUT_MEAN_SQUARED,
            ),
            (
                Self::BAND_OUTPUT_MEAN_SQUARED,
                MultibandCompressor::BAND_OUTPUT_MEAN_SQUARED,
            ),
            (
                Self::HIGH_OUTPUT_MEAN_SQUARED,
                MultibandCompressor::HIGH_OUTPUT_MEAN_SQUARED,
            ),
        ] {
            compressor.use_output(&this.output(module_output), compressor_output);
        }
        this.add_idle_processor(&compressor);

        let mono = |name: &str| this.create_mono_mod_control(name, false, false, None);
        let base = |name: &str| this.create_base_control(name, false, false);

        let attack = mono("compressor_attack");
        let release = mono("compressor_release");
        let low_gain = mono("compressor_low_gain");
        let band_gain = mono("compressor_band_gain");
        let high_gain = mono("compressor_high_gain");

        let enabled_bands = base("compressor_enabled_bands");

        let low_upper_ratio = base("compressor_low_upper_ratio");
        let band_upper_ratio = base("compressor_band_upper_ratio");
        let high_upper_ratio = base("compressor_high_upper_ratio");
        let low_lower_ratio = base("compressor_low_lower_ratio");
        let band_lower_ratio = base("compressor_band_lower_ratio");
        let high_lower_ratio = base("compressor_high_lower_ratio");

        let low_upper_threshold = base("compressor_low_upper_threshold");
        let band_upper_threshold = base("compressor_band_upper_threshold");
        let high_upper_threshold = base("compressor_high_upper_threshold");
        let low_lower_threshold = base("compressor_low_lower_threshold");
        let band_lower_threshold = base("compressor_band_lower_threshold");
        let high_lower_threshold = base("compressor_high_lower_threshold");

        let mix = mono("compressor_mix");

        compressor.plug(&mix, MultibandCompressor::MIX);
        compressor.plug(&attack, MultibandCompressor::ATTACK);
        compressor.plug(&release, MultibandCompressor::RELEASE);
        compressor.plug(&low_gain, MultibandCompressor::LOW_OUTPUT_GAIN);
        compressor.plug(&band_gain, MultibandCompressor::BAND_OUTPUT_GAIN);
        compressor.plug(&high_gain, MultibandCompressor::HIGH_OUTPUT_GAIN);
        compressor.plug(&enabled_bands, MultibandCompressor::ENABLED_BANDS);

        compressor.plug(&low_upper_ratio, MultibandCompressor::LOW_UPPER_RATIO);
        compressor.plug(&band_upper_ratio, MultibandCompressor::BAND_UPPER_RATIO);
        compressor.plug(&high_upper_ratio, MultibandCompressor::HIGH_UPPER_RATIO);
        compressor.plug(&low_lower_ratio, MultibandCompressor::LOW_LOWER_RATIO);
        compressor.plug(&band_lower_ratio, MultibandCompressor::BAND_LOWER_RATIO);
        compressor.plug(&high_lower_ratio, MultibandCompressor::HIGH_LOWER_RATIO);

        compressor.plug(
            &low_upper_threshold,
            MultibandCompressor::LOW_UPPER_THRESHOLD,
        );
        compressor.plug(
            &band_upper_threshold,
            MultibandCompressor::BAND_UPPER_THRESHOLD,
        );
        compressor.plug(
            &high_upper_threshold,
            MultibandCompressor::HIGH_UPPER_THRESHOLD,
        );
        compressor.plug(
            &low_lower_threshold,
            MultibandCompressor::LOW_LOWER_THRESHOLD,
        );
        compressor.plug(
            &band_lower_threshold,
            MultibandCompressor::BAND_LOWER_THRESHOLD,
        );
        compressor.plug(
            &high_lower_threshold,
            MultibandCompressor::HIGH_LOWER_THRESHOLD,
        );
        self.compressor = Some(compressor);

        this.router_init();
    }

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.router_set_sample_rate(sample_rate);
        self.compressor().set_sample_rate(sample_rate);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        this.router_process(num_samples);
        self.compressor().process_with_input(audio_in, num_samples);
    }

    fn enable(&mut self, this: &Processor, enable: bool) {
        this.synth_module_enable(enable);
        if !enable {
            self.compressor().reset(constants::full_mask());
        }
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.compressor().reset(constants::full_mask());
    }
}
