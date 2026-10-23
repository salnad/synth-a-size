// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Chorus section. Ports `ChorusSection`
//! (`vital/src/interface/editor_sections/chorus_section.cpp`).

use super::{divided_area_buffered, place_tempo_controls, with_left, with_top};
use crate::layout::Rect;
use crate::scope::Scope;
use crate::section::place_knobs_in_area;

/// `ChorusSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChorusLayout {
    pub bounds: Rect,
    pub on: Rect,
    pub voices: Rect,
    /// `chorus_frequency` and `chorus_tempo` share these bounds.
    pub frequency: Rect,
    pub sync: Rect,
    pub mod_depth: Rect,
    pub delay_1: Rect,
    pub delay_2: Rect,
    pub viewer: Rect,
    pub filter_viewer: Rect,
    pub feedback: Rect,
    pub dry_wet: Rect,
    pub cutoff: Rect,
    pub spread: Rect,
}

impl ChorusLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> ChorusLayout {
        let widget_margin = scope.widget_margin() as i32;
        let title = crate::section::title_width(scope) as i32;
        let section_height = scope.knob_section_height() as i32;
        let (width, height) = (bounds.width, bounds.height);

        let area = with_left(Rect::new(0, 0, width, height), title);
        let delay_area = divided_area_buffered(area, 3, 0, widget_margin);
        let knobs_area = divided_area_buffered(area, 3, 2, widget_margin);

        let top = place_knobs_in_area(
            scope,
            Rect::new(delay_area.x, 0, delay_area.width, section_height),
            2,
        );
        let voices = with_top(top[0], widget_margin);
        let (frequency, sync) = place_tempo_controls(
            scope,
            top[1].x,
            widget_margin,
            top[1].width,
            section_height - 2 * widget_margin,
        );

        let delay_y = section_height - widget_margin;
        let delays = place_knobs_in_area(
            scope,
            Rect::new(delay_area.x, delay_y, delay_area.width, section_height),
            3,
        );

        let widget_x = delays[2].right() + widget_margin;
        let viewer_width = knobs_area.x - widget_x;
        let delay_height = (height - 3 * widget_margin) / 2;
        let filter_y = delay_height + 2 * widget_margin;
        let viewer = Rect::new(widget_x, widget_margin, viewer_width, delay_height);
        let filter_viewer = Rect::new(
            widget_x,
            filter_y,
            viewer_width,
            height - filter_y - widget_margin,
        );

        let knobs = place_knobs_in_area(
            scope,
            Rect::new(knobs_area.x, 0, knobs_area.width, section_height),
            2,
        );
        let knobs2 = place_knobs_in_area(
            scope,
            Rect::new(
                knobs_area.x,
                section_height - widget_margin,
                knobs_area.width,
                section_height,
            ),
            2,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        ChorusLayout {
            bounds,
            on: Rect::new(dx, dy, title, title),
            voices: voices.translated(dx, dy),
            frequency: frequency.translated(dx, dy),
            sync: sync.translated(dx, dy),
            mod_depth: delays[0].translated(dx, dy),
            delay_1: delays[1].translated(dx, dy),
            delay_2: delays[2].translated(dx, dy),
            viewer: viewer.translated(dx, dy),
            filter_viewer: filter_viewer.translated(dx, dy),
            feedback: knobs[0].translated(dx, dy),
            dry_wet: knobs[1].translated(dx, dy),
            cutoff: knobs2[0].translated(dx, dy),
            spread: knobs2[1].translated(dx, dy),
        }
    }
}

use egui::Ui;

use super::{frequency_bounds, paint_section_background, viewer_corners, Effect};
use crate::controls::slider::{Slider, SliderStyle};
use crate::controls::tempo::{self, TempoMode};
use crate::host::Host;
use crate::paint::PixelPainter;
use crate::section::{draw_label, draw_text_component_background};
use crate::sections::{Overlay, Panel};

/// The section's controls (`ChorusSection`'s constructor).
pub struct ChorusSection {
    voices: Slider,
    delay_1: Slider,
    delay_2: Slider,
    mod_depth: Slider,
    frequency: Slider,
    tempo: Slider,
    feedback: Slider,
    dry_wet: Slider,
    cutoff: Slider,
    spread: Slider,
}

impl Default for ChorusSection {
    fn default() -> ChorusSection {
        /// `kTempoDragSensitivity`.
        const TEMPO_DRAG_SENSITIVITY: f32 = 0.5;
        ChorusSection {
            voices: Slider::new("chorus_voices", SliderStyle::Text)
                .sensitivity(TEMPO_DRAG_SENSITIVITY),
            delay_1: Slider::new("chorus_delay_1", SliderStyle::Rotary),
            delay_2: Slider::new("chorus_delay_2", SliderStyle::Rotary),
            mod_depth: Slider::new("chorus_mod_depth", SliderStyle::Rotary),
            frequency: Slider::new("chorus_frequency", SliderStyle::Text),
            tempo: Slider::new("chorus_tempo", SliderStyle::Text)
                .sensitivity(TEMPO_DRAG_SENSITIVITY),
            feedback: Slider::new("chorus_feedback", SliderStyle::Rotary).bipolar(),
            dry_wet: Slider::new("chorus_dry_wet", SliderStyle::Rotary),
            cutoff: Slider::new("chorus_cutoff", SliderStyle::Rotary),
            spread: Slider::new("chorus_spread", SliderStyle::Rotary),
        }
    }
}

impl ChorusSection {
    /// `ChorusSection::paintBackground`, then its controls.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        scope: &Scope,
        overlay: &mut Overlay,
        bounds: Rect,
        clip: Rect,
    ) {
        let l = ChorusLayout::new(scope, bounds);
        let rotaries = [
            (&self.delay_1, l.delay_1),
            (&self.delay_2, l.delay_2),
            (&self.mod_depth, l.mod_depth),
            (&self.feedback, l.feedback),
            (&self.dry_wet, l.dry_wet),
            (&self.cutoff, l.cutoff),
            (&self.spread, l.spread),
        ];
        paint_section_background(
            pixels,
            scope,
            Effect::Chorus,
            bounds,
            &rotaries,
            &[l.viewer, l.filter_viewer],
        );
        let frequency = frequency_bounds(l.frequency, l.sync);
        draw_text_component_background(pixels, scope, frequency, true);
        draw_text_component_background(pixels, scope, l.voices, true);
        draw_label(pixels, scope, "FREQUENCY", frequency, true);
        draw_label(pixels, scope, "VOICES", l.voices, true);
        for (text, rect) in [
            ("FEEDBACK", l.feedback),
            ("MIX", l.dry_wet),
            ("DEPTH", l.mod_depth),
            ("DELAY 1", l.delay_1),
            ("DELAY 2", l.delay_2),
            ("CUTOFF", l.cutoff),
            ("SPREAD", l.spread),
        ] {
            draw_label(pixels, scope, text, rect, false);
        }
        tempo::paint_divider(pixels, scope, l.sync);

        let mut panel = Panel::new(ui, host, pixels, *scope, overlay, clip);
        let sync = host.value("chorus_sync");
        let rate = if TempoMode::from_value(sync).shows_free_slider() {
            &self.frequency
        } else {
            &self.tempo
        };
        panel.slider(rate, l.frequency);
        panel.slider(&self.voices, l.voices);
        for (slider, rect) in rotaries {
            panel.slider(slider, rect);
        }
        tempo::paint(pixels, scope, l.sync, sync);
        viewer_corners(&mut panel, l.viewer);
        viewer_corners(&mut panel, l.filter_viewer);
        panel.finish();
    }
}
