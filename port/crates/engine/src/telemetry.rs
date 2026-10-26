// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the editor shows of the playing engine: a snapshot written at the
//! end of each audio block and read by the editor without locks shared
//! with the audio thread.
//!
//! Upstream's interface reads the engine directly from the GUI thread:
//! every `StatusOutput` (LFO, envelope and random values and phases, the
//! sample phase, the voice count, peak meters, compressor bands, chorus
//! delays, phaser and flanger positions, each modulation slot's source and
//! amount), each destination's modulation totals (`getMonoModulations` /
//! `getPolyModulations`, read as `trigger_value` by `ModulationMeter` and
//! `Wavetable3d::getOutputsTotal`), and `SynthBase`'s oscilloscope and audio
//! memories (`updateMemoryOutput`, `vital/src/common/synth_base.cpp`). Here
//! the engine belongs to the audio thread, so:
//!
//! - a triple-buffered [`TelemetrySnapshot`]: the audio thread fills the
//!   back buffer and publishes it with one atomic swap; the editor takes the
//!   newest published one. Nothing allocates, frees or waits;
//! - an [`AudioMemory`] ring for the spectrogram, written sample by sample
//!   with relaxed atomics, too large to copy every block.
//!
//! [`status_names`] and [`destination_names`] are fixed tables, so the
//! editor resolves ids once ([`status_id`], [`destination_id`]) and reads by
//! index.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use sas_preset::constants::{
    MAX_MODULATION_CONNECTIONS, NUM_ENVELOPES, NUM_LFOS, NUM_MACROS, NUM_RANDOM_LFOS,
};

/// `kOscilloscopeMemoryResolution`.
pub const OSCILLOSCOPE_MEMORY_RESOLUTION: usize = 512;
/// `kOscilloscopeMemorySampleRate`.
pub const OSCILLOSCOPE_MEMORY_SAMPLE_RATE: i32 = 22000;
/// `kAudioMemorySamples`.
pub const AUDIO_MEMORY_SAMPLES: usize = 1 << 15;
/// The oscilloscope memory's length: `2 * kOscilloscopeMemoryResolution`.
pub const OSCILLOSCOPE_SAMPLES: usize = 2 * OSCILLOSCOPE_MEMORY_RESOLUTION;

/// `kMaxDelayPairs` of the chorus.
const CHORUS_DELAY_PAIRS: usize = 4;

/// Every status output upstream's engine registers, in a fixed order. These
/// are upstream's 32 `createStatusOutput` call sites, with their loops
/// expanded:
///
/// - `SoundEngine::init` (`synth_engine/sound_engine.cpp`): the six
///   compressor bands, `chorus_delays1..4`, `phaser_cutoff`,
///   `flanger_delay_frequency`, `peak_meter` and `peak_meter_memory`;
/// - `SynthVoiceHandler::init` (`modules/synth_voice_handler.cpp`):
///   `macro_control_1..4`, `random_1..4`, `random`, `stereo`,
///   `sample_phase`, `num_voices`, `modulation_source_1..64` and
///   `modulation_amount_1..64`;
/// - `SynthVoiceHandler::createModulators`: `lfo_1..8` with `_phase` and
///   `_frequency`, `env_1..6` with `_phase`, `note`, `note_in_octave`,
///   `aftertouch`, `velocity`, `slide`, `lift`, `mod_wheel` and
///   `pitch_wheel`.
pub fn status_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names: Vec<String> = [
            "compressor_low_input",
            "compressor_band_input",
            "compressor_high_input",
            "compressor_low_output",
            "compressor_band_output",
            "compressor_high_output",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        names.extend((1..=CHORUS_DELAY_PAIRS).map(|i| format!("chorus_delays{i}")));
        names.extend(
            [
                "phaser_cutoff",
                "flanger_delay_frequency",
                "peak_meter",
                "peak_meter_memory",
            ]
            .map(String::from),
        );
        names.extend((1..=NUM_MACROS).map(|i| format!("macro_control_{i}")));
        names.extend((1..=NUM_RANDOM_LFOS).map(|i| format!("random_{i}")));
        names.extend(["random", "stereo", "sample_phase", "num_voices"].map(String::from));
        names.extend((1..=MAX_MODULATION_CONNECTIONS).map(|i| format!("modulation_source_{i}")));
        names.extend((1..=MAX_MODULATION_CONNECTIONS).map(|i| format!("modulation_amount_{i}")));
        for i in 1..=NUM_LFOS {
            names.extend([
                format!("lfo_{i}"),
                format!("lfo_{i}_phase"),
                format!("lfo_{i}_frequency"),
            ]);
        }
        for i in 1..=NUM_ENVELOPES {
            names.extend([format!("env_{i}"), format!("env_{i}_phase")]);
        }
        names.extend(
            [
                "note",
                "note_in_octave",
                "aftertouch",
                "velocity",
                "slide",
                "lift",
                "mod_wheel",
                "pitch_wheel",
            ]
            .map(String::from),
        );
        names
            .into_iter()
            .map(|name| &*Box::leak(name.into_boxed_str()))
            .collect()
    })
}

