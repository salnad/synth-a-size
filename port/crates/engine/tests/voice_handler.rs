// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of the `VoiceHandler` port against the C++ reference for every
//! `engine/voice_handler/*` case (`migration/corpus/modules/engine/voice_handler.json`).
//!
//! Builds the same graph as `migration/reference/src/modules/engine/voice_handler.cpp`
//! (a probe of the 18 per-voice outputs whose envelope is the voice killer,
//! plus a `cr::Multiply(note, 1/127)` last-voice output), replays the case's
//! event script block by block, and compares every array the reference
//! emits under the case tolerances.

use std::cell::RefCell;
use std::rc::Rc;

use sas_dsp_core::operators::cr;
use sas_dsp_core::{Output, Processor, ProcessorImpl, ProcessorRef, Value};
use sas_engine::voice_handler::{
    self, with_voice_handler, NoteHandler, VoiceHandler, MAX_POLYPHONY,
};
use sas_engine::Tuning;
use sas_parity::{all_case_ids, ModuleCase};
use sas_simd::common::{VoiceEvent, MIDI_SIZE};
use sas_simd::PolyFloat;
use serde_json::Value as Json;

const NUM_PROBED: usize = 18;

#[derive(Default)]
struct ProbeLog {
    values: Vec<f32>,
    masks: Vec<f32>,
    offsets: Vec<f32>,
}

/// `VoiceProbe` of the reference module.
#[derive(Clone)]
struct VoiceProbe {
    log: Rc<RefCell<ProbeLog>>,
    amp: [f32; 4],
    decay: [f32; 4],
    release_step: f32,
    kill_step: f32,
}

impl ProcessorImpl for VoiceProbe {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        {
            let mut log = self.log.borrow_mut();
            for i in 0..NUM_PROBED {
                let input = this.input(i);
                let value = input.trigger_value().to_array();
                let mask = input.trigger_mask().to_array();
                let offset = input.trigger_offset().to_array();
                for lane in 0..4 {
                    log.values.push(value[lane]);
                    log.masks.push(if mask[lane] != 0 { 1.0 } else { 0.0 });
                    log.offsets.push(offset[lane] as f32);
                }
            }
        }

        let event = this.input(0);
        let (mask, value, offset) = (
            event.trigger_mask().to_array(),
            event.trigger_value().to_array(),
            event.trigger_offset().to_array(),
        );
        let dest = this.output_buffer(0);
        for (s, sample) in dest.iter().enumerate().take(num_samples) {
            let mut out = PolyFloat::zero();
            for lane in 0..4 {
                if mask[lane] != 0 && offset[lane] as i32 as usize == s {
                    let trigger = value[lane];
                    if trigger == VoiceEvent::VoiceOn.as_f32() {
                        self.amp[lane] = 1.0;
                        self.decay[lane] = 0.0;
                    } else if trigger == VoiceEvent::VoiceOff.as_f32() {
                        self.decay[lane] = self.release_step;
                    } else if trigger == VoiceEvent::VoiceKill.as_f32() {
                        self.decay[lane] = self.kill_step;
                    }
                }
                self.amp[lane] = 0.0f32.max(self.amp[lane] - self.decay[lane]);
                out.set(lane, self.amp[lane]);
            }
            sample.set(out);
        }
    }
}

fn num(event: &Json, key: &str, fallback: f32) -> f32 {
    event
        .get(key)
        .and_then(Json::as_f64)
        .map_or(fallback, |v| v as f32)
}

fn inum(event: &Json, key: &str) -> i32 {
    event.get(key).and_then(Json::as_i64).unwrap_or(0) as i32
}

