// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Wavetable components (sources and modifiers): their keyframe lists,
//! component-level settings and JSON.
//!
//! Ports `wavetable_component.{h,cpp}`, `wavetable_component_factory.{h,cpp}`
//! and the component classes in `vital/src/common/wavetable/` (Copyright
//! 2013-2019 Matt Tytel, GPL-3.0-or-later).

use sas_common::json::{JsonError, Result, Value};
use sas_common::random::{canonical_from_draw, RandomGenerator};
use sas_common::{base64, scalar};

use super::keyframes::*;
use crate::wave_frame::{WaveFrame, WAVEFORM_SIZE};
use crate::NUM_OSCILLATOR_WAVE_FRAMES;

/// `vital::kDefaultSampleRate`.
const DEFAULT_SAMPLE_RATE: i32 = 44100;

/// `WavetableComponent::InterpolationStyle`.
pub const INTERPOLATION_NONE: i32 = 0;
pub const INTERPOLATION_LINEAR: i32 = 1;
pub const INTERPOLATION_CUBIC: i32 = 2;

/// `WavetableComponentFactory::ComponentType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentType {
    WaveSource = 0,
    LineSource = 1,
    FileSource = 2,
    /// Deprecated upstream but still loadable.
    ShepardToneSource = 3,
    PhaseModifier = 4,
    WaveWindow = 5,
    FrequencyFilter = 6,
    SlewLimiter = 7,
    WaveFolder = 8,
    WaveWarp = 9,
}

impl ComponentType {
    /// `getComponentName(type)`: the `type` string saved in JSON.
    pub fn name(self) -> &'static str {
        match self {
            ComponentType::WaveSource => "Wave Source",
            ComponentType::LineSource => "Line Source",
            ComponentType::FileSource => "Audio File Source",
            ComponentType::ShepardToneSource => "Shepard Tone Source",
            ComponentType::PhaseModifier => "Phase Shift",
            ComponentType::WaveWindow => "Wave Window",
            ComponentType::FrequencyFilter => "Frequency Filter",
            ComponentType::SlewLimiter => "Slew Limiter",
            ComponentType::WaveFolder => "Wave Folder",
            ComponentType::WaveWarp => "Wave Warp",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Wave Source" => ComponentType::WaveSource,
            "Line Source" => ComponentType::LineSource,
            "Audio File Source" => ComponentType::FileSource,
            "Shepard Tone Source" => ComponentType::ShepardToneSource,
            "Phase Shift" => ComponentType::PhaseModifier,
            "Wave Window" => ComponentType::WaveWindow,
            "Frequency Filter" => ComponentType::FrequencyFilter,
            "Slew Limiter" => ComponentType::SlewLimiter,
            "Wave Folder" => ComponentType::WaveFolder,
            "Wave Warp" => ComponentType::WaveWarp,
            _ => return None,
        })
    }
}

/// `WavetableComponent`'s keyframe list and interpolation style.
#[derive(Clone, Debug)]
pub struct Keyframes<K> {
    pub keyframes: Vec<Keyframe<K>>,
    /// `InterpolationStyle` as stored (`INTERPOLATION_*`); other values are
    /// kept and, like upstream, interpolate nothing.
    pub interpolation_style: i32,
}

impl<K> Default for Keyframes<K> {
    fn default() -> Self {
        Self {
            keyframes: Vec::new(),
            interpolation_style: INTERPOLATION_LINEAR,
        }
    }
}

impl<K: KeyframeData> Keyframes<K> {
    pub fn num_frames(&self) -> usize {
        self.keyframes.len()
    }

    /// `getIndexFromPosition(position)`: the number of keyframes at or
    /// before `position`.
    pub fn index_from_position(&self, position: i32) -> usize {
        self.keyframes
            .iter()
            .take_while(|k| position >= k.position)
            .count()
    }

    /// `getLastKeyframePosition()` (for components with keyframes).
    pub fn last_keyframe_position(&self) -> i32 {
        self.keyframes.last().map_or(0, |k| k.position)
    }

