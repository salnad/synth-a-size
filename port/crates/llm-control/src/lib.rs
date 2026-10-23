// SPDX-License-Identifier: GPL-3.0-or-later

//! LLM control layer: the tools a language model uses to read and edit the
//! synth by name (an addition with no upstream counterpart; see D13 in
//! `migration/DIVERGENCES.md`).
//!
//! - [`catalog`]: the 772 controls, described in display units, and the
//!   templated catalog for the system prompt.
//! - [`session`]: [`SynthSession`], the state the tools act on, and
//!   [`OfflineSession`] over a preset in memory.
//! - [`tools`]: the tool schemas and their handlers.
//! - [`controller`]: undo, edits waiting for the user's confirmation, and
//!   the [`Policy`] behind them.
//! - [`patch`]: the compact patch description `get_patch` returns.
//! - [`shapes`]: LFO shapes and generated wavetables in upstream's formats.
//! - [`library`]: the user's preset folder, the only files the tools touch.
//!
//! Every edit is one the upstream GUI can also make, so the engine is
//! unchanged by this crate. It has no plugin, network or engine dependency.

use std::fmt;

pub mod catalog;
pub mod controller;
pub mod library;
mod names;
pub mod patch;
pub mod session;
pub mod shapes;
pub mod tools;

pub use controller::{Controller, Pending, Policy, Side};
pub use library::PresetLibrary;
pub use sas_preset;
pub use session::{Applied, Edit, OfflineSession, OutsideChange, SynthSession};
pub use tools::{specs, ToolOutput, ToolSpec};

/// Milestone in `migration/PLAN.md` that builds this crate.
pub const MILESTONE: &str = "M9";

/// Upstream files (relative to the repository root) whose behaviour the
/// tools follow.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/common/synth_parameters.cpp",
    "vital/src/common/wavetable/wavetable_creator.cpp",
];

/// Why an edit could not be applied.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Preset(sas_preset::Error),
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Preset(error) => write!(f, "{error}"),
            Error::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

impl From<sas_preset::Error> for Error {
    fn from(error: sas_preset::Error) -> Self {
        Error::Preset(error)
    }
}

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