fn apply_event(node: &ProcessorRef, inputs: &[ProcessorRef; 3], event: &Json, offset: i32) {
    let kind = event["type"].as_str().unwrap_or_default();
    let channel = inum(event, "channel");
    let from = inum(event, "from");
    let to = inum(event, "to");
    let value = num(event, "value", 0.0);
    let set_input = |index: usize| inputs[index].set_value(value);

    match kind {
        "polyphony" => return set_input(VoiceHandler::POLYPHONY),
        "voice_priority" => return set_input(VoiceHandler::VOICE_PRIORITY),
        "voice_override" => return set_input(VoiceHandler::VOICE_OVERRIDE),
        _ => {}
    }

    with_voice_handler::<VoiceHandler, _>(node, |vh, _| match kind {
        "note_on" => vh.note_on(
            inum(event, "note"),
            num(event, "velocity", 1.0),
            offset,
            channel,
        ),
        "note_off" => vh.note_off(
            inum(event, "note"),
            num(event, "lift", 0.5),
            offset,
            channel,
        ),
        "sustain_on" => vh.sustain_on(channel),
        "sustain_off" => vh.sustain_off(offset, channel),
        "sostenuto_on" => vh.sostenuto_on(channel),
        "sostenuto_off" => vh.sostenuto_off(offset, channel),
        "sustain_on_range" => vh.sustain_on_range(from, to),
        "sustain_off_range" => vh.sustain_off_range(offset, from, to),
        "sostenuto_on_range" => vh.sostenuto_on_range(from, to),
        "sostenuto_off_range" => vh.sostenuto_off_range(offset, from, to),
        "all_notes_off" if event.get("channel").is_some() => {
            vh.all_notes_off_channel(offset, channel)
        }
        "all_notes_off" => vh.all_notes_off(offset),
        "all_notes_off_range" => vh.all_notes_off_range(offset, from, to),
        "all_sounds_off" => vh.all_sounds_off(),
        "aftertouch" => vh.set_aftertouch(inum(event, "note"), value, offset, channel),
        "channel_aftertouch" => vh.set_channel_aftertouch(channel, value, offset),
        "channel_range_aftertouch" => vh.set_channel_range_aftertouch(from, to, value, offset),
        "channel_slide" => vh.set_channel_slide(channel, value, offset),
        "channel_range_slide" => vh.set_channel_range_slide(from, to, value, offset),
        "mod_wheel" => vh.set_mod_wheel(value, channel),
        "mod_wheel_all" => vh.set_mod_wheel_all_channels(value),
        "pitch_wheel" => vh.set_pitch_wheel(value, channel),
        "zoned_pitch_wheel" => vh.set_zoned_pitch_wheel(value, from, to),
        "legato" => vh.set_legato(value != 0.0),
        other => panic!("unknown event type {other:?}"),
    });
}

struct Captured {
    voice_values: Vec<f32>,
    voice_masks: Vec<f32>,
    voice_offsets: Vec<f32>,
    block_state: Vec<f32>,
    audio: Vec<f32>,
    note_percent: Vec<f32>,
}

