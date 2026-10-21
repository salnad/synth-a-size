// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared parity-test driver for the effect captures
//! (`migration/corpus/modules/effects/*.json`).
//!
//! Each test binary builds a table that mirrors the C++ module files in
//! `migration/reference/src/modules/effects/` (names, input indices and
//! rates, outputs, factories), and [`check_modules`] runs every case of
//! those modules through [`sas_dsp_core::testing::run_module`] (the
//! `ref-module` driving sequence) and asserts parity on every output.

#![allow(dead_code)]

pub mod harness;
pub mod params;

use std::collections::BTreeMap;

use sas_dsp_core::testing::{run_module, DriverInput, DriverSettings, InputRate, TriggerEvent};
use sas_dsp_core::ProcessorRef;
use sas_parity::ModuleCase;
use serde_json::Value as Json;

pub type Factory = Box<dyn Fn(&Json) -> ProcessorRef>;

pub struct ModuleDef {
    pub create: Factory,
    pub inputs: Vec<(&'static str, usize, InputRate)>,
    pub outputs: Vec<(&'static str, usize)>,
}

pub fn def(
    create: impl Fn(&Json) -> ProcessorRef + 'static,
    inputs: Vec<(&'static str, usize, InputRate)>,
    outputs: &[(&'static str, usize)],
) -> ModuleDef {
    ModuleDef {
        create: Box::new(create),
        inputs,
        outputs: outputs.to_vec(),
    }
}

pub fn param_i32(params: &Json, name: &str, fallback: i32) -> i32 {
    params
        .get(name)
        .and_then(Json::as_i64)
        .map_or(fallback, |v| v as i32)
}

fn input_rate(case: &ModuleCase, name: &str, registry: InputRate) -> InputRate {
    let spec = &case.spec()["inputs"][name];
    if spec.get("trigger").is_some() {
        return InputRate::Trigger;
    }
    match spec.get("rate").and_then(Json::as_str) {
        Some("audio") => InputRate::Audio,
        Some("control") => InputRate::Control,
        _ => registry,
    }
}

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

/// The case ids of `effects/<module>/*` for each module in `table`. Every
/// module needs loaded cases, except the `parked` ones: those must have none
/// loaded and all of theirs in `migration/corpus/modules/effects/pending/`.
pub fn case_ids(table: &BTreeMap<&'static str, ModuleDef>, parked: &[&str]) -> Vec<String> {
    let ids: Vec<String> = sas_parity::all_case_ids()
        .into_iter()
        .filter(|id| {
            id.strip_prefix("effects/")
                .and_then(|rest| rest.split('/').next())
                .is_some_and(|file| table.contains_key(file))
        })
        .collect();
    for name in table.keys() {
        let loaded = ids
            .iter()
            .any(|id| id.starts_with(&format!("effects/{name}/")));
        if parked.contains(name) {
            let pending = sas_parity::cases::corpus_dir()
                .join("modules/effects/pending")
                .join(format!("{name}.json"));
            assert!(
                !loaded && pending.exists(),
                "effects/{name} is listed as parked: it must have no loaded cases and a {}",
                pending.display()
            );
        } else {
            assert!(loaded, "no cases for effects/{name}");
        }
    }
    ids
}

/// Runs every case of the modules in `table` and asserts parity on every
/// output, printing per-output metrics first (`--nocapture`).
pub fn check_modules(table: &BTreeMap<&'static str, ModuleDef>) {
    check_modules_with_parked(table, &[]);
}

/// [`check_modules`] where the `parked` modules have all their cases in
/// `pending/` (see [`case_ids`]).
pub fn check_modules_with_parked(table: &BTreeMap<&'static str, ModuleDef>, parked: &[&str]) {
    params::install();
    let ids = case_ids(table, parked);
    let mut results = Vec::new();
    println!(
        "{:<64} {:>11} {:>11} {:>6}",
        "case:output", "max_abs", "residual", "exact"
    );
    for id in &ids {
        let case = ModuleCase::load(id);
        assert_capture_matches_semantics(id);
        let module = table
            .get(case.module())
            .unwrap_or_else(|| panic!("{id}: module {} has no Rust mirror", case.module()));
        let outputs = run_case(&case, module);
        for (&(name, _), data) in module.outputs.iter().zip(outputs) {
            let reference = case.reference(name);
            let identical = reference.len() == data.len()
                && reference
                    .iter()
                    .flatten()
                    .zip(data.iter().flatten())
                    .all(|(r, t)| r.to_bits() == t.to_bits() || (r.is_nan() && t.is_nan()));
            let report = case.report(name, &data);
            println!(
                "{:<64} {:>11.3e} {:>11.1} {:>6}",
                format!("{id}:{name}"),
                report.max_abs_error,
                report.residual_db,
                identical
            );
            results.push((report.pass, id.clone(), name, data));
        }
    }
    // Failing outputs first, so the panic names the first failure.
    results.sort_by_key(|(pass, ..)| *pass);
    for (_, id, name, data) in &results {
        ModuleCase::load(id).assert_parity(name, data);
    }
}
