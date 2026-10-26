// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Parameter registry and preset state, with load/save of the upstream preset
//! JSON format (`.vital` files) including the version upgrade chain.
//!
//! - [`registry`]: every parameter (name, range, default, value scale,
//!   display details, version added), ported from
//!   `vital/src/common/synth_parameters.cpp`.
//! - [`engine`]: which parameters the engine instantiates as controls, which
//!   accept modulation, and the modulation sources.
//! - [`PresetState`]: control values, modulation slots, sample, wavetables,
//!   LFO shapes and save info, loaded and saved like
//!   `vital/src/common/load_save.cpp`, with by-name access for the plugin
//!   and the LLM control layer.
//!
//! This crate is pure data: it depends only on `sas-common` and the
//! wavetable state model of `sas-wavetable`, not on the DSP crates.
//!
//! ```
//! use sas_preset::{lookup, InitContent, PresetState};
//!
//! let init = InitContent { sample_seed: 0 };
//! let mut preset = PresetState::new(&init);
//! assert_eq!(lookup("filter_1_cutoff").unwrap().max, 136.0);
//! preset.set_value("filter_1_cutoff", 72.0).unwrap();
//! let slot = preset.add_modulation("lfo_1", "filter_1_cutoff").unwrap();
//! preset.set_value(&format!("modulation_{}_amount", slot + 1), 0.25).unwrap();
//! let saved = preset.to_json_string();
//! assert!(saved.contains("\"source\":\"lfo_1\""));
//! ```

pub mod constants;
pub mod display;
pub mod engine;
mod json;
pub mod registry;
pub mod sample;
mod state;
pub mod strings;
pub mod upgrade;
pub mod user_folders;
pub mod version;

pub use registry::{lookup, registry, ParamInfo, Registry, ValueScale};
pub use sample::SampleState;
pub use state::{GuiConnection, InitContent, ModulationSlot, PresetState, SaveInfo, SlotValues};
pub use strings::StringTable;

/// Why a preset could not be loaded or a change could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The preset's feature version is newer than this build's
    /// ("Preset was created with a newer version." upstream).
    NewerVersion(String),
    /// The JSON does not have the shape the loader needs ("Preset file is
    /// corrupted." upstream).
    Corrupted(String),
    /// The file could not be read.
    Io(String),
    UnknownParameter(String),
    /// A registry parameter the engine has no control for (legacy
    /// parameters kept only for upgrades).
    NotAControl(String),
    UnknownModulationSource(String),
    UnknownModulationDestination(String),
    /// Every modulation slot is in use.
    NoModulationSlot,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NewerVersion(version) => {
                write!(f, "preset was created with a newer version ({version})")
            }
            Error::Corrupted(detail) => write!(f, "preset file is corrupted: {detail}"),
            Error::Io(detail) => write!(f, "cannot read preset: {detail}"),
            Error::UnknownParameter(name) => write!(f, "unknown parameter {name}"),
            Error::NotAControl(name) => {
                write!(f, "{name} is a legacy parameter with no engine control")
            }
            Error::UnknownModulationSource(name) => write!(f, "unknown modulation source {name}"),
            Error::UnknownModulationDestination(name) => {
                write!(f, "{name} is not a modulation destination")
            }
            Error::NoModulationSlot => write!(f, "no free modulation slot"),
        }
    }
}

impl std::error::Error for Error {}

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M2";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/common/synth_parameters.cpp",
    "vital/src/common/synth_parameters.h",
    "vital/src/common/load_save.cpp",
    "vital/src/common/synth_base.cpp",
    "vital/src/common/synth_constants.h",
    "vital/src/common/synth_types.cpp",
    "vital/src/interface/look_and_feel/synth_strings.h",
    "vital/src/synthesis/producers/sample_source.cpp",
    "vital/src/synthesis/framework/utils.cpp",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_sources_exist() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        for source in UPSTREAM_SOURCES {
            assert!(repo.join(source).exists(), "missing upstream path {source}");
        }
        assert!(MILESTONE.starts_with('M'));
    }
}
