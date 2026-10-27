// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_gui_basics, GPL-3.0-only; juce_core, ISC).
// SPDX-License-Identifier: GPL-3.0-or-later AND GPL-3.0-only

//! The preset state and its JSON load/save.
//!
//! Ports `LoadSave::stateToJson`, `LoadSave::jsonToState` and the
//! `loadControls` / `loadModulations` / `loadSample` / `loadWavetables` /
//! `loadLfos` / `loadSaveState` / `initSaveInfo` helpers from
//! `vital/src/common/load_save.cpp`, and the parts of
//! `vital/src/common/synth_base.cpp` that own this state (`loadFromFile`,
//! `loadInitPreset`, `connectModulation`, `disconnectModulation`, the macro
//! name fallback).

use std::collections::BTreeMap;
use std::path::Path;

use sas_common::json::{self as json, Value};
use sas_common::LineGenerator;
use sas_wavetable::creator::WavetableCreator;

use crate::constants::*;
use crate::engine;
use crate::json::{ensure_object, read_f32, read_string};
use crate::registry::{registry, ParamInfo};
use crate::sample::SampleState;
use crate::upgrade::update_from_old_version;
use crate::version::{compare_feature_version_strings, SYNTH_VERSION};
use crate::Error;

/// What `SynthBase::loadInitPreset` needs besides the parameter defaults:
/// the seed of the white-noise sample it generates (upstream takes the next
/// value of the engine's global seed counter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitContent {
    pub sample_seed: i32,
}

/// `WavetableCreator::init`: the default wavetable.
fn default_wavetable() -> WavetableCreator {
    let mut creator = WavetableCreator::new();
    creator.init();
    creator
}

fn linear_line() -> LineGenerator {
    let mut line = LineGenerator::default();
    line.init_linear();
    line
}

/// One modulation slot (`ModulationConnection`).
#[derive(Clone, Debug)]
pub struct ModulationSlot {
    /// Empty when the slot is unused.
    pub source: String,
    /// Empty when the slot is unused.
    pub destination: String,
    /// The slot's remapping curve; saved only while it is not linear.
    pub line_mapping: LineGenerator,
}

impl Default for ModulationSlot {
    fn default() -> ModulationSlot {
        ModulationSlot {
            source: String::new(),
            destination: String::new(),
            line_mapping: linear_line(),
        }
    }
}

impl ModulationSlot {
    /// `isConnectionAvailable`.
    pub fn is_free(&self) -> bool {
        self.source.is_empty() && self.destination.is_empty()
    }
}

/// What upstream's editor gives a slot it has just created by dragging a
/// source onto a control (`ModulationManager::setModulationValues`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotValues {
    /// `modulation_N_amount`.
    pub amount: f32,
    pub bipolar: bool,
    pub stereo: bool,
    pub bypass: bool,
}

impl SlotValues {
    /// `ModulationManager::modulationDraggedToComponent`: the values for a
    /// source dropped on `destination`'s control while it is at `value`
    /// (its live value, the host parameter's). `bipolar` is whether a
    /// modifier key is held. The amount reaches from the control's position
    /// to the top of its range (to the nearer end, doubled, when bipolar),
    /// and at least `kDefaultModulationRatio`; stereo and bypass are off.
    pub fn dragged_to(destination: &ParamInfo, value: f32, bipolar: bool) -> SlotValues {
        const DEFAULT_MODULATION_RATIO: f32 = 0.25;
        // juce::Slider::valueToProportionOfLength on the control's linear
        // range, in double as JUCE computes it, then stored as a float.
        let (min, max) = (f64::from(destination.min), f64::from(destination.max));
        let percent = ((f64::from(value) - min) / (max - min)).clamp(0.0, 1.0) as f32;
        let mut amount = 1.0 - percent;
        if bipolar {
            amount = amount.min(percent) * 2.0;
        }
        SlotValues {
            amount: amount.max(DEFAULT_MODULATION_RATIO),
            bipolar,
            stereo: false,
            bypass: false,
        }
    }
}

