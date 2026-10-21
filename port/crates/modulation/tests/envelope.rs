// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `Envelope` and `EnvelopeModule` against the C++ reference
//! (`migration/corpus/modules/modulation/envelope*.json`; modules in
//! `migration/reference/src/modules/modulation/envelope*.cpp`).

mod common;

use common::*;
use sas_modulation::{Envelope, EnvelopeModule};

#[test]
fn envelope_matches_reference() {
    check_module(&ModuleDef::new(
        "envelope",
        |params| {
            let envelope = Envelope::create();
            envelope.set_control_rate(param_bool(params, "control_rate", false));
            envelope
        },
        &[
            ("delay", Envelope::DELAY, Control),
            ("attack", Envelope::ATTACK, Control),
            ("attack_power", Envelope::ATTACK_POWER, Control),
            ("hold", Envelope::HOLD, Control),
            ("decay", Envelope::DECAY, Control),
            ("decay_power", Envelope::DECAY_POWER, Control),
            ("sustain", Envelope::SUSTAIN, Control),
            ("release", Envelope::RELEASE, Control),
            ("release_power", Envelope::RELEASE_POWER, Control),
            ("trigger", Envelope::TRIGGER, Trigger),
        ],
        &[("value", Envelope::VALUE), ("phase", Envelope::PHASE)],
    ));
}

#[test]
fn envelope_module_matches_reference() {
    check_module(&ModuleDef::new(
        "envelope_module",
        |params| {
            let module = EnvelopeModule::create(
                param_str(params, "prefix", "env_1"),
                param_bool(params, "force_audio_rate", false),
            );
            module.init();
            set_controls(&module, params);
            module
        },
        &[("trigger", EnvelopeModule::TRIGGER, Trigger)],
        &[
            ("value", EnvelopeModule::VALUE),
            ("phase", EnvelopeModule::PHASE),
        ],
    ));
}
