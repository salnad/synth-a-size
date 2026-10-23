//! What the tools act on: a synth state they can read, edit and restore.
//!
//! Every [`Edit`] is one the upstream GUI can also make, so the engine
//! reacts to it exactly as it would to the same edit made by hand. The
//! offline session edits a [`PresetState`]; the plugin's live session (L4 in
//! the LLM plan) maps edits onto host parameter gestures, the GUI's edit bus
//! (G0) and, for whole-preset loads, `GuiContext::set_state`.

use sas_common::LineGenerator;
use sas_preset::{InitContent, PresetState, SaveInfo, SlotValues};
use sas_wavetable::creator::WavetableCreator;

use crate::Error;

/// One edit, already validated against the registry. Control values are
/// engine values.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Edit {
    /// Sets a control (a host parameter).
    SetControl { name: &'static str, value: f32 },
    /// Connects `source` to `destination` as upstream's GUI does
    /// (`SynthGuiInterface::connectModulation`), then sets the slot's own
    /// controls: `settings` pairs a `modulation_N_` suffix (`amount`,
    /// `bipolar`, ...) with an engine value.
    ConnectModulation {
        source: String,
        destination: String,
        settings: Vec<(&'static str, f32)>,
    },
    /// `SynthGuiInterface::disconnectModulation`: frees the slot.
    DisconnectModulation { source: String, destination: String },
    /// Replaces LFO `lfo`'s shape (0-based).
    SetLfoShape {
        lfo: usize,
        line: Box<LineGenerator>,
    },
    /// Replaces oscillator `oscillator`'s wavetable (0-based).
    SetWavetable {
        oscillator: usize,
        creator: Box<WavetableCreator>,
    },
    /// Replaces the whole preset (a preset file or the init patch).
    LoadPreset(Box<PresetState>),
    /// Replaces the preset's name, author, comments, style and macro names.
    SetInfo(Box<SaveInfo>),
}

/// What applying an edit did, when the tools need to report it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    Done,
    /// The connection's 0-based slot, and whether it was created (rather
    /// than reused).
    Connected {
        slot: usize,
        created: bool,
    },
}

/// A synth state the tools can read and change.
pub trait SynthSession {
    /// The whole current state. Fails if it cannot be read (a remote
    /// session whose editor does not answer); undo steps and batch
    /// rollbacks are never built from a stale copy.
    fn snapshot(&self) -> Result<PresetState, Error>;

    /// The current value of control `name`.
    fn control(&self, name: &str) -> Option<f32>;

    /// Applies `edit`.
    fn apply(&mut self, edit: &Edit) -> Result<Applied, Error>;

    /// Returns to `state` (undo).
    fn restore(&mut self, state: &PresetState) -> Result<(), Error>;

    /// Controls the tools set that have since changed outside the chat
    /// (host automation, or the user turning the knob), each reported once.
    fn take_outside_changes(&mut self) -> Vec<OutsideChange> {
        Vec::new()
    }

    /// Whether the whole state was replaced outside the chat (a host state
    /// restore or a preset load) since the last call. Undo steps from before
    /// it no longer apply.
    fn take_replaced(&mut self) -> bool {
        false
    }
}

/// A control the tools set that has since changed outside the chat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutsideChange {
    pub name: &'static str,
    /// The engine value the tools set.
    pub set: f32,
    /// The engine value it has now.
    pub now: f32,
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

/// The values a slot takes when a connection creates it, and any power
/// setting: upstream's drag values for the destination
/// (`ModulationManager::modulationDraggedToComponent`, computed by
/// `dragged` for the bipolar flag the model gave, unipolar by default), with
/// the model's explicit amount, stereo and bypass on top. They travel with
/// the connection (`PresetState::connect_modulation_from_gui`, the edit
/// bus's `connect_modulation`) instead of following it as separate sets.
/// Power is not a slot value; the connection sets it to 0, and a power the
/// model gave is set after.
pub fn created_slot_values(
    settings: &[(&'static str, f32)],
    dragged: impl FnOnce(bool) -> Option<SlotValues>,
) -> Option<(SlotValues, Option<f32>)> {
    let setting = |name: &str| {
        settings
            .iter()
            .find(|(suffix, _)| *suffix == name)
            .map(|(_, value)| *value)
    };
    let mut values = dragged(setting("bipolar").is_some_and(|value| value != 0.0))?;
    if let Some(amount) = setting("amount") {
        values.amount = amount;
    }
    if let Some(stereo) = setting("stereo") {
        values.stereo = stereo != 0.0;
    }
    if let Some(bypass) = setting("bypass") {
        values.bypass = bypass != 0.0;
    }
    Some((values, setting("power")))
}

impl SynthSession for OfflineSession {
    fn snapshot(&self) -> Result<PresetState, Error> {
        Ok(self.state.clone())
    }

    fn control(&self, name: &str) -> Option<f32> {
        self.state.value(name)
    }

    fn apply(&mut self, edit: &Edit) -> Result<Applied, Error> {
        match edit {
            Edit::SetControl { name, value } => self.state.set_value(name, *value)?,
            Edit::ConnectModulation {
                source,
                destination,
                settings,
            } => {
                let (values, power) = created_slot_values(settings, |bipolar| {
                    self.state.dragged_values(destination, bipolar)
                })
                .ok_or_else(|| {
                    Error::Invalid(format!("{destination} is not a modulatable control"))
                })?;
                let connection =
                    self.state
                        .connect_modulation_from_gui(source, destination, values)?;
                let (slot, created) = (connection.slot, connection.created);
                // An existing route keeps its slot values unless the model
                // set them; a created one already has them.
                let later: Vec<(&str, f32)> = if created {
                    power.map(|power| ("power", power)).into_iter().collect()
                } else {
                    settings.to_vec()
                };
                for (suffix, value) in later {
                    self.state
                        .set_value(&format!("modulation_{}_{suffix}", slot + 1), value)?;
                }
                return Ok(Applied::Connected { slot, created });
            }
            Edit::DisconnectModulation {
                source,
                destination,
            } => {
                self.state
                    .remove_modulation(source, destination)
                    .ok_or_else(|| {
                        Error::Invalid(format!("{source} is not connected to {destination}"))
                    })?;
            }
            Edit::SetLfoShape { lfo, line } => {
                let slot = self
                    .state
                    .lfos
                    .get_mut(*lfo)
                    .ok_or_else(|| Error::Invalid(format!("no LFO {}", lfo + 1)))?;
                *slot = (**line).clone();
            }
            Edit::SetWavetable {
                oscillator,
                creator,
            } => {
                let slot =
                    self.state.wavetables.get_mut(*oscillator).ok_or_else(|| {
                        Error::Invalid(format!("no oscillator {}", oscillator + 1))
                    })?;
                *slot = (**creator).clone();
            }
            Edit::LoadPreset(state) => self.state = (**state).clone(),
            Edit::SetInfo(info) => self.state.info = (**info).clone(),
        }
        Ok(Applied::Done)
    }

    fn restore(&mut self, state: &PresetState) -> Result<(), Error> {
        self.state = state.clone();
        Ok(())
    }
}
