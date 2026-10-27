// SPDX-License-Identifier: GPL-3.0-or-later

//! A host that builds several engines in one process (the editor gallery)
//! must start each as a fresh process does, because the reference renders
//! each capture in its own process: the seed counter set, and
//! `RandomValues`' once-per-process seed taken again. One test in its own
//! binary, so no other test touches that process state meanwhile.

use std::path::Path;

use sas_engine::midi_file::read_midi_file;
use sas_engine::synth::{render, RenderSettings, Synth};
use sas_engine::telemetry::{status_names, Telemetry};
use sas_oscillators::RandomValues;
use sas_preset::{InitContent, PresetState};

/// The gallery's `modulations_live` view: `modulations.vital` playing one
/// second of the chord progression.
fn live_telemetry() -> (Vec<[f32; 4]>, Vec<[f32; 2]>) {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migration/corpus");
    let mut state = PresetState::new(&InitContent { sample_seed: 0 });
    state
        .load_file(&corpus.join("gui/presets/modulations.vital"))
        .unwrap();
    let midi = std::fs::read(corpus.join("midi/chord_progression.mid")).unwrap();
    let events = read_midi_file(&midi).unwrap();

    let mut synth = Synth::new();
    synth.load_preset(&state);
    let settings = RenderSettings {
        sample_rate: 44100,
        block_size: 64,
        bpm: 120.0,
        frames: 44100,
    };
    render(&mut synth, &events, &settings);
    let (mut writer, telemetry) =
        Telemetry::with_equalizer_memory(synth.audio_memory(), synth.equalizer_memory());
    synth.write_telemetry(&mut writer);
    let mut result = (Vec::new(), Vec::new());
    telemetry.read(|snapshot| {
        result.0 = status_names()
            .iter()
            .map(|name| snapshot.status_by_name(name).unwrap())
            .collect();
        result.1 = snapshot.oscilloscope().to_vec();
    });
    result
}

fn random_1(telemetry: &(Vec<[f32; 4]>, Vec<[f32; 2]>)) -> f32 {
    let index = status_names()
        .iter()
        .position(|n| *n == "random_1")
        .unwrap();
    telemetry.0[index][0]
}

#[test]
fn an_engine_after_a_fresh_process_reset_renders_like_the_first() {
    sas_common::random::set_next_seed(0);
    RandomValues::reset_process_state();
    let first = live_telemetry();
    // What ref-gui's engine holds after the same render.
    assert!(
        (random_1(&first) - 0.185_384_36).abs() < 1e-6,
        "{}",
        random_1(&first)
    );

    sas_common::random::set_next_seed(0);
    let seed_only = live_telemetry();
    assert_ne!(random_1(&seed_only), random_1(&first));

    sas_common::random::set_next_seed(0);
    RandomValues::reset_process_state();
    assert!(live_telemetry() == first);
}
