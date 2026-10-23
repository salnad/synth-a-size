// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Everything a plugin saves: the preset plus the tuning.
//!
//! Ports the JSON of `SynthPlugin::getStateInformation` /
//! `setStateInformation` (`vital/src/plugin/synth_plugin.cpp`): the preset
//! JSON (`LoadSave::stateToJson`) plus the tuning (`Tuning::stateToJson`),
//! and restoring it on top of the current state (`LoadSave::jsonToState`,
//! with the version upgrade chain). It lives in the engine crate so the
//! editor and the chat layer can read the plugin's document without
//! depending on the plugin.

use sas_common::json::{self, Value};
use sas_preset::{Error, InitContent, PresetState};

use crate::tuning::Tuning;

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