    /// `interpolate(dest, position)`.
    pub fn interpolate(&self, dest: &mut K, position: f32) {
        let n = self.keyframes.len() as i32;
        if n == 0 {
            return;
        }

        let index = self.index_from_position(position as i32) as i32 - 1;
        let clamped_index = index.max(0).min(n - 1);
        let from_frame = &self.keyframes[clamped_index as usize];

        if index < 0 || index >= n - 1 || self.interpolation_style == INTERPOLATION_NONE {
            dest.copy_from(&from_frame.data);
        } else if self.interpolation_style == INTERPOLATION_LINEAR {
            let to_frame = &self.keyframes[index as usize + 1];
            let t = (position - from_frame.position as f32)
                / (to_frame.position - from_frame.position) as f32;
            dest.interpolate(&from_frame.data, &to_frame.data, t);
        } else if self.interpolation_style == INTERPOLATION_CUBIC {
            let mut next_index = index + 2;
            let mut prev_index = index - 1;
            if next_index >= n {
                next_index = index;
            }
            if prev_index < 0 {
                prev_index = index + 1;
            }

            let to_frame = &self.keyframes[index as usize + 1];
            let next_frame = &self.keyframes[next_index as usize];
            let prev_frame = &self.keyframes[prev_index as usize];

            let t = (position - from_frame.position as f32)
                / (to_frame.position - from_frame.position) as f32;
            let ranges = CubicRanges {
                range_prev: (from_frame.position - prev_frame.position) as f32,
                range: (to_frame.position - from_frame.position) as f32,
                range_next: (next_frame.position - to_frame.position) as f32,
            };
            dest.smooth_interpolate(
                &prev_frame.data,
                &from_frame.data,
                &to_frame.data,
                &next_frame.data,
                ranges,
                t,
            );
        }
    }

    /// Inserts `keyframe` after any keyframes at the same position and
    /// returns its index (`insertNewKeyframe` once the keyframe exists).
    pub fn insert(&mut self, keyframe: Keyframe<K>) -> usize {
        let index = self.index_from_position(keyframe.position);
        self.keyframes.insert(index, keyframe);
        index
    }

    /// `remove(keyframe)`.
    pub fn remove(&mut self, index: usize) -> Keyframe<K> {
        self.keyframes.remove(index)
    }

    /// `reposition(keyframe)` after changing `keyframes[index].position`;
    /// returns the new index.
    pub fn reposition(&mut self, index: usize) -> usize {
        let keyframe = self.keyframes.remove(index);
        self.insert(keyframe)
    }

    fn write_json(&self, data: &mut Value, component_type: ComponentType) {
        let mut keyframes_data = Value::Null;
        for keyframe in &self.keyframes {
            keyframes_data.push(keyframe.to_json());
        }
        data.set("keyframes", keyframes_data);
        data.set("type", component_type.name());
        data.set("interpolation_style", self.interpolation_style);
    }
}

/// Creates a keyframe the way each component's `createKeyframe(position)`
/// does: a default keyframe interpolated from the existing ones.
fn create_interpolated<K: KeyframeData>(keyframes: &Keyframes<K>, position: i32) -> Keyframe<K> {
    let mut keyframe = Keyframe {
        position,
        data: K::default(),
    };
    keyframes.interpolate(&mut keyframe.data, position as f32);
    keyframe
}

/// `WavetableComponent::jsonToState` for the keyframe list: each keyframe
/// is created at its position (interpolated from those already loaded,
/// like `insertNewKeyframe`), then its JSON is read.
fn read_keyframes<K: KeyframeData>(keyframes: &mut Keyframes<K>, data: &Value) -> Result<()> {
    keyframes.keyframes.clear();
    for json_keyframe in data.get("keyframes")?.iter_values() {
        require_object(json_keyframe, "keyframe")?;
        let position = json_keyframe.get("position")?.to_i32()?;
        let keyframe = create_interpolated(keyframes, position);
        let index = keyframes.insert(keyframe);
        keyframes.keyframes[index].read_json(json_keyframe)?;
    }
    read_interpolation_style(keyframes, data)
}

fn read_interpolation_style<K>(keyframes: &mut Keyframes<K>, data: &Value) -> Result<()> {
    if data.count("interpolation_style") != 0 {
        keyframes.interpolation_style = data.get("interpolation_style")?.to_enum()?;
    }
    Ok(())
}

/// `WaveSource` (and `ShepardToneSource`, which saves the same state).
#[derive(Clone, Debug, Default)]
pub struct WaveSource {
    pub keyframes: Keyframes<WaveSourceKeyframe>,
    /// `WaveSource::InterpolationMode` (`WAVE_SOURCE_TIME`/`WAVE_SOURCE_FREQUENCY`).
    pub interpolation_mode: i32,
    /// `compute_frame_`: render scratch that also seeds new keyframes.
    pub compute_frame: WaveSourceKeyframe,
    /// `ShepardToneSource::loop_frame_` (unused by plain wave sources).
    pub loop_frame: WaveSourceKeyframe,
}

