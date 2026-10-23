//! The preview engine and the process's seed counter. This binary has one
//! test, so nothing else takes or sets seeds while it runs.

use sas_common::json;
use sas_common::random::take_next_seed;
use sas_llm_control::{AuditionRequest, Auditioner};
use sas_llm_listen::PreviewEngine;
use sas_preset::{InitContent, PresetState};
use sas_wavetable::creator::WavetableCreator;

fn request(notes: &[u8]) -> AuditionRequest {
    AuditionRequest {
        notes: notes.to_vec(),
        hold: 0.3,
        release: 0.3,
        velocity: 0.8,
    }
}

/// The init patch with oscillator 1 playing an Audio File Source that has
/// no `random_seed`, so building it from JSON draws one from the counter.
fn file_source_patch() -> PresetState {
    let audio: Vec<i16> = (0..3000).map(|i| ((i * 37) % 2000 - 1000) as i16).collect();
    let text = format!(
        r#"{{"name": "File", "version": "1.0.6", "groups": [{{"components": [
            {{"type": "Audio File Source", "normalize_gain": true, "window_size": 600.5,
              "fade_style": 0, "phase_style": 0, "audio_sample_rate": 44100,
              "audio_file": "{audio}",
              "keyframes": [{{"position": 0, "start_position": 0, "window_fade": 1.0, "window_size": 600.5}}]}}
        ]}}]}}"#,
        audio = sas_common::base64::encode_i16s(&audio),
    );
    let mut state = PresetState::new(&InitContent { sample_seed: 0 });
    state.wavetables[0] = WavetableCreator::from_json(&json::parse(&text).unwrap()).unwrap();
    state
}

#[test]
fn auditions_leave_the_process_seed_counter_alone() {
    let patch = file_source_patch();
    let init = PresetState::new(&InitContent { sample_seed: 0 });
    let mut engine = PreviewEngine::new();

    let before = take_next_seed();
    engine.audition(&init, &request(&[60])).unwrap();
    engine.audition(&patch, &request(&[48, 60])).unwrap();
    engine.audition(&patch, &request(&[72])).unwrap();
    // Building three synths and loading a file source drew no global seeds.
    assert_eq!(take_next_seed(), before + 1);
}
