// SPDX-License-Identifier: GPL-3.0-or-later

//! Full-patch render parity: `render/*` cases in
//! `migration/corpus/cases.json`, rendered through [`Synth`] as `ref-render`
//! renders them (preset loaded, MIDI file read, block loop from t = 0).
//!
//! The engine RNG's seed counter is process-wide, and `ref-render` renders
//! each case in a fresh process: every case here sets the counter to the
//! case's seed and resets the once-per-process oscillator table before it
//! builds its synth, and the cases run one after another in one test.

use sas_engine::midi_file::read_midi_file;
use sas_engine::synth::{render, RenderSettings, Synth};
use sas_oscillators::RandomValues;
use sas_parity::RenderCase;
use sas_preset::{InitContent, PresetState};

/// Every `renders` case in `migration/corpus/cases.json`.
const CASES: [&str; 8] = [
    "render/init_saw__single_note",
    "render/init_saw__chords",
    "render/init_saw__single_note_48k_b128",
    "render/unison_pad_fx__chords",
    "render/analog_filter_lfo__chords",
    "render/analog_filter_lfo__velocity_96k_b512",
    "render/mono_legato_glide__legato",
    "render/line_wavetable_fold__velocity",
];

fn render_case(case: &RenderCase) -> Vec<f32> {
    sas_common::random::set_next_seed(case.seed().unwrap_or(0) as i32);
    RandomValues::reset_process_state();

    let mut synth = Synth::new();
    let mut preset = PresetState::new(&InitContent { sample_seed: 0 });
    preset
        .load_file(&case.preset_path())
        .unwrap_or_else(|e| panic!("{}: {e}", case.id()));
    synth.load_preset(&preset);

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

#[test]
fn renders_match_reference() {
    for id in CASES {
        let case = RenderCase::load(id);
        let output = render_case(&case);
        case.assert_parity(&output[..]);
    }
}
