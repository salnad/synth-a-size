// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `OpenGlShapeButton` and JUCE's `ShapeButton` with an empty shape: a
//! button drawn as a path in the icon colours, fading to its hover colour.
//!
//! Ports `OpenGlShapeButtonComponent` (`render`, `incrementHover`) from
//! `vital/src/interface/editor_components/synth_button.cpp`. The paths are
//! upstream's `Paths::prev` / `Paths::next` chevrons; icon paths come with
//! `controls::icon`.

use egui::{Id, PointerButton, Sense, Ui};

use crate::colour::Colour;
use crate::layer::Layer;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::skin::ColorId;

/// `OpenGlShapeButtonComponent::kHoverInc`.
const HOVER_INC: f32 = 0.2;

/// What a shape button draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// `Paths::prev`.
    Prev,
    /// `Paths::next`.
    Next,
    /// `setShape(Path())`: an invisible button over a text component.
    None,
    /// A `SynthButton` with `setNoBackground` (`kJustText`) that isn't a
    /// parameter: its text in the button font.
    Text(&'static str),
}

/// `OpenGlShapeButtonComponent::setColors`: off, hover and pressed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeColours {
    pub normal: Colour,
    pub hover: Colour,
    pub down: Colour,
}

impl ShapeColours {
    /// The icon button colours of `scope`.
    pub fn icon(scope: &Scope) -> ShapeColours {
        ShapeColours {
            normal: scope.color(ColorId::IconButtonOff),
            hover: scope.color(ColorId::IconButtonOffHover),
            down: scope.color(ColorId::IconButtonOffPressed),
        }
    }

    /// The UI button colours, which a section's preset selector arrows take
    /// (`preset_selector_->setColour(Skin::kIconButtonOff, kUiButton)`, ...).
    pub fn ui(scope: &Scope) -> ShapeColours {
        ShapeColours {
            normal: scope.color(ColorId::UiButton),
            hover: scope.color(ColorId::UiButtonHover),
            down: scope.color(ColorId::UiButtonPressed),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ShapeState {
    down: bool,
    hover_amount: f32,
}

/// What a shape button did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShapeEvent {
    /// The primary button went down on it (`setTriggeredOnMouseDown`).
    pub pressed: bool,
    /// It was released over the button after a press on it (JUCE's
    /// `Button::clicked`).
    pub clicked: bool,
}

/// A shape button's look and what it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeButton {
    pub shape: Shape,
    pub colours: ShapeColours,
}

/// Handles the mouse for a shape button at `bounds` and draws it into
/// `layer`.
pub fn show(
    ui: &mut Ui,
    pixels: &PixelPainter,
    layer: &mut Layer,
    scope: &Scope,
    id: Id,
    button: ShapeButton,
    bounds: Rect,
) -> ShapeEvent {
    let ShapeButton { shape, colours } = button;
    let response = ui.interact(pixels.rect(bounds.into()), id, Sense::click());
    let mut state: ShapeState = ui.data(|data| data.get_temp(id)).unwrap_or_default();
    let primary_down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
    let down = (state.down || response.is_pointer_button_down_on()) && primary_down;
    let mut event = ShapeEvent::default();
    if down && !state.down {
        event.pressed = true;
    }
    if state.down && !down {
        let pointer = ui.input(|input| input.pointer.interact_pos());
        event.clicked = pointer.is_some_and(|pos| response.rect.contains(pos));
    }
    state.down = down;

    let hover = response.hovered();
    let target = if hover { 1.0 } else { 0.0 };
    if state.hover_amount != target {
        state.hover_amount = if hover {
            (state.hover_amount + HOVER_INC).min(1.0)
        } else {
            (state.hover_amount - HOVER_INC).max(0.0)
        };
        ui.ctx().request_repaint();
    }
    ui.data_mut(|data| data.insert_temp(id, state));

    let colour = if down {
        colours.down
    } else {
        colours
            .normal
            .interpolated_with(colours.hover, state.hover_amount)
    };
    match shape {
        Shape::Prev => layer.custom(move |pixels| paint_chevron(pixels, bounds, &PREV, colour)),
        Shape::Next => layer.custom(move |pixels| paint_chevron(pixels, bounds, &NEXT, colour)),
        Shape::None => {}
        Shape::Text(text) => {
            let size = scope.value(crate::skin::ValueId::ButtonFontSize);
            layer.text_centred(text, bounds, crate::fonts::Face::Mono, size, colour);
        }
    }
    event
}

/// `Paths::prev` / `Paths::next`: a chevron in unit coordinates of the
/// button's square, stroked 0.05 of its size with round ends. Upstream
/// fills the outline of a curved-join, round-cap stroke scaled to fit; this
/// draws it as egui's polyline until `controls::path`'s stroking (#116) is
/// on main.
pub fn paint_chevron(pixels: &PixelPainter, bounds: Rect, path: &[(f32, f32)], colour: Colour) {
    let size = bounds.width.min(bounds.height) as f32;
    let points: Vec<(f32, f32)> = path
        .iter()
        .map(|&(x, y)| (bounds.x as f32 + x * size, bounds.y as f32 + y * size))
        .collect();
    pixels.polyline(&points, 0.05 * size, colour);
}

/// `Paths::prev`'s chevron.
pub const PREV: [(f32, f32); 3] = [(0.65, 0.3), (0.35, 0.5), (0.65, 0.7)];
/// `Paths::next`'s chevron.
pub const NEXT: [(f32, f32); 3] = [(0.35, 0.3), (0.65, 0.5), (0.35, 0.7)];
