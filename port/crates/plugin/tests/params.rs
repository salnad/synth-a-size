// SPDX-License-Identifier: GPL-3.0-or-later

//! Host parameters: one per engine control, stable IDs, defaults and ranges
//! from the registry.

use std::collections::HashSet;

use nice_plug::prelude::*;
use sas_plugin::params::{host_kind, ControlParam, HostKind};
use sas_plugin::SynthASize;
use sas_preset::{engine, registry, ValueScale};

fn plugin() -> SynthASize {
    SynthASize::in_memory()
}

/// nice-plug's VST3 parameter ID (`wrapper::util::hash_param_id`).
fn vst3_param_id(id: &str) -> u32 {
    let mut hash: u32 = 0;
    for byte in id.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    hash & !(1 << 31)
}

#[test]
fn exposes_every_engine_control_in_registry_order() {
    let plugin = plugin();
    let map = plugin.params().param_map();
    let ids: Vec<&str> = map.iter().map(|(id, _, _)| id.as_str()).collect();
    let expected: Vec<&str> = registry()
        .parameters()
        .iter()
        .map(|info| info.name.as_str())
        .filter(|name| engine::is_control(name))
        .collect();
    assert_eq!(ids, expected);
    assert_eq!(registry().len(), 794);
    assert_eq!(
        ids.len() + engine::NON_CONTROL_PARAMETERS.len(),
        registry().len()
    );
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, engine::controls());
}

#[test]
fn vst3_ids_are_unique_and_clear_of_the_midi_cc_range() {
    const MIDI_PARAMS_END: u32 = 1 << 31;
    const MIDI_PARAMS_START: u32 = MIDI_PARAMS_END - 130 * 16;
    let plugin = plugin();
    let mut seen = HashSet::new();
    for (id, _, _) in plugin.params().param_map() {
        let hash = vst3_param_id(&id);
        assert!(seen.insert(hash), "{id} collides with another parameter");
        assert!(
            !(MIDI_PARAMS_START..MIDI_PARAMS_END).contains(&hash),
            "{id} collides with a MIDI CC parameter"
        );
    }
}

#[test]
fn parameters_carry_registry_names_defaults_and_ranges() {
    let plugin = plugin();
    let params = plugin.synth_params();
    for control in params.controls() {
        let info = control.info;
        match &control.param {
            ControlParam::Float(param) => {
                assert_eq!(param.name(), info.display_name);
                assert_eq!(
                    param.default_plain_value(),
                    info.default_value,
                    "{}",
                    info.name
                );
                assert_eq!(param.unmodulated_plain_value(), info.default_value);
                let range = param.range();
                assert_eq!(range.unnormalize(0.0), info.min, "{}", info.name);
                assert_eq!(range.unnormalize(1.0), info.max, "{}", info.name);
                assert!(!param.flags().contains(ParamFlags::BYPASS));
                assert_eq!(param.step_count(), None, "{}", info.name);
                assert_eq!(host_kind(info), HostKind::Continuous);
            }
            ControlParam::Stepped(param) => {
                assert_eq!(param.name(), info.display_name);
                assert_eq!(
                    param.default_plain_value() as f32,
                    info.default_value,
                    "{}",
                    info.name
                );
                let range = param.range();
                assert_eq!(range.unnormalize(0.0) as f32, info.min, "{}", info.name);
                assert_eq!(range.unnormalize(1.0) as f32, info.max, "{}", info.name);
                // ValueBridge::getNumSteps is 1 + span; VST3 reports span.
                assert_eq!(param.step_count(), Some((info.max - info.min) as usize));
                assert_eq!(host_kind(info), HostKind::Stepped);
            }
            ControlParam::Bypass(param) => {
                assert_eq!(info.name, "bypass");
                assert!(param.flags().contains(ParamFlags::BYPASS));
                assert!(!param.default_plain_value());
            }
        }
    }
    assert!(!params.bypassed());
}

