// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `SynthVoiceHandler` against the full upstream handler for every
//! `engine/synth_voice_handler/*` case
//! (`migration/corpus/modules/engine/synth_voice_handler.json`).
//!
//! Mirrors `migration/reference/src/modules/engine/synth_voice_handler.cpp`:
//! the handler is built and initialised as `SoundEngine` does, the case's
//! controls are set, and the event script is replayed block by block.
//! `hold_released_voices` sets `env_1_release` to its maximum, as the
//! reference does, so the amplitude envelope frees no released voice within
//! the case.

use sas_dsp_core::{ProcessorRef, Value};
use sas_engine::synth_voice_handler::SynthVoiceHandler;
use sas_engine::voice_handler::{with_voice_handler, NoteHandler, VoiceHandler, MAX_POLYPHONY};
use sas_parity::{all_case_ids, ModuleCase};
use serde_json::Value as Json;

const STATUS_NAMES: [&str; 14] = [
    "note",
    "note_in_octave",
    "aftertouch",
    "velocity",
    "slide",
    "lift",
    "mod_wheel",
    "pitch_wheel",
    "stereo",
    "num_voices",
    "macro_control_1",
    "macro_control_2",
    "macro_control_3",
    "macro_control_4",
];

fn num(event: &Json, key: &str, fallback: f32) -> f32 {
    event
        .get(key)
        .and_then(Json::as_f64)
        .map_or(fallback, |v| v as f32)
}

fn inum(event: &Json, key: &str) -> i32 {
    event.get(key).and_then(Json::as_i64).unwrap_or(0) as i32
}

fn apply_event(node: &ProcessorRef, event: &Json, offset: i32) {
    let kind = event["type"].as_str().unwrap_or_default();
    let channel = inum(event, "channel");
    let value = num(event, "value", 0.0);
    node.with::<SynthVoiceHandler, _>(|svh, _| match kind {
        "note_on" => svh.note_on(
            inum(event, "note"),
            num(event, "velocity", 1.0),
            offset,
            channel,
        ),
        "note_off" => svh.note_off(
            inum(event, "note"),
            num(event, "lift", 0.5),
            offset,
            channel,
        ),
        _ => {
            let vh = sas_engine::voice_handler::HasVoiceHandler::voice_handler(svh);
            match kind {
                "sustain_on" => vh.sustain_on(channel),
                "sustain_off" => vh.sustain_off(offset, channel),
                "aftertouch" => vh.set_aftertouch(inum(event, "note"), value, offset, channel),
                "channel_aftertouch" => vh.set_channel_aftertouch(channel, value, offset),
                "channel_slide" => vh.set_channel_slide(channel, value, offset),
                "mod_wheel" => vh.set_mod_wheel(value, channel),
                "pitch_wheel" => vh.set_pitch_wheel(value, channel),
                "zoned_pitch_wheel" => {
                    vh.set_zoned_pitch_wheel(value, inum(event, "from"), inum(event, "to"))
                }
                other => panic!("unknown event type {other:?}"),
            }
        }
    });
}

fn run_case(case: &ModuleCase) -> (Vec<f32>, Vec<f32>) {
    sas_engine::parameters::install();
    let params = case.params();
    let oversample = case.oversample() as i64;
    let block_size = case.block_size() as i64;
    let num_samples = params["num_samples"].as_i64().expect("params.num_samples");

    let beats_per_second = Value::create_control_rate(num(params, "beats_per_second", 2.0));
    let node = SynthVoiceHandler::create(&beats_per_second.output(0));
    with_voice_handler::<SynthVoiceHandler, _>(&node, |vh, _| vh.set_polyphony(MAX_POLYPHONY));

    let inputs = [
        Value::create_control_rate(num(params, "polyphony", 8.0)),
        Value::create_control_rate(num(params, "voice_priority", 4.0)),
        Value::create_control_rate(num(params, "voice_override", 0.0)),
    ];
    node.plug(&inputs[0], VoiceHandler::POLYPHONY);
    node.plug(&inputs[1], VoiceHandler::VOICE_PRIORITY);
    node.plug(&inputs[2], VoiceHandler::VOICE_OVERRIDE);

    node.init();
    node.with::<SynthVoiceHandler, _>(|svh, _| svh.disable_unnecessary_mod_sources());
    node.set_oversample_amount(oversample as i32);
    node.set_sample_rate(case.sample_rate() as i32);

    let controls = node.get_controls();
    if params
        .get("hold_released_voices")
        .and_then(Json::as_bool)
        .unwrap_or(false)
    {
        let max = sas_preset::lookup("env_1_release")
            .expect("env_1_release")
            .max;
        controls["env_1_release"].set_value(max);
    }
    if let Some(values) = params.get("controls").and_then(Json::as_object) {
        for (name, value) in values {
            let control = controls
                .get(name)
                .unwrap_or_else(|| panic!("{}: no control {name:?}", case.id()));
            control.set_value(value.as_f64().unwrap() as f32);
        }
    }
    node.update_all_modulation_switches();
    let legato = params
        .get("legato")
        .and_then(Json::as_bool)
        .unwrap_or(false);

    let mut events: Vec<Json> = params
        .get("events")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    events.sort_by_key(|e| e["sample"].as_i64().unwrap_or(0));

    let mut block_state = Vec::new();
    let mut status = Vec::new();
    let mut next_event = 0;
    let mut block_start = 0;
    while block_start < num_samples {
        let n = block_size.min(num_samples - block_start);
        let base_start = block_start / oversample;
        let base_end = (block_start + n + oversample - 1) / oversample;
        with_voice_handler::<SynthVoiceHandler, _>(&node, |vh, _| vh.set_legato(legato));
        while next_event < events.len()
            && events[next_event]["sample"].as_i64().unwrap_or(0) < base_end
        {
            let sample = events[next_event]["sample"].as_i64().unwrap_or(0);
            let offset = (sample - base_start).max(0) as i32;
            apply_event(&node, &events[next_event], offset);
            next_event += 1;
        }

        let (retrigger, midi_offset) = node.with::<SynthVoiceHandler, _>(|svh, _| {
            (svh.note_retrigger(), svh.midi_offset_output())
        });
        let mut retrigger_state = Vec::with_capacity(12);
        retrigger_state.extend(retrigger.trigger_mask().to_array().map(|m| {
            if m != 0 {
                1.0
            } else {
                0.0
            }
        }));
        retrigger_state.extend(retrigger.trigger_value().to_array());
        retrigger_state.extend(retrigger.trigger_offset().to_array().map(|o| o as f32));

        node.process(n as usize);

        with_voice_handler::<SynthVoiceHandler, _>(&node, |vh, _| {
            block_state.push(vh.num_active_voices() as f32);
            block_state.push(vh.num_pressed_notes() as f32);
        });
        block_state.extend(midi_offset.at(0).to_array());
        block_state.extend(retrigger_state);

        for name in STATUS_NAMES {
            let output = node
                .get_status_output(name)
                .unwrap_or_else(|| panic!("no status output {name:?}"));
            status.extend(output.value().to_array());
        }
        block_start += n;
    }
    (block_state, status)
}

#[test]
fn synth_voice_handler_matches_reference() {
    let ids: Vec<String> = all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with("engine/synth_voice_handler/"))
        .collect();
    assert!(
        ids.len() >= 8,
        "expected the synth_voice_handler cases, found {ids:?}"
    );
    for id in &ids {
        let case = ModuleCase::load(id);
        let (block_state, status) = run_case(&case);
        case.assert_parity("block_state", &block_state);
        case.assert_parity("status", &status);
    }
}
