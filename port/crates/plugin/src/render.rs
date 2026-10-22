// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Offline rendering, as the reference harness's `ref-render` does it
//! (`migration/reference/src/render_main.cpp`): seed the engine's random
//! counter, build the synth, load a preset (or the init state), read a MIDI
//! file, render `last event + tail` seconds through
//! [`sas_engine::synth::render`], and write a 32-bit float WAV. The
//! `sas-render` binary is the command line around [`render_to_wav`]. It
//! supports a subset of `ref-render`'s options: no `--warmup-samples`,
//! `--format` (always f32), `--meta` or `--dump-state`.

use std::path::{Path, PathBuf};

use sas_engine::midi_file::{read_midi_file, TimedMessage};
use sas_engine::sas_dsp_core::sas_simd::common::MAX_SAMPLE_RATE;
use sas_engine::synth::{render, RenderSettings, Synth};
use sas_preset::{InitContent, PresetState};

/// What to render (`ref-render`'s options and defaults).
#[derive(Clone, Debug, PartialEq)]
pub struct RenderOptions {
    /// A preset file, or `None` for the engine's init state (`--init`).
    pub preset: Option<PathBuf>,
    /// A MIDI file, or `None` for no MIDI input (`--no-midi`).
    pub midi: Option<PathBuf>,
    pub sample_rate: i32,
    /// Host block size (engine sub-blocks are at most 128 samples).
    pub block_size: usize,
    /// Total length in seconds; `None` for the last MIDI event plus `tail`.
    pub length: Option<f64>,
    pub tail: f64,
    pub bpm: f64,
    /// The engine's random seed counter before the synth is built; `None`
    /// keeps the process default.
    pub seed: Option<i32>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            preset: None,
            midi: None,
            sample_rate: 44_100,
            block_size: 64,
            length: None,
            tail: 2.0,
            bpm: 120.0,
            seed: None,
        }
    }
}

/// A finished render.
#[derive(Clone, Debug, PartialEq)]
pub struct Render {
    pub sample_rate: i32,
    /// Interleaved stereo.
    pub samples: Vec<f32>,
    /// Events at or after the end of the render (never played).
    pub dropped_events: usize,
}

impl Render {
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    /// The largest absolute sample value.
    pub fn peak(&self) -> f32 {
        self.samples
            .iter()
            .fold(0.0f32, |peak, s| peak.max(s.abs()))
    }

    pub fn non_finite(&self) -> usize {
        self.samples.iter().filter(|s| !s.is_finite()).count()
    }
}

/// Renders `options` (see the module docs).
pub fn render_offline(options: &RenderOptions) -> Result<Render, String> {
    if options.sample_rate <= 0 || options.sample_rate > MAX_SAMPLE_RATE {
        return Err("--sample-rate out of range".to_string());
    }
    if options.block_size == 0 || options.block_size > 65536 {
        return Err("--block-size out of range".to_string());
    }

    if let Some(seed) = options.seed {
        sas_common::random::set_next_seed(seed);
    }
    let mut synth = Synth::new();
    let preset = match &options.preset {
        Some(path) => {
            let mut preset = PresetState::new(&InitContent { sample_seed: 0 });
            preset
                .load_file(path)
                .map_err(|error| format!("failed to load preset {}: {error}", path.display()))?;
            preset
        }
        // SynthBase::loadInitPreset: the white-noise sample takes the next seed.
        None => PresetState::new(&InitContent {
            sample_seed: sas_common::random::take_next_seed(),
        }),
    };
    synth.load_preset(&preset);

    let events: Vec<TimedMessage> = match &options.midi {
        Some(path) => {
            let data = std::fs::read(path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            read_midi_file(&data).map_err(|error| format!("{}: {error}", path.display()))?
        }
        None => Vec::new(),
    };
    let last_event_seconds = events
        .iter()
        .map(|event| event.seconds)
        .fold(0.0f64, f64::max);
    let length_seconds = options.length.unwrap_or(last_event_seconds + options.tail);
    if length_seconds.is_nan() || length_seconds <= 0.0 {
        return Err("render length must be positive".to_string());
    }
    let frames = (length_seconds * options.sample_rate as f64).ceil() as usize;

    let settings = RenderSettings {
        sample_rate: options.sample_rate,
        block_size: options.block_size,
        bpm: options.bpm,
        frames,
    };
    let samples = render(&mut synth, &events, &settings);
    let dropped_events = events
        .iter()
        .filter(|event| {
            (event.seconds * options.sample_rate as f64 + 0.5).floor() as i64 >= frames as i64
        })
        .count();
    Ok(Render {
        sample_rate: options.sample_rate,
        samples,
        dropped_events,
    })
}

/// A 32-bit float stereo WAV (`WAVE_FORMAT_IEEE_FLOAT`) of `render`.
pub fn wav_bytes(render: &Render) -> Vec<u8> {
    const CHANNELS: u16 = 2;
    const BITS: u16 = 32;
    let data_len = (render.samples.len() * 4) as u32;
    let byte_rate = render.sample_rate as u32 * u32::from(CHANNELS) * u32::from(BITS / 8);
    let mut out = Vec::with_capacity(58 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(50 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&18u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&(render.sample_rate as u32).to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&(CHANNELS * BITS / 8).to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&(render.frames() as u32).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in &render.samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Renders `options` and writes the WAV to `out`.
pub fn render_to_wav(options: &RenderOptions, out: &Path) -> Result<Render, String> {
    let render = render_offline(options)?;
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    std::fs::write(out, wav_bytes(&render))
        .map_err(|error| format!("cannot write {}: {error}", out.display()))?;
    Ok(render)
}
