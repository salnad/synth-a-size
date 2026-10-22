// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugin state as preset JSON, and preset files.
//!
//! Ports `SynthPlugin::getStateInformation` / `setStateInformation`
//! (`vital/src/plugin/synth_plugin.cpp`): the host state is the preset JSON
//! (`LoadSave::stateToJson`) plus the tuning, and restoring it loads that
//! JSON on top of the current state (`LoadSave::jsonToState`, with the
//! version upgrade chain). Preset files follow `SynthBase::loadFromFile` and
//! `SynthBase::saveToFile` (`vital/src/common/synth_base.cpp`). The tuning is
//! `sas_engine::Tuning` (`Tuning::stateToJson` / `jsonToState`).
//!
//! nice-plug saves every parameter's value next to the persisted fields. The
//! preset JSON in [`PRESET_FIELD`] is the source of truth: [`filter_state`]
//! replaces the saved parameter values with the preset's control values
//! before the wrapper applies them, so states written by other tools (or a
//! bare preset file) restore the same way.

use std::path::{Path, PathBuf};

use nice_plug::plugin::{ParamValue, PluginState};
use sas_common::json::{self, Value};
use sas_engine::Tuning;
use sas_preset::{engine, registry, Error, InitContent, PresetState};

use crate::params::host_kind;

/// The persisted field holding the preset JSON.
pub const PRESET_FIELD: &str = "preset";

/// File extension of the upstream preset format, kept for compatibility.
pub const PRESET_EXTENSION: &str = "vital";

/// Everything the plugin saves: the preset plus the tuning.
#[derive(Clone, Debug)]
pub struct PresetDocument {
    pub state: PresetState,
    /// `SynthBase::tuning_`. Always saved in the plugin state; restoring a
    /// state without one keeps the current tuning, as upstream.
    pub tuning: Tuning,
}

/// `Tuning::stateToJson` as upstream's serializer writes it.
fn tuning_to_json(tuning: &Tuning) -> Value {
    json::parse(&tuning.state_to_json().to_string()).expect("serde_json writes valid JSON")
}

/// The plugin state's `tuning` object loaded into a fresh copy of `current`
/// (`Tuning::jsonToState`), or `None` if the state has none. A `tuning` that
/// is not an object, or that `Tuning::json_to_state` refuses (a keyboard
/// mapping past its scale, D6), is a corrupted state (D4).
fn read_tuning(data: &Value, current: &Tuning) -> Result<Option<Tuning>, Error> {
    if data.count("tuning") == 0 {
        return Ok(None);
    }
    let value = data.get("tuning")?;
    if !value.is_object() {
        return Err(Error::Corrupted("\"tuning\" is not an object".to_string()));
    }
    let value: serde_json::Value = serde_json::from_str(&value.dump())
        .map_err(|error| Error::Corrupted(format!("tuning: {error}")))?;
    let mut tuning = current.clone();
    tuning
        .json_to_state(&value)
        .map_err(|error| Error::Corrupted(format!("tuning: {error}")))?;
    Ok(Some(tuning))
}

impl PresetDocument {
    /// The init preset (`SynthBase::loadInitPreset`).
    pub fn init(init: &InitContent) -> PresetDocument {
        PresetDocument {
            state: PresetState::new(init),
            tuning: Tuning::new(),
        }
    }

    /// `SynthPlugin::getStateInformation`'s JSON.
    pub fn to_json(&self) -> Value {
        let mut data = self.state.to_json();
        data.set("tuning", tuning_to_json(&self.tuning));
        data
    }

    pub fn to_json_string(&self) -> String {
        self.to_json().dump()
    }

    /// `SynthPlugin::setStateInformation`: loads `data` on top of the current
    /// state. On error nothing changes.
    pub fn load_json(&mut self, data: &Value) -> Result<(), Error> {
        let tuning = read_tuning(data, &self.tuning)?;
        self.state.load_json(data)?;
        if let Some(tuning) = tuning {
            self.tuning = tuning;
        }
        Ok(())
    }

    pub fn load_json_str(&mut self, text: &str) -> Result<(), Error> {
        self.load_json(&json::parse(text)?)
    }
}

/// The init content for a throwaway state whose sample is never used.
const SCRATCH_INIT: InitContent = InitContent { sample_seed: 0 };

/// Loads `data` as a plugin state would, onto a throwaway document: the
/// preset's control values, and whether the whole state loads.
fn scratch_load(data: &Value) -> Result<PresetState, Error> {
    let mut scratch = PresetDocument::init(&SCRATCH_INIT);
    scratch.load_json(data)?;
    Ok(scratch.state)
}

/// `Plugin::filter_state`: makes the preset JSON authoritative for parameter
/// values. A state without a preset is left alone; one whose preset cannot be
/// loaded (corrupted, or from a newer version) is emptied, so restoring it
/// changes nothing (upstream reports the error and keeps going).
pub fn filter_state(state: &mut PluginState) {
    let Some(text) = state.fields.get(PRESET_FIELD) else {
        return;
    };
    match json::parse(text)
        .map_err(Error::from)
        .and_then(|data| scratch_load(&data))
    {
        Ok(scratch) => state.params = parameter_values(&scratch),
        Err(error) => {
            nice_plug::nice_warn!("ignoring a saved state whose preset cannot load: {error}");
            state.params.clear();
            state.fields.clear();
        }
    }
}

/// The saved parameter values (`PluginState::params`) of `preset`.
pub fn parameter_values(preset: &PresetState) -> std::collections::BTreeMap<String, ParamValue> {
    engine::controls()
        .into_iter()
        .map(|name| {
            let value = preset.value(name).expect("every control has a value");
            let info = registry()
                .get(name)
                .expect("every control is in the registry");
            (name.to_string(), host_kind(info).param_value(value))
        })
        .collect()
}

/// A plugin state that loads the preset JSON `text` (from a file or the LLM
/// layer) on top of the current state, for `GuiContext::set_state`.
pub fn plugin_state_from_json(text: &str) -> Result<PluginState, Error> {
    let scratch = scratch_load(&json::parse(text)?)?;
    Ok(PluginState {
        version: env!("CARGO_PKG_VERSION").to_string(),
        params: parameter_values(&scratch),
        fields: [(PRESET_FIELD.to_string(), text.to_string())].into(),
    })
}

/// `SynthBase::loadFromFile`: a plugin state that loads the preset file at
/// `path`, named after the file whatever its `preset_name` says. Like
/// upstream, a preset file never changes the tuning, even if it has one.
pub fn plugin_state_from_file(path: &Path) -> Result<PluginState, Error> {
    let bytes = std::fs::read(path).map_err(|error| Error::Io(error.to_string()))?;
    let mut data = json::parse_bytes(&bytes)?;
    PresetState::new(&SCRATCH_INIT).load_json(&data)?;
    if let Value::Object(fields) = &mut data {
        fields.remove("tuning");
    }
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    data.set("preset_name", stem);
    plugin_state_from_json(&data.dump())
}

/// `SynthBase::saveToFile`: writes `preset` (without the tuning) to `path`
/// with the preset extension, naming the preset after the file. Returns the
/// path written.
pub fn save_preset_file(preset: &PresetState, path: &Path) -> Result<PathBuf, Error> {
    let path = path.with_extension(PRESET_EXTENSION);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| Error::Io(error.to_string()))?;
    }
    let mut preset = preset.clone();
    preset.info.preset_name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    std::fs::write(&path, preset.to_json_string()).map_err(|error| Error::Io(error.to_string()))?;
    Ok(path)
}
