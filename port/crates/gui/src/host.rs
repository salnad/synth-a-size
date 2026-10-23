// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor's access to host parameters.
//!
//! Upstream's controls change a value through `SynthBase::valueChanged` and
//! tell the host with `beginChangeGesture` / `endChangeGesture`. Here every
//! change goes through nice-plug's `GuiContext`, so the host records a normal
//! user gesture (automation, undo, saving the project).

use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::ParamPtr;
use sas_preset::registry::{registry, ParamInfo};

use crate::interface::EditorModel;

/// Reads and writes the synth's controls by registry name.
pub struct Host<'a> {
    model: &'a dyn EditorModel,
    context: Option<&'a GuiContext>,
}

impl<'a> Host<'a> {
    /// `context` is the plugin's GUI context; without one (offscreen
    /// rendering) values are read but changes are dropped.
    pub fn new(model: &'a dyn EditorModel, context: Option<&'a GuiContext>) -> Host<'a> {
        Host { model, context }
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
        if let (Some(context), Some(param)) = (self.context, self.model.param(name)) {
            // SAFETY: as in `value`.
            unsafe { context.raw_end_set_parameter(param) };
        }
    }

    /// One complete gesture setting `name` to `value`.
    pub fn set_once(&self, name: &str, value: f32) {
        self.begin(name);
        self.set(name, value);
        self.end(name);
    }
}

/// A host parameter pointer, for [`EditorModel::param`] implementations.
pub type HostParam = ParamPtr;
