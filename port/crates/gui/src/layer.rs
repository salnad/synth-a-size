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

enum Image {
    Text {
        text: String,
        rect: RectF,
        face: Face,
        point_height: f32,
        colour: Colour,
    },
    Polygon {
        points: Vec<(f32, f32)>,
        colour: Colour,
    },
    /// A component painted into its image (`paintToImage`).
    Custom(Box<dyn FnOnce(&PixelPainter)>),
}

#[derive(Default)]
pub struct Layer {
    quads: QuadBatch,
    images: Vec<Image>,
}

impl Layer {
    pub fn new() -> Layer {
        Layer::default()
    }

    pub fn quad(&mut self, quad: Quad) {
        self.quads.push(quad);
    }

    /// Text centred in `rect` (pixels), drawn over the quads.
    pub fn text_centred(
        &mut self,
        text: &str,
        rect: impl Into<RectF>,
        face: Face,
        point_height: f32,
        colour: Colour,
    ) {
        self.images.push(Image::Text {
            text: text.to_string(),
            rect: rect.into(),
            face,
            point_height,
            colour,
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
        for image in self.images {
            match image {
                Image::Text {
                    text,
                    rect,
                    face,
                    point_height,
                    colour,
                } => pixels.text_centred(&text, rect, face, point_height, colour),
                Image::Polygon { points, colour } => pixels.fill_polygon(&points, colour),
                Image::Custom(paint) => paint(pixels),
            }
        }
    }
}
