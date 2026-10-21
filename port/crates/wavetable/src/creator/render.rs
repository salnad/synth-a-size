//! Wavetable rendering: each source and modifier's `render`, the group and
//! creator render loops, `renderToBuffer` and the audio-file importers.
//!
//! Ports the render code of `vital/src/common/wavetable/*.cpp` (Copyright
//! 2013-2019 Matt Tytel, GPL-3.0-or-later). Mixed `float`/`double`
//! arithmetic follows the upstream types expression by expression. Double
//! `exp`/`cos` go through [`crate::platform_libm`]; float functions use the
//! system libm like the reference.

use std::fmt;

use sas_common::scalar::{self, PI};

use super::component::*;
use super::keyframes::*;
use super::pitch_detector::PitchDetector;
use super::{WavetableCreator, WavetableGroup};
use crate::complex::Complex;
use crate::wave_frame::{WaveFrame, NUM_REAL_COMPLEX, WAVEFORM_SIZE};
use crate::wavetable::Wavetable;
use crate::NUM_OSCILLATOR_WAVE_FRAMES;

/// A render upstream cannot perform (it would assert or read out of range).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    /// A keyframe position beyond the wavetable's frame count.
    TooManyFrames { frames: i32, max: usize },
    /// Audio too short for pitch detection.
    AudioTooShort { samples: usize, needed: usize },
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::TooManyFrames { frames, max } => {
                write!(f, "{frames} frames exceed the maximum of {max}")
            }
            RenderError::AudioTooShort { samples, needed } => {
                write!(f, "{samples} audio samples; pitch detection needs {needed}")
            }
        }
    }
}

impl std::error::Error for RenderError {}

/// `WavetableCreator::AudioFileLoadStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioFileLoadStyle {
    None,
    WavetableSplice,
    Vocoded,
    Ttwt,
    Pitched,
}

/// `std::max(a, b)`: `(a < b) ? b : a`.
#[inline]
fn std_max(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

/// `std::min(a, b)`: `(b < a) ? b : a`.
#[inline]
fn std_min(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}

impl WaveSourceKeyframe {
    /// `linearFrequencyInterpolate(wave_frame(), to, t)` called on the
    /// keyframe's own frame, as `FileSourceKeyframe::renderFreqInterpolate`
    /// does: bin 0 and Nyquist are read back after the loop has already
    /// overwritten them.
    fn linear_frequency_interpolate_from_own_frame(&mut self, to: &WaveFrame, t: f32) {
        let frame = &mut self.wave_frame;
        for i in 0..NUM_REAL_COMPLEX {
            let from_bin = frame.frequency_domain[i];
            let amplitude_from = from_bin.abs().sqrt();
            let amplitude_to = to.frequency_domain[i].abs().sqrt();
            let mut amplitude = linear_tween(amplitude_from, amplitude_to, t);
            amplitude *= amplitude;

            let phase_from = from_bin.arg();
            let phase_delta = (from_bin.conj() * to.frequency_domain[i]).arg();
            let mut phase = phase_from + t * phase_delta;
            if amplitude_from == 0.0 {
                phase = to.frequency_domain[i].arg();
            }
            frame.frequency_domain[i] = Complex::polar(amplitude, phase);
        }

        let dc_from = frame.frequency_domain[0].re;
        let dc_to = to.frequency_domain[0].re;
        frame.frequency_domain[0] = Complex::real(linear_tween(dc_from, dc_to, t));

        let last = NUM_REAL_COMPLEX - 1;
        let last_harmonic_from = frame.frequency_domain[last].re;
        let last_harmonic_to = to.frequency_domain[last].re;
        frame.frequency_domain[last] =
            Complex::real(linear_tween(last_harmonic_from, last_harmonic_to, t));

        frame.to_time_domain();
    }
}

impl WaveSource {
    /// `WaveSource::render(wave_frame, position)`.
    pub fn render(&mut self, wave_frame: &mut WaveFrame, position: f32) {
        let frame = self.render_compute_frame(position);
        wave_frame.copy_from(frame);
    }

    /// `ShepardToneSource::render(wave_frame, position)`: crossfades the
    /// first keyframe into a copy with every harmonic moved up an octave.
    pub fn render_shepard(&mut self, wave_frame: &mut WaveFrame, position: f32) {
        let Some(keyframe) = self.keyframes.keyframes.first() else {
            return;
        };

        let key_frame = &keyframe.data.wave_frame;
        let loop_frame = &mut self.loop_frame.wave_frame;
        for i in 0..WAVEFORM_SIZE / 2 {
            loop_frame.frequency_domain[i * 2] = key_frame.frequency_domain[i];
            loop_frame.frequency_domain[i * 2 + 1] = Complex::ZERO;
        }
        loop_frame.to_time_domain();

        self.compute_frame.interpolation_mode = self.interpolation_mode;
        let t = position / (NUM_OSCILLATOR_WAVE_FRAMES as f32 - 1.0);
        self.compute_frame
            .interpolate(&keyframe.data, &self.loop_frame, t);
        wave_frame.copy_from(&self.compute_frame.wave_frame);
    }
}

impl LineSourceKeyframe {
    /// `WaveLineSourceKeyframe::render(wave_frame)`.
    pub fn render(&mut self, wave_frame: &mut WaveFrame) {
        self.line_generator.render();
        let buffer = self.line_generator.buffer();
        for (out, &value) in wave_frame.time_domain[..WAVEFORM_SIZE]
            .iter_mut()
            .zip(buffer)
        {
            *out = value * 2.0 - 1.0;
        }
        wave_frame.to_frequency_domain();
    }
}

impl WaveLineSource {
    /// `WaveLineSource::render(wave_frame, position)`.
    pub fn render(&mut self, wave_frame: &mut WaveFrame, position: f32) {
        self.keyframes
            .interpolate(&mut self.compute_frame, position);
        self.compute_frame.render(wave_frame);
    }
}

/// The parts of a `FileSource` a `FileSourceKeyframe` renders from.
struct FileRenderContext<'a> {
    buffer: &'a [f32],
    size: i32,
    fade_style: i32,
    phase_style: i32,
    overridden_phase: &'a [f32; WAVEFORM_SIZE],
}