impl WaveSource {
    pub fn new() -> Self {
        Self {
            interpolation_mode: WAVE_SOURCE_FREQUENCY,
            ..Default::default()
        }
    }

    /// `render(wave_frame, position)` for a plain wave source: interpolates
    /// into the compute frame and returns it.
    pub fn render_compute_frame(&mut self, position: f32) -> &WaveFrame {
        self.compute_frame.interpolation_mode = self.interpolation_mode;
        let mut compute = std::mem::take(&mut self.compute_frame);
        self.keyframes.interpolate(&mut compute, position);
        self.compute_frame = compute;
        &self.compute_frame.wave_frame
    }

    /// `insertNewKeyframe(position)`: the new keyframe's frame is the
    /// source rendered at `position`.
    pub fn insert_new_keyframe(&mut self, position: i32) -> usize {
        let mut keyframe = Keyframe {
            position,
            data: WaveSourceKeyframe::default(),
        };
        let frame = self.render_compute_frame(position as f32);
        keyframe.data.wave_frame.copy_from(frame);
        self.keyframes.insert(keyframe)
    }

    fn read_json(&mut self, data: &Value) -> Result<()> {
        self.keyframes.keyframes.clear();
        for json_keyframe in data.get("keyframes")?.iter_values() {
            require_object(json_keyframe, "keyframe")?;
            let position = json_keyframe.get("position")?.to_i32()?;
            let index = self.insert_new_keyframe(position);
            self.keyframes.keyframes[index].read_json(json_keyframe)?;
        }
        read_interpolation_style(&mut self.keyframes, data)?;

        self.interpolation_mode = data.get("interpolation")?.to_enum()?;
        self.compute_frame.interpolation_mode = self.interpolation_mode;
        Ok(())
    }

    fn write_json(&self, data: &mut Value, component_type: ComponentType) {
        self.keyframes.write_json(data, component_type);
        data.set("interpolation", self.interpolation_mode);
    }
}

/// `WaveLineSource`.
#[derive(Clone, Debug)]
pub struct WaveLineSource {
    pub keyframes: Keyframes<LineSourceKeyframe>,
    pub num_points: i32,
    pub compute_frame: LineSourceKeyframe,
}

impl Default for WaveLineSource {
    fn default() -> Self {
        Self {
            keyframes: Keyframes::default(),
            num_points: Self::DEFAULT_LINE_POINTS,
            compute_frame: Default::default(),
        }
    }
}

impl WaveLineSource {
    pub const DEFAULT_LINE_POINTS: i32 = 4;

    pub fn insert_new_keyframe(&mut self, position: i32) -> usize {
        let keyframe = create_interpolated(&self.keyframes, position);
        self.keyframes.insert(keyframe)
    }

    fn read_json(&mut self, data: &Value) -> Result<()> {
        read_keyframes(&mut self.keyframes, data)?;
        self.num_points = data.get("num_points")?.to_i32()?;
        Ok(())
    }

    fn write_json(&self, data: &mut Value) {
        self.keyframes.write_json(data, ComponentType::LineSource);
        data.set("num_points", self.num_points);
    }
}

/// `FileSource::FadeStyle`.
pub const FADE_WAVE_BLEND: i32 = 0;
pub const FADE_NO_INTERPOLATE: i32 = 1;
pub const FADE_TIME_INTERPOLATE: i32 = 2;
pub const FADE_FREQ_INTERPOLATE: i32 = 3;
/// `FileSource::PhaseStyle`.
pub const PHASE_NONE: i32 = 0;
pub const PHASE_CLEAR: i32 = 1;
pub const PHASE_VOCODE: i32 = 2;

/// `FileSource::SampleBuffer`: the audio with one padding sample before and
/// three after (`kExtraBufferSamples`).
#[derive(Clone, Debug, Default)]
pub struct SampleBuffer {
    /// `None` until audio is loaded (upstream's null `data`).
    pub data: Option<Vec<f32>>,
    pub size: i32,
    pub sample_rate: i32,
}

/// `FileSource`.
#[derive(Clone, Debug)]
pub struct FileSource {
    pub keyframes: Keyframes<FileSourceKeyframe>,
    pub compute_frame: FileSourceKeyframe,
    pub sample_buffer: SampleBuffer,
    pub overridden_phase: Box<[f32; WAVEFORM_SIZE]>,
    pub fade_style: i32,
    pub phase_style: i32,
    pub normalize_gain: bool,
    pub normalize_mult: bool,
    pub window_size: f64,
    pub random_seed: i32,
    pub random_generator: RandomGenerator,
    /// `interpolate_from_frame_`/`interpolate_to_frame_`: scratch for the
    /// frequency-interpolating fade style.
    pub interpolate_from_frame: WaveSourceKeyframe,
    pub interpolate_to_frame: WaveSourceKeyframe,
}