#[test]
fn indexed_parameters_are_stepped_like_the_value_bridge() {
    let plugin = plugin();
    let params = plugin.synth_params();
    // ValueBridge::isDiscrete: indexed and fewer than 300 steps.
    for control in params.controls() {
        let info = control.info;
        let discrete = info.value_scale == ValueScale::Indexed
            && (info.max - info.min).round() < 300.0
            && info.name != "bypass";
        assert_eq!(
            matches!(control.param, ControlParam::Stepped(_)),
            discrete,
            "{}",
            info.name
        );
    }
    let ControlParam::Stepped(transpose) = &params.control("osc_1_transpose").unwrap().param else {
        panic!("stepped parameter");
    };
    assert_eq!(transpose.step_count(), Some(96));
    // -48..48: normalized 0.51 is 0.96 semitones, rounded to 1
    // (ValueBridge::convertToEngineValue).
    assert_eq!(transpose.preview_plain(0.51), 1);
    // Indexed with 300 steps or more: continuous for the host, still rounded.
    let ControlParam::Float(quantize) = &params.control("osc_1_transpose_quantize").unwrap().param
    else {
        panic!("continuous parameter");
    };
    assert_eq!(quantize.step_count(), None);
    assert_eq!(quantize.preview_plain(0.5), 4096.0);
    assert!(matches!(
        params.control("effect_chain_order").unwrap().param,
        ControlParam::Float(_)
    ));
    let ControlParam::Float(cutoff) = &params.control("filter_1_cutoff").unwrap().param else {
        panic!("float parameter");
    };
    assert_eq!(cutoff.preview_plain(0.5), 72.0);
    assert_eq!(cutoff.preview_plain(0.51), 8.0 + 0.51 * 128.0);
}

#[test]
fn host_text_shows_options_and_display_units() {
    let plugin = plugin();
    let params = plugin.synth_params();
    let ControlParam::Stepped(style) = &params.control("delay_style").unwrap().param else {
        panic!("stepped parameter");
    };
    assert_eq!(
        style.normalized_value_to_string(2.0 / 3.0, true),
        "Ping Pong"
    );
    assert_eq!(
        style.string_to_normalized_value("Ping Pong"),
        Some(2.0 / 3.0)
    );
    let ControlParam::Float(volume) = &params.control("volume").unwrap().param else {
        panic!("float parameter");
    };
    assert_eq!(volume.to_string(), "-6.02dB");
}

/// Owner decision 2026-10-21 (`migration/ledger/2026-10-21-owner-approval-plugin-text-entry.md`):
/// typed text is read as the displayed value and clamped to the range; option
/// names select their option.
#[test]
fn typed_values_are_read_as_displayed_and_clamped() {
    let plugin = plugin();
    let params = plugin.synth_params();
    let ControlParam::Float(cutoff) = &params.control("filter_1_cutoff").unwrap().param else {
        panic!("float parameter");
    };
    // Displayed as semitones relative to 60 (post_offset -60): "12" is 72.
    assert_eq!(cutoff.string_to_normalized_value("12 semitones"), Some(0.5));
    assert_eq!(cutoff.string_to_normalized_value("1000"), Some(1.0));
    assert_eq!(cutoff.string_to_normalized_value("-1000"), Some(0.0));
    let ControlParam::Stepped(transpose) = &params.control("osc_1_transpose").unwrap().param else {
        panic!("stepped parameter");
    };
    assert_eq!(transpose.string_to_normalized_value("100"), Some(1.0));
    assert_eq!(transpose.string_to_normalized_value("0.4"), Some(0.5));
    let ControlParam::Stepped(style) = &params.control("delay_style").unwrap().param else {
        panic!("stepped parameter");
    };
    assert_eq!(style.string_to_normalized_value("mono"), Some(0.0));
    assert_eq!(style.string_to_normalized_value("not an option"), None);
}

#[test]
fn groups_name_existing_sections() {
    let plugin = plugin();
    let groups: HashSet<String> = plugin
        .params()
        .param_map()
        .into_iter()
        .map(|(_, _, group)| group)
        .collect();
    for group in [
        "",
        "Oscillator 1",
        "Filter FX",
        "Modulation 64",
        "Envelope 6",
        "Reverb",
    ] {
        assert!(groups.contains(group), "missing group {group:?}");
    }
    assert!(groups.iter().all(|group| !group.contains('/')));
}
