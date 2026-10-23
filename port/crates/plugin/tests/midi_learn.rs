// SPDX-License-Identifier: GPL-3.0-or-later

//! MIDI learn through the wrapper: editor requests, learned controllers
//! moving host parameters, and the user config file.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use nice_plug::prelude::NoteEvent;
use sas_engine::midi_manager::MidiLearnMap;
use sas_engine::Tuning;
use sas_plugin::{
    config, BlockOutcome, Engine, MidiLearn, SoundEngineHost, SynthPlugin, Transport,
};
use sas_preset::{PresetState, SampleState};

/// The sound engine, recording the host-side `set_control` calls.
#[derive(Default)]
struct Recording {
    inner: SoundEngineHost,
    controls: Vec<(&'static str, f32)>,
}

impl Engine for Recording {
    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.inner.set_sample_rate(sample_rate);
    }
    fn reset(&mut self) {
        self.inner.reset();
    }
    fn load_preset(&mut self, preset: &PresetState) {
        self.inner.load_preset(preset);
    }
    fn set_tuning(&mut self, tuning: &Tuning) {
        self.inner.set_tuning(tuning);
    }
    fn set_control(&mut self, name: &'static str, value: f32) {
        self.controls.push((name, value));
        self.inner.set_control(name, value);
    }
    fn set_bpm(&mut self, bpm: f64) {
        self.inner.set_bpm(bpm);
    }
    fn correct_to_time(&mut self, seconds: f64) {
        self.inner.correct_to_time(seconds);
    }
    fn process_midi(&mut self, message: [u8; 3], sample_offset: usize) {
        self.inner.process_midi(message, sample_offset);
    }
    fn engine_sample(&self) -> Option<SampleState> {
        self.inner.engine_sample()
    }
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.inner.process(left, right);
    }
    fn arm_midi_learn(&mut self, name: &str) {
        self.inner.arm_midi_learn(name);
    }
    fn cancel_midi_learn(&mut self) {
        self.inner.cancel_midi_learn();
    }
    fn clear_midi_learn(&mut self, name: &str) -> bool {
        self.inner.clear_midi_learn(name)
    }
    fn midi_learn_map(&self) -> Option<&MidiLearnMap> {
        self.inner.midi_learn_map()
    }
    fn set_midi_learn_map(&mut self, midi_learn_map: MidiLearnMap) {
        self.inner.set_midi_learn_map(midi_learn_map);
    }
    fn take_midi_learn_changed(&mut self) -> bool {
        self.inner.take_midi_learn_changed()
    }
    fn drain_midi_value_changes(&mut self, f: &mut dyn FnMut(&'static str, f32)) {
        self.inner.drain_midi_value_changes(f);
    }
}

struct Harness {
    plugin: SynthPlugin<Recording>,
    midi_learn: Arc<MidiLearn>,
}

impl Harness {
    fn new(midi_learn: MidiLearn) -> Harness {
        let midi_learn = Arc::new(midi_learn);
        let mut plugin = SynthPlugin::with_midi_learn(Recording::default(), midi_learn.clone());
        plugin.processor_mut().set_sample_rate(44_100.0);
        Harness { plugin, midi_learn }
    }

    fn block(&mut self, events: Vec<NoteEvent<()>>) -> BlockOutcome {
        let params = self.plugin.synth_params().clone();
        let (mut left, mut right) = (vec![0.0; 64], vec![0.0; 64]);
        let mut queue: VecDeque<_> = events.into();
        self.plugin.processor_mut().engine_mut().controls.clear();
        self.plugin.processor_mut().process(
            &params,
            &mut left,
            &mut right,
            &Transport::default(),
            || queue.pop_front(),
        )
    }

    fn param(&self, name: &str) -> f32 {
        self.plugin
            .synth_params()
            .control(name)
            .unwrap()
            .param
            .value()
    }

    fn host_controls(&mut self) -> Vec<(&'static str, f32)> {
        std::mem::take(&mut self.plugin.processor_mut().engine_mut().controls)
    }
}

fn cc(controller: u8, value: u8) -> NoteEvent<()> {
    NoteEvent::MidiCC {
        timing: 0,
        channel: 0,
        cc: controller,
        value: value as f32 / 127.0,
    }
}

fn temp_config(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sas-midi-learn-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("test.config")
}

