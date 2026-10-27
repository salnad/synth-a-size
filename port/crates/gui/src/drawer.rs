// SPDX-License-Identifier: GPL-3.0-or-later

//! The chat's drawer: a strip at the window's right that the editor makes
//! room for by widening its window, so upstream's layout keeps its size
//! beside it (an addition with no upstream counterpart, D13 in
//! `migration/DIVERGENCES.md`).
//!
//! The drawer only takes room: the chat panel draws inside it. Whether it is
//! docked is read from the window itself: it docks when the window is wide
//! enough to hold upstream's layout and the drawer, so a host that refuses
//! the wider window leaves the panel in its overlay over the editor.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::chat::PANEL_WIDTH;
use crate::layout::{DEFAULT_HEIGHT, DEFAULT_WIDTH};

/// Whether the drawer is open, kept per plugin instance while the window is
/// closed (as the window's size is), and not in the preset; and the last
/// window size the host set, which answers the editor's requests.
#[derive(Clone, Debug, Default)]
pub struct DrawerState {
    open: Arc<AtomicBool>,
    /// Width and height in pixels, packed; 0 when none since the last
    /// request.
    host_size: Arc<AtomicU64>,
}

impl DrawerState {
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Relaxed)
    }

    pub fn set_open(&self, open: bool) {
        self.open.store(open, Ordering::Relaxed);
    }

    /// The host set the window to `width` × `height` pixels (VST3's
    /// `onSize`, through the editor's `set_size`).
    pub fn host_resized(&self, width: u32, height: u32) {
        let packed = (u64::from(width) << 32) | u64::from(height);
        self.host_size.store(packed.max(1), Ordering::Relaxed);
    }

    /// The size the host last set since [`forget_host_size`](Self::forget_host_size).
    pub fn host_size(&self) -> Option<(u32, u32)> {
        match self.host_size.load(Ordering::Relaxed) {
            0 => None,
            packed => Some(((packed >> 32) as u32, packed as u32)),
        }
    }

    pub fn forget_host_size(&self) {
        self.host_size.store(0, Ordering::Relaxed);
    }
}

/// How long the editor waits for the host to set the size it asked for
/// before taking it as refused, in seconds.
pub const HOST_ANSWER_TIMEOUT: f64 = 1.0;

/// A window size the editor asked the host for, awaiting its answer.
/// nice-plug passes VST3's `resizeView` on without its result, and baseview
/// has already resized the editor's view, so a refusal shows only as the
/// host never setting a size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingResize {
    /// The size before the request, in points, to go back to.
    pub previous: egui::Vec2,
    /// When it was asked (egui's input time).
    pub asked_at: f64,
}

/// What became of a [`PendingResize`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostAnswer {
    Waiting,
    /// The host set a size, the one asked for or its own.
    Answered,
    /// No size came in time: the editor goes back to the previous size,
    /// and an open drawer overlays the editor.
    Refused,
}

impl PendingResize {
    pub fn answer(&self, host_size: Option<(u32, u32)>, now: f64) -> HostAnswer {
        if host_size.is_some() {
            HostAnswer::Answered
        } else if now - self.asked_at >= HOST_ANSWER_TIMEOUT {
            HostAnswer::Refused
        } else {
            HostAnswer::Waiting
        }
    }
}

/// The drawer's width in pixels for a window `height` pixels tall:
/// [`PANEL_WIDTH`] points at the editor's scale (the chat's text scale,
/// 0.5 to 2), rounded to whole pixels. The scale comes from the height,
/// which a docked drawer leaves to upstream's layout, so widening the
/// window doesn't change it.
pub fn width_pixels(height: f32, pixels_per_point: f32) -> f32 {
    let scale = (height / pixels_per_point / DEFAULT_HEIGHT as f32).clamp(0.5, 2.0);
    (PANEL_WIDTH * scale * pixels_per_point).round()
}

/// A window split between upstream's layout and the drawer, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Split {
    /// Where upstream's layout goes (letterboxed inside as before).
    pub editor: egui::Rect,
    /// The docked drawer, the window's full height at its right, if open and
    /// the window has room for it.
    pub drawer: Option<egui::Rect>,
}

/// How far, in pixels, the window may fall short of upstream's width plus
/// the drawer's and still dock it (a host's rounding).
const DOCK_TOLERANCE: f32 = 2.0;

/// Splits `window` (points) for a drawer that is `open`. The drawer docks
/// when the window is at least upstream's aspect-ratio width for its height
/// plus the drawer's width; otherwise the editor keeps the whole window.
pub fn split(window: egui::Rect, pixels_per_point: f32, open: bool) -> Split {
    let whole = Split {
        editor: window,
        drawer: None,
    };
    if !open {
        return whole;
    }
    let height = window.height() * pixels_per_point;
    let width = window.width() * pixels_per_point;
    let drawer = width_pixels(height, pixels_per_point);
    let editor_width = height * DEFAULT_WIDTH as f32 / DEFAULT_HEIGHT as f32;
    if width + DOCK_TOLERANCE < editor_width + drawer {
        return whole;
    }
    let right = (window.max.x * pixels_per_point).round();
    let edge = (right - drawer) / pixels_per_point;
    Split {
        editor: egui::Rect::from_min_max(window.min, egui::pos2(edge, window.max.y)),
        drawer: Some(egui::Rect::from_min_max(
            egui::pos2(edge, window.min.y),
            egui::pos2(right / pixels_per_point, window.max.y),
        )),
    }
}

