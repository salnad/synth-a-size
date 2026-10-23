//! What the tools act on: a synth state they can read, edit and restore.
//!
//! Every [`Edit`] is one the upstream GUI can also make, so the engine
//! reacts to it exactly as it would to the same edit made by hand. The
//! offline session edits a [`PresetState`]; the plugin's live session (L4 in
//! the LLM plan) maps edits onto host parameter gestures, the GUI's edit bus
//! (G0) and, for whole-preset loads, `GuiContext::set_state`.

use sas_common::LineGenerator;
use sas_preset::{InitContent, PresetState, SaveInfo};
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
    /// The whole current state.
    fn snapshot(&self) -> PresetState;

    /// The current value of control `name`.
    fn control(&self, name: &str) -> Option<f32>;

    /// Applies `edit`.
    fn apply(&mut self, edit: &Edit) -> Result<Applied, Error>;

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

/// `ModulationManager::kDefaultModulationRatio`: the smallest amount a
/// dragged-on modulation starts with.
const DEFAULT_MODULATION_RATIO: f32 = 0.25;

/// What upstream's editor gives a slot it has just created by dragging a
/// source onto a control (`ModulationManager::setModulationValues`).
///
/// G0 (#94) adds the shared form of this rule to `sas-preset`
/// (`SlotValues`, `PresetState::dragged_values`,
/// `PresetState::connect_modulation_from_gui`); this copy follows it
/// exactly and is deleted when the session switches to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotValues {
    /// `modulation_N_amount`.
    pub amount: f32,
    pub bipolar: bool,
    pub stereo: bool,
    pub bypass: bool,
}

/// `ModulationManager::modulationDraggedToComponent`: the values for a
/// source dropped on `destination`'s control at its current value.
/// `bipolar` stands for the modifier key: a plain drag makes a unipolar
/// route. The amount reaches from the control's position to the top of its
/// range (to the nearer end, doubled, when bipolar), at least
/// `kDefaultModulationRatio`; stereo and bypass are off. `None` if
/// `destination` is not a parameter.
pub fn dragged_values(state: &PresetState, destination: &str, bipolar: bool) -> Option<SlotValues> {
    let info = sas_preset::registry().get(destination)?;
    let value = state.value(destination).unwrap_or(info.default_value);
    // juce::Slider::valueToProportionOfLength on the control's linear range,
    // in double as JUCE computes it, then stored as a float.
    let (min, max) = (f64::from(info.min), f64::from(info.max));
    let percent = ((f64::from(value) - min) / (max - min)).clamp(0.0, 1.0) as f32;
    let mut amount = 1.0 - percent;
    if bipolar {
        amount = amount.min(percent) * 2.0;
    }
    Some(SlotValues {
        amount: amount.max(DEFAULT_MODULATION_RATIO),
        bipolar,
        stereo: false,
        bypass: false,
    })
}

/// A source dropped on a control in upstream's editor: if the two are not
/// connected yet, `SynthGuiInterface::connectModulation`
/// (`PresetState::add_modulation`, then `initModulationValues`: linear remap
/// curve, power 0), then `setModulationValues` with `values`. An existing
/// connection is left alone. Returns the slot and whether it was created.
fn connect_from_gui(
    state: &mut PresetState,
    source: &str,
    destination: &str,
    values: SlotValues,
) -> Result<(usize, bool), Error> {
    let created = state.modulation_index(source, destination).is_none();
    let slot = state.add_modulation(source, destination)?;
    if created {
        if let Some(line) = state.line_mapping_mut(slot) {
            line.init_linear();
        }
        let flag = |on: bool| if on { 1.0 } else { 0.0 };
        for (suffix, value) in [
            ("power", 0.0),
            ("amount", values.amount),
            ("bipolar", flag(values.bipolar)),
            ("stereo", flag(values.stereo)),
            ("bypass", flag(values.bypass)),
        ] {
            state.set_value(&format!("modulation_{}_{suffix}", slot + 1), value)?;
        }
    }
    Ok((slot, created))
}

impl SynthSession for OfflineSession {
    fn snapshot(&self) -> PresetState {
        self.state.clone()
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
                let bipolar = settings
                    .iter()
                    .any(|(suffix, value)| *suffix == "bipolar" && *value != 0.0);
                let values = dragged_values(&self.state, destination, bipolar)
                    .ok_or_else(|| Error::Invalid(format!("{destination} is not a parameter")))?;
                let (slot, created) =
                    connect_from_gui(&mut self.state, source, destination, values)?;
                for (suffix, value) in settings {
                    self.state
                        .set_value(&format!("modulation_{}_{suffix}", slot + 1), *value)?;
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
