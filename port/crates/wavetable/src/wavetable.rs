// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ports `vital/src/synthesis/lookups/wavetable.{h,cpp}` (Copyright 2013-2019
//! Matt Tytel, GPL-3.0-or-later): the rendered frames and the band-limited
//! lookup data the oscillator reads.
//!
//! Upstream double-buffers this between the GUI and audio threads
//! (`current_data_`/`active_audio_data_`). Here the wavetable owns one
//! [`WavetableData`]; the engine decides how to share it across threads.

use crate::complex::Complex;
use crate::wave_frame::{WaveFrame, NUM_REAL_COMPLEX, WAVEFORM_BITS, WAVEFORM_SIZE};
use sas_common::scalar;

/// `Wavetable::kFrequencyBins`.
pub const FREQUENCY_BINS: i32 = WAVEFORM_BITS as i32;
/// `Wavetable::kExtraValues`.
pub const EXTRA_VALUES: usize = 3;
/// `Wavetable::kNumHarmonics`.
pub const NUM_HARMONICS: usize = WAVEFORM_SIZE / 2 + 1;
/// `Wavetable::kPolyFrequencySize`, in `poly_float`s.
pub const POLY_FREQUENCY_SIZE: usize = 2 * NUM_HARMONICS / 4 + 2;
/// The same size in floats (four per `poly_float`).
pub const FREQUENCY_FLOATS: usize = POLY_FREQUENCY_SIZE * 4;

/// `Wavetable::kZeroWaveform`.
pub static ZERO_WAVEFORM: [f32; WAVEFORM_SIZE + EXTRA_VALUES] = [0.0; WAVEFORM_SIZE + EXTRA_VALUES];

/// `Wavetable::WavetableData`. The per-frame frequency arrays use the
/// upstream `poly_float` memory layout: entry `2 * h` and `2 * h + 1` belong
/// to harmonic `h` (amplitude twice, or the real and imaginary parts of the
/// unit phasor), for `h` in `0..NUM_HARMONICS`; the last six floats are
/// padding (zero).
#[derive(Clone, Debug)]
pub struct WavetableData {
    pub num_frames: usize,
    pub frequency_ratio: f32,
    pub sample_rate: f32,
    pub version: i32,
    pub wave_data: Vec<[f32; WAVEFORM_SIZE]>,
    pub frequency_amplitudes: Vec<[f32; FREQUENCY_FLOATS]>,
    pub normalized_frequencies: Vec<[f32; FREQUENCY_FLOATS]>,
    pub phases: Vec<[f32; FREQUENCY_FLOATS]>,
}

impl WavetableData {
    fn new(num_frames: usize, version: i32) -> Self {
        Self {
            num_frames,
            frequency_ratio: 1.0,
            sample_rate: 44100.0,
            version,
            wave_data: vec![[0.0; WAVEFORM_SIZE]; num_frames],
            frequency_amplitudes: vec![[0.0; FREQUENCY_FLOATS]; num_frames],
            normalized_frequencies: vec![[0.0; FREQUENCY_FLOATS]; num_frames],
            phases: vec![[0.0; FREQUENCY_FLOATS]; num_frames],
        }
    }
}

/// One frame of a table, as `loadWaveFrame` stores it: its waveform and its
/// three frequency arrays, for copying a frame from one table to another.
#[derive(Clone, Debug, PartialEq)]
pub struct WavetableFrame {
    pub wave: [f32; WAVEFORM_SIZE],
    pub frequency_amplitudes: [f32; FREQUENCY_FLOATS],
    pub normalized_frequencies: [f32; FREQUENCY_FLOATS],
    pub phases: [f32; FREQUENCY_FLOATS],
}

/// `vital::Wavetable`.
#[derive(Clone, Debug)]
pub struct Wavetable {
    pub name: String,
    pub author: String,
    max_frames: usize,
    data: WavetableData,
    shepard_table: bool,
}

impl Wavetable {
    /// `Wavetable(max_frames)`: one silent frame.
    pub fn new(max_frames: usize) -> Self {
        let mut wavetable = Self {
            name: String::new(),
            author: String::new(),
            max_frames,
            data: WavetableData::new(0, 0),
            shepard_table: false,
        };
        wavetable.load_default_wavetable();
        wavetable
    }

    /// `loadDefaultWavetable()`.
    pub fn load_default_wavetable(&mut self) {
        self.set_num_frames(1);
        let default_frame = WaveFrame::default();
        self.load_wave_frame(&default_frame);
    }

