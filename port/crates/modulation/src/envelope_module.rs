// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::EnvelopeModule`: an [`Envelope`] with its parameters as module
//! controls (`<prefix>_delay`, `<prefix>_attack`, ...).
//!
//! Ports `vital/src/synthesis/modules/envelope_module.h` and
//! `envelope_module.cpp`. Control details (defaults, quartic time scaling)
//! come from the parameter registry installed with
//! [`sas_dsp_core::synth_module::set_parameter_lookup`].

use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef, SynthModule};

use crate::envelope::Envelope;

/// Kernel of `vital::EnvelopeModule`.
#[derive(Clone)]
pub struct EnvelopeModule {
    prefix: String,
    envelope: ProcessorRef,
    force_audio_rate: bool,
}

impl EnvelopeModule {
    pub const TRIGGER: usize = 0;
    pub const NUM_INPUTS: usize = 1;

    pub const VALUE: usize = 0;
    pub const PHASE: usize = 1;
    pub const NUM_OUTPUTS: usize = 2;

    /// `new EnvelopeModule(prefix, force_audio_rate)`.
    pub fn create(prefix: &str, force_audio_rate: bool) -> ProcessorRef {
        let envelope = Envelope::create();
        let module = SynthModule::create_with(
            EnvelopeModule {
                prefix: prefix.to_string(),
                envelope: envelope.clone(),
                force_audio_rate,
            },
            Self::NUM_INPUTS,
            Self::NUM_OUTPUTS,
            false,
        );
        envelope.use_input(&module.input(Self::TRIGGER), Envelope::TRIGGER);

        envelope.use_output(&module.output(Self::VALUE), Envelope::VALUE);
        envelope.use_output(&module.output(Self::PHASE), Envelope::PHASE);
        module.add_processor(&envelope);

        module.set_control_rate(!force_audio_rate);
        module
    }

    /// The envelope this module wraps (`envelope_`).
    pub fn envelope(&self) -> &ProcessorRef {
        &self.envelope
    }
}

impl ProcessorImpl for EnvelopeModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let prefix = &self.prefix;
        let poly = |name: &str| {
            this.create_poly_mod_control(&format!("{prefix}_{name}"), false, false, None, None)
        };
        let delay = poly("delay");
        let attack = poly("attack");
        let hold = poly("hold");
        let decay = poly("decay");
        let sustain = poly("sustain");
        let release = poly("release");

        let base = |name: &str| this.create_base_control(&format!("{prefix}_{name}"), false, false);
        let attack_power = base("attack_power");
        let decay_power = base("decay_power");
        let release_power = base("release_power");

        let envelope = &self.envelope;
        envelope.plug(&delay, Envelope::DELAY);
        envelope.plug(&attack, Envelope::ATTACK);
        envelope.plug(&hold, Envelope::HOLD);
        envelope.plug(&decay, Envelope::DECAY);
        envelope.plug(&sustain, Envelope::SUSTAIN);
        envelope.plug(&release, Envelope::RELEASE);
        envelope.plug(&attack_power, Envelope::ATTACK_POWER);
        envelope.plug(&decay_power, Envelope::DECAY_POWER);
        envelope.plug(&release_power, Envelope::RELEASE_POWER);
    }

    fn set_control_rate(&mut self, _this: &Processor, control_rate: bool) {
        if !self.force_audio_rate {
            self.envelope.set_control_rate(control_rate);
        }
    }
}
