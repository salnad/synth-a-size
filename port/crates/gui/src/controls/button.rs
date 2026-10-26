// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `SynthButton`: an on/off control, shown as text on a rounded background
//! or as a section's power button.
//!
//! Ports `SynthButton::mouseDown` / `mouseUp`
//! (`vital/src/interface/editor_components/synth_button.cpp`), JUCE's
//! toggle-on-release `Button` behaviour, and `OpenGlButtonComponent`'s text
//! and power button styles (`setColors`, `renderTextButton`,
//! `renderPowerButton`, `incrementHover`).

use egui::{PointerButton, Sense, Ui};

use crate::fonts::Face;
use crate::host::Host;
use crate::layer::Layer;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `OpenGlButtonComponent::kHoverInc`: the hover fade per frame.
const HOVER_INC: f32 = 0.2;

/// How a button is drawn (`OpenGlButtonComponent::ButtonStyle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonStyle {
    /// `kTextButton`: the label on a text-component background.
    Text(&'static str),
    /// `kPowerButton`: a dot, with a halo while hovered or pressed
    /// (`SynthSection::setActivator`).
    Power,
}

/// A toggle bound to an on/off synth control.
#[derive(Clone, Copy, Debug)]
pub struct SynthButton {
    pub name: &'static str,
    pub style: ButtonStyle,
}

#[derive(Clone, Copy, Debug, Default)]
struct ButtonState {
    down: bool,
    hover_amount: f32,
}

impl SynthButton {
    /// A text button labelled `text`.
    pub fn text(name: &'static str, text: &'static str) -> SynthButton {
        SynthButton {
            name,
            style: ButtonStyle::Text(text),
        }
    }

    /// A section's power button (its activator).
    pub fn power(name: &'static str) -> SynthButton {
        SynthButton {
            name,
            style: ButtonStyle::Power,
        }
    }

    /// Whether the control is on.
    pub fn is_on(&self, host: &Host) -> bool {
        host.value(self.name) != 0.0
    }

    /// Handles the mouse and draws the button: a press begins the host
    /// gesture, a release over the button toggles the value and ends it.
    /// Returns true on that toggle (`buttonClicked`): a press and release
    /// both on the button.
    pub fn show(
        &self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        layer: &mut Layer,
        scope: &Scope,
        bounds: Rect,
    ) -> bool {
        let id = ui.scope_id().with(("synth button", self.name));
        let response = ui.interact(pixels.rect(bounds.into()), id, Sense::click());
        let mut state: ButtonState = ui.data(|data| data.get_temp(id)).unwrap_or_default();
        let primary_down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
        let down = (state.down || response.is_pointer_button_down_on()) && primary_down;
        if down && !state.down {
            host.begin(self.name);
        }
        let mut clicked = false;
        if state.down && !down {
            // Button::mouseUp: a release over the button toggles it, however
            // far the pointer moved while it was down.
            let pointer = ui.input(|input| input.pointer.interact_pos());
            if pointer.is_some_and(|pos| response.rect.contains(pos)) {
                let toggled = host.value(self.name) != 0.0;
                host.set(self.name, if toggled { 0.0 } else { 1.0 });
                clicked = true;
            }
            host.end(self.name);
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

        let toggled = host.value(self.name) != 0.0;
        match self.style {
            ButtonStyle::Text(text) => paint_text_button(
                layer,
                scope,
                bounds,
                text,
                toggled,
                down,
                state.hover_amount,
            ),
            ButtonStyle::Power => {
                paint_power_button(layer, scope, bounds, toggled, down, state.hover_amount)
            }
        }
        clicked
    }
}

/// An `OpenGlToggleButton` drawn as a text button whose state isn't a
/// parameter (`on` comes from elsewhere): handles the mouse at `bounds` and
/// draws it. Returns true when a release over it clicks it.
#[allow(clippy::too_many_arguments)]
pub fn text_toggle(
    ui: &mut Ui,
    pixels: &PixelPainter,
    layer: &mut Layer,
    scope: &Scope,
    id: egui::Id,
    text: &str,
    on: bool,
    bounds: Rect,
) -> bool {
    let response = ui.interact(pixels.rect(bounds.into()), id, Sense::click());
    let mut state: ButtonState = ui.data(|data| data.get_temp(id)).unwrap_or_default();
    let primary_down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
    let down = (state.down || response.is_pointer_button_down_on()) && primary_down;
    let mut clicked = false;
    if state.down && !down {
        let pointer = ui.input(|input| input.pointer.interact_pos());
        clicked = pointer.is_some_and(|pos| response.rect.contains(pos));
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
    paint_text_button(layer, scope, bounds, text, on, down, state.hover_amount);
    clicked
}

/// `OpenGlButtonComponent::renderTextButton` with the text style's colours.
fn paint_text_button(
    layer: &mut Layer,
    scope: &Scope,
    bounds: Rect,
    text: &str,
    toggled: bool,
    down: bool,
    hover_amount: f32,
) {
    let (active, hover_colour) = if toggled {
        let active = if down {
            ColorId::IconButtonOnPressed
        } else {
            ColorId::IconButtonOn
        };
        (scope.color(active), scope.color(ColorId::IconButtonOnHover))
    } else {
        let active = if down {
            ColorId::IconButtonOffPressed
        } else {
            ColorId::IconButtonOff
        };
        (
            scope.color(active),
            scope.color(ColorId::IconButtonOffHover),
        )
    };
    let colour = if down {
        active
    } else {
        active.interpolated_with(hover_colour, hover_amount)
    };

    let mut background = Quad::new(
        QuadKind::RoundedRectangle,
        bounds.into(),
        scope.color(ColorId::TextComponentBackground),
    );
    background.rounding = scope.value(ValueId::LabelBackgroundRounding);
    layer.quad(background);
    let font_size = scope.value(ValueId::ButtonFontSize);
    layer.text_centred(text, bounds, Face::Mono, font_size, colour);
}

/// `OpenGlButtonComponent::renderPowerButton` with the power style's
/// colours: the dot at 0.45 of the button, then a halo at 0.65 while
/// pressed or hovered.
fn paint_power_button(
    layer: &mut Layer,
    scope: &Scope,
    bounds: Rect,
    toggled: bool,
    down: bool,
    hover_amount: f32,
) {
    const POWER_RADIUS: f32 = 0.45;
    const POWER_HOVER_RADIUS: f32 = 0.65;
    let circle = |radius: f32, colour| {
        // setQuad(0, -r, -r, 2r, 2r) in the button's device coordinates.
        let width = radius * bounds.width as f32;
        let height = radius * bounds.height as f32;
        let x = bounds.x as f32 + (bounds.width as f32 - width) / 2.0;
        let y = bounds.y as f32 + (bounds.height as f32 - height) / 2.0;
        Quad::new(QuadKind::Circle, RectF::new(x, y, width, height), colour)
    };
    let colour = if toggled {
        scope.color(ColorId::PowerButtonOn)
    } else {
        scope.color(ColorId::PowerButtonOff)
    };
    layer.quad(circle(POWER_RADIUS, colour));
    if down {
        layer.quad(circle(
            POWER_HOVER_RADIUS,
            scope.color(ColorId::OverlayScreen),
        ));
    } else if hover_amount > 0.0 {
        let hover = scope
            .color(ColorId::LightenScreen)
            .with_multiplied_alpha(hover_amount);
        layer.quad(circle(POWER_HOVER_RADIUS, hover));
    }
}
