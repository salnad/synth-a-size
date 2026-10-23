// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The pitch and mod wheels' drawing: a rounded well with ridges that roll
//! as the value changes.
//!
//! Ports `ControlWheel::paint` and `paintLine`
//! (`vital/src/interface/editor_sections/bend_section.cpp`).

use std::f32::consts::PI;

use crate::colour::Colour;
use crate::paint::{PixelPainter, RectF};

/// `ControlWheel`'s proportions of its width.
const BUFFER_WIDTH_RATIO: f32 = 0.05;
const WHEEL_ROUND_AMOUNT_RATIO: f32 = 0.25;
const CONTAINER_ROUND_AMOUNT_RATIO: f32 = 0.15;

/// The colours a wheel paints with.
#[derive(Clone, Copy, Debug)]
pub struct WheelColours {
    /// `kWidgetSecondary1`: the well, and the ridges' sides.
    pub background: Colour,
    /// `kWidgetAccent1`: the ridges.
    pub line: Colour,
    /// `kWidgetPrimary1`: the ridge marking the centre value.
    pub marker: Colour,
    /// `kBody`: outside the well.
    pub body: Colour,
    /// `kShadow`: the well's rim.
    pub shadow: Colour,
}

/// `ControlWheel::paint` in `bounds` (pixels), at `proportion` of the range.
pub fn paint(pixels: &PixelPainter, bounds: RectF, proportion: f32, colours: WheelColours) {
    const NUM_LINES: i32 = 6;
    const ROTATE_PERCENT: f32 = 0.8;
    let round_amount = bounds.width * CONTAINER_ROUND_AMOUNT_RATIO;
    let buffer = bounds.width * BUFFER_WIDTH_RATIO;
    let well = RectF::new(
        bounds.x + buffer,
        bounds.y + buffer,
        bounds.width - 2.0 * buffer,
        bounds.height - 2.0 * buffer,
    );
    pixels.fill_rounded_rect(well, round_amount, colours.background);

    let mut t = 1.0 - proportion;
    t = (t - 0.5) * ROTATE_PERCENT + 0.5;
    let ratio_spacing = 1.0 / NUM_LINES as f32;
    let mut i = NUM_LINES;
    while t + ratio_spacing * i as f32 >= 0.5 {
        let line = if i == 0 { colours.marker } else { colours.line };
        paint_line(
            pixels,
            bounds,
            t + ratio_spacing * i as f32,
            line,
            colours.background,
        );
        i -= 1;
    }
    let mut i = -NUM_LINES;
    while t + ratio_spacing * (i as f32) < 0.5 {
        let line = if i == 0 { colours.marker } else { colours.line };
        paint_line(
            pixels,
            bounds,
            t + ratio_spacing * i as f32,
            line,
            colours.background,
        );
        i += 1;
    }

    // Body outside the well, then the shadow rim between the bounds and the
    // well (both even-odd paths upstream).
    pixels.fill_rounded_ring(bounds, 0.0, well, round_amount, colours.body);
    pixels.fill_rounded_ring(bounds, round_amount, well, round_amount, colours.shadow);
}

/// `ControlWheel::paintLine`: one ridge at `y_percent` around the wheel.
fn paint_line(
    pixels: &PixelPainter,
    bounds: RectF,
    y_percent: f32,
    line_color: Colour,
    fill_color: Colour,
) {
    const LINE_WIDTH_RATIO: f32 = 0.165;
    const FADE_RATIO: f32 = 0.12;
    let buffer = bounds.width * BUFFER_WIDTH_RATIO;
    let width = bounds.width - 2.0 * buffer;
    let height = bounds.height - 4.0 * buffer;
    let end_radians = PI / 2.0;
    let radians = 2.0 * end_radians * y_percent - end_radians;
    if radians > PI * 0.6 || radians < -PI * 0.6 {
        return;
    }
    let sin_value = radians.sin();
    let cos_value = radians.cos();
    let mut y = 2.0 * buffer + height * 0.5 + sin_value * height * 0.45;
    let round_amount = sin_value.abs() * bounds.width * WHEEL_ROUND_AMOUNT_RATIO;
    let line_height = cos_value * height * LINE_WIDTH_RATIO;
    let distance_from_edge = (y - 2.0 * buffer).min(height + 2.0 * buffer - y);
    let alpha = (distance_from_edge / (height * FADE_RATIO)).clamp(0.0, 1.0);

    let offset = (line_height + round_amount) / 2.0;
    let ridge = RectF::new(
        bounds.x + buffer,
        bounds.y + y - offset,
        width,
        (line_height + round_amount).max(0.0),
    );
    pixels.fill_rounded_rect(
        ridge,
        round_amount,
        fill_color.interpolated_with(line_color, alpha),
    );

    if sin_value > 0.0 {
        y -= round_amount;
    } else {
        y += line_height;
    }
    let side = RectF::new(
        bounds.x + buffer,
        bounds.y + y - offset,
        width,
        2.0 * round_amount,
    );
    pixels.fill_rounded_rect(side, round_amount, fill_color);
}
