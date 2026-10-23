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
use crate::primitives::{Quad, QuadKind};
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

/// `SynthSection::kPowerButtonPaddingPercent`.
const POWER_BUTTON_PADDING_PERCENT: f32 = 0.29;
/// `SynthSection::paintHeadingText`'s font size, before scaling.
const HEADING_TEXT_HEIGHT: f32 = 14.0;

/// `SynthSection::getTitleWidth`: the height of a section's title strip.
pub fn title_width(scope: &Scope) -> f32 {
    scope.value(ValueId::TitleWidth)
}

/// `SynthSection::getPowerButtonBounds`: the activator's square at the
/// left of the title strip of a section at `bounds` (the default power
/// button offset is 0).
pub fn power_button_bounds(scope: &Scope, bounds: Rect) -> Rect {
    let title_width = title_width(scope) as i32;
    Rect::new(bounds.x, bounds.y, title_width, title_width)
}

/// `SynthSection::getTitleBounds`: where the heading text goes. It starts
/// after the power button if there is one, and ends at `title_text_right`
/// (pixels from the section's left; its width when there is no preset
/// selector).
pub fn title_bounds(
    scope: &Scope,
    bounds: Rect,
    has_activator: bool,
    title_text_right: i32,
) -> Rect {
    let title_width = title_width(scope) as i32;
    let from = if has_activator {
        (power_button_bounds(scope, bounds).right() - bounds.x) as f32
            - title_width as f32 * POWER_BUTTON_PADDING_PERCENT
    } else {
        0.0
    } as i32;
    Rect::new(
        bounds.x + from,
        bounds.y,
        title_text_right - from,
        title_width,
    )
}

/// `SynthSection::sideways_heading_`: where a section's title goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    /// The default: a strip down the left edge, the name reading upwards.
    Sideways,
    /// `setSidewaysHeading(false)`: a strip across the top.
    Horizontal,
}

/// `SynthSection::paintContainer`'s heading background: the rounded
/// rectangle it fills and the clip it fills it under.
pub fn heading_strip(scope: &Scope, bounds: Rect, heading: Heading) -> (Rect, Rect) {
    let title_width = title_width(scope) as i32;
    match heading {
        Heading::Sideways => (
            Rect::new(bounds.x, bounds.y, 2 * title_width, bounds.height),
            Rect::new(bounds.x, bounds.y, title_width, bounds.height),
        ),
        Heading::Horizontal => (
            bounds,
            Rect::new(bounds.x, bounds.y, bounds.width, title_width),
        ),
    }
}

/// `SynthSection::paintContainer`: the body, with the heading background
/// over the title strip.
pub fn paint_container(pixels: &PixelPainter, scope: &Scope, bounds: Rect, heading: Heading) {
    paint_body(pixels, scope, bounds);
    let rounding = scope.value(ValueId::BodyRounding);
    let (fill, clip) = heading_strip(scope, bounds, heading);
    pixels.fill_rounded_rect_clipped(fill, rounding, scope.color(ColorId::BodyHeading), clip);
}

/// `SynthSection::paintHeadingText` with a horizontal heading: the
/// section's name centred in `title` (`title_bounds`).
pub fn paint_heading_text(pixels: &PixelPainter, scope: &Scope, name: &str, title: Rect) {
    pixels.text_centred(
        name,
        title,
        Face::Light,
        scope.size_ratio() * HEADING_TEXT_HEIGHT,
        scope.color(ColorId::HeadingText),
    );
}

/// `SynthSection::paintSidewaysHeadingText`'s text box, in the section's
/// coordinates: upstream draws in `(0, 0, height, title_width)` rotated
/// -π/2 about the bottom-left corner, so the box runs up the left edge from
/// the bottom. With an activator it stops `title_width / 2` short of the
/// top.
pub fn sideways_title_bounds(scope: &Scope, bounds: Rect, has_activator: bool) -> Rect {
    let title_width = title_width(scope) as i32;
    let length = if has_activator {
        bounds.height - title_width / 2
    } else {
        bounds.height
    };
    Rect::new(bounds.x, bounds.bottom() - length, title_width, length)
}

/// `SynthSection::paintSidewaysHeadingText`: the section's name reading
/// upwards, centred in `sideways_title_bounds`.
pub fn paint_sideways_heading_text(
    pixels: &PixelPainter,
    scope: &Scope,
    name: &str,
    bounds: Rect,
    has_activator: bool,
) {
    pixels.text_centred_rotated(
        name,
        sideways_title_bounds(scope, bounds, has_activator),
        Face::Light,
        scope.size_ratio() * HEADING_TEXT_HEIGHT,
        scope.color(ColorId::HeadingText),
        -std::f32::consts::FRAC_PI_2,
    );
}

/// `SynthSection::OffOverlay`: the veil over a section whose activator is
/// off, drawn over its controls and under its power button.
pub fn off_overlay(scope: &Scope, bounds: Rect) -> Quad {
    Quad::new(
        QuadKind::Color,
        bounds.into(),
        scope.color(ColorId::Background).with_multiplied_alpha(0.8),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::{Section, Skin};

    #[test]
    fn heading_strips_follow_paint_container() {
        let skin = Skin::default();
        let scope = Scope::top(&skin, 1.0);
        let scope = scope.within(&[Section::Chorus, Section::AllEffects]);
        assert_eq!(title_width(&scope), 32.0);
        let bounds = Rect::new(100, 50, 400, 120);
        // Sideways: a 2 × title-width rounded rectangle clipped to the left
        // title width, so only its left corners show.
        assert_eq!(
            heading_strip(&scope, bounds, Heading::Sideways),
            (Rect::new(100, 50, 64, 120), Rect::new(100, 50, 32, 120))
        );
        // Horizontal: the body's rounded rectangle clipped to the top strip.
        assert_eq!(
            heading_strip(&scope, bounds, Heading::Horizontal),
            (bounds, Rect::new(100, 50, 400, 32))
        );
    }

    #[test]
    fn sideways_titles_run_up_the_left_edge() {
        let skin = Skin::default();
        let scope = Scope::top(&skin, 1.0);
        let bounds = Rect::new(100, 50, 400, 121);
        // Rectangle(0, 0, height, title_width) turned -π/2 about the
        // bottom-left corner.
        assert_eq!(
            sideways_title_bounds(&scope, bounds, false),
            Rect::new(100, 50, 32, 121)
        );
        // With a power button it stops title_width / 2 short of the top.
        assert_eq!(
            sideways_title_bounds(&scope, bounds, true),
            Rect::new(100, 66, 32, 105)
        );
        // Odd title widths halve as ints, as upstream's int title_width.
        let scaled = Scope::top(&skin, 1.3);
        let title_width = title_width(&scaled) as i32;
        assert_eq!(title_width, 41);
        assert_eq!(
            sideways_title_bounds(&scaled, bounds, true),
            Rect::new(100, 50 + 20, 41, 121 - 20)
        );
    }
}
