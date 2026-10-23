// SPDX-License-Identifier: GPL-3.0-or-later

//! The by-name API used by the plugin and the LLM control layer.

use sas_preset::{engine, lookup, registry, Error, InitContent, PresetState, ValueScale};

fn fresh() -> PresetState {
    PresetState::new(&InitContent { sample_seed: 0 })
}

#[test]
fn looks_up_parameters_by_name() {
    let cutoff = lookup("filter_1_cutoff").unwrap();
    assert_eq!(
        (cutoff.min, cutoff.max, cutoff.default_value),
        (8.0, 136.0, 60.0)
    );
    assert_eq!(cutoff.display_name, "Filter 1 Cutoff");
    assert_eq!(cutoff.local_description, "Cutoff");
    assert_eq!(cutoff.value_scale, ValueScale::Linear);
    assert_eq!(lookup("osc_1_on").unwrap().default_value, 1.0);
    assert_eq!(
        lookup("delay_style").unwrap().options().unwrap()[2],
        "Ping Pong"
    );
    assert!(lookup("no_such_parameter").is_none());
    assert_eq!(registry().len(), 794);
    assert_eq!(
        registry().index_of(&registry().get_index(10).unwrap().name),
        Some(10)
    );
}

#[test]
fn sets_controls_and_rejects_others() {
    let mut preset = fresh();
    preset.set_value("filter_1_cutoff", 200.0).unwrap();
    assert_eq!(preset.value("filter_1_cutoff"), Some(200.0));
    assert_eq!(
        preset.set_value("sub_level", 0.5),
        Err(Error::NotAControl("sub_level".into()))
    );
    assert_eq!(
        preset.set_value("nope", 0.5),
        Err(Error::UnknownParameter("nope".into()))
    );
    assert_eq!(preset.controls().len(), engine::controls().len());
}

#[test]
fn adds_and_removes_modulations_like_the_modulation_bank() {
    let mut preset = fresh();
    assert_eq!(preset.add_modulation("lfo_1", "filter_1_cutoff"), Ok(0));
    assert_eq!(preset.value("modulation_1_bipolar"), Some(1.0));
    assert_eq!(preset.add_modulation("lfo_1", "filter_1_cutoff"), Ok(0));
    assert_eq!(preset.add_modulation("env_2", "osc_1_level"), Ok(1));
    assert_eq!(preset.value("modulation_2_bipolar"), Some(0.0));
    // Slot n never modulates its own amount.
    assert_eq!(
        preset.add_modulation("macro_control_1", "modulation_3_amount"),
        Ok(3)
    );

    assert_eq!(
        preset.add_modulation("nope", "filter_1_cutoff"),
        Err(Error::UnknownModulationSource("nope".into()))
    );
    assert_eq!(
        preset.add_modulation("lfo_1", "osc_1_on"),
        Err(Error::UnknownModulationDestination("osc_1_on".into()))
    );

    assert_eq!(
        preset.remove_modulation("lfo_1", "filter_1_cutoff"),
        Some(0)
    );
    assert!(preset.modulations()[0].is_free());
    assert_eq!(preset.add_modulation("random_1", "osc_2_pan"), Ok(0));
    assert_eq!(preset.remove_modulation("lfo_1", "filter_1_cutoff"), None);
}

#[test]
fn round_trips_through_json_text() {
    let mut preset = fresh();
    preset.set_value("osc_2_on", 1.0).unwrap();
    preset.add_modulation("lfo_2", "osc_2_wave_frame").unwrap();
    preset.info.author = "test".into();
    preset.info.macros[0] = "Brightness".into();

    let text = preset.to_json_string();
    let mut loaded = fresh();
    loaded.load_json_str(&text).unwrap();
    assert_eq!(loaded.value("osc_2_on"), Some(1.0));
    assert_eq!(
        loaded.modulation_index("lfo_2", "osc_2_wave_frame"),
        Some(0)
    );
    assert_eq!(loaded.info.author, "test");
    assert_eq!(loaded.info.macro_name(0), "Brightness");
    assert_eq!(loaded.info.macro_name(1), "MACRO 2");
}

#[test]
fn rejects_bad_presets_without_changing_state() {
    let mut preset = fresh();
    preset.set_value("osc_1_level", 0.25).unwrap();
    let before = preset.to_json_string();
    assert!(matches!(
        preset.load_json_str("{\"synth_version\": \"2.0.0\"}"),
        Err(Error::NewerVersion(_))
    ));
    assert!(matches!(
        preset.load_json_str("{\"synth_version\": \"1.0.6\", \"settings\": {}}"),
        Err(Error::Corrupted(_))
    ));
    assert!(matches!(
        preset.load_json_str("not json"),
        Err(Error::Corrupted(_))
    ));
    assert_eq!(preset.to_json_string(), before);
}

#[test]
fn rejects_upgrades_with_missing_keys() {
    let preset = r#"{"synth_version": "0.4.9", "settings": {"sub_on": 1.0}}"#;
    assert!(matches!(
        fresh().load_json_str(preset),
        Err(Error::Corrupted(_))
    ));
}

/// `SynthGuiInterface::connectModulation`: a new slot gets the source's
/// bipolar default, power 0 and a linear curve; reconnecting changes nothing.
#[test]
fn gui_connect_initializes_a_new_slot_only() {
    let mut preset = fresh();
    preset.set_value("modulation_1_power", 3.0).unwrap();
    preset.line_mapping_mut(0).unwrap().init_square();

    let connection = preset
        .connect_modulation_from_gui("env_2", "filter_1_cutoff")
        .unwrap();
    assert_eq!(connection.slot, 0);
    assert!(connection.created);
    assert_eq!(
        connection.parameter_changes,
        vec![("modulation_1_bipolar", 0.0), ("modulation_1_power", 0.0)]
    );
    assert_eq!(preset.value("modulation_1_power"), Some(0.0));
    assert!(preset.modulations()[0].line_mapping.is_linear());

    preset.set_value("modulation_1_power", 2.0).unwrap();
    let again = preset
        .connect_modulation_from_gui("env_2", "filter_1_cutoff")
        .unwrap();
    assert_eq!((again.slot, again.created), (0, false));
    assert!(again.parameter_changes.is_empty());
    assert_eq!(preset.value("modulation_1_power"), Some(2.0));

    let bipolar = preset
        .connect_modulation_from_gui("lfo_1", "modulation_1_amount")
        .unwrap();
    assert_eq!(bipolar.slot, 1);
    assert_eq!(bipolar.parameter_changes[0], ("modulation_2_bipolar", 1.0));
    assert!(matches!(
        preset.connect_modulation_from_gui("lfo_9", "filter_1_cutoff"),
        Err(Error::UnknownModulationSource(_))
    ));
}
