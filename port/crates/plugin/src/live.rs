// SPDX-License-Identifier: GPL-3.0-or-later

//! The LLM tools' session on a running plugin (L4 of the LLM plan; an
//! addition with no upstream counterpart, D13 in `migration/DIVERGENCES.md`).
//!
//! [`LiveSession`] implements `sas_llm_control::SynthSession` the way the
//! editor changes the synth:
//!
//! - a control changes through its host parameter, as one gesture (begin,
//!   set, end), so the host records automation if armed and saves the value;
//! - a modulation route, an LFO shape or a wavetable changes through the edit
//!   bus (`sas_engine::edit_bus`). A created slot's final values (upstream's
//!   drag values with the model's explicit settings on top) travel with its
//!   connection, and its `parameter_changes` go to the host parameters;
//! - a whole preset (a preset file, the init patch, or an undo that changes
//!   the routing) goes through `GuiContext::set_state`, as the editor loads a
//!   preset.
//!
//! nice-plug's `GuiContext` may only be used on the GUI thread, so the
//! session lives there; the chat worker reaches it through
//! [`crate::chat_bridge`].

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use nice_plug::context::gui::GuiContext;
use nice_plug::plugin::PluginState;
use sas_engine::edit_bus::{EditError, EditHandle, PreparedWavetable};
use sas_llm_control::session::created_slot_values;
use sas_llm_control::{Applied, Edit, Error, OutsideChange, SynthSession};
use sas_preset::{engine, registry, PresetState, SlotValues};

use crate::params::{Control, ControlParam, SynthParams};
use crate::state;

/// How a [`LiveSession`] tells the host about changes: in the plugin, the
/// editor's `GuiContext`.
pub trait HostWriter {
    /// Sets `control` to engine value `value` as one gesture (begin, set,
    /// end).
    fn set_control(&self, control: &Control, value: f32);

    /// Replaces the whole plugin state (`GuiContext::set_state`).
    fn set_state(&self, state: PluginState);
}

impl HostWriter for GuiContext {
    fn set_control(&self, control: &Control, value: f32) {
        let setter = self.param_setter();
        match &control.param {
            ControlParam::Float(param) => {
                setter.begin_set_parameter(param);
                setter.set_parameter(param, value);
                setter.end_set_parameter(param);
            }
            ControlParam::Stepped(param) => {
                setter.begin_set_parameter(param);
                setter.set_parameter(param, value.round() as i32);
                setter.end_set_parameter(param);
            }
            ControlParam::Bypass(param) => {
                setter.begin_set_parameter(param);
                setter.set_parameter(param, value != 0.0);
                setter.end_set_parameter(param);
            }
        }
    }

    fn set_state(&self, state: PluginState) {
        GuiContext::set_state(self, state);
    }
}

/// Tries per edit bus call before giving up on a full queue. The queue only
/// fills while the audio thread is not processing.
const QUEUE_RETRIES: usize = 50;
const QUEUE_RETRY_DELAY: Duration = Duration::from_millis(10);

/// Calls `edit` until the bus has room, as `sas_engine::edit_bus` asks.
fn with_room<R>(mut edit: impl FnMut() -> Result<R, EditError>) -> Result<R, Error> {
    for _ in 0..QUEUE_RETRIES {
        match edit() {
            Err(EditError::QueueFull) => std::thread::sleep(QUEUE_RETRY_DELAY),
            result => return result.map_err(|error| Error::Invalid(error.to_string())),
        }
    }
    Err(Error::Invalid(
        "the synth is not processing audio, so the edit cannot reach it".to_string(),
    ))
}

/// The tools' session on the plugin's parameters and edit bus.
pub struct LiveSession<W: HostWriter> {
    params: Arc<SynthParams>,
    writer: W,
    /// The engine values the tools last set, to notice outside changes.
    expected: BTreeMap<&'static str, f32>,
    /// The edit bus generation last seen.
    generation: u64,
    /// Whole-state replacements this session made that have not shown up as
    /// a generation change yet.
    own_replacements: u64,
}

impl<W: HostWriter> LiveSession<W> {
    pub fn new(params: Arc<SynthParams>, writer: W) -> LiveSession<W> {
        let generation = params.edit_handle().generation();
        LiveSession {
            params,
            writer,
            expected: BTreeMap::new(),
            generation,
            own_replacements: 0,
        }
    }

