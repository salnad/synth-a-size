// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::ReverbModule`: the reverb effect, a [`Reverb`] with its
//! `reverb_*` controls.
//!
//! Ports `vital/src/synthesis/modules/reverb_module.h` and
//! `reverb_module.cpp`.

use sas_dsp_core::output::Buffer;
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::SynthModule;

use crate::reverb::Reverb;

/// `vital::ReverbModule`.
#[derive(Clone, Default)]
pub struct ReverbModule {
    reverb: Option<ProcessorRef>,
}

impl ReverbModule {
    /// `new ReverbModule()`.
    pub fn create() -> ProcessorRef {
        SynthModule::create_with(ReverbModule::default(), 0, 1, false)
    }

    fn reverb(&self) -> &ProcessorRef {
        self.reverb
            .as_ref()
            .expect("ReverbModule::init creates the reverb")
    }
}

impl ProcessorImpl for ReverbModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let reverb = Reverb::create();
        reverb.use_output(&this.output(0), 0);
        this.add_idle_processor(&reverb);

        let control = |name: &str| this.create_mono_mod_control(name, false, false, None);
        let decay_time = control("reverb_decay_time");
        let pre_low_cutoff = control("reverb_pre_low_cutoff");
        let pre_high_cutoff = control("reverb_pre_high_cutoff");
        let low_shelf_cutoff = control("reverb_low_shelf_cutoff");
        let low_shelf_gain = control("reverb_low_shelf_gain");
        let high_shelf_cutoff = control("reverb_high_shelf_cutoff");
        let high_shelf_gain = control("reverb_high_shelf_gain");
        let chorus_amount = control("reverb_chorus_amount");
        let chorus_frequency = control("reverb_chorus_frequency");
        let size = control("reverb_size");
        let delay = control("reverb_delay");
        let wet = control("reverb_dry_wet");

        reverb.plug(&decay_time, Reverb::DECAY_TIME);
        reverb.plug(&pre_low_cutoff, Reverb::PRE_LOW_CUTOFF);
        reverb.plug(&pre_high_cutoff, Reverb::PRE_HIGH_CUTOFF);
        reverb.plug(&low_shelf_cutoff, Reverb::LOW_CUTOFF);
        reverb.plug(&low_shelf_gain, Reverb::LOW_GAIN);
        reverb.plug(&high_shelf_cutoff, Reverb::HIGH_CUTOFF);
        reverb.plug(&high_shelf_gain, Reverb::HIGH_GAIN);
        reverb.plug(&chorus_amount, Reverb::CHORUS_AMOUNT);
        reverb.plug(&chorus_frequency, Reverb::CHORUS_FREQUENCY);
        reverb.plug(&delay, Reverb::DELAY);
        reverb.plug(&size, Reverb::SIZE);
        reverb.plug(&wet, Reverb::WET);
        self.reverb = Some(reverb);

        this.router_init();
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.reverb().hard_reset();
    }

    fn enable(&mut self, this: &Processor, enable: bool) {
        this.synth_module_enable(enable);
        this.router_process(1);
        if !enable {
            self.reverb().hard_reset();
        }
    }

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.router_set_sample_rate(sample_rate);
        self.reverb().set_sample_rate(sample_rate);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        this.router_process(num_samples);
        self.reverb().process_with_input(audio_in, num_samples);
    }
}