impl Default for FileSource {
    fn default() -> Self {
        Self::new()
    }
}

/// `random_seed_ = random_generator_.next() * (INT_MAX / kPi)` in
/// `FileSource()`, from the generator's raw mt19937 draw `draw`, with the
/// generator spanning `[-kPi, kPi)`.
///
/// Apple clang's fast-math folds the expression to
/// `(int)((float)draw + -2^31f)` (A13 in
/// `migration/decisions/reference-arm.md`): `ucvtf`, `fadd`, `fcvtzs`. That
/// can differ from rounding each step, as the no-fast-math build and x86
/// GCC do, in the last bits of the seed, so Apple targets take the folded
/// form and others the generic one.
pub fn default_random_seed(draw: u32) -> i32 {
    #[cfg(target_vendor = "apple")]
    {
        folded_random_seed(draw)
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        generic_random_seed(draw)
    }
}

/// The seed with each step rounded: `next()`'s
/// `canonical * (max - min) + min`, then `* (INT_MAX / kPi)`, truncated.
pub fn generic_random_seed(draw: u32) -> i32 {
    let (min, max) = (-scalar::PI, scalar::PI);
    let value = canonical_from_draw(draw) * (max - min) + min;
    (value * (i32::MAX as f32 / scalar::PI)) as i32
}

/// The seed as Apple clang's fast-math folds it (A13): the draw as a float
/// less 2^31, truncated (saturating, as `fcvtzs` does).
pub fn folded_random_seed(draw: u32) -> i32 {
    (draw as f32 + -2_147_483_648.0) as i32
}

impl FileSource {
    pub const MAX_FILE_SOURCE_SAMPLES: f32 = 176_400.0;
    pub const EXTRA_SAVE_SAMPLES: i32 = 4;
    pub const EXTRA_BUFFER_SAMPLES: i32 = 4;
    pub const PITCH_DETECT_MAX_PERIOD: i32 = 8096;

    /// `FileSource()`: takes a seed from the global generator counter and
    /// draws the default `random_seed` from it.
    pub fn new() -> Self {
        let mut random_generator = RandomGenerator::from_global_counter(-scalar::PI, scalar::PI);
        let random_seed = default_random_seed(random_generator.next_draw());
        Self {
            keyframes: Keyframes::default(),
            compute_frame: FileSourceKeyframe::default(),
            sample_buffer: SampleBuffer::default(),
            overridden_phase: Box::new([0.0; WAVEFORM_SIZE]),
            fade_style: FADE_WAVE_BLEND,
            phase_style: PHASE_NONE,
            normalize_gain: false,
            normalize_mult: false,
            window_size: WAVEFORM_SIZE as f64,
            random_seed,
            random_generator,
            interpolate_from_frame: WaveSourceKeyframe::default(),
            interpolate_to_frame: WaveSourceKeyframe::default(),
        }
    }

    pub fn insert_new_keyframe(&mut self, position: i32) -> usize {
        let keyframe = create_interpolated(&self.keyframes, position);
        self.keyframes.insert(keyframe)
    }

    /// `getDataBuffer()`: the audio without padding.
    pub fn data_buffer(&self) -> Option<&[f32]> {
        let data = self.sample_buffer.data.as_ref()?;
        Some(&data[1..1 + self.sample_buffer.size as usize])
    }

    /// `loadBuffer(buffer, size, sample_rate)`.
    pub fn load_buffer(&mut self, buffer: &[f32], sample_rate: i32) {
        let size = buffer.len();
        let mut data = vec![0.0f32; size + Self::EXTRA_BUFFER_SAMPLES as usize];
        data[1..1 + size].copy_from_slice(buffer);
        data[0] = data[1];
        for i in 1..Self::EXTRA_BUFFER_SAMPLES as usize {
            data[size + i] = data[size];
        }
        self.sample_buffer = SampleBuffer {
            data: Some(data),
            size: size as i32,
            sample_rate,
        };
    }

