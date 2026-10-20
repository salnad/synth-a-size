//! The wavetable creator's state: groups of components with keyframes, and
//! upstream's exact JSON load/save including the version upgrades.
//!
//! Ports the state parts of `vital/src/common/wavetable/wavetable_creator.{h,cpp}`
//! (Copyright 2013-2019 Matt Tytel, GPL-3.0-or-later). The wavetable name
//! and author, which upstream keeps on the `vital::Wavetable`, live on the
//! creator here.

pub mod component;
pub mod group;
pub mod keyframes;

use sas_common::json::{JsonError, Result, Value};
use sas_common::load_save::{self, compare_version_strings};
use sas_common::{scalar, LineGenerator};

pub use component::{ComponentType, WaveSource, WavetableComponent};
pub use group::WavetableGroup;
pub use keyframes::Keyframe;

use crate::wave_frame::{PredefinedShape, PredefinedWaveFrames, WAVEFORM_SIZE};
use crate::NUM_OSCILLATOR_WAVE_FRAMES;
use component::INTERPOLATION_NONE;
use keyframes::require_object;

/// `WavetableCreator`'s state.
#[derive(Clone, Debug)]
pub struct WavetableCreator {
    pub groups: Vec<WavetableGroup>,
    pub name: String,
    pub author: String,
    pub last_file_loaded: String,
    pub full_normalize: bool,
    pub remove_all_dc: bool,
}

impl Default for WavetableCreator {
    fn default() -> Self {
        Self::new()
    }
}

/// Component type names in the order before version 0.3.3, when `type` was
/// saved as an index.
const OLD_ORDER: [&str; 9] = [
    "Wave Source",
    "Line Source",
    "Audio File Source",
    "Phase Shift",
    "Wave Window",
    "Frequency Filter",
    "Slew Limiter",
    "Wave Folder",
    "Wave Warp",
];

fn set_field(target: &mut Value, key: &str, value: Value) -> Result<()> {
    require_object(target, key)?;
    target.set(key, value);
    Ok(())
}

/// Rebuilds `data["groups"][*]["components"][*]` through `f`, the way
/// upstream's upgrade loops copy and reassemble each level (an empty level
/// becomes `null`).
fn map_components(data: &mut Value, mut f: impl FnMut(&mut Value) -> Result<()>) -> Result<()> {
    let mut new_groups = Value::Null;
    for json_group in data.get("groups")?.iter_values() {
        let mut json_group = json_group.clone();
        let mut new_components = Value::Null;
        for json_component in json_group.get("components")?.iter_values() {
            let mut json_component = json_component.clone();
            f(&mut json_component)?;
            new_components.push(json_component);
        }
        set_field(&mut json_group, "components", new_components)?;
        new_groups.push(json_group);
    }
    set_field(data, "groups", new_groups)
}

impl WavetableCreator {
    /// A creator with no groups, like a freshly constructed upstream one.
    pub fn new() -> Self {
        Self {
            groups: Vec::new(),
            name: String::new(),
            author: String::new(),
            last_file_loaded: String::new(),
            full_normalize: true,
            remove_all_dc: true,
        }
    }

    pub fn add_group(&mut self, group: WavetableGroup) {
        self.groups.push(group);
    }

    /// `removeGroup(index)`.
    pub fn remove_group(&mut self, index: usize) {
        if index < self.groups.len() {
            self.groups.remove(index);
        }
    }

    /// `moveUp(index)`.
    pub fn move_up(&mut self, index: usize) {
        if index == 0 || index >= self.groups.len() {
            return;
        }
        self.groups.swap(index, index - 1);
    }

    /// `moveDown(index)`.
    pub fn move_down(&mut self, index: usize) {
        if index + 1 >= self.groups.len() {
            return;
        }
        self.groups.swap(index, index + 1);
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        self.groups.clear();
        self.remove_all_dc = true;
        self.full_normalize = true;
    }

    /// `loadDefaultCreator()`: names the table "Init" and adds the default
    /// saw group.
    pub fn load_default_creator(&mut self) {
        self.name = "Init".to_string();
        let mut group = WavetableGroup::new();
        group.load_default_group();
        self.add_group(group);
    }

    /// The state part of `init()`: `clear()` then `loadDefaultCreator()`.
    pub fn init(&mut self) {
        self.clear();
        self.load_default_creator();
    }

