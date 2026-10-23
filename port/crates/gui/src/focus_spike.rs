// SPDX-License-Identifier: GPL-3.0-or-later

//! Keyboard-focus check for FL Studio (G1 spike, `focus-spike` feature
//! only; never in the shipped editor).
//!
//! Hosts on macOS can keep keyboard focus away from a plugin's text fields.
//! This panel has a text field, a switch that asks the window for keyboard
//! focus when the field is clicked (what egui-baseview's
//! `keyboard_focus_workaround` does at compile time), and live readouts of
//! what the editor receives. Keys pass to the host whenever no text field is
//! focused, so host shortcuts such as the space bar keep working.

use std::collections::VecDeque;

use egui::{Align2, Event};
use nice_plug_egui::Frame;

#[derive(Default)]
pub struct FocusSpike {
    text: String,
    grab_focus: bool,
    log: VecDeque<String>,
}

impl FocusSpike {
    pub fn show(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        let events: Vec<String> = ui.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some(format!("key {key:?} {modifiers:?}")),
                    Event::Text(text) => Some(format!("text {text:?}")),
                    _ => None,
                })
                .collect()
        });
        for event in events {
            self.log.push_front(event);
            self.log.truncate(8);
        }

        egui::Window::new("Keyboard focus check")
            .anchor(Align2::RIGHT_BOTTOM, [-16.0, -16.0])
            .resizable(false)
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                let response =
                    ui.add(egui::TextEdit::singleline(&mut self.text).hint_text("Type here"));
                if response.clicked() && self.grab_focus {
                    let _ = frame.baseview_window().focus();
                }
                ui.checkbox(
                    &mut self.grab_focus,
                    "Ask for window focus when the field is clicked",
                );
                ui.label(format!(
                    "Window has focus: {}",
                    frame.baseview_window().has_focus()
                ));
                ui.label(format!(
                    "Text field wants keys: {}",
                    ui.ctx().egui_wants_keyboard_input()
                ));
                ui.separator();
                ui.label("Last keys received:");
                for line in &self.log {
                    ui.monospace(line);
                }
            });
    }
}
