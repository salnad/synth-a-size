// SPDX-License-Identifier: GPL-3.0-or-later

//! Offline render benchmark for the engine, the port's side of the
//! reference harness's `ref-bench` (`migration/reference/src/bench_main.cpp`).
//!
//! Each repeat builds a fresh [`Synth`] (not timed), loads the preset as
//! `sas-render` does, and renders `length` seconds through
//! [`Synth::render_block`], the block loop of `ref-render`. Only the block
//! loop is timed: the wall-clock time of each `render_block` call, and the
//! whole loop's wall-clock and thread CPU time. `migration/scripts/bench.py`
//! runs both binaries and compares their JSON; see `migration/BENCHMARKS.md`.

use std::path::PathBuf;
use std::time::Instant;

use sas_engine::midi_file::read_midi_file;
use sas_engine::synth::Synth;
use sas_preset::{InitContent, PresetState};
use serde_json::{json, Value};

/// Milestone in `migration/PLAN.md` this crate serves (performance).
pub const MILESTONE: &str = "M11";

/// Upstream files this crate's measurements cover; nothing is ported here.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/common/synth_base.cpp",
    "vital/src/synthesis/synth_engine/sound_engine.cpp",
];

/// What to time (`ref-bench`'s options and defaults).
#[derive(Clone, Debug, PartialEq)]
pub struct BenchOptions {
    /// A preset file, or `None` for the engine's init state (`--init`).
    pub preset: Option<PathBuf>,
    /// A MIDI file, or `None` for no MIDI input (`--no-midi`).
    pub midi: Option<PathBuf>,
    pub sample_rate: i32,
    pub block_size: usize,
    /// Render length in seconds.
    pub length: f64,
    pub bpm: f64,
    /// The engine's random seed counter before each synth is built.
    pub seed: i32,
    /// Untimed renders before the timed ones.
    pub warmup: usize,
    /// Timed renders.
    pub repeats: usize,
}

impl Default for BenchOptions {
    fn default() -> Self {
        BenchOptions {
            preset: None,
            midi: None,
            sample_rate: 44_100,
            block_size: 64,
            length: 0.0,
            bpm: 120.0,
            seed: 1,
            warmup: 1,
            repeats: 3,
        }
    }
}

/// One timed render.
#[derive(Clone, Debug)]
pub struct Run {
    pub wall_seconds: f64,
    pub cpu_seconds: f64,
    /// Wall-clock time of each `render_block` call.
    pub block_seconds: Vec<f64>,
    /// Interleaved stereo output.
    pub interleaved: Vec<f32>,
}

/// The timed renders of one [`BenchOptions`].
#[derive(Clone, Debug)]
pub struct Bench {
    pub options: BenchOptions,
    pub frames: usize,
    pub runs: Vec<Run>,
}

/// The calling thread's CPU time in seconds.
fn thread_cpu_seconds() -> f64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is a valid timespec for clock_gettime to write.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) };
    now.tv_sec as f64 + now.tv_nsec as f64 * 1e-9
}

/// Nearest-rank percentile, as `ref-bench` computes it.
pub fn percentile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = (fraction * sorted.len() as f64).ceil() as usize;
    sorted[index.clamp(1, sorted.len()) - 1]
}

/// The harness's `samples_fnv1a64` (`harness_io.cpp`): FNV-1a 64 with its
/// offset basis 1469598103934665603, not the standard one.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(1_469_598_103_934_665_603, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn load_synth(options: &BenchOptions) -> Result<Synth, String> {
    sas_common::random::set_next_seed(options.seed);
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
    synth.prepare(options.sample_rate, options.bpm);
    Ok(synth)
}