    /// The state part of `initPredefinedWaves()`: one wave source with the
    /// six basic shapes, no interpolation, no normalisation or DC removal.
    /// The name is left unchanged.
    pub fn init_predefined_waves(&mut self) {
        self.clear();

        let mut group = WavetableGroup::new();
        let mut wave_source = WaveSource::new();
        let num_shapes = PredefinedShape::ALL.len() as i32;
        for (i, shape) in PredefinedShape::ALL.iter().enumerate() {
            let position = (NUM_OSCILLATOR_WAVE_FRAMES * i as i32) / num_shapes;
            wave_source.insert_new_keyframe(position);
            let keyframe = &mut wave_source.keyframes.keyframes[i];
            keyframe
                .data
                .wave_frame
                .copy_from(PredefinedWaveFrames::get(*shape));
        }
        wave_source.keyframes.interpolation_style = INTERPOLATION_NONE;
        self.full_normalize = false;
        self.remove_all_dc = false;

        group.add_component(WavetableComponent::WaveSource(wave_source));
        self.add_group(group);
    }

    /// The state part of `initFromLineGenerator(generator)`.
    pub fn init_from_line_generator(&mut self, line_generator: &LineGenerator) -> Result<()> {
        self.clear();
        self.name = line_generator.name().to_string();

        let mut group = WavetableGroup::new();
        let mut line_source = component::WaveLineSource::default();
        line_source.insert_new_keyframe(0);
        line_source.keyframes.keyframes[0]
            .data
            .line_generator
            .from_json(&line_generator.to_json())?;
        group.add_component(WavetableComponent::LineSource(line_source));
        self.add_group(group);
        Ok(())
    }

    /// `isValidJson(data)`: a line shape, or an object with `version`,
    /// `name` and an array of `groups`.
    pub fn is_valid_json(data: &Value) -> bool {
        if LineGenerator::is_valid_json(data) {
            return true;
        }
        if data.count("version") == 0 || data.count("groups") == 0 || data.count("name") == 0 {
            return false;
        }
        data.get("groups").is_ok_and(Value::is_array)
    }

    /// `updateJson(data)`: upgrades JSON saved by older versions.
    pub fn update_json(mut data: Value) -> Result<Value> {
        let version = if data.count("version") != 0 {
            data.get("version")?.to_str()?.to_string()
        } else {
            "0.0.0".to_string()
        };

        if compare_version_strings(&version, "0.3.3") < 0 {
            map_components(&mut data, |json_component| {
                let int_type = json_component.get("type")?.to_i32()?;
                let name = usize::try_from(int_type)
                    .ok()
                    .and_then(|i| OLD_ORDER.get(i))
                    .ok_or_else(|| JsonError(format!("invalid old component type {int_type}")))?;
                set_field(json_component, "type", Value::from(*name))
            })?;
        }

        if compare_version_strings(&version, "0.3.7") < 0 {
            map_components(&mut data, |json_component| {
                if json_component.get("type")?.to_str()? == "Audio File Source" {
                    load_save::convert_buffer_to_pcm(json_component, "audio_file")?;
                }
                Ok(())
            })?;
        }

        if compare_version_strings(&version, "0.3.8") < 0 {
            set_field(&mut data, "remove_all_dc", Value::Bool(false))?;
        }

        if compare_version_strings(&version, "0.3.9") < 0
            && compare_version_strings(&version, "0.3.7") >= 0
        {
            map_components(&mut data, |json_component| {
                let type_name = json_component.get("type")?.to_str()?;
                if type_name == "Wave Source" || type_name == "Shepard Tone Source" {
                    let mut new_keyframes = Value::Null;
                    for json_keyframe in json_component.get("keyframes")?.iter_values() {
                        let mut json_keyframe = json_keyframe.clone();
                        require_object(&json_keyframe, "keyframe")?;
                        load_save::convert_pcm_to_float_buffer(&mut json_keyframe, "wave_data")?;
                        new_keyframes.push(json_keyframe);
                    }
                    set_field(json_component, "keyframes", new_keyframes)?;
                }
                Ok(())
            })?;
        }

        if compare_version_strings(&version, "0.4.7") < 0 {
            set_field(&mut data, "full_normalize", Value::Bool(false))?;
        }

        if compare_version_strings(&version, "0.7.7") < 0 {
            let mut line_converter = LineGenerator::new(LineGenerator::DEFAULT_RESOLUTION);
            map_components(&mut data, |json_component| {
                if json_component.get("type")?.to_str()? == "Line Source" {
                    upgrade_line_source(json_component, &mut line_converter)?;
                }
                Ok(())
            })?;
        }

        Ok(data)
    }

