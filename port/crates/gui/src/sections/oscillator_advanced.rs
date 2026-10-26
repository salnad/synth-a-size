// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Each oscillator's advanced section on the Advanced tab: its options
//! (note tracking, high-resolution wavetables) and its unison settings with
//! the spread display. Ports `OscillatorAdvancedSection`, `OscillatorOptions`,
//! `OscillatorUnison` and `SpreadVisualizer`
//! (`vital/src/interface/editor_sections/oscillator_advanced_section.cpp`).

use sas_engine::sas_dsp_core::sas_simd::PolyFloat;
use sas_engine::telemetry::{destination_id, TelemetrySnapshot};

use super::Panel;
use crate::controls::bars::BarRenderer;
use crate::controls::button::SynthButton;
use crate::controls::slider::{Slider, SliderStyle};
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::section::{
    draw_label, draw_text_component_background, paint_border, paint_container,
    paint_sideways_heading_text, paint_tab_shadow, place_knobs_in_area, title_width, Heading,
};
use crate::skin::{ColorId, ValueId};

/// `SynthOscillator::kMaxUnison`.
const MAX_UNISON: usize = 16;
/// `SpreadVisualizer::kNumSpreads`: frame, spectral morph and distortion.
const NUM_SPREADS: usize = 3;
/// `vital::kNumOscillatorWaveFrames`.
const NUM_OSCILLATOR_WAVE_FRAMES: f32 = 257.0;

/// `strings::kUnisonStackNames`.
const UNISON_STACK_NAMES: &[&str] = &[
    "Unison",
    "Center Drop 12",
    "Center Drop 24",
    "Octave",
    "2x Octave",
    "Power Chord",
    "2x Power Chord",
    "Major Chord",
    "Minor Chord",
    "Harmonics",
    "Odd Harmonics",
];

/// One oscillator's control names.
#[derive(Clone, Copy, Debug)]
pub struct OscillatorNames {
    pub on: &'static str,
    pub midi_track: &'static str,
    pub smooth_interpolation: &'static str,
    pub stack_style: &'static str,
    pub unison_blend: &'static str,
    pub detune_range: &'static str,
    pub stereo_spread: &'static str,
    pub frame_spread: &'static str,
    pub spectral_morph_spread: &'static str,
    pub distortion_spread: &'static str,
    pub unison_voices: &'static str,
    pub wave_frame: &'static str,
    pub spectral_morph_amount: &'static str,
    pub distortion_amount: &'static str,
}

macro_rules! oscillator_names {
    ($n:literal) => {
        OscillatorNames {
            on: concat!("osc_", $n, "_on"),
            midi_track: concat!("osc_", $n, "_midi_track"),
            smooth_interpolation: concat!("osc_", $n, "_smooth_interpolation"),
            stack_style: concat!("osc_", $n, "_stack_style"),
            unison_blend: concat!("osc_", $n, "_unison_blend"),
            detune_range: concat!("osc_", $n, "_detune_range"),
            stereo_spread: concat!("osc_", $n, "_stereo_spread"),
            frame_spread: concat!("osc_", $n, "_frame_spread"),
            spectral_morph_spread: concat!("osc_", $n, "_spectral_morph_spread"),
            distortion_spread: concat!("osc_", $n, "_distortion_spread"),
            unison_voices: concat!("osc_", $n, "_unison_voices"),
            wave_frame: concat!("osc_", $n, "_wave_frame"),
            spectral_morph_amount: concat!("osc_", $n, "_spectral_morph_amount"),
            distortion_amount: concat!("osc_", $n, "_distortion_amount"),
        }
    };
}

/// Each oscillator's names, `OSC 1` to `OSC 3`.
pub const NAMES: [OscillatorNames; 3] = [
    oscillator_names!("1"),
    oscillator_names!("2"),
    oscillator_names!("3"),
];

/// `OscillatorAdvancedSection::resized` with its two subsections'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OscillatorAdvancedLayout {
    pub bounds: Rect,
    pub options: Rect,
    pub unison: Rect,
    pub midi_track: Rect,
    pub smooth_interpolation: Rect,
    pub stack_style: Rect,
    pub detune_range: Rect,
    pub unison_blend: Rect,
    pub stereo_spread: Rect,
    pub frame_spread: Rect,
    pub spectral_morph_spread: Rect,
    pub distortion_spread: Rect,
    pub spread_visualizer: Rect,
}

