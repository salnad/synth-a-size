//! Keyframe types of every wavetable source and modifier: their state,
//! `copy`/`interpolate`/`smoothInterpolate` and JSON.
//!
//! Ports the keyframe classes in `vital/src/common/wavetable/*.{h,cpp}`
//! (Copyright 2013-2019 Matt Tytel, GPL-3.0-or-later):
//! `wavetable_keyframe`, `wave_source`, `wave_line_source`, `file_source`,
//! `phase_modifier`, `wave_window_modifier`, `frequency_filter_modifier`,
//! `slew_limit_modifier`, `wave_fold_modifier` and `wave_warp_modifier`.
//!
//! Each keyframe's `copy` copies only the fields upstream's `copy` does;
//! settings that a component pushes into its keyframes before rendering
//! (styles, modes, flags) live on the component instead.

use sas_common::json::{JsonError, Result, Value};
use sas_common::{base64, scalar, LineGenerator};

use crate::complex::Complex;
use crate::wave_frame::{WaveFrame, NUM_REAL_COMPLEX, WAVEFORM_SIZE};

/// `WavetableKeyframe::linearTween`.
#[inline]
pub fn linear_tween(point_from: f32, point_to: f32, t: f32) -> f32 {
    scalar::interpolate(point_from, point_to, t)
}

/// `WavetableKeyframe::cubicTween`.
#[allow(clippy::too_many_arguments)]
#[inline]
pub fn cubic_tween(
    point_prev: f32,
    point_from: f32,
    point_to: f32,
    point_next: f32,
    range_prev: f32,
    range: f32,
    range_next: f32,
    t: f32,
) -> f32 {
    let mut slope_from = 0.0f32;
    let mut slope_to = 0.0f32;
    if range_prev > 0.0 {
        slope_from = (point_to - point_prev) / (1.0 + range_prev / range);
    }
    if range_next > 0.0 {
        slope_to = (point_next - point_from) / (1.0 + range_next / range);
    }
    let delta = point_to - point_from;

    let movement = linear_tween(point_from, point_to, t);
    let smooth = t * (1.0 - t) * ((1.0 - t) * (slope_from - delta) + t * (delta - slope_to));
    movement + smooth
}

/// Distances between the four keyframes of a cubic interpolation
/// (`from - prev`, `to - from`, `next - to`), as floats.
#[derive(Clone, Copy, Debug)]
pub struct CubicRanges {
    pub range_prev: f32,
    pub range: f32,
    pub range_next: f32,
}

/// The per-type behaviour shared by all keyframes.
pub trait KeyframeData: Clone + Default {
    /// `copy(keyframe)`.
    fn copy_from(&mut self, other: &Self);
    /// `interpolate(from, to, t)`.
    fn interpolate(&mut self, from: &Self, to: &Self, t: f32);
    /// `smoothInterpolate(prev, from, to, next, t)`. Upstream leaves the
    /// destination untouched for every type except wave sources.
    fn smooth_interpolate(
        &mut self,
        _prev: &Self,
        _from: &Self,
        _to: &Self,
        _next: &Self,
        _ranges: CubicRanges,
        _t: f32,
    ) {
    }
    /// The type-specific fields of `stateToJson()`.
    fn write_json(&self, data: &mut Value);
    /// The type-specific part of `jsonToState(data)`.
    fn read_json(&mut self, data: &Value) -> Result<()>;
}

/// `WaveSource::InterpolationMode`.
pub const WAVE_SOURCE_TIME: i32 = 0;
pub const WAVE_SOURCE_FREQUENCY: i32 = 1;

/// `WaveSourceKeyframe` (also used by the Shepard tone source).
#[derive(Clone, Debug)]
pub struct WaveSourceKeyframe {
    pub wave_frame: Box<WaveFrame>,
    /// Set by the owning component before interpolating.
    pub interpolation_mode: i32,
}

impl Default for WaveSourceKeyframe {
    fn default() -> Self {
        Self {
            wave_frame: Box::default(),
            interpolation_mode: WAVE_SOURCE_FREQUENCY,
        }
    }
}

impl WaveSourceKeyframe {
    /// `linearTimeInterpolate(from, to, t)`.
    pub fn linear_time_interpolate(&mut self, from: &WaveFrame, to: &WaveFrame, t: f32) {
        for i in 0..WAVEFORM_SIZE {
            self.wave_frame.time_domain[i] =
                linear_tween(from.time_domain[i], to.time_domain[i], t);
        }
        self.wave_frame.to_frequency_domain();
    }

