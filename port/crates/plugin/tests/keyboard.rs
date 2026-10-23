// SPDX-License-Identifier: GPL-3.0-or-later

//! The on-screen keyboard through the audio callback: its notes merge into
//! the block's MIDI as upstream's `processKeyboardEvents` merges them, wait
//! while bypassed, light the held keys in the telemetry snapshot, and play
//! exactly as the same notes from the host.

use std::collections::VecDeque;

use nice_plug::midi::{Channel, Key, VoiceID};
use nice_plug::params::InternalParamMut;
use nice_plug::prelude::NoteEvent;
use sas_engine::telemetry::TelemetryWriter;
use sas_engine::Tuning;
use sas_plugin::params::ControlParam;
use sas_plugin::{Engine, SoundEngineHost, SynthPlugin, Transport};
use sas_preset::PresetState;

#[derive(Clone, Debug, PartialEq)]
enum Call {
    Midi([u8; 3], usize),
    Process(usize),
}

#[derive(Default)]
struct Recorder {
    calls: Vec<Call>,
}

impl Engine for Recorder {
    fn set_sample_rate(&mut self, _sample_rate: f32) {}
    fn reset(&mut self) {}
    fn load_preset(&mut self, _preset: &PresetState) {}
    fn set_tuning(&mut self, _tuning: &Tuning) {}
    fn set_control(&mut self, _name: &'static str, _value: f32) {}
    fn set_bpm(&mut self, _bpm: f64) {}
    fn correct_to_time(&mut self, _seconds: f64) {}
    fn process_midi(&mut self, message: [u8; 3], sample_offset: usize) {
        self.calls.push(Call::Midi(message, sample_offset));
    }
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        self.calls.push(Call::Process(left.len()));
    }
    fn publish_telemetry(&mut self, telemetry: &mut TelemetryWriter) {
        telemetry.publish();
    }
}

fn run<E: Engine>(plugin: &mut SynthPlugin<E>, samples: usize, events: Vec<NoteEvent<()>>) -> Vec<f32> {
    let params = plugin.synth_params().clone();
    let mut left = vec![0.0; samples];
    let mut right = vec![0.0; samples];
    let mut queue: VecDeque<_> = events.into();
    plugin
        .processor_mut()
        .process(&params, &mut left, &mut right, &Transport::default(), || {
            queue.pop_front()
        });
    left.extend(right);
    left
}

fn calls(plugin: &mut SynthPlugin<Recorder>, samples: usize, events: Vec<NoteEvent<()>>) -> Vec<Call> {
    plugin.processor_mut().engine_mut().calls.clear();
    run(plugin, samples, events);
    std::mem::take(&mut plugin.processor_mut().engine_mut().calls)
}

fn note_on(timing: u32, channel: u8, key: u8) -> NoteEvent<()> {
    NoteEvent::NoteOn {
        timing,
        voice_id: VoiceID::Wildcard,
        channel: Channel::Number(channel),
        key: Key::Number(key),
        velocity: 1.0,
    }
}

fn note_off(timing: u32, channel: u8, key: u8) -> NoteEvent<()> {
    NoteEvent::NoteOff {
        timing,
        voice_id: VoiceID::Wildcard,
        channel: Channel::Number(channel),
        key: Key::Number(key),
        velocity: 0.0,
    }
}

fn set_bypass<E: Engine>(plugin: &SynthPlugin<E>, on: bool) {
    let params = plugin.synth_params();
    let ControlParam::Bypass(param) = &params.control("bypass").unwrap().param else {
        panic!("bypass is a bool parameter");
    };
    unsafe { param._internal_set_plain_value(on) };
}

