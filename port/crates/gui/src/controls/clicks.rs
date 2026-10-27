// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_gui_basics, GPL-3.0-only).
// SPDX-License-Identifier: GPL-3.0-or-later AND GPL-3.0-only

//! JUCE's multiple-click rules (`MouseInputSource::getNumberOfMultipleClicks`
//! and `isLongPressOrDrag`, in
//! `vital/third_party/JUCE/modules/juce_gui_basics/mouse/juce_MouseInputSource.cpp`),
//! which decide how many clicks a press makes and so when a component gets
//! `mouseDoubleClick`.

use egui::{Modifiers, PointerButton, Pos2};

/// `MouseEvent::getDoubleClickTimeout`: the most time between two presses of
/// a double click; the third press of a triple click gets twice this.
pub(crate) const DOUBLE_CLICK_TIMEOUT: f64 = 0.4;
/// `getPositionToleranceForInputType` for a mouse: the most distance, in
/// points along each axis, between presses that make one multiple click.
pub(crate) const MULTIPLE_CLICK_DISTANCE: f32 = 8.0;
/// `MouseInputSourceImpl::handleEvent`: a press that has moved this many
/// points is a drag.
const MOVED_SIGNIFICANTLY: f32 = 4.0;
/// `isLongPressOrDrag`: a press held longer than this, in seconds, is a long
/// press.
const LONG_PRESS: f64 = 0.3;

/// `RecentMouseDown`: one press, with its buttons and modifier keys
/// (JUCE's `ModifierKeys` holds both).
#[derive(Clone, Copy, Debug, PartialEq)]
struct RecentPress {
    time: f64,
    at: Pos2,
    button: PointerButton,
    modifiers: Modifiers,
}

impl RecentPress {
    /// `canBePartOfMultipleClickWith`.
    fn chains_with(&self, earlier: &RecentPress, timeout: f64) -> bool {
        self.time - earlier.time < timeout
            && (self.at.x - earlier.at.x).abs() < MULTIPLE_CLICK_DISTANCE
            && (self.at.y - earlier.at.y).abs() < MULTIPLE_CLICK_DISTANCE
            && self.button == earlier.button
            && self.modifiers == earlier.modifiers
    }
}

/// The recent presses of one component's mouse, newest first (JUCE's
/// `mouseDowns`), and whether the newest has moved enough to be a drag.
/// Positions are in points.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Clicks {
    presses: [Option<RecentPress>; 4],
    moved: bool,
}

impl Clicks {
    /// A press of `button` at `at`, with `modifiers` held.
    pub fn press(&mut self, time: f64, at: Pos2, button: PointerButton, modifiers: Modifiers) {
        self.presses.rotate_right(1);
        self.presses[0] = Some(RecentPress {
            time,
            at,
            button,
            modifiers,
        });
        self.moved = false;
    }

    /// The pointer moved to `at` while pressed.
    pub fn drag(&mut self, at: Pos2) {
        if let Some(press) = self.presses[0] {
            self.moved |= at.distance(press.at) >= MOVED_SIGNIFICANTLY;
        }
    }

    /// `isLongPressOrDrag`, at `now`: the newest press moved at least 4
    /// points or has been held more than 0.3 s.
    pub fn is_long_press_or_drag(&self, now: f64) -> bool {
        self.moved || self.presses[0].is_some_and(|press| now > press.time + LONG_PRESS)
    }

    /// `getNumberOfMultipleClicks`, at `now`: 1, plus each earlier press the
    /// newest chains with, unless the newest is a long press or a drag.
    pub fn count(&self, now: f64) -> u32 {
        let Some(newest) = self.presses[0] else {
            return 0;
        };
        if self.is_long_press_or_drag(now) {
            return 1;
        }
        let mut clicks = 1;
        for (i, earlier) in self.presses.iter().enumerate().skip(1) {
            let timeout = DOUBLE_CLICK_TIMEOUT * i.min(2) as f64;
            match earlier {
                Some(earlier) if newest.chains_with(earlier, timeout) => clicks += 1,
                _ => break,
            }
        }
        clicks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: PointerButton = PointerButton::Primary;

    fn click(clicks: &mut Clicks, time: f64, x: f32) {
        clicks.press(time, Pos2::new(x, 0.0), PRIMARY, Modifiers::NONE);
    }

    #[test]
    fn presses_close_in_time_and_place_chain() {
        let mut clicks = Clicks::default();
        click(&mut clicks, 0.0, 10.0);
        assert_eq!(clicks.count(0.1), 1);
        click(&mut clicks, 0.3, 17.0);
        assert_eq!(clicks.count(0.35), 2);
        // The third press gets twice the timeout from the first.
        click(&mut clicks, 0.7, 17.0);
        assert_eq!(clicks.count(0.75), 3);
        click(&mut clicks, 1.2, 17.0);
        assert_eq!(clicks.count(1.25), 1);
        click(&mut clicks, 1.3, 26.0);
        assert_eq!(clicks.count(1.35), 1);
    }

    #[test]
    fn a_drag_or_a_long_press_is_one_click() {
        let mut clicks = Clicks::default();
        click(&mut clicks, 0.0, 10.0);
        click(&mut clicks, 0.2, 10.0);
        clicks.drag(Pos2::new(13.0, 0.0));
        assert_eq!(clicks.count(0.25), 2);
        clicks.drag(Pos2::new(14.0, 0.0));
        assert_eq!(clicks.count(0.25), 1);
        // Moving back doesn't undo it.
        clicks.drag(Pos2::new(10.0, 0.0));
        assert_eq!(clicks.count(0.25), 1);

        click(&mut clicks, 1.0, 10.0);
        click(&mut clicks, 1.2, 10.0);
        assert_eq!(clicks.count(1.5), 2);
        assert_eq!(clicks.count(1.51), 1);
    }

    #[test]
    fn presses_chain_only_with_the_same_buttons_and_modifiers() {
        let mut clicks = Clicks::default();
        click(&mut clicks, 0.0, 10.0);
        clicks.press(
            0.1,
            Pos2::new(10.0, 0.0),
            PointerButton::Secondary,
            Modifiers::NONE,
        );
        assert_eq!(clicks.count(0.15), 1);
        click(&mut clicks, 0.2, 10.0);
        assert_eq!(clicks.count(0.25), 1);
        clicks.press(0.3, Pos2::new(10.0, 0.0), PRIMARY, Modifiers::SHIFT);
        assert_eq!(clicks.count(0.35), 1);
    }
}
