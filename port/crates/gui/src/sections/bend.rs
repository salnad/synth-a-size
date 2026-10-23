// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Bend section: the pitch and mod wheels. Ports `BendSection`
//! (`vital/src/interface/editor_sections/bend_section.cpp`).
//!
//! Upstream also sends a wheel's GUI moves to every channel's voices
//! (`pitchWheelGuiChanged`, `modWheelGuiChanged`); the port's engine does
//! that for any change of the `pitch_wheel` and `mod_wheel` controls
//! (`Synth::set_control`), so the wheels only set their host parameters.

use super::Panel;
use crate::controls::slider::{Placement, Slider, SliderStyle};
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::section::{paint_body, paint_border};
use crate::skin::Section;

/// The section's skin override (`setSkinOverride(Skin::kKeyboard)`).
pub const SKIN: &[Section] = &[Section::Keyboard];

/// `BendSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BendLayout {
    pub bounds: Rect,
    pub pitch_wheel: Rect,
    pub mod_wheel: Rect,
}

impl BendLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> BendLayout {
        let widget_margin = scope.widget_margin() as i32;
        let mut x_padding = (bounds.width as f32 * 0.16) as i32;
        x_padding -= (bounds.width + x_padding) % 2;
        let wheel_height = bounds.height - 2 * widget_margin;
        let right1 = (bounds.width - x_padding) / 2;
        let pitch_wheel = Rect::new(x_padding, widget_margin, right1 - x_padding, wheel_height);
        let x2 = right1 + x_padding;
        let mod_wheel = Rect::new(
            x2,
            widget_margin,
            bounds.width - x_padding - x2,
            wheel_height,
        );
        BendLayout {
            bounds,
            pitch_wheel: pitch_wheel.translated(bounds.x, bounds.y),
            mod_wheel: mod_wheel.translated(bounds.x, bounds.y),
        }
    }
}

/// The section's controls (`BendSection`'s constructor).
pub struct BendSection {
    pitch_wheel: Slider,
    mod_wheel: Slider,
}

impl Default for BendSection {
    fn default() -> BendSection {
        /// `ControlWheel`'s `setSensitivity(0.5f)`.
        const WHEEL_SENSITIVITY: f32 = 0.5;
        let wheel = |name, spring_back| {
            let mut slider = Slider::new(name, SliderStyle::Wheel { spring_back })
                .sensitivity(WHEEL_SENSITIVITY);
            slider.popup_placement = Placement::Above;
            slider
        };
        BendSection {
            pitch_wheel: wheel("pitch_wheel", true),
            mod_wheel: wheel("mod_wheel", false),
        }
    }
}

impl BendSection {
    /// `BendSection::paintBackground`: the wheels draw no shadows.
    pub fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, layout: &BendLayout) {
        paint_body(pixels, scope, layout.bounds);
        paint_border(pixels, scope, layout.bounds);
    }

    pub fn show(&self, panel: &mut Panel, layout: &BendLayout) {
        panel.slider(&self.pitch_wheel, layout.pitch_wheel);
        panel.slider(&self.mod_wheel, layout.mod_wheel);
    }
}
