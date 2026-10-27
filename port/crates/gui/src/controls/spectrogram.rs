// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! A spectrum of recent audio, each channel's drawn as a filled line over
//! log frequency, falling back slowly as the sound dies away.
//!
//! Ports `Spectrogram` (`vital/src/interface/editor_components/oscilloscope.cpp`),
//! reading an engine [`AudioMemory`] (the output's or the equalizer's)
//! through telemetry.

use sas_engine::sas_oscillators::sas_wavetable::FourierTransform;
use sas_engine::telemetry::AudioMemory;
use sas_simd::utils;

use crate::colour::Colour;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadBatch};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `kResolution`: the points along each line.
pub const RESOLUTION: usize = 300;
/// `kDecayMult`.
const DECAY_MULT: f32 = 0.008;
/// `kBits`.
const BITS: u32 = 14;
/// `kAudioSize`: the samples analysed each frame.
pub const AUDIO_SIZE: usize = 1 << BITS;
/// `kDefaultMaxDb`.
pub const DEFAULT_MAX_DB: f32 = 0.0;
/// `kDefaultMinDb`.
pub const DEFAULT_MIN_DB: f32 = -50.0;
/// `kDefaultMinFrequency`.
pub const DEFAULT_MIN_FREQUENCY: f32 = 9.2;
/// `kDefaultMaxFrequency`.
pub const DEFAULT_MAX_FREQUENCY: f32 = 21000.0;
/// `kDbSlopePerOctave`: the tilt that flattens pink noise.
const DB_SLOPE_PER_OCTAVE: f32 = 3.0;
/// The constructor's `kDefaultAmp`.
const DEFAULT_AMP: f32 = 0.000_001;
/// `render`'s `setLineWidth(2.0f)`, whatever the owner set before.
const LINE_WIDTH: f32 = 2.0;

/// How a spectrogram reads its memory and scales what it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spectrogram {
    pub min_frequency: f32,
    pub max_frequency: f32,
    pub min_db: f32,
    pub max_db: f32,
    /// `oversample_amount_`: the memory runs this many times faster than
    /// `sample_rate`.
    pub oversample_amount: i32,
    /// `sample_rate_`: the synth's (`getSampleRate`).
    pub sample_rate: i32,
    /// `paint_background_lines_`.
    pub paint_background_lines: bool,
}

impl Default for Spectrogram {
    fn default() -> Spectrogram {
        Spectrogram {
            min_frequency: DEFAULT_MIN_FREQUENCY,
            max_frequency: DEFAULT_MAX_FREQUENCY,
            min_db: DEFAULT_MIN_DB,
            max_db: DEFAULT_MAX_DB,
            oversample_amount: 1,
            sample_rate: 44100,
            paint_background_lines: true,
        }
    }
}

/// A spectrogram's running state: each channel's smoothed amplitudes
/// (`left_amps_`, `right_amps_`) and the transform's buffers, made once.
pub struct SpectrogramAnalysis {
    amps: [[f32; RESOLUTION]; 2],
    transform: FourierTransform,
    /// `transform_buffer_` (`2 * kAudioSize`).
    buffer: Vec<f32>,
    /// The memory's frames, read once per frame for both channels.
    frames: Vec<[f32; 2]>,
    /// The egui frame the amplitudes last moved in.
    last_frame: Option<u64>,
}

impl Default for SpectrogramAnalysis {
    fn default() -> SpectrogramAnalysis {
        SpectrogramAnalysis {
            amps: [[DEFAULT_AMP; RESOLUTION]; 2],
            transform: FourierTransform::new(BITS),
            buffer: vec![0.0; 2 * AUDIO_SIZE],
            frames: vec![[0.0; 2]; AUDIO_SIZE + 1],
            last_frame: None,
        }
    }
}

impl SpectrogramAnalysis {
    /// Channel `channel`'s smoothed amplitudes, low frequency first.
    pub fn amplitudes(&self, channel: usize) -> &[f32; RESOLUTION] {
        &self.amps[channel]
    }

    /// [`update`](Self::update) once for egui frame `frame`: upstream
    /// moves the amplitudes once per render, and egui may run a frame in
    /// more than one pass.
    pub fn update_once(&mut self, spectrogram: &Spectrogram, memory: &AudioMemory, frame: u64) {
        if self.last_frame == Some(frame) {
            return;
        }
        self.last_frame = Some(frame);
        self.update(spectrogram, memory);
    }