impl FileRenderContext<'_> {
    /// `getScaledInterpolatedSample(position)`: Catmull-Rom interpolation
    /// of the padded buffer.
    fn sample(&self, position: f32) -> f32 {
        let clamped_position = scalar::clamp(position, 0.0, (self.size - 1) as f32);
        let start_index = clamped_position as i32;
        let t = clamped_position - start_index as f32;
        scalar::catmull_interpolate(self.buffer, start_index.max(0) as usize, t)
    }
}

/// `(start + i) % kWaveformSize` as an index; upstream's negative remainder
/// would index out of bounds, so it wraps instead.
fn wrap_index(index: i32) -> usize {
    index.rem_euclid(WAVEFORM_SIZE as i32) as usize
}

impl FileSourceKeyframe {
    fn render_wave_blend(&self, ctx: &FileRenderContext, wave_frame: &mut WaveFrame) {
        let size = WAVEFORM_SIZE as f64;
        let window_ratio = self.window_size / size;
        let waveform_middle = (WAVEFORM_SIZE / 2) as f64;
        let mut start_index =
            (self.start_position / window_ratio + self.window_size / 2.0 + waveform_middle) as i32;
        start_index %= WAVEFORM_SIZE as i32;

        for i in 0..WAVEFORM_SIZE {
            let t = i as f64 / (size * 1.0);
            let position = self.start_position + t * self.window_size;
            let write_index = wrap_index(start_index + i as i32);
            wave_frame.time_domain[write_index] = ctx.sample(position as f32);
        }

        let fade_samples = (self.window_fade * size) as i32;
        let fade_size = f64::from(fade_samples) * window_ratio;
        for i in 0..fade_samples {
            let t = f64::from(i as f32 / (fade_samples as f32 - 1.0));
            let fade = 0.5 + 0.5 * crate::platform_libm::cos(f64::from(PI) * t);

            let write_index = wrap_index(start_index + i);
            let position = self.start_position + self.window_size + t * fade_size;
            let existing_value = f64::from(wave_frame.time_domain[write_index]);
            let fade_value = f64::from(ctx.sample(position as f32));
            wave_frame.time_domain[write_index] =
                linear_tween(existing_value as f32, fade_value as f32, fade as f32);
        }
        wave_frame.to_frequency_domain();
    }

    fn render_no_interpolate(&self, ctx: &FileRenderContext, wave_frame: &mut WaveFrame) {
        let cycles_in = self.start_position / self.window_size;
        let cycle = cycles_in as i32;
        let start_index = f64::from(cycle) * self.window_size;

        for i in 0..WAVEFORM_SIZE {
            let t = i as f64 / (WAVEFORM_SIZE as f64 * 1.0);
            let position = start_index + t * self.window_size;
            wave_frame.time_domain[i] = ctx.sample(position as f32);
        }
        wave_frame.to_frequency_domain();
    }

    fn cycles(&self) -> (f64, f64, f32) {
        let cycles_in = self.start_position / self.window_size;
        let from_cycle = cycles_in as i32;
        let to_cycle = from_cycle + 1;
        let transition = (cycles_in - f64::from(from_cycle)) as f32;
        (
            f64::from(from_cycle) * self.window_size,
            f64::from(to_cycle) * self.window_size,
            transition,
        )
    }