/// The result of [`PresetState::connect_modulation_from_gui`].
#[derive(Clone, Debug, PartialEq)]
pub struct GuiConnection {
    /// The bank slot (0-based; its controls are `modulation_{slot + 1}_*`).
    pub slot: usize,
    /// Whether the connection took a new slot.
    pub created: bool,
    /// The control values the connection set, which a plugin must also set
    /// on its host parameters, when a slot was created: `modulation_N_power`
    /// (0), then `modulation_N_amount`, `_bipolar`, `_stereo` and `_bypass`
    /// from the [`SlotValues`]. A reused slot would otherwise keep the old
    /// route's amount, stereo and bypass.
    pub parameter_changes: Vec<(&'static str, f32)>,
}

/// The registry's own copy of control `name`.
fn control_name(name: &str) -> &'static str {
    registry()
        .get(name)
        .map(|info| info.name.as_str())
        .expect("every modulation slot control is in the registry")
}

/// Preset name, author, comments, style and macro names (`save_info_`).
#[derive(Clone, Debug, PartialEq)]
pub struct SaveInfo {
    pub preset_name: String,
    pub author: String,
    pub comments: String,
    pub style: String,
    pub macros: [String; NUM_MACROS],
}

fn default_macro_name(index: usize) -> String {
    format!("MACRO {}", index + 1)
}

impl Default for SaveInfo {
    /// `LoadSave::initSaveInfo`.
    fn default() -> SaveInfo {
        SaveInfo {
            preset_name: String::new(),
            author: String::new(),
            comments: String::new(),
            style: String::new(),
            macros: std::array::from_fn(default_macro_name),
        }
    }
}

impl SaveInfo {
    /// `SynthBase::getMacroName`: a blank name reads as "MACRO n".
    pub fn macro_name(&self, index: usize) -> String {
        let name = &self.macros[index];
        // juce::String::trim strips the C-locale whitespace characters.
        if name
            .trim_matches(|c: char| c.is_ascii_whitespace() || c == '\u{b}')
            .is_empty()
        {
            default_macro_name(index)
        } else {
            name.clone()
        }
    }
}

/// The whole preset state of the synth: every control value, the modulation
/// slots, the sample, wavetables, LFO shapes and save info.
#[derive(Clone, Debug)]
pub struct PresetState {
    controls: BTreeMap<String, f32>,
    modulations: Vec<ModulationSlot>,
    /// The connected slots in `SynthBase::mod_connections_` order: slot
    /// order after a load, then each new connection last.
    connection_order: Vec<usize>,
    pub sample: SampleState,
    pub wavetables: Vec<WavetableCreator>,
    pub lfos: Vec<LineGenerator>,
    pub info: SaveInfo,
}

impl PresetState {
    /// The init state (`SynthBase::loadInitPreset` on a new synth): default
    /// controls, no modulations, triangle LFOs, default wavetables and a
    /// white-noise sample from `init.sample_seed`. A preset loaded on top
    /// keeps the wavetables and LFO shapes it does not set.
    pub fn new(init: &InitContent) -> PresetState {
        let mut state = PresetState {
            controls: BTreeMap::new(),
            modulations: vec![ModulationSlot::default(); MAX_MODULATION_CONNECTIONS],
            connection_order: Vec::new(),
            sample: SampleState::white_noise(init.sample_seed),
            wavetables: Vec::new(),
            lfos: Vec::new(),
            info: SaveInfo::default(),
        };
        state.load_init(init);
        state
    }

    /// `SynthBase::loadInitPreset` (`SynthBase::initEngine` plus
    /// `LoadSave::initSaveInfo`).
    pub fn load_init(&mut self, init: &InitContent) {
        self.clear_modulations();
        self.wavetables = (0..NUM_OSCILLATORS).map(|_| default_wavetable()).collect();
        self.sample = SampleState::white_noise(init.sample_seed);
        self.lfos.resize_with(NUM_LFOS, LineGenerator::default);
        for lfo in &mut self.lfos {
            lfo.init_triangle();
        }
        self.controls = engine::controls()
            .into_iter()
            .map(|name| (name.to_string(), default_value(name)))
            .collect();
        self.info = SaveInfo::default();
    }