    /// `cubicTimeInterpolate(prev, from, to, next, ...)`.
    pub fn cubic_time_interpolate(
        &mut self,
        prev: &WaveFrame,
        from: &WaveFrame,
        to: &WaveFrame,
        next: &WaveFrame,
        ranges: CubicRanges,
        t: f32,
    ) {
        for i in 0..WAVEFORM_SIZE {
            self.wave_frame.time_domain[i] = cubic_tween(
                prev.time_domain[i],
                from.time_domain[i],
                to.time_domain[i],
                next.time_domain[i],
                ranges.range_prev,
                ranges.range,
                ranges.range_next,
                t,
            );
        }
        self.wave_frame.to_frequency_domain();
    }

    /// `linearFrequencyInterpolate(from, to, t)`: interpolates square-root
    /// amplitudes and unwrapped phases bin by bin.
    pub fn linear_frequency_interpolate(&mut self, from: &WaveFrame, to: &WaveFrame, t: f32) {
        for i in 0..NUM_REAL_COMPLEX {
            let amplitude_from = from.frequency_domain[i].abs().sqrt();
            let amplitude_to = to.frequency_domain[i].abs().sqrt();
            let mut amplitude = linear_tween(amplitude_from, amplitude_to, t);
            amplitude *= amplitude;

            let phase_from = from.frequency_domain[i].arg();
            let phase_delta = (from.frequency_domain[i].conj() * to.frequency_domain[i]).arg();
            let mut phase = phase_from + t * phase_delta;
            if amplitude_from == 0.0 {
                phase = to.frequency_domain[i].arg();
            }
            self.wave_frame.frequency_domain[i] = Complex::polar(amplitude, phase);
        }

        let dc_from = from.frequency_domain[0].re;
        let dc_to = to.frequency_domain[0].re;
        self.wave_frame.frequency_domain[0] = Complex::real(linear_tween(dc_from, dc_to, t));

        let last = NUM_REAL_COMPLEX - 1;
        let last_harmonic_from = from.frequency_domain[last].re;
        let last_harmonic_to = to.frequency_domain[last].re;
        self.wave_frame.frequency_domain[last] =
            Complex::real(linear_tween(last_harmonic_from, last_harmonic_to, t));

        self.wave_frame.to_time_domain();
    }

    /// `cubicFrequencyInterpolate(prev, from, to, next, ...)`.
    pub fn cubic_frequency_interpolate(
        &mut self,
        prev: &WaveFrame,
        from: &WaveFrame,
        to: &WaveFrame,
        next: &WaveFrame,
        ranges: CubicRanges,
        t: f32,
    ) {
        let CubicRanges {
            range_prev,
            range,
            range_next,
        } = ranges;
        for i in 0..NUM_REAL_COMPLEX {
            let amplitude_prev = prev.frequency_domain[i].abs().sqrt();
            let amplitude_from = from.frequency_domain[i].abs().sqrt();
            let amplitude_to = to.frequency_domain[i].abs().sqrt();
            let amplitude_next = next.frequency_domain[i].abs().sqrt();
            let mut amplitude = cubic_tween(
                amplitude_prev,
                amplitude_from,
                amplitude_to,
                amplitude_next,
                range_prev,
                range,
                range_next,
                t,
            );
            amplitude *= amplitude;

            let phase_delta_from =
                (prev.frequency_domain[i].conj() * from.frequency_domain[i]).arg();
            let phase_delta_to = (from.frequency_domain[i].conj() * to.frequency_domain[i]).arg();
            let phase_delta_next = (to.frequency_domain[i].conj() * next.frequency_domain[i]).arg();
            let phase_prev = prev.frequency_domain[i].arg();
            let mut phase_from = phase_prev;
            if amplitude_from != 0.0 {
                phase_from += phase_delta_from;
            }
            let mut phase_to = phase_from;
            if amplitude_to != 0.0 {
                phase_to += phase_delta_to;
            }
            let mut phase_next = phase_to;
            if amplitude_next != 0.0 {
                phase_next += phase_delta_next;
            }

            let phase = cubic_tween(
                phase_prev, phase_from, phase_to, phase_next, range_prev, range, range_next, t,
            );
            self.wave_frame.frequency_domain[i] = Complex::polar(amplitude, phase);
        }

        let dc = cubic_tween(
            prev.frequency_domain[0].re,
            from.frequency_domain[0].re,
            to.frequency_domain[0].re,
            next.frequency_domain[0].re,
            range_prev,
            range,
            range_next,
            t,
        );
        self.wave_frame.frequency_domain[0] = Complex::real(dc);

        let last = NUM_REAL_COMPLEX - 1;
        let last_harmonic = cubic_tween(
            prev.frequency_domain[last].re,
            from.frequency_domain[last].re,
            to.frequency_domain[last].re,
            next.frequency_domain[last].re,
            range_prev,
            range,
            range_next,
            t,
        );
        self.wave_frame.frequency_domain[last] = Complex::real(last_harmonic);
        self.wave_frame.to_time_domain();
    }
}