    fn render_time_interpolate(&self, ctx: &FileRenderContext, wave_frame: &mut WaveFrame) {
        let (start_index_from, start_index_to, transition) = self.cycles();
        for i in 0..WAVEFORM_SIZE {
            let t = i as f64 / (WAVEFORM_SIZE as f64 * 1.0);
            let from_position = start_index_from + t * self.window_size;
            let to_position = start_index_to + t * self.window_size;
            let from_sample = ctx.sample(from_position as f32);
            let to_sample = ctx.sample(to_position as f32);
            wave_frame.time_domain[i] = scalar::interpolate(from_sample, to_sample, transition);
        }
        wave_frame.to_frequency_domain();
    }

    fn render_freq_interpolate(
        &self,
        ctx: &FileRenderContext,
        wave_frame: &mut WaveFrame,
        from_frame: &mut WaveSourceKeyframe,
        to_frame: &mut WaveSourceKeyframe,
    ) {
        let (start_index_from, start_index_to, transition) = self.cycles();
        for i in 0..WAVEFORM_SIZE {
            let t = i as f64 / (WAVEFORM_SIZE as f64 * 1.0);
            let from_position = start_index_from + t * self.window_size;
            let to_position = start_index_to + t * self.window_size;
            from_frame.wave_frame.time_domain[i] = ctx.sample(from_position as f32);
            to_frame.wave_frame.time_domain[i] = ctx.sample(to_position as f32);
        }

        from_frame.wave_frame.to_frequency_domain();
        to_frame.wave_frame.to_frequency_domain();
        from_frame.linear_frequency_interpolate_from_own_frame(&to_frame.wave_frame, transition);
        wave_frame.copy_from(&from_frame.wave_frame);
    }

    /// `FileSourceKeyframe::render(wave_frame)`.
    fn render(
        &self,
        ctx: &FileRenderContext,
        wave_frame: &mut WaveFrame,
        from_frame: &mut WaveSourceKeyframe,
        to_frame: &mut WaveSourceKeyframe,
    ) {
        if ctx.size <= 0 {
            wave_frame.clear();
            return;
        }

        match ctx.fade_style {
            FADE_WAVE_BLEND => self.render_wave_blend(ctx, wave_frame),
            FADE_NO_INTERPOLATE => self.render_no_interpolate(ctx, wave_frame),
            FADE_TIME_INTERPOLATE => self.render_time_interpolate(ctx, wave_frame),
            FADE_FREQ_INTERPOLATE => {
                self.render_freq_interpolate(ctx, wave_frame, from_frame, to_frame)
            }
            _ => {}
        }

        if ctx.phase_style == PHASE_CLEAR || ctx.phase_style == PHASE_VOCODE {
            for (bin, &phase) in wave_frame
                .frequency_domain
                .iter_mut()
                .zip(ctx.overridden_phase.iter())
            {
                let amplitude = bin.abs();
                *bin = Complex::polar(amplitude, phase);
            }
        }

        wave_frame.to_time_domain();
    }
}

impl FileSource {
    /// `FileSource::render(wave_frame, position)`.
    pub fn render(&mut self, wave_frame: &mut WaveFrame, position: f32) {
        let Some(data) = self.sample_buffer.data.as_ref() else {
            wave_frame.clear();
            return;
        };

        self.keyframes
            .interpolate(&mut self.compute_frame, position);
        self.compute_frame.window_size = self.window_size;
        let ctx = FileRenderContext {
            buffer: data,
            size: self.sample_buffer.size,
            fade_style: self.fade_style,
            phase_style: self.phase_style,
            overridden_phase: &self.overridden_phase,
        };
        self.compute_frame.render(
            &ctx,
            wave_frame,
            &mut self.interpolate_from_frame,
            &mut self.interpolate_to_frame,
        );
        wave_frame.set_frequency_ratio((self.window_size / WAVEFORM_SIZE as f64) as f32);
        wave_frame.set_sample_rate(self.sample_buffer.sample_rate as f32);
        if self.normalize_mult {
            wave_frame.normalize(self.normalize_gain);
        }
        wave_frame.to_frequency_domain();
    }

    /// `setWindowSize(window_size)`.
    pub fn set_window_size(&mut self, window_size: f64) {
        self.window_size = window_size;
    }

    /// `detectPitch(max_period)`: sets the window size to the detected
    /// period, measured on 8096 samples a third of the way in.
    pub fn detect_pitch(&mut self, max_period: i32) -> Result<(), RenderError> {
        let size = self.sample_buffer.size;
        let needed = Self::PITCH_DETECT_MAX_PERIOD;
        let start = (size - needed) / 3;
        let Some(data) = self.data_buffer() else {
            return Err(RenderError::AudioTooShort {
                samples: 0,
                needed: needed as usize,
            });
        };
        if start < 0 {
            return Err(RenderError::AudioTooShort {
                samples: size as usize,
                needed: needed as usize,
            });
        }
        let mut detector = PitchDetector::new();
        detector.load_signal(&data[start as usize..(start + needed) as usize]);
        let period = detector.match_period(max_period);
        self.set_window_size(f64::from(period));
        Ok(())
    }

