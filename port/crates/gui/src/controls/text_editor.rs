// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `OpenGlTextEditor` (`vital/src/interface/editor_components/
//! open_gl_image_component.h`): a single-line text field drawn with the
//! skin's text editor colours, in the light face (or monospace) at half its
//! height, with JUCE's indents and the prompt it shows while empty.
//!
//! Hosts on macOS can keep keyboard focus away from a plugin's text fields
//! (FL Studio checklist item 1). As the chat's fields do, a click on the
//! field asks the editor window for keyboard focus; the editor does so
//! after the frame if its focus setting asks for it
//! ([`take_window_focus_request`]).

use egui::{Id, Key, Ui, UiBuilder};

use crate::fonts::Face;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};
use crate::text::{Justify, Overflow};

fn window_focus_request() -> Id {
    Id::unique("text editor window focus")
}

/// Whether a text field was clicked this frame, so the editor window should
/// ask the host for keyboard focus. Clears the request.
pub fn take_window_focus_request(ctx: &egui::Context) -> bool {
    ctx.data_mut(|data| data.remove_temp::<bool>(window_focus_request()))
        .is_some()
}

/// `DefaultLookAndFeel::fillTextEditorBackground`: the rounded background
/// and its one-pixel border.
pub fn paint_background(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    let rounding = scope.value(ValueId::WidgetRoundedCorner);
    pixels.fill_rounded_rect(bounds, rounding, scope.color(ColorId::TextEditorBackground));
    pixels.draw_rounded_rect(
        RectF::from(bounds).reduced(0.5),
        rounding,
        1.0,
        scope.color(ColorId::TextEditorBorder),
    );
}

/// What the field did this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct TextEditorEvents {
    /// The text changed (`textEditorTextChanged`).
    pub changed: bool,
    /// Escape was pressed while it had focus (`textEditorEscapeKeyPressed`);
    /// the field keeps focus, as JUCE's does.
    pub escape: bool,
    /// Return was pressed while it had focus
    /// (`textEditorReturnKeyPressed`).
    pub return_pressed: bool,
    /// It has keyboard focus after this frame.
    pub has_focus: bool,
    /// It had the keyboard last frame and lost it (`textEditorFocusLost`).
    pub lost_focus: bool,
}

/// Gives the field `salt` (as passed to [`single_line`] in `ui`) the
/// keyboard with all of `text` selected (`grabKeyboardFocus` and
/// `selectAll`), asking the editor window for keyboard focus as a click
/// does (`grabKeyboardFocus` focuses the plugin's window too).
pub fn focus_select_all(ui: &Ui, salt: impl std::hash::Hash + std::fmt::Debug, text: &str) {
    let all = egui::text_selection::CCursorRange::select_all_str(text);
    focus_with(ui, salt, all);
}

/// Gives the field the keyboard with the caret after its text
/// (`grabKeyboardFocus`), raising the window focus request as
/// [`focus_select_all`] does.
pub fn focus_at_end(ui: &Ui, salt: impl std::hash::Hash + std::fmt::Debug, text: &str) {
    let end = egui::text::CCursor::new(text.chars().count());
    focus_with(ui, salt, egui::text_selection::CCursorRange::one(end));
}

/// Takes the keyboard from the field (its parent's `grabKeyboardFocus`).
pub fn surrender_focus(ui: &Ui, salt: impl std::hash::Hash + std::fmt::Debug) {
    let id = ui.scope_id().with(("text editor", salt));
    ui.memory_mut(|memory| memory.surrender_focus(id));
}

fn focus_with(
    ui: &Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    range: egui::text_selection::CCursorRange,
) {
    let id = ui.scope_id().with(("text editor", salt));
    ui.memory_mut(|memory| memory.request_focus(id));
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    state.cursor.set_char_range(Some(range));
    egui::TextEdit::store_state(ui.ctx(), id, state);
    ui.data_mut(|data| {
        data.insert_temp(id.with("had focus"), true);
        data.insert_temp(window_focus_request(), true);
    });
}

/// A field's text: its size in pixels, in the light face, and its colour.
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    pub font_size: f32,
    pub colour: ColorId,
    /// `setJustification(Justification::centred)`; left otherwise.
    pub centred: bool,
    /// No `setTextToShowWhenEmpty`: the prompt only names the field (its
    /// component name) and isn't drawn.
    pub hide_prompt: bool,
    /// `setMonospace`: Droid Sans Mono, with its own top indent.
    pub mono: bool,
}

/// A single-line `OpenGlTextEditor` at `bounds` (pixels) editing `text`,
/// showing `prompt` while empty and unfocused (`setTextToShowWhenEmpty`, in
/// the body text colour at half its alpha), in `applyFont`'s light face at
/// half its height.
pub fn single_line(
    ui: &mut Ui,
    pixels: &PixelPainter,
    scope: &Scope,
    salt: impl std::hash::Hash + std::fmt::Debug,
    bounds: Rect,
    text: &mut String,
    prompt: &str,
) -> TextEditorEvents {
    let style = TextStyle {
        font_size: bounds.height as f32 / 2.0,
        colour: ColorId::BodyText,
        centred: false,
        hide_prompt: false,
        mono: false,
    };
    single_line_styled(ui, pixels, scope, salt, bounds, text, prompt, style)
}

