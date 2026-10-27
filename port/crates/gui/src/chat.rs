// SPDX-License-Identifier: GPL-3.0-or-later

//! The LLM chat panel (L5 of the LLM plan; an addition with no upstream
//! counterpart, D13 in `migration/DIVERGENCES.md`).
//!
//! The panel only draws and forwards what the user does: the plugin's
//! [`ChatHost`] runs the chat (a worker thread with the model, the tools on
//! the running synth), so this crate stays free of the LLM crates.
//!
//! The toggle is our logo section (D14's product-name area, whose upstream
//! click opens an About section that isn't ported), and it opens the
//! editor's drawer ([`crate::drawer`]). The panel docks in the drawer, the
//! window's full height beside upstream's layout, when the host widened the
//! window for it. If the host kept the window's width, it overlays the right
//! of the editor instead, inside its letterboxed bounds and below the
//! header. Either way it is coloured from the skin's Overlay section, as
//! upstream's overlays are, and placed from the editor's layout
//! ([`Interface::chat_placement`](crate::Interface::chat_placement)), so it
//! scales and letterboxes with it.

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Id, Key, Layout, Order, RichText,
    ScrollArea, Sense, Stroke, TextEdit, Ui, Visuals, WidgetInfo,
};

use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// The chat panel's colours, from the skin's Overlay section.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChatStyle {
    pub body: Color32,
    pub border: Color32,
    /// Corner radius in points.
    pub rounding: f32,
    pub text: Color32,
    pub field: Color32,
    pub field_border: Color32,
    pub caret: Color32,
    pub selection: Color32,
    pub button: Color32,
    pub button_text: Color32,
    pub button_hover: Color32,
    pub button_pressed: Color32,
    pub accent: Color32,
}

impl ChatStyle {
    /// The colours of `overlay` (a scope within the Overlay section), for a
    /// window with `pixels_per_point`.
    pub fn from_scope(overlay: &Scope, pixels_per_point: f32) -> ChatStyle {
        let colour = |id| overlay.color(id).to_egui();
        ChatStyle {
            body: colour(ColorId::Body),
            border: colour(ColorId::Border),
            rounding: overlay.value(ValueId::BodyRounding) / pixels_per_point,
            text: colour(ColorId::BodyText),
            field: colour(ColorId::TextEditorBackground),
            field_border: colour(ColorId::TextEditorBorder),
            caret: colour(ColorId::TextEditorCaret),
            selection: colour(ColorId::TextEditorSelection),
            button: colour(ColorId::UiButton),
            button_text: colour(ColorId::UiButtonText),
            button_hover: colour(ColorId::UiButtonHover),
            button_pressed: colour(ColorId::UiButtonPressed),
            accent: colour(ColorId::WidgetPrimary1),
        }
    }

    /// Egui's widget colours from the skin's.
    fn apply(&self, visuals: &mut Visuals) {
        visuals.override_text_color = Some(self.text);
        visuals.panel_fill = self.body;
        visuals.window_fill = self.body;
        visuals.extreme_bg_color = self.field;
        visuals.text_cursor.stroke.color = self.caret;
        visuals.selection.bg_fill = self.selection;
        visuals.selection.stroke.color = self.accent;
        visuals.hyperlink_color = self.accent;
        let radius = CornerRadius::same(self.rounding.min(6.0).round() as u8);
        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_stroke.color = self.border;
        widgets.noninteractive.fg_stroke.color = self.text;
        for (state, fill) in [
            (&mut widgets.inactive, self.button),
            (&mut widgets.hovered, self.button_hover),
            (&mut widgets.active, self.button_pressed),
            (&mut widgets.open, self.button_pressed),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.fg_stroke.color = self.button_text;
            state.bg_stroke.color = self.field_border;
            state.corner_radius = radius;
        }
    }
}