impl OscillatorAdvancedLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> OscillatorAdvancedLayout {
        let padding = scope.padding() as i32;
        let options_width = (bounds.width as f32 * 0.22) as i32;
        let options = Rect::new(0, 0, options_width, bounds.height);
        let unison_x = options.right() + padding;
        let unison = Rect::new(unison_x, 0, bounds.width - unison_x, bounds.height);

        let title = title_width(scope) as i32;
        let widget_margin = scope.widget_margin() as i32;
        let section_height = scope.knob_section_height() as i32;

        // OscillatorOptions::resized.
        let x = title + widget_margin;
        let width = options.width - x - widget_margin;
        let midi_track = Rect::new(x, widget_margin, width, section_height - 2 * widget_margin);
        let smooth_y = midi_track.bottom() + widget_margin;
        let smooth_interpolation = Rect::new(
            x,
            smooth_y,
            width,
            options.height - smooth_y - widget_margin,
        );

        // OscillatorUnison::resized.
        const TEXT_COMPONENT_WIDTH_RATIO: f32 = 0.23;
        let text_width =
            (unison.width as f32 * TEXT_COMPONENT_WIDTH_RATIO - 2.0 * widget_margin as f32) as i32;
        let text_height = section_height - 2 * widget_margin;
        let stack_style = Rect::new(
            title + widget_margin,
            widget_margin,
            text_width,
            text_height,
        );
        let detune_range = Rect::new(
            title + widget_margin,
            section_height,
            text_width,
            text_height,
        );
        let controls_x = detune_range.right();
        let row1 = place_knobs_in_area(
            scope,
            Rect::new(controls_x, 0, unison.width - controls_x, section_height),
            4,
        );
        let knob_y2 = unison.height - section_height;
        let row2 = place_knobs_in_area(
            scope,
            Rect::new(
                controls_x,
                knob_y2,
                unison.width - controls_x,
                section_height,
            ),
            4,
        );
        let (stereo_spread, frame_spread, spectral_morph_spread, distortion_spread) =
            (row2[0], row2[1], row2[2], row2[3]);
        let spread_visualizer = Rect::new(
            frame_spread.x,
            widget_margin,
            distortion_spread.right() - frame_spread.x,
            text_height,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        let in_unison = |rect: Rect| rect.translated(dx + unison.x, dy);
        OscillatorAdvancedLayout {
            bounds,
            options: options.translated(dx, dy),
            unison: unison.translated(dx, dy),
            midi_track: midi_track.translated(dx, dy),
            smooth_interpolation: smooth_interpolation.translated(dx, dy),
            stack_style: in_unison(stack_style),
            detune_range: in_unison(detune_range),
            unison_blend: in_unison(row1[0]),
            stereo_spread: in_unison(stereo_spread),
            frame_spread: in_unison(frame_spread),
            spectral_morph_spread: in_unison(spectral_morph_spread),
            distortion_spread: in_unison(distortion_spread),
            spread_visualizer: in_unison(spread_visualizer),
        }
    }
}

/// One oscillator's advanced controls.
pub struct OscillatorAdvancedSection {
    index: usize,
    names: OscillatorNames,
    midi_track: SynthButton,
    smooth_interpolation: SynthButton,
    stack_style: Slider,
    unison_blend: Slider,
    detune_range: Slider,
    stereo_spread: Slider,
    frame_spread: Slider,
    spectral_morph_spread: Slider,
    distortion_spread: Slider,
}

impl OscillatorAdvancedSection {
    /// Oscillator `index` (0 to 2).
    pub fn new(index: usize) -> OscillatorAdvancedSection {
        let names = NAMES[index];
        let rotary = |name| Slider::new(name, SliderStyle::Rotary);
        OscillatorAdvancedSection {
            index,
            names,
            midi_track: SynthButton::text(names.midi_track, "NOTE TRACK"),
            smooth_interpolation: SynthButton::text(names.smooth_interpolation, "HI-RES WAVETABLE"),
            stack_style: Slider::selector(names.stack_style, Some(UNISON_STACK_NAMES)),
            unison_blend: rotary(names.unison_blend),
            detune_range: Slider::new(names.detune_range, SliderStyle::Text),
            stereo_spread: rotary(names.stereo_spread),
            frame_spread: rotary(names.frame_spread).bipolar(),
            spectral_morph_spread: rotary(names.spectral_morph_spread).bipolar(),
            distortion_spread: rotary(names.distortion_spread).bipolar(),
        }
    }

