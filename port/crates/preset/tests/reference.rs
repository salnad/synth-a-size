// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity with the reference: the registry and engine tables, and the saved
//! state after loading every corpus preset, every preset-state test preset and
//! the init preset, compared with the reference's artifacts (module cases
//! `preset/registry` and `preset/state` in `migration/corpus/modules/preset/`,
//! rendered by `ref-module` and pinned by hash). The registry is exact; saved
//! states are compared under the owner-approved JSON bound declared in
//! `state.json`, with `wave_data` decoded as base64 f32.

use std::path::{Path, PathBuf};

use sas_common::json::{self, Value};
use sas_parity::{all_case_ids, ModuleCase};
use sas_preset::{engine, registry, Error, InitContent, PresetState, StringTable};

const STATE_CASES: &str = "preset/state/";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn reference_registry() -> Value {
    let case = ModuleCase::load("preset/registry/engine");
    json::parse(&case.artifact("registry.json")).expect("registry artifact parses")
}

/// The init content of the reference: the seed its init sample used.
fn init_content() -> InitContent {
    let sample_seed = reference_registry()
        .get("init")
        .unwrap()
        .get("sample_seed")
        .unwrap()
        .to_i32()
        .unwrap();
    InitContent { sample_seed }
}

/// Asserts with `assert_artifact`: hash-verified, and under the JSON bound the
/// case declares for `state.json` and `roundtrip.json`.
fn assert_same_state(case: &ModuleCase, artifact: &str, state: &PresetState) {
    case.assert_artifact(artifact, &(state.to_json_string() + "\n"));
}

/// The preset-state cases: (case, preset path relative to migration/corpus,
/// or `None` for the init preset).
fn state_cases() -> Vec<(ModuleCase, Option<String>)> {
    all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with(STATE_CASES))
        .map(|id| {
            let case = ModuleCase::load(&id);
            let preset = case
                .params()
                .get("preset")
                .map(|path| path.as_str().unwrap().to_string());
            (case, preset)
        })
        .collect()
}

/// The fields of each parameter in the registry capture, all compared by
/// `registry_matches_reference`.
const PARAMETER_FIELDS: [&str; 13] = [
    "default_value",
    "display_invert",
    "display_multiply",
    "display_name",
    "display_units",
    "local_description",
    "max",
    "min",
    "name",
    "post_offset",
    "string_table",
    "value_scale",
    "version_added",
];

/// The keys of a JSON object, sorted.
fn keys(value: &Value) -> Vec<String> {
    let Value::Object(object) = value else {
        panic!("not an object: {}", value.dump())
    };
    object.keys().cloned().collect()
}

