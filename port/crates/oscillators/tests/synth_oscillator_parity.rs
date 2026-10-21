// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `SynthOscillator` against
//! every `migration/corpus/modules/oscillators/*.json` case of the
//! `synth_oscillator` module.
//!
//! Mirrors `migration/reference/src/modules/oscillators/synth_oscillator.cpp`:
//! the wavetable is rendered from `params.wavetable`, then the oscillator is
//! constructed (after the seed counter is set, as `ref-module` does), and the
//! extra `modulation` input is what the FM/RM distortion types read. Cases
//! of the `synth_oscillator_pitch` module capture the block's phase
//! increments times `2^-32` (frequency / sample rate) instead of the audio.
//! Every case runs in one test because the seed counter is process-global.
//! Run with `--nocapture` for per-case metrics.

mod support;

use std::rc::Rc;
use std::{cell::RefCell, sync::Mutex};

use sas_common::json;
use sas_dsp_core::testing::{run_module, DriverInput, DriverSettings, InputRate, TriggerEvent};
use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_oscillators::synth_oscillator::{self as osc, RandomValues, SynthOscillator};
use sas_parity::{assert_parity, ModuleCase};
use sas_simd::PolyMask;
use sas_wavetable::creator::WavetableCreator;
use sas_wavetable::{Wavetable, NUM_OSCILLATOR_WAVE_FRAMES};
use serde_json::Value as Json;

use InputRate::{Audio, Control, Trigger};

const MODULATION: usize = osc::NUM_INPUTS;

const INPUTS: &[(&str, usize, InputRate)] = &[
    ("wave_frame", osc::WAVE_FRAME, Control),
    ("midi_note", osc::MIDI_NOTE, Control),
    ("midi_track", osc::MIDI_TRACK, Control),
    ("smooth_interpolate", osc::SMOOTHLY_INTERPOLATE, Control),
    ("transpose", osc::TRANSPOSE, Audio),
    ("transpose_quantize", osc::TRANSPOSE_QUANTIZE, Control),
    ("tune", osc::TUNE, Audio),
    ("amplitude", osc::AMPLITUDE, Audio),
    ("pan", osc::PAN, Control),
    ("unison_voices", osc::UNISON_VOICES, Control),
    ("unison_detune", osc::UNISON_DETUNE, Control),
    ("phase", osc::PHASE, Audio),
    ("distortion_phase", osc::DISTORTION_PHASE, Control),
    ("random_phase", osc::RANDOM_PHASE, Control),
    ("blend", osc::BLEND, Control),
    ("stereo_spread", osc::STEREO_SPREAD, Control),
    ("stack_style", osc::STACK_STYLE, Control),
    ("detune_power", osc::DETUNE_POWER, Control),
    ("detune_range", osc::DETUNE_RANGE, Control),
    ("frame_spread", osc::UNISON_FRAME_SPREAD, Control),
    ("distortion_spread", osc::UNISON_DISTORTION_SPREAD, Control),
    (
        "spectral_morph_spread",
        osc::UNISON_SPECTRAL_MORPH_SPREAD,
        Control,
    ),
    ("spectral_morph_type", osc::SPECTRAL_MORPH_TYPE, Control),
    ("spectral_morph_amount", osc::SPECTRAL_MORPH_AMOUNT, Control),
    ("spectral_unison", osc::SPECTRAL_UNISON, Control),
    ("distortion_type", osc::DISTORTION_TYPE, Control),
    ("distortion_amount", osc::DISTORTION_AMOUNT, Control),
    ("active_voices", osc::ACTIVE_VOICES, Control),
    ("reset", osc::RESET, Trigger),
    ("retrigger", osc::RETRIGGER, Trigger),
    ("modulation", MODULATION, Audio),
];

const PHASE_INC: usize = osc::NUM_OUTPUTS;

const OUTPUTS: &[(&str, usize)] = &[("raw", osc::RAW), ("levelled", osc::LEVELLED)];
const PITCH_OUTPUTS: &[(&str, usize)] = &[("phase_inc", PHASE_INC)];

fn outputs(case: &ModuleCase) -> &'static [(&'static str, usize)] {
    match case.module() {
        "synth_oscillator_pitch" => PITCH_OUTPUTS,
        _ => OUTPUTS,
    }
}

/// `OscillatorProbe` / `OscillatorPitchProbe`: the oscillator plus the
/// `modulation` input wired as its FM/RM sources, and output `phase_inc`
/// (written in every case; read for `synth_oscillator_pitch`).
#[derive(Clone)]
struct OscillatorProbe {
    oscillator: SynthOscillator,
}

impl ProcessorImpl for OscillatorProbe {
    sas_dsp_core::processor_boilerplate!();

    fn reset(&mut self, this: &Processor, reset_mask: PolyMask) {
        self.oscillator.reset(this, reset_mask);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        self.oscillator.set_oversample_amount(this, oversample);
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let modulation = this.input(MODULATION).source();
        self.oscillator
            .set_first_oscillator_output(modulation.clone());
        self.oscillator
            .set_second_oscillator_output(modulation.clone());
        self.oscillator.set_sample_output(modulation);
        self.oscillator.process(this, num_samples);
        let phase_inc = this.output_buffer(PHASE_INC);
        for (dest, &increment) in phase_inc
            .iter()
            .zip(self.oscillator.phase_increments())
            .take(num_samples)
        {
            dest.set(increment * (1.0 / 4_294_967_296.0));
        }
    }
}

fn render_wavetable(params: &Json) -> Wavetable {
    let spec = &params["wavetable"];
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
    let mut wavetable = Wavetable::new(NUM_OSCILLATOR_WAVE_FRAMES as usize);
    creator.render(&mut wavetable).expect("renders");
    wavetable
}

fn create(case: &ModuleCase) -> ProcessorRef {
    let seed = case.spec()["seed"].as_i64().expect("cases set a seed") as i32;
    sas_common::random::set_next_seed(seed);
    RandomValues::reset_process_state();
    let wavetable = Rc::new(RefCell::new(render_wavetable(case.params())));
    let oscillator = SynthOscillator::new(wavetable);
    Processor::new(
        OscillatorProbe { oscillator },
        osc::NUM_INPUTS + 1,
        osc::NUM_OUTPUTS + 1,
        false,
        1,
    )
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
    let indices: Vec<usize> = outputs(case).iter().map(|&(_, index)| index).collect();
    run_module(&processor, &inputs, &settings, &indices)
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
fn synth_oscillator_matches_reference() {
    let _guard = SEED_COUNTER.lock().unwrap();
    let ids: Vec<String> = sas_parity::all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with("oscillators/"))
        .filter(|id| {
            matches!(
                ModuleCase::load(id).module(),
                "synth_oscillator" | "synth_oscillator_pitch"
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
        let results = run_case(&case);
        for (&(name, _), data) in outputs(&case).iter().zip(&results) {
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
        checked.push((id.clone(), results));
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
    for (id, results) in &checked {
        let case = ModuleCase::load(id);
        for (&(name, _), data) in outputs(&case).iter().zip(results) {
            assert_parity!(&format!("{id}:{name}"), data);
        }
    }
}