/// Every modulation destination (the registry's, in its order); each has a
/// mono modulation total, and the per-voice ones a poly total too.
pub fn destination_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(sas_preset::engine::modulation_destinations)
}

/// An index into [`status_names`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StatusId(pub usize);

/// An index into [`destination_names`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DestinationId(pub usize);

/// The id of status output `name`.
pub fn status_id(name: &str) -> Option<StatusId> {
    status_names()
        .iter()
        .position(|status| *status == name)
        .map(StatusId)
}

/// The id of modulation destination `name`.
pub fn destination_id(name: &str) -> Option<DestinationId> {
    destination_names()
        .iter()
        .position(|destination| *destination == name)
        .map(DestinationId)
}

/// One destination's modulation totals: the `trigger_value` of its mono and
/// poly readouts, as `ModulationMeter` reads them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModulationTotal {
    pub mono: [f32; 4],
    /// `None` for a destination without per-voice modulation.
    pub poly: Option<[f32; 4]>,
    /// Whether the mono readout's processor is enabled (`owner->enabled()`;
    /// `Wavetable3d::getOutputsTotal` shows the slider value when not).
    pub enabled: bool,
}

/// One block's telemetry.
#[derive(Clone, Debug)]
pub struct TelemetrySnapshot {
    /// How many snapshots the audio thread had published before this one
    /// (0 for the first).
    pub block: u64,
    status: Box<[[f32; 4]]>,
    totals: Box<[ModulationTotal]>,
    /// `SynthBase::oscilloscope_memory_`: the last complete window, left and
    /// right.
    oscilloscope: Box<[[f32; 2]]>,
    /// Bit `n` is set while note `n` is held, from the on-screen keyboard or
    /// the host (`sas_engine::keyboard`).
    held_notes: u128,
    /// `SynthBase::getSampleRate()`: the engine's sample rate, before
    /// oversampling.
    pub sample_rate: i32,
    /// `SoundEngine::getOversamplingAmount()`: the oversampling the
    /// `oversampling` control asks for.
    pub oversampling_amount: i32,
}

impl Default for TelemetrySnapshot {
    fn default() -> Self {
        TelemetrySnapshot {
            block: 0,
            status: vec![[0.0; 4]; status_names().len()].into_boxed_slice(),
            totals: vec![ModulationTotal::default(); destination_names().len()].into_boxed_slice(),
            oscilloscope: vec![[0.0; 2]; OSCILLOSCOPE_SAMPLES].into_boxed_slice(),
            held_notes: 0,
            sample_rate: 44100,
            oversampling_amount: 1,
        }
    }
}

impl TelemetrySnapshot {
    /// Status output `id`'s value (`StatusOutput::value()`, four lanes).
    pub fn status(&self, id: StatusId) -> [f32; 4] {
        self.status[id.0]
    }

    /// Status output `name`'s value (resolve ids once with [`status_id`]
    /// where it matters).
    pub fn status_by_name(&self, name: &str) -> Option<[f32; 4]> {
        status_id(name).map(|id| self.status(id))
    }

    /// Destination `id`'s modulation totals.
    pub fn modulation_total(&self, id: DestinationId) -> ModulationTotal {
        self.totals[id.0]
    }

    /// The oscilloscope memory (`2 * kOscilloscopeMemoryResolution` frames).
    pub fn oscilloscope(&self) -> &[[f32; 2]] {
        &self.oscilloscope
    }

    /// The notes held at the end of the block, one bit per note.
    pub fn held_notes(&self) -> u128 {
        self.held_notes
    }

    /// Whether note `note` was held at the end of the block.
    pub fn note_held(&self, note: u8) -> bool {
        note < 128 && self.held_notes & 1 << note != 0
    }

    /// Set by the block processor before the engine publishes.
    pub fn set_held_notes(&mut self, held_notes: u128) {
        self.held_notes = held_notes;
    }

    /// Writable views, for the engine that fills the snapshot.
    pub fn parts_mut(&mut self) -> (&mut [[f32; 4]], &mut [ModulationTotal], &mut [[f32; 2]]) {
        (&mut self.status, &mut self.totals, &mut self.oscilloscope)
    }
}

/// The index of the middle buffer, and whether it holds a snapshot the
/// reader has not taken yet.
const NEW: u8 = 4;

struct TripleBuffer {
    buffers: [UnsafeCell<TelemetrySnapshot>; 3],
    /// The middle buffer's index, plus [`NEW`] once the writer has
    /// published into it.
    middle: AtomicU8,
}

