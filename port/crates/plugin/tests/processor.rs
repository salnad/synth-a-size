// SPDX-License-Identifier: GPL-3.0-or-later

//! The audio callback against a recording engine: preset and parameter
//! hand-off, 128-sample chunks, MIDI/MPE timing, transport and bypass.

use std::collections::VecDeque;

use nice_plug::midi::{Channel, Key, VoiceID};
use nice_plug::params::InternalParamMut;
use nice_plug::prelude::NoteEvent;
use sas_engine::Tuning;
use sas_plugin::params::ControlParam;
use sas_plugin::{state, BlockProcessor, Engine, SynthParams, SynthPlugin, Transport};
use sas_preset::PresetState;

#[derive(Clone, Debug, PartialEq)]
enum Call {
    SampleRate(f32),
    Reset,
    LoadPreset(String),
    Tuning(String),
    Control(&'static str, f32),
    Bpm(f64),
    Time(f64),
    Midi([u8; 3], usize),
    Process(usize),
}

#[derive(Default)]
struct Recorder {
    calls: Vec<Call>,
}

impl Engine for Recorder {
    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.calls.push(Call::SampleRate(sample_rate));
    }
    fn reset(&mut self) {
        self.calls.push(Call::Reset);
    }
    fn load_preset(&mut self, preset: &PresetState) {
        self.calls
            .push(Call::LoadPreset(preset.info.preset_name.clone()));
    }
    fn set_tuning(&mut self, tuning: &Tuning) {
        self.calls.push(Call::Tuning(tuning.name()));
    }
    fn set_control(&mut self, name: &'static str, value: f32) {
        self.calls.push(Call::Control(name, value));
    }
    fn set_bpm(&mut self, bpm: f64) {
        self.calls.push(Call::Bpm(bpm));
    }
    fn correct_to_time(&mut self, seconds: f64) {
        self.calls.push(Call::Time(seconds));
    }
    fn process_midi(&mut self, message: [u8; 3], sample_offset: usize) {
        self.calls.push(Call::Midi(message, sample_offset));
    }
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.25);
        right.fill(-0.25);
        self.calls.push(Call::Process(left.len()));
    }
}

struct Harness {
    plugin: SynthPlugin<Recorder>,
}

impl Harness {
    fn new() -> Harness {
        let mut plugin = SynthPlugin::new(Recorder::default());
        plugin.processor_mut().set_sample_rate(48_000.0);
        Harness { plugin }
    }

    fn params(&self) -> std::sync::Arc<SynthParams> {
        self.plugin.synth_params().clone()
    }

    fn processor(&mut self) -> &mut BlockProcessor<Recorder> {
        self.plugin.processor_mut()
    }

    /// Runs one block and returns the engine calls it made.
    fn run(
        &mut self,
        samples: usize,
        transport: Transport,
        events: Vec<NoteEvent<()>>,
    ) -> (Vec<Call>, Vec<f32>, Vec<f32>) {
        let params = self.params();
        let mut left = vec![1.0; samples];
        let mut right = vec![1.0; samples];
        let mut queue: VecDeque<_> = events.into();
        self.processor().engine_mut().calls.clear();
        self.processor()
            .process(&params, &mut left, &mut right, &transport, || {
                queue.pop_front()
            });
        assert!(queue.is_empty(), "every event is consumed");
        let calls = std::mem::take(&mut self.processor().engine_mut().calls);
        (calls, left, right)
    }

    fn set(&self, name: &str, value: f32) {
        let params = self.params();
        let ControlParam::Float(param) = &params.control(name).unwrap().param else {
            panic!("{name} is not a float parameter");
        };
        unsafe { param._internal_set_plain_value(value) };
    }

    fn set_bypass(&self, on: bool) {
        let params = self.params();
        let ControlParam::Bypass(param) = &params.control("bypass").unwrap().param else {
            panic!("bypass is a bool parameter");
        };
        unsafe { param._internal_set_plain_value(on) };
    }
}

