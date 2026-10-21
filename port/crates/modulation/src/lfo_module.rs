// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::LfoModule`: a [`SynthLfo`] with its parameters as module controls
//! and a tempo-sync frequency switch.
//!
//! Ports `vital/src/synthesis/modules/lfo_module.h` and `lfo_module.cpp`.

use std::cell::RefCell;
use std::rc::Rc;

use sas_common::LineGenerator;
use sas_dsp_core::{Output, Processor, ProcessorImpl, ProcessorRef, SynthModule};

use crate::synth_lfo::SynthLfo;

/// Kernel of `vital::LfoModule`.
#[derive(Clone)]
pub struct LfoModule {
    prefix: String,
    lfo: ProcessorRef,
    beats_per_second: Rc<Output>,
}

impl LfoModule {
    pub const NOTE_TRIGGER: usize = 0;
    pub const NOTE_COUNT: usize = 1;
    pub const MIDI: usize = 2;
    pub const NUM_INPUTS: usize = 3;

    pub const VALUE: usize = 0;
    pub const OSC_PHASE: usize = 1;
    pub const OSC_FREQUENCY: usize = 2;
    pub const NUM_OUTPUTS: usize = 3;

    /// `new LfoModule(prefix, line_generator, beats_per_second)`.
    pub fn create(
        prefix: &str,
        line_generator: Rc<RefCell<LineGenerator>>,
        beats_per_second: &Rc<Output>,
    ) -> ProcessorRef {
        let lfo = SynthLfo::create(line_generator);
        let module = SynthModule::create_with(
            LfoModule {
                prefix: prefix.to_string(),
                lfo: lfo.clone(),
                beats_per_second: beats_per_second.clone(),
            },
            Self::NUM_INPUTS,
            Self::NUM_OUTPUTS,
            false,
        );
        module.add_processor(&lfo);

        module.set_control_rate(true);
        module
    }

    /// The LFO this module wraps (`lfo_`).
    pub fn lfo(&self) -> &ProcessorRef {
        &self.lfo
    }
}

impl ProcessorImpl for LfoModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let prefix = &self.prefix;
        let poly = |name: &str| {
            this.create_poly_mod_control(&format!("{prefix}_{name}"), false, false, None, None)
        };
        let base = |name: &str| this.create_base_control(&format!("{prefix}_{name}"), false, false);

        let free_frequency = poly("frequency");
        let phase = poly("phase");
        let fade = poly("fade_time");
        let delay = poly("delay_time");
        let stereo_phase = poly("stereo");
        let sync_type = base("sync_type");
        let smooth_mode = base("smooth_mode");
        let smooth_time = poly("smooth_time");

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
        lfo.use_input(&this.input(Self::NOTE_TRIGGER), SynthLfo::NOTE_TRIGGER);
        lfo.use_input(&this.input(Self::NOTE_COUNT), SynthLfo::NOTE_COUNT);

        lfo.use_output(&this.output(Self::VALUE), SynthLfo::VALUE);
        lfo.use_output(&this.output(Self::OSC_PHASE), SynthLfo::OSC_PHASE);
        lfo.use_output(&this.output(Self::OSC_FREQUENCY), SynthLfo::OSC_FREQUENCY);
        lfo.plug(&frequency, SynthLfo::FREQUENCY);
        lfo.plug(&phase, SynthLfo::PHASE);
        lfo.plug(&stereo_phase, SynthLfo::STEREO_PHASE);
        lfo.plug(&sync_type, SynthLfo::SYNC_TYPE);
        lfo.plug(&smooth_mode, SynthLfo::SMOOTH_MODE);
        lfo.plug(&fade, SynthLfo::FADE);
        lfo.plug(&smooth_time, SynthLfo::SMOOTH_TIME);
        lfo.plug(&delay, SynthLfo::DELAY);
    }

    fn correct_to_time(&mut self, _this: &Processor, seconds: f64) {
        self.lfo
            .with::<SynthLfo, _>(|lfo, _| lfo.correct_to_time(seconds));
    }

    fn set_control_rate(&mut self, this: &Processor, control_rate: bool) {
        this.state().control_rate.set(control_rate);
        self.lfo.set_control_rate(control_rate);
    }
}