    /// `setNumFrames(num_frames)`: keeps existing frames and fills new ones
    /// with copies of the last; bumps the version when the count changes.
    pub fn set_num_frames(&mut self, num_frames: usize) {
        assert!(
            num_frames <= self.max_frames,
            "{num_frames} frames > max {}",
            self.max_frames
        );
        let has_data = self.data.num_frames > 0 || self.data.version > 0;
        if has_data && num_frames == self.data.num_frames {
            return;
        }

        let old = std::mem::replace(&mut self.data, WavetableData::new(num_frames, 0));
        self.data.version = old.version + 1;
        let copy_frames = num_frames.min(old.num_frames);
        for i in 0..copy_frames {
            self.data.wave_data[i] = old.wave_data[i];
            self.data.frequency_amplitudes[i] = old.frequency_amplitudes[i];
            self.data.normalized_frequencies[i] = old.normalized_frequencies[i];
            self.data.phases[i] = old.phases[i];
        }

        if has_data {
            self.data.frequency_ratio = old.frequency_ratio;
            self.data.sample_rate = old.sample_rate;
            if old.num_frames > 0 {
                let last = old.num_frames - 1;
                for i in old.num_frames..num_frames {
                    self.data.wave_data[i] = old.wave_data[last];
                    self.data.frequency_amplitudes[i] = old.frequency_amplitudes[last];
                    self.data.normalized_frequencies[i] = old.normalized_frequencies[last];
                    self.data.phases[i] = old.phases[last];
                }
            }
        }
    }

    pub fn set_frequency_ratio(&mut self, frequency_ratio: f32) {
        self.data.frequency_ratio = frequency_ratio;
    }

    pub fn set_sample_rate(&mut self, rate: f32) {
        self.data.sample_rate = rate;
    }

    /// `getFrequencyFloatBin(phase_increment)`.
    #[inline]
    pub fn frequency_float_bin(phase_increment: f32) -> f32 {
        scalar::log2(1.0 / phase_increment)
    }

    /// `getFrequencyBin(phase_increment)`.
    #[inline]
    pub fn frequency_bin(phase_increment: f32) -> i32 {
        let num_waves = (1.0 / phase_increment) as i32;
        scalar::iclamp(scalar::ilog2(num_waves), 0, FREQUENCY_BINS - 1)
    }

    /// `clampFrame(frame)`.
    #[inline]
    pub fn clamp_frame(&self, frame: usize) -> usize {
        frame.min(self.data.num_frames - 1)
    }

    /// `getAllData()`.
    pub fn data(&self) -> &WavetableData {
        &self.data
    }

    /// `getBuffer(frame_index)`.
    pub fn buffer(&self, frame_index: usize) -> &[f32; WAVEFORM_SIZE] {
        &self.data.wave_data[self.clamp_frame(frame_index)]
    }

    /// `getFrequencyAmplitudes(frame_index)`.
    pub fn frequency_amplitudes(&self, frame_index: usize) -> &[f32; FREQUENCY_FLOATS] {
        &self.data.frequency_amplitudes[self.clamp_frame(frame_index)]
    }

    /// `getNormalizedFrequencies(frame_index)`.
    pub fn normalized_frequencies(&self, frame_index: usize) -> &[f32; FREQUENCY_FLOATS] {
        &self.data.normalized_frequencies[self.clamp_frame(frame_index)]
    }

    pub fn version(&self) -> i32 {
        self.data.version
    }

    /// Swaps in `rendered`, a table a `WavetableCreator` rendered into a
    /// fresh `Wavetable` of the same `max_frames`, as if it had rendered into
    /// this one: the version moves on only when the frame count changes
    /// (`setNumFrames`), so oscillators reset exactly when they would have.
    /// `rendered` gets this table's old contents; nothing is allocated or
    /// freed here.
    pub fn take_rendered(&mut self, rendered: &mut Wavetable) {
        debug_assert_eq!(self.max_frames, rendered.max_frames);
        let frames_changed = rendered.data.num_frames != self.data.num_frames;
        let version = self.data.version + i32::from(frames_changed);
        std::mem::swap(self, rendered);
        self.data.version = version;
    }

    pub fn num_frames(&self) -> usize {
        self.data.num_frames
    }

    pub fn set_shepard_table(&mut self, shepard: bool) {
        self.shepard_table = shepard;
    }

    pub fn is_shepard_table(&self) -> bool {
        self.shepard_table
    }

    /// `loadWaveFrame(wave_frame)`: stores the frame at `wave_frame.index`.
    pub fn load_wave_frame(&mut self, wave_frame: &WaveFrame) {
        self.load_wave_frame_to(wave_frame, wave_frame.index);
    }

    /// `loadWaveFrame(wave_frame, to_index)`: ignored past the last frame.
    pub fn load_wave_frame_to(&mut self, wave_frame: &WaveFrame, to_index: i32) {
        if to_index < 0 || to_index as usize >= self.data.num_frames {
            return;
        }
        let index = to_index as usize;
        let spectrum = &wave_frame.frequency_domain[..NUM_HARMONICS];

        let amplitudes = &mut self.data.frequency_amplitudes[index];
        for (i, bin) in spectrum.iter().enumerate() {
            let amplitude = bin.abs();
            amplitudes[2 * i] = amplitude;
            amplitudes[2 * i + 1] = amplitude;
        }

        let normalized = &mut self.data.normalized_frequencies[index];
        let phases = &mut self.data.phases[index];
        for (i, bin) in spectrum.iter().enumerate() {
            let arg = bin.arg();
            let unit = Complex::polar(1.0, arg);
            normalized[2 * i] = unit.re;
            normalized[2 * i + 1] = unit.im;
            phases[2 * i] = arg;
            phases[2 * i + 1] = arg;
        }

        self.data.wave_data[index].copy_from_slice(&wave_frame.time_domain[..WAVEFORM_SIZE]);
    }

