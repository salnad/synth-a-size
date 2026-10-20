// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity against the C++ reference for every `framework/*` module case
//! (`migration/corpus/modules/framework/*.json`), under the corpus
//! tolerances.
//!
//! The module table mirrors `migration/reference/src/modules/framework/*.cpp`
//! (names, input indices and rates, outputs, factories and graphs), and
//! [`sas_dsp_core::testing::run_module`] drives each processor the way
//! `ref-module` does. Run with `--nocapture` for per-case metrics.

use std::collections::BTreeMap;

use sas_dsp_core::operators::{self, cr, ModulationSum};
use sas_dsp_core::testing::{run_module, DriverInput, DriverSettings, InputRate, TriggerEvent};
use sas_dsp_core::utilities::peak_meter::PeakMeter;
use sas_dsp_core::utilities::portamento_slope::PortamentoSlope;
use sas_dsp_core::utilities::smooth_value::{CrSmoothValue, SmoothValue};
use sas_dsp_core::{
    Memory, OneDimLookup, OnePoleFilter, Processor, ProcessorImpl, ProcessorRef, ProcessorRouter,
    StereoMemory, Value,
};
use sas_parity::ModuleCase;
use sas_simd::{futils, PolyFloat, PolyMask};
use serde_json::Value as Json;

use InputRate::{Audio, Control, Trigger};

type Factory = Box<dyn Fn(&Json) -> ProcessorRef>;

struct ModuleDef {
    create: Factory,
    inputs: Vec<(&'static str, usize, InputRate)>,
    outputs: Vec<(&'static str, usize)>,
}

fn p(params: &Json, name: &str, fallback: f32) -> f32 {
    params
        .get(name)
        .and_then(Json::as_f64)
        .map_or(fallback, |v| v as f32)
}

fn def(
    create: impl Fn(&Json) -> ProcessorRef + 'static,
    inputs: &[(&'static str, usize, InputRate)],
    outputs: &[(&'static str, usize)],
) -> ModuleDef {
    ModuleDef {
        create: Box::new(create),
        inputs: inputs.to_vec(),
        outputs: outputs.to_vec(),
    }
}

const OUT: &[(&str, usize)] = &[("out", 0)];

// ----- graph helpers (graph.cpp) -----

#[derive(Clone)]
struct Passthrough;

impl ProcessorImpl for Passthrough {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let source = this.input_buffer(0);
        let dest = this.output_buffer(0);
        let samples = if this.is_control_rate() {
            1
        } else {
            num_samples
        };
        for i in 0..samples {
            dest[i].set(source[i].get());
        }
    }
}

fn passthrough(control_rate: bool) -> ProcessorRef {
    Processor::new(Passthrough, 1, 1, control_rate, 1)
}

fn modulation_sum_graph(_: &Json) -> ProcessorRef {
    let router = ProcessorRouter::create(3, 1, false);
    let control = passthrough(true);
    let audio = passthrough(false);
    control.use_input(&router.input(1), 0);
    audio.use_input(&router.input(2), 0);

    let sum = ModulationSum::create(0);
    sum.use_input(&router.input(0), ModulationSum::RESET);
    sum.plug_next(&control);
    sum.plug_next(&audio);
    sum.use_output(&router.output(0), 0);

    router.add_processor(&control);
    router.add_processor(&audio);
    router.add_processor(&sum);
    router
}

fn feedback_cycle_graph(params: &Json) -> ProcessorRef {
    let router = ProcessorRouter::create(1, 1, true);
    let input = passthrough(true);
    let out = passthrough(true);
    let add = cr::Add::create();
    let multiply = cr::Multiply::create();
    let gain = Value::create_control_rate(p(params, "gain", 0.5));

    input.use_input(&router.input(0), 0);
    out.use_output(&router.output(0), 0);

    router.add_processor(&out);
    router.add_processor(&multiply);
    router.add_processor(&add);
    router.add_processor(&input);
    router.add_idle_processor(&gain);

    add.plug(&input, 0);
    multiply.plug(&add, 0);
    multiply.plug(&gain, 1);
    add.plug(&multiply, 1);
    out.plug(&add, 0);
    router
}

fn audio_chain_graph(params: &Json) -> ProcessorRef {
    let router = ProcessorRouter::create(1, 1, false);
    let input = passthrough(false);
    let out = passthrough(false);
    let negate = operators::Negate::create();
    let scale = operators::LinearScale::create(p(params, "scale", 0.5));
    let add = operators::Add::create();

    input.use_input(&router.input(0), 0);
    out.use_output(&router.output(0), 0);

    router.add_processor(&out);
    router.add_processor(&add);
    router.add_processor(&scale);
    router.add_processor(&negate);
    router.add_processor(&input);

    negate.plug(&input, 0);
    scale.plug(&negate, 0);
    add.plug(&scale, 0);
    add.plug(&input, 1);
    out.plug(&add, 0);
    router
}

// ----- probes (lookups.cpp) -----

#[derive(Clone)]
enum MemoryKind {
    Mono(Memory),
    Stereo(StereoMemory),
}

#[derive(Clone)]
struct MemoryProbe {
    memory: MemoryKind,
}

impl ProcessorImpl for MemoryProbe {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio = this.input_buffer(0);
        let delay = this.input_buffer(1);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            let value = match &mut self.memory {
                MemoryKind::Mono(m) => {
                    m.push(audio[i].get());
                    m.get(delay[i].get())
                }
                MemoryKind::Stereo(m) => {
                    m.push(audio[i].get());
                    m.get(delay[i].get())
                }
            };
            dest[i].set(value);
        }
    }
}

#[derive(Clone)]
struct LookupProbe {
    lookup: OneDimLookup,
}

impl ProcessorImpl for LookupProbe {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let source = this.input_buffer(0);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            dest[i].set(self.lookup.cubic_lookup(source[i].get()));
        }
    }
}

#[derive(Clone)]
struct OnePoleProbe {
    filter: OnePoleFilter,
    mode: i64,
}

impl ProcessorImpl for OnePoleProbe {
    sas_dsp_core::processor_boilerplate!();

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.filter.reset(reset_mask);
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio = this.input_buffer(0);
        let cutoff = this.input_buffer(1);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            let coefficient =
                OnePoleFilter::compute_coefficient(cutoff[i].get(), this.sample_rate());
            let value = match self.mode {
                0 => self.filter.tick_basic(audio[i].get(), coefficient),
                1 => self.filter.tick(audio[i].get(), coefficient),
                _ => self.filter.tick_derivative(audio[i].get(), coefficient),
            };
            dest[i].set(value);
        }
    }
}

