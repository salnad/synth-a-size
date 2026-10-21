// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `SynthLfo` and `LfoModule` against the C++ reference
//! (`migration/corpus/modules/modulation/{synth_lfo,lfo_module}.json`;
//! modules in `migration/reference/src/modules/modulation/`).

mod common;

use common::*;
use sas_dsp_core::Value;
use sas_modulation::{LfoModule, SynthLfo};

#[test]
fn synth_lfo_matches_reference() {
    check_module(
        &ModuleDef::new(
            "synth_lfo",
            |params| {
                let lfo = SynthLfo::create(line_shape(params, "shape"));
                lfo.set_control_rate(param_bool(params, "control_rate", true));
                let sync_seconds = params.get("sync_seconds").and_then(|v| v.as_f64());
                lfo.with::<SynthLfo, _>(|kernel, _| {
                    kernel.correct_to_time(sync_seconds.unwrap_or(0.0))
                });
                lfo
            },
            &[
                ("frequency", SynthLfo::FREQUENCY, Control),
                ("phase", SynthLfo::PHASE, Control),
                ("note_trigger", SynthLfo::NOTE_TRIGGER, Trigger),
                ("sync_type", SynthLfo::SYNC_TYPE, Control),
                ("smooth_mode", SynthLfo::SMOOTH_MODE, Control),
                ("fade", SynthLfo::FADE, Control),
                ("smooth_time", SynthLfo::SMOOTH_TIME, Control),
                ("stereo_phase", SynthLfo::STEREO_PHASE, Control),
                ("delay", SynthLfo::DELAY, Control),
                ("note_count", SynthLfo::NOTE_COUNT, Control),
            ],
            &[
                ("value", SynthLfo::VALUE),
                ("osc_phase", SynthLfo::OSC_PHASE),
                ("osc_frequency", SynthLfo::OSC_FREQUENCY),
            ],
        )
        .normalized("osc_frequency", phase_increment),
    );
}

#[test]
fn lfo_module_matches_reference() {
    check_module(
        &ModuleDef::new(
            "lfo_module",
            |params| {
                let beats_per_second =
                    Value::create_control_rate(param(params, "beats_per_second", 2.0));
                let module = LfoModule::create(
                    param_str(params, "prefix", "lfo_1"),
                    line_shape(params, "shape"),
                    &beats_per_second.output(0),
                );
                module.init();
                module.set_control_rate(param_bool(params, "control_rate", true));
                let sync_seconds = params.get("sync_seconds").and_then(|v| v.as_f64());
                module.correct_to_time(sync_seconds.unwrap_or(0.0));
                set_controls(&module, params);
                module
            },
            &[
                ("note_trigger", LfoModule::NOTE_TRIGGER, Trigger),
                ("note_count", LfoModule::NOTE_COUNT, Control),
                ("midi", LfoModule::MIDI, Control),
            ],
            &[
                ("value", LfoModule::VALUE),
                ("osc_phase", LfoModule::OSC_PHASE),
                ("osc_frequency", LfoModule::OSC_FREQUENCY),
            ],
        )
        .normalized("osc_frequency", phase_increment),
    );
}
