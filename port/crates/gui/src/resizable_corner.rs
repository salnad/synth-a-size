// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor window's corner resizer: JUCE's `ResizableCornerComponent`,
//! which upstream's plugin editor shows at its bottom right
//! (`SynthEditor`: `setResizable(true, true)`), driven through its
//! `BorderBoundsConstrainer` (`vital/src/common/border_bounds_constrainer.cpp`).
//! Dragging it resizes the window; letting go saves the size
//! (`resizeEnd`: `LoadSave::saveWindowSize`). Sizes the host sets are never
//! saved, as upstream's aren't.

use egui::{Color32, CursorIcon, Pos2, Rect, Sense, Stroke, Vec2};

use crate::editor::{MIN_HEIGHT, MIN_WIDTH};
use crate::layout::{DEFAULT_HEIGHT, DEFAULT_WIDTH};

/// `AudioProcessorEditor::editorResized`'s `resizerSize`.
pub const SIZE: f32 = 18.0;

/// What the corner did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CornerEvent {
    /// `mouseDrag`: the editor's new size, constrained, to ask of the host.
    pub resize: Option<Vec2>,
    /// `mouseUp` (`resizeEnd`): the drag ended, and the editor's size is to
    /// be saved as the window has it (`saveWindowSize(gui width /
    /// kDefaultWindowWidth)`), once the host has answered `resize`.
    pub ended: bool,
}

/// The corner's state between frames: where a drag started.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResizableCorner {
    /// `originalBounds`' size and the pointer where the press was.
    drag: Option<(Vec2, Pos2)>,
    /// The size the drag last asked for.
    last: Option<Vec2>,
}

/// JUCE's `roundToInt`, which rounds halves to even.
fn round_to_int(value: f64) -> i32 {
    value.round_ties_even() as i32
}

/// `ComponentBoundsConstrainer::checkBounds` for a drag of the bottom right
/// corner (`isStretchingBottom` and `isStretchingRight`), with upstream's
/// minimum size and fixed aspect ratio and no maximum: `bounds` is the size
/// asked for, `old` the editor's size before this step. Upstream's border is
/// empty in the plugin, and its bound by the screen isn't kept (D30).
pub fn constrain(bounds: (i32, i32), old: (i32, i32)) -> (i32, i32) {
    const MAX: i32 = 0x3fff_ffff;
    let (min_w, min_h) = (MIN_WIDTH as i32, MIN_HEIGHT as i32);
    let aspect = DEFAULT_WIDTH as f64 / DEFAULT_HEIGHT as f64;
    let mut width = bounds.0.clamp(min_w, MAX);
    let mut height = bounds.1.clamp(min_h, MAX);
    let old_ratio = if old.1 > 0 {
        (old.0 as f64 / old.1 as f64).abs()
    } else {
        0.0
    };
    let new_ratio = (width as f64 / height as f64).abs();
    if old_ratio > new_ratio {
        width = round_to_int(height as f64 * aspect);
        if width > MAX || width < min_w {
            width = width.clamp(min_w, MAX);
            height = round_to_int(width as f64 / aspect);
        }
    } else {
        height = round_to_int(width as f64 / aspect);
        if height > MAX || height < min_h {
            height = height.clamp(min_h, MAX);
            width = round_to_int(height as f64 * aspect);
        }
    }
    (width, height)
}

/// `ResizableCornerComponent::hitTest`, in the corner's own pixels: the part
/// on and below a line a quarter of the corner above its diagonal.
pub fn hits(x: i32, y: i32) -> bool {
    let size = SIZE as i32;
    let y_at_x = size - (size * x / size);
    y >= y_at_x - size / 4
}

