// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin editor window: [`Interface`] in a nice-plug egui editor drawn
//! with wgpu (Metal on macOS).
//!
//! Window behaviour follows upstream's plugin editor
//! (`vital/src/plugin/synth_editor.cpp`, `common/border_bounds_constrainer.cpp`):
//! it opens at the default 1400 × 820 and the host may resize it down to
//! 350 × 205, keeping the 1400:820 aspect ratio. Upstream also scales the
//! first size by a factor saved in its config file; the port does not yet.

use std::sync::Arc;

use nice_plug::context::gui::GuiContext;
use nice_plug_egui::baseview::dpi::LogicalSize;
use nice_plug_egui::{
    create_egui_editor, EguiEditor, EguiEditorState, EguiNiceSettings, Frame, NiceEguiApp,
};

use crate::fonts;
use crate::interface::{EditorModel, Interface, PRODUCT_NAME};
use crate::layout::{DEFAULT_HEIGHT, DEFAULT_WIDTH};

/// `vital::kMinWindowWidth` / `kMinWindowHeight`.
pub const MIN_WIDTH: f32 = 350.0;
pub const MIN_HEIGHT: f32 = 205.0;

/// The editor type `sas-plugin` hands to nice-plug.
pub type Editor = EguiEditor<App>;

/// The editor's size state at the default size. Keep one per plugin
/// instance so the size survives closing and reopening the window.
pub fn default_state() -> Arc<EguiEditorState> {
    EguiEditorState::from_size(
        LogicalSize::new(DEFAULT_WIDTH as f64, DEFAULT_HEIGHT as f64),
        1.0,
    )
}

/// The editor for one plugin instance.
pub fn create(state: Arc<EguiEditorState>, model: Arc<dyn EditorModel>) -> Option<Editor> {
    let resize_hint = nice_plug::prelude::ResizeHint {
        preserve_aspect_ratio: true,
        ..nice_plug::prelude::ResizeHint::resizable()
            .with_min_logical_size(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT))
            .with_aspect_ratio(DEFAULT_WIDTH as u32, DEFAULT_HEIGHT as u32)
    };
    let settings = EguiNiceSettings::default()
        .with_tile(PRODUCT_NAME)
        .with_resize_hint(resize_hint);
    create_egui_editor(state, Default::default(), settings, App::new(model))
}

/// The egui app behind [`Editor`].
pub struct App {
    model: Arc<dyn EditorModel>,
    interface: Interface,
    #[cfg(feature = "focus-spike")]
    focus_spike: crate::focus_spike::FocusSpike,
}

impl App {
    fn new(model: Arc<dyn EditorModel>) -> App {
        App {
            model,
            interface: Interface::default(),
            #[cfg(feature = "focus-spike")]
            focus_spike: Default::default(),
        }
    }
}

impl NiceEguiApp for App {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        _gui_context: GuiContext,
        _frame: &mut Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        egui_ctx.set_fonts(fonts::definitions());
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        self.interface.show(ui, self.model.as_ref());
        #[cfg(feature = "focus-spike")]
        self.focus_spike.show(ui, _frame);
    }
}