    /// `detectWaveEditTable()`: a 64 × 256-sample file whose spectrum looks
    /// like 256-sample cycles gets a 256-sample window.
    pub fn detect_wave_edit_table(&mut self) {
        const WAVE_EDIT_FRAME_LENGTH: usize = 256;
        const FREQUENCY_DOMAIN_TOTALS: usize = 8;
        const WAVE_EDIT_NUM_FRAMES: usize = 64;
        if self.sample_buffer.size as usize != WAVE_EDIT_FRAME_LENGTH * WAVE_EDIT_NUM_FRAMES {
            return;
        }
        let Some(buffer) = self.data_buffer() else {
            return;
        };

        let mut wave_frame = WaveFrame::new();
        wave_frame.time_domain[..WAVEFORM_SIZE].copy_from_slice(&buffer[..WAVEFORM_SIZE]);
        wave_frame.to_frequency_domain();

        let size_mult = WAVEFORM_SIZE / WAVE_EDIT_FRAME_LENGTH;
        let mut totals = vec![0.0f32; size_mult];
        for (i, total) in totals.iter_mut().enumerate() {
            for j in 0..FREQUENCY_DOMAIN_TOTALS {
                *total += wave_frame.frequency_domain[i + 1 + j * size_mult].abs();
            }
        }

        if totals[..size_mult - 1]
            .iter()
            .any(|&total| total > totals[size_mult - 1])
        {
            return;
        }
        self.set_window_size(WAVE_EDIT_FRAME_LENGTH as f64);
    }
}

fn multiply_and_mix(value: Complex, mult: Complex, mix: f32) -> Complex {
    let result = value * mult;
    result.scale(mix) + value.scale(1.0 - mix)
}

/// `PhaseModifier::PhaseStyle`.
pub const PHASE_STYLE_NORMAL: i32 = 0;
pub const PHASE_STYLE_EVEN_ODD: i32 = 1;
pub const PHASE_STYLE_HARMONIC: i32 = 2;
pub const PHASE_STYLE_HARMONIC_EVEN_ODD: i32 = 3;
pub const PHASE_STYLE_CLEAR: i32 = 4;

fn render_phase(keyframe: &PhaseModifierKeyframe, phase_style: i32, wave_frame: &mut WaveFrame) {
    let mix = keyframe.mix;
    let phase_shift = Complex::polar(1.0, -keyframe.phase);
    let bins = &mut wave_frame.frequency_domain;
    if phase_style == PHASE_STYLE_HARMONIC {
        for bin in bins.iter_mut() {
            *bin = multiply_and_mix(*bin, phase_shift, mix);
        }
    } else if phase_style == PHASE_STYLE_HARMONIC_EVEN_ODD {
        let odd_shift = Complex::real_div(1.0, phase_shift);
        for i in (0..WAVEFORM_SIZE).step_by(2) {
            bins[i] = multiply_and_mix(bins[i], phase_shift, mix);
            bins[i + 1] = multiply_and_mix(bins[i + 1], odd_shift, mix);
        }
    } else if phase_style == PHASE_STYLE_NORMAL {
        let mut current_phase_shift = Complex::real(1.0);
        for bin in bins.iter_mut() {
            *bin = multiply_and_mix(*bin, current_phase_shift, mix);
            current_phase_shift *= phase_shift;
        }
    } else if phase_style == PHASE_STYLE_EVEN_ODD {
        let mut current_phase_shift = Complex::real(1.0);
        for i in (0..WAVEFORM_SIZE).step_by(2) {
            bins[i] = multiply_and_mix(bins[i], current_phase_shift, mix);
            let odd_shift = Complex::real_div(1.0, current_phase_shift * phase_shift);
            bins[i + 1] = multiply_and_mix(bins[i + 1], odd_shift, mix);
            current_phase_shift *= phase_shift * phase_shift;
        }
    } else if phase_style == PHASE_STYLE_CLEAR {
        for bin in bins.iter_mut() {
            *bin = Complex::real(bin.abs());
        }
    }
    wave_frame.to_time_domain();
}

/// `WaveWindowModifier::WindowShape`.
pub const WINDOW_COS: i32 = 0;
pub const WINDOW_HALF_SIN: i32 = 1;
pub const WINDOW_LINEAR: i32 = 2;
pub const WINDOW_SQUARE: i32 = 3;
pub const WINDOW_WIGGLE: i32 = 4;

