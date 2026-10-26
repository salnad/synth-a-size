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
    let (_, size, _) = label_font(scope);
    draw_label_with_size(pixels, scope, text, bounds, text_component, size);
}

/// `SynthSection::drawLabel` in the label face and colour at point height
/// `size`, for sections that set their own font before drawing labels (the
/// reverb's `size_ratio * 10`).
pub fn draw_label_with_size(
    pixels: &PixelPainter,
    scope: &Scope,
    text: &str,
    bounds: Rect,
    text_component: bool,
    size: f32,
) {
    if bounds.width <= 0 || bounds.height <= 0 {
        return;
    }
    draw_label_background(pixels, scope, bounds, text_component);
    let background = label_background_bounds(scope, bounds, text_component);
    let (face, _, colour) = label_font(scope);
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

/// `SynthSection::getSliderWidth`: a linear slider's full width, overlap
/// included.
pub fn slider_width(scope: &Scope) -> i32 {
    scope.value(ValueId::SliderWidth) as i32
}

/// `SynthSection::getSliderOverlap`: how far a linear slider's bounds reach
/// past its drawn track on each side.
pub fn slider_overlap(scope: &Scope) -> i32 {
    let total_width = slider_width(scope);
    let extra = total_width % 2;
    let width = ((0.26f32 * total_width as f32 * 0.5).floor() * 2.0) as i32 + extra;
    (total_width - width) / 2
}

/// `SynthSection::getSliderOverlapWithSpace`.
pub fn slider_overlap_with_space(scope: &Scope) -> i32 {
    slider_overlap(scope) - scope.widget_margin() as i32
}

/// `SynthSection::placeJointControls`: two square controls at the ends of
/// `(x, y, width, height)`, and the widget between them under the label.
/// Returns (left, right, widget).
pub fn place_joint_controls(
    scope: &Scope,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> (Rect, Rect, Rect) {
    let width_control = height;
    let label_height = scope.value(ValueId::LabelBackgroundHeight) as i32;
    (
        Rect::new(x, y, width_control, height),
        Rect::new(x + width - width_control, y, width_control, height),
        Rect::new(
            x + width_control,
            y + label_height,
            width - 2 * width_control,
            height - label_height,
        ),
    )
}

/// `SynthSection::paintJointControlBackground`.
pub fn paint_joint_control_background(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    let Rect {
        x,
        y,
        width,
        height,
    } = bounds;
    let rounding = scope.value(ValueId::LabelBackgroundRounding);
    pixels.fill_rect(
        RectF::new(
            x as f32 + rounding,
            y as f32,
            width as f32 - 2.0 * rounding,
            height as f32 / 2.0,
        ),
        scope.color(ColorId::LabelBackground),
    );

    let label_height = scope.value(ValueId::LabelBackgroundHeight) as i32;
    let half_label_height = label_height / 2;
    let side_width = height;
    let text_background = scope.color(ColorId::TextComponentBackground);
    pixels.fill_rounded_rect(
        Rect::new(x, y + half_label_height, width, height - half_label_height),
        rounding,
        text_background,
    );
    pixels.fill_rounded_rect(
        Rect::new(x, y, side_width, height),
        rounding,
        text_background,
    );
    pixels.fill_rounded_rect(
        Rect::new(x + width - side_width, y, side_width, height),
        rounding,
        text_background,
    );

    let mut label_colour = scope.color(ColorId::LabelBackground);
    if label_colour.alpha() == 0 {
        label_colour = scope.color(ColorId::Body);
    }
    let rect_width = (width - 2 * side_width).max(0);
    pixels.fill_rect(
        Rect::new(x + side_width, y, rect_width, half_label_height),
        label_colour,
    );
    pixels.fill_rounded_rect(
        Rect::new(x + side_width, y, rect_width, label_height),
        rounding,
        label_colour,
    );
}

/// `SynthSection::paintJointControl`: the background and `name` across its
/// top.
pub fn paint_joint_control(pixels: &PixelPainter, scope: &Scope, bounds: Rect, name: &str) {
    paint_joint_control_background(pixels, scope, bounds);
    let (face, size, colour) = label_font(scope);
    let label_height = scope.value(ValueId::LabelBackgroundHeight) as i32;
    pixels.text_centred(
        name,
        Rect::new(bounds.x, bounds.y, bounds.width, label_height),
        face,
        size,
        colour,
    );
}

/// `SynthSection::drawLabelConnectionForComponents`: the bar joining two
/// knobs' labels.
pub fn draw_label_connection(pixels: &PixelPainter, scope: &Scope, left: Rect, right: Rect) {
    let label_offset = scope.value(ValueId::LabelOffset) as i32;
    let background_height = scope.value(ValueId::LabelBackgroundHeight) as i32;
    let background_y = left.bottom() - background_height + label_offset;
    let centre_x = |r: Rect| r.x + r.width / 2;
    pixels.fill_rect(
        Rect::new(
            centre_x(left),
            background_y,
            centre_x(right) - centre_x(left),
            background_height,
        ),
        scope.color(ColorId::LabelConnection),
    );
}

/// `SynthSection::placeTempoControls` for a rate area at `(x, y, width,
/// height)`: the rate slider and, at its right, the sync selector.
/// Returns (rate, sync, the rate's modulation area relative to the rate).
pub fn place_tempo_controls(
    scope: &Scope,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> (Rect, Rect, Rect) {
    const MAX_SYNC_WIDTH_RATIO: f32 = 0.35;
    let sync_width =
        (width as f32 * MAX_SYNC_WIDTH_RATIO).min(scope.value(ValueId::TextComponentHeight)) as i32;
    let sync_y = (y as f32
        + (height - sync_width) as f32 / 2.0
        + scope.value(ValueId::TextComponentOffset)) as i32;
    let sync = Rect::new(x + width - sync_width, sync_y, sync_width, sync_width);
    let rate = Rect::new(x, y, width - sync_width, height);
    let modulation_area = Rect::new(0, sync_y - y, rate.width, sync_width);
    (rate, sync, modulation_area)
}

/// The keytrack sliders' divider (`transpose_tune_divider_`): a one-pixel
/// line at the centre of the rate's modulation area, a quarter of its
/// height in from each end.
pub fn keytrack_divider(rate: Rect, modulation_area: Rect) -> Rect {
    let area = modulation_area.translated(rate.x, rate.y);
    let reduce = area.height / 4;
    let reduced = Rect::new(
        area.x + reduce,
        area.y + reduce,
        area.width - 2 * reduce,
        area.height - 2 * reduce,
    );
    Rect::new(reduced.x + reduced.width / 2, reduced.y, 1, reduced.height)
}

/// `OpenGlComponent::paintBackground`: a widget's backdrop. Its rounded
/// corners are drawn over it in the body colour (`renderCorners`).
pub fn paint_widget_background(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    pixels.fill_rect(bounds, scope.color(ColorId::WidgetBackground));
}

/// `SynthSection::getComponentShadowWidth`.
pub fn component_shadow_width(scope: &Scope) -> i32 {
    (scope.size_ratio() * 2.0).round() as i32
}

/// `SynthSection::paintTabShadow`: a shadow fading out round `bounds`,
/// with rounded corners.
pub fn paint_tab_shadow(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    use crate::paint::{Gradient, GradientKind};
    const CORNER_SCALE: f32 = 0.707_106_77;
    let corner_size = scope.value(ValueId::BodyRounding) as i32;
    let shadow_size = component_shadow_width(scope);
    let corner_and_shadow = corner_size + shadow_size;
    let corner_shadow_offset = corner_size as f32 - corner_and_shadow as f32 * CORNER_SCALE;
    let corner_ratio = corner_size as f32 / corner_and_shadow as f32;

    let shadow = scope.color(ColorId::Shadow);
    let transparent = shadow.with_alpha(0);
    let (left, top, right, bottom) = (bounds.x, bounds.y, bounds.right(), bounds.bottom());
    let (l, t, r, b) = (left as f32, top as f32, right as f32, bottom as f32);
    let s = shadow_size as f32;
    let linear = |from: (f32, f32), to: (f32, f32)| {
        Gradient::new(GradientKind::Linear, from, shadow, to, transparent)
    };

    pixels.fill_rect_gradient(
        Rect::new(
            left - shadow_size,
            top + corner_size,
            shadow_size,
            bottom - top - corner_size * 2,
        ),
        &linear((l, 0.0), (l - s, 0.0)),
    );
    pixels.fill_rect_gradient(
        Rect::new(
            right,
            top + corner_size,
            shadow_size,
            bottom - top - corner_size * 2,
        ),
        &linear((r, 0.0), (r + s, 0.0)),
    );
    pixels.fill_rect_gradient(
        Rect::new(
            left + corner_size,
            top - shadow_size,
            right - left - corner_size * 2,
            shadow_size,
        ),
        &linear((0.0, t), (0.0, t - s)),
    );
    pixels.fill_rect_gradient(
        Rect::new(
            left + corner_size,
            bottom,
            right - left - corner_size * 2,
            shadow_size,
        ),
        &linear((0.0, b), (0.0, b + s)),
    );

    let c = corner_size as f32;
    let o = corner_shadow_offset;
    let corner = |centre: (f32, f32), edge: (f32, f32), area: Rect| {
        let gradient = Gradient::new(GradientKind::Radial, centre, shadow, edge, transparent)
            .with_stop(corner_ratio, shadow);
        pixels.fill_rect_gradient(area, &gradient);
    };
    corner(
        (l + c, t + c),
        (l + o, t + o),
        Rect::new(
            left - shadow_size,
            top - shadow_size,
            corner_and_shadow,
            corner_and_shadow,
        ),
    );
    corner(
        (r - c, t + c),
        (r - o, t + o),
        Rect::new(
            right - corner_size,
            top - shadow_size,
            corner_and_shadow,
            corner_and_shadow,
        ),
    );
    corner(
        (l + c, b - c),
        (l + o, b - o),
        Rect::new(
            left - shadow_size,
            bottom - corner_size,
            corner_and_shadow,
            corner_and_shadow,
        ),
    );
    corner(
        (r - c, b - c),
        (r - o, b - o),
        Rect::new(
            right - corner_size,
            bottom - corner_size,
            corner_and_shadow,
            corner_and_shadow,
        ),
    );
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

/// `SynthSection::drawTempoDivider`: a 1-pixel line, half the sync
/// selector's height, at its left edge.
pub fn draw_tempo_divider(pixels: &PixelPainter, scope: &Scope, sync: Rect) {
    const LINE_RATIO: f32 = 0.5;
    let line_height = (sync.height as f32 * LINE_RATIO) as i32;
    let y = sync.y + (sync.height - line_height) / 2;
    pixels.fill_rect(
        Rect::new(sync.x, y, 1, line_height),
        scope.color(ColorId::LightenScreen),
    );
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
