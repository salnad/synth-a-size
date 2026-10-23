// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The on-screen keyboard: 128 keys that play notes and light up while
//! held.
//!
//! Ports `KeyboardInterface` (`vital/src/interface/editor_sections/keyboard_interface.cpp`)
//! and `MidiKeyboard` (`editor_components/midi_keyboard.{h,cpp}`). Notes go
//! to the plugin through [`EditorModel::note_on`] / [`EditorModel::note_off`]
//! (upstream's `MidiKeyboardState`).
//!
//! [`EditorModel::note_on`]: crate::EditorModel::note_on
//! [`EditorModel::note_off`]: crate::EditorModel::note_off

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use egui::{Pos2, Sense};

use super::Panel;
use crate::interface::EditorModel;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::section::paint_body;
use crate::skin::{ColorId, Section, ValueId};

/// The section's skin override (`setSkinOverride(Skin::kKeyboard)`).
pub const SKIN: &[Section] = &[Section::Keyboard];

const MIDI_SIZE: i32 = 128;
const NOTES_PER_OCTAVE: i32 = 12;
const NUM_WHITE_KEYS: i32 = 75;
const NUM_WHITE_KEYS_PER_OCTAVE: i32 = 7;
const NUM_BLACK_KEYS: i32 = MIDI_SIZE - NUM_WHITE_KEYS;
const NUM_BLACK_KEYS_PER_OCTAVE: i32 = NOTES_PER_OCTAVE - NUM_WHITE_KEYS_PER_OCTAVE;
const BLACK_KEY_HEIGHT_RATIO: f32 = 0.7;
const BLACK_KEY_WIDTH_RATIO: f32 = 0.8;

const BLACK_KEY_OFFSETS: [f32; NUM_BLACK_KEYS_PER_OCTAVE as usize] = [
    1.0 - 0.6 * BLACK_KEY_WIDTH_RATIO,
    2.0 - 0.4 * BLACK_KEY_WIDTH_RATIO,
    4.0 - 0.7 * BLACK_KEY_WIDTH_RATIO,
    5.0 - 0.5 * BLACK_KEY_WIDTH_RATIO,
    6.0 - 0.3 * BLACK_KEY_WIDTH_RATIO,
];
const WHITE_KEYS: [bool; NOTES_PER_OCTAVE as usize] = [
    true, false, true, false, true, true, false, true, false, true, false, true,
];

pub fn is_white_key(midi: i32) -> bool {
    WHITE_KEYS[(midi % NOTES_PER_OCTAVE) as usize]
}

fn black_key_octave_offset(mut black_key_index: i32) -> i32 {
    for (i, &white) in WHITE_KEYS.iter().enumerate() {
        if !white {
            if black_key_index == 0 {
                return i as i32;
            }
            black_key_index -= 1;
        }
    }
    -1
}

fn white_key_octave_offset(mut white_key_index: i32) -> i32 {
    for (i, &white) in WHITE_KEYS.iter().enumerate() {
        if white {
            if white_key_index == 0 {
                return i as i32;
            }
            white_key_index -= 1;
        }
    }
    -1
}

fn black_key_index_from_offset(note_offset: i32) -> i32 {
    WHITE_KEYS[..note_offset as usize]
        .iter()
        .filter(|white| !**white)
        .count() as i32
}

fn white_key_index_from_offset(note_offset: i32) -> i32 {
    WHITE_KEYS[..note_offset as usize]
        .iter()
        .filter(|white| **white)
        .count() as i32
}

/// `MidiKeyboard::resized`'s key geometry for a keyboard `bounds` (pixels).
#[derive(Clone, Debug, PartialEq)]
pub struct KeyboardLayout {
    pub bounds: Rect,
    /// Each black key, lowest first.
    pub black_keys: Vec<RectF>,
}

