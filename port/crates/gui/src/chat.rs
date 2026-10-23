// SPDX-License-Identifier: GPL-3.0-or-later

//! The LLM chat panel (L5 of the LLM plan; an addition with no upstream
//! counterpart, D13 in `migration/DIVERGENCES.md`).
//!
//! The panel only draws and forwards what the user does: the plugin's
//! [`ChatHost`] runs the chat (a worker thread with the model, the tools on
//! the running synth), so this crate stays free of the LLM crates. Until the
//! editor's right-side drawer exists (G7), the panel opens over the right of
//! the editor from a header button, as upstream's own overlays do.

use egui::{Align, Align2, Color32, Key, Layout, RichText, ScrollArea, TextEdit, Ui};

/// How the chat's text field gets the keyboard. Hosts on macOS can keep
/// keyboard focus away from a plugin's text fields; which approach works in
/// FL Studio is still being checked (G1's focus spike), so it is a setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusMode {
    /// Ask the plugin window for keyboard focus when the field is clicked
    /// (egui-baseview's `keyboard_focus_workaround`, at run time).
    #[default]
    GrabOnClick,
    /// Rely on the host to give the window the keyboard.
    Plain,
}

/// Who wrote a line of the conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    User,
    Assistant,
    /// A tool the model called, summarised.
    Tool,
    /// Something went wrong (a provider error, a timeout).
    Error,
    /// A status note (stopped, applied, declined).
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    pub kind: LineKind,
    pub text: String,
}

/// Which side an A/B comparison is playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareSide {
    Before,
    After,
}

/// What the panel shows, read once per frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatView {
    /// Whether the chat is turned on in the settings.
    pub enabled: bool,
    pub lines: Vec<ChatLine>,
    /// A message is being answered.
    pub busy: bool,
    /// An edit held for the user's confirmation, and why.
    pub pending: Option<String>,
    pub can_undo: bool,
    pub can_compare: bool,
    pub comparing: Option<CompareSide>,
    /// Why the chat cannot run (no API key, no network in this build).
    pub problem: Option<String>,
}

/// The chat settings the panel edits. The API key is never read back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatSettingsView {
    pub enabled: bool,
    /// `anthropic` or `openai`.
    pub provider: String,
    pub model: String,
    pub base_url: String,
    /// Whether a key is stored for the provider.
    pub has_key: bool,
    pub focus: FocusMode,
}

/// The plugin's side of the chat.
pub trait ChatHost: Send + Sync + 'static {
    /// Called on the GUI thread once per frame, before the panel draws: runs
    /// the tool calls the chat worker is waiting on.
    fn run_frame(&self);
    fn view(&self) -> ChatView;
    fn send(&self, text: String);
    /// Stops the message being answered after the model call in flight.
    fn stop(&self);
    fn undo(&self);
    /// A/B: switches between the patch before and after the last turn.
    fn compare(&self);
    /// Answers the edit held for confirmation.
    fn confirm(&self, accept: bool);
    fn settings(&self) -> ChatSettingsView;
    /// Saves `settings`, and `api_key` for its provider if given.
    fn save_settings(
        &self,
        settings: ChatSettingsView,
        api_key: Option<String>,
    ) -> Result<(), String>;
}

/// The panel's own state.
#[derive(Default)]
pub struct ChatPanel {
    open: bool,
    input: String,
    settings: Option<(ChatSettingsView, String)>,
    settings_error: Option<String>,
}

/// Width of the panel in points (the drawer the GUI plan reserves).
pub const PANEL_WIDTH: f32 = 360.0;

