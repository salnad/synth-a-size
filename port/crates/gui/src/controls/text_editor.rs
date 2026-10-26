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
}

/// A single-line `OpenGlTextEditor` at `bounds` (pixels) editing `text`,
/// showing `prompt` while empty and unfocused (`setTextToShowWhenEmpty`, in
/// the body text colour at half its alpha).
pub fn single_line(
    ui: &mut Ui,
    pixels: &PixelPainter,
    scope: &Scope,
    salt: impl std::hash::Hash + std::fmt::Debug,
    bounds: Rect,
    text: &mut String,
    prompt: &str,
) -> TextEditorEvents {
    // OpenGlTextEditor::resized and applyFont: the indents and the light
    // face at half the height.
    let height = bounds.height as f32;
    let left_indent = (height * 0.2) as i32;
    let top_indent = (height * 0.15) as i32;
    let inner = Rect::new(
        bounds.x + left_indent,
        bounds.y + top_indent,
        bounds.width - left_indent,
        bounds.height - top_indent,
    );
    let font_size = height / 2.0;
    let body_text = scope.color(ColorId::BodyText);

    let id = ui.scope_id().with(("text editor", salt));
    // Whether it had the keyboard last frame: egui drops focus on Escape
    // before the field sees the key.
    let focus_key = id.with("had focus");
    let had_focus = ui.data(|data| data.get_temp::<bool>(focus_key).unwrap_or(false));
    let escape = had_focus && ui.input(|input| input.key_pressed(Key::Escape));
    let return_pressed = had_focus && ui.input(|input| input.key_pressed(Key::Enter));

    let rect = pixels.rect(inner.into());
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    child.visuals_mut().selection.bg_fill = scope.color(ColorId::TextEditorSelection).to_egui();
    child.visuals_mut().text_cursor.stroke.color = scope.color(ColorId::TextEditorCaret).to_egui();
    let edit = egui::TextEdit::singleline(text)
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .font(Face::Light.at(pixels.points(font_size)))
        .text_color(body_text.to_egui())
        .align(egui::Align2::LEFT_CENTER)
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

    if text.is_empty() && !has_focus {
        pixels.text_line(
            prompt,
            inner,
            (Face::Light, font_size, body_text.with_multiplied_alpha(0.5)),
            Justify::Left,
            Overflow::Curtail,
        );
    }
    TextEditorEvents {
        changed: response.changed(),
        escape,
        return_pressed,
        has_focus,
    }
}
