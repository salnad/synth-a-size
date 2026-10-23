// SPDX-License-Identifier: GPL-3.0-or-later

//! A section's OpenGL-rendered parts, painted after its background.
//!
//! Upstream renders a section's quads and text images after its background
//! image, with text over the quads it labels. A [`Layer`] collects them while
//! the section's controls run and paints them in that order.

use crate::colour::Colour;
use crate::fonts::Face;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadBatch};

struct Text {
    text: String,
    rect: RectF,
    face: Face,
    point_height: f32,
    colour: Colour,
}

#[derive(Default)]
pub struct Layer {
    quads: QuadBatch,
    texts: Vec<Text>,
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
        self.texts.push(Text {
            text: text.to_string(),
            rect: rect.into(),
            face,
            point_height,
            colour,
        });
    }

    /// Paints the quads, clipped to `clip` (pixels), then the text.
    pub fn paint(self, pixels: &PixelPainter, clip: RectF) {
        self.quads.paint(pixels, clip);
        for text in self.texts {
            pixels.text_centred(
                &text.text,
                text.rect,
                text.face,
                text.point_height,
                text.colour,
            );
        }
    }
}
