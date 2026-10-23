//! The chat panel against a fake host, through egui_kittest (no GPU).

use std::cell::Cell;
use std::sync::Mutex;

use egui::Key;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use sas_gui::chat::{ChatLine, CompareSide, LineKind};
use sas_gui::{
    ChatHost, ChatPanel, ChatPlacement, ChatSettingsView, ChatView, FocusMode, Interface,
};

#[derive(Default)]
struct FakeHost {
    view: Mutex<ChatView>,
    settings: Mutex<ChatSettingsView>,
    calls: Mutex<Vec<String>>,
    frames: Mutex<usize>,
}

impl FakeHost {
    fn enabled() -> FakeHost {
        let host = FakeHost::default();
        host.view.lock().unwrap().enabled = true;
        host.settings.lock().unwrap().enabled = true;
        host
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: impl Into<String>) {
        self.calls.lock().unwrap().push(call.into());
    }
}

impl ChatHost for FakeHost {
    fn run_frame(&self) {
        *self.frames.lock().unwrap() += 1;
    }
    fn view(&self) -> ChatView {
        self.view.lock().unwrap().clone()
    }
    fn send(&self, text: String) {
        self.record(format!("send {text}"));
    }
    fn stop(&self) {
        self.record("stop");
    }
    fn undo(&self) {
        self.record("undo");
    }
    fn compare(&self) {
        self.record("compare");
    }
    fn confirm(&self, accept: bool) {
        self.record(format!("confirm {accept}"));
    }
    fn settings(&self) -> ChatSettingsView {
        self.settings.lock().unwrap().clone()
    }
    fn save_settings(
        &self,
        settings: ChatSettingsView,
        api_key: Option<String>,
    ) -> Result<(), String> {
        self.record(format!(
            "save enabled={} focus={:?} key={}",
            settings.enabled,
            settings.focus,
            api_key.unwrap_or_default()
        ));
        *self.settings.lock().unwrap() = settings.clone();
        self.view.lock().unwrap().enabled = settings.enabled;
        Ok(())
    }
}

/// Runs frames until the UI settles, or a few frames while a spinner keeps
/// it repainting.
fn settle(harness: &mut Harness, busy: bool) {
    if busy {
        harness.run_steps(4);
    } else {
        harness.run();
    }
}

/// The chat's placement over a default editor filling the window.
fn placement(ctx: &egui::Context) -> ChatPlacement {
    Interface::default().chat_placement(ctx.content_rect(), ctx.pixels_per_point())
}

fn harness<'a>(
    panel: &'a mut ChatPanel,
    host: &'a FakeHost,
    focus_requests: &'a Cell<usize>,
) -> Harness<'a> {
    panel.set_open(true);
    let busy = host.view.lock().unwrap().busy;
    let mut harness = Harness::builder()
        .with_size((1400.0, 820.0))
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .build_ui(move |ui| {
            let placement = placement(ui.ctx());
            panel.show(ui.ctx(), &placement, host, &mut || {
                focus_requests.set(focus_requests.get() + 1)
            });
        });
    settle(&mut harness, busy);
    harness
}

#[test]
fn the_logo_section_opens_and_closes_the_panel() {
    let host = FakeHost::enabled();
    let mut panel = ChatPanel::default();
    {
        let mut harness = Harness::builder()
            .with_size((1400.0, 820.0))
            .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
            .build_ui(|ui| {
                let placement = placement(ui.ctx());
                panel.show(ui.ctx(), &placement, &host, &mut || {});
            });
        harness.run();
        assert!(harness.query_by_label("Sound design chat").is_none());
        harness.get_by_label("Chat").click();
        harness.run();
        assert!(harness.query_by_label("Sound design chat").is_some());
        harness.get_by_label("Close chat").click();
        harness.run();
        assert!(harness.query_by_label("Sound design chat").is_none());
    }
    assert!(
        *host.frames.lock().unwrap() > 0,
        "run_frame is called every frame"
    );
}