impl KeyframeData for WaveSourceKeyframe {
    fn copy_from(&mut self, other: &Self) {
        self.wave_frame.copy_from(&other.wave_frame);
    }

    fn interpolate(&mut self, from: &Self, to: &Self, t: f32) {
        if self.interpolation_mode == WAVE_SOURCE_FREQUENCY {
            self.linear_frequency_interpolate(&from.wave_frame, &to.wave_frame, t);
        } else {
            self.linear_time_interpolate(&from.wave_frame, &to.wave_frame, t);
        }
    }

    fn smooth_interpolate(
        &mut self,
        prev: &Self,
        from: &Self,
        to: &Self,
        next: &Self,
        ranges: CubicRanges,
        t: f32,
    ) {
        if self.interpolation_mode == WAVE_SOURCE_FREQUENCY {
            self.cubic_frequency_interpolate(
                &prev.wave_frame,
                &from.wave_frame,
                &to.wave_frame,
                &next.wave_frame,
                ranges,
                t,
            );
        } else {
            self.cubic_time_interpolate(
                &prev.wave_frame,
                &from.wave_frame,
                &to.wave_frame,
                &next.wave_frame,
                ranges,
                t,
            );
        }
    }

    fn write_json(&self, data: &mut Value) {
        data.set("wave_data", base64::encode_f32s(self.wave_frame.samples()));
    }

    /// Decodes `wave_data` into the time domain. Upstream copies 2048 floats
    /// regardless of the decoded length (reading uninitialised memory when
    /// it is short); a short buffer is zero-filled here.
    fn read_json(&mut self, data: &Value) -> Result<()> {
        let encoded = data.get("wave_data")?.to_str()?;
        let samples = base64::decode_f32s(encoded);
        for i in 0..WAVEFORM_SIZE {
            self.wave_frame.time_domain[i] = samples.get(i).copied().unwrap_or(0.0);
        }
        self.wave_frame.to_frequency_domain();
        Ok(())
    }
}

/// `WaveLineSource::WaveLineSourceKeyframe`.
#[derive(Clone, Debug)]
pub struct LineSourceKeyframe {
    pub line_generator: LineGenerator,
    pub pull_power: f32,
}

impl Default for LineSourceKeyframe {
    fn default() -> Self {
        Self {
            line_generator: LineGenerator::new(WAVEFORM_SIZE),
            pull_power: 0.0,
        }
    }
}

impl KeyframeData for LineSourceKeyframe {
    fn copy_from(&mut self, other: &Self) {
        let source = &other.line_generator;
        self.line_generator.set_num_points(source.num_points());
        self.line_generator.set_smooth(source.smooth());
        for i in 0..self.line_generator.num_points() {
            self.line_generator.set_point(i, source.point(i));
            self.line_generator.set_power(i, source.power(i));
        }
    }

    fn interpolate(&mut self, from: &Self, to: &Self, t: f32) {
        let relative_power = from.pull_power - to.pull_power;
        let adjusted_t = scalar::power_scale(t, relative_power);

        let from_generator = &from.line_generator;
        let to_generator = &to.line_generator;
        let num_points = from_generator.num_points();
        self.line_generator.set_num_points(num_points);
        self.line_generator.set_smooth(from_generator.smooth());

        for i in 0..num_points {
            let from_point = from_generator.point(i);
            let to_point = to_generator.point(i);
            self.line_generator.set_point(
                i,
                (
                    linear_tween(from_point.0, to_point.0, adjusted_t),
                    linear_tween(from_point.1, to_point.1, adjusted_t),
                ),
            );
            self.line_generator.set_power(
                i,
                linear_tween(from_generator.power(i), to_generator.power(i), adjusted_t),
            );
        }
    }

    fn write_json(&self, data: &mut Value) {
        data.set("pull_power", self.pull_power);
        data.set("line", self.line_generator.to_json());
    }

    fn read_json(&mut self, data: &Value) -> Result<()> {
        self.pull_power = 0.0;
        if data.count("pull_power") != 0 {
            self.pull_power = data.get("pull_power")?.to_f32()?;
        }
        if data.count("line") != 0 {
            self.line_generator.from_json(data.get("line")?)?;
        }
        Ok(())
    }
}

/// `FileSource::FileSourceKeyframe`: where in the audio a frame is read.
#[derive(Clone, Debug)]
pub struct FileSourceKeyframe {
    pub start_position: f64,
    pub window_fade: f64,
    pub window_size: f64,
}

