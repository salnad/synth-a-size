// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `OpenGlScrollBar` (`vital/src/interface/editor_components/
//! open_gl_multi_quad.h`, `OpenGlScrollQuad`) with JUCE's `ScrollBar`
//! behaviour: a rounded bar over the visible part of a taller column that
//! widens while hovered, wheel scrolling over the column, dragging the
//! thumb, and paging towards a press beside it.

use egui::{Id, Sense, Ui};

use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::scope::Scope;
use crate::skin::ColorId;

/// `OpenGlScrollQuad::kHoverChange`.
const HOVER_CHANGE: f32 = 0.2;
/// `ScrollBar::mouseDown`'s first timer, then `timerCallback`'s.
const FIRST_REPEAT: f64 = 0.4;
const REPEAT: f64 = 0.04;

/// A column's scroll position and its bar's state.
#[derive(Clone, Copy, Debug)]
pub struct ScrollBar {
    /// The view position, in pixels from the column's top.
    pub position: f32,
    /// `OpenGlScrollQuad::hover_amount_`.
    hover: f32,
    /// Whether the bar is pressed (`OpenGlScrollBar::mouseDown`).
    pressed: bool,
    /// Whether that press began on the thumb, so a drag moves it
    /// (`ScrollBar::isDraggingThumb`).
    thumb_drag: bool,
    /// When a press held beside the thumb pages again, in egui's time.
    page_at: Option<f64>,
}

impl Default for ScrollBar {
    fn default() -> ScrollBar {
        ScrollBar {
            position: 0.0,
            hover: -1.0,
            pressed: false,
            thumb_drag: false,
            page_at: None,
        }
    }
}

impl ScrollBar {
    /// The wheel over `viewport` and the mouse on `bar` (`id`) move the
    /// view of a column `total` pixels tall through a viewport `visible`
    /// pixels tall; the position ends within the column. `wheel_scale`
    /// is the pixels a point of wheel movement scrolls.
    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        ui: &mut Ui,
        pixels: &PixelPainter,
        id: Id,
        viewport: Rect,
        bar: Rect,
        total: f32,
        visible: f32,
        wheel_scale: f32,
    ) {
        let max = (total - visible).max(0.0);
        let viewport_rect = pixels.rect(viewport.into());
        if max > 0.0 && ui.rect_contains_pointer(viewport_rect) {
            let wheel = ui.input(|input| input.smooth_scroll_delta.y);
            self.position -= wheel * wheel_scale;
        }
        let mut hover = false;
        self.pressed = false;
        if max > 0.0 {
            let bar_rect = pixels.rect(bar.into());
            let response = ui.interact(bar_rect, id, Sense::click_and_drag());
            hover = response.hovered() || response.dragged();
            self.pressed = response.is_pointer_button_down_on();
            let ppp = pixels.pixels_per_point();
            let scale = total / bar.height as f32;
            let (pressed, now) = ui.input(|input| (input.pointer.primary_pressed(), input.time));
            let page_towards = |position: &mut f32, pos: egui::Pos2| -> bool {
                let y = (pos.y - bar_rect.min.y) * ppp * scale;
                if y < *position {
                    *position -= visible;
                } else if y > *position + visible {
                    *position += visible;
                } else {
                    return false;
                }
                true
            };
            if pressed && response.is_pointer_button_down_on() {
                self.thumb_drag = false;
                self.page_at = None;
                if let Some(pos) = response.interact_pointer_pos() {
                    if page_towards(&mut self.position, pos) {
                        self.page_at = Some(now + FIRST_REPEAT);
                    } else {
                        self.thumb_drag = true;
                    }
                }
            } else if response.is_pointer_button_down_on() {
                // A press held beside the thumb keeps paging towards the
                // pointer while it stays beside it.
                if let (Some(at), Some(pos)) = (self.page_at, response.interact_pointer_pos()) {
                    if now >= at {
                        page_towards(&mut self.position, pos);
                        self.page_at = Some(now + REPEAT);
                    }
                    ui.ctx().request_repaint();
                }
            } else {
                self.page_at = None;
            }
            if response.dragged() && self.thumb_drag {
                self.position += response.drag_delta().y * ppp * scale;
            }
        }
        // OpenGlScrollQuad::render: the bar widens while hovered.
        let last_hover = self.hover;
        self.hover = if hover {
            (self.hover + HOVER_CHANGE).min(1.0)
        } else {
            (self.hover - HOVER_CHANGE).max(0.0)
        };
        if self.hover != last_hover {
            ui.ctx().request_repaint();
        }
        self.position = self.position.clamp(0.0, max);
    }

    /// The bar over the visible part of the column, shown while the column
    /// is taller than the viewport (JUCE's auto-hide), in the scope's
    /// lightened screen colour.
    pub fn paint(&self, pixels: &PixelPainter, scope: &Scope, bar: Rect, total: f32, visible: f32) {
        if let Some(quad) = self.quad(scope, bar, total, visible) {
            let mut batch = QuadBatch::new();
            batch.push(quad);
            batch.paint(pixels, RectF::from(bar));
        }
    }

    /// The bar [`Self::paint`] draws, for a caller that draws it with its
    /// own quads; None while the column fits the viewport.
    pub fn quad(&self, scope: &Scope, bar: Rect, total: f32, visible: f32) -> Option<Quad> {
        if total <= visible || total <= 0.0 {
            return None;
        }
        let start = self.position / total;
        let end = (self.position + visible) / total;
        // setShrinkLeft: the left half of the track at rest, all of it
        // hovered.
        let width = bar.width as f32 * (1.0 + self.hover.max(0.0)) / 2.0;
        let rect = RectF::new(
            bar.x as f32,
            bar.y as f32 + start * bar.height as f32,
            width,
            (end - start) * bar.height as f32,
        );
        let color = scope.color(ColorId::LightenScreen);
        let color = if self.pressed {
            color.overlaid_with(color)
        } else {
            color
        };
        let mut quad = Quad::new(QuadKind::RoundedRectangle, rect, color);
        quad.rounding = bar.width as f32 * 0.25;
        Some(quad)
    }
}