    /// Control values by name, sorted.
    pub fn controls(&self) -> &BTreeMap<String, f32> {
        &self.controls
    }

    /// The value of control `name`.
    pub fn value(&self, name: &str) -> Option<f32> {
        self.controls.get(name).copied()
    }

    /// Sets control `name`. Values are stored as given, like upstream's
    /// `Value::set`; nothing is clamped to the parameter's range.
    pub fn set_value(&mut self, name: &str, value: f32) -> Result<(), Error> {
        match self.controls.get_mut(name) {
            Some(slot) => {
                *slot = value;
                Ok(())
            }
            None if registry().contains(name) => Err(Error::NotAControl(name.to_string())),
            None => Err(Error::UnknownParameter(name.to_string())),
        }
    }

    /// The modulation slots, in bank order.
    pub fn modulations(&self) -> &[ModulationSlot] {
        &self.modulations
    }

    /// The connected slots in the order the synth keeps its connections
    /// (`SynthBase::mod_connections_`, which `getSourceConnections` lists):
    /// slot order after a load, then each new connection last.
    pub fn connection_order(&self) -> &[usize] {
        &self.connection_order
    }

    /// Mutable access to one slot's line mapping.
    pub fn line_mapping_mut(&mut self, slot: usize) -> Option<&mut LineGenerator> {
        self.modulations
            .get_mut(slot)
            .map(|slot| &mut slot.line_mapping)
    }

    /// Slot index of the connection `source` → `destination`, if connected.
    pub fn modulation_index(&self, source: &str, destination: &str) -> Option<usize> {
        self.modulations
            .iter()
            .position(|slot| slot.source == source && slot.destination == destination)
    }

    /// `SynthBase::connectModulation(source, destination)`: reuses an existing
    /// connection between the two, otherwise takes the first free slot
    /// (`ModulationConnectionBank::createConnection`, which never gives slot
    /// n a connection to `modulation_n_amount`) and sets the slot's bipolar
    /// switch from the source's default. Returns the slot index.
    pub fn add_modulation(&mut self, source: &str, destination: &str) -> Result<usize, Error> {
        if !engine::is_modulation_source(source) {
            return Err(Error::UnknownModulationSource(source.to_string()));
        }
        if !engine::is_modulation_destination(destination) {
            return Err(Error::UnknownModulationDestination(destination.to_string()));
        }
        if let Some(index) = self.modulation_index(source, destination) {
            return Ok(index);
        }
        let index = (0..MAX_MODULATION_CONNECTIONS)
            .find(|&i| {
                destination != format!("modulation_{}_amount", i + 1)
                    && self.modulations[i].is_free()
            })
            .ok_or(Error::NoModulationSlot)?;
        let slot = &mut self.modulations[index];
        slot.source = source.to_string();
        slot.destination = destination.to_string();
        self.connection_order.push(index);
        let bipolar = if engine::is_source_default_bipolar(source) {
            1.0
        } else {
            0.0
        };
        self.controls
            .insert(format!("modulation_{}_bipolar", index + 1), bipolar);
        Ok(index)
    }

    /// [`SlotValues::dragged_to`] for `destination` at its value in this
    /// state, for sessions whose state holds the live control values (the
    /// offline chat session). A plugin reads the live value from its host
    /// parameter instead. `None` if `destination` is not a modulation
    /// destination.
    pub fn dragged_values(&self, destination: &str, bipolar: bool) -> Option<SlotValues> {
        if !engine::is_modulation_destination(destination) {
            return None;
        }
        let info = registry().get(destination)?;
        let value = self.value(destination).unwrap_or(info.default_value);
        Some(SlotValues::dragged_to(info, value, bipolar))
    }

