// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugin state as preset JSON, and preset files.
//!
//! Ports `SynthPlugin::getStateInformation` / `setStateInformation`
//! (`vital/src/plugin/synth_plugin.cpp`): the host state is the preset JSON
//! (`LoadSave::stateToJson`) plus the tuning, and restoring it loads that
//! JSON on top of the current state (`LoadSave::jsonToState`, with the
//! version upgrade chain). Preset files follow `SynthBase::loadFromFile` and
//! `SynthBase::saveToFile` (`vital/src/common/synth_base.cpp`).
//!
//! nice-plug saves every parameter's value next to the persisted fields. The
//! preset JSON in [`PRESET_FIELD`] is the source of truth: [`filter_state`]
//! replaces the saved parameter values with the preset's control values
//! before the wrapper applies them, so states written by other tools (or a
//! bare preset file) restore the same way.

use std::path::{Path, PathBuf};

use nice_plug::plugin::{ParamValue, PluginState};
use sas_common::json::{self, Value};
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
    /// The `tuning` object of the plugin state (`Tuning::stateToJson`), kept
    /// as JSON until the engine's tuning state is ported. Restoring a state
    /// without one keeps the current tuning, as upstream.
    pub tuning: Option<Value>,
}

impl PresetDocument {
    /// The init preset (`SynthBase::loadInitPreset`).
    pub fn init(init: &InitContent) -> PresetDocument {
        PresetDocument {
            state: PresetState::new(init),
            tuning: None,
        }
    }

    /// `SynthPlugin::getStateInformation`'s JSON.
    pub fn to_json(&self) -> Value {
        let mut data = self.state.to_json();
        if let Some(tuning) = &self.tuning {
            data.set("tuning", tuning.clone());
        }
        data
    }

    pub fn to_json_string(&self) -> String {
        self.to_json().dump()
    }

    /// `SynthPlugin::setStateInformation`: loads `data` on top of the current
    /// state. On error nothing changes.
    pub fn load_json(&mut self, data: &Value) -> Result<(), Error> {
        self.state.load_json(data)?;
        if data.count("tuning") != 0 {
            self.tuning = Some(data.get("tuning")?.clone());
        }
        Ok(())
    }

    pub fn load_json_str(&mut self, text: &str) -> Result<(), Error> {
        self.load_json(&json::parse(text)?)
    }
}

/// The init content for a throwaway state whose sample is never used.
const SCRATCH_INIT: InitContent = InitContent { sample_seed: 0 };

/// `Plugin::filter_state`: makes the preset JSON authoritative for parameter
/// values. A state without a preset is left alone; one whose preset cannot be
/// loaded (corrupted, or from a newer version) is emptied, so restoring it
/// changes nothing (upstream reports the error and keeps going).
pub fn filter_state(state: &mut PluginState) {
    let Some(text) = state.fields.get(PRESET_FIELD) else {
        return;
    };
    let mut scratch = PresetState::new(&SCRATCH_INIT);
    match scratch.load_json_str(text) {
        Ok(()) => state.params = parameter_values(&scratch),
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
    let mut scratch = PresetState::new(&SCRATCH_INIT);
    scratch.load_json_str(text)?;
    Ok(PluginState {
        version: env!("CARGO_PKG_VERSION").to_string(),
        params: parameter_values(&scratch),
        fields: [(PRESET_FIELD.to_string(), text.to_string())].into(),
    })
}

/// `SynthBase::loadFromFile`: a plugin state that loads the preset file at
/// `path`, named after the file whatever its `preset_name` says.
pub fn plugin_state_from_file(path: &Path) -> Result<PluginState, Error> {
    let bytes = std::fs::read(path).map_err(|error| Error::Io(error.to_string()))?;
    let mut data = json::parse_bytes(&bytes)?;
    let mut scratch = PresetState::new(&SCRATCH_INIT);
    scratch.load_json(&data)?;
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