impl ChatPanel {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// The header button that opens and closes the panel, at the top right
    /// of `area`, and the panel itself when open. `request_focus` asks the
    /// plugin window for keyboard focus.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        area: egui::Rect,
        host: &dyn ChatHost,
        request_focus: &mut dyn FnMut(),
    ) {
        host.run_frame();
        egui::Area::new(egui::Id::unique("chat_toggle"))
            .fixed_pos(egui::pos2(area.max.x - 64.0, area.min.y + 8.0))
            .show(ctx, |ui| {
                let label = if self.open { "Close chat" } else { "Chat" };
                if ui.button(label).clicked() {
                    self.open = !self.open;
                }
            });
        if !self.open {
            return;
        }
        let height = (area.height() - 56.0).max(200.0);
        egui::Window::new("Sound design chat")
            .anchor(Align2::RIGHT_TOP, [-8.0, 44.0])
            .fixed_size([PANEL_WIDTH, height])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| self.contents(ui, host, request_focus));
    }

    fn contents(&mut self, ui: &mut Ui, host: &dyn ChatHost, request_focus: &mut dyn FnMut()) {
        let view = host.view();
        if self.settings.is_some() || !view.enabled {
            self.settings_form(ui, host, request_focus);
            return;
        }

        ui.horizontal(|ui| {
            if ui
                .add_enabled(view.can_undo && !view.busy, egui::Button::new("Undo"))
                .clicked()
            {
                host.undo();
            }
            let compare = match view.comparing {
                Some(CompareSide::Before) => "A/B: before",
                Some(CompareSide::After) => "A/B: after",
                None => "A/B",
            };
            if ui
                .add_enabled(view.can_compare && !view.busy, egui::Button::new(compare))
                .clicked()
            {
                host.compare();
            }
            if view.busy && ui.button("Stop").clicked() {
                host.stop();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Settings").clicked() {
                    self.settings = Some((host.settings(), String::new()));
                }
            });
        });
        if let Some(problem) = &view.problem {
            ui.colored_label(Color32::from_rgb(0xe0, 0x80, 0x60), problem);
        }
        ui.separator();

        let input_height = 64.0;
        ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .max_height(ui.available_height() - input_height - 40.0)
            .show(ui, |ui| {
                for line in &view.lines {
                    show_line(ui, line);
                }
                if view.busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Working...");
                    });
                }
            });

        if let Some(reason) = &view.pending {
            ui.separator();
            ui.label(format!("Apply this change? {reason}"));
            ui.horizontal(|ui| {
                if ui.button("Yes").clicked() {
                    host.confirm(true);
                }
                if ui.button("No").clicked() {
                    host.confirm(false);
                }
            });
        }

        ui.separator();
        let mut send = false;
        let label = ui.label(RichText::new("Message").small()).id;
        ui.horizontal(|ui| {
            let field = ui
                .add_sized(
                    [PANEL_WIDTH - 70.0, input_height],
                    TextEdit::multiline(&mut self.input)
                        .hint_text("Describe a sound or a change")
                        .desired_rows(3),
                )
                .labelled_by(label);
            if field.clicked() && host.settings().focus == FocusMode::GrabOnClick {
                request_focus();
            }
            // Enter sends; Shift+Enter starts a new line.
            if field.has_focus()
                && ui.input(|input| input.key_pressed(Key::Enter) && !input.modifiers.shift)
            {
                send = true;
            }
            let ready = !view.busy && !self.input.trim().is_empty();
            if ui.add_enabled(ready, egui::Button::new("Send")).clicked() {
                send = true;
            }
        });
        if send && !view.busy {
            let text = self.input.trim().to_string();
            if !text.is_empty() {
                host.send(text);
                self.input.clear();
            }
        }
    }

    fn settings_form(&mut self, ui: &mut Ui, host: &dyn ChatHost, request_focus: &mut dyn FnMut()) {
        let (mut draft, mut key) = self
            .settings
            .take()
            .unwrap_or_else(|| (host.settings(), String::new()));
        let mut keep_open = true;
        let (draft, key) = (&mut draft, &mut key);
        ui.label(
            RichText::new(
                "The chat sends your messages and a description of the patch to the provider \
                 you choose. It is off until you turn it on.",
            )
            .small(),
        );
        ui.checkbox(&mut draft.enabled, "Turn on the chat");
        let grab = draft.focus == FocusMode::GrabOnClick;
        egui::Grid::new("chat_settings")
            .num_columns(2)
            .show(ui, |ui| {
                let label = ui.label("Provider").id;
                egui::ComboBox::from_id_salt("chat_provider")
                    .selected_text(match draft.provider.as_str() {
                        "openai" => "OpenAI-compatible",
                        _ => "Anthropic",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.provider,
                            "anthropic".to_string(),
                            "Anthropic",
                        );
                        ui.selectable_value(
                            &mut draft.provider,
                            "openai".to_string(),
                            "OpenAI-compatible",
                        );
                    })
                    .response
                    .labelled_by(label);
                ui.end_row();
                let hint = if draft.has_key {
                    "stored (type to replace)"
                } else {
                    "not set"
                };
                for (name, text, hint, password) in [
                    ("Model", &mut draft.model, "provider default", false),
                    ("Base URL", &mut draft.base_url, "provider default", false),
                    ("API key", key, hint, true),
                ] {
                    let label = ui.label(name).id;
                    let field = ui
                        .add(
                            TextEdit::singleline(text)
                                .password(password)
                                .hint_text(hint),
                        )
                        .labelled_by(label);
                    if field.clicked() && grab {
                        request_focus();
                    }
                    ui.end_row();
                }
            });
        let mut grab = draft.focus == FocusMode::GrabOnClick;
        if ui
            .checkbox(
                &mut grab,
                "Ask for keyboard focus when a text field is clicked",
            )
            .changed()
        {
            draft.focus = if grab {
                FocusMode::GrabOnClick
            } else {
                FocusMode::Plain
            };
        }
        if let Some(error) = &self.settings_error {
            ui.colored_label(Color32::from_rgb(0xe0, 0x80, 0x60), error);
        }
        let mut error = self.settings_error.take();
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                let new_key = (!key.trim().is_empty()).then(|| key.trim().to_string());
                match host.save_settings(draft.clone(), new_key) {
                    Ok(()) => {
                        keep_open = false;
                        error = None;
                    }
                    Err(message) => error = Some(message),
                }
            } else if host.view().enabled && ui.button("Cancel").clicked() {
                keep_open = false;
                error = None;
            }
        });
        self.settings_error = error;
        if keep_open {
            self.settings = Some((draft.clone(), std::mem::take(key)));
        }
    }
}

fn show_line(ui: &mut Ui, line: &ChatLine) {
    let text = match line.kind {
        LineKind::User => RichText::new(format!("You: {}", line.text)).strong(),
        LineKind::Assistant => RichText::new(&line.text),
        LineKind::Tool => RichText::new(&line.text).small().weak(),
        LineKind::Error => RichText::new(&line.text).color(Color32::from_rgb(0xe0, 0x80, 0x60)),
        LineKind::Note => RichText::new(&line.text).italics().weak(),
    };
    ui.add(egui::Label::new(text).wrap());
    ui.add_space(4.0);
}