    /// What upstream's editor does when a source is dropped on a control
    /// (`ModulationManager::modulationDraggedToComponent`): if the two are
    /// not connected yet, `SynthGuiInterface::connectModulation`
    /// ([`add_modulation`](Self::add_modulation), then `initModulationValues`:
    /// the remap curve goes back to linear and the power to 0), then
    /// `setModulationValues` with `values`. An existing connection is left
    /// alone. Every live and offline edit path uses this, so they all pick
    /// the same slot and give it the same values.
    pub fn connect_modulation_from_gui(
        &mut self,
        source: &str,
        destination: &str,
        values: SlotValues,
    ) -> Result<GuiConnection, Error> {
        let created = self.modulation_index(source, destination).is_none();
        let slot = self.add_modulation(source, destination)?;
        let mut parameter_changes = Vec::new();
        if created {
            self.modulations[slot].line_mapping.init_linear();
            let number = slot + 1;
            let flag = |on: bool| if on { 1.0 } else { 0.0 };
            for (suffix, value) in [
                ("power", 0.0),
                ("amount", values.amount),
                ("bipolar", flag(values.bipolar)),
                ("stereo", flag(values.stereo)),
                ("bypass", flag(values.bypass)),
            ] {
                let name = control_name(&format!("modulation_{number}_{suffix}"));
                self.controls.insert(name.to_string(), value);
                parameter_changes.push((name, value));
            }
        }
        Ok(GuiConnection {
            slot,
            created,
            parameter_changes,
        })
    }

    /// A modulation matrix row choosing slot `slot`'s route
    /// (`ModulationMatrixRow::sliderValueChanged`): the slot's old route, if
    /// any, is disconnected (`disconnectModulation(connection)`), then
    /// `route` connected in that same slot (`connectModulation(connection)`).
    /// The slot's controls and remap curve stay as they are, and the route may
    /// repeat another slot's. A route to the slot's own amount is refused
    /// (`isInvalidConnection`) and leaves the slot free. Returns whether the
    /// slot is connected afterwards.
    pub fn set_slot_route(
        &mut self,
        slot: usize,
        route: Option<(&str, &str)>,
    ) -> Result<bool, Error> {
        if slot >= MAX_MODULATION_CONNECTIONS {
            return Err(Error::NoModulationSlot);
        }
        if let Some((source, destination)) = route {
            if !engine::is_modulation_source(source) {
                return Err(Error::UnknownModulationSource(source.to_string()));
            }
            if !engine::is_modulation_destination(destination) {
                return Err(Error::UnknownModulationDestination(destination.to_string()));
            }
        }
        let freed = &mut self.modulations[slot];
        freed.source.clear();
        freed.destination.clear();
        self.connection_order.retain(|&index| index != slot);
        let Some((source, destination)) = route else {
            return Ok(false);
        };
        if destination == format!("modulation_{}_amount", slot + 1) {
            return Ok(false);
        }
        let connected = &mut self.modulations[slot];
        connected.source = source.to_string();
        connected.destination = destination.to_string();
        self.connection_order.push(slot);
        Ok(true)
    }

    /// `SynthBase::disconnectModulation(source, destination)`: frees the slot
    /// (its amount and line mapping are left as they are). Returns the slot
    /// index, or `None` if the two were not connected.
    pub fn remove_modulation(&mut self, source: &str, destination: &str) -> Option<usize> {
        let index = self.modulation_index(source, destination)?;
        self.modulations[index].source.clear();
        self.modulations[index].destination.clear();
        self.connection_order.retain(|&slot| slot != index);
        Some(index)
    }

    /// `SynthBase::clearModulations`.
    pub fn clear_modulations(&mut self) {
        self.connection_order.clear();
        for slot in &mut self.modulations {
            slot.source.clear();
            slot.destination.clear();
            slot.line_mapping.init_linear();
        }
    }

