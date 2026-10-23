// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Delay section. Ports `DelaySection`
//! (`vital/src/interface/editor_sections/delay_section.cpp`).

use super::{divided_area_buffered, divided_area_unbuffered, place_tempo_controls, with_left};
use crate::layout::Rect;
use crate::scope::Scope;
use crate::section::place_knobs_in_area;

/// `vital::StereoDelay::kMono`: one delay time.
pub const STYLE_MONO: usize = 0;

/// `DelaySection::resized` and `resizeTempoControls`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelayLayout {
    pub bounds: Rect,
    pub on: Rect,
    /// `delay_frequency` and `delay_tempo` share these bounds.
    pub frequency: Rect,
    pub sync: Rect,
    /// The second delay time; empty for the mono style.
    pub aux_frequency: Rect,
    pub aux_sync: Rect,
    pub style: Rect,
    pub viewer: Rect,
    pub filter_viewer: Rect,
    pub feedback: Rect,
    pub dry_wet: Rect,
    pub filter_cutoff: Rect,
    pub filter_spread: Rect,
}

impl DelayLayout {
    /// `style` is the `delay_style` value.
    pub fn new(scope: &Scope, bounds: Rect, style: usize) -> DelayLayout {
        let title = crate::section::title_width(scope) as i32;
        let knob_section_height = scope.knob_section_height() as i32;
        let widget_margin = scope.widget_margin() as i32;
        let (width, height) = (bounds.width, bounds.height);
        let area = with_left(Rect::new(0, 0, width, height), title);
        let knobs_area = divided_area_buffered(area, 3, 2, widget_margin);

        let knob_y2 = height - knob_section_height;
        let text_area = divided_area_unbuffered(area, 3, 0, widget_margin);
        let style_bounds = Rect::new(
            text_area.x,
            knob_y2 + widget_margin,
            text_area.width,
            knob_section_height - 2 * widget_margin,
        );

        let widget_x = text_area.right() + widget_margin;
        let viewer_width = knobs_area.x - widget_x;
        let delay_height = (height - 3 * widget_margin) / 2;
        let viewer = Rect::new(widget_x, widget_margin, viewer_width, delay_height);
        let filter_y = viewer.bottom() + widget_margin;
        let filter_viewer = Rect::new(
            widget_x,
            filter_y,
            viewer_width,
            height - filter_y - widget_margin,
        );

        // knobs_area.withBottom(knob_section_height): the same top.
        let top_knobs = Rect::new(
            knobs_area.x,
            knobs_area.y,
            knobs_area.width,
            knob_section_height,
        );
        let knobs = place_knobs_in_area(scope, top_knobs, 2);
        let bottom_knobs = Rect::new(knobs_area.x, knob_y2, knobs_area.width, knob_section_height);
        let knobs2 = place_knobs_in_area(scope, bottom_knobs, 2);

        // resizeTempoControls.
        let (text_x, text_width, text_height) =
            (style_bounds.x, style_bounds.width, style_bounds.height);
        let (frequency, sync, aux_frequency, aux_sync) = if style == STYLE_MONO {
            let (frequency, sync) =
                place_tempo_controls(scope, text_x, widget_margin, text_width, text_height);
            (frequency, sync, Rect::default(), Rect::default())
        } else {
            let half = text_width / 2;
            let (frequency, sync) =
                place_tempo_controls(scope, text_x, widget_margin, half, text_height);
            let (aux_frequency, aux_sync) = place_tempo_controls(
                scope,
                text_x + half,
                widget_margin,
                text_width - half,
                knob_section_height - 2 * widget_margin,
            );
            (frequency, sync, aux_frequency, aux_sync)
        };

        let (dx, dy) = (bounds.x, bounds.y);
        DelayLayout {
            bounds,
            on: Rect::new(dx, dy, title, title),
            frequency: frequency.translated(dx, dy),
            sync: sync.translated(dx, dy),
            aux_frequency: aux_frequency.translated(dx, dy),
            aux_sync: aux_sync.translated(dx, dy),
            style: style_bounds.translated(dx, dy),
            viewer: viewer.translated(dx, dy),
            filter_viewer: filter_viewer.translated(dx, dy),
            feedback: knobs[0].translated(dx, dy),
            dry_wet: knobs[1].translated(dx, dy),
            filter_cutoff: knobs2[0].translated(dx, dy),
            filter_spread: knobs2[1].translated(dx, dy),
        }
    }
}

use egui::Ui;
use sas_preset::strings::StringTable;

use super::{paint_section_background, viewer_corners, Effect};
use crate::controls::slider::{Slider, SliderStyle};
use crate::controls::tempo::{self, TempoMode};
use crate::host::Host;
use crate::paint::PixelPainter;
use crate::section::{draw_label, draw_text_component_background};
use crate::sections::{Overlay, Panel};

/// The section's controls (`DelaySection`'s constructor).
pub struct DelaySection {
    frequency: Slider,
    tempo: Slider,
    aux_frequency: Slider,
    aux_tempo: Slider,
    filter_cutoff: Slider,
    filter_spread: Slider,
    feedback: Slider,
    dry_wet: Slider,
    style: Slider,
}

