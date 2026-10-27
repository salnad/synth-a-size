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
    /// `power_scale_` and `square_scale_`: how [`Self::set_scaled_y`] maps
    /// a value to a bar's height.
    pub power_scale: bool,
    pub square_scale: bool,
}

/// `BarRenderer::kScaleConstant`.
const SCALE_CONSTANT: f32 = 5.0;

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
            power_scale: false,
            square_scale: false,
        }
    }

    /// `setScaledY`: bar `index` at `value`, raised by its index under the
    /// power scale and square-rooted under the square scale.
    pub fn set_scaled_y(&mut self, index: usize, value: f32) {
        let mut value = value;
        if self.power_scale {
            value *= index.max(1) as f32 / SCALE_CONSTANT;
        }
        if self.square_scale {
            value = value.sqrt();
        }
        self.y[index] = 2.0 * value - 1.0;
    }

    /// `yAt`: bar `index`'s top.
    pub fn y_at(&self, index: usize) -> f32 {
        self.y[index]
    }

    /// `scaledYAt`: the value [`Self::set_scaled_y`] would give bar
    /// `index`'s top.
    pub fn scaled_y_at(&self, index: usize) -> f32 {
        let mut value = self.y[index] * 0.5 + 0.5;
        if self.square_scale {
            value *= value;
        }
        if self.power_scale {
            value /= index.max(1) as f32 / SCALE_CONSTANT;
        }
        value
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

    /// The bars' quads for a component at `bounds` (pixels).
    ///
    /// Upstream's layout already includes the display scale and it never
    /// sets `BarRenderer::display_scale_` (so it stays 1): the vertex
    /// shaders' dimensions are the component's size in pixels, and the
    /// four-pixel minimum width is four pixels at any scale.
    pub fn quads(&self, bounds: RectF) -> Vec<Quad> {
        let (width, height) = (bounds.width, bounds.height);
        if width <= 0.0 || height <= 0.0 {
            return Vec::new();
        }
        let dimensions = if self.vertical { width } else { height };
        // Upstream's minimum is four pixels of the width whichever way the
        // bars run (`4.0f / getWidth()`).
        let min_width = 4.0 / width;
        let width_percent =
            min_width.max(self.bar_width * self.scale * 2.0 / self.num_points as f32);
        let mut quads = Vec::with_capacity(self.total_points());
        for i in 0..self.total_points() {
            // setBarSizes: half the bar's length (times a display scale of 1).
            let length = (self.y[i] - self.bottom[i]).abs() * 0.5;
            let along = self.scale * (self.x[i] + 1.0) - 1.0 + self.offset;
            let (rect, size) = if self.vertical {
                let left = bounds.x + (along + 1.0) / 2.0 * width;
                let top = bounds.y + (1.0 - self.y[i].max(self.bottom[i])) / 2.0 * height;
                let bottom = bounds.y + (1.0 - self.y[i].min(self.bottom[i])) / 2.0 * height;
                (
                    RectF::new(left, top, width_percent / 2.0 * width, bottom - top),
                    [width_percent * dimensions / 2.0, length * height / 2.0],
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
                    [length * width / 2.0, width_percent * dimensions / 2.0],
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
        for quad in self.quads(bounds) {
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
        let quads = bars.quads(RectF::new(10.0, 20.0, 100.0, 40.0));
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

    /// At display scale 2 the component is twice as many pixels and the
    /// bars scale with it, as upstream's (its `display_scale_` stays 1):
    /// twice as wide, with twice the shader size, and the minimum width is
    /// still four pixels' worth of the component, not eight.
    #[test]
    fn bars_scale_with_the_component_at_2x() {
        let mut bars = BarRenderer::new(4, true);
        bars.bar_width = 0.3;
        bars.set_x(1, -0.5);
        bars.set_y(1, 0.5);
        bars.set_bottom(1, -0.5);
        let one = bars.quads(RectF::new(10.0, 20.0, 100.0, 40.0));
        let two = bars.quads(RectF::new(20.0, 40.0, 200.0, 80.0));
        let bar = |quads: &[Quad]| {
            *quads
                .iter()
                .find(|q| q.rect.height > 0.0 && q.rect.y > 0.0 && q.rect.height < q.rect.y)
                .unwrap()
        };
        let (one, two) = (bar(&one), bar(&two));
        assert!((two.rect.width - 2.0 * one.rect.width).abs() < 1e-4);
        assert!((two.shader_values[0] - 15.0).abs() < 1e-4);
        assert!((two.shader_values[1] - 20.0).abs() < 1e-4);

        let mut thin = BarRenderer::new(100, true);
        thin.bar_width = 0.3;
        thin.set_y(0, 1.0);
        for (width, expected) in [(100.0, 2.0), (200.0, 2.0)] {
            let quads = thin.quads(RectF::new(0.0, 0.0, width, 40.0));
            // min_width = 4 / w: four device units of 2/w, half of which
            // is the quad's width.
            assert!((quads[0].rect.width - expected).abs() < 1e-4);
            assert!((quads[0].shader_values[0] - 2.0).abs() < 1e-4);
        }
    }

    /// Horizontal bars' minimum thickness is 4 / width too, as upstream's,
    /// though their thickness is measured against the height: a thin bar in
    /// a 200 × 40 component is (4 / 200) / 2 of the height, 0.4 pixels, with
    /// a shader size of 0.4.
    #[test]
    fn horizontal_bars_take_the_minimum_from_the_width() {
        let mut thin = BarRenderer::new(100, false);
        thin.bar_width = 0.3;
        thin.set_y(0, 1.0);
        let quads = thin.quads(RectF::new(0.0, 0.0, 200.0, 40.0));
        let bar = quads[0];
        assert!((bar.rect.height - 0.4).abs() < 1e-4, "{}", bar.rect.height);
        assert!((bar.shader_values[1] - 0.4).abs() < 1e-4);
    }
}