    /// `setPhaseStyle(style)`.
    pub fn set_phase_style(&mut self, phase_style: i32) {
        if self.phase_style == phase_style {
            return;
        }
        self.phase_style = phase_style;
        if self.phase_style == PHASE_VOCODE {
            self.random_seed = self.random_seed.wrapping_add(1);
        }
        self.write_phase_override_buffer();
    }

    /// `writePhaseOverrideBuffer()`.
    pub fn write_phase_override_buffer(&mut self) {
        if self.phase_style == PHASE_CLEAR {
            for i in 0..WAVEFORM_SIZE / 2 {
                self.overridden_phase[2 * i] = -0.5 * scalar::PI;
                self.overridden_phase[2 * i + 1] = 0.5 * scalar::PI;
            }
        } else if self.phase_style == PHASE_VOCODE {
            self.random_generator.seed(self.random_seed);
            for value in self.overridden_phase.iter_mut() {
                *value = self.random_generator.next();
            }
        }
    }

    fn read_json(&mut self, data: &Value) -> Result<()> {
        self.normalize_gain = data.get("normalize_gain")?.to_bool()?;
        self.normalize_mult = if data.count("normalize_mult") != 0 {
            data.get("normalize_mult")?.to_bool()?
        } else {
            true
        };
        self.window_size = data.get("window_size")?.to_f64()?;
        self.fade_style = FADE_WAVE_BLEND;
        if data.count("fade_style") != 0 {
            self.fade_style = data.get("fade_style")?.to_enum()?;
        }
        self.phase_style = PHASE_NONE;
        if data.count("phase_style") != 0 {
            self.phase_style = data.get("phase_style")?.to_enum()?;
        }
        if data.count("random_seed") != 0 {
            self.random_seed = data.get("random_seed")?.to_i32()?;
        }
        self.write_phase_override_buffer();

        read_keyframes(&mut self.keyframes, data)?;

        let mut sample_rate = DEFAULT_SAMPLE_RATE;
        if data.count("audio_sample_rate") != 0 {
            sample_rate = data.get("audio_sample_rate")?.to_i32()?;
        }

        let audio = data.get("audio_file")?.to_str()?;
        let pcm = base64::decode_i16s(audio);
        let floats = scalar::pcm_to_float(&pcm);
        self.load_buffer(&floats, sample_rate);
        Ok(())
    }

    fn write_json(&self, data: &mut Value) {
        let mut max_position = 0.0f64;
        for keyframe in &self.keyframes.keyframes {
            max_position = if max_position < keyframe.data.start_position {
                keyframe.data.start_position
            } else {
                max_position
            };
        }

        self.keyframes.write_json(data, ComponentType::FileSource);
        data.set("normalize_gain", self.normalize_gain);
        data.set("normalize_mult", self.normalize_mult);
        data.set("window_size", self.window_size);
        data.set("fade_style", self.fade_style);
        data.set("phase_style", self.phase_style);
        data.set("random_seed", self.random_seed);
        data.set("audio_sample_rate", self.sample_buffer.sample_rate);

        let save_samples =
            (max_position + 2.0 * self.window_size + f64::from(Self::EXTRA_SAVE_SAMPLES)) as i32;
        let num_samples = self.sample_buffer.size.min(save_samples).max(0) as usize;
        let encoded = match self.data_buffer() {
            Some(buffer) => base64::encode_i16s(&scalar::float_to_pcm(&buffer[..num_samples])),
            None => String::new(),
        };
        data.set("audio_file", encoded);
    }
}

/// A modifier whose component-level state is a set of integer/bool
/// settings that it pushes into its keyframe before rendering.
#[derive(Clone, Debug, Default)]
pub struct Modifier<K, S> {
    pub keyframes: Keyframes<K>,
    pub settings: S,
    pub compute_frame: K,
}

impl<K: KeyframeData, S> Modifier<K, S> {
    pub fn insert_new_keyframe(&mut self, position: i32) -> usize {
        let keyframe = create_interpolated(&self.keyframes, position);
        self.keyframes.insert(keyframe)
    }
}

/// `PhaseModifier` settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhaseSettings {
    /// `PhaseModifier::PhaseStyle`: normal, even/odd, harmonic,
    /// harmonic even/odd, clear.
    pub phase_style: i32,
}

/// `WaveWindowModifier` settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowSettings {
    /// `WaveWindowModifier::WindowShape`: cos, half sin, linear, square, wiggle.
    pub window_shape: i32,
}

/// `FrequencyFilterModifier` settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterSettings {
    /// `FrequencyFilterModifier::FilterStyle`: low pass, band pass, high
    /// pass, comb.
    pub style: i32,
    pub normalize: bool,
}

