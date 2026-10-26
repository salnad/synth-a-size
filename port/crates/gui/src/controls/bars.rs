// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `BarRenderer`: a row of bars in a component, positioned in its device
//! coordinates (-1 to 1, y up). Ports
//! `vital/src/interface/editor_components/bar_renderer.cpp` and its vertex
//! shaders (`kBarVerticalVertex`, `kBarHorizontalVertex`), drawn with
//! [`QuadKind::Bar`].

use crate::colour::Colour;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadBatch, QuadKind};

/// One `BarRenderer`.
#[derive(Clone, Debug)]
pub struct BarRenderer {
    /// Each bar's position along the row (`setX`): its left edge for
    /// vertical bars.
    x: Vec<f32>,
    /// Each bar's top and bottom (`setY`, `setBottom`).
    y: Vec<f32>,
    bottom: Vec<f32>,
    vertical: bool,
    pub color: Colour,
    pub scale: f32,
    pub offset: f32,
    /// `bar_width_`: the bars' width as a fraction of their spacing.
    pub bar_width: f32,
    /// `num_points_`: the count the width is spread over.
    pub num_points: usize,
    pub additive: bool,
}

impl BarRenderer {
    /// `BarRenderer(num_points, vertical)`: bars spread evenly from -1, all
    /// at the bottom.
    pub fn new(num_points: usize, vertical: bool) -> BarRenderer {
        BarRenderer {
            x: (0..num_points)
                .map(|i| 2.0 * (i as f32 / num_points as f32) - 1.0)
                .collect(),
            y: vec![-1.0; num_points],
            bottom: vec![-1.0; num_points],
            vertical,
            color: Colour::TRANSPARENT_BLACK,
            scale: 1.0,
            offset: 0.0,
            bar_width: 1.0,
            num_points,
            additive: true,
        }
    }

    pub fn total_points(&self) -> usize {
        self.x.len()
    }

    pub fn set_x(&mut self, index: usize, x: f32) {
        self.x[index] = x;
    }

    pub fn set_y(&mut self, index: usize, y: f32) {
        self.y[index] = y;
    }

    pub fn set_bottom(&mut self, index: usize, bottom: f32) {
        self.bottom[index] = bottom;
    }

    /// The bars' quads for a component at `bounds` (pixels), at
    /// `display_scale` (`BarRenderer::display_scale_`).
    pub fn quads(&self, bounds: RectF, display_scale: f32) -> Vec<Quad> {
        let (width, height) = (bounds.width, bounds.height);
        if width <= 0.0 || height <= 0.0 {
            return Vec::new();
        }
        // The vertex shaders' uniforms, with the dimensions in logical
        // pixels as upstream's components measure them.
        let dimensions = if self.vertical { width } else { height } / display_scale;
        let min_width = 4.0 / dimensions;
        let width_percent =
            min_width.max(self.bar_width * self.scale * 2.0 / self.num_points as f32);
        let mut quads = Vec::with_capacity(self.total_points());
        for i in 0..self.total_points() {
            // setBarSizes: half the bar's length, times the display scale.
            let length = (self.y[i] - self.bottom[i]).abs() * 0.5 * display_scale;
            let along = self.scale * (self.x[i] + 1.0) - 1.0 + self.offset;
            let (rect, size) = if self.vertical {
                let left = bounds.x + (along + 1.0) / 2.0 * width;
                let top = bounds.y + (1.0 - self.y[i].max(self.bottom[i])) / 2.0 * height;
                let bottom = bounds.y + (1.0 - self.y[i].min(self.bottom[i])) / 2.0 * height;
                (
                    RectF::new(left, top, width_percent / 2.0 * width, bottom - top),
                    [
                        width_percent * dimensions / 2.0,
                        length * height / display_scale / 2.0,
                    ],
                )
            } else {
                // kBarHorizontalVertex: the bar runs along x from its
                // position to its bottom value, across y below `offset`.
                let from = (self.scale * (self.y[i] + 1.0) - 1.0)
                    .min(self.scale * (self.bottom[i] + 1.0) - 1.0);
                let to = (self.scale * (self.y[i] + 1.0) - 1.0)
                    .max(self.scale * (self.bottom[i] + 1.0) - 1.0);
                let top = bounds.y + (1.0 - (self.x[i] + self.offset)) / 2.0 * height;
                (
                    RectF::new(
                        bounds.x + (from + 1.0) / 2.0 * width,
                        top,
                        (to - from) / 2.0 * width,
                        width_percent / 2.0 * height,
                    ),
                    [
                        length * width / display_scale / 2.0,
                        width_percent * dimensions / 2.0,
                    ],
                )
            };
            let finite = [rect.x, rect.y, rect.width, rect.height]
                .iter()
                .all(|value| value.is_finite());
            if !finite || rect.width <= 0.0 || rect.height <= 0.0 {
                continue;
            }
            let mut quad = Quad::new(QuadKind::Bar, rect, self.color);
            quad.shader_values[0] = size[0];
            quad.shader_values[1] = size[1];
            quads.push(quad);
        }
        quads
    }

    /// Draws the bars, clipped to `bounds`.
    pub fn paint(&self, pixels: &PixelPainter, bounds: RectF) {
        let mut batch = if self.additive {
            QuadBatch::additive()
        } else {
            QuadBatch::new()
        };
        for quad in self.quads(bounds, pixels.pixels_per_point()) {
            batch.push(quad);
        }
        if !batch.is_empty() {
            batch.paint(pixels, bounds);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_bars_follow_the_vertex_shader() {
        let mut bars = BarRenderer::new(4, true);
        bars.bar_width = 0.3;
        bars.set_x(1, -0.5);
        bars.set_y(1, 0.5);
        bars.set_bottom(1, -0.5);
        let quads = bars.quads(RectF::new(10.0, 20.0, 100.0, 40.0), 1.0);
        // Bar 1: x from 25% across; 0.15 of the spacing wide (at least four
        // pixels); from y = 0.5 down to -0.5, the middle half.
        let bar = quads.iter().find(|q| q.rect.x == 10.0 + 25.0).unwrap();
        assert_eq!(bar.rect.y, 30.0);
        assert_eq!(bar.rect.height, 20.0);
        assert!((bar.rect.width - 7.5).abs() < 1e-5);
        // size: the thickness across, half the length along.
        assert!((bar.shader_values[0] - 7.5).abs() < 1e-5);
        assert!((bar.shader_values[1] - 10.0).abs() < 1e-5);
    }
}