/// Where the chat draws, in points, from the editor's layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChatPlacement {
    /// The logo section, which is the chat's toggle.
    pub toggle: egui::Rect,
    /// The editor's letterboxed bounds; the panel stays inside them.
    pub editor: egui::Rect,
    /// The bottom of the header; the panel opens below it.
    pub header_bottom: f32,
    /// The editor's scale in points (1 for a 1400×820-point editor), which
    /// the panel's text follows within limits.
    pub scale: f32,
    pub style: ChatStyle,
    /// Whether the panel docks in the drawer or overlays the editor.
    pub mode: ChatMode,
}

/// Where the open panel goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChatMode {
    /// In the drawer at the window's right (points, pixel-aligned), beside
    /// upstream's layout: the drawer is open and the window was widened.
    Docked(egui::Rect),
    /// Over the editor's right side ([`ChatPlacement::panel`]): the drawer
    /// is closed, or the host kept the window's width.
    Overlay,
}

impl ChatPlacement {
    /// How much the panel's text and spacing are scaled: the editor's,
    /// kept legible in small windows.
    pub fn text_scale(&self) -> f32 {
        self.scale.clamp(0.5, 2.0)
    }

    /// The open panel's rectangle: at the editor's right, below the header,
    /// [`PANEL_WIDTH`] at the text scale, and inside the editor.
    pub fn panel(&self) -> egui::Rect {
        if let ChatMode::Docked(drawer) = self.mode {
            return drawer;
        }
        let margin = (self.editor.height() * 0.008).clamp(1.0, 8.0);
        let top = self.header_bottom + margin;
        let width = (PANEL_WIDTH * self.text_scale())
            .min(self.editor.width() - 2.0 * margin)
            .max(0.0);
        let height = (self.editor.max.y - margin - top).max(0.0);
        egui::Rect::from_min_size(
            egui::pos2(self.editor.max.x - margin - width, top),
            egui::vec2(width, height),
        )
    }
}

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

/// Width of the panel in points at the default editor size (the drawer the
/// GUI plan reserves).
pub const PANEL_WIDTH: f32 = 360.0;