#[test]
fn registry_matches_reference() {
    let reference = reference_registry();
    // Every field of the capture is checked by these tests, except
    // poly_modulation_destinations: the preset loader only consults mono
    // destinations (getMonoModulationDestination), so the port has no poly
    // table. A field added to the capture later fails here until it is
    // checked too.
    assert_eq!(
        keys(&reference),
        [
            "controls",
            "init",
            "modulation_sources",
            "mono_modulation_destinations",
            "num_parameters",
            "parameters",
            "poly_modulation_destinations",
            "slot_self_destinations",
            "string_tables",
            "synth_version",
        ]
    );
    assert_eq!(
        keys(reference.get("init").unwrap()),
        ["random_generators_constructed", "sample_seed", "seed"]
    );
    // The capture is taken with the seed counter at 0 (preset/registry.json);
    // sample_seed and random_generators_constructed are checked by the
    // init-state and engine-table tests.
    assert_eq!(
        reference
            .get("init")
            .unwrap()
            .get("seed")
            .unwrap()
            .to_i32()
            .unwrap(),
        0
    );
    let parameters = reference.get("parameters").unwrap();
    let registry = registry();
    assert_eq!(
        reference.get("num_parameters").unwrap().to_i32().unwrap() as usize,
        registry.len()
    );
    assert_eq!(parameters.size(), registry.len());

    for (index, expected) in parameters.iter_values().enumerate() {
        assert_eq!(keys(expected), PARAMETER_FIELDS, "parameter {index} fields");
        let actual = registry.get_index(index).unwrap();
        let name = expected.get("name").unwrap().to_str().unwrap();
        assert_eq!(actual.name, name, "parameter {index}");
        let float = |key: &str| expected.get(key).unwrap().to_f32().unwrap();
        let text = |key: &str| expected.get(key).unwrap().to_str().unwrap().to_string();
        assert_eq!(
            actual.version_added,
            expected.get("version_added").unwrap().to_i32().unwrap(),
            "{name}"
        );
        assert_eq!(actual.min.to_bits(), float("min").to_bits(), "{name} min");
        assert_eq!(actual.max.to_bits(), float("max").to_bits(), "{name} max");
        assert_eq!(
            actual.default_value.to_bits(),
            float("default_value").to_bits(),
            "{name} default"
        );
        assert_eq!(
            actual.post_offset.to_bits(),
            float("post_offset").to_bits(),
            "{name} post_offset"
        );
        assert_eq!(
            actual.display_multiply.to_bits(),
            float("display_multiply").to_bits(),
            "{name} display_multiply"
        );
        assert_eq!(
            actual.value_scale.as_str(),
            text("value_scale"),
            "{name} value_scale"
        );
        assert_eq!(
            actual.display_invert,
            expected.get("display_invert").unwrap().to_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            actual.display_units,
            text("display_units"),
            "{name} display_units"
        );
        assert_eq!(
            actual.display_name,
            text("display_name"),
            "{name} display_name"
        );
        assert_eq!(
            actual.local_description,
            text("local_description"),
            "{name} local_description"
        );
        let table = expected.get("string_table").unwrap();
        let actual_table = actual.string_table.map(StringTable::upstream_name);
        assert_eq!(actual_table, table.to_str().ok(), "{name} string_table");
        assert_eq!(
            registry.get(name).map(|info| &info.name),
            Some(&actual.name)
        );
    }
}

#[test]
fn string_tables_match_reference() {
    let tables = reference_registry().get("string_tables").unwrap().clone();
    let Value::Object(tables) = tables else {
        panic!("string_tables is not an object")
    };
    let mut expected_names: Vec<&str> = StringTable::ALL
        .iter()
        .map(|table| table.upstream_name())
        .collect();
    expected_names.sort_unstable();
    let mut names: Vec<&str> = tables.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(names, expected_names);
    for table in StringTable::ALL {
        let expected: Vec<&str> = tables[table.upstream_name()]
            .iter_values()
            .map(|value| value.to_str().unwrap())
            .collect();
        assert_eq!(
            table.values(),
            expected.as_slice(),
            "{}",
            table.upstream_name()
        );
    }
}

fn names(value: &Value) -> Vec<String> {
    value
        .iter_values()
        .map(|name| name.to_str().unwrap().to_string())
        .collect()
}

#[test]
fn engine_tables_match_reference() {
    let reference = reference_registry();
    assert_eq!(
        engine::controls(),
        names(reference.get("controls").unwrap())
    );
    assert_eq!(
        engine::modulation_sources(),
        names(reference.get("modulation_sources").unwrap())
    );
    assert_eq!(
        engine::modulation_destinations(),
        names(reference.get("mono_modulation_destinations").unwrap())
    );
    // SynthBase::isInvalidConnection never matches in this engine: no slot's
    // own processor owns a modulation destination, so the port has no such
    // rule. This keeps that assumption checked.
    for (slot, destinations) in reference
        .get("slot_self_destinations")
        .unwrap()
        .iter_values()
        .enumerate()
    {
        assert_eq!(destinations.size(), 0, "slot {slot} has self destinations");
    }
    assert_eq!(
        reference.get("synth_version").unwrap().to_str().unwrap(),
        sas_preset::version::SYNTH_VERSION
    );
    assert_eq!(
        reference
            .get("init")
            .unwrap()
            .get("random_generators_constructed")
            .unwrap()
            .to_i32()
            .unwrap(),
        1
    );
}