/// `WaveWindowModifier::applyWindow(shape, t)`.
pub fn apply_window(window_shape: i32, t: f32) -> f32 {
    match window_shape {
        WINDOW_COS => 0.5 - 0.5 * (PI * t).cos(),
        WINDOW_HALF_SIN => (PI * t / 2.0).sin(),
        WINDOW_SQUARE => {
            if t < 1.0 {
                0.0
            } else {
                1.0
            }
        }
        WINDOW_WIGGLE => t * (PI * (t * 1.5 + 0.5)).cos(),
        _ => t,
    }
}

fn render_window(keyframe: &WaveWindowKeyframe, window_shape: i32, wave_frame: &mut WaveFrame) {
    let last = WAVEFORM_SIZE as f32 - 1.0;
    for i in 0..WAVEFORM_SIZE {
        let t = i as f32 / last;
        if t >= keyframe.left_position {
            break;
        }
        wave_frame.time_domain[i] *= apply_window(window_shape, t / keyframe.left_position);
    }
    for i in (0..=WAVEFORM_SIZE).rev() {
        let t = i as f32 / last;
        if t <= keyframe.right_position {
            break;
        }
        wave_frame.time_domain[i] *=
            apply_window(window_shape, (1.0 - t) / (1.0 - keyframe.right_position));
    }
    wave_frame.to_frequency_domain();
}

/// `FrequencyFilterModifier::FilterStyle`.
pub const FILTER_LOW_PASS: i32 = 0;
pub const FILTER_BAND_PASS: i32 = 1;
pub const FILTER_HIGH_PASS: i32 = 2;
pub const FILTER_COMB: i32 = 3;

/// The `double` `powerScale` local to `frequency_filter_modifier.cpp`.
fn filter_power_scale(value: f64, power: f64) -> f64 {
    const MIN_POWER: f32 = 0.01;
    if power.abs() < f64::from(MIN_POWER) {
        return value;
    }
    let abs_value = value.abs();
    let numerator = crate::platform_libm::exp(power * abs_value) - 1.0;
    let denominator = crate::platform_libm::exp(power) - 1.0;
    if value >= 0.0 {
        numerator / denominator
    } else {
        -numerator / denominator
    }
}

fn comb_wave(t: f32, power: f32) -> f32 {
    let range = t - t.floor();
    (2.0 * filter_power_scale(f64::from(1.0 - (2.0 * range - 1.0).abs()), f64::from(power))) as f32
}

/// `FrequencyFilterModifierKeyframe::getMultiplier(index)`.
fn filter_multiplier(keyframe: &FrequencyFilterKeyframe, style: i32, index: f32) -> f32 {
    const MIN_POWER: f32 = -9.0;
    const MAX_POWER: f32 = 9.0;
    const MAX_SLOPE_REACH: f32 = 128.0;

    let cutoff_index = 2.0f32.powf(keyframe.cutoff);
    let cutoff_delta = index - cutoff_index;
    let slope = 1.0 / scalar::interpolate(1.0, MAX_SLOPE_REACH, keyframe.shape * keyframe.shape);
    let power = scalar::interpolate(MIN_POWER, MAX_POWER, keyframe.shape);

    match style {
        FILTER_LOW_PASS => scalar::clamp(1.0 - slope * cutoff_delta, 0.0, 1.0),
        FILTER_BAND_PASS => scalar::clamp(1.0 - (slope * cutoff_delta).abs(), 0.0, 1.0),
        FILTER_HIGH_PASS => scalar::clamp(1.0 + slope * cutoff_delta, 0.0, 1.0),
        FILTER_COMB => comb_wave(index / (cutoff_index * 2.0), power),
        _ => 0.0,
    }
}

fn render_filter(
    keyframe: &FrequencyFilterKeyframe,
    settings: &FilterSettings,
    wave_frame: &mut WaveFrame,
) {
    for i in 0..NUM_REAL_COMPLEX {
        wave_frame.frequency_domain[i] *= filter_multiplier(keyframe, settings.style, i as f32);
    }
    wave_frame.to_time_domain();
    if settings.normalize {
        wave_frame.normalize(true);
        wave_frame.to_frequency_domain();
    }
}

fn render_slew(keyframe: &SlewLimitKeyframe, wave_frame: &mut WaveFrame) {
    let size = WAVEFORM_SIZE as f32;
    let min_slew_limit = 1.0 / size;
    let max_up_delta = (2.0 / size) / std_max(keyframe.slew_up_run_rise, min_slew_limit);
    let max_down_delta = (2.0 / size) / std_max(keyframe.slew_down_run_rise, min_slew_limit);
    let mut current_value = wave_frame.time_domain[0];
    for i in 1..2 * WAVEFORM_SIZE {
        let index = i % WAVEFORM_SIZE;
        let target_value = wave_frame.time_domain[index];
        let delta = target_value - current_value;
        if delta > 0.0 {
            current_value += std_min(delta, max_up_delta);
        } else {
            current_value -= std_min(-delta, max_down_delta);
        }
        wave_frame.time_domain[index] = current_value;
    }
    wave_frame.to_frequency_domain();
}