    /// Both subsections: backgrounds, controls, the spread display and, for
    /// an inactive subsection, its off overlay. The options are active while
    /// the oscillator is on; the unison while it is on with more than one
    /// voice (`checkActive`).
    pub fn show(&self, panel: &mut Panel, l: &OscillatorAdvancedLayout) {
        let (pixels, scope, host) = (panel.pixels, panel.scope, panel.host);
        let on = host.value(self.names.on) != 0.0;
        let unison_active = on && host.value(self.names.unison_voices) > 1.0;
        let number = self.index + 1;

        // OscillatorOptions.
        if on {
            paint_tab_shadow(pixels, &scope, l.options);
        }
        paint_subsection(pixels, &scope, &format!("OSC {number} OPTIONS"), l.options);
        let rounding = scope.value(ValueId::LabelBackgroundRounding);
        let text_background = scope.color(ColorId::TextComponentBackground);
        pixels.fill_rounded_rect(l.midi_track, rounding, text_background);
        pixels.fill_rounded_rect(l.smooth_interpolation, rounding, text_background);
        panel.button(&self.midi_track, l.midi_track);
        panel.button(&self.smooth_interpolation, l.smooth_interpolation);
        if !on {
            panel.off_overlay(l.options);
        }

        // OscillatorUnison.
        if unison_active {
            paint_tab_shadow(pixels, &scope, l.unison);
        }
        paint_subsection(pixels, &scope, &format!("OSC {number} UNISON"), l.unison);
        let active = |slider: &Slider| Slider {
            active: unison_active,
            ..*slider
        };
        for (slider, bounds) in [
            (&self.unison_blend, l.unison_blend),
            (&self.stereo_spread, l.stereo_spread),
            (&self.frame_spread, l.frame_spread),
            (&self.spectral_morph_spread, l.spectral_morph_spread),
            (&self.distortion_spread, l.distortion_spread),
        ] {
            active(slider).paint_shadow(pixels, &scope, bounds);
        }
        draw_text_component_background(pixels, &scope, l.stack_style, true);
        draw_text_component_background(pixels, &scope, l.detune_range, true);
        for (text, bounds, text_component) in [
            ("STACK", l.stack_style, true),
            ("DETUNE RANGE", l.detune_range, true),
            ("UNISON BLEND", l.unison_blend, false),
            ("STEREO UNISON", l.stereo_spread, false),
            ("TABLE SPREAD", l.frame_spread, false),
            ("DIST SPREAD", l.distortion_spread, false),
            ("SPECT SPREAD", l.spectral_morph_spread, false),
        ] {
            draw_label(pixels, &scope, text, bounds, text_component);
        }
        // Upstream never lays out its SPECTRAL UNISON button (0 by 0), so
        // it isn't shown.
        self.paint_spread(pixels, &scope, panel, l.spread_visualizer);
        for (slider, bounds) in [
            (&self.stack_style, l.stack_style),
            (&self.detune_range, l.detune_range),
            (&self.unison_blend, l.unison_blend),
            (&self.stereo_spread, l.stereo_spread),
            (&self.frame_spread, l.frame_spread),
            (&self.spectral_morph_spread, l.spectral_morph_spread),
            (&self.distortion_spread, l.distortion_spread),
        ] {
            panel.slider(&active(slider), bounds);
        }
        if !unison_active {
            panel.off_overlay(l.unison);
        }
    }

