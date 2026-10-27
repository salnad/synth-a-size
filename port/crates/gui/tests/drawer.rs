// SPDX-License-Identifier: GPL-3.0-or-later

//! The chat's drawer through the chat panel, composed as the editor
//! composes them (`show_chat`), against a fake chat host.

use std::cell::RefCell;
use std::sync::Mutex;

use egui::Key;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use sas_gui::{
    drawer_resize, show_chat, ChatHost, ChatMode, ChatPanel, ChatSettingsView, ChatView, Interface,
};

#[derive(Default)]
struct FakeHost {
    view: Mutex<ChatView>,
    settings: Mutex<ChatSettingsView>,
    calls: Mutex<Vec<String>>,
}

impl FakeHost {
    fn enabled() -> FakeHost {
        let host = FakeHost::default();
        host.view.lock().unwrap().enabled = true;
        host.settings.lock().unwrap().enabled = true;
        host
    }
}

impl ChatHost for FakeHost {
    fn run_frame(&self) {}
    fn view(&self) -> ChatView {
        self.view.lock().unwrap().clone()
    }
    fn send(&self, text: String) {
        self.calls.lock().unwrap().push(format!("send {text}"));
    }
    fn stop(&self) {}
    fn undo(&self) {}
    fn compare(&self) {}
    fn confirm(&self, _accept: bool) {}
    fn settings(&self) -> ChatSettingsView {
        self.settings.lock().unwrap().clone()
    }
    fn save_settings(
        &self,
        settings: ChatSettingsView,
        _api_key: Option<String>,
    ) -> Result<(), String> {
        *self.settings.lock().unwrap() = settings;
        Ok(())
    }
}

fn window(width: f32, height: f32) -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))
}

/// The toggle opens the drawer, which the interface keeps; the editor then
/// asks for a window wider by the drawer, and until the host gives it the
/// panel overlays the editor. In the wider window it docks beside
/// upstream's layout at its old size. Closing gives the width back.
#[test]
fn the_toggle_opens_the_drawer_and_the_window_widens_for_it() {
    let host = FakeHost::enabled();
    let interface = RefCell::new(Interface::default());
    let mut panel = ChatPanel::default();
    let mut harness = Harness::builder()
        .with_size((1400.0, 820.0))
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .build_ui(|ui| {
            let window = ui.ctx().content_rect();
            show_chat(
                &mut interface.borrow_mut(),
                &mut panel,
                ui.ctx(),
                window,
                &host,
                &mut || {},
            );
        });
    harness.run();
    assert!(!interface.borrow().drawer_open());
    harness.get_by_label("Chat").click();
    harness.run();
    assert!(interface.borrow().drawer_open());
    assert_eq!(
        drawer_resize(Some(false), true, window(1400.0, 820.0), 1.0),
        Some(egui::vec2(1760.0, 820.0))
    );
    let placement = interface
        .borrow()
        .chat_placement(window(1400.0, 820.0), 1.0);
    assert_eq!(placement.mode, ChatMode::Overlay);

    harness.set_size(egui::vec2(1760.0, 820.0));
    harness.run();
    let placement = interface
        .borrow()
        .chat_placement(window(1760.0, 820.0), 1.0);
    assert_eq!(placement.editor, window(1400.0, 820.0));
    assert_eq!(
        placement.mode,
        ChatMode::Docked(egui::Rect::from_min_max(
            egui::pos2(1400.0, 0.0),
            egui::pos2(1760.0, 820.0)
        ))
    );

    harness.get_by_label("Close chat").click();
    harness.run();
    assert!(!interface.borrow().drawer_open());
    assert_eq!(
        drawer_resize(Some(true), false, window(1760.0, 820.0), 1.0),
        Some(egui::vec2(1400.0, 820.0))
    );
    // A host that never widened the window gets no request on closing, and
    // the first frame of a window keeps the size the instance saved.
    assert_eq!(
        drawer_resize(Some(true), false, window(1400.0, 820.0), 1.0),
        None
    );
    assert_eq!(drawer_resize(None, true, window(1400.0, 820.0), 1.0), None);
}

/// Typing in the chat survives the window widening: the field keeps egui's
/// focus across the resize, and opening the drawer grabs no focus.
#[test]
fn the_chat_field_keeps_focus_across_the_resize() {
    let host = FakeHost::enabled();
    let interface = RefCell::new(Interface::default());
    interface.borrow_mut().set_drawer_open(true);
    let mut panel = ChatPanel::default();
    let focus_requests = std::cell::Cell::new(0);
    let mut harness = Harness::builder()
        .with_size((1400.0, 820.0))
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .build_ui(|ui| {
            let window = ui.ctx().content_rect();
            show_chat(
                &mut interface.borrow_mut(),
                &mut panel,
                ui.ctx(),
                window,
                &host,
                &mut || focus_requests.set(focus_requests.get() + 1),
            );
        });
    harness.run();
    assert_eq!(focus_requests.get(), 0);
    let field = harness.get_by_label("Message");
    field.click();
    field.type_text("warmer");
    harness.run();
    let focused = harness.ctx.memory(|memory| memory.focused());
    assert!(focused.is_some());

    harness.set_size(egui::vec2(1760.0, 820.0));
    harness.run();
    assert_eq!(harness.ctx.memory(|memory| memory.focused()), focused);
    harness.get_by_label("Message").type_text(" pad");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(*host.calls.lock().unwrap(), ["send warmer pad"]);
}
