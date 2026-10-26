// SPDX-License-Identifier: GPL-3.0-or-later

//! A section's OpenGL-rendered parts, painted after its background.
//!
//! Upstream renders a section's quads, line renderers and images after its
//! background image, in the order they were added. A [`Layer`] collects the
//! quads, then paints the images (text, filled paths) over them.

use crate::colour::Colour;
use crate::fonts::Face;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadBatch};
use crate::text::{Justify, Overflow};

enum Image {
    Text {
        text: String,
        rect: RectF,
        face: Face,
        point_height: f32,
        colour: Colour,
        overflow: Overflow,
    },
    Polygon {
        points: Vec<(f32, f32)>,
        colour: Colour,
    },
    /// A component painted into its image (`paintToImage`).
    Custom(Box<dyn FnOnce(&PixelPainter)>),
}

pub struct Layer {
    quads: QuadBatch,
    /// Quads blended additively (`BarRenderer`'s default), drawn after the
    /// others.
    additive: QuadBatch,
    images: Vec<Image>,
}

impl Default for Layer {
    fn default() -> Layer {
        Layer {
            quads: QuadBatch::new(),
            additive: QuadBatch::additive(),
            images: Vec::new(),
        }
    }
}

impl Layer {
    pub fn new() -> Layer {
        Layer::default()
    }

    pub fn quad(&mut self, quad: Quad) {
        self.quads.push(quad);
    }

    /// A quad blended additively.
    pub fn additive_quad(&mut self, quad: Quad) {
        self.additive.push(quad);
    }

    /// Text centred in `rect` (pixels), drawn over the quads and curtailed
    /// like `Graphics::drawText(..., false)`.
    pub fn text_centred(
        &mut self,
        text: &str,
        rect: impl Into<RectF>,
        face: Face,
        point_height: f32,
        colour: Colour,
    ) {
        self.text(text, rect, face, point_height, colour, Overflow::Curtail);
    }

    /// A `PlainTextComponent`'s centred text in `rect` (pixels), drawn over
    /// the quads and fitted like `Graphics::drawFittedText`.
    pub fn fitted_text(
        &mut self,
        text: &str,
        rect: impl Into<RectF>,
        face: Face,
        point_height: f32,
        colour: Colour,
    ) {
        self.text(text, rect, face, point_height, colour, Overflow::Fit);
    }

    fn text(
        &mut self,
        text: &str,
        rect: impl Into<RectF>,
        face: Face,
        point_height: f32,
        colour: Colour,
        overflow: Overflow,
    ) {
        self.images.push(Image::Text {
            text: text.to_string(),
            rect: rect.into(),
            face,
            point_height,
            colour,
            overflow,
        });
    }

    /// A filled convex polygon (pixels), drawn over the quads.
    pub fn polygon(&mut self, points: Vec<(f32, f32)>, colour: Colour) {
        self.images.push(Image::Polygon { points, colour });
    }

    /// Painting done by a component that paints to an image.
    pub fn custom(&mut self, paint: impl FnOnce(&PixelPainter) + 'static) {
        self.images.push(Image::Custom(Box::new(paint)));
    }

    /// Paints the quads, clipped to `clip` (pixels), then the images.
    pub fn paint(self, pixels: &PixelPainter, clip: RectF) {
        self.quads.paint(pixels, clip);
        self.additive.paint(pixels, clip);
        for image in self.images {
            match image {
                Image::Text {
                    text,
                    rect,
                    face,
                    point_height,
                    colour,
                    overflow,
                } => pixels.text_line(
                    &text,
                    rect,
                    (face, point_height, colour),
                    Justify::Centred,
                    overflow,
                ),
                Image::Polygon { points, colour } => pixels.fill_polygon(&points, colour),
                Image::Custom(paint) => paint(pixels),
            }
        }
    }
}
