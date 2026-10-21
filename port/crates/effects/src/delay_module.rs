// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::DelayModule`: the delay effect, a [`StereoDelay`] with its
//! controls (`delay_*` parameters, tempo-synced times).
//!
//! Ports `vital/src/synthesis/modules/delay_module.h` and `delay_module.cpp`.

use std::rc::Rc;

use sas_dsp_core::output::{Buffer, Output};
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::{StereoMemory, SynthModule};

use crate::delay::{self, StereoDelay};

/// `vital::DelayModule`.
#[derive(Clone)]
pub struct DelayModule {
    beats_per_second: Rc<Output>,
    delay: Option<ProcessorRef>,
}

impl DelayModule {
    pub const MAX_DELAY_TIME: f32 = 4.0;

    /// `new DelayModule(beats_per_second)`.
    pub fn create(beats_per_second: &Rc<Output>) -> ProcessorRef {
        let module = SynthModule::create_with(
            DelayModule {
                beats_per_second: beats_per_second.clone(),
                delay: None,
            },
            0,
            1,
            false,
        );
        let size = (Self::MAX_DELAY_TIME * module.sample_rate() as f32) as i32;
        let delay = StereoDelay::create(size);
        module.add_idle_processor(&delay);
        module.with::<DelayModule, _>(|kernel, _| kernel.delay = Some(delay));
        module
    }

    fn delay(&self) -> &ProcessorRef {
        self.delay
            .as_ref()
            .expect("DelayModule::create sets the delay")
    }
}

impl ProcessorImpl for DelayModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let delay = self.delay().clone();
        delay.use_output(&this.output(0), 0);

        let free_frequency = this.create_mono_mod_control("delay_frequency", false, false, None);
        let frequency = this.create_tempo_sync_switch(
            "delay",
            &free_frequency.owner().expect("control output has an owner"),
            &self.beats_per_second,
            false,
            None,
        );
        let free_frequency_aux =
            this.create_mono_mod_control("delay_aux_frequency", false, false, None);
        let frequency_aux = this.create_tempo_sync_switch(
            "delay_aux",
            &free_frequency_aux
                .owner()
                .expect("control output has an owner"),
            &self.beats_per_second,
            false,
            None,
        );
        let feedback = this.create_mono_mod_control("delay_feedback", false, false, None);
        let wet = this.create_mono_mod_control("delay_dry_wet", false, false, None);

        let filter_cutoff = this.create_mono_mod_control("delay_filter_cutoff", false, false, None);
        let filter_spread = this.create_mono_mod_control("delay_filter_spread", false, false, None);

        let style = this.create_base_control("delay_style", false, false);

        delay.plug(&frequency, StereoDelay::FREQUENCY);
        delay.plug(&frequency_aux, StereoDelay::FREQUENCY_AUX);
        delay.plug(&feedback, StereoDelay::FEEDBACK);
        delay.plug(&wet, StereoDelay::WET);
        delay.plug(&style, StereoDelay::STYLE);
        delay.plug(&filter_cutoff, StereoDelay::FILTER_CUTOFF);
        delay.plug(&filter_spread, StereoDelay::FILTER_SPREAD);

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

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.router_set_sample_rate(sample_rate);
        let delay = self.delay();
        delay.set_sample_rate(sample_rate);
        delay::set_max_samples::<StereoMemory>(
            delay,
            (Self::MAX_DELAY_TIME * this.sample_rate() as f32) as i32,
        );
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.router_set_oversample_amount(oversample);
        delay::set_max_samples::<StereoMemory>(
            self.delay(),
            (Self::MAX_DELAY_TIME * this.sample_rate() as f32) as i32,
        );
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        this.router_process(num_samples);
        self.delay().process_with_input(audio_in, num_samples);
    }
}
