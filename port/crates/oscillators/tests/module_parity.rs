// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `OscillatorModule` and `ProducersModule` against every
//! `migration/corpus/modules/oscillators/*.json` case of the
//! `oscillator_module` and `producers_module` modules.
//!
//! Mirrors `migration/reference/src/modules/oscillators/{oscillator_module,
//! producers_module,module_content}.{cpp,h}`: the module is constructed after
//! the seed counter is set, initialized, then given its wavetables, sample
//! and control values. Controls take their details from the parameter
//! registry (`sas-preset`), as the reference's `Parameters::getDetails`.
//! `oscillator_module_pitch` cases capture the module's oscillator's phase
//! increments times `2^-32` (frequency / sample rate) instead of the audio.
//! `producers_module` cases can plug filter-on values and schedule control
//! changes at block starts, and also capture the sampler's phase output.
//! Every case runs in one test because the seed counter is process-global.

mod support;

use std::sync::Mutex;

use sas_common::json::{self, Value};
use sas_dsp_core::synth_module::{set_parameter_lookup, ParameterDetails, ValueScale};
use sas_dsp_core::testing::{run_module, DriverInput, DriverSettings, InputRate, TriggerEvent};
use sas_dsp_core::{
    Output, Processor, ProcessorImpl, ProcessorRef, SynthModule, Value as ValueProcessor,
};
use sas_oscillators::oscillator_module::{self, OscillatorModule};
use sas_oscillators::producers_module::{self, ProducersModule};
use sas_oscillators::{RandomValues, SharedSample, SharedWavetable, SynthOscillator};
use sas_parity::{assert_parity, ModuleCase};
use sas_preset::registry;
use sas_wavetable::creator::WavetableCreator;
use serde_json::Value as Json;

use InputRate::{Control, Trigger};

struct ModuleDef {
    inputs: &'static [(&'static str, usize, InputRate)],
    outputs: &'static [(&'static str, usize)],
}

const OSCILLATOR_MODULE: ModuleDef = ModuleDef {
    inputs: &[
        ("reset", oscillator_module::RESET, Trigger),
        ("retrigger", oscillator_module::RETRIGGER, Trigger),
        ("midi", oscillator_module::MIDI, Control),
        ("active_voices", oscillator_module::ACTIVE_VOICES, Control),
    ],
    outputs: &[
        ("raw", oscillator_module::RAW),
        ("levelled", oscillator_module::LEVELLED),
    ],
};

const OSCILLATOR_MODULE_PITCH: ModuleDef = ModuleDef {
    inputs: OSCILLATOR_MODULE.inputs,
    outputs: &[("phase_inc", PHASE_INC)],
};

const PHASE_INC: usize = oscillator_module::NUM_OUTPUTS;

/// `OscillatorModulePitchProbe`: the module, with its oscillator's phase
/// increments copied to output `phase_inc` after each block.
#[derive(Clone)]
struct OscillatorModulePitchProbe {
    module: OscillatorModule,
}

impl ProcessorImpl for OscillatorModulePitchProbe {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        self.module.init(this);
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        self.module.process(this, num_samples);
        let oscillator = self.module.oscillator().expect("initialized").clone();
        let phase_inc = this.output_buffer(PHASE_INC);
        oscillator.with::<SynthOscillator, _>(|oscillator, _| {
            for (dest, &increment) in phase_inc
                .iter()
                .zip(oscillator.phase_increments())
                .take(num_samples)
            {
                dest.set(increment * (1.0 / 4_294_967_296.0));
            }
        });
    }
}

const PRODUCERS_MODULE: ModuleDef = ModuleDef {
    inputs: &[
        ("reset", producers_module::RESET, Trigger),
        ("retrigger", producers_module::RETRIGGER, Trigger),
        ("midi", producers_module::MIDI, Control),
        ("active_voices", producers_module::ACTIVE_VOICES, Control),
        ("note_count", producers_module::NOTE_COUNT, Control),
    ],
    outputs: &[
        ("to_filter1", producers_module::TO_FILTER1),
        ("to_filter2", producers_module::TO_FILTER2),
        ("raw_out", producers_module::RAW_OUT),
        ("direct_out", producers_module::DIRECT_OUT),
        ("sample_phase", SAMPLE_PHASE),
    ],
};

const SAMPLE_PHASE: usize = producers_module::NUM_OUTPUTS;