    /// `SpreadVisualizer::paintBackground` and `render`: a bar per unison
    /// voice pair in each of three rows, at the voice's wave frame, spectral
    /// morph and distortion amounts with their spreads.
    fn paint_spread(&self, pixels: &PixelPainter, scope: &Scope, panel: &Panel, bounds: Rect) {
        pixels.fill_rounded_rect(
            bounds,
            scope.value(ValueId::WidgetRoundedCorner),
            scope.color(ColorId::WidgetBackground),
        );
        let host = panel.host;
        let names = self.names;
        let telemetry = host.model().telemetry();
        let totals = |snapshot: Option<&TelemetrySnapshot>| {
            // getOutputsTotal: mono and poly modulated values while the
            // processor runs, else the control's.
            let total = |name: &str| -> PolyFloat {
                let default = PolyFloat::splat(host.value(name));
                let (Some(snapshot), Some(id)) = (snapshot, destination_id(name)) else {
                    return default;
                };
                let total = snapshot.modulation_total(id);
                if !total.enabled {
                    return default;
                }
                let mono = PolyFloat::from_array(total.mono);
                match total.poly {
                    Some(poly) => mono + PolyFloat::from_array(poly),
                    None => mono,
                }
            };
            [
                total(names.unison_voices),
                total(names.wave_frame),
                total(names.spectral_morph_amount),
                total(names.distortion_amount),
                total(names.frame_spread),
                total(names.spectral_morph_spread),
                total(names.distortion_spread),
            ]
        };
        let [voices, frame, morph, distortion, frame_spread, morph_spread, distortion_spread] =
            match &telemetry {
                Some(telemetry) => telemetry.read(|snapshot| totals(Some(snapshot))),
                None => totals(None),
            };

        let mut voices = (voices.get(0) as i32).max(0) as usize;
        if voices <= 2 {
            return;
        }
        let (width, height) = (bounds.width as f32, bounds.height as f32);
        let mut bars = BarRenderer::new(NUM_SPREADS * MAX_UNISON, true);
        bars.additive = false;
        bars.color = scope.color(ColorId::WidgetSecondary1);
        bars.bar_width = 2.0 / width;

        let widget_margin = scope.widget_margin();
        let height_buffer = 2.0 * widget_margin / height;
        let row_height = (2.0 - height_buffer) / NUM_SPREADS as f32 - height_buffer;
        let mut y = height_buffer - 1.0;
        for row in 0..NUM_SPREADS {
            for i in 0..MAX_UNISON {
                bars.set_bottom(row * MAX_UNISON + i, y + row_height);
                bars.set_y(row * MAX_UNISON + i, y);
            }
            y += row_height + height_buffer;
        }

        let buffer = 2.0 * widget_margin / width;
        let mult = 2.0 - 2.0 * buffer;
        let offset = -1.0 + buffer - 1.0 / width;
        let frame_scale = 1.0 / (NUM_OSCILLATOR_WAVE_FRAMES - 1.0);
        voices += voices % 2;
        let clamp =
            |value: PolyFloat| PolyFloat::from_array(value.to_array().map(|v| v.clamp(0.0, 1.0)));
        for i in (0..voices.min(MAX_UNISON)).step_by(2) {
            let t = PolyFloat::splat(2.0 * i as f32 / (voices as f32 - 2.0));
            let voice_frame = clamp((frame + frame_spread * t) * PolyFloat::splat(frame_scale));
            let voice_morph = clamp(morph + morph_spread * t);
            let voice_distortion = clamp(distortion + distortion_spread * t);
            for (row, value) in [voice_frame, voice_morph, voice_distortion]
                .iter()
                .enumerate()
            {
                bars.set_x(row * MAX_UNISON + i, value.get(0) * mult + offset);
                bars.set_x(row * MAX_UNISON + i + 1, value.get(1) * mult + offset);
            }
        }
        for row in 0..NUM_SPREADS {
            for i in voices..MAX_UNISON {
                bars.set_x(row * MAX_UNISON + i, -2.0);
            }
        }
        bars.paint(pixels, bounds.into());
    }
}

/// `SynthSection::paintBackground` for a subsection with a sideways
/// heading: the container, its name up the left edge, and the border.
fn paint_subsection(pixels: &PixelPainter, scope: &Scope, name: &str, bounds: Rect) {
    paint_container(pixels, scope, bounds, Heading::Sideways);
    paint_sideways_heading_text(pixels, scope, name, bounds, false);
    paint_border(pixels, scope, bounds);
}