    /// `updateAmplitudes(0)` and `updateAmplitudes(1)`: analyses the latest
    /// `kAudioSize` samples of `memory` and moves each channel's amplitudes
    /// towards them.
    pub fn update(&mut self, spectrogram: &Spectrogram, memory: &AudioMemory) {
        // readSamples(…, offset 0) ends one sample before the newest.
        memory.read_latest(&mut self.frames);
        for channel in 0..2 {
            self.update_channel(spectrogram, channel);
        }
    }

    fn update_channel(&mut self, spectrogram: &Spectrogram, channel: usize) {
        const MIN_AMP: f32 = 0.000_001;
        const START_SCALE_AMP: f32 = 0.001;
        const MIN_DECAY: f32 = 0.06;
        let oversample = spectrogram.oversample_amount as f32;
        let min_frequency = spectrogram.min_frequency / oversample;
        let max_frequency = spectrogram.max_frequency / oversample;
        let sample_hz = spectrogram.sample_rate as f32 / AUDIO_SIZE as f32;
        let start_octave = (min_frequency / sample_hz).log2();
        let end_octave = (BITS as f32 - start_octave + 1.0).min((max_frequency / sample_hz).log2());
        let num_octaves = end_octave - start_octave;

        for (value, frame) in self.buffer.iter_mut().zip(&self.frames[..AUDIO_SIZE]) {
            *value = frame[channel];
        }
        apply_window(&mut self.buffer[..AUDIO_SIZE]);
        self.transform.transform_real_forward(&mut self.buffer);
        let magnitude = |bin: usize| {
            let (re, im) = (self.buffer[2 * bin], self.buffer[2 * bin + 1]);
            (re * re + im * im).sqrt()
        };

        let amps = &mut self.amps[channel];
        let mut last_bin = 2.0f32.powf(start_octave);
        for (i, amp) in amps.iter_mut().enumerate() {
            let t = i as f32 / (RESOLUTION as f32 - 1.0);
            let octave = num_octaves * t + start_octave;
            let bin = 2.0f32.powf(octave);
            let bin_index = last_bin as usize;
            let bin_t = last_bin - bin_index as f32;
            let previous = magnitude(bin_index);
            let next = magnitude(bin_index + 1);
            let mut amplitude = previous + (next - previous) * bin_t;
            if bin - last_bin > 1.0 {
                // for (int j = last_bin + 1; j < bin; ++j): the float
                // last_bin + 1 truncates when it starts the int loop.
                let mut j = (last_bin + 1.0) as usize;
                while (j as f32) < bin {
                    amplitude = amplitude.max(magnitude(j));
                    j += 1;
                }
            }
            last_bin = bin;
            let amplitude = MIN_AMP.max(2.0 * amplitude / AUDIO_SIZE as f32);
            let mut db = utils::magnitude_to_db(amplitude.max(*amp) / START_SCALE_AMP);
            db += octave * DB_SLOPE_PER_OCTAVE;
            let decay = MIN_DECAY.max((DECAY_MULT * db).min(1.0));
            *amp = MIN_AMP.max(*amp + (amplitude - *amp) * decay);
        }
    }
}

/// `applyWindow`: upstream's window, `0.5 · (cos θ + 1)` with θ stepped by
/// rotation in double precision from π to 2π, so it rises from 0 to 1 over
/// the samples (half a Hann window, not a whole one).
fn apply_window(samples: &mut [f32]) {
    let increment = std::f64::consts::PI / (samples.len() as f64 - 1.0);
    let (mut real, mut imag) = (-1.0f64, 0.0f64);
    let (real_mult, imag_mult) = (increment.cos(), increment.sin());
    for sample in samples {
        // `float *= double`: the product is taken in double, then rounded.
        *sample = (f64::from(*sample) * 0.5 * (real + 1.0)) as f32;
        let next_real = real * real_mult - imag * imag_mult;
        let next_imag = imag * real_mult + real * imag_mult;
        real = next_real;
        imag = next_imag;
    }
}