    /// A copy of frame `index`, or None past the last frame.
    pub fn frame(&self, index: usize) -> Option<WavetableFrame> {
        if index >= self.data.num_frames {
            return None;
        }
        Some(WavetableFrame {
            wave: self.data.wave_data[index],
            frequency_amplitudes: self.data.frequency_amplitudes[index],
            normalized_frequencies: self.data.normalized_frequencies[index],
            phases: self.data.phases[index],
        })
    }

    /// Writes `frame` over frame `index`, as `loadWaveFrame` would have
    /// stored it; ignored past the last frame. The version stays: the frame
    /// count doesn't change. Nothing is allocated.
    pub fn write_frame(&mut self, index: usize, frame: &WavetableFrame) {
        if index >= self.data.num_frames {
            return;
        }
        self.data.wave_data[index] = frame.wave;
        self.data.frequency_amplitudes[index] = frame.frequency_amplitudes;
        self.data.normalized_frequencies[index] = frame.normalized_frequencies;
        self.data.phases[index] = frame.phases;
    }

    /// `postProcess(max_span)`: scales every frame by `2 / max_span` (when
    /// positive), then gives harmonics that are near silent in some frames
    /// the phase interpolated between the frames where they are audible.
    pub fn post_process(&mut self, max_span: f32) {
        const MIN_AMPLITUDE_PHASE: f32 = 0.1;
        let num_frames = self.data.num_frames;

        if max_span > 0.0 {
            let scale = 2.0 / max_span;
            for w in 0..num_frames {
                for value in self.data.frequency_amplitudes[w].iter_mut() {
                    *value *= scale;
                }
                for value in self.data.wave_data[w].iter_mut() {
                    *value *= scale;
                }
            }
        }

        let get = |data: &WavetableData, w: usize, i: usize| {
            Complex::new(
                data.normalized_frequencies[w][2 * i],
                data.normalized_frequencies[w][2 * i + 1],
            )
        };
        for i in 0..NUM_HARMONICS {
            let amp_index = 2 * i;
            let mut last_min_amp_frame: i32 = -1;
            let mut last_normalized_frequency = Complex::new(0.0, 1.0);
            for w in 0..num_frames {
                let amplitude = self.data.frequency_amplitudes[w][amp_index];
                let normalized_frequency = get(&self.data, w, i);

                if amplitude > MIN_AMPLITUDE_PHASE {
                    if last_min_amp_frame < 0 {
                        last_min_amp_frame = 0;
                        last_normalized_frequency = normalized_frequency;
                    }

                    let delta_normalized_frequency =
                        normalized_frequency - last_normalized_frequency;
                    for frame in (last_min_amp_frame + 1) as usize..w {
                        let t = (frame as i32 - last_min_amp_frame) as f32 * 1.0
                            / (w as i32 - last_min_amp_frame) as f32;
                        let normalized = delta_normalized_frequency * t + last_normalized_frequency;
                        self.data.normalized_frequencies[frame][2 * i] = normalized.re;
                        self.data.normalized_frequencies[frame][2 * i + 1] = normalized.im;
                    }
                    last_normalized_frequency = normalized_frequency;
                    last_min_amp_frame = w as i32;
                }
            }
            for frame in (last_min_amp_frame + 1) as usize..num_frames {
                self.data.normalized_frequencies[frame][2 * i] = last_normalized_frequency.re;
                self.data.normalized_frequencies[frame][2 * i + 1] = last_normalized_frequency.im;
            }
        }
    }
}

const _: () = assert!(NUM_REAL_COMPLEX == NUM_HARMONICS);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_upstream() {
        assert_eq!(POLY_FREQUENCY_SIZE, 514);
        assert_eq!(FREQUENCY_FLOATS, 2056);
    }

    #[test]
    fn default_and_resize() {
        let mut wavetable = Wavetable::new(257);
        assert_eq!(wavetable.num_frames(), 1);
        assert_eq!(wavetable.version(), 1);
        assert_eq!(wavetable.normalized_frequencies(0)[0], 1.0);
        wavetable.set_num_frames(1);
        assert_eq!(wavetable.version(), 1);
        wavetable.set_num_frames(3);
        assert_eq!(wavetable.version(), 2);
        assert_eq!(wavetable.normalized_frequencies(2)[0], 1.0);
        assert_eq!(wavetable.clamp_frame(10), 2);
    }

    #[test]
    fn frequency_bins() {
        assert_eq!(Wavetable::frequency_bin(1.0 / 2048.0), 10);
        assert_eq!(Wavetable::frequency_bin(0.5), 1);
        assert_eq!(Wavetable::frequency_bin(1.0), 0);
        assert!((Wavetable::frequency_float_bin(1.0 / 8.0) - 3.0).abs() < 1e-3);
    }
}
