// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared parity-test harness for `migration/corpus/modules/modulation/*`.
//!
//! Each test file mirrors the matching C++ registrations in
//! `migration/reference/src/modules/modulation/*.cpp` (names, input
//! indices and rates, outputs, factories) as a [`ModuleDef`], and
//! [`check_module`] drives every case of that module through
//! [`sas_dsp_core::testing::run_module`] and asserts each output with
//! `assert_parity!` under the case's tolerances. Run with `--nocapture`
//! for per-case metrics.

#![allow(dead_code)]

pub mod parameters;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

use sas_common::LineGenerator;

use sas_dsp_core::testing::{run_module, DriverInput, DriverSettings, InputRate, TriggerEvent};
use sas_dsp_core::ProcessorRef;
use sas_parity::{assert_parity, ModuleCase};
use serde_json::Value as Json;

pub use InputRate::{Audio, Control, Trigger};

pub type Factory = Box<dyn Fn(&Json) -> ProcessorRef>;
pub type Normalize = fn(&ProcessorRef, &mut [[f32; 4]]);

/// One registered `ref-module` module.
pub struct ModuleDef {
    pub name: &'static str,
    pub create: Factory,
    pub inputs: Vec<(&'static str, usize, InputRate)>,
    /// Output name, processor output index, and the normalization the C++
    /// module applies before capture (if any).
    pub outputs: Vec<(&'static str, usize, Option<Normalize>)>,
}

impl ModuleDef {
    pub fn new(
        name: &'static str,
        create: impl Fn(&Json) -> ProcessorRef + 'static,
        inputs: &[(&'static str, usize, InputRate)],
        outputs: &[(&'static str, usize)],
    ) -> ModuleDef {
        ModuleDef {
            name,
            create: Box::new(create),
            inputs: inputs.to_vec(),
            outputs: outputs.iter().map(|&(n, i)| (n, i, None)).collect(),
        }
    }

    /// Marks output `name` as captured through `normalize`.
    pub fn normalized(mut self, name: &str, normalize: Normalize) -> ModuleDef {
        let output = self
            .outputs
            .iter_mut()
            .find(|(n, _, _)| *n == name)
            .expect("unknown output");
        output.2 = Some(normalize);
        self
    }
}

/// `frequency * (1.0f / getSampleRate())`: a frequency captured as
/// SynthLfo's per-sample phase increment (`synth_lfo.cpp:411-413`), the
/// normalized form of `PhaseIncrementOf` in the C++ modules.
pub fn phase_increment(processor: &ProcessorRef, data: &mut [[f32; 4]]) {
    let tick_time = 1.0f32 / processor.sample_rate() as f32;
    for value in data.iter_mut().flatten() {
        *value *= tick_time;
    }
}

/// Reads a float param.
pub fn param(params: &Json, name: &str, fallback: f32) -> f32 {
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

/// `refmod_modulation::applyLineShape` (`line_shape.h`): applies the shape
/// named or described by `params[key]`; leaves `line` alone when absent.
pub fn apply_line_shape(line: &mut LineGenerator, params: &Json, key: &str) {
    match params.get(key) {
        None => {}
        Some(Json::String(name)) => match name.as_str() {
            "linear" => line.init_linear(),
            "triangle" => line.init_triangle(),
            "square" => line.init_square(),
            "sin" => line.init_sin(),
            "saw_up" => line.init_saw_up(),
            "saw_down" => line.init_saw_down(),
            other => panic!("unknown shape {other}"),
        },
        Some(shape) => {
            let points = shape["points"].as_array().expect("shape.points");
            line.set_num_points(points.len());
            for (i, point) in points.iter().enumerate() {
                let x = point[0].as_f64().unwrap() as f32;
                let y = point[1].as_f64().unwrap() as f32;
                line.set_point(i, (x, y));
                let power = shape
                    .get("powers")
                    .map_or(0.0, |powers| powers[i].as_f64().unwrap() as f32);
                line.set_power(i, power);
            }
            line.set_smooth(param_bool(shape, "smooth", false));
            line.render();
        }
    }
}

/// `refmod_modulation::createLineShape`: a new generator with `params[key]`
/// applied, `"triangle"` when absent.
pub fn line_shape(params: &Json, key: &str) -> Rc<RefCell<LineGenerator>> {
    let mut line = LineGenerator::default();
    line.init_triangle();
    apply_line_shape(&mut line, params, key);
    Rc::new(RefCell::new(line))
}

/// Sets `params.controls` on a module after `init()`, as the C++ factories do.
pub fn set_controls(module: &ProcessorRef, params: &Json) {
    let Some(values) = params.get("controls").and_then(Json::as_object) else {
        return;
    };
    let controls = module.get_controls();
    for (name, value) in values {
        let control = controls
            .get(name)
            .unwrap_or_else(|| panic!("module has no control {name}"));
        control.set_value(value.as_f64().unwrap() as f32);
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

/// Serializes tests in one binary: processors draw seeds from the global
/// counter (`sas_common::random`), which the cases reset.
fn lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Builds the processor and drives it as `ref-module` does.
pub fn run_case(case: &ModuleCase, module: &ModuleDef) -> Vec<Vec<[f32; 4]>> {
    if let Some(seed) = case.spec().get("seed").and_then(Json::as_i64) {
        sas_common::random::set_next_seed(seed as i32);
    }
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
    let indices: Vec<usize> = module.outputs.iter().map(|&(_, index, _)| index).collect();
    let mut data = run_module(&processor, &inputs, &settings, &indices);
    for ((_, _, normalize), output) in module.outputs.iter().zip(data.iter_mut()) {
        if let Some(normalize) = normalize {
            normalize(&processor, output);
        }
    }
    data
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

/// Case ids of `modulation/<module>/*`.
pub fn case_ids(module: &str) -> Vec<String> {
    let prefix = format!("modulation/{module}/");
    sas_parity::all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with(&prefix))
        .collect()
}

/// Runs every case of `module` and asserts each output against the
/// reference. Prints the metrics of every output first, so one run shows
/// the whole module.
pub fn check_module(module: &ModuleDef) {
    let _guard = lock();
    parameters::install();
    let ids = case_ids(module.name);
    assert!(!ids.is_empty(), "no cases for modulation/{}", module.name);

    let mut results = Vec::new();
    println!(
        "{:<64} {:>11} {:>11} {:>6}",
        "case:output", "max_abs", "residual", "exact"
    );
    for id in &ids {
        let case = ModuleCase::load(id);
        assert_eq!(case.module(), module.name, "{id}");
        assert_capture_matches_semantics(id);
        let outputs = run_case(&case, module);
        for (&(name, _, _), data) in module.outputs.iter().zip(outputs) {
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
            results.push((format!("{id}:{name}"), data));
        }
    }
    for (label, data) in &results {
        assert_parity!(label, data);
    }
}

type BlockHook = dyn Fn(&ProcessorRef, u64, u64);

/// Kernel of a test wrapper that mirrors the C++ ref-module subclasses that
/// act before each block (`Clocked*`, `ScaleChangingConnection`): it calls
/// `hook(inner, block_index, frames_before)` and then processes the wrapped
/// processor. The wrapper uses the wrapped processor's input and output
/// objects, so `run_module` drives it unchanged.
#[derive(Clone)]
struct Hooked {
    inner: ProcessorRef,
    hook: Rc<BlockHook>,
    block: u64,
    frames: u64,
}

impl sas_dsp_core::ProcessorImpl for Hooked {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, _this: &sas_dsp_core::Processor, num_samples: usize) {
        (self.hook)(&self.inner, self.block, self.frames);
        self.inner.process(num_samples);
        self.block += 1;
        self.frames += num_samples as u64;
    }

    fn set_sample_rate(&mut self, this: &sas_dsp_core::Processor, sample_rate: i32) {
        this.processor_set_sample_rate(sample_rate);
        self.inner.set_sample_rate(sample_rate);
    }

    fn set_oversample_amount(&mut self, this: &sas_dsp_core::Processor, oversample: i32) {
        this.processor_set_oversample_amount(oversample);
        self.inner.set_oversample_amount(oversample);
    }

    fn reset(&mut self, _this: &sas_dsp_core::Processor, reset_mask: sas_simd::PolyMask) {
        self.inner.reset(reset_mask);
    }

    fn hard_reset(&mut self, _this: &sas_dsp_core::Processor) {
        self.inner.hard_reset();
    }
}

/// Wraps `inner` so `hook(inner, block_index, frames_before)` runs before
/// each block.
pub fn per_block(
    inner: ProcessorRef,
    hook: impl Fn(&ProcessorRef, u64, u64) + 'static,
) -> ProcessorRef {
    let num_inputs = inner.num_inputs();
    let num_outputs = inner.num_outputs();
    let wrapper = sas_dsp_core::Processor::new(
        Hooked {
            inner: inner.clone(),
            hook: Rc::new(hook),
            block: 0,
            frames: 0,
        },
        0,
        0,
        inner.is_control_rate(),
        1,
    );
    for i in 0..num_inputs {
        wrapper.register_input(inner.input(i));
    }
    for i in 0..num_outputs {
        wrapper.register_output(inner.output(i));
    }
    wrapper
}

/// Mirrors the C++ `Clocked*` subclasses: before each block, calls
/// `correct_to_time(inner, start + frames / rate)`, as the engine does every
/// block.
pub fn clocked(
    inner: ProcessorRef,
    start: f64,
    correct_to_time: impl Fn(&ProcessorRef, f64) + 'static,
) -> ProcessorRef {
    per_block(inner, move |inner, _block, frames| {
        correct_to_time(inner, start + frames as f64 / inner.sample_rate() as f64)
    })
}
