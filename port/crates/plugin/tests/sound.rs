// SPDX-License-Identifier: GPL-3.0-or-later

//! The shipped plugin makes sound, and the same sound as the engine's offline
//! render (`sas_engine::synth::render`, which the reference parity tests
//! cover): a preset restored as host state and MIDI played through the
//! plugin's audio callback give bit-identical output.
//!
//! The engine's seed counter is process-wide, so everything here runs in one
//! test.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use nice_plug::prelude::NoteEvent;
use sas_engine::midi_file::read_midi_file;
use sas_engine::sas_oscillators::RandomValues;
use sas_engine::synth::{render, RenderSettings, Synth};
use sas_plugin::{state, SoundEngineHost, SynthASize, SynthPlugin, Transport};
use sas_preset::{InitContent, PresetState};

const SAMPLE_RATE: i32 = 48_000;
const BLOCK: usize = 128;
const SEED: i32 = 1;

fn corpus(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../migration/corpus")
        .join(path)
}

/// Interleaved stereo from the plugin's audio callback, in host blocks of
/// [`BLOCK`] samples, with `events` at their sample positions.
fn plugin_render(preset: &Path, events: &[(usize, [u8; 3])], frames: usize) -> Vec<f32> {
    sas_common::random::set_next_seed(SEED);
    RandomValues::reset_process_state();
    let mut plugin: SynthASize = SynthPlugin::new(SoundEngineHost::new());
    plugin
        .synth_params()
        .restore_state(state::plugin_state_from_file(preset).unwrap());
    // As a host opening a project: the state is restored, then the plugin is
    // activated.
    let params = plugin.synth_params().clone();
    plugin.processor_mut().activate(&params, SAMPLE_RATE as f32);
    let transport = Transport {
        tempo: Some(120.0),
        playing: false,
        pos_beats: None,
    };

    let mut out = Vec::with_capacity(frames * 2);
    let (mut left, mut right) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let mut next = 0;
    let mut start = 0;
    while start < frames {
        let n = BLOCK.min(frames - start);
        let mut block_events: VecDeque<NoteEvent<()>> = VecDeque::new();
        while next < events.len() && events[next].0 < start + n {
            let (sample, message) = events[next];
            let timing = sample.saturating_sub(start) as u32;
            block_events.push_back(NoteEvent::from_midi(timing, &message).unwrap());
            next += 1;
        }
        plugin.processor_mut().process(
            &params,
            &mut left[..n],
            &mut right[..n],
            &transport,
            || block_events.pop_front(),
        );
        for i in 0..n {
            out.push(left[i]);
            out.push(right[i]);
        }
        start += n;
    }
    out
}

#[test]
fn the_plugin_plays_the_engines_render() {
    let preset = corpus("presets/init_saw.vital");
    let midi =
        read_midi_file(&std::fs::read(corpus("midi/chord_progression.mid")).unwrap()).unwrap();
    let frames = SAMPLE_RATE as usize * 3;
    let events: Vec<(usize, [u8; 3])> = midi
        .iter()
        .map(|e| {
            let sample = (e.seconds * SAMPLE_RATE as f64 + 0.5).floor() as usize;
            (sample, e.message)
        })
        .filter(|(sample, _)| *sample < frames)
        .collect();
    assert!(events.len() >= 4, "the chords start inside the render");

    let from_plugin = plugin_render(&preset, &events, frames);

    sas_common::random::set_next_seed(SEED);
    RandomValues::reset_process_state();
    let mut synth = Synth::new();
    let mut loaded = PresetState::new(&InitContent { sample_seed: 0 });
    loaded.load_file(&preset).unwrap();
    synth.load_preset(&loaded);
    let settings = RenderSettings {
        sample_rate: SAMPLE_RATE,
        block_size: BLOCK,
        bpm: 120.0,
        frames,
    };
    let from_engine = render(&mut synth, &midi, &settings);

    let peak = from_plugin.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.1, "the plugin is silent (peak {peak})");
    assert!(from_plugin.iter().all(|s| s.is_finite()));
    assert_eq!(from_plugin.len(), from_engine.len());
    let first_difference = from_plugin
        .iter()
        .zip(&from_engine)
        .position(|(a, b)| a.to_bits() != b.to_bits());
    assert_eq!(
        first_difference, None,
        "the plugin and the offline render differ (peak {peak})"
    );
}
