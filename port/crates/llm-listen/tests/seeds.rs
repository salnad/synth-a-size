//! The preview engine and the process's seed counter. This binary has one
//! test, so nothing else takes or sets seeds while it runs.

use sas_common::random::take_next_seed;
use sas_llm_control::{AuditionRequest, Auditioner};
use sas_llm_listen::PreviewEngine;
use sas_preset::{InitContent, PresetState};

fn init() -> PresetState {
    PresetState::new(&InitContent { sample_seed: 0 })
}

fn request(notes: &[u8]) -> AuditionRequest {
    AuditionRequest {
        notes: notes.to_vec(),
        hold: 0.3,
        release: 0.3,
        velocity: 0.8,
    }
}

#[test]
fn the_preview_never_sets_the_seed_counter_and_builds_its_synth_once() {
    let mut engine = PreviewEngine::new();
    let before = take_next_seed();
    engine.audition(&init(), &request(&[60])).unwrap();
    let built = take_next_seed();
    // Building the synth took seeds, as another instance would.
    assert!(built > before + 1, "{before} -> {built}");
    engine.audition(&init(), &request(&[64])).unwrap();
    // A reused synth takes none, and nothing moved the counter back.
    assert_eq!(take_next_seed(), built + 1);
}
