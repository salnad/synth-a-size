// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `TempoSelector`: the small icon beside a frequency that picks seconds,
//! tempo, dotted tempo or triplets (`vital/src/interface/editor_components/tempo_selector.cpp`),
//! with the icons of `Paths::clock`, `note` and `tripletNotes`
//! (`look_and_feel/paths.h`).

use std::f32::consts::PI;
use std::sync::OnceLock;

use super::icon::fill_path_to_fit;
use super::path::Path;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::scope::Scope;
use crate::skin::ColorId;

/// `TempoSelector::MenuId`: the sync control's values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TempoMode {
    Seconds = 0,
    Tempo = 1,
    TempoDotted = 2,
    TempoTriplet = 3,
    Keytrack = 4,
}

impl TempoMode {
    pub fn from_value(value: f32) -> TempoMode {
        match value as i32 {
            0 => TempoMode::Seconds,
            2 => TempoMode::TempoDotted,
            3 => TempoMode::TempoTriplet,
            4 => TempoMode::Keytrack,
            _ => TempoMode::Tempo,
        }
    }

    /// `TempoSelector::valueChanged`: whether the free (seconds) slider
    /// shows rather than the tempo slider.
    pub fn shows_free_slider(self) -> bool {
        self == TempoMode::Seconds
    }
}

/// `TempoSelector`'s popup menu items.
pub const MENU_ITEMS: [(TempoMode, &str); 4] = [
    (TempoMode::Seconds, "Seconds"),
    (TempoMode::Tempo, "Tempo"),
    (TempoMode::TempoDotted, "Tempo Dotted"),
    (TempoMode::TempoTriplet, "Tempo Triplets"),
];

/// Pads an icon to the unit square, as upstream's icon paths do with
/// zero-length line segments.
fn pad_to_unit_square(path: &mut Path, thickness: f32) {
    path.add_line_segment((0.0, 0.0), (0.0, 0.0), thickness);
    path.add_line_segment((1.0, 1.0), (1.0, 1.0), thickness);
}

/// `Paths::clock`.
pub fn clock() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const CLOCK_ANGLE: f32 = 1.0;
        const CLOCK_WIDTH: f32 = 0.4;
        const BUFFER: f32 = (1.0 - CLOCK_WIDTH) / 2.0;
        let mut path = Path::new();
        path.add_pie_segment(
            BUFFER,
            BUFFER,
            CLOCK_WIDTH,
            CLOCK_WIDTH,
            0.0,
            CLOCK_ANGLE - 2.0 * PI,
            0.0,
        );
        pad_to_unit_square(&mut path, 0.2);
        path
    })
}

/// `Paths::note`.
pub fn note() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const LEFT_ADJUSTMENT: f32 = 1.0 / 32.0;
        const NOTE_WIDTH: f32 = 1.0 / 4.0;
        const NOTE_HEIGHT: f32 = 1.0 / 5.0;
        const BAR_HEIGHT: f32 = 2.0 / 5.0;
        const BAR_WIDTH: f32 = 1.0 / 20.0;
        const Y: f32 = 1.0 - (1.0 - BAR_HEIGHT + NOTE_HEIGHT / 2.0) / 2.0;
        const X: f32 = (1.0 - NOTE_WIDTH) / 2.0 - LEFT_ADJUSTMENT;
        let mut path = Path::new();
        path.add_ellipse(X, Y - NOTE_HEIGHT / 2.0, NOTE_WIDTH, NOTE_HEIGHT);
        path.add_rectangle(
            X + NOTE_WIDTH - BAR_WIDTH,
            Y - BAR_HEIGHT,
            BAR_WIDTH,
            BAR_HEIGHT,
        );
        path.close();
        pad_to_unit_square(&mut path, 0.2);
        path
    })
}

/// `Paths::tripletNotes`.
pub fn triplet_notes() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const NOTE_WIDTH: f32 = 1.0 / 5.0;
        const NOTE_HEIGHT: f32 = 1.0 / 6.0;
        const X: f32 = (1.0 - 3.0 * NOTE_WIDTH) / 2.0;
        const BAR_WIDTH: f32 = 1.0 / 20.0;
        const BAR_HEIGHT: f32 = 1.0 / 4.0;
        const Y: f32 = 1.0 - (1.0 - BAR_HEIGHT) / 2.0;
        let mut path = Path::new();
        path.add_rectangle(
            X + NOTE_WIDTH - BAR_WIDTH,
            Y - BAR_HEIGHT - BAR_WIDTH,
            2.0 * NOTE_WIDTH + BAR_WIDTH,
            BAR_WIDTH,
        );
        for i in 0..3 {
            let offset = i as f32 * NOTE_WIDTH;
            path.add_ellipse(X + offset, Y - NOTE_HEIGHT / 2.0, NOTE_WIDTH, NOTE_HEIGHT);
            path.add_rectangle(
                X + NOTE_WIDTH - BAR_WIDTH + offset,
                Y - BAR_HEIGHT,
                BAR_WIDTH,
                BAR_HEIGHT,
            );
        }
        path.close();
        pad_to_unit_square(&mut path, 0.2);
        path
    })
}

/// `TempoSelector::paint` for the sync value `value`.
pub fn paint(pixels: &PixelPainter, scope: &Scope, bounds: Rect, value: f32) {
    let colour = scope.color(ColorId::IconSelectorIcon);
    let mode = TempoMode::from_value(value);
    let (key, path) = match mode {
        TempoMode::Seconds => ("tempo clock", clock()),
        TempoMode::Tempo | TempoMode::TempoDotted => ("tempo note", note()),
        TempoMode::TempoTriplet => ("tempo triplet", triplet_notes()),
        // `Paths::keyboardBordered`: only the LFOs offer keytrack (G4).
        TempoMode::Keytrack => return,
    };
    fill_path_to_fit(pixels, key, path, RectF::from(bounds), colour);
    if mode == TempoMode::TempoDotted {
        let width = bounds.width as f32;
        let dot_width = width / 8.0;
        pixels.fill_ellipse(
            RectF::new(
                bounds.x as f32 + 3.0 * width / 4.0 - dot_width / 2.0,
                bounds.y as f32 + bounds.height as f32 / 2.0,
                dot_width,
                dot_width,
            ),
            colour,
        );
    }
}

/// `SynthSection::drawTempoDivider`: a short line left of the selector.
pub fn paint_divider(pixels: &PixelPainter, scope: &Scope, sync: Rect) {
    const LINE_RATIO: f32 = 0.5;
    let line_height = (sync.height as f32 * LINE_RATIO) as i32;
    let y = sync.y + (sync.height - line_height) / 2;
    pixels.fill_rect(
        Rect::new(sync.x, y, 1, line_height),
        scope.color(ColorId::LightenScreen),
    );
}