impl KeyboardLayout {
    pub fn new(bounds: Rect) -> KeyboardLayout {
        let width = bounds.width as f32;
        let height = bounds.height as f32;
        let black_key_height = 2.0 * ((height * BLACK_KEY_HEIGHT_RATIO) as i32) as f32 / height;
        let black_key_y = 1.0 - black_key_height;
        let white_key_width = 2.0 / NUM_WHITE_KEYS as f32;
        let black_key_width =
            (((BLACK_KEY_WIDTH_RATIO * white_key_width * width / 4.0) as i32) as f32 * 4.0 + 2.0)
                / width;
        let octave_width = NUM_WHITE_KEYS_PER_OCTAVE as f32 * white_key_width;
        let black_keys = (0..NUM_BLACK_KEYS)
            .map(|i| {
                let octave = i / NUM_BLACK_KEYS_PER_OCTAVE;
                let index = i % NUM_BLACK_KEYS_PER_OCTAVE;
                let mut x = -1.0
                    + octave_width * octave as f32
                    + BLACK_KEY_OFFSETS[index as usize] * white_key_width;
                x = ((((x + 1.0) * width / 2.0) as i32) as f32) * 2.0 / width - 1.0;
                // setQuad(i, x, black_key_y, black_key_width, black_key_height
                // + 0.5) in device coordinates, as pixels.
                let quad_height = black_key_height + 0.5;
                RectF::new(
                    bounds.x as f32 + (x + 1.0) * width / 2.0,
                    bounds.y as f32 + (1.0 - (black_key_y + quad_height)) * height / 2.0,
                    black_key_width * width / 2.0,
                    quad_height * height / 2.0,
                )
            })
            .collect();
        KeyboardLayout { bounds, black_keys }
    }

    /// `setWhiteKeyQuad`: white key `index`'s highlight.
    fn white_key(&self, index: i32) -> RectF {
        let full_width = self.bounds.width as f32;
        let start_x = (index as f32 * full_width / NUM_WHITE_KEYS as f32 + 1.0) as i32;
        let end_x = ((index + 1) as f32 * full_width / NUM_WHITE_KEYS as f32) as i32;
        // y from -2 to 2 in device coordinates: well past both edges.
        let height = self.bounds.height as f32;
        RectF::new(
            (self.bounds.x + start_x) as f32,
            self.bounds.y as f32 - height / 2.0,
            (end_x - start_x) as f32,
            2.0 * height,
        )
    }

    /// `setBlackKeyQuad`: black key `index`'s highlight, a pixel inside it.
    fn black_key(&self, index: i32) -> RectF {
        let key = self.black_keys[index as usize];
        RectF::new(key.x + 1.0, key.y - 1.0, key.width - 2.0, key.height)
    }

    /// `getNoteAtPosition`, for a position in pixels from the keyboard's top
    /// left.
    pub fn note_at(&self, x: f32, y: f32) -> i32 {
        let white_key_position = NUM_WHITE_KEYS as f32 * x / self.bounds.width as f32;
        let octave = (white_key_position / NUM_WHITE_KEYS_PER_OCTAVE as f32) as i32;
        let white_key_in_octave = white_key_position - (octave * NUM_WHITE_KEYS_PER_OCTAVE) as f32;
        if y / (self.bounds.height as f32) < BLACK_KEY_HEIGHT_RATIO {
            for (i, offset) in BLACK_KEY_OFFSETS.iter().enumerate() {
                let note_offset = white_key_in_octave - offset;
                if (0.0..=BLACK_KEY_WIDTH_RATIO).contains(&note_offset) {
                    let note = octave * NOTES_PER_OCTAVE + black_key_octave_offset(i as i32);
                    return note.clamp(0, MIDI_SIZE - 1);
                }
            }
        }
        let white_key_index = (white_key_in_octave as i32).clamp(0, NUM_WHITE_KEYS_PER_OCTAVE - 1);
        let note = octave * NOTES_PER_OCTAVE + white_key_octave_offset(white_key_index);
        note.clamp(0, MIDI_SIZE - 1)
    }

    /// `getVelocityForNote`: lower on the key is louder.
    pub fn velocity_for(&self, midi: i32, y: f32) -> f32 {
        let min_velocity = 1.0 / (MIDI_SIZE - 1) as f32;
        let height = self.bounds.height as f32;
        let velocity = if is_white_key(midi) {
            y / height
        } else {
            y / (BLACK_KEY_HEIGHT_RATIO * height)
        };
        velocity.clamp(min_velocity, 1.0)
    }

