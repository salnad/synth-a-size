// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! An `OpenGlToggleButton` with `setUiButton(primary)`
//! (`vital/src/interface/editor_components/synth_button.cpp`,
//! `OpenGlButtonComponent::renderUiButton`): a rounded label background in
//! the UI button colours (the action colours for a primary button), fading
//! to the hover colour, with its text in the light face at 45% of its
//! height.

use egui::{Id, PointerButton, Sense, Ui};

use crate::fonts::Face;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};
use crate::text::{Justify, Overflow};

/// `OpenGlButtonComponent::kHoverInc`.
const HOVER_INC: f32 = 0.2;
/// `OpenGlToggleButton::resized`'s `kUiButtonSizeMult`.
const TEXT_SIZE_MULT: f32 = 0.45;

#[derive(Clone, Copy, Debug, Default)]
struct UiButtonState {
    down: bool,
    hover_amount: f32,
}

/// Draws the button labelled `text` at `bounds` and handles the mouse.
/// Returns true when it is clicked: a press and a release both on it
/// (`Button::clicked`).
pub fn show(
    ui: &mut Ui,
    pixels: &PixelPainter,
    scope: &Scope,
    id: Id,
    text: &str,
    primary: bool,
    bounds: Rect,
) -> bool {
    let response = ui.interact(pixels.rect(bounds.into()), id, Sense::click());
    let mut state: UiButtonState = ui.data(|data| data.get_temp(id)).unwrap_or_default();
    let primary_down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
    let down = (state.down || response.is_pointer_button_down_on()) && primary_down;
    let mut clicked = false;
    if state.down && !down {
        let pointer = ui.input(|input| input.pointer.interact_pos());
        clicked = pointer.is_some_and(|pos| response.rect.contains(pos));
    }
    state.down = down;

    // incrementHover.
    let target = if response.hovered() { 1.0 } else { 0.0 };
    if state.hover_amount != target {
        state.hover_amount = if target > 0.0 {
            (state.hover_amount + HOVER_INC).min(1.0)
        } else {
            (state.hover_amount - HOVER_INC).max(0.0)
        };
        ui.ctx().request_repaint();
    }
    ui.data_mut(|data| data.insert_temp(id, state));

    let (on, pressed, hover) = if primary {
        (
            ColorId::UiActionButton,
            ColorId::UiActionButtonPressed,
            ColorId::UiActionButtonHover,
        )
    } else {
        (
            ColorId::UiButton,
            ColorId::UiButtonPressed,
            ColorId::UiButtonHover,
        )
    };
    let colour = if down {
        scope.color(pressed)
    } else {
        scope
            .color(on)
            .interpolated_with(scope.color(hover), state.hover_amount)
    };
    pixels.fill_rounded_rect(
        bounds,
        scope.value(ValueId::LabelBackgroundRounding),
        colour,
    );
    pixels.text_line(
        text,
        bounds,
        (
            Face::Light,
            TEXT_SIZE_MULT * bounds.height as f32,
            scope.color(ColorId::UiButtonText),
        ),
        Justify::Centred,
        Overflow::Fit,
    );
    clicked
}
