// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `TransposeQuantizeButton`: twelve dots laid out like an octave of keys,
//! lit for the notes transposition snaps to.
//!
//! Ports `setKeyBounds`, `TransposeQuantizeButton::paint` and the value
//! encoding (`getValue` / `setValue`) from
//! `vital/src/interface/editor_components/transpose_quantize.cpp`.

use crate::colour::Colour;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::scope::Scope;
use crate::skin::ColorId;

/// `vital::kNotesPerOctave`.
pub const NOTES_PER_OCTAVE: usize = 12;

/// `setKeyBounds`: each note's dot, in the coordinates of a `width` ×
/// `height` area whose top is at `y`.
pub fn key_bounds(y: f32, width: f32, height: f32) -> [RectF; NOTES_PER_OCTAVE] {
    const WHITE_KEYS: usize = 7;
    const MISSING_BLACK_KEY: usize = 2;
    const HEIGHT_DIFFERENCE_MULT: f32 = 0.866_025_4;
    const PADDING_RATIO: f32 = 0.03;
    const OUTER_PADDING_RATIO: f32 = 0.11;

    let inner_padding = (width * PADDING_RATIO) as i32 as f32;
    let outer_padding = (width * OUTER_PADDING_RATIO) as i32 as f32;
    let key_width = (width - (WHITE_KEYS - 1) as f32 * inner_padding - 2.0 * outer_padding) * 1.0
        / WHITE_KEYS as f32;
    let y_mid = y + (height - key_width) / 2.0;
    let height_offset = HEIGHT_DIFFERENCE_MULT * (key_width + inner_padding);
    let y_black = y_mid - height_offset / 2.0;
    let y_white = y_black + height_offset;

    let mut bounds = [RectF::new(0.0, 0.0, 0.0, 0.0); NOTES_PER_OCTAVE];
    for i in 0..WHITE_KEYS {
        let x = outer_padding + (key_width + inner_padding) * i as f32;
        let mut index = 2 * i;
        if i > MISSING_BLACK_KEY {
            index -= 1;
        }
        bounds[index] = RectF::new(x, y_white, key_width, key_width);
    }
    let black_offset = (key_width + inner_padding) / 2.0;
    for i in 0..WHITE_KEYS - 1 {
        if i == MISSING_BLACK_KEY {
            continue;
        }
        let x = outer_padding + black_offset + (key_width + inner_padding) * i as f32;
        let mut index = 2 * i;
        if i < MISSING_BLACK_KEY {
            index += 1;
        }
        bounds[index] = RectF::new(x, y_black, key_width, key_width);
    }
    bounds
}

/// The parameter value's notes (bits 0–11) and global snap (bit 12)
/// (`TransposeQuantizeButton::setValue`).
pub fn decode(value: f32) -> ([bool; NOTES_PER_OCTAVE], bool) {
    let value = value as i32;
    let mut selected = [false; NOTES_PER_OCTAVE];
    for (i, note) in selected.iter_mut().enumerate() {
        *note = (value >> i) & 1 != 0;
    }
    (selected, value >> NOTES_PER_OCTAVE != 0)
}

/// The button's colours: a selected note, a selected note with global
/// snap, and the lightening of unselected or hovered notes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuantizeColours {
    pub on: Colour,
    pub global_on: Colour,
    pub lighten: Colour,
}

impl QuantizeColours {
    pub fn new(scope: &Scope) -> QuantizeColours {
        QuantizeColours {
            on: scope.color(ColorId::WidgetPrimary1),
            global_on: scope.color(ColorId::UiActionButton),
            lighten: scope.color(ColorId::LightenScreen),
        }
    }
}

/// `TransposeQuantizeButton::paint` at `bounds` for the parameter `value`;
/// `hover` lightens every dot.
pub fn paint(
    pixels: &PixelPainter,
    colours: QuantizeColours,
    bounds: Rect,
    value: f32,
    hover: bool,
) {
    let (selected, global_snap) = decode(value);
    let keys = key_bounds(0.0, bounds.width as f32, bounds.height as f32);
    let lighten = colours.lighten;
    let on = if global_snap {
        colours.global_on
    } else {
        colours.on
    };
    let at = |key: RectF| {
        RectF::new(
            bounds.x as f32 + key.x,
            bounds.y as f32 + key.y,
            key.width,
            key.height,
        )
    };
    for (key, selected) in keys.iter().zip(selected) {
        pixels.fill_ellipse(at(*key), if selected { on } else { lighten });
    }
    if hover {
        for key in keys {
            pixels.fill_ellipse(at(key), lighten);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_follow_an_octave() {
        let keys = key_bounds(0.0, 47.0, 20.0);
        // White keys (C, D, E, F, G, A, B) share one row, black keys the
        // other, and run left to right.
        let whites = [0, 2, 4, 5, 7, 9, 11];
        let blacks = [1, 3, 6, 8, 10];
        assert!(whites.iter().all(|&i| keys[i].y == keys[0].y));
        assert!(blacks.iter().all(|&i| keys[i].y == keys[1].y));
        assert!(keys[1].y < keys[0].y);
        assert!(whites.windows(2).all(|w| keys[w[0]].x < keys[w[1]].x));
        assert!(blacks.windows(2).all(|w| keys[w[0]].x < keys[w[1]].x));
        // E and F are adjacent white keys with no black key between.
        assert_eq!(
            keys[3].x + keys[3].width / 2.0,
            (keys[2].x + keys[4].x) / 2.0 + keys[2].width / 2.0
        );
    }

    #[test]
    fn value_bits_are_notes_then_global_snap() {
        let (notes, global) = decode((1 + 4 + (1 << 12)) as f32);
        assert!(notes[0] && !notes[1] && notes[2]);
        assert!(global);
        assert_eq!(decode(0.0), ([false; 12], false));
    }
}
