// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::DistortionModule`: the distortion effect, a [`Distortion`] with
//! an optional [`DigitalSvf`] before or after it and a dry/wet mix.
//!
//! Ports `vital/src/synthesis/modules/distortion_module.h` and
//! `distortion_module.cpp`.

use sas_dsp_core::output::Buffer;
use sas_dsp_core::output::Output;
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::sas_simd::poly_utils as utils;
use sas_dsp_core::sas_simd::PolyFloat;
use sas_dsp_core::SynthModule;
use sas_filters::synth_filter::input;
use sas_filters::DigitalSvf;
use std::rc::Rc;

use crate::distortion::Distortion;

/// `vital::DistortionModule`.
#[derive(Clone)]
pub struct DistortionModule {
    distortion: Option<ProcessorRef>,
    filter_order: Option<ProcessorRef>,
    filter: Option<ProcessorRef>,
    distortion_mix: Option<Rc<Output>>,
    mix: PolyFloat,
}

fn node(processor: &Option<ProcessorRef>) -> &ProcessorRef {
    processor
        .as_ref()
        .expect("DistortionModule::init creates its processors")
}

impl DistortionModule {
    /// `new DistortionModule()`.
    pub fn create() -> ProcessorRef {
        SynthModule::create_with(
            DistortionModule {
                distortion: None,
                filter_order: None,
                filter: None,
                distortion_mix: None,
                mix: PolyFloat::zero(),
            },
            0,
            1,
            false,
        )
    }
}

impl ProcessorImpl for DistortionModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let distortion = Distortion::create();
        distortion.use_output(&this.output(0), Distortion::AUDIO_OUT);
        this.add_idle_processor(&distortion);

        let distortion_type = this.create_base_control("distortion_type", false, false);
        let distortion_drive = this.create_mono_mod_control("distortion_drive", true, true, None);
        self.distortion_mix =
            Some(this.create_mono_mod_control("distortion_mix", false, false, None));

        distortion.plug(&distortion_type, Distortion::TYPE);
        distortion.plug(&distortion_drive, Distortion::DRIVE);

        self.filter_order = Some(this.create_base_control("distortion_filter_order", false, false));
        let midi_cutoff =
            this.create_mono_mod_control("distortion_filter_cutoff", true, true, None);
        let resonance =
            this.create_mono_mod_control("distortion_filter_resonance", false, false, None);
        let blend = this.create_mono_mod_control("distortion_filter_blend", false, false, None);

        let mut svf = DigitalSvf::new();
        svf.set_drive_compensation(false);
        svf.set_basic(true);
        let filter = DigitalSvf::create_with(svf);
        filter.use_output(&this.output(0), 0);
        filter.plug(&midi_cutoff, input::MIDI_CUTOFF);
        filter.plug(&resonance, input::RESONANCE);
        filter.plug(&blend, input::PASS_BLEND);
        this.add_idle_processor(&filter);

        self.distortion = Some(distortion);
        self.filter = Some(filter);

        this.router_init();
    }

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.router_set_sample_rate(sample_rate);
        node(&self.distortion).set_sample_rate(sample_rate);
        node(&self.filter).set_sample_rate(sample_rate);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        this.router_process(num_samples);

        let distortion = node(&self.distortion);
        let filter = node(&self.filter);
        let filter_order = node(&self.filter_order).output(0).at(0).get(0);
        if filter_order < 1.0 {
            distortion.process_with_input(audio_in, num_samples);
        } else if filter_order > 1.0 {
            distortion.process_with_input(audio_in, num_samples);
            filter.process_with_input(&this.output(0).buffer(), num_samples);
        } else {
            filter.process_with_input(audio_in, num_samples);
            distortion.process_with_input(&this.output(0).buffer(), num_samples);
        }

        let mut current_mix = self.mix;
        self.mix = utils::clamp(
            self.distortion_mix.as_ref().expect("initialized").at(0),
            0.0,
            1.0,
        );
        let delta_mix = (self.mix - current_mix) * (1.0 / num_samples as f32);
        let audio_out = this.output_buffer(0);

        for i in 0..num_samples {
            current_mix += delta_mix;
            audio_out[i].set(utils::interpolate(
                audio_in[i].get(),
                audio_out[i].get(),
                current_mix,
            ));
        }
    }
}