#[derive(Clone)]
struct ControlChange {
    frame: usize,
    target: ProcessorRef,
    value: f32,
}

/// `ProducersProbe`: the module with optional filter-on values, control
/// changes applied before the first block starting at or after their frame,
/// and the sampler's phase output copied to output `sample_phase` after each
/// block.
#[derive(Clone)]
struct ProducersProbe {
    module: ProducersModule,
    filter_on: [Option<ProcessorRef>; 2],
    changes: Vec<ControlChange>,
    next_change: usize,
    frame: usize,
}

impl ProducersProbe {
    fn configure(&mut self, this: &Processor, params: &Json) {
        let mut targets = this.get_controls();
        for (i, name) in ["filter1_on", "filter2_on"].into_iter().enumerate() {
            if let Some(value) = params.get(name) {
                let on = ValueProcessor::create(value.as_f64().expect("filter on") as f32, false);
                targets.insert(name.to_string(), on.clone());
                self.filter_on[i] = Some(on);
            }
        }
        self.module.set_filter1_on(self.filter_on[0].clone());
        self.module.set_filter2_on(self.filter_on[1].clone());

        let changes = params.get("control_changes").and_then(Json::as_array);
        for change in changes.into_iter().flatten() {
            let name = change["name"].as_str().expect("change name");
            self.changes.push(ControlChange {
                frame: change["sample"].as_u64().expect("change sample") as usize,
                target: targets
                    .get(name)
                    .unwrap_or_else(|| panic!("no control {name}"))
                    .clone(),
                value: change["value"].as_f64().expect("change value") as f32,
            });
        }
        self.changes.sort_by_key(|change| change.frame);
    }
}

impl ProcessorImpl for ProducersProbe {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        self.module.init(this);
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        while let Some(change) = self.changes.get(self.next_change) {
            if change.frame > self.frame {
                break;
            }
            change.target.set_value(change.value);
            self.next_change += 1;
        }
        self.module.process(this, num_samples);
        this.output(SAMPLE_PHASE)
            .set_at(0, self.module.sample_phase_output().at(0));
        self.frame += num_samples;
    }
}

fn install_registry_lookup() {
    set_parameter_lookup(|name| {
        let info = registry::lookup(name).unwrap_or_else(|| panic!("no parameter {name}"));
        let value_scale = match info.value_scale {
            registry::ValueScale::Indexed => ValueScale::Indexed,
            registry::ValueScale::Linear => ValueScale::Linear,
            registry::ValueScale::Quadratic => ValueScale::Quadratic,
            registry::ValueScale::Cubic => ValueScale::Cubic,
            registry::ValueScale::Quartic => ValueScale::Quartic,
            registry::ValueScale::SquareRoot => ValueScale::SquareRoot,
            registry::ValueScale::Exponential => ValueScale::Exponential,
        };
        ParameterDetails {
            min: info.min,
            max: info.max,
            default_value: info.default_value,
            post_offset: info.post_offset,
            value_scale,
        }
    });
}

/// `module_content::renderWavetable`.
fn render_wavetable(wavetable: &SharedWavetable, spec: &Json) {
    let mut creator = WavetableCreator::new();
    if let Some(text) = spec["creator"].as_str() {
        let input = support::expand(&json::parse(text).expect("creator JSON"));
        creator.read_json(&input).expect("loads");
    } else {
        match spec["preset"].as_str().unwrap_or("init") {
            "init" => creator.init(),
            "predefined" => creator.init_predefined_waves(),
            other => panic!("unknown preset {other:?}"),
        }
    }
    creator
        .render(&mut wavetable.borrow_mut())
        .expect("renders");
}

fn recipe_json(spec: &Json, length: i64) -> Value {
    let mut recipe = json::parse(&spec.to_string()).expect("recipe JSON");
    recipe.set("length", length);
    recipe.set("encoding", "pcm");
    recipe
}

/// `module_content::loadSample`.
fn load_sample(sample: &SharedSample, spec: &Json) {
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
    sample
        .borrow_mut()
        .json_to_state(&data)
        .expect("sample loads");
}

/// `module_content::setControls`.
fn set_controls(module: &ProcessorRef, controls: &Json) {
    let map = module.get_controls();
    for (name, value) in controls.as_object().into_iter().flatten() {
        let control = map.get(name).unwrap_or_else(|| panic!("no control {name}"));
        control.set_value(value.as_f64().expect("control value") as f32);
    }
}