#[test]
fn keyboard_notes_follow_host_notes_at_the_same_sample() {
    let mut plugin = SynthPlugin::new(Recorder::default());
    run(&mut plugin, 16, vec![]);
    let keyboard = plugin.keyboard_handle();
    // 300 samples over 1002 + 1 - 1000 ms: 100 samples per ms.
    assert!(keyboard.note_on_at(60, 1.0, 1000));
    assert!(keyboard.note_on_at(62, 1.0, 1002));
    let events = vec![
        note_on(0, 0, 48),
        note_on(150, 0, 50),
        note_on(200, 0, 52),
        note_on(299, 0, 53),
    ];
    assert_eq!(
        calls(&mut plugin, 300, events),
        [
            Call::Midi([0x90, 48, 127], 0),
            Call::Midi([0x90, 60, 127], 0),
            Call::Process(128),
            Call::Midi([0x90, 50, 127], 22),
            Call::Midi([0x90, 52, 127], 72),
            Call::Midi([0x90, 62, 127], 72),
            Call::Process(128),
            Call::Midi([0x90, 53, 127], 43),
            Call::Process(44),
        ]
    );
}

#[test]
fn keyboard_notes_wait_while_bypassed_and_through_empty_blocks() {
    let mut plugin = SynthPlugin::new(Recorder::default());
    run(&mut plugin, 16, vec![]);
    set_bypass(&plugin, true);
    plugin.keyboard_handle().note_on_at(60, 0.5, 0);
    assert_eq!(calls(&mut plugin, 64, vec![]), []);
    set_bypass(&plugin, false);
    assert_eq!(calls(&mut plugin, 0, vec![]), []);
    assert_eq!(
        calls(&mut plugin, 64, vec![]),
        [Call::Midi([0x90, 60, 64], 0), Call::Process(64)]
    );
}

#[test]
fn held_notes_reach_the_telemetry_snapshot() {
    let mut plugin = SynthPlugin::new(Recorder::default());
    let telemetry = plugin.telemetry();
    let keyboard = plugin.keyboard_handle();
    keyboard.note_on(60, 1.0);
    run(&mut plugin, 32, vec![note_on(3, 1, 70)]);
    assert_eq!(telemetry.read(|snapshot| snapshot.held_notes()), 1 << 60 | 1 << 70);
    assert!(telemetry.read(|snapshot| snapshot.note_held(70)));

    keyboard.note_off(60);
    run(&mut plugin, 32, vec![note_off(0, 1, 70)]);
    assert_eq!(telemetry.read(|snapshot| snapshot.held_notes()), 0);
}

fn render_note(from_keyboard: bool) -> Vec<f32> {
    sas_common::random::set_next_seed(5);
    sas_engine::sas_oscillators::RandomValues::reset_process_state();
    let mut plugin = SynthPlugin::new(SoundEngineHost::new());
    let params = plugin.synth_params().clone();
    plugin.processor_mut().activate(&params, 44100.0);
    let keyboard = plugin.keyboard_handle();
    let mut output = run(&mut plugin, 256, vec![]);
    let (on, off) = if from_keyboard {
        keyboard.note_on(60, 1.0);
        (vec![], vec![])
    } else {
        (vec![note_on(0, 0, 60)], vec![NoteEvent::from_midi(0, &[0x80, 60, 0]).unwrap()])
    };
    output.extend(run(&mut plugin, 256, on));
    for _ in 0..20 {
        output.extend(run(&mut plugin, 256, vec![]));
    }
    if from_keyboard {
        keyboard.note_off(60);
    }
    output.extend(run(&mut plugin, 256, off));
    for _ in 0..20 {
        output.extend(run(&mut plugin, 256, vec![]));
    }
    output
}

#[test]
fn a_keyboard_note_plays_like_the_same_host_note() {
    let from_keyboard = render_note(true);
    assert!(from_keyboard.iter().any(|&sample| sample != 0.0));
    let from_host = render_note(false);
    assert!(
        from_keyboard.iter().zip(&from_host).all(|(a, b)| a.to_bits() == b.to_bits()),
        "the keyboard's note sounds different from the host's"
    );
}
