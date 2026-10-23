// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! JUCE `Graphics` calls the interface makes, drawn with egui in pixel
//! coordinates.
//!
//! Upstream paints its background with JUCE's software renderer and its
//! widgets with OpenGL shaders; both work in physical pixels. This painter
//! takes the same pixel coordinates and converts them to egui points. The
//! shader-drawn widgets get their own wgpu primitives in a later milestone.

use egui::{Align2, Color32, CornerRadius, Pos2, Stroke, StrokeKind, Vec2};

use crate::colour::Colour;
use crate::fonts::Face;
use crate::layout::Rect;

/// A float rectangle in pixels (`juce::Rectangle<float>`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectF {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> RectF {
        RectF {
            x,
            y,
            width,
            height,
        }
    }

    /// `Rectangle::reduced`.
    pub fn reduced(&self, amount: f32) -> RectF {
        RectF::new(
            self.x + amount,
            self.y + amount,
            (self.width - 2.0 * amount).max(0.0),
            (self.height - 2.0 * amount).max(0.0),
        )
    }
}

impl From<Rect> for RectF {
    fn from(rect: Rect) -> RectF {
        RectF::new(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
        )
    }
}

/// Paints in pixels onto an egui painter whose origin is the editor's top
/// left corner.
pub struct PixelPainter<'a> {
    painter: &'a egui::Painter,
    origin: Pos2,
    pixels_per_point: f32,
}

impl<'a> PixelPainter<'a> {
    pub fn new(
        painter: &'a egui::Painter,
        origin: Pos2,
        pixels_per_point: f32,
    ) -> PixelPainter<'a> {
        PixelPainter {
            painter,
            origin,
            pixels_per_point,
        }
    }

    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    /// Pixels to points.
    pub fn points(&self, pixels: f32) -> f32 {
        pixels / self.pixels_per_point
    }

    pub fn pos(&self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(self.points(x), self.points(y))
    }

    pub fn rect(&self, rect: RectF) -> egui::Rect {
        egui::Rect::from_min_size(
            self.pos(rect.x, rect.y),
            Vec2::new(self.points(rect.width), self.points(rect.height)),
        )
    }

    fn corner(&self, radius: f32) -> CornerRadius {
        CornerRadius::same(self.points(radius).round().clamp(0.0, 255.0) as u8)
    }

    /// `Graphics::fillAll` / `fillRect`.
    pub fn fill_rect(&self, rect: impl Into<RectF>, colour: Colour) {
        self.painter
            .rect_filled(self.rect(rect.into()), CornerRadius::ZERO, colour.to_egui());
    }

    /// `Graphics::fillRoundedRectangle`.
    pub fn fill_rounded_rect(&self, rect: impl Into<RectF>, radius: f32, colour: Colour) {
        self.painter.rect_filled(
            self.rect(rect.into()),
            self.corner(radius),
            colour.to_egui(),
        );
    }

    /// `Graphics::drawRoundedRectangle(rect, radius, thickness)`, with the
    /// stroke centred on `rect` as JUCE strokes paths.
    pub fn draw_rounded_rect(
        &self,
        rect: impl Into<RectF>,
        radius: f32,
        thickness: f32,
        colour: Colour,
    ) {
        let stroke = Stroke::new(self.points(thickness), colour.to_egui());
        self.painter.rect_stroke(
            self.rect(rect.into()),
            self.corner(radius),
            stroke,
            StrokeKind::Middle,
        );
    }

    /// A polyline `points` (pixels), stroked `thickness` wide.
    pub fn polyline(&self, points: &[(f32, f32)], thickness: f32, colour: Colour) {
        let points: Vec<Pos2> = points.iter().map(|&(x, y)| self.pos(x, y)).collect();
        self.painter.line(
            points,
            Stroke::new(self.points(thickness), colour.to_egui()),
        );
    }

    /// `Graphics::drawText(text, rect, Justification::centred)` with `face`
    /// at JUCE point height `point_height` (pixels).
    pub fn text_centred(
        &self,
        text: &str,
        rect: impl Into<RectF>,
        face: Face,
        point_height: f32,
        colour: Colour,
    ) {
        let rect = self.rect(rect.into());
        self.painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            face.at(self.points(point_height)),
            colour.to_egui(),
        );
    }

    pub fn egui_color(colour: Colour) -> Color32 {
        colour.to_egui()
    }
}