fn modules() -> BTreeMap<&'static str, ModuleDef> {
    let mut m = BTreeMap::new();
    let a1 = [("audio", 0, Audio)];
    let ab = [("a", 0, Audio), ("b", 1, Audio)];
    let cr_in = [("in", 0, Control)];
    let cr_ab = [("a", 0, Control), ("b", 1, Control)];

    // operators.cpp
    m.insert(
        "op_clamp",
        def(
            |p_| operators::Clamp::create(p(p_, "min", -1.0), p(p_, "max", 1.0)),
            &a1,
            OUT,
        ),
    );
    m.insert("op_negate", def(|_| operators::Negate::create(), &a1, OUT));
    m.insert(
        "op_inverse",
        def(|_| operators::Inverse::create(), &a1, OUT),
    );
    m.insert(
        "op_linear_scale",
        def(
            |p_| operators::LinearScale::create(p(p_, "scale", 1.0)),
            &a1,
            OUT,
        ),
    );
    m.insert("op_square", def(|_| operators::Square::create(), &a1, OUT));
    m.insert("op_add", def(|_| operators::Add::create(), &ab, OUT));
    m.insert(
        "op_subtract",
        def(|_| operators::Subtract::create(), &ab, OUT),
    );
    m.insert(
        "op_multiply",
        def(|_| operators::Multiply::create(), &ab, OUT),
    );
    m.insert(
        "op_variable_add",
        def(
            |_| operators::VariableAdd::create(3),
            &[("a", 0, Audio), ("b", 1, Audio), ("c", 2, Audio)],
            OUT,
        ),
    );
    let smooth_inputs = [
        ("audio", 0, Audio),
        ("multiply", 1, Control),
        ("reset", 2, Trigger),
    ];
    m.insert(
        "op_smooth_multiply",
        def(|_| operators::SmoothMultiply::create(), &smooth_inputs, OUT),
    );
    m.insert(
        "op_smooth_volume",
        def(
            |p_| {
                operators::SmoothVolume::create(p(
                    p_,
                    "max_db",
                    operators::SmoothVolume::DEFAULT_MAX_DB,
                ))
            },
            &[
                ("audio", 0, Audio),
                ("db", 1, Control),
                ("reset", 2, Trigger),
            ],
            OUT,
        ),
    );
    m.insert(
        "op_interpolate",
        def(
            |_| operators::Interpolate::create(),
            &[
                ("from", 0, Audio),
                ("to", 1, Audio),
                ("fractional", 2, Audio),
                ("reset", 3, Trigger),
            ],
            OUT,
        ),
    );
    m.insert(
        "op_bilinear_interpolate",
        def(
            |_| operators::BilinearInterpolate::create(),
            &[
                ("top_left", 0, Control),
                ("top_right", 1, Control),
                ("bottom_left", 2, Control),
                ("bottom_right", 3, Control),
                ("x", 4, Audio),
                ("y", 5, Audio),
            ],
            OUT,
        ),
    );
    m.insert(
        "op_sample_and_hold",
        def(
            |_| operators::SampleAndHoldBuffer::create(),
            &[("audio", 0, Control)],
            OUT,
        ),
    );
    m.insert(
        "op_stereo_encoder",
        def(
            |p_| operators::StereoEncoder::create(p(p_, "decoding", 0.0) != 0.0),
            &[
                ("audio", 0, Audio),
                ("encoding", 1, Control),
                ("mode", 2, Control),
            ],
            OUT,
        ),
    );
    m.insert(
        "op_tempo_chooser",
        def(
            |_| operators::TempoChooser::create(),
            &[
                ("frequency", 0, Control),
                ("tempo_index", 1, Control),
                ("beats_per_second", 2, Control),
                ("sync", 3, Control),
                ("midi", 4, Control),
                ("keytrack_transpose", 5, Control),
                ("keytrack_tune", 6, Control),
            ],
            OUT,
        ),
    );
    m.insert(
        "op_cr_clamp",
        def(
            |p_| cr::Clamp::create(p(p_, "min", -1.0), p(p_, "max", 1.0)),
            &cr_in,
            OUT,
        ),
    );
    m.insert(
        "op_cr_lower_bound",
        def(|p_| cr::LowerBound::create(p(p_, "min", 0.0)), &cr_in, OUT),
    );
    m.insert(
        "op_cr_upper_bound",
        def(|p_| cr::UpperBound::create(p(p_, "max", 0.0)), &cr_in, OUT),
    );
    m.insert("op_cr_square", def(|_| cr::Square::create(), &cr_in, OUT));
    m.insert("op_cr_cube", def(|_| cr::Cube::create(), &cr_in, OUT));
    m.insert("op_cr_quart", def(|_| cr::Quart::create(), &cr_in, OUT));
    m.insert(
        "op_cr_quadratic",
        def(
            |p_| cr::Quadratic::create(p(p_, "offset", 0.0)),
            &cr_in,
            OUT,
        ),
    );
    m.insert(
        "op_cr_cubic",
        def(|p_| cr::Cubic::create(p(p_, "offset", 0.0)), &cr_in, OUT),
    );
    m.insert(
        "op_cr_quartic",
        def(|p_| cr::Quartic::create(p(p_, "offset", 0.0)), &cr_in, OUT),
    );
    m.insert(
        "op_cr_root",
        def(|p_| cr::Root::create(p(p_, "offset", 0.0)), &cr_in, OUT),
    );
    m.insert(
        "op_cr_exponential_scale",
        def(
            |p_| {
                cr::ExponentialScale::create(
                    p(p_, "min", 0.0),
                    p(p_, "max", 1.0),
                    p(p_, "scale", 1.0),
                    p(p_, "offset", 0.0),
                )
            },
            &cr_in,
            OUT,
        ),
    );
    m.insert(
        "op_cr_frequency_to_phase",
        def(|_| cr::FrequencyToPhase::create(), &cr_in, OUT),
    );
    m.insert(
        "op_cr_frequency_to_samples",
        def(|_| cr::FrequencyToSamples::create(), &cr_in, OUT),
    );
    m.insert(
        "op_cr_time_to_samples",
        def(|_| cr::TimeToSamples::create(), &cr_in, OUT),
    );
    m.insert(
        "op_cr_magnitude_scale",
        def(|_| cr::MagnitudeScale::create(), &cr_in, OUT),
    );
    m.insert(
        "op_cr_midi_scale",
        def(|_| cr::MidiScale::create(), &cr_in, OUT),
    );
    m.insert("op_cr_add", def(|_| cr::Add::create(), &cr_ab, OUT));
    m.insert(
        "op_cr_multiply",
        def(|_| cr::Multiply::create(), &cr_ab, OUT),
    );
    m.insert(
        "op_cr_interpolate",
        def(
            |_| cr::Interpolate::create(),
            &[
                ("from", 0, Control),
                ("to", 1, Control),
                ("fractional", 2, Control),
            ],
            OUT,
        ),
    );
    m.insert(
        "op_cr_variable_add",
        def(
            |_| cr::VariableAdd::create(3),
            &[("a", 0, Control), ("b", 1, Control), ("c", 2, Control)],
            OUT,
        ),
    );
    m.insert(
        "op_cr_bilinear_interpolate",
        def(
            |_| cr::BilinearInterpolate::create(),
            &[
                ("top_left", 0, Control),
                ("top_right", 1, Control),
                ("bottom_left", 2, Control),
                ("bottom_right", 3, Control),
                ("x", 4, Control),
                ("y", 5, Control),
            ],
            OUT,
        ),
    );

    // values.cpp
    let set = [("set", 0, Trigger)];
    m.insert(
        "value",
        def(|p_| Value::create(p(p_, "value", 0.0), false), &set, OUT),
    );
    m.insert(
        "cr_value",
        def(
            |p_| Value::create_control_rate(p(p_, "value", 0.0)),
            &set,
            OUT,
        ),
    );
    m.insert(
        "smooth_value",
        def(
            |p_| {
                let value = SmoothValue::create(p(p_, "initial", 0.0));
                value.set_value(p(p_, "target", 1.0));
                value
            },
            &set,
            OUT,
        ),
    );
    m.insert(
        "cr_smooth_value",
        def(
            |p_| {
                let value = CrSmoothValue::create(p(p_, "initial", 0.0));
                value.set_value(p(p_, "target", 1.0));
                value
            },
            &set,
            OUT,
        ),
    );
    m.insert(
        "portamento_slope",
        def(
            |_| PortamentoSlope::create(),
            &[
                ("target", 0, Control),
                ("source", 1, Control),
                ("force", 2, Control),
                ("scale", 3, Control),
                ("run_seconds", 4, Control),
                ("slope_power", 5, Control),
                ("reset", 6, Trigger),
                ("num_notes_pressed", 7, Control),
            ],
            OUT,
        ),
    );
    m.insert(
        "peak_meter",
        def(
            |_| PeakMeter::create(),
            &a1,
            &[("level", 0), ("memory_peak", 1)],
        ),
    );

    // graph.cpp
    m.insert(
        "graph_modulation_sum",
        def(
            modulation_sum_graph,
            &[
                ("reset", 0, Trigger),
                ("control", 1, Control),
                ("audio", 2, Audio),
            ],
            OUT,
        ),
    );
    m.insert(
        "graph_feedback_cycle",
        def(feedback_cycle_graph, &cr_in, OUT),
    );
    m.insert(
        "graph_audio_chain",
        def(audio_chain_graph, &[("in", 0, Audio)], OUT),
    );

    // lookups.cpp
    let memory_inputs = [("audio", 0, Audio), ("delay", 1, Audio)];
    m.insert(
        "memory_probe",
        def(
            |p_| {
                let size = p(p_, "size", 1024.0) as i32;
                Processor::new(
                    MemoryProbe {
                        memory: MemoryKind::Mono(Memory::new(size)),
                    },
                    2,
                    1,
                    false,
                    1,
                )
            },
            &memory_inputs,
            OUT,
        ),
    );
    m.insert(
        "stereo_memory_probe",
        def(
            |p_| {
                let size = p(p_, "size", 1024.0) as i32;
                Processor::new(
                    MemoryProbe {
                        memory: MemoryKind::Stereo(StereoMemory::new(size)),
                    },
                    2,
                    1,
                    false,
                    1,
                )
            },
            &memory_inputs,
            OUT,
        ),
    );
    m.insert(
        "lookup_probe",
        def(
            |p_| {
                let lookup = OneDimLookup::new(f32::sin, 256, p(p_, "scale", 1.0));
                Processor::new(LookupProbe { lookup }, 1, 1, false, 1)
            },
            &[("in", 0, Audio)],
            OUT,
        ),
    );
    m.insert(
        "one_pole_probe",
        def(
            |p_| {
                let mode = p(p_, "mode", 0.0) as i64;
                let filter = if p(p_, "saturate", 0.0) != 0.0 {
                    OnePoleFilter::with_saturate(futils::tanh)
                } else {
                    OnePoleFilter::new()
                };
                Processor::new(OnePoleProbe { filter, mode }, 2, 1, false, 1)
            },
            &[("audio", 0, Audio), ("cutoff", 1, Audio)],
            OUT,
        ),
    );
    m
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