impl ResizableCorner {
    /// The corner at the bottom right of `editor` (points): drawn on top,
    /// and its presses and drags handled.
    pub fn show(&mut self, ui: &mut egui::Ui, editor: Rect) -> CornerEvent {
        let rect = Rect::from_min_size(editor.max - Vec2::splat(SIZE), Vec2::splat(SIZE));
        let id = ui.scope_id().with("resizable corner");
        let inside = |pos: Pos2| {
            let local = pos - rect.min;
            rect.contains(pos) && hits(local.x.floor() as i32, local.y.floor() as i32)
        };
        let pointer = ui.input(|input| input.pointer.latest_pos());
        let over = pointer.is_some_and(inside);
        let pressed = ui.input(|input| input.pointer.primary_pressed());
        let released = ui.input(|input| input.pointer.primary_released());
        // The corner takes the pointer only where its hit test allows, so a
        // press in the square's other half reaches the editor beneath.
        let on_top = (over || self.drag.is_some())
            && ui.interact(rect, id, Sense::click_and_drag()).hovered();
        let mut event = CornerEvent::default();
        if pressed && over && on_top {
            let at = ui.input(|input| input.pointer.press_origin()).or(pointer);
            if let Some(at) = at {
                self.drag = Some((editor.size(), at));
                self.last = None;
            }
        }
        if let Some((original, start)) = self.drag {
            if let Some(now) = pointer {
                let delta = now - start;
                let asked = (
                    round_to_int(f64::from(original.x + delta.x)),
                    round_to_int(f64::from(original.y + delta.y)),
                );
                let old = (
                    round_to_int(f64::from(editor.width())),
                    round_to_int(f64::from(editor.height())),
                );
                let (width, height) = constrain(asked, old);
                let size = Vec2::new(width as f32, height as f32);
                if self.last != Some(size) && (delta.x != 0.0 || delta.y != 0.0) {
                    self.last = Some(size);
                    event.resize = Some(size);
                }
            }
            if released {
                event.ended = true;
                self.drag = None;
                self.last = None;
            }
        }
        if over || self.drag.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
        }
        paint(ui.painter(), rect);
        event
    }

    /// Whether a drag is under way.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }
}

/// `LookAndFeel_V2::drawCornerResizer` (upstream's look and feel keeps
/// it): four diagonals, light grey with dark grey beside, each
/// `min(w, h) * 0.075` thick, clipped to the corner.
fn paint(painter: &egui::Painter, rect: Rect) {
    let painter = painter.with_clip_rect(rect);
    let (w, h) = (rect.width(), rect.height());
    let thickness = w.min(h) * 0.075;
    let at = |x: f32, y: f32| rect.min + Vec2::new(x, y);
    let light = Color32::from_rgb(0xd3, 0xd3, 0xd3);
    let dark = Color32::from_rgb(0x55, 0x55, 0x55);
    let mut i = 0.0f32;
    while i < 1.0 {
        painter.line_segment(
            [at(w * i, h + 1.0), at(w + 1.0, h * i)],
            Stroke::new(thickness, light),
        );
        painter.line_segment(
            [
                at(w * i + thickness, h + 1.0),
                at(w + 1.0, h * i + thickness),
            ],
            Stroke::new(thickness, dark),
        );
        i += 0.3;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drag of the corner keeps upstream's ratio, keeping the side that
    /// moved past it, and stops at the minimum size.
    #[test]
    fn a_corner_drag_keeps_the_ratio_and_the_minimum() {
        // Wider than the old ratio: the width stays and the height follows.
        assert_eq!(constrain((1600, 900), (1400, 820)), (1600, 937));
        // Taller: the height stays and the width follows.
        assert_eq!(constrain((1500, 1000), (1400, 820)), (1707, 1000));
        assert_eq!(constrain((100, 50), (1400, 820)), (350, 205));
        // A height of exactly a half rounds to even, as `roundToInt` does.
        assert_eq!(constrain((455, 266), (1400, 820)), (455, 266));
        assert_eq!(constrain((595, 348), (1400, 820)), (595, 348));
        // From a host's 1900 × 400 docker, a small drag gives a window in
        // the ratio again.
        assert_eq!(constrain((1910, 405), (1900, 400)), (691, 405));
    }

    /// `hitTest`: the bottom right half of the square, and a quarter above
    /// the diagonal.
    #[test]
    fn the_corner_takes_presses_below_its_diagonal() {
        assert!(hits(17, 17));
        assert!(hits(9, 9));
        assert!(hits(4, 10));
        assert!(!hits(1, 1));
        assert!(!hits(2, 8));
    }
}