    /// The host writer, e.g. to check what reached the host in tests.
    pub fn writer(&self) -> &W {
        &self.writer
    }

    fn edits(&self) -> &EditHandle {
        self.params.edit_handle()
    }

    fn control_param(&self, name: &str) -> Result<&Control, Error> {
        self.params
            .control(name)
            .ok_or_else(|| Error::Invalid(format!("{name} is not a control")))
    }

    /// Sets a control as one host gesture and remembers it.
    fn set_control(&mut self, name: &str, value: f32) -> Result<(), Error> {
        let control = self.control_param(name)?;
        self.writer.set_control(control, value);
        let held = match control.param {
            ControlParam::Float(_) => value,
            ControlParam::Stepped(_) => value.round(),
            ControlParam::Bypass(_) => f32::from(value != 0.0),
        };
        self.expected.insert(control.info.name.as_str(), held);
        Ok(())
    }

    /// Replaces the whole state through the host, as the editor loads a
    /// preset. The tuning stays (a preset never changes it).
    fn replace(&mut self, preset: &PresetState) -> Result<(), Error> {
        let state = state::plugin_state_from_json(&preset.to_json_string())
            .map_err(|error| Error::Invalid(format!("cannot load the preset: {error}")))?;
        self.writer.set_state(state);
        self.own_replacements += 1;
        self.expected.clear();
        Ok(())
    }

