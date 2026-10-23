// SPDX-License-Identifier: GPL-3.0-or-later

//! The by-name API used by the plugin and the LLM control layer.

use sas_preset::SlotValues;
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

/// `ModulationManager::modulationDraggedToComponent`: a new slot gets power
/// 0, a linear curve and the drag's amount, polarity, stereo and bypass, even
/// where a freed slot held an old route's; reconnecting changes nothing.
#[test]
fn gui_connect_initializes_a_new_slot_only() {
    let drag = SlotValues {
        amount: 0.4,
        bipolar: false,
        stereo: false,
        bypass: false,
    };
    let mut preset = fresh();
    let old = preset
        .connect_modulation_from_gui("lfo_1", "osc_1_level", drag)
        .unwrap();
    assert_eq!((old.slot, old.created), (0, true));
    for (name, value) in [
        ("modulation_1_power", 3.0),
        ("modulation_1_amount", -0.8),
        ("modulation_1_stereo", 1.0),
        ("modulation_1_bypass", 1.0),
    ] {
        preset.set_value(name, value).unwrap();
    }
    preset.line_mapping_mut(0).unwrap().init_square();
    preset.remove_modulation("lfo_1", "osc_1_level").unwrap();

    let connection = preset
        .connect_modulation_from_gui("env_2", "filter_1_cutoff", drag)
        .unwrap();
    assert_eq!((connection.slot, connection.created), (0, true));
    let expected = [
        ("modulation_1_power", 0.0),
        ("modulation_1_amount", 0.4),
        ("modulation_1_bipolar", 0.0),
        ("modulation_1_stereo", 0.0),
        ("modulation_1_bypass", 0.0),
    ];
    assert_eq!(connection.parameter_changes, expected);
    for (name, value) in expected {
        assert_eq!(preset.value(name), Some(value), "{name}");
    }
    assert!(preset.modulations()[0].line_mapping.is_linear());

    preset.set_value("modulation_1_power", 2.0).unwrap();
    let again = preset
        .connect_modulation_from_gui("env_2", "filter_1_cutoff", drag)
        .unwrap();
    assert_eq!((again.slot, again.created), (0, false));
    assert!(again.parameter_changes.is_empty());
    assert_eq!(preset.value("modulation_1_power"), Some(2.0));

    assert!(matches!(
        preset.connect_modulation_from_gui("lfo_9", "filter_1_cutoff", drag),
        Err(Error::UnknownModulationSource(_))
    ));
}

/// The amount of `modulationDraggedToComponent`: from the control's
/// position to the top of its range, to the nearer end doubled when bipolar,
/// and never below `kDefaultModulationRatio` (0.25).
#[test]
fn a_drag_amount_follows_the_control_position() {
    let level = sas_preset::registry().get("osc_1_level").unwrap();
    assert_eq!((level.min, level.max), (0.0, 1.0));
    let amount = |value: f32, bipolar: bool| SlotValues::dragged_to(level, value, bipolar).amount;
    assert_eq!(amount(0.25, false), 0.75);
    assert_eq!(amount(0.9, false), 0.25);
    assert_eq!(amount(0.25, true), 0.5);
    assert_eq!(amount(0.5, true), 1.0);
    assert_eq!(amount(2.0, false), 0.25);
    let drag = SlotValues::dragged_to(level, 0.1, true);
    assert!(drag.bipolar && !drag.stereo && !drag.bypass);
}

/// The offline session's drag values read the destination from the state.
#[test]
fn dragged_values_read_the_state() {
    let mut preset = fresh();
    preset.set_value("osc_1_level", 0.25).unwrap();
    assert_eq!(
        preset.dragged_values("osc_1_level", false),
        Some(SlotValues {
            amount: 0.75,
            bipolar: false,
            stereo: false,
            bypass: false,
        })
    );
    assert_eq!(
        preset.dragged_values("osc_1_level", true).unwrap().amount,
        0.5
    );
    assert_eq!(preset.dragged_values("not_a_destination", false), None);
}