    /// `LoadSave::stateToJson` (`SynthBase::saveToJson`).
    pub fn to_json(&self) -> Value {
        let mut settings = Value::object();
        for (name, value) in &self.controls {
            settings.set(name, *value);
        }
        settings.set("sample", self.sample.to_json());

        let mut modulations = Value::Null;
        for slot in &self.modulations {
            let mut data = Value::object();
            data.set("source", slot.source.as_str());
            data.set("destination", slot.destination.as_str());
            if !slot.line_mapping.is_linear() {
                data.set("line_mapping", slot.line_mapping.to_json());
            }
            modulations.push(data);
        }
        settings.set("modulations", modulations);

        let mut wavetables = Value::Null;
        for wavetable in &self.wavetables {
            wavetables.push(wavetable.to_json());
        }
        settings.set("wavetables", wavetables);

        let mut lfos = Value::Null;
        for lfo in &self.lfos {
            lfos.push(lfo.to_json());
        }
        settings.set("lfos", lfos);

        let mut data = Value::object();
        data.set("synth_version", SYNTH_VERSION);
        data.set("preset_name", self.info.preset_name.as_str());
        data.set("author", self.info.author.as_str());
        data.set("comments", self.info.comments.as_str());
        data.set("preset_style", self.info.style.as_str());
        for i in 0..NUM_MACROS {
            data.set(&format!("macro{}", i + 1), self.info.macro_name(i));
        }
        data.set("settings", settings);
        data
    }

    /// [`to_json`](Self::to_json) as compact text, byte for byte what
    /// upstream's `saveToJson().dump()` writes.
    pub fn to_json_string(&self) -> String {
        self.to_json().dump()
    }

    /// `SynthBase::loadFromJson` (`LoadSave::jsonToState`): loads `data` on
    /// top of the current state. Controls the preset omits take their
    /// defaults; wavetables and LFO shapes it omits keep their current value.
    ///
    /// Upstream applies a failing load partially (the steps before the
    /// failure stay applied); here a failed load leaves the state unchanged.
    pub fn load_json(&mut self, data: &Value) -> Result<(), Error> {
        let mut data = data.clone();
        let version = read_string(&data, "synth_version")?;

        if compare_feature_version_strings(&version, SYNTH_VERSION) > 0 {
            return Err(Error::NewerVersion(version));
        }
        let has_sub_octave = data.get("settings")?.count("sub_octave") != 0;
        if sas_common::load_save::compare_version_strings(&version, SYNTH_VERSION) < 0
            || has_sub_octave
        {
            data = update_from_old_version(data)?;
        }

        let settings = data.get("settings")?;
        ensure_object(settings)?;
        let mut next = self.clone();
        next.load_controls(settings)?;
        next.load_modulations(settings.get("modulations")?)?;
        next.sample = SampleState::from_json(settings.get("sample")?)?;
        // Upstream indexes past the third wavetable and eighth LFO (undefined
        // behaviour); extra entries are ignored here.
        for (i, wavetable) in settings
            .get("wavetables")?
            .iter_values()
            .take(NUM_OSCILLATORS)
            .enumerate()
        {
            next.wavetables[i].read_json(wavetable)?;
        }
        for (i, lfo) in settings
            .get("lfos")?
            .iter_values()
            .take(NUM_LFOS)
            .enumerate()
        {
            next.lfos[i].from_json(lfo)?;
        }
        next.load_save_info(&data)?;
        *self = next;
        Ok(())
    }

    /// Parses `text` and loads it with [`load_json`](Self::load_json).
    pub fn load_json_str(&mut self, text: &str) -> Result<(), Error> {
        let data = json::parse(text)?;
        self.load_json(&data)
    }

    /// `SynthBase::loadFromFile`: loads the file, then names the preset after
    /// the file (without its extension), whatever its `preset_name` says.
    pub fn load_file(&mut self, path: &Path) -> Result<(), Error> {
        let bytes = std::fs::read(path).map_err(|error| Error::Io(error.to_string()))?;
        let data = json::parse_bytes(&bytes)?;
        self.load_json(&data)?;
        self.info.preset_name = file_name_without_extension(&path.to_string_lossy());
        Ok(())
    }

    /// `LoadSave::loadControls`.
    fn load_controls(&mut self, settings: &Value) -> Result<(), Error> {
        for (name, value) in self.controls.iter_mut() {
            *value = if settings.count(name) != 0 {
                read_f32(settings, name)?
            } else {
                default_value(name)
            };
        }
        Ok(())
    }