fn render_fold(keyframe: &WaveFoldKeyframe, wave_frame: &mut WaveFrame) {
    let max_value = std_max(1.0, wave_frame.max_zero_offset());
    for value in &mut wave_frame.time_domain[..WAVEFORM_SIZE] {
        let clamped = scalar::clamp(*value / max_value, -1.0, 1.0);
        let adjusted_value = max_value * keyframe.wave_fold_boost * clamped.asin();
        *value = adjusted_value.sin();
    }
    wave_frame.to_frequency_domain();
}

/// `highResPowerScale` local to `wave_warp_modifier.cpp`.
fn high_res_power_scale(value: f32, power: f32) -> f64 {
    const MIN_POWER: f32 = 0.01;
    if power.abs() < MIN_POWER {
        return f64::from(value);
    }
    let abs_value = f64::from(value.abs());
    let numerator = crate::platform_libm::exp(f64::from(power) * abs_value) - 1.0;
    // `exp(power) - 1.0f` with a float `power` (A12): libc++ (the macOS arm64
    // reference) resolves the call to `expf`, so the subtraction is in float
    // too, then widened; libstdc++ (x86) resolves it to the double `exp`.
    let denominator = if sas_common::AARCH64_SEMANTICS {
        f64::from(power.exp() - 1.0)
    } else {
        crate::platform_libm::exp(f64::from(power)) - 1.0
    };
    if value >= 0.0 {
        numerator / denominator
    } else {
        -numerator / denominator
    }
}

fn render_warp(keyframe: &WaveWarpKeyframe, settings: &WarpSettings, wave_frame: &mut WaveFrame) {
    for i in 0..WAVEFORM_SIZE {
        wave_frame.frequency_domain[i] = Complex::real(wave_frame.time_domain[i]);
    }

    let last = (WAVEFORM_SIZE - 1) as f32;
    for i in 0..WAVEFORM_SIZE {
        let horizontal = i as f32 / last;
        let warped_horizontal = if settings.horizontal_asymmetric {
            high_res_power_scale(horizontal, keyframe.horizontal_power) as f32
        } else {
            (0.5 * high_res_power_scale(2.0 * horizontal - 1.0, keyframe.horizontal_power) + 0.5)
                as f32
        };

        let float_index = last * warped_horizontal;
        let index = scalar::iclamp(float_index as i32, 0, WAVEFORM_SIZE as i32 - 2) as usize;
        let vertical_from = wave_frame.frequency_domain[index].re;
        let vertical_to = wave_frame.frequency_domain[index + 1].re;
        let mut vertical = linear_tween(vertical_from, vertical_to, float_index - index as f32);
        vertical = scalar::clamp(vertical, -1.0, 1.0);
        wave_frame.time_domain[i] = if settings.vertical_asymmetric {
            (2.0 * high_res_power_scale(0.5 * vertical + 0.5, keyframe.vertical_power) - 1.0) as f32
        } else {
            high_res_power_scale(vertical, keyframe.vertical_power) as f32
        };
    }
    wave_frame.to_frequency_domain();
}

impl<K: KeyframeData, S> Modifier<K, S> {
    fn interpolate_compute(&mut self, position: f32) {
        self.keyframes
            .interpolate(&mut self.compute_frame, position);
    }
}

impl WavetableComponent {
    /// `render(wave_frame, position)`.
    pub fn render(&mut self, wave_frame: &mut WaveFrame, position: f32) {
        match self {
            WavetableComponent::WaveSource(c) => c.render(wave_frame, position),
            WavetableComponent::ShepardToneSource(c) => c.render_shepard(wave_frame, position),
            WavetableComponent::LineSource(c) => c.render(wave_frame, position),
            WavetableComponent::FileSource(c) => c.render(wave_frame, position),
            WavetableComponent::PhaseModifier(c) => {
                c.interpolate_compute(position);
                render_phase(&c.compute_frame, c.settings.phase_style, wave_frame);
            }
            WavetableComponent::WaveWindow(c) => {
                c.interpolate_compute(position);
                render_window(&c.compute_frame, c.settings.window_shape, wave_frame);
            }
            WavetableComponent::FrequencyFilter(c) => {
                c.interpolate_compute(position);
                render_filter(&c.compute_frame, &c.settings, wave_frame);
            }
            WavetableComponent::SlewLimiter(c) => {
                c.interpolate_compute(position);
                render_slew(&c.compute_frame, wave_frame);
            }
            WavetableComponent::WaveFolder(c) => {
                c.interpolate_compute(position);
                render_fold(&c.compute_frame, wave_frame);
            }
            WavetableComponent::WaveWarp(c) => {
                c.interpolate_compute(position);
                render_warp(&c.compute_frame, &c.settings, wave_frame);
            }
        }
    }
}

