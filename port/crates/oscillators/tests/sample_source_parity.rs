// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `SampleSource` against every
//! `migration/corpus/modules/oscillators/*.json` case of the
//! `sample_source` module.
//!
//! Mirrors `migration/reference/src/modules/oscillators/sample_source.cpp`:
//! the player is constructed after the seed counter is set (it takes two
//! seeds: its generator and the default noise sample), `params.sample`
//! loads a PCM recipe through `Sample::json_to_state`, and the extra
//! control-rate output `phase` is the player's phase output. Every case runs
//! in one test because the seed counter is process-global.

mod support;

use std::sync::Mutex;

use sas_common::json::Value;
use sas_dsp_core::testing::{run_module, DriverInput, DriverSettings, InputRate, TriggerEvent};
use sas_dsp_core::{Output, Processor, ProcessorImpl, ProcessorRef};
use sas_oscillators::sample_source::{self as src, SampleSource};
use sas_parity::{assert_parity, ModuleCase};
use serde_json::Value as Json;

use InputRate::{Audio, Control, Trigger};

const PHASE: usize = src::NUM_OUTPUTS;

const INPUTS: &[(&str, usize, InputRate)] = &[
    ("reset", src::RESET, Trigger),
    ("midi", src::MIDI, Control),
    ("keytrack", src::KEYTRACK, Control),
    ("level", src::LEVEL, Audio),
    ("random_phase", src::RANDOM_PHASE, Control),
    ("transpose", src::TRANSPOSE, Control),
    ("transpose_quantize", src::TRANSPOSE_QUANTIZE, Control),
    ("tune", src::TUNE, Control),
    ("loop", src::LOOP, Control),
    ("bounce", src::BOUNCE, Control),
    ("pan", src::PAN, Control),
    ("note_count", src::NOTE_COUNT, Control),
];

const OUTPUTS: &[(&str, usize)] = &[
    ("raw", src::RAW),
    ("levelled", src::LEVELLED),
    ("phase", PHASE),
];

/// `SampleSourceProbe`: the player with its phase output copied to an
/// extra processor output after each block.
#[derive(Clone)]
struct SampleSourceProbe {
    source: SampleSource,
}

impl ProcessorImpl for SampleSourceProbe {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        self.source.process(this, num_samples);
        this.output(PHASE)
            .set_at(0, self.source.phase_output().at(0));
    }
}

fn recipe_json(spec: &Json, length: i64) -> Value {
    let mut recipe = sas_common::json::parse(&spec.to_string()).expect("recipe JSON");
    recipe.set("length", length);
    recipe.set("encoding", "pcm");
    recipe
}

/// The `Sample::jsonToState` input `sample_source.cpp` builds from
/// `params.sample`.
fn sample_json(spec: &Json) -> Value {
    let length = spec["length"].as_i64().expect("length");
    let mut data = Value::object();
    data.set("name", "S");
    data.set("length", length);
    data.set(
        "sample_rate",
        spec["sample_rate"].as_i64().expect("sample_rate"),
    );
    data.set(
        "samples",
        support::encode(&recipe_json(&spec["recipe"], length)),
    );
    if let Some(stereo) = spec.get("recipe_stereo") {
        data.set(
            "samples_stereo",
            support::encode(&recipe_json(stereo, length)),
        );
    }
    data
}

fn create(case: &ModuleCase) -> ProcessorRef {
    let seed = case.spec()["seed"].as_i64().expect("cases set a seed") as i32;
    sas_common::random::set_next_seed(seed);
    let source = SampleSource::new();
    if let Some(spec) = case.params().get("sample") {
        source
            .sample()
            .borrow_mut()
            .json_to_state(&sample_json(spec))
            .expect("sample loads");
    }
    let processor = Processor::new(
        SampleSourceProbe { source },
        src::NUM_INPUTS,
        src::NUM_OUTPUTS,
        false,
        1,
    );
    processor.register_output(Output::new_control_rate());
    processor
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

fn run_case(case: &ModuleCase) -> Vec<Vec<[f32; 4]>> {
    let processor = create(case);
    let inputs: Vec<DriverInput> = INPUTS
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
    let outputs: Vec<usize> = OUTPUTS.iter().map(|&(_, index)| index).collect();
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

static SEED_COUNTER: Mutex<()> = Mutex::new(());

#[test]
fn sample_source_matches_reference() {
    let _guard = SEED_COUNTER.lock().unwrap();
    let ids: Vec<String> = sas_parity::all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with("oscillators/"))
        .filter(|id| ModuleCase::load(id).module() == "sample_source")
        .collect();
    assert!(!ids.is_empty());

    let mut failures = Vec::new();
    let mut checked = Vec::new();
    let mut worst_abs = (0.0f64, String::new());
    let mut worst_residual = (f64::NEG_INFINITY, String::new());
    println!("{:<64} {:>11} {:>11}", "case:output", "max_abs", "residual");
    for id in &ids {
        let case = ModuleCase::load(id);
        assert_capture_matches_semantics(id);
        let outputs = run_case(&case);
        for (&(name, _), data) in OUTPUTS.iter().zip(&outputs) {
            let label = format!("{id}:{name}");
            let report = case.report(name, data);
            println!(
                "{:<64} {:>11.3e} {:>11.1}",
                label, report.max_abs_error, report.residual_db
            );
            if report.max_abs_error > worst_abs.0 {
                worst_abs = (report.max_abs_error, label.clone());
            }
            if report.residual_db > worst_residual.0 && report.residual_db.is_finite() {
                worst_residual = (report.residual_db, label.clone());
            }
            if !report.pass {
                failures.push(label.clone());
            }
        }
        checked.push((id.clone(), outputs));
    }
    println!(
        "{} cases; worst max_abs_error {:.3e} ({}), worst residual {:.1} dB ({})",
        ids.len(),
        worst_abs.0,
        worst_abs.1,
        worst_residual.0,
        worst_residual.1
    );

    if !failures.is_empty() {
        eprintln!("failing outputs: {failures:?}");
    }
    for (id, outputs) in &checked {
        for (&(name, _), data) in OUTPUTS.iter().zip(outputs) {
            assert_parity!(&format!("{id}:{name}"), data);
        }
    }
}