    /// `LoadSave::loadModulations`. Entries map to slots by position; an
    /// entry whose source or destination the engine does not know (including
    /// empty ones) leaves its slot free with a linear mapping.
    fn load_modulations(&mut self, modulations: &Value) -> Result<(), Error> {
        self.clear_modulations();
        for (index, modulation) in modulations.iter_values().enumerate() {
            // Upstream reads these through a const reference, where a missing
            // key is undefined behaviour; it is an error here.
            let field = |key: &str| -> Result<String, Error> {
                if modulation.count(key) == 0 {
                    return Err(Error::Corrupted(format!("modulation without \"{key}\"")));
                }
                read_string(modulation, key)
            };
            let source = field("source")?;
            let destination = field("destination")?;
            // Slots past the bank are out of range upstream; they are ignored.
            let Some(slot) = self.modulations.get_mut(index) else {
                continue;
            };
            if !engine::is_modulation_source(&source)
                || !engine::is_modulation_destination(&destination)
            {
                continue;
            }
            slot.source = source;
            slot.destination = destination;
            self.connection_order.push(index);
            if modulation.count("line_mapping") != 0 {
                slot.line_mapping
                    .from_json(modulation.get("line_mapping")?)?;
            } else {
                slot.line_mapping.init_linear();
            }
        }
        Ok(())
    }

    /// `LoadSave::loadSaveState`.
    fn load_save_info(&mut self, data: &Value) -> Result<(), Error> {
        let text = |key: &str| -> Result<Option<String>, Error> {
            if data.count(key) != 0 {
                read_string(data, key).map(Some)
            } else {
                Ok(None)
            }
        };
        if let Some(name) = text("preset_name")? {
            self.info.preset_name = name;
        }
        if let Some(author) = text("author")? {
            self.info.author = author;
        }
        if let Some(comments) = text("comments")? {
            self.info.comments = comments;
        }
        if let Some(style) = text("preset_style")? {
            self.info.style = style;
        }
        for i in 0..NUM_MACROS {
            self.info.macros[i] =
                text(&format!("macro{}", i + 1))?.unwrap_or_else(|| default_macro_name(i));
        }
        Ok(())
    }
}

/// The value a control loads when a preset omits it.
fn default_value(name: &str) -> f32 {
    registry().get(name).map_or(0.0, |info| info.default_value)
}

/// `juce::File::getFileNameWithoutExtension`.
fn file_name_without_extension(path: &str) -> String {
    let last_slash = path.rfind(std::path::MAIN_SEPARATOR).map_or(0, |i| i + 1);
    match path.rfind('.') {
        Some(last_dot) if last_dot > last_slash => path[last_slash..last_dot].to_string(),
        _ => path[last_slash..].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_follow_juce() {
        assert_eq!(
            file_name_without_extension("/a/b/init saw.vital"),
            "init saw"
        );
        assert_eq!(file_name_without_extension("/a/b.c/name"), "name");
        assert_eq!(file_name_without_extension("x.y.vital"), "x.y");
        assert_eq!(file_name_without_extension("/a/.vital"), ".vital");
    }

    /// `mod_connections_`: a load connects in slot order, a new connection
    /// goes last even in a lower slot, and a removed one leaves the rest in
    /// place.
    #[test]
    fn connections_keep_the_order_they_were_made_in() {
        let mut state = PresetState::new(&InitContent { sample_seed: 0 });
        state.add_modulation("lfo_1", "osc_1_level").unwrap();
        state.add_modulation("lfo_2", "osc_2_level").unwrap();
        state.add_modulation("env_2", "osc_3_level").unwrap();
        assert_eq!(state.connection_order(), [0, 1, 2]);
        state.remove_modulation("lfo_1", "osc_1_level");
        state.add_modulation("random_1", "filter_1_cutoff").unwrap();
        assert_eq!(state.connection_order(), [1, 2, 0]);

        let mut loaded = PresetState::new(&InitContent { sample_seed: 0 });
        loaded.load_json(&state.to_json()).unwrap();
        assert_eq!(loaded.connection_order(), [0, 1, 2]);
        loaded.clear_modulations();
        assert!(loaded.connection_order().is_empty());
    }
}