impl WavetableGroup {
    /// `render(wave_frame, position)`: runs every component in order.
    pub fn render(&mut self, wave_frame: &mut WaveFrame, position: f32) {
        wave_frame.index = position as i32;
        for component in &mut self.components {
            component.render(wave_frame, position);
        }
    }
}

impl WavetableCreator {
    /// `render(position)`: renders and stores one frame, returning its
    /// peak-to-peak span.
    pub fn render_frame(&mut self, wavetable: &mut Wavetable, position: i32) -> f32 {
        self.compute_frame_combine.clear();
        self.compute_frame_combine.index = position;
        self.compute_frame.index = position;

        for group in &mut self.groups {
            group.render(&mut self.compute_frame, position as f32);
            self.compute_frame_combine.add_from(&self.compute_frame);
        }

        if self.groups.len() > 1 {
            self.compute_frame_combine
                .multiply(1.0 / self.groups.len() as f32);
        }

        if self.remove_all_dc {
            self.compute_frame_combine.removed_dc();
        }

        let mut max_value = 0.0f32;
        let mut min_value = 0.0f32;
        for &value in &self.compute_frame_combine.time_domain[..WAVEFORM_SIZE] {
            max_value = std_max(value, max_value);
            min_value = std_min(value, min_value);
        }

        wavetable.load_wave_frame(&self.compute_frame_combine);
        max_value - min_value
    }

    /// `render()`: renders every frame up to the last keyframe into
    /// `wavetable`, then normalises it (`postRender`). Also copies the name
    /// and author, which upstream keeps on the wavetable.
    pub fn render(&mut self, wavetable: &mut Wavetable) -> Result<(), RenderError> {
        let mut last_waveframe = 0;
        let mut shepard = !self.groups.is_empty();
        for group in &self.groups {
            last_waveframe = last_waveframe.max(group.last_keyframe_position());
            shepard = shepard && group.is_shepard_tone();
        }

        let max_frames = NUM_OSCILLATOR_WAVE_FRAMES as usize;
        if !(0..max_frames as i32).contains(&last_waveframe) {
            return Err(RenderError::TooManyFrames {
                frames: last_waveframe + 1,
                max: max_frames,
            });
        }

        wavetable.name = self.name.clone();
        wavetable.author = self.author.clone();
        wavetable.set_num_frames(last_waveframe as usize + 1);
        wavetable.set_shepard_table(shepard);
        let mut max_span = 0.0f32;
        for i in 0..=last_waveframe {
            max_span = std_max(self.render_frame(wavetable, i), max_span);
        }
        wavetable.set_frequency_ratio(self.compute_frame.frequency_ratio);
        wavetable.set_sample_rate(self.compute_frame.sample_rate);

        self.post_render(wavetable, max_span);
        Ok(())
    }

    /// `postRender(max_span)`.
    pub fn post_render(&self, wavetable: &mut Wavetable, max_span: f32) {
        if self.full_normalize {
            wavetable.post_process(max_span);
        } else {
            wavetable.post_process(0.0);
        }
    }

    /// `renderToBuffer(buffer, num_frames, frame_size)`: `num_frames` frames
    /// spread over the table, peak-normalised (only downwards) together.
    /// Only `frame_size == kWaveformSize` is supported, as upstream.
    pub fn render_to_buffer(&mut self, num_frames: usize) -> Vec<f32> {
        let mut buffer = vec![0.0f32; num_frames * WAVEFORM_SIZE];
        for i in 0..num_frames {
            let position = (1.0 * i as f32 * NUM_OSCILLATOR_WAVE_FRAMES as f32) / num_frames as f32;
            self.compute_frame_combine.clear();
            self.compute_frame_combine.index = position as i32;
            self.compute_frame.index = position as i32;

            for group in &mut self.groups {
                group.render(&mut self.compute_frame, position);
                self.compute_frame_combine.add_from(&self.compute_frame);
            }

            buffer[i * WAVEFORM_SIZE..(i + 1) * WAVEFORM_SIZE]
                .copy_from_slice(&self.compute_frame_combine.time_domain[..WAVEFORM_SIZE]);
        }

        let mut max_value = 1.0f32;
        for &value in &buffer {
            max_value = std_max(max_value, value.abs());
        }
        let scale = 1.0 / max_value;
        for value in &mut buffer {
            *value *= scale;
        }
        buffer
    }