impl ChatPanel {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// The toggle in the logo section, and the panel when open, where
    /// `placement` says. `request_focus` asks the plugin window for keyboard
    /// focus.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        placement: &ChatPlacement,
        host: &dyn ChatHost,
        request_focus: &mut dyn FnMut(),
    ) {
        host.run_frame();
        let style = placement.style;
        egui::Area::new(Id::unique("chat_toggle"))
            .order(Order::Foreground)
            .fixed_pos(placement.toggle.min)
            .constrain(false)
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(placement.toggle.size(), Sense::click());
                let label = if self.open { "Close chat" } else { "Chat" };
                response.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, label));
                let painter = ui.painter_at(rect);
                if response.hovered() {
                    painter.rect_filled(
                        rect,
                        style.rounding,
                        style.button_hover.gamma_multiply(0.5),
                    );
                }
                // Under the product name, which the editor draws centred.
                painter.text(
                    egui::pos2(rect.center().x, rect.max.y - rect.height() * 0.1),
                    Align2::CENTER_BOTTOM,
                    if self.open { "close chat" } else { "chat" },
                    FontId::proportional(rect.height() * 0.2),
                    if self.open { style.accent } else { style.text },
                );
                if response.clicked() {
                    self.open = !self.open;
                }
            });
        if !self.open {
            return;
        }
        let panel =
            egui::emath::GuiRounding::round_to_pixels(placement.panel(), ctx.pixels_per_point());
        egui::Area::new(Id::unique("chat_panel"))
            .order(Order::Foreground)
            .fixed_pos(panel.min)
            .constrain(false)
            .show(ctx, |ui| {
                ui.set_clip_rect(panel);
                style.apply(ui.visuals_mut());
                scale_style(
                    ui.style_mut(),
                    placement.text_scale(),
                    ctx.pixels_per_point(),
                );
                let margin = (panel.width() * 0.025).clamp(2.0, 8.0).round();
                // Docked, the panel is flush with the window's edges.
                let rounding = match placement.mode {
                    ChatMode::Docked(_) => 0.0,
                    ChatMode::Overlay => style.rounding,
                };
                Frame::new()
                    .fill(style.body)
                    .stroke(Stroke::new(1.0, style.border))
                    .corner_radius(rounding)
                    .inner_margin(margin)
                    .show(ui, |ui| {
                        let inner = panel.shrink(margin + 1.0);
                        ui.set_min_size(inner.size());
                        ui.set_max_size(inner.size());
                        ui.label(RichText::new("Sound design chat").strong());
                        self.contents(ui, inner, host, request_focus);
                    });
            });
    }

    /// The panel's contents within `inner`, the frame's inside.
    fn contents(
        &mut self,
        ui: &mut Ui,
        inner: egui::Rect,
        host: &dyn ChatHost,
        request_focus: &mut dyn FnMut(),
    ) {
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

        let mut send = false;
        let field_id = Id::unique("chat_message");
        // Enter sends; Shift+Enter starts a new line. Plain Enter is taken
        // before the field sees it, so a send never embeds a newline.
        if ui.memory(|memory| memory.has_focus(field_id)) && ui.input_mut(take_plain_enter) {
            send = true;
        }
        // The message field and a held edit's buttons sit at the bottom,
        // and the conversation scrolls in the height left above them.
        let rest = egui::Rect::from_min_max(egui::pos2(inner.min.x, ui.cursor().min.y), inner.max);
        let rest = egui::emath::GuiRounding::round_to_pixels(rest, ui.pixels_per_point());
        ui.scope_builder(egui::UiBuilder::new().max_rect(rest), |ui| {
            egui::Panel::bottom(Id::unique("chat_bottom"))
                .frame(Frame::NONE)
                .resizable(false)
                .show(ui, |ui| {
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
                    let label = ui.label(RichText::new("Message").small()).id;
                    ui.horizontal(|ui| {
                        let send_width = ui.spacing().interact_size.x * 1.5;
                        let width = (ui.available_width() - send_width).max(0.0);
                        let height = ui.text_style_height(&egui::TextStyle::Body) * 3.5;
                        let field = ui
                            .add_sized(
                                [width, height],
                                TextEdit::multiline(&mut self.input)
                                    .id(field_id)
                                    .hint_text("Describe a sound or a change")
                                    .desired_rows(3),
                            )
                            .labelled_by(label);
                        if field.clicked() && host.settings().focus == FocusMode::GrabOnClick {
                            request_focus();
                        }
                        let ready = !view.busy && !self.input.trim().is_empty();
                        if ui.add_enabled(ready, egui::Button::new("Send")).clicked() {
                            send = true;
                        }
                    });
                });
            egui::CentralPanel::default()
                .frame(Frame::NONE)
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
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
                });
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

/// Scales `style`'s text and spacing by `scale`, in whole pixels so rows
/// stay on the pixel grid.
fn scale_style(style: &mut egui::Style, scale: f32, pixels_per_point: f32) {
    let pixels = |points: f32| (points * scale * pixels_per_point).round() / pixels_per_point;
    for font in style.text_styles.values_mut() {
        font.size = pixels(font.size);
    }
    let spacing = &mut style.spacing;
    spacing.item_spacing = egui::vec2(
        pixels(spacing.item_spacing.x),
        pixels(spacing.item_spacing.y),
    );
    spacing.button_padding = egui::vec2(
        pixels(spacing.button_padding.x),
        pixels(spacing.button_padding.y),
    );
    spacing.interact_size = egui::vec2(
        pixels(spacing.interact_size.x),
        pixels(spacing.interact_size.y),
    );
    spacing.icon_width = pixels(spacing.icon_width);
    spacing.icon_width_inner = pixels(spacing.icon_width_inner);
    spacing.icon_spacing = pixels(spacing.icon_spacing);
}

/// Removes plain Enter presses from this frame's input. `consume_key`
/// matches modifiers logically, which would also take Shift+Enter.
fn take_plain_enter(input: &mut egui::InputState) -> bool {
    let before = input.events.len();
    input.events.retain(|event| {
        !matches!(
            event,
            egui::Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if modifiers.is_none()
        )
    });
    input.events.len() != before
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
