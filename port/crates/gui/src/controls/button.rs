// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `SynthButton` with a text label: an on/off control shown as text on a
//! rounded background.
//!
//! Ports `SynthButton::mouseDown` / `mouseUp`
//! (`vital/src/interface/editor_components/synth_button.cpp`), JUCE's
//! toggle-on-release `Button` behaviour, and `OpenGlButtonComponent`'s text
//! button style (`setColors`, `renderTextButton`, `incrementHover`).

use egui::{Sense, Ui};

use crate::fonts::Face;
use crate::host::Host;
use crate::layer::Layer;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `OpenGlButtonComponent::kHoverInc`: the hover fade per frame.
const HOVER_INC: f32 = 0.2;

/// A toggle bound to an on/off synth control.
#[derive(Clone, Copy, Debug)]
pub struct TextButton {
    pub name: &'static str,
    pub text: &'static str,
}

#[derive(Clone, Copy, Debug, Default)]
struct ButtonState {
    down: bool,
    hover_amount: f32,
}

impl TextButton {
    pub fn new(name: &'static str, text: &'static str) -> TextButton {
        TextButton { name, text }
    }

    /// Handles the mouse and draws the button: a press begins the host
    /// gesture, a release over the button toggles the value and ends it.
    pub fn show(
        &self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        layer: &mut Layer,
        scope: &Scope,
        bounds: Rect,
    ) {
        let id = ui.scope_id().with(("synth button", self.name));
        let response = ui.interact(pixels.rect(bounds.into()), id, Sense::click());
        let mut state: ButtonState = ui.data(|data| data.get_temp(id)).unwrap_or_default();
        let down = response.is_pointer_button_down_on() && !response.secondary_clicked();
        if down && !state.down {
            host.begin(self.name);
        }
        if response.clicked() {
            let toggled = host.value(self.name) != 0.0;
            host.set(self.name, if toggled { 0.0 } else { 1.0 });
        }
        if state.down && !down {
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
            active.interpolated_with(hover_colour, state.hover_amount)
        };

        let mut background = Quad::new(
            QuadKind::RoundedRectangle,
            bounds.into(),
            scope.color(ColorId::TextComponentBackground),
        );
        background.rounding = scope.value(ValueId::LabelBackgroundRounding);
        layer.quad(background);
        let font_size = scope.value(ValueId::ButtonFontSize);
        layer.text_centred(self.text, bounds, Face::Mono, font_size, colour);
    }
}