    fn connect(
        &mut self,
        source: &str,
        destination: &str,
        settings: &[(&'static str, f32)],
    ) -> Result<Applied, Error> {
        let live = self.control(destination);
        let (values, power) = created_slot_values(settings, |bipolar| {
            let info = registry().get(destination)?;
            engine::is_modulation_destination(destination)
                .then(|| SlotValues::dragged_to(info, live.unwrap_or(info.default_value), bipolar))
        })
        .ok_or_else(|| Error::Invalid(format!("{destination} is not a modulatable control")))?;
        let edits = self.edits().clone();
        let connection = with_room(|| edits.connect_modulation(source, destination, values))?;
        for (name, value) in &connection.parameter_changes {
            self.set_control(name, *value)?;
        }
        let number = connection.slot + 1;
        let later: Vec<(&str, f32)> = if connection.created {
            power.map(|power| ("power", power)).into_iter().collect()
        } else {
            settings.to_vec()
        };
        for (suffix, value) in later {
            self.set_control(&format!("modulation_{number}_{suffix}"), value)?;
        }
        Ok(Applied::Connected {
            slot: connection.slot,
            created: connection.created,
        })
    }
}

/// Whether a control reads `now` where the tools set `set`: equal within
/// the rounding of a host parameter's normalized value.
fn same_value(name: &str, set: f32, now: f32) -> bool {
    let range = registry().get(name).map_or(1.0, |info| info.max - info.min);
    (set - now).abs() <= 1e-5 * range.abs().max(1.0)
}

impl<W: HostWriter> SynthSession for LiveSession<W> {
    fn snapshot(&self) -> PresetState {
        self.params.preset_state()
    }

    fn control(&self, name: &str) -> Option<f32> {
        self.params
            .control(name)
            .map(|control| control.param.saved_value())
    }

    fn apply(&mut self, edit: &Edit) -> Result<Applied, Error> {
        let edits = self.edits().clone();
        match edit {
            Edit::SetControl { name, value } => self.set_control(name, *value)?,
            Edit::ConnectModulation {
                source,
                destination,
                settings,
            } => return self.connect(source, destination, settings),
            Edit::DisconnectModulation {
                source,
                destination,
            } => {
                with_room(|| edits.disconnect_modulation(source, destination))?.ok_or_else(
                    || Error::Invalid(format!("{source} is not connected to {destination}")),
                )?;
            }
            Edit::SetLfoShape { lfo, line } => {
                with_room(|| edits.set_lfo_shape(*lfo, (**line).clone()))?;
            }
            Edit::SetWavetable {
                oscillator,
                creator,
            } => {
                // Rendered on this thread before the batch, as the bus asks;
                // again on a retry, since a refused batch drops it.
                with_room(|| {
                    let prepared = PreparedWavetable::render((**creator).clone())?;
                    edits.batch(|batch| batch.set_prepared_wavetable(*oscillator, prepared))
                })?;
            }
            Edit::LoadPreset(preset) => self.replace(preset)?,
            Edit::SetInfo(info) => {
                let mut preset = self.snapshot();
                preset.info = (**info).clone();
                self.replace(&preset)?;
            }
            _ => {
                return Err(Error::Invalid(
                    "this edit is not supported live".to_string(),
                ))
            }
        }
        Ok(Applied::Done)
    }

    /// Undo and A/B compare. Controls, LFO shapes, remap curves and
    /// wavetables return as edits (the bus applies its part within one
    /// block); a different routing, sample or save info reloads `state`
    /// whole through the host.
    fn restore(&mut self, target: &PresetState) -> Result<(), Error> {
        let current = self.snapshot();
        let same_routes = current
            .modulations()
            .iter()
            .zip(target.modulations())
            .all(|(a, b)| a.source == b.source && a.destination == b.destination);
        let same_sample = current.sample.to_json().dump() == target.sample.to_json().dump();
        let same_info = current.info.preset_name == target.info.preset_name
            && current.info.author == target.info.author
            && current.info.comments == target.info.comments
            && current.info.style == target.info.style
            && current.info.macros == target.info.macros;
        if !(same_routes && same_sample && same_info) {
            return self.replace(target);
        }

        let wavetables: Vec<_> = current
            .wavetables
            .iter()
            .zip(&target.wavetables)
            .enumerate()
            .filter(|(_, (now, then))| now.to_json().dump() != then.to_json().dump())
            .map(|(oscillator, (_, then))| (oscillator, then.clone()))
            .collect();
        let lfos: Vec<_> = current
            .lfos
            .iter()
            .zip(&target.lfos)
            .enumerate()
            .filter(|(_, (now, then))| now.to_json().dump() != then.to_json().dump())
            .map(|(index, (_, then))| (index, then.clone()))
            .collect();
        let curves: Vec<_> = current
            .modulations()
            .iter()
            .zip(target.modulations())
            .enumerate()
            .filter(|(_, (now, then))| {
                now.line_mapping.to_json().dump() != then.line_mapping.to_json().dump()
            })
            .map(|(slot, (_, then))| (slot, then.line_mapping.clone()))
            .collect();
        if !(wavetables.is_empty() && lfos.is_empty() && curves.is_empty()) {
            let edits = self.edits().clone();
            with_room(|| {
                let prepared = wavetables
                    .iter()
                    .map(|(oscillator, creator)| {
                        Ok((*oscillator, PreparedWavetable::render(creator.clone())?))
                    })
                    .collect::<Result<Vec<_>, EditError>>()?;
                edits.batch(|batch| {
                    for (lfo, line) in &lfos {
                        batch.set_lfo_shape(*lfo, line.clone())?;
                    }
                    for (slot, line) in &curves {
                        batch.set_line_mapping(*slot, line.clone())?;
                    }
                    for (oscillator, prepared) in prepared {
                        batch.set_prepared_wavetable(oscillator, prepared)?;
                    }
                    Ok(())
                })
            })?;
        }
        for (name, value) in target.controls() {
            let Some(control) = self.params.control(name) else {
                continue;
            };
            if !control.param.matches(*value, control.param.saved_value()) {
                self.set_control(name, *value)?;
            }
        }
        Ok(())
    }

    fn take_outside_changes(&mut self) -> Vec<OutsideChange> {
        if self.generation != self.edits().generation() {
            // A replaced state is reported by `take_replaced`.
            return Vec::new();
        }
        let changed: Vec<OutsideChange> = self
            .expected
            .iter()
            .filter_map(|(&name, &set)| {
                let now = self.control(name)?;
                (!same_value(name, set, now)).then_some(OutsideChange { name, set, now })
            })
            .collect();
        for change in &changed {
            self.expected.remove(change.name);
        }
        changed
    }

    fn take_replaced(&mut self) -> bool {
        let generation = self.edits().generation();
        if generation == self.generation {
            return false;
        }
        let bumps = generation.saturating_sub(self.generation);
        self.generation = generation;
        let own = bumps.min(self.own_replacements);
        self.own_replacements -= own;
        if bumps > own {
            self.expected.clear();
            true
        } else {
            false
        }
    }
}