#[test]
fn a_learned_controller_moves_the_host_parameter() {
    let mut harness = Harness::new(MidiLearn::in_memory());
    harness.block(vec![]);
    harness.midi_learn.arm("filter_1_cutoff");
    assert!(!harness.midi_learn.is_midi_mapped("filter_1_cutoff"));

    let outcome = harness.block(vec![cc(20, 64)]);
    assert!(outcome.save_midi_learn);
    assert!(harness.midi_learn.is_midi_mapped("filter_1_cutoff"));
    assert_eq!(harness.midi_learn.learned_ccs("filter_1_cutoff"), [20]);
    let expected = 64.0 / 127.0 * 128.0 + 8.0;
    assert_eq!(harness.param("filter_1_cutoff"), expected);

    // The parameter change is not sent back to the engine as a host edit.
    harness.block(vec![]);
    assert_eq!(harness.host_controls(), []);

    // Saved states keep the learned value.
    let saved = harness.plugin.synth_params().preset_state();
    assert_eq!(saved.value("filter_1_cutoff"), Some(expected));

    // No new learn, nothing to save.
    assert!(!harness.block(vec![cc(20, 127)]).save_midi_learn);
    assert_eq!(harness.param("filter_1_cutoff"), 136.0);
}

#[test]
fn stepped_parameters_take_the_rounded_value() {
    let mut harness = Harness::new(MidiLearn::in_memory());
    harness.midi_learn.arm("osc_1_unison_voices");
    harness.block(vec![cc(30, 64)]);
    assert_eq!(harness.param("osc_1_unison_voices"), 9.0);
}

#[test]
fn cancel_clear_and_bypass() {
    let mut harness = Harness::new(MidiLearn::in_memory());
    harness.midi_learn.arm("macro_control_1");
    harness.midi_learn.cancel();
    assert!(!harness.block(vec![cc(3, 127)]).save_midi_learn);
    assert!(!harness.midi_learn.is_midi_mapped("macro_control_1"));

    harness.midi_learn.arm("macro_control_1");
    harness.block(vec![cc(3, 127)]);
    assert!(harness.midi_learn.is_midi_mapped("macro_control_1"));
    harness.midi_learn.clear("macro_control_1");
    assert!(!harness.midi_learn.is_midi_mapped("macro_control_1"));
    // The editor already published (and saved) the clear.
    assert!(!harness.block(vec![]).save_midi_learn);
    assert!(harness
        .plugin
        .processor()
        .engine()
        .midi_learn_map()
        .unwrap()
        .values()
        .all(|controls| controls.is_empty()));

    // Requests reach the engine while bypassed; MIDI does not.
    let params = harness.plugin.synth_params().clone();
    let bypass = &params.control("bypass").unwrap().param;
    let sas_plugin::params::ControlParam::Bypass(bypass) = bypass else {
        panic!("bypass is a bool parameter");
    };
    unsafe { nice_plug::params::InternalParamMut::_internal_set_plain_value(bypass, true) };
    harness.midi_learn.arm("macro_control_2");
    harness.block(vec![cc(4, 127)]);
    assert!(!harness.midi_learn.is_midi_mapped("macro_control_2"));
    unsafe { nice_plug::params::InternalParamMut::_internal_set_plain_value(bypass, false) };
    harness.block(vec![cc(4, 127)]);
    assert!(harness.midi_learn.is_midi_mapped("macro_control_2"));
}

#[test]
fn the_map_is_saved_to_and_restored_from_the_config_file() {
    let path = temp_config("save");
    let mut harness = Harness::new(MidiLearn::with_config_file(path.clone()));
    harness.midi_learn.arm("filter_1_cutoff");
    assert!(harness.block(vec![cc(20, 0)]).save_midi_learn);
    // nice-plug runs the save as a background task; run it here.
    harness.midi_learn.save_config();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"{"midi_learn":[{"destinations":[{"destination":"filter_1_cutoff","max_range":136.0,"min_range":8.0}],"source":20}]}"#
    );

    // A new instance loads it into the engine before processing.
    let mut restored = Harness::new(MidiLearn::with_config_file(path.clone()));
    assert!(restored.midi_learn.is_midi_mapped("filter_1_cutoff"));
    restored.block(vec![cc(20, 127)]);
    assert_eq!(restored.param("filter_1_cutoff"), 136.0);

    // Clearing saves at once, on the editor's thread.
    restored.midi_learn.clear("filter_1_cutoff");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        r#"{"midi_learn":[{"destinations":null,"source":20}]}"#
    );
    assert!(config::load_midi_map_config(&path).is_empty());
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_clear_racing_a_learn_is_republished() {
    // The editor clears while the audio thread is about to publish a map
    // that still holds the parameter: the next block applies the clear and
    // publishes again.
    let path = temp_config("race");
    let mut harness = Harness::new(MidiLearn::with_config_file(path.clone()));
    harness.midi_learn.arm("macro_control_1");
    harness.block(vec![cc(8, 0)]);
    harness.midi_learn.arm("macro_control_2");
    harness.midi_learn.clear("macro_control_1");
    // This block applies arm then clear, learns macro_control_2 on CC 9.
    assert!(harness.block(vec![cc(9, 0)]).save_midi_learn);
    assert!(!harness.midi_learn.is_midi_mapped("macro_control_1"));
    assert!(harness.midi_learn.is_midi_mapped("macro_control_2"));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