fn run_case(case: &ModuleCase) -> Captured {
    let params = case.params();
    let oversample = case.oversample() as i64;
    let block_size = case.block_size() as i64;
    let num_samples = params["num_samples"].as_i64().expect("params.num_samples");

    let node = VoiceHandler::create(0, MAX_POLYPHONY, false);
    let inputs = [
        Value::create_control_rate(num(params, "polyphony", 8.0)),
        Value::create_control_rate(num(params, "voice_priority", 4.0)),
        Value::create_control_rate(num(params, "voice_override", 0.0)),
    ];
    for (i, input) in inputs.iter().enumerate() {
        node.plug(input, i);
    }

    let log = Rc::new(RefCell::new(ProbeLog::default()));
    let probe = Processor::new(
        VoiceProbe {
            log: log.clone(),
            amp: [0.0; 4],
            decay: [0.0; 4],
            release_step: 1.0 / num(params, "release_samples", 300.0),
            kill_step: 1.0 / num(params, "kill_samples", 40.0),
        },
        NUM_PROBED,
        1,
        false,
        1,
    );
    let probed: Vec<Rc<Output>> = with_voice_handler::<VoiceHandler, _>(&node, |vh, _| {
        vec![
            vh.voice_event(),
            vh.retrigger(),
            vh.reset_output(),
            vh.note(),
            vh.last_note(),
            vh.note_pressed(),
            vh.note_count(),
            vh.note_in_octave(),
            vh.channel(),
            vh.velocity(),
            vh.lift(),
            vh.aftertouch(),
            vh.slide(),
            vh.active_mask(),
            vh.mod_wheel(),
            vh.pitch_wheel(),
            vh.pitch_wheel_percent(),
            vh.local_pitch_bend(),
        ]
    });
    for (i, output) in probed.iter().enumerate() {
        probe.plug(output, i);
    }
    voice_handler::add_processor::<VoiceHandler>(&node, &probe);

    let max_midi_invert = Value::create_control_rate(1.0 / (MIDI_SIZE - 1) as f32);
    let note_percent = cr::Multiply::create();
    note_percent.plug(&max_midi_invert, 0);
    note_percent.plug(&probed[3], 1);
    voice_handler::add_processor::<VoiceHandler>(&node, &note_percent);

    let probe_output = probe.output(0);
    let audio = voice_handler::register_output::<VoiceHandler>(
        &node,
        &probe_output,
        VoiceHandler::should_accumulate(&probe_output),
    );
    let note_out = voice_handler::register_control_rate_output::<VoiceHandler>(
        &node,
        &note_percent.output(0),
        true,
    );
    with_voice_handler::<VoiceHandler, _>(&node, |vh, _| {
        vh.set_voice_killer(Some(probe_output.clone()));
        vh.set_legato(
            params
                .get("legato")
                .and_then(Json::as_bool)
                .unwrap_or(false),
        );
        if let Some(lines) = params.get("tuning_scala").and_then(Json::as_array) {
            let lines: Vec<String> = lines
                .iter()
                .map(|l| l.as_str().unwrap().to_string())
                .collect();
            let mut tuning = Tuning::new();
            tuning.load_scala_lines(&lines);
            vh.set_tuning(Some(Rc::new(tuning)));
        }
    });

    node.init();
    node.set_sample_rate(case.sample_rate() as i32);
    node.set_oversample_amount(oversample as i32);
    *log.borrow_mut() = ProbeLog::default();

    let mut events: Vec<Json> = params
        .get("events")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    events.sort_by_key(|e| e["sample"].as_i64().unwrap_or(0));

    let mut block_state = Vec::new();
    let mut audio_capture = Vec::new();
    let mut note_capture = Vec::new();
    let mut next_event = 0;
    let mut block_start = 0;
    while block_start < num_samples {
        let n = block_size.min(num_samples - block_start);
        let base_start = block_start / oversample;
        let base_end = (block_start + n + oversample - 1) / oversample;
        while next_event < events.len()
            && events[next_event]["sample"].as_i64().unwrap_or(0) < base_end
        {
            let sample = events[next_event]["sample"].as_i64().unwrap_or(0);
            let offset = (sample - base_start).max(0) as i32;
            apply_event(&node, &inputs, &events[next_event], offset);
            next_event += 1;
        }

        node.process(n as usize);

        with_voice_handler::<VoiceHandler, _>(&node, |vh, _| {
            let mask = vh.current_voice_mask().to_array();
            block_state.push(vh.num_active_voices() as f32);
            block_state.push(vh.num_pressed_notes() as f32);
            block_state.push(vh.last_active_note());
            for lane in mask {
                block_state.push(if lane != 0 { 1.0 } else { 0.0 });
            }
        });

        let audio_buffer = audio.buffer();
        let note_value = note_out.at(0).to_array();
        for sample in audio_buffer.iter().take(n as usize) {
            audio_capture.extend(sample.get().to_array());
            note_capture.extend(note_value);
        }
        block_start += n;
    }

    let log = log.borrow();
    Captured {
        voice_values: log.values.clone(),
        voice_masks: log.masks.clone(),
        voice_offsets: log.offsets.clone(),
        block_state,
        audio: audio_capture,
        note_percent: note_capture,
    }
}

#[test]
fn voice_handler_matches_reference() {
    let ids: Vec<String> = all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with("engine/voice_handler/"))
        .collect();
    assert!(
        ids.len() >= 20,
        "expected the voice_handler cases, found {ids:?}"
    );
    for id in &ids {
        let case = ModuleCase::load(id);
        let captured = run_case(&case);
        case.assert_parity("voice_values", &captured.voice_values);
        case.assert_parity("voice_masks", &captured.voice_masks);
        case.assert_parity("voice_offsets", &captured.voice_offsets);
        case.assert_parity("block_state", &captured.block_state);
        case.assert_parity("audio", &captured.audio);
        case.assert_parity("note_percent", &captured.note_percent);
    }
}