impl Default for FilterSettings {
    fn default() -> Self {
        Self {
            style: 0,
            normalize: true,
        }
    }
}

/// `WaveWarpModifier` settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WarpSettings {
    pub horizontal_asymmetric: bool,
    pub vertical_asymmetric: bool,
}

pub type PhaseModifier = Modifier<PhaseModifierKeyframe, PhaseSettings>;
pub type WaveWindowModifier = Modifier<WaveWindowKeyframe, WindowSettings>;
pub type FrequencyFilterModifier = Modifier<FrequencyFilterKeyframe, FilterSettings>;
pub type SlewLimitModifier = Modifier<SlewLimitKeyframe, ()>;
pub type WaveFoldModifier = Modifier<WaveFoldKeyframe, ()>;
pub type WaveWarpModifier = Modifier<WaveWarpKeyframe, WarpSettings>;

/// Runs `$body` with `$keyframes` bound to the component's keyframe list,
/// whatever its keyframe type.
macro_rules! with_keyframes {
    ($component:expr, $keyframes:ident => $body:expr) => {
        match $component {
            WavetableComponent::WaveSource(c) | WavetableComponent::ShepardToneSource(c) => {
                let $keyframes = &mut c.keyframes;
                $body
            }
            WavetableComponent::LineSource(c) => {
                let $keyframes = &mut c.keyframes;
                $body
            }
            WavetableComponent::FileSource(c) => {
                let $keyframes = &mut c.keyframes;
                $body
            }
            WavetableComponent::PhaseModifier(c) => {
                let $keyframes = &mut c.keyframes;
                $body
            }
            WavetableComponent::WaveWindow(c) => {
                let $keyframes = &mut c.keyframes;
                $body
            }
            WavetableComponent::FrequencyFilter(c) => {
                let $keyframes = &mut c.keyframes;
                $body
            }
            WavetableComponent::SlewLimiter(c) => {
                let $keyframes = &mut c.keyframes;
                $body
            }
            WavetableComponent::WaveFolder(c) => {
                let $keyframes = &mut c.keyframes;
                $body
            }
            WavetableComponent::WaveWarp(c) => {
                let $keyframes = &mut c.keyframes;
                $body
            }
        }
    };
}

/// One wavetable component.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum WavetableComponent {
    WaveSource(WaveSource),
    LineSource(WaveLineSource),
    FileSource(Box<FileSource>),
    ShepardToneSource(WaveSource),
    PhaseModifier(PhaseModifier),
    WaveWindow(WaveWindowModifier),
    FrequencyFilter(FrequencyFilterModifier),
    SlewLimiter(SlewLimitModifier),
    WaveFolder(WaveFoldModifier),
    WaveWarp(WaveWarpModifier),
}

impl WavetableComponent {
    /// `WavetableComponentFactory::createComponent(type)`.
    pub fn new(component_type: ComponentType) -> Self {
        match component_type {
            ComponentType::WaveSource => WavetableComponent::WaveSource(WaveSource::new()),
            ComponentType::LineSource => WavetableComponent::LineSource(WaveLineSource::default()),
            ComponentType::FileSource => WavetableComponent::FileSource(Box::default()),
            ComponentType::ShepardToneSource => {
                WavetableComponent::ShepardToneSource(WaveSource::new())
            }
            ComponentType::PhaseModifier => WavetableComponent::PhaseModifier(Modifier::default()),
            ComponentType::WaveWindow => WavetableComponent::WaveWindow(Modifier::default()),
            ComponentType::FrequencyFilter => {
                WavetableComponent::FrequencyFilter(Modifier::default())
            }
            ComponentType::SlewLimiter => WavetableComponent::SlewLimiter(Modifier::default()),
            ComponentType::WaveFolder => WavetableComponent::WaveFolder(Modifier::default()),
            ComponentType::WaveWarp => WavetableComponent::WaveWarp(Modifier::default()),
        }
    }

    pub fn component_type(&self) -> ComponentType {
        match self {
            WavetableComponent::WaveSource(_) => ComponentType::WaveSource,
            WavetableComponent::LineSource(_) => ComponentType::LineSource,
            WavetableComponent::FileSource(_) => ComponentType::FileSource,
            WavetableComponent::ShepardToneSource(_) => ComponentType::ShepardToneSource,
            WavetableComponent::PhaseModifier(_) => ComponentType::PhaseModifier,
            WavetableComponent::WaveWindow(_) => ComponentType::WaveWindow,
            WavetableComponent::FrequencyFilter(_) => ComponentType::FrequencyFilter,
            WavetableComponent::SlewLimiter(_) => ComponentType::SlewLimiter,
            WavetableComponent::WaveFolder(_) => ComponentType::WaveFolder,
            WavetableComponent::WaveWarp(_) => ComponentType::WaveWarp,
        }
    }

