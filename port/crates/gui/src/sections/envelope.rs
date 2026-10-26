// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! An envelope section: delay, attack, hold, decay, sustain and release
//! under the envelope editor. Ports `EnvelopeSection`
//! (`vital/src/interface/editor_sections/envelope_section.cpp`).
//!
//! The editor itself (`EnvelopeEditor`) and its magnifying glass come
//! later; its backdrop is drawn here.

use super::Panel;
use crate::controls::slider::{Slider, SliderStyle};
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::section::{draw_label, paint_widget_background, place_knobs_in_area};
use crate::skin::Section;

/// The section's skin override (`setSkinOverride(Skin::kEnvelope)`).
pub const SKIN: &[Section] = &[Section::Envelope];

/// `vital::kNumEnvelopes`.
pub const NUM_ENVELOPES: usize = 6;

/// `EnvelopeSection::resized`'s `kMagnifyingHeightRatio`.
const MAGNIFYING_HEIGHT_RATIO: f32 = 0.2;

/// The knobs, left to right, and their labels.
const LABELS: [&str; 6] = ["DELAY", "ATTACK", "HOLD", "DECAY", "SUSTAIN", "RELEASE"];

/// `EnvelopeSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvelopeLayout {
    pub bounds: Rect,
    /// The envelope editor.
    pub editor: Rect,
    /// Delay, attack, hold, decay, sustain, release.
    pub knobs: [Rect; 6],
    pub magnifying_glass: Rect,
}

impl EnvelopeLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> EnvelopeLayout {
        let knob_section_height = scope.knob_section_height() as i32;
        let knob_y = bounds.height - knob_section_height;
        let widget_margin = scope.widget_margin() as i32;
        let editor = Rect::new(
            widget_margin,
            widget_margin,
            bounds.width - 2 * widget_margin,
            knob_y - widget_margin,
        );
        let knobs = place_knobs_in_area(
            scope,
            Rect::new(0, knob_y, bounds.width, knob_section_height),
            6,
        );
        let magnify_height = (editor.height as f32 * MAGNIFYING_HEIGHT_RATIO) as i32;
        let magnifying_glass = Rect::new(
            editor.right() - magnify_height,
            editor.y,
            magnify_height,
            magnify_height,
        );
        let at = |r: Rect| r.translated(bounds.x, bounds.y);
        EnvelopeLayout {
            bounds,
            editor: at(editor),
            knobs: std::array::from_fn(|i| at(knobs[i])),
            magnifying_glass: at(magnifying_glass),
        }
    }
}

macro_rules! envelope_names {
    ($($n:literal),*) => {
        [$([
            concat!("env_", $n, "_delay"),
            concat!("env_", $n, "_attack"),
            concat!("env_", $n, "_hold"),
            concat!("env_", $n, "_decay"),
            concat!("env_", $n, "_sustain"),
            concat!("env_", $n, "_release"),
        ]),*]
    };
}

/// Each envelope's knob parameters, in `LABELS` order.
const NAMES: [[&str; 6]; NUM_ENVELOPES] = envelope_names!("1", "2", "3", "4", "5", "6");

/// One envelope's controls (`EnvelopeSection`'s constructor).
pub struct EnvelopeSection {
    knobs: [Slider; 6],
}

impl EnvelopeSection {
    /// Envelope `index` (0-based): `env_N_delay` and so on.
    pub fn new(index: usize) -> EnvelopeSection {
        EnvelopeSection {
            knobs: NAMES[index].map(|name| Slider::new(name, SliderStyle::Rotary)),
        }
    }

    /// `EnvelopeSection::paintBackground`.
    pub fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, layout: &EnvelopeLayout) {
        for (label, bounds) in LABELS.iter().zip(layout.knobs) {
            draw_label(pixels, scope, label, bounds, false);
        }
        for (knob, bounds) in self.knobs.iter().zip(layout.knobs) {
            knob.paint_shadow(pixels, scope, bounds);
        }
        paint_widget_background(pixels, scope, layout.editor);
    }

    pub fn show(&self, panel: &mut Panel, layout: &EnvelopeLayout) {
        for (knob, bounds) in self.knobs.iter().zip(layout.knobs) {
            panel.slider(knob, bounds);
        }
        panel.widget_corners(layout.editor);
    }
}