fn render_once(
    options: &BenchOptions,
    events: &[(i64, [u8; 3])],
    frames: usize,
) -> Result<Run, String> {
    let mut synth = load_synth(options)?;
    let block_size = options.block_size;
    let mut interleaved = vec![0.0f32; frames * 2];
    let mut block_seconds = Vec::with_capacity(frames / block_size + 1);
    let mut left = vec![0.0f32; block_size];
    let mut right = vec![0.0f32; block_size];
    let mut midi = Vec::new();
    let mut seconds_time = 0.0;
    let mut event_index = 0;

    let cpu_start = thread_cpu_seconds();
    let start = Instant::now();
    let mut block_start = 0usize;
    while block_start < frames {
        let n = block_size.min(frames - block_start);
        left[..n].fill(0.0);
        right[..n].fill(0.0);

        midi.clear();
        while event_index < events.len() && events[event_index].0 < (block_start + n) as i64 {
            let offset = (events[event_index].0 - block_start as i64).max(0) as usize;
            midi.push((offset, events[event_index].1));
            event_index += 1;
        }

        let block_begin = Instant::now();
        synth.render_block(
            &mut left[..n],
            &mut right[..n],
            &midi,
            &mut seconds_time,
            options.sample_rate,
        );
        block_seconds.push(block_begin.elapsed().as_secs_f64());

        for i in 0..n {
            interleaved[2 * (block_start + i)] = left[i];
            interleaved[2 * (block_start + i) + 1] = right[i];
        }
        block_start += n;
    }
    Ok(Run {
        wall_seconds: start.elapsed().as_secs_f64(),
        cpu_seconds: thread_cpu_seconds() - cpu_start,
        block_seconds,
        interleaved,
    })
}

