// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::PhaserModule`: the phaser effect, a [`Phaser`] with its
//! `phaser_*` controls.
//!
//! Ports `vital/src/synthesis/modules/phaser_module.h` and
//! `phaser_module.cpp`.

use std::rc::Rc;

use sas_dsp_core::output::{Buffer, Output};
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::SynthModule;

use crate::phaser::Phaser;

/// `vital::PhaserModule`.
#[derive(Clone)]
pub struct PhaserModule {
    beats_per_second: Rc<Output>,
    phaser: Option<ProcessorRef>,
}

impl PhaserModule {
    pub const AUDIO_OUTPUT: usize = 0;
    pub const CUTOFF_OUTPUT: usize = 1;
    pub const NUM_OUTPUTS: usize = 2;

    /// `new PhaserModule(beats_per_second)`.
    pub fn create(beats_per_second: &Rc<Output>) -> ProcessorRef {
        SynthModule::create_with(
            PhaserModule {
                beats_per_second: beats_per_second.clone(),
                phaser: None,
            },
            0,
            Self::NUM_OUTPUTS,
            false,
        )
    }

    fn phaser(&self) -> &ProcessorRef {
        self.phaser
            .as_ref()
            .expect("PhaserModule::init creates the phaser")
    }
}

impl ProcessorImpl for PhaserModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let phaser = Phaser::create();
        phaser.use_output(&this.output(Self::AUDIO_OUTPUT), Phaser::AUDIO_OUTPUT);
        phaser.use_output(&this.output(Self::CUTOFF_OUTPUT), Phaser::CUTOFF_OUTPUT);
        this.add_idle_processor(&phaser);

        let free_frequency = this.create_mono_mod_control("phaser_frequency", false, false, None);
        let frequency = this.create_tempo_sync_switch(
            "phaser",
            &free_frequency.owner().expect("control output has an owner"),
            &self.beats_per_second,
            false,
            None,
        );
        let feedback = this.create_mono_mod_control("phaser_feedback", false, false, None);
        let wet = this.create_mono_mod_control("phaser_dry_wet", false, false, None);
        let center = this.create_mono_mod_control("phaser_center", true, true, None);
        let mod_depth = this.create_mono_mod_control("phaser_mod_depth", false, false, None);
        let phase_offset = this.create_mono_mod_control("phaser_phase_offset", false, false, None);
        let blend = this.create_mono_mod_control("phaser_blend", false, false, None);

        phaser.plug(&frequency, Phaser::RATE);
        phaser.plug(&wet, Phaser::MIX);
        phaser.plug(&feedback, Phaser::FEEDBACK_GAIN);
        phaser.plug(&center, Phaser::CENTER);
        phaser.plug(&mod_depth, Phaser::MOD_DEPTH);
        phaser.plug(&phase_offset, Phaser::PHASE_OFFSET);
        phaser.plug(&blend, Phaser::BLEND);
        phaser.init();
        self.phaser = Some(phaser);

        this.router_init();
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.phaser().hard_reset();
    }

    fn enable(&mut self, this: &Processor, enable: bool) {
        this.synth_module_enable(enable);
        this.router_process(1);
        if enable {
            self.phaser().hard_reset();
        }
    }

    fn correct_to_time(&mut self, _this: &Processor, seconds: f64) {
        self.phaser().correct_to_time(seconds);
    }

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.router_set_sample_rate(sample_rate);
        self.phaser().set_sample_rate(sample_rate);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        this.router_process(num_samples);
        self.phaser().process_with_input(audio_in, num_samples);
    }
}