    /// `hasKeyframes()`: false only for the Shepard tone source.
    pub fn has_keyframes(&self) -> bool {
        !matches!(self, WavetableComponent::ShepardToneSource(_))
    }

    /// `numFrames()`.
    pub fn num_frames(&self) -> usize {
        match self {
            WavetableComponent::WaveSource(c) | WavetableComponent::ShepardToneSource(c) => {
                c.keyframes.num_frames()
            }
            WavetableComponent::LineSource(c) => c.keyframes.num_frames(),
            WavetableComponent::FileSource(c) => c.keyframes.num_frames(),
            WavetableComponent::PhaseModifier(c) => c.keyframes.num_frames(),
            WavetableComponent::WaveWindow(c) => c.keyframes.num_frames(),
            WavetableComponent::FrequencyFilter(c) => c.keyframes.num_frames(),
            WavetableComponent::SlewLimiter(c) => c.keyframes.num_frames(),
            WavetableComponent::WaveFolder(c) => c.keyframes.num_frames(),
            WavetableComponent::WaveWarp(c) => c.keyframes.num_frames(),
        }
    }

    /// Its keyframes' positions, in order.
    pub fn keyframe_positions(&self) -> Vec<i32> {
        fn positions<K>(k: &Keyframes<K>) -> Vec<i32> {
            k.keyframes.iter().map(|k| k.position).collect()
        }
        match self {
            WavetableComponent::WaveSource(c) | WavetableComponent::ShepardToneSource(c) => {
                positions(&c.keyframes)
            }
            WavetableComponent::LineSource(c) => positions(&c.keyframes),
            WavetableComponent::FileSource(c) => positions(&c.keyframes),
            WavetableComponent::PhaseModifier(c) => positions(&c.keyframes),
            WavetableComponent::WaveWindow(c) => positions(&c.keyframes),
            WavetableComponent::FrequencyFilter(c) => positions(&c.keyframes),
            WavetableComponent::SlewLimiter(c) => positions(&c.keyframes),
            WavetableComponent::WaveFolder(c) => positions(&c.keyframes),
            WavetableComponent::WaveWarp(c) => positions(&c.keyframes),
        }
    }

    /// `getLastKeyframePosition()`.
    pub fn last_keyframe_position(&self) -> i32 {
        let positions = self.keyframe_positions();
        if positions.is_empty() {
            return 0;
        }
        if !self.has_keyframes() {
            return NUM_OSCILLATOR_WAVE_FRAMES - 1;
        }
        *positions.last().expect("non-empty")
    }

    /// `insertNewKeyframe(position)`: returns the new keyframe's index.
    pub fn insert_new_keyframe(&mut self, position: i32) -> usize {
        match self {
            WavetableComponent::WaveSource(c) | WavetableComponent::ShepardToneSource(c) => {
                c.insert_new_keyframe(position)
            }
            WavetableComponent::LineSource(c) => c.insert_new_keyframe(position),
            WavetableComponent::FileSource(c) => c.insert_new_keyframe(position),
            WavetableComponent::PhaseModifier(c) => c.insert_new_keyframe(position),
            WavetableComponent::WaveWindow(c) => c.insert_new_keyframe(position),
            WavetableComponent::FrequencyFilter(c) => c.insert_new_keyframe(position),
            WavetableComponent::SlewLimiter(c) => c.insert_new_keyframe(position),
            WavetableComponent::WaveFolder(c) => c.insert_new_keyframe(position),
            WavetableComponent::WaveWarp(c) => c.insert_new_keyframe(position),
        }
    }

    /// `reset()`: the keyframes cleared and one inserted at position 0.
    /// The component's own settings stay.
    pub fn reset(&mut self) {
        with_keyframes!(self, keyframes => keyframes.keyframes.clear());
        self.insert_new_keyframe(0);
    }

    /// `remove(keyframe)` for the keyframe at `index`.
    pub fn remove_keyframe(&mut self, index: usize) {
        with_keyframes!(self, keyframes => {
            if index < keyframes.keyframes.len() {
                keyframes.remove(index);
            }
        })
    }

