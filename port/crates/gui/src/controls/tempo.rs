// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `TempoSelector`: the small icon beside a rate that picks seconds, tempo,
//! dotted tempo, triplets or keytrack, and with it which rate slider shows
//! (`vital/src/interface/editor_components/tempo_selector.cpp`).

use super::icon::fill_path_to_fit;
use super::paths;
use super::slider::Slider;
use crate::host::Host;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::scope::Scope;
use crate::sections::Panel;
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

    /// `TempoSelector::valueChanged`: the free (seconds) slider shows.
    pub fn shows_free_slider(self) -> bool {
        self == TempoMode::Seconds
    }

    /// `TempoSelector::valueChanged`: the tempo slider shows.
    pub fn shows_tempo_slider(self) -> bool {
        self != TempoMode::Seconds && self != TempoMode::Keytrack
    }

    /// `TempoSelector::valueChanged`: the keytrack transpose and tune
    /// sliders show.
    pub fn shows_keytrack_sliders(self) -> bool {
        self == TempoMode::Keytrack
    }
}

/// `TempoSelector::mouseDown`'s menu items, by value. Keytrack is listed
/// only when the control goes that high (`getMaximum() >= kKeytrack`).
const MENU_NAMES: [&str; 5] = [
    "Seconds",
    "Tempo",
    "Tempo Dotted",
    "Tempo Triplets",
    "Keytrack",
];

/// One `TempoSelector`, bound to its sync control.
#[derive(Clone, Copy, Debug)]
pub struct TempoSelector {
    /// The sync control as a selector: a press opens [`MENU_NAMES`] up to
    /// the control's maximum, and a right-click opens the slider's own menu
    /// (`SynthSlider::mouseDown`).
    pub slider: Slider,
}

impl TempoSelector {
    pub fn new(name: &'static str) -> TempoSelector {
        TempoSelector {
            slider: Slider::selector(name, Some(&MENU_NAMES)),
        }
    }

    pub fn mode(&self, host: &Host) -> TempoMode {
        TempoMode::from_value(host.value(self.slider.name))
    }

    /// Handles the selector's mouse and draws its icon.
    pub fn show(&self, panel: &mut Panel, bounds: Rect) {
        let (value, _) = panel.slider_input(&self.slider, bounds);
        paint(panel.pixels, &panel.scope, bounds, value);
    }
}

/// `TempoSelector::paint` for the sync value `value`.
pub fn paint(pixels: &PixelPainter, scope: &Scope, bounds: Rect, value: f32) {
    let colour = scope.color(ColorId::IconSelectorIcon);
    let mode = TempoMode::from_value(value);
    let (key, path) = match mode {
        TempoMode::Seconds => ("tempo clock", paths::clock()),
        TempoMode::Tempo | TempoMode::TempoDotted => ("tempo note", paths::note()),
        TempoMode::TempoTriplet => ("tempo triplet", paths::triplet_notes()),
        TempoMode::Keytrack => ("tempo keytrack", paths::keyboard_bordered()),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `TempoSelector::valueChanged`'s slider visibility per mode.
    #[test]
    fn modes_pick_their_sliders() {
        let modes = [0.0, 1.0, 2.0, 3.0, 4.0].map(TempoMode::from_value);
        assert_eq!(
            modes.map(|mode| (
                mode.shows_free_slider(),
                mode.shows_tempo_slider(),
                mode.shows_keytrack_sliders()
            )),
            [
                (true, false, false),
                (false, true, false),
                (false, true, false),
                (false, true, false),
                (false, false, true),
            ]
        );
    }

    /// The menu lists modes up to the sync control's maximum: four for the
    /// effects, five (with Keytrack) for the LFOs.
    #[test]
    fn keytrack_is_offered_only_where_the_control_has_it() {
        assert_eq!(
            TempoSelector::new("chorus_sync").slider.selector_options(),
            MENU_NAMES[..4].to_vec()
        );
        assert_eq!(
            TempoSelector::new("lfo_1_sync").slider.selector_options(),
            MENU_NAMES.to_vec()
        );
    }
}
