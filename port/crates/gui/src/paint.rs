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
    area: egui::Rect,
    pixels_per_point: f32,
}

impl<'a> PixelPainter<'a> {
    /// Paints into `area` (points) of `painter`, whose top left corner is
    /// pixel (0, 0).
    pub fn new(
        painter: &'a egui::Painter,
        area: egui::Rect,
        pixels_per_point: f32,
    ) -> PixelPainter<'a> {
        PixelPainter {
            painter,
            origin: area.min,
            area,
            pixels_per_point,
        }
    }

    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    pub fn painter(&self) -> &egui::Painter {
        self.painter
    }

    /// The editor's area, in points.
    pub fn area(&self) -> egui::Rect {
        self.area
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

    /// `Graphics::fillRoundedRectangle` under `Graphics::reduceClipRegion(clip)`.
    pub fn fill_rounded_rect_clipped(
        &self,
        rect: impl Into<RectF>,
        radius: f32,
        colour: Colour,
        clip: impl Into<RectF>,
    ) {
        self.painter
            .with_clip_rect(self.rect(clip.into()))
            .rect_filled(
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

    /// The outline of a rounded rectangle (pixels), clockwise from the top
    /// left corner's start, `segments` points per corner. A zero radius
    /// repeats each corner point, so two outlines always pair up.
    fn rounded_outline(rect: RectF, radius: f32, segments: usize) -> Vec<(f32, f32)> {
        use std::f32::consts::FRAC_PI_2;
        let r = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
        let centres = [
            (rect.x + r, rect.y + r, 2.0 * FRAC_PI_2),
            (rect.x + rect.width - r, rect.y + r, 3.0 * FRAC_PI_2),
            (rect.x + rect.width - r, rect.y + rect.height - r, 0.0),
            (rect.x + r, rect.y + rect.height - r, FRAC_PI_2),
        ];
        let mut points = Vec::with_capacity(4 * (segments + 1));
        for (cx, cy, start) in centres {
            for i in 0..=segments {
                let angle = start + FRAC_PI_2 * i as f32 / segments as f32;
                points.push((cx + r * angle.cos(), cy + r * angle.sin()));
            }
        }
        points
    }

    /// Fills between two rounded rectangles (pixels), `inner` inside
    /// `outer`: JUCE's even-odd fill of one rounded rectangle path with
    /// another inside it.
    pub fn fill_rounded_ring(
        &self,
        outer: RectF,
        outer_radius: f32,
        inner: RectF,
        inner_radius: f32,
        colour: Colour,
    ) {
        const SEGMENTS: usize = 12;
        let outer = Self::rounded_outline(outer, outer_radius, SEGMENTS);
        let inner = Self::rounded_outline(inner, inner_radius, SEGMENTS);
        let mut mesh = egui::Mesh::default();
        let colour = colour.to_egui();
        for (&(ox, oy), &(ix, iy)) in outer.iter().zip(&inner) {
            mesh.colored_vertex(self.pos(ox, oy), colour);
            mesh.colored_vertex(self.pos(ix, iy), colour);
        }
        let count = outer.len() as u32;
        for i in 0..count {
            let next = (i + 1) % count;
            let (o0, i0, o1, i1) = (2 * i, 2 * i + 1, 2 * next, 2 * next + 1);
            mesh.add_triangle(o0, i0, o1);
            mesh.add_triangle(o1, i0, i1);
        }
        self.painter.add(egui::Shape::mesh(mesh));
    }

    /// `Graphics::fillPath` for a convex polygon (pixels).
    pub fn fill_polygon(&self, points: &[(f32, f32)], colour: Colour) {
        let points: Vec<Pos2> = points.iter().map(|&(x, y)| self.pos(x, y)).collect();
        self.painter.add(egui::Shape::convex_polygon(
            points,
            colour.to_egui(),
            Stroke::NONE,
        ));
    }

    /// `Graphics::strokePath` with rounded end caps (`PathStrokeType::rounded`).
    pub fn stroke_path(&self, points: &[(f32, f32)], thickness: f32, colour: Colour) {
        if points.is_empty() {
            return;
        }
        self.polyline(points, thickness, colour);
        for &(x, y) in [points[0], points[points.len() - 1]].iter() {
            self.painter.circle_filled(
                self.pos(x, y),
                self.points(thickness / 2.0),
                colour.to_egui(),
            );
        }
    }

    /// `Path::addCentredArc` from `from` to `to` (radians clockwise from
    /// twelve o'clock), stroked with rounded caps.
    pub fn stroke_arc(
        &self,
        center: (f32, f32),
        radius: f32,
        from: f32,
        to: f32,
        thickness: f32,
        colour: Colour,
    ) {
        let segments = ((to - from).abs() * radius / 2.0).ceil().max(8.0) as usize;
        let points: Vec<(f32, f32)> = (0..=segments)
            .map(|i| {
                let angle = from + (to - from) * i as f32 / segments as f32;
                (
                    center.0 + radius * angle.sin(),
                    center.1 - radius * angle.cos(),
                )
            })
            .collect();
        self.stroke_path(&points, thickness, colour);
    }

    /// `Graphics::fillEllipse` for a circle.
    pub fn fill_ellipse(&self, rect: RectF, colour: Colour) {
        let rect = self.rect(rect);
        self.painter
            .circle_filled(rect.center(), rect.width() / 2.0, colour.to_egui());
    }

    /// `Graphics::drawEllipse` for a circle.
    pub fn draw_ellipse(&self, rect: RectF, thickness: f32, colour: Colour) {
        let rect = self.rect(rect);
        self.painter.circle_stroke(
            rect.center(),
            rect.width() / 2.0,
            Stroke::new(self.points(thickness), colour.to_egui()),
        );
    }

    /// The width of `text` in `face` at `point_height` (pixels), as laid
    /// out by egui.
    pub fn text_width(&self, text: &str, face: Face, point_height: f32) -> f32 {
        let galley = self.painter.layout_no_wrap(
            text.to_string(),
            face.at(self.points(point_height)),
            Color32::WHITE,
        );
        galley.size().x * self.pixels_per_point
    }

    /// `Graphics::drawText(text, rect, Justification::centredLeft)`.
    pub fn text_left(
        &self,
        text: &str,
        rect: impl Into<RectF>,
        face: Face,
        point_height: f32,
        colour: Colour,
    ) {
        let rect = self.rect(rect.into());
        self.painter.text(
            egui::pos2(rect.min.x, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            face.at(self.points(point_height)),
            colour.to_egui(),
        );
    }

    pub fn egui_color(colour: Colour) -> Color32 {
        colour.to_egui()
    }
}