fn run_case(case: &ModuleCase, module: &ModuleDef) -> Vec<Vec<[f32; 4]>> {
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

fn framework_case_ids() -> Vec<String> {
    sas_parity::all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with("framework/"))
        .collect()
}

#[test]
fn every_framework_module_is_mirrored() {
    let table = modules();
    for id in framework_case_ids() {
        let case = ModuleCase::load(&id);
        assert!(
            table.contains_key(case.module()),
            "{id}: module {} has no Rust mirror",
            case.module()
        );
    }
}

#[test]
fn framework_modules_match_reference() {
    let table = modules();
    let ids = framework_case_ids();
    assert!(!ids.is_empty());
    let mut failures = Vec::new();
    let mut exact = 0;
    let mut compared = 0;
    let mut worst_abs = (0.0f64, String::new());
    let mut worst_residual = (f64::NEG_INFINITY, String::new());
    println!(
        "{:<52} {:>11} {:>11} {:>6}",
        "case:output", "max_abs", "residual", "exact"
    );

    for id in &ids {
        let case = ModuleCase::load(id);
        assert_capture_matches_semantics(id);
        let module = &table[case.module()];
        let outputs = run_case(&case, module);
        for (&(name, _), data) in module.outputs.iter().zip(&outputs) {
            compared += 1;
            let label = format!("{id}:{name}");
            let reference = case.reference(name);
            let identical = reference.len() == data.len()
                && reference
                    .iter()
                    .flatten()
                    .zip(data.iter().flatten())
                    .all(|(r, t)| r.to_bits() == t.to_bits() || (r.is_nan() && t.is_nan()));
            exact += identical as usize;
            let report = case.report(name, data);
            println!(
                "{:<52} {:>11.3e} {:>11.1} {:>6}",
                label, report.max_abs_error, report.residual_db, identical
            );
            if report.max_abs_error > worst_abs.0 {
                worst_abs = (report.max_abs_error, label.clone());
            }
            if report.residual_db > worst_residual.0 && report.residual_db.is_finite() {
                worst_residual = (report.residual_db, label.clone());
            }
            if !report.pass {
                failures.push(label);
            }
        }
    }

    println!(
        "{} outputs in {} cases, {} bit-identical; worst max_abs_error {:.3e} ({}), worst residual {:.1} dB ({})",
        compared,
        ids.len(),
        exact,
        worst_abs.0,
        worst_abs.1,
        worst_residual.0,
        worst_residual.1
    );

    if let Some(first) = failures.first() {
        eprintln!("failing outputs: {failures:?}");
        let (id, name) = first.split_once(':').unwrap();
        let case = ModuleCase::load(id);
        let module = &table[case.module()];
        let outputs = run_case(&case, module);
        let index = module.outputs.iter().position(|&(n, _)| n == name).unwrap();
        case.assert_parity(name, &outputs[index]);
    }
    let _ = PolyFloat::zero();
}