impl Spectrogram {
    /// `paintBackground`: the widget background and, unless turned off, a
    /// line at 1, 2, … 10, 20, … 100 Hz and so on.
    pub fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
        const LINE_SPACING: i32 = 10;
        pixels.fill_rect(bounds, scope.color(ColorId::WidgetBackground));
        if !self.paint_background_lines {
            return;
        }
        let max_octave = (self.max_frequency / self.min_frequency).log2();
        let colour = scope
            .color(ColorId::LightenScreen)
            .with_multiplied_alpha(0.5);
        // Graphics clips to the component: lines below the lowest frequency
        // fall to its left and don't show.
        let line_at = |x: i32| {
            if (0..bounds.width).contains(&x) {
                pixels.fill_rect(Rect::new(bounds.x + x, bounds.y, 1, bounds.height), colour);
            }
        };
        let (mut frequency, mut increment, mut x) = (0.0f32, 1.0f32, 0);
        while frequency < self.max_frequency {
            for _ in 0..LINE_SPACING {
                frequency += increment;
                let t = (frequency / self.min_frequency).log2() / max_octave;
                x = (t * bounds.width as f32).round() as i32;
                line_at(x);
            }
            line_at(x);
            increment *= LINE_SPACING as f32;
        }
    }

    /// `render`: both channels' lines, filled to the bottom, in `line`
    /// (`kWidgetPrimary1`) over `fill` (`kWidgetPrimary2`, faded by the
    /// skin's fill fade), and `body`-coloured rounded corners.
    pub fn paint(
        &self,
        pixels: &PixelPainter,
        scope: &Scope,
        bounds: Rect,
        analysis: &SpectrogramAnalysis,
        (line_colour, fill): (Colour, Colour),
        body: Colour,
    ) {
        let mut line = LineRenderer::new(RESOLUTION, false);
        line.fill = true;
        line.line_width = LINE_WIDTH;
        line.fill_center = -1.0;
        line.color = line_colour;
        let fill_fade = scope.value(ValueId::WidgetFillFade);
        line.set_fill_colors(fill.with_multiplied_alpha(1.0 - fill_fade), fill);

        let (width, height) = (bounds.width as f32, bounds.height as f32);
        let range_mult = 1.0 / (self.max_db - self.min_db);
        let num_octaves = (self.max_frequency / self.min_frequency).log2();
        for channel in 0..2 {
            // drawWaveform.
            for (i, &amp) in analysis.amplitudes(channel).iter().enumerate() {
                let t = i as f32 / (RESOLUTION as f32 - 1.0);
                let db = utils::magnitude_to_db(amp) + t * num_octaves * DB_SLOPE_PER_OCTAVE;
                let y = (db - self.min_db) * range_mult;
                line.set_x_at(i, t * width);
                line.set_y_at(i, height - y * height);
            }
            line.paint(pixels, RectF::from(bounds), true);
        }

        let mut corners = QuadBatch::new();
        for corner in Quad::corners(
            bounds.into(),
            scope.value(ValueId::WidgetRoundedCorner),
            body,
        ) {
            corners.push(corner);
        }
        corners.paint(pixels, RectF::from(bounds));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full-scale sine at 1 kHz, the memory at 44.1 kHz: after enough
    /// frames to settle, the loudest point sits at 1 kHz on the log axis, at
    /// about the window's gain (its mean, ½), and silence decays.
    #[test]
    fn a_sine_peaks_at_its_frequency() {
        let memory = AudioMemory::default();
        let frequency = 1000.0f32;
        for i in 0..AUDIO_SIZE + 1 {
            let value = (2.0 * std::f32::consts::PI * frequency * i as f32 / 44100.0).sin();
            memory.push(value, value);
        }
        let spectrogram = Spectrogram::default();
        let mut analysis = SpectrogramAnalysis::default();
        for _ in 0..200 {
            analysis.update(&spectrogram, &memory);
        }
        let amps = analysis.amplitudes(0);
        let (peak, &amp) = amps
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap();
        let num_octaves = (DEFAULT_MAX_FREQUENCY / DEFAULT_MIN_FREQUENCY).log2();
        let t = peak as f32 / (RESOLUTION as f32 - 1.0);
        let peak_frequency = DEFAULT_MIN_FREQUENCY * 2.0f32.powf(t * num_octaves);
        assert!(
            (peak_frequency / frequency).log2().abs() < 0.05,
            "peak at {peak_frequency} Hz"
        );
        // 2·|bin|/N is the window's mean, ½ (−6 dB), at the sine's own
        // bin; 1 kHz falls between bins and between the 300 points, which
        // reads lower.
        let db = utils::magnitude_to_db(amp);
        assert!((db - utils::magnitude_to_db(0.5)).abs() < 2.0, "{db} dB");
        assert_eq!(analysis.amplitudes(1), amps, "the right channel matches");

        let silence = AudioMemory::default();
        let before = analysis.amplitudes(0)[peak];
        analysis.update(&spectrogram, &silence);
        let after = analysis.amplitudes(0)[peak];
        assert!(after < before && after > 0.0, "{before} -> {after}");
    }

    /// `applyWindow` rises from 0 to 1, through ½ halfway.
    #[test]
    fn the_window_rises_from_zero_to_one() {
        let mut samples = vec![1.0f32; 9];
        apply_window(&mut samples);
        assert!(samples[0].abs() < 1e-6);
        assert!((samples[4] - 0.5).abs() < 1e-6);
        assert!((samples[8] - 1.0).abs() < 1e-6);
        assert!(samples.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
