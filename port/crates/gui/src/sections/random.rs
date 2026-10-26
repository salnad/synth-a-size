// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! A random LFO section: sync and stereo switches, the viewer, the style
//! and the rate. Ports `RandomSection` and `RandomViewer`'s background
//! (`vital/src/interface/editor_sections/random_section.cpp`).
//!
//! The viewer is drawn at rest (it animates only while its source is in
//! use, with the animated views later). The sync selector comes with the
//! shared `TempoSelector`; until then its slot is empty.

use super::lfo::Rate;
use super::Panel;
use crate::controls::button::SynthButton;
use crate::controls::slider::{Slider, SliderStyle};
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::section::{
    draw_label, draw_tempo_divider, draw_text_component_background, keytrack_divider,
    paint_widget_background, place_tempo_controls,
};
use crate::skin::{ColorId, Section, ValueId};

/// The section's skin override (`setSkinOverride(Skin::kRandomLfo)`).
pub const SKIN: &[Section] = &[Section::RandomLfo];

/// `vital::kNumRandomLfos`.
pub const NUM_RANDOM_LFOS: usize = 4;

/// `strings::kRandomNames`: the style list.
pub const RANDOM_NAMES: [&str; 4] = [
    "Perlin",
    "Sample & Hold",
    "Sine Interpolate",
    "Lorenz Attractor",
];

/// The parameter names of random LFO `index` (0-based).
#[derive(Clone, Copy, Debug)]
pub struct Names {
    pub frequency: &'static str,
    pub tempo: &'static str,
    pub keytrack_transpose: &'static str,
    pub keytrack_tune: &'static str,
    pub sync: &'static str,
    pub style: &'static str,
    pub stereo: &'static str,
    pub sync_type: &'static str,
}

macro_rules! names {
    ($($n:literal),*) => {
        [$(Names {
            frequency: concat!("random_", $n, "_frequency"),
            tempo: concat!("random_", $n, "_tempo"),
            keytrack_transpose: concat!("random_", $n, "_keytrack_transpose"),
            keytrack_tune: concat!("random_", $n, "_keytrack_tune"),
            sync: concat!("random_", $n, "_sync"),
            style: concat!("random_", $n, "_style"),
            stereo: concat!("random_", $n, "_stereo"),
            sync_type: concat!("random_", $n, "_sync_type"),
        }),*]
    };
}

/// Each random LFO's parameter names.
pub const NAMES: [Names; NUM_RANDOM_LFOS] = names!("1", "2", "3", "4");

/// `RandomSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RandomLayout {
    pub bounds: Rect,
    pub sync_type: Rect,
    pub stereo: Rect,
    pub viewer: Rect,
    pub style: Rect,
    pub rate: Rect,
    pub sync: Rect,
    pub keytrack_transpose: Rect,
    pub keytrack_tune: Rect,
    pub divider: Rect,
}

impl RandomLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> RandomLayout {
        let (width, height) = (bounds.width, bounds.height);
        let knob_section_height = scope.knob_section_height() as i32;
        let text_button_height = scope.value(ValueId::TextButtonHeight) as i32;
        let widget_margin = scope.widget_margin() as i32;
        let button_width = (width - 3 * widget_margin) / 2;
        let sync_type = Rect::new(
            widget_margin,
            widget_margin,
            button_width,
            text_button_height,
        );
        let stereo_x = sync_type.right() + widget_margin;
        let stereo = Rect::new(
            stereo_x,
            widget_margin,
            width - stereo_x - widget_margin,
            text_button_height,
        );
        let viewer_y = text_button_height + 2 * widget_margin;
        let viewer = Rect::new(
            widget_margin,
            viewer_y,
            width - 2 * widget_margin,
            height - knob_section_height - viewer_y,
        );
        let component_width = (width - 3 * widget_margin) / 2;
        let control_y = height - knob_section_height + widget_margin;
        let style = Rect::new(
            widget_margin,
            control_y,
            component_width,
            knob_section_height - 2 * widget_margin,
        );
        let frequency_x = style.right() + widget_margin;
        let (rate, sync, modulation_area) = place_tempo_controls(
            scope,
            frequency_x,
            control_y,
            width - widget_margin - frequency_x,
            knob_section_height - 2 * widget_margin,
        );
        let divider = keytrack_divider(rate, modulation_area);
        let keytrack_transpose = Rect::new(rate.x, rate.y, rate.width / 2, rate.height);
        let keytrack_tune = Rect::new(
            keytrack_transpose.right(),
            rate.y,
            rate.right() - keytrack_transpose.right(),
            rate.height,
        );
        let at = |r: Rect| r.translated(bounds.x, bounds.y);
        RandomLayout {
            bounds,
            sync_type: at(sync_type),
            stereo: at(stereo),
            viewer: at(viewer),
            style: at(style),
            rate: at(rate),
            sync: at(sync),
            keytrack_transpose: at(keytrack_transpose),
            keytrack_tune: at(keytrack_tune),
            divider: at(divider),
        }
    }
}

