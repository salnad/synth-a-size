// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The header's oscilloscope: the last window of output, both channels,
//! drawn as filled lines.
//!
//! Ports `Oscilloscope` (`vital/src/interface/editor_components/oscilloscope.cpp`),
//! reading the engine's oscilloscope memory from the telemetry snapshot.

use sas_engine::telemetry::OSCILLOSCOPE_MEMORY_RESOLUTION;

use crate::colour::Colour;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadBatch};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `Oscilloscope::kResolution`.
pub const RESOLUTION: usize = 512;

/// `OpenGlComponent::paintBackground`: the widget background.
pub fn paint_background(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    pixels.fill_rect(bounds, scope.color(ColorId::WidgetBackground));
}

/// `Oscilloscope::render`: `memory` is the telemetry's oscilloscope frames
/// (silence when there is no engine); `body` is the colour of the rounded
/// corners (the header sets it to the background).
pub fn paint(
    pixels: &PixelPainter,
    scope: &Scope,
    bounds: Rect,
    memory: Option<&[[f32; 2]]>,
    body: Colour,
) {
    let mut line = LineRenderer::new(RESOLUTION, false);
    line.fill = true;
    line.line_width = scope.value(ValueId::WidgetLineWidth);
    line.fill_center = scope.value(ValueId::WidgetFillCenter);
    line.color = scope.color(ColorId::WidgetPrimary1);
    let fill_color = scope.color(ColorId::WidgetPrimary2);
    let fill_fade = scope.value(ValueId::WidgetFillFade);
    line.set_fill_colors(
        fill_color.with_multiplied_alpha(1.0 - fill_fade),
        fill_color,
    );

    let width = bounds.width as f32;
    let y_adjust = bounds.height as f32 / 2.0;
    for channel in 0..2 {
        // Oscilloscope::drawWaveform.
        for i in 0..RESOLUTION {
            let t = i as f32 / (RESOLUTION as f32 - 1.0);
            let memory_spot = (i * OSCILLOSCOPE_MEMORY_RESOLUTION) as f32 / RESOLUTION as f32;
            let memory_index = memory_spot as usize;
            let remainder = memory_spot - memory_index as f32;
            let (from, to) = match memory {
                Some(memory) => (
                    memory[memory_index][channel],
                    memory[memory_index + 1][channel],
                ),
                None => (0.0, 0.0),
            };
            line.set_x_at(i, t * width);
            line.set_y_at(i, (1.0 - (from + (to - from) * remainder)) * y_adjust);
        }
        line.paint(pixels, RectF::from(bounds), true);
    }

    let mut corners = QuadBatch::new();
    for corner in Quad::corners(
        bounds.into(),
        scope.value(ValueId::WidgetRoundedCorner),
        body,
    ) {
        corners.push(corner);
    }
    corners.paint(pixels, RectF::from(bounds));
}