    /// The state part of `initFromAudioFile(...)`: builds an Audio File
    /// Source the way dropping a file in the editor does. Call
    /// [`Self::render`] afterwards.
    pub fn init_from_audio_file(
        &mut self,
        audio: &[f32],
        sample_rate: i32,
        load_style: AudioFileLoadStyle,
        fade_style: i32,
    ) -> Result<(), RenderError> {
        let beginning_sample = audio.iter().position(|&s| s != 0.0).unwrap_or(0);
        let shortened = &audio[beginning_sample..];
        match load_style {
            AudioFileLoadStyle::Vocoded => {
                self.init_from_vocoded_audio(shortened, sample_rate, false)
            }
            AudioFileLoadStyle::Ttwt => self.init_from_vocoded_audio(shortened, sample_rate, true),
            AudioFileLoadStyle::Pitched => self.init_from_pitched_audio(shortened, sample_rate),
            AudioFileLoadStyle::None | AudioFileLoadStyle::WavetableSplice => {
                self.init_from_spliced_audio(audio, sample_rate, fade_style);
                Ok(())
            }
        }
    }

    fn add_file_source(&mut self, file_source: FileSource) {
        let mut group = WavetableGroup::new();
        group.add_component(WavetableComponent::FileSource(Box::new(file_source)));
        self.add_group(group);
    }

    fn init_from_spliced_audio(&mut self, audio: &[f32], sample_rate: i32, fade_style: i32) {
        self.clear();
        let num_samples = audio.len() as i32;

        let mut file_source = FileSource::new();
        file_source.load_buffer(audio, sample_rate);
        file_source.fade_style = fade_style;
        file_source.set_phase_style(PHASE_NONE);
        file_source.insert_new_keyframe(0);
        file_source.detect_wave_edit_table();

        let window_size = file_source.window_size;
        if fade_style == FADE_NO_INTERPOLATE {
            let num_cycles = 1.max((f64::from(num_samples) / window_size) as i32);
            let buffer_frames = NUM_OSCILLATOR_WAVE_FRAMES / num_cycles;
            file_source.insert_new_keyframe(0.max(NUM_OSCILLATOR_WAVE_FRAMES - 1 - buffer_frames));
        } else {
            file_source.insert_new_keyframe(NUM_OSCILLATOR_WAVE_FRAMES - 1);
        }

        file_source.keyframes.keyframes[0].data.start_position = 0.0;
        let mut last_sample_position = (f64::from(num_samples) - window_size) as i32;
        let keyframe_end =
            (f64::from(file_source.keyframes.keyframes[1].position) * window_size) as i32;
        last_sample_position = keyframe_end.min(last_sample_position);
        file_source.keyframes.keyframes[1].data.start_position =
            f64::from(0.max(last_sample_position));

        self.add_file_source(file_source);
    }

    fn init_from_vocoded_audio(
        &mut self,
        audio: &[f32],
        sample_rate: i32,
        ttwt: bool,
    ) -> Result<(), RenderError> {
        const MAX_TTWT_PERIOD: f32 = 0.02;
        self.clear();

        let mut file_source = FileSource::new();
        file_source.load_buffer(audio, sample_rate);
        if ttwt {
            file_source.detect_pitch((MAX_TTWT_PERIOD * sample_rate as f32) as i32)?;
        } else {
            file_source.detect_pitch(WAVEFORM_SIZE as i32)?;
        }

        file_source.fade_style = FADE_WAVE_BLEND;
        file_source.set_phase_style(PHASE_VOCODE);
        self.finish_pitched_source(file_source, audio.len());
        Ok(())
    }

    fn init_from_pitched_audio(
        &mut self,
        audio: &[f32],
        sample_rate: i32,
    ) -> Result<(), RenderError> {
        self.clear();

        let mut file_source = FileSource::new();
        file_source.load_buffer(audio, sample_rate);
        file_source.detect_pitch(WAVEFORM_SIZE as i32)?;
        file_source.fade_style = FADE_WAVE_BLEND;
        self.finish_pitched_source(file_source, audio.len());
        Ok(())
    }

    fn finish_pitched_source(&mut self, mut file_source: FileSource, num_samples: usize) {
        file_source.insert_new_keyframe(0);
        file_source.insert_new_keyframe(NUM_OSCILLATOR_WAVE_FRAMES - 1);
        file_source.keyframes.keyframes[0].data.start_position = 0.0;
        let samples_needed = file_source.keyframes.keyframes[1].data.samples_needed();
        file_source.keyframes.keyframes[1].data.start_position =
            f64::from(num_samples as i32 - samples_needed);
        self.add_file_source(file_source);
    }
}