/// The window size (pixels) closest to `size` that keeps room for the
/// drawer: `adjust` (the editor's own rule, upstream's aspect ratio and
/// minimum size) sizes the rest, and the drawer's width for the resulting
/// height is added back.
pub fn adjust_size(
    size: (u32, u32),
    scale_factor: f32,
    adjust: impl FnOnce((u32, u32)) -> (u32, u32),
) -> (u32, u32) {
    let drawer = |height: u32| width_pixels(height as f32, scale_factor) as u32;
    let (width, height) = size;
    let (editor_width, editor_height) = adjust((width.saturating_sub(drawer(height)), height));
    (editor_width + drawer(editor_height), editor_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(width: f32, height: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))
    }

    /// 360 points at the default height, following the height within the
    /// text scale's limits, in whole pixels.
    #[test]
    fn the_drawer_follows_the_editors_scale() {
        assert_eq!(width_pixels(820.0, 1.0), 360.0);
        assert_eq!(width_pixels(1640.0, 2.0), 720.0);
        assert_eq!(width_pixels(1640.0, 1.0), 720.0);
        assert_eq!(width_pixels(410.0, 1.0), 180.0);
        assert_eq!(width_pixels(205.0, 1.0), 180.0);
        assert_eq!(width_pixels(4000.0, 1.0), 720.0);
        assert_eq!(width_pixels(615.0, 1.5), 270.0);
    }

    /// Open in a window widened by the drawer, upstream's layout keeps the
    /// default size and the drawer takes the full height at the right.
    #[test]
    fn a_widened_window_docks_the_drawer() {
        let split = split(rect(1760.0, 820.0), 1.0, true);
        assert_eq!(split.editor, rect(1400.0, 820.0));
        assert_eq!(
            split.drawer,
            Some(egui::Rect::from_min_max(
                egui::pos2(1400.0, 0.0),
                egui::pos2(1760.0, 820.0)
            ))
        );
    }

    /// A window the host didn't widen keeps the whole editor (the overlay
    /// fallback), as does a closed drawer.
    #[test]
    fn without_room_the_editor_keeps_the_window() {
        let window = rect(1400.0, 820.0);
        assert_eq!(split(window, 1.0, true).drawer, None);
        assert_eq!(split(window, 1.0, true).editor, window);
        assert_eq!(split(rect(1760.0, 820.0), 1.0, false).drawer, None);
    }

    /// The edge between the two is on the pixel grid at a fractional scale.
    #[test]
    fn the_split_is_pixel_aligned() {
        let ppp = 1.5;
        let window = rect((1050.0 + 270.0) / ppp, 615.0 / ppp);
        let split = split(window, ppp, true);
        let drawer = split.drawer.unwrap();
        assert_eq!(drawer.min.x * ppp, 1050.0);
        assert_eq!(drawer.width() * ppp, 270.0);
        assert_eq!(split.editor.width() * ppp, 1050.0);
    }

    /// A size the host sets answers a request; without one the request
    /// times out as refused.
    #[test]
    fn a_request_is_answered_by_the_host_setting_a_size() {
        let state = DrawerState::default();
        assert_eq!(state.host_size(), None);
        let pending = PendingResize {
            previous: egui::vec2(1400.0, 820.0),
            asked_at: 10.0,
        };
        assert_eq!(pending.answer(state.host_size(), 10.5), HostAnswer::Waiting);
        assert_eq!(pending.answer(state.host_size(), 11.0), HostAnswer::Refused);
        state.host_resized(1760, 820);
        assert_eq!(state.host_size(), Some((1760, 820)));
        assert_eq!(
            pending.answer(state.host_size(), 11.0),
            HostAnswer::Answered
        );
        state.forget_host_size();
        assert_eq!(state.host_size(), None);
    }

    /// A host's proposed size keeps upstream's rule for the editor's part
    /// and adds the drawer for the resulting height, so closing it gives
    /// back exactly its width.
    #[test]
    fn sizes_keep_room_for_the_drawer() {
        let aspect = |(width, _height): (u32, u32)| (width, width * 820 / 1400);
        assert_eq!(adjust_size((1760, 820), 1.0, aspect), (1760, 820));
        // 220 pixels of drawer at 500 high leave 840 for the editor, which
        // makes it 492 high, where the drawer is 216.
        assert_eq!(adjust_size((1060, 500), 1.0, aspect), (1056, 492));
        assert_eq!(adjust_size((3520, 1640), 2.0, aspect), (3520, 1640));
        let (open, height) = adjust_size((1760, 820), 1.0, aspect);
        assert_eq!(open - width_pixels(height as f32, 1.0) as u32, 1400);
    }
}