fn controls(calls: &[Call]) -> Vec<(&'static str, f32)> {
    calls
        .iter()
        .filter_map(|call| match call {
            Call::Control(name, value) => Some((*name, *value)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_constructed_engine_is_the_init_state_then_only_changes_follow() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.processor().engine().calls,
        [Call::SampleRate(48_000.0)]
    );
    // The engine starts as constructed, which the init document describes
    // (upstream's plugin never calls loadInitPreset): nothing is loaded.
    let (calls, ..) = harness.run(64, Transport::default(), vec![]);
    assert!(!calls.iter().any(|call| matches!(call, Call::LoadPreset(_))));
    assert!(
        controls(&calls).is_empty(),
        "the parameters match the preset"
    );

    let (calls, ..) = harness.run(64, Transport::default(), vec![]);
    assert!(controls(&calls).is_empty());

    harness.set("filter_1_cutoff", 90.0);
    harness.set("osc_2_level", 0.25);
    let (calls, ..) = harness.run(64, Transport::default(), vec![]);
    let mut pushed = controls(&calls);
    pushed.sort_by(|a, b| a.0.cmp(b.0));
    assert_eq!(pushed, [("filter_1_cutoff", 90.0), ("osc_2_level", 0.25)]);
}

#[test]
fn restored_state_reloads_the_preset_and_every_control() {
    let mut harness = Harness::new();
    harness.run(16, Transport::default(), vec![]);

    let mut preset = PresetState::new(&sas_preset::InitContent { sample_seed: 0 });
    preset.info.preset_name = "restored".to_string();
    preset.set_value("filter_1_cutoff", 33.0).unwrap();
    let state = state::plugin_state_from_json(&preset.to_json_string()).unwrap();
    harness.params().restore_state(state);
    // Host automation between the restore and the next block.
    harness.set("osc_1_level", 0.125);

    let (calls, ..) = harness.run(16, Transport::default(), vec![]);
    assert_eq!(calls[0], Call::LoadPreset("restored".to_string()));
    // The tuning follows the preset, as in setStateInformation.
    assert_eq!(calls[1], Call::Tuning(String::new()));
    assert_eq!(controls(&calls), [("osc_1_level", 0.125)]);
    assert_eq!(
        harness
            .params()
            .control("filter_1_cutoff")
            .unwrap()
            .param
            .value(),
        33.0
    );
}

#[test]
fn a_stepped_parameter_leaves_the_presets_own_value_in_the_engine() {
    let mut harness = Harness::new();
    harness.run(16, Transport::default(), vec![]);
    let mut preset = PresetState::new(&sas_preset::InitContent { sample_seed: 0 });
    preset.set_value("osc_1_transpose", 0.25).unwrap();
    let state = state::plugin_state_from_json(&preset.to_json_string()).unwrap();
    harness.params().restore_state(state);
    let (calls, ..) = harness.run(16, Transport::default(), vec![]);
    // The engine keeps 0.25 from load_preset; the host shows step 0.
    assert!(controls(&calls).is_empty());

    let params = harness.params();
    let ControlParam::Stepped(transpose) = &params.control("osc_1_transpose").unwrap().param else {
        panic!("stepped parameter");
    };
    unsafe { transpose._internal_set_plain_value(7) };
    let (calls, ..) = harness.run(16, Transport::default(), vec![]);
    assert_eq!(controls(&calls), [("osc_1_transpose", 7.0)]);
}

#[test]
fn a_control_moved_away_and_back_before_the_reload_reaches_the_engine() {
    let mut harness = Harness::new();
    harness.run(16, Transport::default(), vec![]);
    let mut preset = PresetState::new(&sas_preset::InitContent { sample_seed: 0 });
    preset.set_value("osc_1_transpose", 0.25).unwrap();
    let state = state::plugin_state_from_json(&preset.to_json_string()).unwrap();
    harness.params().restore_state(state);
    let params = harness.params();
    let ControlParam::Stepped(transpose) = &params.control("osc_1_transpose").unwrap().param else {
        panic!("stepped parameter");
    };
    unsafe {
        transpose._internal_set_plain_value(1);
        transpose._internal_set_plain_value(0);
    }
    let (calls, ..) = harness.run(16, Transport::default(), vec![]);
    assert_eq!(calls[0], Call::LoadPreset(String::new()));
    assert_eq!(controls(&calls), [("osc_1_transpose", 0.0)]);
}

#[test]
fn renders_in_chunks_of_at_most_128_with_midi_offsets_inside_each() {
    let mut harness = Harness::new();
    harness.run(16, Transport::default(), vec![]);
    let note_on = |timing, key| NoteEvent::NoteOn {
        timing,
        voice_id: VoiceID::Wildcard,
        channel: Channel::Number(0),
        key: Key::Number(key),
        velocity: 1.0,
    };
    let (calls, left, right) = harness.run(
        300,
        Transport::default(),
        vec![note_on(0, 60), note_on(130, 62), note_on(299, 64)],
    );
    assert!(left.iter().all(|&sample| sample == 0.25));
    assert!(right.iter().all(|&sample| sample == -0.25));
    // The clock keeps running from the 16-sample warm-up block.
    let sample_time = 1.0 / 48_000.0f64;
    let t0 = 0.0 + 16.0 * sample_time;
    let t1 = t0 + 128.0 * sample_time;
    let t2 = t1 + 128.0 * sample_time;
    assert_eq!(
        calls,
        [
            Call::Time(t0),
            Call::Midi([0x90, 60, 127], 0),
            Call::Process(128),
            Call::Time(t1),
            Call::Midi([0x90, 62, 127], 2),
            Call::Process(128),
            Call::Time(t2),
            Call::Midi([0x90, 64, 127], 43),
            Call::Process(44),
        ]
    );
}

#[test]
fn transport_sets_tempo_and_song_position_while_playing() {
    let mut harness = Harness::new();
    harness.run(16, Transport::default(), vec![]);
    let playing = Transport {
        tempo: Some(120.0),
        playing: true,
        pos_beats: Some(8.0),
    };
    let (calls, ..) = harness.run(32, playing, vec![]);
    assert_eq!(
        calls,
        [Call::Bpm(120.0), Call::Time(4.0), Call::Process(32)]
    );

    // Stopped: the tempo still applies, the clock keeps running from where
    // it was.
    let stopped = Transport {
        playing: false,
        pos_beats: Some(0.0),
        ..playing
    };
    let (calls, ..) = harness.run(32, stopped, vec![]);
    assert_eq!(
        calls,
        [
            Call::Bpm(120.0),
            Call::Time(4.0 + 32.0 / 48_000.0),
            Call::Process(32)
        ]
    );
}

#[test]
fn mpe_arrives_as_per_channel_midi() {
    let mut harness = Harness::new();
    harness.run(16, Transport::default(), vec![]);
    let events = vec![
        NoteEvent::NoteOn {
            timing: 0,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(1),
            key: Key::Number(60),
            velocity: 0.5,
        },
        NoteEvent::MidiPitchBend {
            timing: 1,
            channel: 1,
            value: 0.5,
        },
        NoteEvent::MidiCC {
            timing: 2,
            channel: 1,
            cc: 74,
            value: 1.0,
        },
        NoteEvent::MidiChannelPressure {
            timing: 3,
            channel: 1,
            pressure: 1.0,
        },
        NoteEvent::PolyPressure {
            timing: 4,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(2),
            key: Key::Number(61),
            pressure: 1.0,
        },
        // Note expressions without a MIDI equivalent are not forwarded
        // (upstream's JUCE wrapper has none).
        NoteEvent::PolyTuning {
            timing: 5,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(1),
            key: Key::Number(60),
            tuning: 0.5,
        },
        NoteEvent::NoteOff {
            timing: 6,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(1),
            key: Key::Number(60),
            velocity: 0.0,
        },
    ];
    let (calls, ..) = harness.run(16, Transport::default(), events);
    let midi: Vec<_> = calls
        .into_iter()
        .filter(|call| matches!(call, Call::Midi(..)))
        .collect();
    assert_eq!(
        midi,
        [
            Call::Midi([0x91, 60, 64], 0),
            Call::Midi([0xE1, 0, 64], 1),
            Call::Midi([0xB1, 74, 127], 2),
            Call::Midi([0xD1, 127, 0], 3),
            Call::Midi([0xA2, 61, 127], 4),
            Call::Midi([0x81, 60, 0], 6),
        ]
    );
}

#[test]
fn bypass_outputs_silence_and_drops_midi() {
    let mut harness = Harness::new();
    harness.run(16, Transport::default(), vec![]);
    harness.set_bypass(true);
    let (calls, left, right) = harness.run(
        200,
        Transport::default(),
        vec![NoteEvent::NoteOn {
            timing: 3,
            voice_id: VoiceID::Wildcard,
            channel: Channel::Number(0),
            key: Key::Number(60),
            velocity: 1.0,
        }],
    );
    // The bypass control still reaches the engine (as upstream's
    // ValueBridge sets it); nothing is rendered and the MIDI is dropped.
    assert_eq!(calls, [Call::Control("bypass", 1.0)]);
    assert!(left.iter().chain(&right).all(|&sample| sample == 0.0));

    harness.set_bypass(false);
    let (calls, ..) = harness.run(16, Transport::default(), vec![]);
    assert_eq!(controls(&calls), [("bypass", 0.0)]);
}

#[test]
fn activation_prepares_the_engine_in_upstreams_order_with_restores() {
    let restored = || {
        let mut preset = PresetState::new(&sas_preset::InitContent { sample_seed: 0 });
        preset.info.preset_name = "restored".to_string();
        state::plugin_state_from_json(&preset.to_json_string()).unwrap()
    };
    let prepared_after_load = [
        Call::LoadPreset("restored".to_string()),
        Call::Tuning(String::new()),
        Call::SampleRate(44_100.0),
    ];

    // A host opening a project restores the state, then activates: the
    // preset loads, then the engine is prepared (setStateInformation, then
    // prepareToPlay).
    let mut plugin = SynthPlugin::new(Recorder::default());
    let params = plugin.synth_params().clone();
    params.restore_state(restored());
    plugin.processor_mut().activate(&params, 44_100.0);
    let (mut left, mut right) = (vec![0.0; 16], vec![0.0; 16]);
    plugin.processor_mut().process(
        &params,
        &mut left,
        &mut right,
        &Transport::default(),
        || None::<NoteEvent<()>>,
    );
    assert_eq!(plugin.processor().engine().calls[..3], prepared_after_load);

    // A state restored after activation loads into the prepared engine.
    let mut plugin = SynthPlugin::new(Recorder::default());
    let params = plugin.synth_params().clone();
    plugin.processor_mut().activate(&params, 44_100.0);
    params.restore_state(restored());
    plugin.processor_mut().process(
        &params,
        &mut left,
        &mut right,
        &Transport::default(),
        || None::<NoteEvent<()>>,
    );
    assert_eq!(
        plugin.processor().engine().calls[..3],
        [
            Call::SampleRate(44_100.0),
            Call::LoadPreset("restored".to_string()),
            Call::Tuning(String::new()),
        ]
    );
}

#[test]
fn reset_reaches_the_engine() {
    let mut harness = Harness::new();
    harness.processor().reset();
    assert_eq!(
        harness.processor().engine().calls.last(),
        Some(&Call::Reset)
    );
}

#[test]
fn the_host_tempo_is_kept_for_work_off_the_audio_thread() {
    let mut harness = Harness::new();
    assert_eq!(harness.params().host_tempo(), 120.0);
    let at_140 = Transport {
        tempo: Some(140.0),
        playing: false,
        pos_beats: None,
    };
    harness.run(32, at_140, vec![]);
    assert_eq!(harness.params().host_tempo(), 140.0);
    // A block without a tempo keeps the last one.
    harness.run(32, Transport::default(), vec![]);
    assert_eq!(harness.params().host_tempo(), 140.0);
}