fn create(case: &ModuleCase) -> ProcessorRef {
    let seed = case.spec()["seed"].as_i64().expect("cases set a seed") as i32;
    sas_common::random::set_next_seed(seed);
    RandomValues::reset_process_state();
    let params = case.params();
    let empty = Json::Object(Default::default());
    match case.module() {
        "oscillator_module" => {
            let module = OscillatorModule::create("osc_1");
            module.init();
            let wavetable = module.with::<OscillatorModule, _>(|m, _| m.wavetable().clone());
            render_wavetable(&wavetable, params.get("wavetable").unwrap_or(&empty));
            set_controls(&module, params.get("controls").unwrap_or(&empty));
            module
        }
        "oscillator_module_pitch" => {
            let module = SynthModule::create_with(
                OscillatorModulePitchProbe {
                    module: OscillatorModule::new("osc_1"),
                },
                oscillator_module::NUM_INPUTS,
                oscillator_module::NUM_OUTPUTS + 1,
                false,
            );
            module.init();
            let wavetable =
                module.with::<OscillatorModulePitchProbe, _>(|m, _| m.module.wavetable().clone());
            render_wavetable(&wavetable, params.get("wavetable").unwrap_or(&empty));
            set_controls(&module, params.get("controls").unwrap_or(&empty));
            module
        }
        "producers_module" => {
            let kernel = ProducersModule::new();
            let submodules = kernel.submodules();
            let probe = ProducersProbe {
                module: kernel,
                filter_on: [None, None],
                changes: Vec::new(),
                next_change: 0,
                frame: 0,
            };
            let module = SynthModule::create_with(
                probe,
                producers_module::NUM_INPUTS,
                producers_module::NUM_OUTPUTS,
                false,
            );
            module.register_output(Output::new_control_rate());
            ProducersModule::add_submodules(&module, &submodules);
            module.init();
            let wavetables = params.get("wavetables").unwrap_or(&empty);
            for i in 0..producers_module::NUM_OSCILLATORS {
                if let Some(spec) = wavetables.get(format!("osc_{}", i + 1)) {
                    let wavetable =
                        module.with::<ProducersProbe, _>(|probe, _| probe.module.wavetable(i));
                    render_wavetable(&wavetable, spec);
                }
            }
            if let Some(spec) = params.get("sample") {
                let sample = module.with::<ProducersProbe, _>(|probe, _| probe.module.sample());
                load_sample(&sample, spec);
            }
            set_controls(&module, params.get("controls").unwrap_or(&empty));
            module.with::<ProducersProbe, _>(|probe, this| probe.configure(this, params));
            module
        }
        other => panic!("not a module case: {other}"),
    }
}

fn definition(case: &ModuleCase) -> &'static ModuleDef {
    match case.module() {
        "oscillator_module" => &OSCILLATOR_MODULE,
        "oscillator_module_pitch" => &OSCILLATOR_MODULE_PITCH,
        _ => &PRODUCERS_MODULE,
    }
}

fn input_rate(case: &ModuleCase, name: &str, registry: InputRate) -> InputRate {
    let spec = &case.spec()["inputs"][name];
    if spec.get("trigger").is_some() {
        return Trigger;
    }
    match spec.get("rate").and_then(Json::as_str) {
        Some("audio") => InputRate::Audio,
        Some("control") => Control,
        _ => registry,
    }
}

fn run_case(case: &ModuleCase) -> Vec<Vec<[f32; 4]>> {
    let def = definition(case);
    let processor = create(case);
    let inputs: Vec<DriverInput> = def
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
    let outputs: Vec<usize> = def.outputs.iter().map(|&(_, index)| index).collect();
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
fn oscillator_modules_match_reference() {
    let _guard = SEED_COUNTER.lock().unwrap();
    install_registry_lookup();
    let ids: Vec<String> = sas_parity::all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with("oscillators/"))
        .filter(|id| {
            matches!(
                ModuleCase::load(id).module(),
                "oscillator_module" | "oscillator_module_pitch" | "producers_module"
            )
        })
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
        for (&(name, _), data) in definition(&case).outputs.iter().zip(&outputs) {
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
        let case = ModuleCase::load(id);
        for (&(name, _), data) in definition(&case).outputs.iter().zip(outputs) {
            assert_parity!(&format!("{id}:{name}"), data);
        }
    }
}