#[test]
fn enter_sends_the_message_and_clears_the_field() {
    let host = FakeHost::enabled();
    let focus = Cell::new(0);
    let mut panel = ChatPanel::default();
    let mut harness = harness(&mut panel, &host, &focus);
    let field = harness.get_by_label("Message");
    field.click();
    field.type_text("warmer pad");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(host.calls(), ["send warmer pad"]);
    // Grab-on-click is the default focus mode.
    assert_eq!(focus.get(), 1);
}

#[test]
fn enter_mid_text_sends_without_a_newline_and_shift_enter_adds_one() {
    let host = FakeHost::enabled();
    let focus = Cell::new(0);
    let mut panel = ChatPanel::default();
    let mut harness = harness(&mut panel, &host, &focus);
    let field = harness.get_by_label("Message");
    field.click();
    field.type_text("warm pad");
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::SHIFT, Key::Enter);
    harness.get_by_label("Message").type_text("slow attack");
    harness.run();
    for _ in 0.."slow attack".len() {
        harness.key_press(Key::ArrowLeft);
    }
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(host.calls(), ["send warm pad\nslow attack"]);
}

#[test]
fn the_send_button_is_disabled_while_busy_and_stop_shows() {
    let host = FakeHost::enabled();
    host.view.lock().unwrap().busy = true;
    let focus = Cell::new(0);
    let mut panel = ChatPanel::default();
    let mut harness = harness(&mut panel, &host, &focus);
    harness.get_by_label("Message").type_text("again");
    settle(&mut harness, true);
    harness.get_by_label("Send").click();
    settle(&mut harness, true);
    harness.get_by_label("Stop").click();
    settle(&mut harness, true);
    assert_eq!(host.calls(), ["stop"]);
}

#[test]
fn undo_compare_and_confirmation_reach_the_host() {
    let host = FakeHost::enabled();
    {
        let mut view = host.view.lock().unwrap();
        view.can_undo = true;
        view.can_compare = true;
        view.comparing = Some(CompareSide::Before);
        view.pending = Some("volume would rise to 3dB".into());
        view.lines = vec![
            ChatLine {
                kind: LineKind::User,
                text: "louder".into(),
            },
            ChatLine {
                kind: LineKind::Assistant,
                text: "That raises the volume; shall I?".into(),
            },
        ];
    }
    let focus = Cell::new(0);
    let mut panel = ChatPanel::default();
    let mut harness = harness(&mut panel, &host, &focus);
    assert!(harness
        .query_by_label_contains("That raises the volume")
        .is_some());
    harness.get_by_label("Undo").click();
    harness.run();
    harness.get_by_label("A/B: before").click();
    harness.run();
    harness.get_by_label("Yes").click();
    harness.run();
    assert_eq!(host.calls(), ["undo", "compare", "confirm true"]);
}

#[test]
fn a_chat_that_is_off_shows_the_settings() {
    let host = FakeHost::default();
    let focus = Cell::new(0);
    let mut panel = ChatPanel::default();
    let mut harness = harness(&mut panel, &host, &focus);
    assert!(harness.query_by_label("Message").is_none());
    harness.get_by_label("Turn on the chat").click();
    harness.run();
    harness.get_by_label("Save").click();
    harness.run();
    assert_eq!(host.calls(), ["save enabled=true focus=GrabOnClick key="]);
    // The conversation view replaces the form.
    assert!(harness.query_by_label("Message").is_some());
}

#[test]
fn the_focus_approach_is_a_setting() {
    let host = FakeHost::enabled();
    host.settings.lock().unwrap().focus = FocusMode::Plain;
    let focus = Cell::new(0);
    let mut panel = ChatPanel::default();
    let mut harness = harness(&mut panel, &host, &focus);
    harness.get_by_label("Message").click();
    harness.run();
    assert_eq!(focus.get(), 0, "plain mode never asks for window focus");

    harness.get_by_label("Settings").click();
    harness.run();
    harness
        .get_by_label("Ask for keyboard focus when a text field is clicked")
        .click();
    harness.run();
    harness.get_by_label("Save").click();
    harness.run();
    assert_eq!(host.calls(), ["save enabled=true focus=GrabOnClick key="]);
}
