//! What the tools act on: a synth state they can read, edit and restore.
//!
//! Every [`Edit`] is one the upstream GUI can also make, so the engine
//! reacts to it exactly as it would to the same edit made by hand. The
//! offline session edits a [`PresetState`]; the plugin's live session (L4 in
//! the LLM plan) maps edits onto host parameter gestures and the GUI's edit
//! bus (G0).

use sas_preset::{InitContent, PresetState};

use crate::Error;

/// One edit, already validated against the registry. Control values are
/// engine values.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Edit {
    /// Sets a control (a host parameter).
    SetControl { name: &'static str, value: f32 },
}

/// A synth state the tools can read and change.
pub trait SynthSession {
    /// The whole current state.
    fn snapshot(&self) -> PresetState;

    /// The current value of control `name`.
    fn control(&self, name: &str) -> Option<f32>;

    /// Applies `edit`.
    fn apply(&mut self, edit: &Edit) -> Result<(), Error>;

    /// Returns to `state` (undo).
    fn restore(&mut self, state: &PresetState) -> Result<(), Error>;
}

/// A session over a preset in memory, with no engine: the CLI, the MCP
/// server and tests use it.
#[derive(Clone, Debug)]
pub struct OfflineSession {
    state: PresetState,
}

impl OfflineSession {
    pub fn new(state: PresetState) -> OfflineSession {
        OfflineSession { state }
    }

    /// The init preset, with the white-noise sample from seed 0.
    pub fn init() -> OfflineSession {
        OfflineSession::new(PresetState::new(&InitContent { sample_seed: 0 }))
    }

    pub fn state(&self) -> &PresetState {
        &self.state
    }

    pub fn into_state(self) -> PresetState {
        self.state
    }
}

impl SynthSession for OfflineSession {
    fn snapshot(&self) -> PresetState {
        self.state.clone()
    }

    fn control(&self, name: &str) -> Option<f32> {
        self.state.value(name)
    }

    fn apply(&mut self, edit: &Edit) -> Result<(), Error> {
        match edit {
            Edit::SetControl { name, value } => self.state.set_value(name, *value)?,
        }
        Ok(())
    }

    fn restore(&mut self, state: &PresetState) -> Result<(), Error> {
        self.state = state.clone();
        Ok(())
    }
}