/// Runs `options.warmup` untimed and `options.repeats` timed renders.
pub fn run_bench(options: &BenchOptions) -> Result<Bench, String> {
    if options.sample_rate <= 0 {
        return Err("--sample-rate out of range".to_string());
    }
    if options.block_size == 0 || options.block_size > 65536 {
        return Err("--block-size out of range".to_string());
    }
    if options.length.is_nan() || options.length <= 0.0 {
        return Err("--length must be positive".to_string());
    }
    if options.repeats == 0 {
        return Err("--repeats must be >= 1".to_string());
    }
    let events: Vec<(i64, [u8; 3])> = match &options.midi {
        Some(path) => {
            let data = std::fs::read(path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            read_midi_file(&data)
                .map_err(|error| format!("{}: {error}", path.display()))?
                .iter()
                .map(|event| {
                    let sample = (event.seconds * options.sample_rate as f64 + 0.5).floor();
                    (sample as i64, event.message)
                })
                .collect()
        }
        None => Vec::new(),
    };
    let frames = (options.length * options.sample_rate as f64).ceil() as usize;
    let mut runs = Vec::with_capacity(options.repeats);
    for i in 0..options.warmup + options.repeats {
        let run = render_once(options, &events, frames)?;
        if i >= options.warmup {
            runs.push(run);
        }
    }
    Ok(Bench {
        options: options.clone(),
        frames,
        runs,
    })
}

impl Bench {
    /// The last timed render's output.
    pub fn output(&self) -> &[f32] {
        &self.runs.last().expect("at least one repeat").interleaved
    }

    pub fn non_finite(&self) -> usize {
        self.output().iter().filter(|s| !s.is_finite()).count()
    }

    /// The fields `ref-bench` prints (`migration/BENCHMARKS.md`).
    pub fn to_json(&self) -> Value {
        let options = &self.options;
        let output = self.output();
        let (mut sum_squares, mut peak) = (0.0f64, 0.0f32);
        for sample in output.iter().filter(|s| s.is_finite()) {
            sum_squares += f64::from(*sample) * f64::from(*sample);
            peak = peak.max(sample.abs());
        }
        let bytes: Vec<u8> = output.iter().flat_map(|s| s.to_le_bytes()).collect();
        let blocks: Vec<f64> = self
            .runs
            .iter()
            .flat_map(|run| run.block_seconds.iter().copied())
            .collect();
        let mean = blocks.iter().sum::<f64>() / blocks.len().max(1) as f64;
        let first = self
            .runs
            .iter()
            .map(|run| run.block_seconds.first().copied().unwrap_or(0.0))
            .sum::<f64>()
            / self.runs.len() as f64;
        let path = |p: &Option<PathBuf>, none: &str| {
            p.as_ref()
                .map_or(none.to_string(), |p| p.display().to_string())
        };
        json!({
            "tool": "sas-bench",
            "build": {
                "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
                "arch": std::env::consts::ARCH,
                "os": std::env::consts::OS,
                "aarch64_semantics": cfg!(feature = "aarch64-semantics"),
            },
            "preset": path(&options.preset, "<init>"),
            "midi": path(&options.midi, "<none>"),
            "sample_rate": options.sample_rate,
            "block_size": options.block_size,
            "frames": self.frames,
            "blocks": self.runs.last().map_or(0, |run| run.block_seconds.len()),
            "audio_seconds": self.frames as f64 / options.sample_rate as f64,
            "warmup": options.warmup,
            "repeats": self.runs.iter().map(|run| json!({
                "wall_seconds": run.wall_seconds,
                "cpu_seconds": run.cpu_seconds,
            })).collect::<Vec<_>>(),
            "block_us": {
                "mean": 1e6 * mean,
                "p50": 1e6 * percentile(&blocks, 0.50),
                "p99": 1e6 * percentile(&blocks, 0.99),
                "max": 1e6 * percentile(&blocks, 1.0),
                "first": 1e6 * first,
            },
            "output": {
                "rms": (sum_squares / output.len().max(1) as f64).sqrt(),
                "peak": peak,
                "non_finite": self.non_finite(),
                "samples_fnv1a64": format!("{:016x}", fnv1a64(&bytes)),
            },
        })
    }
}

/// A 32-bit float stereo WAV (`WAVE_FORMAT_IEEE_FLOAT`) of `interleaved`.
pub fn wav_bytes(interleaved: &[f32], sample_rate: i32) -> Vec<u8> {
    const CHANNELS: u16 = 2;
    const BITS: u16 = 32;
    let data_len = (interleaved.len() * 4) as u32;
    let byte_rate = sample_rate as u32 * u32::from(CHANNELS) * u32::from(BITS / 8);
    let mut out = Vec::with_capacity(58 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(50 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&18u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&(sample_rate as u32).to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&(CHANNELS * BITS / 8).to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&((interleaved.len() / 2) as u32).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in interleaved {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_sources_exist() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        for source in UPSTREAM_SOURCES {
            assert!(repo.join(source).exists(), "missing upstream path {source}");
        }
        assert!(MILESTONE.starts_with('M'));
    }

    #[test]
    fn percentile_is_nearest_rank() {
        let values = [4.0, 1.0, 3.0, 2.0];
        assert_eq!(percentile(&values, 0.5), 2.0);
        assert_eq!(percentile(&values, 0.99), 4.0);
        assert_eq!(percentile(&values, 1.0), 4.0);
        assert_eq!(percentile(&values, 0.0), 1.0);
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn fnv1a64_matches_the_harness() {
        assert_eq!(fnv1a64(b""), 1_469_598_103_934_665_603);
        // run_corpus.py's fnv1a64(b"a").
        assert_eq!(fnv1a64(b"a"), 0x44bd_8ad4_73cd_9906);
    }

    #[test]
    fn repeats_render_identically_and_time_every_block() {
        let midi = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../migration/corpus/midi/single_note_c3.mid");
        // The first synth built in a process renders differently from the
        // later ones (the reference's ref-bench does the same), so one
        // warmup makes every timed repeat identical.
        let options = BenchOptions {
            midi: Some(midi),
            block_size: 100,
            length: 0.05,
            warmup: 1,
            repeats: 2,
            ..BenchOptions::default()
        };
        let bench = run_bench(&options).unwrap();
        assert_eq!(bench.frames, 2205);
        assert_eq!(bench.runs.len(), 2);
        for run in &bench.runs {
            assert_eq!(run.block_seconds.len(), 23);
            assert!(run.wall_seconds > 0.0);
        }
        assert_eq!(bench.runs[0].interleaved, bench.runs[1].interleaved);
        let result = bench.to_json();
        assert!(result["output"]["peak"].as_f64().unwrap() > 0.0);
        assert_eq!(result["blocks"], 23);
        assert_eq!(result["repeats"].as_array().unwrap().len(), 2);
    }
}