impl Default for FileSourceKeyframe {
    fn default() -> Self {
        Self {
            start_position: 0.0,
            window_fade: 1.0,
            window_size: WAVEFORM_SIZE as f64,
        }
    }
}

impl FileSourceKeyframe {
    /// `getWindowFadeSamples()`.
    pub fn window_fade_samples(&self) -> f64 {
        self.window_fade * self.window_size
    }

    /// `getSamplesNeeded()`.
    pub fn samples_needed(&self) -> i32 {
        (self.window_size + self.window_fade_samples()) as i32
    }
}

impl KeyframeData for FileSourceKeyframe {
    fn copy_from(&mut self, other: &Self) {
        self.start_position = other.start_position;
        self.window_fade = other.window_fade;
    }

    /// Upstream tweens the doubles through the `float` `linearTween`.
    fn interpolate(&mut self, from: &Self, to: &Self, t: f32) {
        self.start_position = f64::from(linear_tween(
            from.start_position as f32,
            to.start_position as f32,
            t,
        ));
        self.window_fade = f64::from(linear_tween(
            from.window_fade as f32,
            to.window_fade as f32,
            t,
        ));
    }

    fn write_json(&self, data: &mut Value) {
        data.set("start_position", self.start_position);
        data.set("window_fade", self.window_fade);
        data.set("window_size", self.window_size);
    }

    fn read_json(&mut self, data: &Value) -> Result<()> {
        self.start_position = data.get("start_position")?.to_f64()?;
        self.window_fade = data.get("window_fade")?.to_f64()?;
        self.window_size = data.get("window_size")?.to_f64()?;
        Ok(())
    }
}

macro_rules! float_keyframe {
    (
        $(#[$meta:meta])*
        $name:ident { $($field:ident : $json:literal = $default:expr),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name {
            $(pub $field: f32,)+
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $($field: $default,)+ }
            }
        }

        impl KeyframeData for $name {
            fn copy_from(&mut self, other: &Self) {
                $(self.$field = other.$field;)+
            }

            fn interpolate(&mut self, from: &Self, to: &Self, t: f32) {
                $(self.$field = linear_tween(from.$field, to.$field, t);)+
            }

            fn write_json(&self, data: &mut Value) {
                $(data.set($json, self.$field);)+
            }

            fn read_json(&mut self, data: &Value) -> Result<()> {
                $(self.$field = data.get($json)?.to_f32()?;)+
                Ok(())
            }
        }
    };
}

float_keyframe! {
    /// `PhaseModifier::PhaseModifierKeyframe`.
    PhaseModifierKeyframe { phase: "phase" = 0.0, mix: "mix" = 1.0 }
}

float_keyframe! {
    /// `WaveWindowModifier::WaveWindowModifierKeyframe`.
    WaveWindowKeyframe { left_position: "left_position" = 0.25, right_position: "right_position" = 1.0 - 0.25 }
}

float_keyframe! {
    /// `FrequencyFilterModifier::FrequencyFilterModifierKeyframe`.
    FrequencyFilterKeyframe { cutoff: "cutoff" = 4.0, shape: "shape" = 0.5 }
}

float_keyframe! {
    /// `SlewLimitModifier::SlewLimitModifierKeyframe`.
    SlewLimitKeyframe { slew_up_run_rise: "up_run_rise" = 0.0, slew_down_run_rise: "down_run_rise" = 0.0 }
}

float_keyframe! {
    /// `WaveFoldModifier::WaveFoldModifierKeyframe`.
    WaveFoldKeyframe { wave_fold_boost: "fold_boost" = 1.0 }
}

float_keyframe! {
    /// `WaveWarpModifier::WaveWarpModifierKeyframe`.
    WaveWarpKeyframe { horizontal_power: "horizontal_power" = 0.0, vertical_power: "vertical_power" = 0.0 }
}

/// A keyframe: its position (0..=256 in the wavetable) and its data.
#[derive(Clone, Debug, Default)]
pub struct Keyframe<K> {
    pub position: i32,
    pub data: K,
}

impl<K: KeyframeData> Keyframe<K> {
    /// `stateToJson()`.
    pub fn to_json(&self) -> Value {
        let mut data = Value::object();
        data.set("position", self.position);
        self.data.write_json(&mut data);
        data
    }

    /// `jsonToState(data)`.
    pub fn read_json(&mut self, data: &Value) -> Result<()> {
        self.position = data.get("position")?.to_i32()?;
        self.data.read_json(data)
    }
}

pub(crate) fn require_object(value: &Value, what: &str) -> Result<()> {
    if value.is_object() || value.is_null() {
        Ok(())
    } else {
        Err(JsonError(format!(
            "[json.exception.type_error.305] cannot use operator[] with {} ({what})",
            value.type_name()
        )))
    }
}