// SAFETY: each buffer is owned by exactly one of the writer (back), the
// middle slot and the reader (front) at any time; ownership moves only
// through the atomic swap of `middle`, with acquire/release ordering.
unsafe impl Sync for TripleBuffer {}

/// The audio side: fill [`snapshot_mut`](Self::snapshot_mut), then
/// [`publish`](Self::publish).
pub struct TelemetryWriter {
    shared: Arc<TripleBuffer>,
    back: usize,
    published: u64,
}

impl TelemetryWriter {
    /// The back buffer, which holds the snapshot published three swaps ago
    /// (or a default): every field the engine writes is overwritten.
    pub fn snapshot_mut(&mut self) -> &mut TelemetrySnapshot {
        // SAFETY: the back buffer belongs to the writer until it publishes.
        unsafe { &mut *self.shared.buffers[self.back].get() }
    }

    /// Makes the back buffer the newest snapshot.
    pub fn publish(&mut self) {
        let block = self.published;
        self.snapshot_mut().block = block;
        self.published += 1;
        let previous = self
            .shared
            .middle
            .swap(self.back as u8 | NEW, Ordering::AcqRel);
        self.back = usize::from(previous & !NEW);
    }
}

struct Reader {
    shared: Arc<TripleBuffer>,
    front: usize,
}

impl Reader {
    fn latest(&mut self) -> &TelemetrySnapshot {
        if self.shared.middle.load(Ordering::Acquire) & NEW != 0 {
            let previous = self.shared.middle.swap(self.front as u8, Ordering::AcqRel);
            self.front = usize::from(previous & !NEW);
        }
        // SAFETY: the front buffer belongs to the reader.
        unsafe { &*self.shared.buffers[self.front].get() }
    }
}

/// The editor side: cheap to clone. Reading takes a mutex that only readers
/// share; the audio thread never touches it.
#[derive(Clone)]
pub struct Telemetry {
    reader: Arc<Mutex<Reader>>,
    audio_memory: Arc<AudioMemory>,
    equalizer_memory: Arc<AudioMemory>,
}

impl Telemetry {
    /// A writer and its reader, around the engine's audio memory (with an
    /// equalizer memory that stays silent).
    pub fn new(audio_memory: Arc<AudioMemory>) -> (TelemetryWriter, Telemetry) {
        Telemetry::with_equalizer_memory(audio_memory, Arc::default())
    }

    /// A writer and its reader, around the engine's audio memory and its
    /// equalizer's.
    pub fn with_equalizer_memory(
        audio_memory: Arc<AudioMemory>,
        equalizer_memory: Arc<AudioMemory>,
    ) -> (TelemetryWriter, Telemetry) {
        let shared = Arc::new(TripleBuffer {
            buffers: std::array::from_fn(|_| UnsafeCell::new(TelemetrySnapshot::default())),
            middle: AtomicU8::new(1),
        });
        let writer = TelemetryWriter {
            shared: shared.clone(),
            back: 0,
            published: 0,
        };
        let reader = Reader { shared, front: 2 };
        (
            writer,
            Telemetry {
                reader: Arc::new(Mutex::new(reader)),
                audio_memory,
                equalizer_memory,
            },
        )
    }

    /// Runs `f` on the newest published snapshot.
    pub fn read<R>(&self, f: impl FnOnce(&TelemetrySnapshot) -> R) -> R {
        let mut reader = self
            .reader
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(reader.latest())
    }

    /// The spectrogram's audio memory.
    pub fn audio_memory(&self) -> &AudioMemory {
        &self.audio_memory
    }

    /// The equalizer's output memory (`SynthBase::getEqualizerMemory`), for
    /// the EQ section's spectrogram.
    pub fn equalizer_memory(&self) -> &AudioMemory {
        &self.equalizer_memory
    }
}

/// `SynthBase::audio_memory_` (a `StereoMemory` of `kAudioMemorySamples`):
/// the latest output samples, left and right, for the spectrogram. Written
/// by the audio thread and read by the editor with relaxed atomics; a read
/// that overlaps a write can mix the two blocks, which a display tolerates.
pub struct AudioMemory {
    samples: Box<[[AtomicU32; 2]]>,
    /// Frames pushed so far.
    written: AtomicUsize,
}

impl Default for AudioMemory {
    fn default() -> Self {
        AudioMemory {
            samples: (0..AUDIO_MEMORY_SAMPLES)
                .map(|_| [AtomicU32::new(0), AtomicU32::new(0)])
                .collect(),
            written: AtomicUsize::new(0),
        }
    }
}

