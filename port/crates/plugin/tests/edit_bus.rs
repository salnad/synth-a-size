// SPDX-License-Identifier: GPL-3.0-or-later

//! The edit bus through the plugin: edits made on `SynthPlugin::edit_handle`
//! reach the engine at the next block and play exactly as the same edits
//! made on the engine directly; the mirror is what the host saves; a host
//! restore drops the edits queued before it.
//!
//! The engine's seed counter is process-wide, so the renders here run one at
//! a time.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use nice_plug::params::InternalParamMut;
use nice_plug::prelude::NoteEvent;
use sas_common::LineGenerator;
use sas_engine::sas_oscillators::RandomValues;
use sas_engine::synth::Synth;
use sas_plugin::params::ControlParam;
use sas_plugin::{state, SoundEngineHost, SynthASize, SynthPlugin, Transport};
use sas_preset::{registry, InitContent, PresetState, SlotValues};

const SAMPLE_RATE: i32 = 44_100;
const BLOCK: usize = 64;
const SEED: i32 = 5;

static ENGINE_RNG: Mutex<()> = Mutex::new(());

fn corpus(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../migration/corpus")
        .join(path)
}

fn new_plugin(preset: &Path) -> SynthASize {
    sas_common::random::set_next_seed(SEED);
    RandomValues::reset_process_state();
    let mut plugin: SynthASize = SynthPlugin::new(SoundEngineHost::new());
    plugin
        .synth_params()
        .restore_state(state::plugin_state_from_file(preset).unwrap());
    let params = plugin.synth_params().clone();
    plugin.processor_mut().activate(&params, SAMPLE_RATE as f32);
    plugin
}

fn set_param(plugin: &SynthASize, name: &str, value: f32) {
    let control = plugin.synth_params().control(name).unwrap();
    // SAFETY: as a host sets a parameter between blocks; nothing else is
    // setting it.
    unsafe {
        match &control.param {
            ControlParam::Float(param) => {
                param._internal_set_plain_value(value);
            }
            ControlParam::Stepped(param) => {
                param._internal_set_plain_value(value.round() as i32);
            }
            ControlParam::Bypass(param) => {
                param._internal_set_plain_value(value != 0.0);
            }
        }
    }
}

fn play(plugin: &mut SynthASize, blocks: usize) -> Vec<f32> {
    let params = plugin.synth_params().clone();
    let transport = Transport {
        tempo: Some(120.0),
        playing: false,
        pos_beats: None,
    };
    let mut out = Vec::new();
    let (mut left, mut right) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    for block in 0..blocks {
        let mut events: Vec<NoteEvent<()>> = match block {
            0 => vec![NoteEvent::from_midi(4, &[0x90, 50, 100]).unwrap()],
            300 => vec![NoteEvent::from_midi(9, &[0x80, 50, 0]).unwrap()],
            _ => vec![],
        };
        events.reverse();
        plugin
            .processor_mut()
            .process(&params, &mut left, &mut right, &transport, || events.pop());
        out.extend(left.iter().zip(&right).flat_map(|(l, r)| [*l, *r]));
    }
    out
}

fn first_difference(a: &[f32], b: &[f32]) -> Option<usize> {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .position(|(x, y)| x.to_bits() != y.to_bits())
}

#[test]
fn edits_play_like_direct_engine_edits() {
    let _rng = ENGINE_RNG.lock().unwrap_or_else(|p| p.into_inner());
    let preset_path = corpus("presets/init_saw.vital");
    let mut lfo = LineGenerator::default();
    lfo.init_saw_down();

    // Through the plugin, as an editor would: bus edits plus the host
    // parameter changes they report.
    let mut plugin = new_plugin(&preset_path);
    let edits = plugin.edit_handle();
    // A drag onto the cutoff knob at its preset value.
    let cutoff = registry().get("filter_1_cutoff").unwrap();
    let cutoff_value = plugin
        .synth_params()
        .control("filter_1_cutoff")
        .unwrap()
        .param
        .value();
    let drag = SlotValues::dragged_to(cutoff, cutoff_value, false);
    let connection = edits
        .connect_modulation("lfo_1", "filter_1_cutoff", drag)
        .unwrap();
    for (name, value) in &connection.parameter_changes {
        set_param(&plugin, name, *value);
    }
    edits.set_lfo_shape(0, lfo.clone()).unwrap();
    set_param(&plugin, "modulation_1_amount", 0.5);
    set_param(&plugin, "filter_1_on", 1.0);
    set_param(&plugin, "lfo_1_frequency", 3.0);
    let from_plugin = play(&mut plugin, 500);

    // The engine directly.
    sas_common::random::set_next_seed(SEED);
    RandomValues::reset_process_state();
    let mut synth = Synth::new();
    let mut preset = PresetState::new(&InitContent { sample_seed: 0 });
    preset.load_file(&preset_path).unwrap();
    synth.load_preset(&preset);
    synth.prepare(SAMPLE_RATE, 120.0);
    synth.connect_modulation("lfo_1", "filter_1_cutoff");
    for (name, value) in &connection.parameter_changes {
        synth.set_control(name, *value);
    }
    synth.set_lfo_shape(0, &mut lfo.clone());
    for (name, value) in [
        ("modulation_1_amount", 0.5),
        ("filter_1_on", 1.0),
        ("lfo_1_frequency", 3.0),
    ] {
        synth.set_control(name, value);
    }
    let mut from_engine = Vec::new();
    let (mut left, mut right) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let mut seconds = 0.0;
    for block in 0..500 {
        let midi: &[(usize, [u8; 3])] = match block {
            0 => &[(4, [0x90, 50, 100])],
            300 => &[(9, [0x80, 50, 0])],
            _ => &[],
        };
        synth.render_block(&mut left, &mut right, midi, &mut seconds, SAMPLE_RATE);
        from_engine.extend(left.iter().zip(&right).flat_map(|(l, r)| [*l, *r]));
    }

    let peak = from_plugin.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05, "silent (peak {peak})");
    assert_eq!(first_difference(&from_plugin, &from_engine), None);

    // The host saves the edited state.
    let saved = plugin.synth_params().preset_state();
    assert_eq!(saved.modulation_index("lfo_1", "filter_1_cutoff"), Some(0));
    assert_eq!(saved.lfos[0].name(), "Saw Down");
    assert_eq!(saved.value("modulation_1_amount"), Some(0.5));
}

#[test]
fn a_host_restore_drops_edits_queued_before_it() {
    let _rng = ENGINE_RNG.lock().unwrap_or_else(|p| p.into_inner());
    let preset_path = corpus("presets/init_saw.vital");

    let mut reference = new_plugin(&preset_path);
    let expected = play(&mut reference, 200);

    let mut plugin = new_plugin(&preset_path);
    let edits = plugin.edit_handle();
    let generation = edits.generation();
    let drag = SlotValues {
        amount: 1.0,
        bipolar: false,
        stereo: false,
        bypass: false,
    };
    edits
        .connect_modulation("lfo_1", "osc_1_level", drag)
        .unwrap();
    set_param(&plugin, "modulation_1_amount", 1.0);
    // The host restores the same preset before the next block: the
    // connection made before it is dropped, and the restored amount stands.
    plugin
        .synth_params()
        .restore_state(state::plugin_state_from_file(&preset_path).unwrap());
    assert_eq!(edits.generation(), generation + 1);
    edits.with_document(|document| {
        assert!(document
            .state
            .modulation_index("lfo_1", "osc_1_level")
            .is_none());
    });
    let actual = play(&mut plugin, 200);
    assert_eq!(first_difference(&actual, &expected), None);
}
