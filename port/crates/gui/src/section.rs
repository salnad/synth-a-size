// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! What every section draws and how it places its controls.
//!
//! Ports the shared drawing and layout helpers of `SynthSection`
//! (`vital/src/interface/editor_sections/synth_section.cpp`): bodies, borders,
//! labels, text-component backgrounds and `placeKnobsInArea`. Rectangles are
//! in interface pixels.

use crate::colour::Colour;
use crate::fonts::Face;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `SynthSection::paintBody`.
pub fn paint_body(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    pixels.fill_rounded_rect(
        bounds,
        scope.value(ValueId::BodyRounding),
        scope.color(ColorId::Body),
    );
}

/// `SynthSection::paintBorder`.
pub fn paint_border(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    let rounding = scope.value(ValueId::BodyRounding) as i32 as f32;
    pixels.draw_rounded_rect(
        RectF::from(bounds).reduced(0.5),
        rounding,
        1.0,
        scope.color(ColorId::Border),
    );
}

/// `SynthSection::getLabelBackgroundBounds`.
pub fn label_background_bounds(scope: &Scope, bounds: Rect, text_component: bool) -> Rect {
    let background_height = scope.value(ValueId::LabelBackgroundHeight) as i32;
    let label_offset = if text_component {
        scope.value(ValueId::TextComponentLabelOffset) as i32
    } else {
        scope.value(ValueId::LabelOffset) as i32
    };
    let background_y = bounds.bottom() - background_height + label_offset;
    Rect::new(bounds.x, background_y, bounds.width, background_height)
}

/// `SynthSection::drawLabelBackground`.
pub fn draw_label_background(
    pixels: &PixelPainter,
    scope: &Scope,
    bounds: Rect,
    text_component: bool,
) {
    let rounding = scope.value(ValueId::LabelBackgroundRounding) as i32 as f32;
    let colour = scope.color(ColorId::LabelBackground);
    let label = label_background_bounds(scope, bounds, text_component);
    pixels.fill_rounded_rect(label, rounding, colour);
    if text_component && scope.color(ColorId::TextComponentBackground).alpha() != 0 {
        let half = RectF::new(
            label.x as f32,
            label.y as f32,
            label.width as f32,
            label.height as f32 / 2.0,
        );
        pixels.fill_rect(half, colour);
    }
}

/// `SynthSection::getLabelFont` with `setLabelFont`'s colour.
pub fn label_font(scope: &Scope) -> (Face, f32, Colour) {
    (
        Face::Regular,
        scope.value(ValueId::LabelHeight),
        scope.color(ColorId::BodyText),
    )
}

/// `SynthSection::drawLabel` (after `setLabelFont`).
pub fn draw_label(
    pixels: &PixelPainter,
    scope: &Scope,
    text: &str,
    bounds: Rect,
    text_component: bool,
) {
    if bounds.width <= 0 || bounds.height <= 0 {
        return;
    }
    draw_label_background(pixels, scope, bounds, text_component);
    let background = label_background_bounds(scope, bounds, text_component);
    let (face, size, colour) = label_font(scope);
    let text_bounds = Rect::new(bounds.x, background.y, bounds.width, background.height);
    pixels.text_centred(text, text_bounds, face, size, colour);
}

/// `SynthSection::drawTextComponentBackground`.
pub fn draw_text_component_background(
    pixels: &PixelPainter,
    scope: &Scope,
    bounds: Rect,
    extend_to_label: bool,
) {
    if bounds.width <= 0 || bounds.height <= 0 {
        return;
    }
    let colour = scope.color(ColorId::TextComponentBackground);
    let y = bounds.y;
    let mut rounding = bounds.height / 2;
    if extend_to_label {
        let label_bottom = bounds.bottom() + scope.value(ValueId::TextComponentLabelOffset) as i32;
        rounding = scope.value(ValueId::LabelBackgroundRounding) as i32;
        pixels.fill_rounded_rect(bounds, rounding as f32, colour);
        let extend_y = y + bounds.height / 2;
        pixels.fill_rect(
            Rect::new(
                bounds.x,
                extend_y,
                bounds.width,
                label_bottom - extend_y - rounding,
            ),
            colour,
        );
    } else {
        pixels.fill_rounded_rect(bounds, rounding as f32, colour);
    }
}

/// `SynthSection::placeKnobsInArea`: `count` knobs side by side in `area`.
pub fn place_knobs_in_area(scope: &Scope, area: Rect, count: usize) -> Vec<Rect> {
    let widget_margin = scope.widget_margin() as i32;
    let component_width =
        (area.width - (count as i32 + 1) * widget_margin) as f32 / (1.0f32 * count as f32);
    let y = area.y;
    let height = area.height - widget_margin;
    let mut x = (area.x + widget_margin) as f32;
    let mut bounds = Vec::with_capacity(count);
    for _ in 0..count {
        let left = x.round() as i32;
        let right = (x + component_width).round() as i32;
        bounds.push(Rect::new(left, y, right - left, height));
        x += component_width + widget_margin as f32;
    }
    bounds
}
