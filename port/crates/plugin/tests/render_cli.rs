// SPDX-License-Identifier: GPL-3.0-or-later

//! `sas-render`'s render path against the reference's `ref-render` captures:
//! the same options give the same length and audio within each case's
//! tolerances, and the WAV it writes holds exactly the rendered samples.
//!
//! The engine's seed counter is process-wide, so the cases run one after
//! another in a single test (as in sas-engine's `tests/render.rs`).

use sas_engine::sas_oscillators::RandomValues;
use sas_parity::RenderCase;
use sas_plugin::render::{render_offline, wav_bytes, RenderOptions};

/// The render cases the engine covers so far (sas-engine `tests/render.rs`).
const CASES: [&str; 4] = [
    "render/init_saw__single_note",
    "render/init_saw__chords",
    "render/init_saw__single_note_48k_b128",
    "render/unison_pad_fx__chords",
];

fn options_for(case: &RenderCase) -> RenderOptions {
    let settings = case.settings();
    RenderOptions {
        preset: Some(case.preset_path()),
        midi: Some(case.midi_path()),
        sample_rate: case.sample_rate() as i32,
        block_size: case.block_size(),
        length: settings["length"].as_f64(),
        tail: settings["tail"].as_f64().unwrap_or(2.0),
        bpm: case.bpm(),
        seed: Some(case.seed().unwrap_or(0) as i32),
    }
}

/// The samples of a 32-bit float WAV written by [`wav_bytes`].
fn wav_samples(bytes: &[u8]) -> (u32, Vec<f32>) {
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 3, "IEEE float");
    assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2, "stereo");
    let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let data = bytes
        .windows(4)
        .position(|w| w == b"data")
        .expect("data chunk");
    let len = u32::from_le_bytes(bytes[data + 4..data + 8].try_into().unwrap()) as usize;
    let samples = bytes[data + 8..data + 8 + len]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect();
    (rate, samples)
}

#[test]
fn cli_renders_match_the_reference() {
    for id in CASES {
        let case = RenderCase::load(id);
        RandomValues::reset_process_state();
        let render = render_offline(&options_for(&case)).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(render.frames(), case.frames(), "{id}: length");
        assert_eq!(render.dropped_events, 0, "{id}");

        let (rate, samples) = wav_samples(&wav_bytes(&render));
        assert_eq!(rate, case.sample_rate(), "{id}");
        assert!(
            samples
                .iter()
                .zip(&render.samples)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
            "{id}: the WAV holds the rendered samples"
        );
        case.assert_parity(&samples[..]);
    }
}
