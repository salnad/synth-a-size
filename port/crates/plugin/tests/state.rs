// SPDX-License-Identifier: GPL-3.0-or-later

//! Host state as preset JSON, and preset files, against sas-preset's own
//! load/save of the authored corpus presets.

use std::path::{Path, PathBuf};

use nice_plug::params::InternalParamMut;
use nice_plug::plugin::{ParamValue, PluginState};
use sas_engine::Tuning;
use sas_plugin::params::ControlParam;
use sas_plugin::{state, SynthASize, SynthParams};
use sas_preset::{InitContent, PresetState, SampleState};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn corpus_presets() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for directory in ["presets", "preset_tests"] {
        for entry in std::fs::read_dir(repo().join("migration/corpus").join(directory)).unwrap() {
            let path = entry.unwrap().path();
            if path
                .extension()
                .is_some_and(|extension| extension == state::PRESET_EXTENSION)
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    assert!(paths.len() >= 20);
    paths
}

fn is_error_case(path: &Path) -> bool {
    path.file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("error_")
}

/// What the host stores: the state as nice-plug writes it (serde JSON).
fn host_round_trip(state: &PluginState) -> PluginState {
    let bytes = serde_json::to_vec(state).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn set(params: &SynthParams, name: &str, value: f32) {
    let ControlParam::Float(param) = &params.control(name).unwrap().param else {
        panic!("{name} is not a float parameter");
    };
    unsafe { param._internal_set_plain_value(value) };
}

fn assert_params_match(params: &SynthParams, preset: &PresetState, context: &str) {
    for control in params.controls() {
        let expected = preset.value(&control.info.name).unwrap();
        // How the parameter holds the preset value (-0.0 as 0.0, stepped
        // values rounded); the saved JSON keeps the value itself and is
        // compared separately.
        let saved = control.param.saved_value();
        assert!(
            control.param.matches(expected, saved),
            "{context}: {} is {saved}, preset has {expected}",
            control.info.name
        );
    }
}

/// Upstream's `Sample::stateToJson` encodes the stored buffer from its
/// start, zero padding included, so a save and reload delays the sample by
/// `kBufferSamples` (4) frames (sas-preset `SampleState::to_json`).
fn assert_saved_sample(before: &SampleState, after: &SampleState) {
    const BUFFER_SAMPLES: usize = 4;
    let delayed = |channel: &[f32]| -> Vec<f32> {
        let mut out = vec![0.0; BUFFER_SAMPLES];
        out.extend_from_slice(&channel[..channel.len() - BUFFER_SAMPLES]);
        out
    };
    assert_eq!(
        (&after.name, after.sample_rate),
        (&before.name, before.sample_rate)
    );
    let to_pcm = sas_common::scalar::float_to_pcm;
    assert_eq!(to_pcm(&after.left), to_pcm(&delayed(&before.left)));
    assert_eq!(
        after.right.as_deref().map(to_pcm),
        before.right.as_deref().map(|right| to_pcm(&delayed(right)))
    );
}

#[test]
fn corpus_presets_load_through_the_host_state_like_sas_preset() {
    for path in corpus_presets() {
        let name = path.display().to_string();
        let mut expected = PresetState::new(&InitContent { sample_seed: 0 });
        let loaded = expected.load_file(&path);
        let from_file = state::plugin_state_from_file(&path);
        if is_error_case(&path) {
            assert!(loaded.is_err(), "{name} loads in sas-preset");
            assert_eq!(from_file.err(), loaded.err(), "{name}");
            continue;
        }
        loaded.unwrap();

        let plugin = SynthASize::default();
        let params = plugin.synth_params();
        params.restore_state(host_round_trip(&from_file.unwrap()));
        assert_params_match(params, &expected, &name);
        assert_eq!(
            params.preset_state().to_json_string(),
            expected.to_json_string(),
            "{name}"
        );
    }
}

#[test]
fn host_state_round_trips_exactly() {
    let source = SynthASize::default();
    let params = source.synth_params();
    let path = repo().join("migration/corpus/preset_tests/modulation_slots.vital");
    let mut preset = PresetState::new(&InitContent { sample_seed: 0 });
    preset.load_file(&path).unwrap();
    // Values no host could set: out of range, and between the steps of a
    // stepped indexed parameter. Upstream stores them as given.
    preset.set_value("osc_1_transpose", 0.25).unwrap();
    preset.set_value("polyphony", 40.5).unwrap();
    params.restore_state(state::plugin_state_from_json(&preset.to_json_string()).unwrap());
    set(params, "filter_1_cutoff", 200.0);
    set(params, "lfo_2_frequency", -3.125);
    // The stepped parameter shows the rounded step.
    assert_eq!(
        params
            .control("osc_1_transpose")
            .unwrap()
            .param
            .saved_value(),
        0.0
    );
    let saved = host_round_trip(&params.capture_state());

    let target = SynthASize::default();
    target.synth_params().restore_state(saved);
    let source_document = params.preset_document();
    let mut target_document = target.synth_params().preset_document();
    assert_saved_sample(&source_document.state.sample, &target_document.state.sample);
    target_document.state.sample = source_document.state.sample.clone();
    assert_eq!(
        target_document.to_json_string(),
        source_document.to_json_string()
    );
    let restored = target.synth_params().preset_state();
    assert_eq!(restored.value("filter_1_cutoff"), Some(200.0));
    assert_eq!(restored.value("osc_1_transpose"), Some(0.25));
    assert_eq!(restored.value("polyphony"), Some(40.5));
    assert_eq!(restored.value("lfo_2_frequency"), Some(-3.125));
    assert!(restored.modulations().iter().any(|slot| !slot.is_free()));
}

#[test]
fn negative_zero_survives_the_host_path() {
    // controls_and_metadata.vital has voice_tune: -0.0; nice-plug's float
    // parameter reads it back as 0.0.
    let path = repo().join("migration/corpus/preset_tests/controls_and_metadata.vital");
    let source = SynthASize::default();
    source
        .synth_params()
        .restore_state(state::plugin_state_from_file(&path).unwrap());
    let voice_tune = |params: &SynthParams| {
        let text = params.capture_state().fields[state::PRESET_FIELD].clone();
        let data = sas_common::json::parse(&text).unwrap();
        let value = data
            .get("settings")
            .unwrap()
            .get("voice_tune")
            .unwrap()
            .dump();
        (value, params.preset_state().value("voice_tune").unwrap())
    };
    let (text, value) = voice_tune(source.synth_params());
    assert_eq!(text, "-0.0");
    assert!(value == 0.0 && value.is_sign_negative());

    let saved = host_round_trip(&source.synth_params().capture_state());
    let target = SynthASize::default();
    target.synth_params().restore_state(saved);
    let (text, value) = voice_tune(target.synth_params());
    assert_eq!(text, "-0.0");
    assert!(value == 0.0 && value.is_sign_negative());
    assert_eq!(
        target
            .synth_params()
            .control("voice_tune")
            .unwrap()
            .param
            .saved_value(),
        0.0
    );
}

#[test]
fn a_control_moved_away_and_back_saves_the_parameter_value() {
    let mut preset = PresetState::new(&InitContent { sample_seed: 0 });
    preset.set_value("osc_1_transpose", 0.25).unwrap();
    preset.set_value("voice_tune", -0.0).unwrap();
    let plugin = SynthASize::default();
    let params = plugin.synth_params();
    params.restore_state(state::plugin_state_from_json(&preset.to_json_string()).unwrap());
    let saved = |name: &str| params.preset_state().value(name).unwrap();
    assert_eq!(saved("osc_1_transpose"), 0.25);
    assert!(saved("voice_tune").is_sign_negative());

    let ControlParam::Stepped(transpose) = &params.control("osc_1_transpose").unwrap().param else {
        panic!("stepped parameter");
    };
    unsafe {
        transpose._internal_set_plain_value(1);
        transpose._internal_set_plain_value(0);
    }
    set(params, "voice_tune", 0.5);
    set(params, "voice_tune", 0.0);
    // What plays is the parameter's value now, and that is what is saved.
    assert_eq!(saved("osc_1_transpose"), 0.0);
    assert!(saved("voice_tune") == 0.0 && saved("voice_tune").is_sign_positive());

    // A new restore makes the preset's values stand again.
    params.restore_state(state::plugin_state_from_json(&preset.to_json_string()).unwrap());
    assert_eq!(saved("osc_1_transpose"), 0.25);
}

#[test]
fn the_preset_json_wins_over_saved_parameter_values() {
    let mut preset = PresetState::new(&InitContent { sample_seed: 0 });
    preset.set_value("filter_1_cutoff", 72.0).unwrap();
    let mut saved = state::plugin_state_from_json(&preset.to_json_string()).unwrap();
    saved
        .params
        .insert("filter_1_cutoff".to_string(), ParamValue::F32(10.0));
    saved.params.remove("osc_1_level");

    let plugin = SynthASize::default();
    plugin.synth_params().restore_state(saved);
    assert_params_match(plugin.synth_params(), &preset, "filtered");
}

#[test]
fn an_unloadable_state_changes_nothing() {
    let plugin = SynthASize::default();
    let params = plugin.synth_params();
    set(params, "filter_1_cutoff", 99.0);
    let before = params.preset_document().to_json_string();

    for text in [
        "{not json".to_string(),
        std::fs::read_to_string(
            repo().join("migration/corpus/preset_tests/error_newer_version.vital"),
        )
        .unwrap(),
    ] {
        let mut saved = params.capture_state();
        saved.fields.insert(state::PRESET_FIELD.to_string(), text);
        saved
            .params
            .insert("filter_1_cutoff".to_string(), ParamValue::F32(10.0));
        params.restore_state(saved);
        assert_eq!(params.preset_document().to_json_string(), before);
    }
}

#[test]
fn a_state_without_a_preset_restores_parameters_only() {
    // The shape of a state saved by the pre-M8 stub, or by another tool.
    let plugin = SynthASize::default();
    let params = plugin.synth_params();
    let before = params.preset_state();
    params.restore_state(PluginState {
        version: "0.0.1".to_string(),
        params: [("volume".to_string(), ParamValue::F32(4000.0))].into(),
        fields: Default::default(),
    });
    let after = params.preset_state();
    assert_eq!(after.value("volume"), Some(4000.0));
    assert_eq!(after.wavetables.len(), before.wavetables.len());
}

/// A 19-tone equal temperament scale, with A4 = 432 Hz.
fn nineteen_tone_tuning() -> Tuning {
    let mut lines = vec![
        "! 19edo.scl".to_string(),
        "19 tone equal".to_string(),
        "19".to_string(),
    ];
    lines.extend((1..19).map(|step| format!("{:.5}", step as f64 * 1200.0 / 19.0)));
    lines.push("2/1".to_string());
    let mut tuning = Tuning::new();
    tuning.load_scala_lines(&lines);
    tuning.set_name("19 tone equal");
    tuning.set_reference_note_frequency(69, 432.0);
    tuning
}

/// `Tuning::stateToJson` as it appears in the plugin state.
fn tuning_json(tuning: &Tuning) -> sas_common::json::Value {
    sas_common::json::parse(&tuning.state_to_json().to_string()).unwrap()
}

fn saved_tuning(params: &SynthParams) -> String {
    let saved = params.capture_state();
    let data = sas_common::json::parse(&saved.fields[state::PRESET_FIELD]).unwrap();
    data.get("tuning").unwrap().dump()
}

fn state_with_tuning(preset: &PresetState, tuning: &sas_common::json::Value) -> String {
    let mut data = preset.to_json();
    data.set("tuning", tuning.clone());
    data.dump()
}

#[test]
fn the_default_tuning_is_always_saved() {
    // SynthPlugin::getStateInformation writes getTuning()->stateToJson()
    // whatever the tuning; this is upstream's dump of a default Tuning.
    let plugin = SynthASize::default();
    assert_eq!(
        saved_tuning(plugin.synth_params()),
        r#"{"default":true,"mapping_name":"","reference_midi_note":0.0,"scale":[0.0,1.0,2.0,3.0,4.0,5.0,6.0,7.0,8.0,9.0,10.0,11.0,12.0],"scale_start_midi_note":60,"tuning_name":""}"#
    );
}

#[test]
fn tuning_round_trips_through_the_host_state() {
    let tuning = nineteen_tone_tuning();
    let preset = PresetState::new(&InitContent { sample_seed: 0 });
    let source = SynthASize::default();
    source.synth_params().restore_state(
        state::plugin_state_from_json(&state_with_tuning(&preset, &tuning_json(&tuning))).unwrap(),
    );
    assert_eq!(
        saved_tuning(source.synth_params()),
        tuning_json(&tuning).dump()
    );

    let saved = host_round_trip(&source.synth_params().capture_state());
    let target = SynthASize::default();
    target.synth_params().restore_state(saved);
    let restored = target.synth_params().preset_document().tuning;
    assert_eq!(restored.table(), tuning.table());
    assert_eq!(restored.name(), "19 tone equal");
    assert!(!restored.is_default());
    assert_eq!(
        saved_tuning(target.synth_params()),
        tuning_json(&tuning).dump()
    );
}

#[test]
fn states_and_preset_files_without_a_tuning_keep_the_current_one() {
    let tuning = nineteen_tone_tuning();
    let preset = PresetState::new(&InitContent { sample_seed: 0 });
    let plugin = SynthASize::default();
    let params = plugin.synth_params();
    params.restore_state(
        state::plugin_state_from_json(&state_with_tuning(&preset, &tuning_json(&tuning))).unwrap(),
    );

    // Upstream only loads "tuning" when the state has one.
    params.restore_state(state::plugin_state_from_json(&preset.to_json_string()).unwrap());
    assert_eq!(params.preset_document().tuning.table(), tuning.table());

    // SynthBase::loadFromFile never touches the tuning, even when the file
    // has one.
    let directory = std::env::temp_dir().join(format!("sas-plugin-tuning-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("with tuning.vital");
    std::fs::write(
        &path,
        state_with_tuning(&preset, &tuning_json(&Tuning::new())),
    )
    .unwrap();
    params.restore_state(state::plugin_state_from_file(&path).unwrap());
    assert_eq!(params.preset_document().tuning.table(), tuning.table());
    assert_eq!(params.preset_state().info.preset_name, "with tuning");
    std::fs::remove_dir_all(&directory).unwrap();

    // Preset files are saved without it.
    assert_eq!(params.preset_state().to_json().count("tuning"), 0);
}

#[test]
fn a_tuning_that_is_not_an_object_rejects_the_state() {
    let preset = PresetState::new(&InitContent { sample_seed: 0 });
    let mut data = preset.to_json();
    data.set("tuning", 12);
    assert!(matches!(
        state::plugin_state_from_json(&data.dump()),
        Err(sas_preset::Error::Corrupted(_))
    ));

    let plugin = SynthASize::default();
    let params = plugin.synth_params();
    set(params, "filter_1_cutoff", 99.0);
    let before = params.preset_document().to_json_string();
    let mut saved = params.capture_state();
    let mut changed = preset.clone();
    changed.set_value("filter_1_cutoff", 10.0).unwrap();
    let mut text = changed.to_json();
    text.set("tuning", "not a tuning");
    saved
        .fields
        .insert(state::PRESET_FIELD.to_string(), text.dump());
    params.restore_state(saved);
    assert_eq!(params.preset_document().to_json_string(), before);
}

#[test]
fn preset_files_save_with_the_extension_and_file_name() {
    let directory = std::env::temp_dir().join(format!("sas-plugin-presets-{}", std::process::id()));
    let plugin = SynthASize::default();
    let params = plugin.synth_params();
    set(params, "filter_1_cutoff", 77.5);
    let written = state::save_preset_file(
        &params.preset_state(),
        &directory.join("nested/my patch.txt"),
    )
    .unwrap();
    assert_eq!(written, directory.join("nested/my patch.vital"));

    let mut reloaded = PresetState::new(&InitContent { sample_seed: 0 });
    reloaded.load_file(&written).unwrap();
    assert_eq!(reloaded.info.preset_name, "my patch");
    assert_eq!(reloaded.value("filter_1_cutoff"), Some(77.5));
    let mut expected = params.preset_state();
    expected.info.preset_name = "my patch".to_string();
    assert_eq!(
        std::fs::read_to_string(&written).unwrap(),
        expected.to_json_string()
    );
    std::fs::remove_dir_all(&directory).unwrap();
}