#[test]
fn every_preset_has_a_state_case() {
    let cases: Vec<String> = state_cases()
        .into_iter()
        .filter_map(|(_, preset)| preset)
        .collect();
    for directory in ["presets", "preset_tests"] {
        for entry in std::fs::read_dir(repo().join("migration/corpus").join(directory)).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().to_string();
            if name.ends_with(".vital") {
                let path = format!("{directory}/{name}");
                assert!(
                    cases.contains(&path),
                    "{path} has no case in modules/preset/state.json"
                );
            }
        }
    }
    let init = state_cases()
        .into_iter()
        .filter(|(_, preset)| preset.is_none())
        .count();
    assert_eq!(init, 1, "exactly one init case");
}

/// Loads each case's preset on a fresh synth (the reference loads every
/// preset on a newly constructed `SynthBase`), or the init preset, and
/// compares the saved state, the load error, and the second round trip.
#[test]
fn every_preset_matches_reference() {
    let cases = state_cases();
    assert!(cases.len() >= 21);
    let mut failures = Vec::new();
    for (case, preset) in &cases {
        let result = std::panic::catch_unwind(|| check_case(case, preset.as_deref()));
        if let Err(panic) = result {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
                .unwrap_or_default();
            failures.push(message);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

fn check_case(case: &ModuleCase, preset: Option<&str>) {
    let mut state = PresetState::new(&init_content());
    let artifacts = case.artifact_names();
    let Some(preset) = preset else {
        assert_same_state(case, "state.json", &state);
        check_round_trip(case, "init", &state);
        return;
    };

    let path = repo().join("migration/corpus").join(preset);
    let result = state.load_file(&path);
    if artifacts.iter().any(|name| name == "load_error.txt") {
        let message = case.artifact("load_error.txt");
        match (message.trim_end(), &result) {
            ("Preset was created with a newer version.", Err(Error::NewerVersion(_))) => {}
            ("Preset file is corrupted.", Err(Error::Corrupted(_))) => {}
            _ => panic!(
                "{}: reference fails with {message:?}, port gives {result:?}",
                case.id()
            ),
        }
        return;
    }
    if let Err(error) = result {
        panic!("{}: reference loads it, port fails: {error}", case.id());
    }
    assert_same_state(case, "state.json", &state);
    if artifacts.iter().any(|name| name == "roundtrip.json") {
        check_round_trip(case, path.file_stem().unwrap().to_str().unwrap(), &state);
    }
}

/// Saves `state` to a file named like the original and loads it again, as the
/// reference capture does.
fn reload(name: &str, state: &PresetState) -> PresetState {
    let directory = std::env::temp_dir().join(format!(
        "sas-preset-roundtrip-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{name}.vital"));
    std::fs::write(&path, state.to_json_string()).unwrap();
    let mut reloaded = PresetState::new(&init_content());
    reloaded.load_file(&path).unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
    reloaded
}

/// Load -> save -> load -> save matches the reference doing the same, and
/// everything except the sample is a fixed point after the first save. The
/// sample is not: upstream saves it four frames late (its zero padding is
/// encoded), so every round trip delays it again.
fn check_round_trip(case: &ModuleCase, name: &str, first: &PresetState) {
    let second = reload(name, first);
    assert_same_state(case, "roundtrip.json", &second);

    let strip = |value: &mut Value| {
        let Value::Object(top) = value else {
            panic!("state is not an object")
        };
        let Some(Value::Object(settings)) = top.get_mut("settings") else {
            panic!("no settings")
        };
        settings.remove("sample");
        // The init preset is unnamed; loading it from a file names it.
        top.remove("preset_name");
    };
    let (mut once, mut twice) = (first.to_json(), second.to_json());
    strip(&mut once);
    strip(&mut twice);
    assert_eq!(
        once.dump(),
        twice.dump(),
        "{}: second round trip changed more than the sample",
        case.id()
    );
}
