// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `RandomLfo`, `TriggerRandom` and `RandomLfoModule` against the
//! C++ reference (`migration/corpus/modules/modulation/{random_lfo,
//! trigger_random,random_lfo_module}.json`). Every case sets the spec seed,
//! so the RNG streams (`sas_common::random`) are compared exactly.

mod common;

use common::*;
use sas_dsp_core::{ProcessorRef, Value};
use sas_modulation::{RandomLfo, RandomLfoModule, TriggerRandom};
use serde_json::Value as Json;

fn maybe_clone(processor: ProcessorRef, params: &Json) -> ProcessorRef {
    if param_bool(params, "use_clone", false) {
        processor.clone_processor()
    } else {
        processor
    }
}

fn clock_start(params: &Json) -> Option<f64> {
    params.get("clock_start_seconds").and_then(Json::as_f64)
}

#[test]
fn random_lfo_matches_reference() {
    check_module(&ModuleDef::new(
        "random_lfo",
        |params| {
            let lfo = maybe_clone(RandomLfo::create(), params);
            lfo.set_control_rate(param_bool(params, "control_rate", false));
            match clock_start(params) {
                Some(start) => clocked(lfo, start, |lfo, seconds| {
                    lfo.with::<RandomLfo, _>(|kernel, _| kernel.correct_to_time(seconds))
                }),
                None => lfo,
            }
        },
        &[
            ("frequency", RandomLfo::FREQUENCY, Control),
            ("reset", RandomLfo::RESET, Trigger),
            ("sync", RandomLfo::SYNC, Control),
            ("style", RandomLfo::STYLE, Control),
            ("stereo", RandomLfo::STEREO, Control),
        ],
        &[("value", 0)],
    ));
}

#[test]
fn trigger_random_matches_reference() {
    check_module(&ModuleDef::new(
        "trigger_random",
        |params| maybe_clone(TriggerRandom::create(), params),
        &[("reset", TriggerRandom::RESET, Trigger)],
        &[("value", 0)],
    ));
}

#[test]
fn random_lfo_module_matches_reference() {
    check_module(&ModuleDef::new(
        "random_lfo_module",
        |params| {
            let beats_per_second =
                Value::create_control_rate(param(params, "beats_per_second", 2.0));
            let module = RandomLfoModule::create(
                param_str(params, "prefix", "random_1"),
                &beats_per_second.output(0),
            );
            module.init();
            set_controls(&module, params);
            match clock_start(params) {
                Some(start) => clocked(module, start, |module, seconds| {
                    module.correct_to_time(seconds)
                }),
                None => module,
            }
        },
        &[
            ("note_trigger", RandomLfoModule::NOTE_TRIGGER, Trigger),
            ("midi", RandomLfoModule::MIDI, Control),
        ],
        &[("value", 0)],
    ));
}
