// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor's access to host parameters.
//!
//! Upstream's controls change a value through `SynthBase::valueChanged` and
//! tell the host with `beginChangeGesture` / `endChangeGesture`. Here every
//! change goes through nice-plug's `GuiContext`, so the host records a normal
//! user gesture (automation, undo, saving the project).

use std::collections::BTreeSet;
use std::sync::Mutex;

use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::ParamPtr;
use sas_preset::registry::{registry, ParamInfo};

use crate::interface::EditorModel;

/// Reads and writes the synth's controls by registry name.
pub struct Host<'a> {
    model: &'a dyn EditorModel,
    context: Option<&'a GuiContext>,
    open: Option<&'a OpenGestures>,
}

/// The controls with a host gesture in progress, kept across frames so the
/// editor can end them when it closes.
pub type OpenGestures = Mutex<BTreeSet<String>>;

impl<'a> Host<'a> {
    /// `context` is the plugin's GUI context; without one (offscreen
    /// rendering) values are read but changes are dropped.
    pub fn new(model: &'a dyn EditorModel, context: Option<&'a GuiContext>) -> Host<'a> {
        Host {
            model,
            context,
            open: None,
        }
    }

    /// Like [`new`](Self::new), recording open gestures in `open`.
    pub fn tracking(
        model: &'a dyn EditorModel,
        context: Option<&'a GuiContext>,
        open: &'a OpenGestures,
    ) -> Host<'a> {
        Host {
            model,
            context,
            open: Some(open),
        }
    }

    /// Ends every gesture still open (the editor is closing mid-drag).
    pub fn end_open_gestures(&self) {
        let Some(open) = self.open else {
            return;
        };
        let names = std::mem::take(&mut *lock(open));
        for name in names {
            self.end(&name);
        }
    }

    pub fn model(&self) -> &'a dyn EditorModel {
        self.model
    }

    /// The registry entry for control `name`.
    pub fn info(name: &str) -> &'static ParamInfo {
        registry()
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not a registry parameter"))
    }

    /// The control's value (the engine value the user set, before any host
    /// modulation), or its default when the plugin has no such parameter.
    pub fn value(&self, name: &str) -> f32 {
        match self.model.param(name) {
            // SAFETY: `EditorModel::param` returns pointers that stay valid
            // while the model is alive, and the model outlives this host.
            Some(param) => unsafe { param.unmodulated_plain_value() },
            None => Host::info(name).default_value,
        }
    }

    /// `SynthBase::beginChangeGesture`.
    pub fn begin(&self, name: &str) {
        if let Some(open) = self.open {
            lock(open).insert(name.to_string());
        }
        if let (Some(context), Some(param)) = (self.context, self.model.param(name)) {
            // SAFETY: as in `value`.
            unsafe { context.raw_begin_set_parameter(param) };
        }
    }

    /// `SynthBase::valueChanged`: sets control `name` to engine value
    /// `value`. Call between [`begin`](Self::begin) and [`end`](Self::end).
    pub fn set(&self, name: &str, value: f32) {
        if let (Some(context), Some(param)) = (self.context, self.model.param(name)) {
            // SAFETY: as in `value`.
            unsafe {
                let normalized = param.preview_normalized(value);
                context.raw_set_parameter_normalized(param, normalized);
            }
        }
    }

    /// `SynthBase::endChangeGesture`.
    pub fn end(&self, name: &str) {
        if let Some(open) = self.open {
            lock(open).remove(name);
        }
        if let (Some(context), Some(param)) = (self.context, self.model.param(name)) {
            // SAFETY: as in `value`.
            unsafe { context.raw_end_set_parameter(param) };
        }
    }

    /// Tells the host about the controls MIDI-learned controllers moved
    /// since the last frame ([`EditorModel::drain_midi_moved`]): one gesture
    /// each with the control's current value, or only the value if a drag of
    /// that control already holds a gesture open.
    pub fn forward_midi_moves(&self) {
        self.model.drain_midi_moved(&mut |name| {
            let value = self.value(name);
            if let Some(param) = self.model.param(name) {
                // SAFETY: as in `value`.
                let normalized = unsafe { param.preview_normalized(value) };
                self.model.midi_move_forwarded(name, normalized);
            }
            let dragging = self.open.is_some_and(|open| lock(open).contains(name));
            if dragging {
                self.set(name, value);
            } else {
                self.set_once(name, value);
            }
        });
    }

    /// One complete gesture setting `name` to `value`.
    pub fn set_once(&self, name: &str, value: f32) {
        self.begin(name);
        self.set(name, value);
        self.end(name);
    }
}

fn lock(open: &OpenGestures) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
    open.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A host parameter pointer, for [`EditorModel::param`] implementations.
pub type HostParam = ParamPtr;
