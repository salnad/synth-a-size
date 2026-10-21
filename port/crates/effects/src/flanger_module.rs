// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::FlangerModule`: a [`StereoDelay`] (clamped, unfiltered) whose
//! delay time follows a triangle LFO around a center note.
//!
//! Ports `vital/src/synthesis/modules/flanger_module.h` and
//! `flanger_module.cpp`. Like upstream, the delay is an idle processor that
//! never receives `setSampleRate`.

use std::rc::Rc;

use sas_dsp_core::output::{Buffer, Output};
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::sas_simd::poly_utils as utils;
use sas_dsp_core::sas_simd::PolyFloat;
use sas_dsp_core::{constants, SynthModule, Value};

use crate::delay::{StereoDelay, Style};

/// `vital::FlangerModule`.
#[derive(Clone)]
pub struct FlangerModule {
    beats_per_second: Rc<Output>,
    frequency: Option<Rc<Output>>,
    phase_offset: Option<Rc<Output>>,
    center: Option<Rc<Output>>,
    mod_depth: Option<Rc<Output>>,
    phase: PolyFloat,

    delay_frequency: ProcessorRef,
    delay: Option<ProcessorRef>,
}

fn control(output: &Option<Rc<Output>>) -> PolyFloat {
    output
        .as_ref()
        .expect("FlangerModule::init creates its controls")
        .at(0)
}

impl FlangerModule {
    pub const MAX_FLANGER_SEMITONE_OFFSET: f32 = 24.0;
    pub const FLANGER_DELAY_RANGE: f32 = 0.01;
    pub const FLANGER_CENTER: f32 = Self::FLANGER_DELAY_RANGE * 0.5 + 0.0005;
    pub const MODULATION_DELAY_BUFFER: f32 = 0.0005;

    pub const AUDIO_OUTPUT: usize = 0;
    pub const FREQUENCY_OUTPUT: usize = 1;
    pub const NUM_OUTPUTS: usize = 2;

    /// `new FlangerModule(beats_per_second)`.
    pub fn create(beats_per_second: &Rc<Output>) -> ProcessorRef {
        SynthModule::create_with(
            FlangerModule {
                beats_per_second: beats_per_second.clone(),
                frequency: None,
                phase_offset: None,
                center: None,
                mod_depth: None,
                phase: PolyFloat::zero(),
                delay_frequency: Value::create_control_rate(0.0),
                delay: None,
            },
            0,
            Self::NUM_OUTPUTS,
            false,
        )
    }

    fn delay(&self) -> &ProcessorRef {
        self.delay
            .as_ref()
            .expect("FlangerModule::init creates the delay")
    }
}

impl ProcessorImpl for FlangerModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        const MAX_SAMPLES: i32 = 40000;
        let delay_style = Value::create_control_rate(Style::ClampedUnfiltered as i32 as f32);

        let delay = StereoDelay::create(MAX_SAMPLES);
        this.add_idle_processor(&delay);
        self.phase = PolyFloat::zero();
        delay.use_output(&this.output(Self::AUDIO_OUTPUT), 0);

        let free_frequency = this.create_mono_mod_control("flanger_frequency", false, false, None);
        self.frequency = Some(this.create_tempo_sync_switch(
            "flanger",
            &free_frequency.owner().expect("control output has an owner"),
            &self.beats_per_second,
            false,
            None,
        ));
        self.center = Some(this.create_mono_mod_control("flanger_center", false, false, None));
        let feedback = this.create_mono_mod_control("flanger_feedback", false, false, None);
        let wet = this.create_mono_mod_control("flanger_dry_wet", false, false, None);
        self.mod_depth =
            Some(this.create_mono_mod_control("flanger_mod_depth", false, false, None));

        self.phase_offset =
            Some(this.create_mono_mod_control("flanger_phase_offset", false, false, None));

        delay.plug(&self.delay_frequency, StereoDelay::FREQUENCY);
        delay.plug(&feedback, StereoDelay::FEEDBACK);
        delay.plug(&wet, StereoDelay::WET);
        delay.plug(&delay_style, StereoDelay::STYLE);
        self.delay = Some(delay);

        this.router_init();
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.delay().hard_reset();
    }

    fn enable(&mut self, this: &Processor, enable: bool) {
        this.synth_module_enable(enable);
        this.router_process(1);
        if !enable {
            self.delay().hard_reset();
        }
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        const MAX_FREQUENCY: f32 = 20000.0;

        this.router_process(num_samples);
        let frequency = control(&self.frequency);
        let delta_phase = (frequency * num_samples as f32) / this.sample_rate() as f32;
        self.phase = utils::modulo(self.phase + delta_phase);

        let phase_offset = control(&self.phase_offset);
        let right_offset = phase_offset & constants::right_mask();
        let phase_total = self.phase - phase_offset / 2.0 + right_offset;

        let modulation =
            control(&self.mod_depth) * (utils::triangle_wave(phase_total) * 2.0 - 1.0) + 1.0;
        let mut delay =
            PolyFloat::splat(1.0) / utils::midi_note_to_frequency(control(&self.center));
        delay =
            (delay - Self::MODULATION_DELAY_BUFFER) * modulation + Self::MODULATION_DELAY_BUFFER;
        let delay_frequency =
            PolyFloat::splat(1.0) / utils::max(delay, PolyFloat::splat(1.0 / MAX_FREQUENCY));

        this.output(Self::FREQUENCY_OUTPUT)
            .set_at(0, delay_frequency);
        self.delay_frequency.set_value(delay_frequency);
        self.delay().process_with_input(audio_in, num_samples);
    }

    fn correct_to_time(&mut self, _this: &Processor, seconds: f64) {
        self.phase = utils::get_cycle_offset_from_seconds(seconds, control(&self.frequency));
    }
}