/// One random LFO's controls (`RandomSection`'s constructor).
pub struct RandomSection {
    names: Names,
    frequency: Slider,
    tempo: Slider,
    keytrack_transpose: Slider,
    keytrack_tune: Slider,
    style: Slider,
    stereo: SynthButton,
    sync_type: SynthButton,
}

impl RandomSection {
    /// Random LFO `index` (0-based).
    pub fn new(index: usize) -> RandomSection {
        /// `RandomSection`'s `kTempoDragSensitivity`.
        const TEMPO_DRAG_SENSITIVITY: f32 = 0.3;
        /// `SynthSection::kTransposeMouseSensitivity`.
        const TRANSPOSE_MOUSE_SENSITIVITY: f32 = 0.2;
        let names = NAMES[index];
        RandomSection {
            names,
            frequency: Slider::new(names.frequency, SliderStyle::Text),
            tempo: Slider::new(names.tempo, SliderStyle::Text).sensitivity(TEMPO_DRAG_SENSITIVITY),
            keytrack_transpose: Slider::new(names.keytrack_transpose, SliderStyle::Text)
                .sensitivity(TRANSPOSE_MOUSE_SENSITIVITY)
                .bipolar(),
            keytrack_tune: Slider::new(names.keytrack_tune, SliderStyle::Text)
                .bipolar()
                .display(3, 0),
            style: Slider::selector(names.style, Some(&RANDOM_NAMES)),
            stereo: SynthButton::text(names.stereo, "STEREO"),
            sync_type: SynthButton::text(names.sync_type, "SYNC"),
        }
    }

    /// `RandomSection::paintBackground`, with the viewer's background.
    pub fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, layout: &RandomLayout) {
        let widget_margin = scope.widget_margin() as i32;
        let frequency_right = layout.bounds.right() - widget_margin;
        let frequency = Rect::new(
            layout.rate.x,
            layout.rate.y,
            frequency_right - layout.rate.x,
            layout.rate.height,
        );
        draw_text_component_background(pixels, scope, layout.style, true);
        draw_text_component_background(pixels, scope, frequency, true);
        draw_tempo_divider(pixels, scope, layout.sync);
        draw_label(pixels, scope, "STYLE", layout.style, true);
        draw_label(pixels, scope, "FREQUENCY", frequency, true);

        // RandomViewer::paintBackground: the backdrop and a centre line.
        let viewer = layout.viewer;
        paint_widget_background(pixels, scope, viewer);
        pixels.fill_rect(
            Rect::new(viewer.x, viewer.y + viewer.height / 2, viewer.width, 1),
            scope.color(ColorId::WidgetPrimaryDisabled),
        );
    }

    pub fn show(&self, panel: &mut Panel, layout: &RandomLayout) {
        let host = panel.host;
        match Rate::from_sync(host.value(self.names.sync)) {
            Rate::Free => panel.slider(&self.frequency, layout.rate),
            Rate::Tempo => panel.slider(&self.tempo, layout.rate),
            Rate::Keytrack => {
                panel.slider(&self.keytrack_transpose, layout.keytrack_transpose);
                panel.slider(&self.keytrack_tune, layout.keytrack_tune);
                let colour = panel.scope.color(ColorId::LightenScreen);
                panel.quad(Quad::new(QuadKind::Color, layout.divider.into(), colour));
            }
        }
        panel.slider(&self.style, layout.style);
        panel.button(&self.sync_type, layout.sync_type);
        panel.button(&self.stereo, layout.stereo);
        panel.widget_corners(layout.viewer);
    }
}