impl AudioMemory {
    /// `push(sample)`: the left and right lanes of one output frame.
    pub fn push(&self, left: f32, right: f32) {
        let written = self.written.load(Ordering::Relaxed);
        let slot = &self.samples[written % AUDIO_MEMORY_SAMPLES];
        slot[0].store(left.to_bits(), Ordering::Relaxed);
        slot[1].store(right.to_bits(), Ordering::Relaxed);
        self.written
            .store(written.wrapping_add(1), Ordering::Release);
    }

    /// Frames pushed so far.
    pub fn written(&self) -> usize {
        self.written.load(Ordering::Acquire)
    }

    /// Fills `frames` with the latest `frames.len()` frames (at most
    /// [`AUDIO_MEMORY_SAMPLES`]), oldest first; frames never written read
    /// as silence.
    pub fn read_latest(&self, frames: &mut [[f32; 2]]) {
        let count = frames.len().min(AUDIO_MEMORY_SAMPLES);
        let written = self.written();
        let start = frames.len() - count;
        frames[..start].fill([0.0; 2]);
        for (i, frame) in frames[start..].iter_mut().enumerate() {
            let age = count - i;
            if age > written {
                *frame = [0.0; 2];
                continue;
            }
            let slot = &self.samples[(written - age) % AUDIO_MEMORY_SAMPLES];
            *frame = [
                f32::from_bits(slot[0].load(Ordering::Relaxed)),
                f32::from_bits(slot[1].load(Ordering::Relaxed)),
            ];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_cover_upstream() {
        // Sound engine; macros, random LFOs, the four singles; slots; LFOs;
        // envelopes; expression sources.
        assert_eq!(status_names().len(), 14 + 4 + 4 + 4 + 128 + 24 + 12 + 8);
        let unique: std::collections::BTreeSet<_> = status_names().iter().collect();
        assert_eq!(unique.len(), status_names().len());
        assert_eq!(
            status_id("lfo_3_phase"),
            Some(StatusId(
                status_names()
                    .iter()
                    .position(|n| *n == "lfo_3_phase")
                    .unwrap()
            ))
        );
        assert_eq!(
            destination_names().len(),
            sas_preset::engine::modulation_destinations().len()
        );
        assert!(destination_id("filter_1_cutoff").is_some());
        assert!(destination_id("nothing").is_none());
    }

    #[test]
    fn the_reader_sees_the_newest_published_snapshot() {
        let (mut writer, telemetry) = Telemetry::new(Arc::new(AudioMemory::default()));
        assert_eq!(telemetry.read(|snapshot| snapshot.block), 0);
        for block in 0..5u64 {
            writer.snapshot_mut().parts_mut().0[0] = [block as f32; 4];
            writer.publish();
        }
        telemetry.read(|snapshot| {
            assert_eq!(snapshot.block, 4);
            assert_eq!(snapshot.status(StatusId(0)), [4.0; 4]);
        });
        // Nothing new: the same snapshot again.
        assert_eq!(telemetry.read(|snapshot| snapshot.block), 4);
        writer.snapshot_mut().parts_mut().0[0] = [9.0; 4];
        writer.publish();
        assert_eq!(
            telemetry.read(|snapshot| snapshot.status(StatusId(0))),
            [9.0; 4]
        );
    }

    #[test]
    fn snapshots_never_tear_across_threads() {
        let (mut writer, telemetry) = Telemetry::new(Arc::new(AudioMemory::default()));
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = {
            let done = done.clone();
            std::thread::spawn(move || {
                let mut last = 0;
                while !done.load(Ordering::Relaxed) {
                    telemetry.read(|snapshot| {
                        let value = snapshot.block as f32;
                        assert!(snapshot.block >= last, "went back in time");
                        last = snapshot.block;
                        if snapshot.block > 0 {
                            assert!(snapshot.status.iter().all(|v| *v == [value; 4]));
                            assert!(snapshot.oscilloscope.iter().all(|v| *v == [value; 2]));
                        }
                    });
                }
            })
        };
        for block in 0..20_000u64 {
            let value = block as f32;
            let (status, _, oscilloscope) = writer.snapshot_mut().parts_mut();
            status.fill([value; 4]);
            oscilloscope.fill([value; 2]);
            writer.publish();
        }
        done.store(true, Ordering::Relaxed);
        reader.join().unwrap();
    }

    #[test]
    fn audio_memory_reads_the_latest_frames_oldest_first() {
        let memory = AudioMemory::default();
        let mut frames = [[9.0f32; 2]; 4];
        memory.read_latest(&mut frames);
        assert_eq!(frames, [[0.0; 2]; 4]);
        for i in 0..(AUDIO_MEMORY_SAMPLES + 3) {
            memory.push(i as f32, -(i as f32));
        }
        memory.read_latest(&mut frames);
        let last = (AUDIO_MEMORY_SAMPLES + 2) as f32;
        assert_eq!(frames[3], [last, -last]);
        assert_eq!(frames[0], [last - 3.0, -(last - 3.0)]);
    }
}