impl Default for DelaySection {
    fn default() -> DelaySection {
        /// `TEMPO_DRAG_SENSITIVITY`.
        const TEMPO_DRAG_SENSITIVITY: f32 = 0.3;
        DelaySection {
            frequency: Slider::new("delay_frequency", SliderStyle::Text),
            tempo: Slider::new("delay_tempo", SliderStyle::Text)
                .sensitivity(TEMPO_DRAG_SENSITIVITY),
            aux_frequency: Slider::new("delay_aux_frequency", SliderStyle::Text),
            aux_tempo: Slider::new("delay_aux_tempo", SliderStyle::Text)
                .sensitivity(TEMPO_DRAG_SENSITIVITY),
            filter_cutoff: Slider::new("delay_filter_cutoff", SliderStyle::Rotary),
            filter_spread: Slider::new("delay_filter_spread", SliderStyle::Rotary),
            feedback: Slider::new("delay_feedback", SliderStyle::Rotary).bipolar(),
            dry_wet: Slider::new("delay_dry_wet", SliderStyle::Rotary),
            style: Slider::selector("delay_style", Some(StringTable::DelayStyle.values())),
        }
    }
}

impl DelaySection {
    /// `DelaySection::paintBackground`, then its controls.
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
        let style = host.value("delay_style") as usize;
        let l = DelayLayout::new(scope, bounds, style);
        let rotaries = [
            (&self.feedback, l.feedback),
            (&self.dry_wet, l.dry_wet),
            (&self.filter_cutoff, l.filter_cutoff),
            (&self.filter_spread, l.filter_spread),
        ];
        paint_section_background(
            pixels,
            scope,
            Effect::Delay,
            bounds,
            &rotaries,
            &[l.viewer, l.filter_viewer],
        );
        let widget_margin = scope.widget_margin() as i32;
        let section_height = scope.knob_section_height() as i32;
        let right = l.sync.right().max(l.aux_sync.right());
        let frequency = Rect::new(
            l.frequency.x,
            bounds.y + widget_margin,
            right - l.frequency.x,
            section_height - 2 * widget_margin,
        );
        draw_text_component_background(pixels, scope, frequency, true);
        draw_text_component_background(pixels, scope, l.style, true);
        for (text, rect) in [
            ("FEEDBACK", l.feedback),
            ("MIX", l.dry_wet),
            ("CUTOFF", l.filter_cutoff),
            ("SPREAD", l.filter_spread),
        ] {
            draw_label(pixels, scope, text, rect, false);
        }
        draw_label(pixels, scope, "MODE", l.style, true);
        draw_label(pixels, scope, "FREQUENCY", frequency, true);
        tempo::paint_divider(pixels, scope, l.sync);
        tempo::paint_divider(pixels, scope, l.aux_sync);

        let mut panel = Panel::new(ui, host, pixels, *scope, overlay, clip);
        let sync = host.value("delay_sync");
        let rate = if TempoMode::from_value(sync).shows_free_slider() {
            &self.frequency
        } else {
            &self.tempo
        };
        panel.slider(rate, l.frequency);
        let aux_sync = host.value("delay_aux_sync");
        if style != STYLE_MONO {
            let aux_rate = if TempoMode::from_value(aux_sync).shows_free_slider() {
                &self.aux_frequency
            } else {
                &self.aux_tempo
            };
            panel.slider(aux_rate, l.aux_frequency);
        }
        panel.slider(&self.style, l.style);
        for (slider, rect) in rotaries {
            panel.slider(slider, rect);
        }
        tempo::paint(pixels, scope, l.sync, sync);
        if style != STYLE_MONO {
            tempo::paint(pixels, scope, l.aux_sync, aux_sync);
        }
        viewer_corners(&mut panel, l.viewer);
        viewer_corners(&mut panel, l.filter_viewer);
        panel.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sections::effects::Effect;
    use crate::skin::Skin;

    /// The stereo branch of `resizeTempoControls`, against upstream's bounds
    /// for a stereo delay at 1400×820 (ref-gui with
    /// `migration/corpus/presets/fx_chain_reordered.vital`; the layout
    /// fixtures cover the mono branch).
    #[test]
    fn a_stereo_delay_splits_its_tempo_controls() {
        let skin = Skin::default();
        let scope = Scope::top(&skin, 1.0).within(Effect::Delay.skin());
        let layout = DelayLayout::new(&scope, Rect::new(240, 1035, 552, 137), 1);
        assert_eq!(layout.frequency, Rect::new(277, 1040, 54, 61));
        assert_eq!(layout.sync, Rect::new(331, 1048, 29, 29));
        assert_eq!(layout.aux_frequency, Rect::new(360, 1040, 55, 61));
        assert_eq!(layout.aux_sync, Rect::new(415, 1048, 29, 29));
        assert_eq!(layout.style, Rect::new(277, 1106, 167, 61));
    }
}
