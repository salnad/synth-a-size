// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared parity harness for the `filters/*` module cases.
//!
//! [`ModuleDef`] mirrors one `migration/reference/src/modules/filters/*.cpp`
//! registration (input names, indices and registry rates, outputs, factory);
//! [`run_case`] drives the port the way `ref-module` does through
//! [`sas_dsp_core::testing::run_module`], and [`check_file`] asserts every
//! case of one case file against its reference capture under the case's
//! tolerances.

#![allow(dead_code)]

use sas_dsp_core::testing::{run_module, DriverInput, DriverSettings, InputRate, TriggerEvent};
use sas_dsp_core::ProcessorRef;
use sas_filters::synth_filter::{create_filter, input, FilterModel};
use sas_parity::ModuleCase;
use serde_json::Value as Json;

pub use InputRate::{Audio, Control, Trigger};

pub type Factory = fn(&Json) -> ProcessorRef;

pub struct ModuleDef {
    pub create: Factory,
    pub inputs: Vec<(&'static str, usize, InputRate)>,
    pub outputs: Vec<(&'static str, usize)>,
}

pub fn param_f32(params: &Json, name: &str, fallback: f32) -> f32 {
    params
        .get(name)
        .and_then(Json::as_f64)
        .map_or(fallback, |v| v as f32)
}

pub fn param_bool(params: &Json, name: &str, fallback: bool) -> bool {
    params.get(name).and_then(Json::as_bool).unwrap_or(fallback)
}

pub fn param_str<'a>(params: &'a Json, name: &str, fallback: &'a str) -> &'a str {
    params.get(name).and_then(Json::as_str).unwrap_or(fallback)
}

/// `filterModelFromName` in `synth_filter.cpp` (the "filter" ref-module).
pub fn filter_model_from_name(name: &str) -> Option<FilterModel> {
    Some(match name {
        "analog" => FilterModel::Analog,
        "dirty" => FilterModel::Dirty,
        "ladder" => FilterModel::Ladder,
        "digital" => FilterModel::Digital,
        "diode" => FilterModel::Diode,
        "formant" => FilterModel::Formant,
        "comb" => FilterModel::Comb,
        "phase" => FilterModel::Phase,
        _ => return None,
    })
}

/// The "filter" module's inputs (`SynthFilter` input layout).
pub fn synth_filter_inputs() -> Vec<(&'static str, usize, InputRate)> {
    vec![
        ("audio", input::AUDIO, Audio),
        ("reset", input::RESET, Trigger),
        ("midi_cutoff", input::MIDI_CUTOFF, Audio),
        ("resonance", input::RESONANCE, Control),
        ("drive_gain", input::DRIVE_GAIN, Control),
        ("gain", input::GAIN, Control),
        ("style", input::STYLE, Control),
        ("pass_blend", input::PASS_BLEND, Control),
        ("interpolate_x", input::INTERPOLATE_X, Control),
        ("interpolate_y", input::INTERPOLATE_Y, Control),
        ("transpose", input::TRANSPOSE, Control),
        ("spread", input::SPREAD, Control),
    ]
}

/// `filters/synth_filter.cpp`: module "filter".
pub fn filter_module() -> ModuleDef {
    ModuleDef {
        create: |params| {
            let name = param_str(params, "model", "digital");
            let model = filter_model_from_name(name)
                .unwrap_or_else(|| panic!("unknown filter model {name:?}"));
            create_filter(model).unwrap_or_else(|| panic!("filter model {name:?} is not ported"))
        },
        inputs: synth_filter_inputs(),
        outputs: vec![("audio", 0)],
    }
}

fn input_rate(case: &ModuleCase, name: &str, registry: InputRate) -> InputRate {
    let spec = &case.spec()["inputs"][name];
    if spec.get("trigger").is_some() {
        return Trigger;
    }
    match spec.get("rate").and_then(Json::as_str) {
        Some("audio") => Audio,
        Some("control") => Control,
        _ => registry,
    }
}

/// Runs one case through the port; one frame vector per output.
pub fn run_case(case: &ModuleCase, module: &ModuleDef) -> Vec<Vec<[f32; 4]>> {
    let processor = (module.create)(case.params());
    let inputs: Vec<DriverInput> = module
        .inputs
        .iter()
        .map(|&(name, index, rate)| DriverInput {
            index,
            rate: input_rate(case, name, rate),
            frames: case.input(name),
            triggers: case
                .triggers(name)
                .into_iter()
                .map(|t| TriggerEvent {
                    sample: t.sample,
                    value: t.value,
                    lanes: t.lanes,
                })
                .collect(),
        })
        .collect();
    let settings = DriverSettings {
        sample_rate: case.sample_rate() as i32,
        oversample: case.oversample() as i32,
        block_size: case.block_size(),
        num_samples: case.num_samples(),
    };
    let outputs: Vec<usize> = module.outputs.iter().map(|&(_, index)| index).collect();
    run_module(&processor, &inputs, &settings, &outputs)
}

/// Captures must come from a reference of the ISA whose semantics this build
/// uses (`migration/decisions/reference-arm.md`).
fn assert_capture_matches_semantics(id: &str) {
    let path = sas_parity::captures::captures_dir()
        .join("modules")
        .join(id)
        .join("capture.json");
    let capture: Json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let arch = capture["build"]["arch"].as_str().unwrap_or_default();
    assert_eq!(
        matches!(arch, "arm64" | "aarch64"),
        sas_simd::AARCH64_SEMANTICS,
        "captures in {} were rendered on {arch:?}; compare against a reference of the same ISA",
        path.display()
    );
}

/// The case ids of `migration/corpus/modules/filters/<file>.json`.
pub fn case_ids(file: &str) -> Vec<String> {
    let prefix = format!("filters/{file}/");
    sas_parity::all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with(&prefix))
        .collect()
}

/// The ids of every `filters/*` case whose module is `module`, in any file.
pub fn module_case_ids(module: &str) -> Vec<String> {
    sas_parity::cases::load_module_cases()
        .unwrap_or_else(|e| panic!("sas-parity: {e}"))
        .into_iter()
        .filter(|case| case.id.starts_with("filters/") && case.json["spec"]["module"] == module)
        .map(|case| case.id)
        .collect()
}

/// Runs and asserts every case of `filters/<file>.json` against the
/// reference.
pub fn check_file(file: &str, module: &ModuleDef) {
    let ids = case_ids(file);
    assert!(!ids.is_empty(), "no cases in filters/{file}.json");
    check_ids(&ids, module);
}

/// Runs and asserts every `filters/*` case of ref-module `name`, whatever
/// file it is in.
pub fn check_module(name: &str, module: &ModuleDef) {
    let ids = module_case_ids(name);
    assert!(!ids.is_empty(), "no filters/* cases use module {name:?}");
    check_ids(&ids, module);
}

/// Runs and asserts every `filters/*` case of the generic "filter"
/// ref-module with `params.model == model`, whatever file it is in.
pub fn check_filter_model(model: &str) {
    let ids: Vec<String> = sas_parity::cases::load_module_cases()
        .unwrap_or_else(|e| panic!("sas-parity: {e}"))
        .into_iter()
        .filter(|case| {
            case.id.starts_with("filters/")
                && case.json["spec"]["module"] == "filter"
                && case.json["spec"]["params"]["model"] == model
        })
        .map(|case| case.id)
        .collect();
    assert!(
        !ids.is_empty(),
        "no filters/* cases for filter model {model:?}"
    );
    check_ids(&ids, &filter_module());
}

/// Prints per-case metrics (`--nocapture`), then asserts each output with
/// `assert_parity`, so a failure names its case and metrics.
pub fn check_ids(ids: &[String], module: &ModuleDef) {
    let mut results = Vec::new();
    for id in ids {
        let case = ModuleCase::load(id);
        assert_capture_matches_semantics(id);
        let outputs = run_case(&case, module);
        for (&(name, _), data) in module.outputs.iter().zip(outputs) {
            let reference = case.reference(name);
            let identical = reference.len() == data.len()
                && reference
                    .iter()
                    .flatten()
                    .zip(data.iter().flatten())
                    .all(|(r, t)| r.to_bits() == t.to_bits());
            let report = case.report(name, &data);
            println!(
                "{:<64} max_abs {:>10.3e}  residual {:>7.1} dB  exact {}",
                format!("{id}:{name}"),
                report.max_abs_error,
                report.residual_db,
                identical
            );
            results.push((case.clone(), name, data));
        }
    }
    for (case, name, data) in &results {
        case.assert_parity(name, data);
    }
}