    /// The position of the keyframe at `index`.
    pub fn keyframe_position(&self, index: usize) -> Option<i32> {
        self.keyframe_positions().get(index).copied()
    }

    /// `keyframe->setPosition(position)` for the keyframe at `index`, which
    /// stays where it is in the list until [`Self::reposition_keyframe`].
    pub fn set_keyframe_position(&mut self, index: usize, position: i32) {
        with_keyframes!(self, keyframes => {
            if let Some(keyframe) = keyframes.keyframes.get_mut(index) {
                keyframe.position = position;
            }
        })
    }

    /// `reposition(keyframe)` for the keyframe at `index`: moves it to its
    /// place for its position, and returns its new index.
    pub fn reposition_keyframe(&mut self, index: usize) -> usize {
        with_keyframes!(self, keyframes => {
            if index < keyframes.keyframes.len() {
                keyframes.reposition(index)
            } else {
                index
            }
        })
    }

    /// `stateToJson()`.
    pub fn to_json(&self) -> Value {
        let mut data = Value::object();
        let component_type = self.component_type();
        match self {
            WavetableComponent::WaveSource(c) | WavetableComponent::ShepardToneSource(c) => {
                c.write_json(&mut data, component_type)
            }
            WavetableComponent::LineSource(c) => c.write_json(&mut data),
            WavetableComponent::FileSource(c) => c.write_json(&mut data),
            WavetableComponent::PhaseModifier(c) => {
                c.keyframes.write_json(&mut data, component_type);
                data.set("style", c.settings.phase_style);
            }
            WavetableComponent::WaveWindow(c) => {
                c.keyframes.write_json(&mut data, component_type);
                data.set("window_shape", c.settings.window_shape);
            }
            WavetableComponent::FrequencyFilter(c) => {
                c.keyframes.write_json(&mut data, component_type);
                data.set("style", c.settings.style);
                data.set("normalize", c.settings.normalize);
            }
            WavetableComponent::SlewLimiter(c) => c.keyframes.write_json(&mut data, component_type),
            WavetableComponent::WaveFolder(c) => c.keyframes.write_json(&mut data, component_type),
            WavetableComponent::WaveWarp(c) => {
                c.keyframes.write_json(&mut data, component_type);
                data.set("horizontal_asymmetric", c.settings.horizontal_asymmetric);
                data.set("vertical_asymmetric", c.settings.vertical_asymmetric);
            }
        }
        data
    }

    /// `jsonToState(data)`.
    pub fn read_json(&mut self, data: &Value) -> Result<()> {
        match self {
            WavetableComponent::WaveSource(c) | WavetableComponent::ShepardToneSource(c) => {
                c.read_json(data)
            }
            WavetableComponent::LineSource(c) => c.read_json(data),
            WavetableComponent::FileSource(c) => c.read_json(data),
            WavetableComponent::PhaseModifier(c) => {
                read_keyframes(&mut c.keyframes, data)?;
                c.settings.phase_style = data.get("style")?.to_enum()?;
                Ok(())
            }
            WavetableComponent::WaveWindow(c) => {
                read_keyframes(&mut c.keyframes, data)?;
                c.settings.window_shape = data.get("window_shape")?.to_enum()?;
                Ok(())
            }
            WavetableComponent::FrequencyFilter(c) => {
                read_keyframes(&mut c.keyframes, data)?;
                c.settings.style = data.get("style")?.to_enum()?;
                c.settings.normalize = data.get("normalize")?.to_bool()?;
                Ok(())
            }
            WavetableComponent::SlewLimiter(c) => read_keyframes(&mut c.keyframes, data),
            WavetableComponent::WaveFolder(c) => read_keyframes(&mut c.keyframes, data),
            WavetableComponent::WaveWarp(c) => {
                read_keyframes(&mut c.keyframes, data)?;
                c.settings.horizontal_asymmetric = data.get("horizontal_asymmetric")?.to_bool()?;
                c.settings.vertical_asymmetric = data.get("vertical_asymmetric")?.to_bool()?;
                Ok(())
            }
        }
    }

    /// Creates a component from its JSON (`createComponent(type)` then
    /// `jsonToState`).
    pub fn from_json(data: &Value) -> Result<Self> {
        require_object(data, "component")?;
        let type_name = data.get("type")?.to_str()?;
        let component_type = ComponentType::from_name(type_name)
            .ok_or_else(|| JsonError(format!("unknown wavetable component type '{type_name}'")))?;
        let mut component = Self::new(component_type);
        component.read_json(data)?;
        Ok(component)
    }
}
