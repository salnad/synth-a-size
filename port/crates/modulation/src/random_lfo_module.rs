// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::RandomLfoModule`: a [`RandomLfo`] with its parameters as module
//! controls and a tempo-sync frequency switch.
//!
//! Ports `vital/src/synthesis/modules/random_lfo_module.h` and
//! `random_lfo_module.cpp`.

use std::rc::Rc;

use sas_dsp_core::{Output, Processor, ProcessorImpl, ProcessorRef, SynthModule};

use crate::random_lfo::RandomLfo;

/// Kernel of `vital::RandomLfoModule`.
#[derive(Clone)]
pub struct RandomLfoModule {
    prefix: String,
    lfo: ProcessorRef,
    beats_per_second: Rc<Output>,
}

impl RandomLfoModule {
    pub const NOTE_TRIGGER: usize = 0;
    pub const MIDI: usize = 1;
    pub const NUM_INPUTS: usize = 2;

    /// `new RandomLfoModule(prefix, beats_per_second)`.
    pub fn create(prefix: &str, beats_per_second: &Rc<Output>) -> ProcessorRef {
        let lfo = RandomLfo::create();
        let module = SynthModule::create_with(
            RandomLfoModule {
                prefix: prefix.to_string(),
                lfo: lfo.clone(),
                beats_per_second: beats_per_second.clone(),
            },
            Self::NUM_INPUTS,
            1,
            false,
        );
        module.add_processor(&lfo);
        module
    }

    /// The random LFO this module wraps (`lfo_`).
    pub fn lfo(&self) -> &ProcessorRef {
        &self.lfo
    }
}

impl ProcessorImpl for RandomLfoModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let prefix = &self.prefix;
        let free_frequency =
            this.create_poly_mod_control(&format!("{prefix}_frequency"), false, false, None, None);
        let base = |name: &str| this.create_base_control(&format!("{prefix}_{name}"), false, false);
        let style = base("style");
        let stereo = base("stereo");
        let sync_type = base("sync_type");

        let free_frequency_owner = free_frequency
            .owner()
            .expect("frequency control output has an owner");
        let frequency = this.create_tempo_sync_switch(
            prefix,
            &free_frequency_owner,
            &self.beats_per_second,
            true,
            Some(&this.input(Self::MIDI)),
        );
        let lfo = &self.lfo;
        lfo.use_input(&this.input(Self::NOTE_TRIGGER), RandomLfo::RESET);
        lfo.use_output(&this.output(0), 0);
        lfo.plug(&frequency, RandomLfo::FREQUENCY);
        lfo.plug(&style, RandomLfo::STYLE);
        lfo.plug(&stereo, RandomLfo::STEREO);
        lfo.plug(&sync_type, RandomLfo::SYNC);
    }

    fn correct_to_time(&mut self, _this: &Processor, seconds: f64) {
        self.lfo
            .with::<RandomLfo, _>(|lfo, _| lfo.correct_to_time(seconds));
    }
}
