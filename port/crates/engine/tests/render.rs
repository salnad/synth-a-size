// SPDX-License-Identifier: GPL-3.0-or-later

//! Full-patch render parity: `render/*` cases in
//! `migration/corpus/cases.json`, rendered through [`Synth`] as `ref-render`
//! renders them (preset loaded, MIDI file read, block loop from t = 0).
//!
//! The engine RNG's seed counter is process-wide, and `ref-render` renders
//! each case in a fresh process: every case here sets the counter to the
//! case's seed and resets the once-per-process oscillator table before it
//! builds its synth, and every render holds one lock while it runs.

use sas_engine::midi_file::read_midi_file;
use sas_engine::synth::{render, RenderSettings, Synth};
use sas_engine::Tuning;
use sas_oscillators::RandomValues;
use sas_parity::RenderCase;
use sas_preset::{InitContent, PresetState};

/// Every `renders` case in `migration/corpus/cases.json`.
const CASES: [&str; 16] = [
    "render/init_saw__single_note",
    "render/init_saw__chords",
    "render/init_saw__single_note_48k_b128",
    "render/unison_pad_fx__chords",
    "render/analog_filter_lfo__chords",
    "render/analog_filter_lfo__velocity_96k_b512",
    "render/mono_legato_glide__legato",
    "render/line_wavetable_fold__velocity",
    "render/velocity_filter_poly__overlap",
    "render/osc_stack_serial__arpeggio",
    "render/osc_formant_comb__chords",
    "render/osc_diode_phase__overlap",
    "render/fx_chain_reordered__chords_140bpm",
    "render/random_macro_shapes__pad_93bpm",
    "render/mpe_expression__expression",
    "render/tuned_glide__septimal_run",
];

/// The engine RNG's seed counter is process-wide, so the tests here build
/// and render their synths one at a time.
static ENGINE_RNG: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn render_case(case: &RenderCase) -> Vec<f32> {
    render_after(case, &[])
}

fn preset_file(path: &std::path::Path) -> PresetState {
    let mut preset = PresetState::new(&InitContent { sample_seed: 0 });
    preset
        .load_file(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    preset
}

/// Renders `case` on a synth that first loaded each of `earlier` (corpus
/// preset paths) and applied its modulation changes, as a block boundary
/// does, so loading the case's preset disconnects them.
fn render_after(case: &RenderCase, earlier: &[std::path::PathBuf]) -> Vec<f32> {
    let _engine_rng = ENGINE_RNG
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    sas_common::random::set_next_seed(case.seed().unwrap_or(0) as i32);
    RandomValues::reset_process_state();

    let mut synth = Synth::new();
    for path in earlier {
        synth.load_preset(&preset_file(path));
        synth.process_modulation_changes();
    }
    synth.load_preset(&preset_file(&case.preset_path()));
    load_tuning(&mut synth, case);

    let midi = std::fs::read(case.midi_path()).expect("MIDI file");
    let events = read_midi_file(&midi).unwrap_or_else(|e| panic!("{}: {e}", case.id()));
    let settings = RenderSettings {
        sample_rate: case.sample_rate() as i32,
        block_size: case.block_size(),
        bpm: case.bpm(),
        frames: case.frames(),
    };
    render(&mut synth, &events, &settings)
}

/// `loadTuningFile` for the case's tuning file, then its keyboard map, as
/// `ref-render` applies them after the preset.
fn load_tuning(synth: &mut Synth, case: &RenderCase) {
    let files: Vec<_> = [case.tuning_path(), case.keyboard_map_path()]
        .into_iter()
        .flatten()
        .collect();
    if files.is_empty() {
        return;
    }
    let mut tuning = Tuning::new();
    for path in files {
        tuning
            .load_file(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
    synth.set_tuning(&tuning);
}

#[test]
fn renders_match_reference() {
    for id in CASES {
        let case = RenderCase::load(id);
        let output = render_case(&case);
        case.assert_parity(&output[..]);
    }
}

fn corpus_preset(name: &str) -> std::path::PathBuf {
    let case = RenderCase::load("render/init_saw__single_note");
    case.preset_path()
        .parent()
        .expect("presets directory")
        .join(format!("{name}.vital"))
}

/// Preset changes exercise disconnection (the slots of the previous
/// preset's modulations): a render after loading another preset first must
/// match the case's own reference.
#[test]
fn renders_after_a_modulated_preset_match_reference() {
    let earlier = [
        "analog_filter_lfo",
        "mono_legato_glide",
        "line_wavetable_fold",
        "velocity_filter_poly",
    ];
    for id in [
        "render/init_saw__chords",
        "render/unison_pad_fx__chords",
        "render/analog_filter_lfo__chords",
    ] {
        let case = RenderCase::load(id);
        for name in earlier {
            let output = render_after(&case, &[corpus_preset(name)]);
            case.assert_parity(&output[..]);
        }
    }
}

/// A modulated preset, then the init saw, then the same preset again: the
/// second connection runs on a graph its first one was disconnected from.
#[test]
fn renders_after_a_reload_match_reference() {
    for id in [
        "render/analog_filter_lfo__chords",
        "render/analog_filter_lfo__velocity_96k_b512",
        "render/mono_legato_glide__legato",
        "render/line_wavetable_fold__velocity",
        "render/velocity_filter_poly__overlap",
    ] {
        let case = RenderCase::load(id);
        let earlier = [case.preset_path(), corpus_preset("init_saw")];
        let output = render_after(&case, &earlier);
        case.assert_parity(&output[..]);
    }
}
