//! Ports `vital/src/common/wavetable/wavetable_group.{h,cpp}` (Copyright
//! 2013-2019 Matt Tytel, GPL-3.0-or-later): a chain of one source followed
//! by modifiers.

use sas_common::json::{Result, Value};

use super::component::{ComponentType, WaveSource, WavetableComponent};
use super::keyframes::require_object;
use crate::wave_frame::WAVEFORM_SIZE;

/// `WavetableGroup`.
#[derive(Clone, Debug, Default)]
pub struct WavetableGroup {
    pub components: Vec<WavetableComponent>,
}

impl WavetableGroup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_component(&mut self, component: WavetableComponent) {
        self.components.push(component);
    }

    /// `removeComponent(index)`.
    pub fn remove_component(&mut self, index: usize) {
        if index < self.components.len() {
            self.components.remove(index);
        }
    }

    /// `moveUp(index)`.
    pub fn move_up(&mut self, index: usize) {
        if index == 0 || index >= self.components.len() {
            return;
        }
        self.components.swap(index, index - 1);
    }

    /// `moveDown(index)`.
    pub fn move_down(&mut self, index: usize) {
        if index + 1 >= self.components.len() {
            return;
        }
        self.components.swap(index, index + 1);
    }

    /// `reset()`.
    pub fn reset(&mut self) {
        self.components.clear();
        self.load_default_group();
    }

    /// `isShepardTone()`: true when every component is a Shepard tone
    /// source (including an empty group).
    pub fn is_shepard_tone(&self) -> bool {
        self.components
            .iter()
            .all(|c| c.component_type() == ComponentType::ShepardToneSource)
    }

    /// `getLastKeyframePosition()`.
    pub fn last_keyframe_position(&self) -> i32 {
        self.components
            .iter()
            .map(WavetableComponent::last_keyframe_position)
            .fold(0, i32::max)
    }

    /// `loadDefaultGroup()`: a wave source with one saw keyframe.
    pub fn load_default_group(&mut self) {
        let mut wave_source = WaveSource::new();
        wave_source.insert_new_keyframe(0);
        let wave_frame = &mut wave_source.keyframes.keyframes[0].data.wave_frame;
        // The fast-math reference multiplies by the reciprocal of 2047
        // instead of dividing (4 samples differ by 1 ulp otherwise).
        const INV_LAST_INDEX: f32 = 1.0 / (WAVEFORM_SIZE as f32 - 1.0);
        for i in 0..WAVEFORM_SIZE {
            let t = i as f32 * INV_LAST_INDEX;
            let half_shift = (i + WAVEFORM_SIZE / 2) % WAVEFORM_SIZE;
            wave_frame.time_domain[half_shift] = 1.0 - 2.0 * t;
        }
        wave_frame.to_frequency_domain();
        self.add_component(WavetableComponent::WaveSource(wave_source));
    }

    /// `stateToJson()`.
    pub fn to_json(&self) -> Value {
        let mut json_components = Value::Null;
        for component in &self.components {
            json_components.push(component.to_json());
        }
        let mut data = Value::object();
        data.set("components", json_components);
        data
    }

    /// `jsonToState(data)`.
    pub fn read_json(&mut self, data: &Value) -> Result<()> {
        require_object(data, "group")?;
        self.components.clear();
        for json_component in data.get("components")?.iter_values() {
            let component = WavetableComponent::from_json(json_component)?;
            self.add_component(component);
        }
        Ok(())
    }
}