/// [`single_line`] with its text in `style` (`applyFontToAllText`,
/// `TextEditor::textColourId`).
#[allow(clippy::too_many_arguments)]
pub fn single_line_styled(
    ui: &mut Ui,
    pixels: &PixelPainter,
    scope: &Scope,
    salt: impl std::hash::Hash + std::fmt::Debug,
    bounds: Rect,
    text: &mut String,
    prompt: &str,
    style: TextStyle,
) -> TextEditorEvents {
    // OpenGlTextEditor::resized: the indents.
    let height = bounds.height as f32;
    let left_indent = (height * 0.2) as i32;
    let top_indent = (height * if style.mono { 0.17 } else { 0.15 }) as i32;
    let inner = Rect::new(
        bounds.x + left_indent,
        bounds.y + top_indent,
        bounds.width - left_indent,
        bounds.height - top_indent,
    );
    edit(ui, pixels, scope, salt, inner, text, prompt, style, None)
}

/// A multi-line `OpenGlTextEditor` (`setMultiLine(true)`, Return starting a
/// new line): its indents are the label rounding on every side, its text
/// wraps, and it holds at most `max_chars` characters
/// (`setInputRestrictions`).
#[allow(clippy::too_many_arguments)]
pub fn multi_line(
    ui: &mut Ui,
    pixels: &PixelPainter,
    scope: &Scope,
    salt: impl std::hash::Hash + std::fmt::Debug,
    bounds: Rect,
    text: &mut String,
    prompt: &str,
    style: TextStyle,
    max_chars: usize,
) -> TextEditorEvents {
    let indent = scope.value(ValueId::LabelBackgroundRounding) as i32;
    let inner = Rect::new(
        bounds.x + indent,
        bounds.y + indent,
        bounds.width - 2 * indent,
        bounds.height - 2 * indent,
    );
    edit(
        ui,
        pixels,
        scope,
        salt,
        inner,
        text,
        prompt,
        style,
        Some(max_chars),
    )
}

/// A field editing `text` in `inner`: one line, or wrapped lines up to
/// `max_chars` characters.
#[allow(clippy::too_many_arguments)]
fn edit(
    ui: &mut Ui,
    pixels: &PixelPainter,
    scope: &Scope,
    salt: impl std::hash::Hash + std::fmt::Debug,
    inner: Rect,
    text: &mut String,
    prompt: &str,
    style: TextStyle,
    max_chars: Option<usize>,
) -> TextEditorEvents {
    let font_size = style.font_size;
    let face = if style.mono { Face::Mono } else { Face::Light };
    let body_text = scope.color(ColorId::BodyText);

    let id = ui.scope_id().with(("text editor", salt));
    // Whether it had the keyboard last frame: egui drops focus on Escape
    // before the field sees the key.
    let focus_key = id.with("had focus");
    let had_focus = ui.data(|data| data.get_temp::<bool>(focus_key).unwrap_or(false));
    let escape = had_focus && ui.input(|input| input.key_pressed(Key::Escape));
    let return_pressed =
        max_chars.is_none() && had_focus && ui.input(|input| input.key_pressed(Key::Enter));

    let rect = pixels.rect(inner.into());
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    child.visuals_mut().selection.bg_fill = scope.color(ColorId::TextEditorSelection).to_egui();
    child.visuals_mut().text_cursor.stroke.color = scope.color(ColorId::TextEditorCaret).to_egui();
    let edit = match max_chars {
        None => egui::TextEdit::singleline(text).align(if style.centred {
            egui::Align2::CENTER_CENTER
        } else {
            egui::Align2::LEFT_CENTER
        }),
        Some(max) => egui::TextEdit::multiline(text).char_limit(max),
    }
    .id(id)
    .frame(egui::Frame::NONE)
    .margin(egui::Margin::ZERO)
    .font(face.at(pixels.points(font_size)))
    .text_color(scope.color(style.colour).to_egui())
    .desired_width(rect.width());
    let response = child.add_sized(rect.size(), edit);
    // The prompt (drawn below, as upstream draws it) names the field.
    let value = text.clone();
    response.widget_info(|| egui::WidgetInfo::text_edit(true, &value, &value, prompt));
    if response.clicked() {
        ui.ctx()
            .data_mut(|data| data.insert_temp(window_focus_request(), true));
    }
    if escape {
        // JUCE's TextEditor keeps focus on Escape; egui's gives it up.
        response.request_focus();
    }
    let has_focus = response.has_focus() || escape;
    ui.data_mut(|data| data.insert_temp(focus_key, has_focus));

    if text.is_empty() && !has_focus && !style.hide_prompt {
        // A multi-line field shows its prompt on its first line.
        let line = match max_chars {
            None => inner,
            Some(_) => Rect::new(inner.x, inner.y, inner.width, (font_size * 1.3) as i32),
        };
        pixels.text_line(
            prompt,
            line,
            (face, font_size, body_text.with_multiplied_alpha(0.5)),
            Justify::Left,
            Overflow::Curtail,
        );
    }
    TextEditorEvents {
        changed: response.changed(),
        escape,
        return_pressed,
        has_focus,
        lost_focus: had_focus && !has_focus,
    }
}