    /// `stateToJson()`.
    pub fn to_json(&self) -> Value {
        let mut json_groups = Value::Null;
        for group in &self.groups {
            json_groups.push(group.to_json());
        }

        let mut data = Value::object();
        data.set("groups", json_groups);
        data.set("name", self.name.as_str());
        data.set("author", self.author.as_str());
        data.set("version", load_save::VERSION_STRING);
        data.set("remove_all_dc", self.remove_all_dc);
        data.set("full_normalize", self.full_normalize);
        data
    }

    /// The state part of `jsonToState(data)`: a line shape becomes a single
    /// line source; anything else is upgraded with [`Self::update_json`] and
    /// loaded. Upstream renders afterwards; that is the caller's job.
    pub fn read_json(&mut self, data: &Value) -> Result<()> {
        if LineGenerator::is_valid_json(data) {
            let mut generator = LineGenerator::new(WAVEFORM_SIZE);
            generator.from_json(data)?;
            return self.init_from_line_generator(&generator);
        }

        self.clear();
        let data = Self::update_json(data.clone())?;

        self.name = if data.count("name") != 0 {
            data.get("name")?.to_str()?.to_string()
        } else {
            String::new()
        };
        self.author = if data.count("author") != 0 {
            data.get("author")?.to_str()?.to_string()
        } else {
            String::new()
        };

        if data.count("remove_all_dc") != 0 {
            self.remove_all_dc = data.get("remove_all_dc")?.to_bool()?;
        }
        self.full_normalize = if data.count("full_normalize") != 0 {
            data.get("full_normalize")?.to_bool()?
        } else {
            false
        };

        for json_group in data.get("groups")?.iter_values() {
            let mut group = WavetableGroup::new();
            group.read_json(json_group)?;
            self.add_group(group);
        }
        Ok(())
    }

    /// A creator loaded from JSON.
    pub fn from_json(data: &Value) -> Result<Self> {
        let mut creator = Self::new();
        creator.read_json(data)?;
        Ok(creator)
    }

    /// `getLastKeyframePosition` over all groups (the frame count of a
    /// render is this plus one).
    pub fn last_keyframe_position(&self) -> i32 {
        self.groups
            .iter()
            .map(WavetableGroup::last_keyframe_position)
            .fold(0, i32::max)
    }
}

/// The 0.7.7 Line Source upgrade: old keyframes stored `points` in -1..1 and
/// `powers` without the end points; each becomes a `line` with two extra
/// points closing the loop. `converter` persists across all keyframes, as
/// upstream's does.
fn upgrade_line_source(json_component: &mut Value, converter: &mut LineGenerator) -> Result<()> {
    let num_points = json_component.get("num_points")?.to_i32()?;
    if !(1..=(LineGenerator::MAX_POINTS as i32 - 2)).contains(&num_points) {
        return Err(JsonError(format!(
            "old line source num_points {num_points} out of range"
        )));
    }
    set_field(json_component, "num_points", Value::from(num_points + 2))?;
    converter.set_num_points(num_points as usize + 2);
    let n = num_points as usize;

    let mut new_keyframes = Value::Null;
    for json_keyframe in json_component.get("keyframes")?.iter_values() {
        let mut json_keyframe = json_keyframe.clone();
        require_object(&json_keyframe, "keyframe")?;
        let point_data = json_keyframe.get("points")?.clone();
        let power_data = json_keyframe.get("powers")?.clone();
        for i in 0..n {
            let x = point_data.at(2 * i)?.to_f32()?;
            let y = point_data.at(2 * i + 1)?.to_f32()?;
            converter.set_point(i + 1, (x, y * 0.5 + 0.5));
            converter.set_power(i + 1, power_data.at(i)?.to_f32()?);
        }
        let start_x = point_data.at(0)?.to_f32()?;
        let start_y = point_data.at(1)?.to_f32()?;
        let end_x = point_data.at(2 * (n - 1))?.to_f32()?;
        let end_y = point_data.at(2 * (n - 1) + 1)?.to_f32()?;

        let range_x = start_x - end_x + 1.0;
        let y = if range_x < 0.001 {
            0.5 * (start_y + end_y)
        } else {
            let t = (1.0 - end_x) / range_x;
            scalar::interpolate(end_y, start_y, t)
        };

        converter.set_point(0, (0.0, y * 0.5 + 0.5));
        converter.set_point(n + 1, (1.0, y * 0.5 + 0.5));
        converter.set_power(0, 0.0);
        converter.set_power(n + 1, 0.0);

        set_field(&mut json_keyframe, "line", converter.to_json())?;
        new_keyframes.push(json_keyframe);
    }
    set_field(json_component, "keyframes", new_keyframes)
}

#[cfg(test)]
mod tests;
