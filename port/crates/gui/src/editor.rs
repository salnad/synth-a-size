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

use crate::chat::ChatPanel;
use crate::fonts;
use crate::host::{Host, OpenGestures};
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
    /// The host connection, from the window's `build` until it closes.
    gui_context: Option<GuiContext>,
    open_gestures: Arc<OpenGestures>,
    chat: ChatPanel,
    #[cfg(feature = "focus-spike")]
    focus_spike: crate::focus_spike::FocusSpike,
}

impl App {
    fn new(model: Arc<dyn EditorModel>) -> App {
        App {
            model,
            interface: Interface::default(),
            gui_context: None,
            open_gestures: Arc::default(),
            chat: ChatPanel::default(),
            #[cfg(feature = "focus-spike")]
            focus_spike: Default::default(),
        }
    }
}

impl NiceEguiApp for App {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        gui_context: GuiContext,
        frame: &mut Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        egui_ctx.set_fonts(fonts::definitions());
        // A drag cut short by the window closing must not leave the host
        // waiting for the end of its gesture. nice-plug-egui keeps this app
        // across windows without telling it one closed; the patched
        // egui-baseview does.
        let (model, open) = (self.model.clone(), self.open_gestures.clone());
        let context = gui_context.clone();
        // Likewise a key the on-screen keyboard is still playing, and a
        // modulation a drag has connected but not yet written to the host.
        let playing_note = self.interface.playing_note();
        let pending = self.interface.pending_connection();
        frame.on_close(move || {
            model.set_editor_open(false);
            let host = Host::tracking(model.as_ref(), Some(&context), &open);
            host.end_open_gestures();
            pending.commit(&host);
            playing_note.release(model.as_ref());
        });
        let repaint = egui_ctx.clone();
        self.model.attach(
            gui_context.clone(),
            Box::new(move || repaint.request_repaint()),
        );
        self.gui_context = Some(gui_context);
        self.model.set_editor_open(true);
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        let area = ui.max_rect();
        let host = Host::tracking(
            self.model.as_ref(),
            self.gui_context.as_ref(),
            &self.open_gestures,
        );
        // Parameters a learned MIDI controller moved reach the host as
        // gestures, before this frame draws them.
        host.forward_midi_moves();
        self.interface.show(ui, &host);
        if let Some(chat) = self.model.chat() {
            let placement = self
                .interface
                .chat_placement(area, ui.ctx().pixels_per_point());
            self.chat.show(ui.ctx(), &placement, chat, &mut || {
                let _ = frame.baseview_window().focus();
            });
        }
        #[cfg(feature = "focus-spike")]
        self.focus_spike.show(ui, frame);
    }
}
