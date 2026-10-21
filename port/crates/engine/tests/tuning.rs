// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of the `Tuning` port against the C++ reference for every
//! `engine/tuning/*` case (`migration/corpus/modules/engine/tuning.json`).
//!
//! Mirrors `migration/reference/src/modules/engine/tuning.cpp`: the case's
//! files are written with `\n` line endings (JUCE `replaceWithText`) and
//! loaded in order through [`Tuning::load_file`].

use sas_engine::Tuning;
use sas_parity::{all_case_ids, ModuleCase};
use sas_simd::common::MIDI_SIZE;
use serde_json::Value as Json;

fn run_case(case: &ModuleCase) -> (Tuning, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let params = case.params();
    let directory = std::env::temp_dir().join(format!(
        "sas-tuning-{}-{}",
        std::process::id(),
        case.id().replace('/', "_")
    ));
    std::fs::create_dir_all(&directory).unwrap();

    let mut tuning = Tuning::new();
    for file in params
        .get("files")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let text: String = file["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| format!("{}\n", line.as_str().unwrap()))
            .collect();
        let path = directory.join(file["name"].as_str().unwrap());
        std::fs::write(&path, text).unwrap();
        tuning
            .load_file(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", case.id()));
    }
    std::fs::remove_dir_all(&directory).ok();

    if let Some(ratio) = params.get("reference_ratio").and_then(Json::as_f64) {
        tuning.set_reference_ratio(ratio as f32);
    }
    if let Some(pair) = params
        .get("reference_note_frequency")
        .and_then(Json::as_array)
    {
        tuning.set_reference_note_frequency(
            pair[0].as_i64().unwrap() as i32,
            pair[1].as_f64().unwrap() as f32,
        );
    }
    if let Some(start) = params.get("start_midi_note").and_then(Json::as_i64) {
        tuning.set_start_midi_note(start as i32);
    }

    let converted: Vec<f32> = (0..128)
        .map(|note| tuning.convert_midi_note(note))
        .collect();

    let data = tuning.state_to_json();
    let mut loaded = Tuning::new();
    loaded
        .json_to_state(&data)
        .expect("the saved state reloads");
    let roundtrip: Vec<f32> = (0..128)
        .map(|note| loaded.convert_midi_note(note))
        .collect();

    let state = vec![
        data["scale_start_midi_note"].as_i64().unwrap() as f32,
        data["reference_midi_note"].as_f64().unwrap() as f32,
    ];
    let scale = data["scale"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    (tuning, converted, roundtrip, state, scale)
}

/// Raw outputs the case does not capture (`params.skip_raw`).
fn skip_raw(case: &ModuleCase) -> Vec<String> {
    case.params()
        .get("skip_raw")
        .and_then(Json::as_array)
        .map(|names| {
            names
                .iter()
                .map(|n| n.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn tuning_matches_reference() {
    let ids: Vec<String> = all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with("engine/tuning/"))
        .collect();
    assert!(ids.len() >= 12, "expected the tuning cases, found {ids:?}");
    for id in &ids {
        let case = ModuleCase::load(id);
        let (tuning, converted, roundtrip, state, scale) = run_case(&case);
        let note_scale = 1.0 / (MIDI_SIZE - 1) as f32;
        let percent = |notes: &[f32]| notes.iter().map(|n| n * note_scale).collect::<Vec<f32>>();
        case.assert_parity("converted_note_percent", &percent(&converted));
        case.assert_parity("roundtrip_note_percent", &percent(&roundtrip));
        case.assert_parity("state", &state[..1]);
        let skip = skip_raw(&case);
        for (name, data) in [
            ("converted", &converted[..]),
            ("roundtrip", &roundtrip[..]),
            ("reference_midi_note", &state[1..]),
        ] {
            if !skip.iter().any(|s| s == name) {
                case.assert_parity(name, &data);
            }
        }
        case.assert_parity("scale", &scale);
        case.assert_artifact("name.txt", &tuning.name());
    }
}