    /// The highlight for `midi`: a white or black key quad.
    fn key_quad(&self, midi: i32) -> RectF {
        let octave = midi / NOTES_PER_OCTAVE;
        let offset = midi - octave * NOTES_PER_OCTAVE;
        if is_white_key(midi) {
            self.white_key(octave * NUM_WHITE_KEYS_PER_OCTAVE + white_key_index_from_offset(offset))
        } else {
            self.black_key(octave * NUM_BLACK_KEYS_PER_OCTAVE + black_key_index_from_offset(offset))
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct KeyboardState {
    /// `hover_note_`.
    hover: Option<i32>,
}

/// The note the mouse is playing (-1 for none), shared with the editor so
/// a window closing mid-press can release it.
#[derive(Clone, Debug)]
pub struct PlayingNote(Arc<AtomicI32>);

impl Default for PlayingNote {
    fn default() -> PlayingNote {
        PlayingNote(Arc::new(AtomicI32::new(-1)))
    }
}

impl PlayingNote {
    pub fn get(&self) -> Option<i32> {
        let note = self.0.load(Ordering::Relaxed);
        (note >= 0).then_some(note)
    }

    fn set(&self, note: Option<i32>) {
        self.0.store(note.unwrap_or(-1), Ordering::Relaxed);
    }

    /// Releases the note, if one is playing (the editor is closing).
    pub fn release(&self, model: &dyn EditorModel) {
        if let Some(note) = self.get() {
            model.note_off(note as u8);
            self.set(None);
        }
    }
}

/// `KeyboardInterface::paintBackground` with `MidiKeyboard::paintBackground`:
/// the body and the lines between white keys.
pub fn paint_background(pixels: &PixelPainter, scope: &Scope, layout: &KeyboardLayout) {
    paint_body(pixels, scope, layout.bounds);
    let black_key_color = scope.color(ColorId::WidgetSecondary2);
    let width = layout.bounds.width as f32;
    for i in 1..NUM_WHITE_KEYS {
        let x = (i as f32 * width / NUM_WHITE_KEYS as f32) as i32;
        pixels.fill_rect(
            Rect::new(
                layout.bounds.x + x,
                layout.bounds.y,
                1,
                layout.bounds.height,
            ),
            black_key_color,
        );
    }
}

/// `MidiKeyboard`'s mouse handling and `render`. Bit n of `held` is set
/// while note n is on (any channel).
pub fn show(panel: &mut Panel, layout: &KeyboardLayout, held: u128, playing: &PlayingNote) {
    let id = panel.ui.scope_id().with("midi keyboard");
    let rect = panel.pixels.rect(layout.bounds.into());
    let response = panel.ui.interact(rect, id, Sense::click_and_drag());
    let mut state: KeyboardState = panel.ui.data(|data| data.get_temp(id)).unwrap_or_default();
    let pointer = panel.ui.input(|input| input.pointer.interact_pos());
    let local = pointer.map(|pos| to_local(panel, layout, pos));
    let model = panel.host.model();

    let pressed = response.is_pointer_button_down_on();
    match (playing.get(), pressed, local) {
        (None, true, Some((x, y))) => {
            // mouseDown.
            let note = layout.note_at(x, y);
            model.note_on(note as u8, layout.velocity_for(note, y));
            playing.set(Some(note));
            state.hover = Some(note);
        }
        (Some(current), true, Some((x, y))) => {
            // mouseDrag: a new key releases the old one.
            let note = layout.note_at(x, y);
            if note != current {
                model.note_off(current as u8);
                model.note_on(note as u8, layout.velocity_for(note, y));
                playing.set(Some(note));
            }
            state.hover = Some(note);
        }
        (Some(current), false, _) => {
            // mouseUp.
            model.note_off(current as u8);
            playing.set(None);
            state.hover = local
                .filter(|_| response.hovered())
                .map(|(x, y)| layout.note_at(x, y));
        }
        _ => {
            // mouseMove / mouseEnter / mouseExit.
            state.hover = local
                .filter(|_| response.hovered())
                .map(|(x, y)| layout.note_at(x, y));
        }
    }
    panel.ui.data_mut(|data| data.insert_temp(id, state));

    let scope = &panel.scope;
    let rounding = scope.value(ValueId::WidgetRoundedCorner);
    let press_color = scope.color(ColorId::WidgetPrimary1);
    let hover_color = scope.color(ColorId::WidgetAccent2);
    let black_key_color = scope.color(ColorId::WidgetSecondary2);
    // Until a note's press reaches the plugin's shared state, the key the
    // mouse plays is lit too.
    let is_held = |midi: i32| held >> midi & 1 == 1 || playing.get() == Some(midi);

    let mut quads = Vec::new();
    if let Some(hover) = state.hover.filter(|note| is_white_key(*note)) {
        quads.push(Quad::new(
            QuadKind::RoundedRectangle,
            layout.key_quad(hover),
            hover_color,
        ));
    }
    for midi in (0..MIDI_SIZE).filter(|&midi| is_white_key(midi) && is_held(midi)) {
        quads.push(Quad::new(
            QuadKind::RoundedRectangle,
            layout.key_quad(midi),
            press_color,
        ));
    }
    for key in &layout.black_keys {
        let mut quad = Quad::new(QuadKind::RoundedRectangle, *key, black_key_color);
        quad.rounding = rounding;
        quads.push(quad);
    }
    if let Some(hover) = state.hover.filter(|note| !is_white_key(*note)) {
        let mut quad = Quad::new(
            QuadKind::RoundedRectangle,
            layout.key_quad(hover),
            hover_color,
        );
        quad.rounding = rounding;
        quads.push(quad);
    }
    for midi in (0..MIDI_SIZE).filter(|&midi| !is_white_key(midi) && is_held(midi)) {
        let mut quad = Quad::new(
            QuadKind::RoundedRectangle,
            layout.key_quad(midi),
            press_color,
        );
        quad.rounding = rounding;
        quads.push(quad);
    }
    for quad in quads {
        panel.quad(quad);
    }
}

fn to_local(panel: &Panel, layout: &KeyboardLayout, pointer: Pos2) -> (f32, f32) {
    let area = panel.pixels.area();
    let ppp = panel.pixels.pixels_per_point();
    (
        (pointer.x - area.min.x) * ppp - layout.bounds.x as f32,
        (pointer.y - area.min.y) * ppp - layout.bounds.y as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_at_positions() {
        let layout = KeyboardLayout::new(Rect::new(0, 0, 750, 100));
        // Ten pixels a white key. Low on the first key: C0 (note 0).
        assert_eq!(layout.note_at(5.0, 90.0), 0);
        // High between the first two white keys: the C# black key.
        assert_eq!(layout.note_at(10.0, 10.0), 1);
        // The eighth white key is the next octave's C.
        assert_eq!(layout.note_at(75.0, 90.0), 12);
        assert_eq!(layout.note_at(749.0, 90.0), 127);
    }

    #[test]
    fn velocity_rises_down_the_key() {
        let layout = KeyboardLayout::new(Rect::new(0, 0, 750, 100));
        assert_eq!(layout.velocity_for(0, 50.0), 0.5);
        assert_eq!(layout.velocity_for(1, 35.0), 0.5);
        assert_eq!(layout.velocity_for(0, 0.0), 1.0 / 127.0);
    }

    #[test]
    fn black_keys_follow_upstream_geometry() {
        let layout = KeyboardLayout::new(Rect::new(0, 0, 750, 100));
        assert_eq!(layout.black_keys.len(), 53);
        let first = layout.black_keys[0];
        // x: 0.52 of a 10-pixel white key, truncated. Width: (int(0.8 * 20 /
        // 4) * 4 + 2) device units of 750, which is 9 pixels.
        assert!((first.x - 5.0).abs() < 1e-4, "{first:?}");
        assert!((first.width - 9.0).abs() < 1e-4, "{first:?}");
        // Down to 70% of the height, from above the top edge.
        assert!((first.y + first.height - 70.0).abs() < 1e-4);
    }
}
