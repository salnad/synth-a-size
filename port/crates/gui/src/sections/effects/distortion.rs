// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Distortion section. Ports `DistortionSection`
//! (`vital/src/interface/editor_sections/distortion_section.cpp`).

use super::{divided_area_buffered, divided_area_unbuffered, with_left};
use crate::layout::Rect;
use crate::scope::Scope;
use crate::section::place_knobs_in_area;

/// `DistortionSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DistortionLayout {
    pub bounds: Rect,
    pub on: Rect,
    pub kind: Rect,
    pub filter_order: Rect,
    pub viewer: Rect,
    pub filter_response: Rect,
    pub drive: Rect,
    pub mix: Rect,
    pub filter_cutoff: Rect,
    pub filter_resonance: Rect,
    pub filter_blend: Rect,
}

impl DistortionLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> DistortionLayout {
        let widget_margin = scope.widget_margin() as i32;
        let title = crate::section::title_width(scope) as i32;
        let section_height = scope.knob_section_height() as i32;
        let (width, height) = (bounds.width, bounds.height);

        let area = with_left(Rect::new(0, 0, width, height), title);
        let knobs_area = divided_area_buffered(area, 2, 1, widget_margin);
        let settings_area = divided_area_unbuffered(area, 4, 0, widget_margin);

        let widget_x = settings_area.right() + widget_margin;
        let widget_width = knobs_area.x - widget_x;

        let knob_y2 = section_height - widget_margin;
        let kind = Rect::new(
            settings_area.x,
            widget_margin,
            settings_area.width,
            section_height - 2 * widget_margin,
        );
        let filter_order = Rect::new(
            settings_area.x,
            knob_y2 + widget_margin,
            settings_area.width,
            section_height - 2 * widget_margin,
        );

        let viewer_height = (height - 3 * widget_margin) / 2;
        let viewer = Rect::new(widget_x, widget_margin, widget_width, viewer_height);
        let response_height = height - viewer_height - 3 * widget_margin;
        let filter_y = height - response_height - widget_margin;
        let filter_response = Rect::new(widget_x, filter_y, widget_width, response_height);

        let knobs = place_knobs_in_area(
            scope,
            Rect::new(knobs_area.x, 0, knobs_area.width, section_height),
            2,
        );
        let knobs2 = place_knobs_in_area(
            scope,
            Rect::new(knobs_area.x, knob_y2, knobs_area.width, section_height),
            3,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        DistortionLayout {
            bounds,
            on: Rect::new(dx, dy, title, title),
            kind: kind.translated(dx, dy),
            filter_order: filter_order.translated(dx, dy),
            viewer: viewer.translated(dx, dy),
            filter_response: filter_response.translated(dx, dy),
            drive: knobs[0].translated(dx, dy),
            mix: knobs[1].translated(dx, dy),
            filter_cutoff: knobs2[0].translated(dx, dy),
            filter_resonance: knobs2[1].translated(dx, dy),
            filter_blend: knobs2[2].translated(dx, dy),
        }
    }
}

use egui::Ui;
use sas_preset::strings::StringTable;

use super::{paint_section_background, viewer_corners, Effect};
use crate::controls::slider::{Slider, SliderStyle};
use crate::host::Host;
use crate::paint::PixelPainter;
use crate::section::{draw_label, draw_text_component_background};
use crate::sections::{Overlay, Panel};

/// The section's controls (`DistortionSection`'s constructor).
pub struct DistortionSection {
    kind: Slider,
    filter_order: Slider,
    filter_cutoff: Slider,
    filter_resonance: Slider,
    filter_blend: Slider,
    drive: Slider,
    mix: Slider,
}

impl Default for DistortionSection {
    fn default() -> DistortionSection {
        DistortionSection {
            kind: Slider::selector(
                "distortion_type",
                Some(StringTable::DistortionType.values()),
            ),
            filter_order: Slider::selector(
                "distortion_filter_order",
                Some(StringTable::DistortionFilterOrder.values()),
            ),
            filter_cutoff: Slider::new("distortion_filter_cutoff", SliderStyle::Rotary),
            filter_resonance: Slider::new("distortion_filter_resonance", SliderStyle::Rotary),
            filter_blend: Slider::new("distortion_filter_blend", SliderStyle::Rotary).bipolar(),
            drive: Slider::new("distortion_drive", SliderStyle::Rotary),
            mix: Slider::new("distortion_mix", SliderStyle::Rotary),
        }
    }
}

impl DistortionSection {
    /// `DistortionSection::paintBackground`, then its controls.
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
        let l = DistortionLayout::new(scope, bounds);
        // setFilterActive: the filter knobs show disabled without a filter.
        let filter_active = host.value("distortion_filter_order") != 0.0;
        let filter = |slider: &Slider| Slider {
            active: filter_active,
            ..*slider
        };
        let (cutoff, resonance, blend) = (
            filter(&self.filter_cutoff),
            filter(&self.filter_resonance),
            filter(&self.filter_blend),
        );
        let rotaries = [
            (&self.drive, l.drive),
            (&self.mix, l.mix),
            (&cutoff, l.filter_cutoff),
            (&resonance, l.filter_resonance),
            (&blend, l.filter_blend),
        ];
        paint_section_background(
            pixels,
            scope,
            Effect::Distortion,
            bounds,
            &rotaries,
            &[l.viewer, l.filter_response],
        );
        draw_text_component_background(pixels, scope, l.kind, true);
        draw_text_component_background(pixels, scope, l.filter_order, true);
        for (text, rect) in [
            ("DRIVE", l.drive),
            ("MIX", l.mix),
            ("CUTOFF", l.filter_cutoff),
            ("RESONANCE", l.filter_resonance),
            ("BLEND", l.filter_blend),
        ] {
            draw_label(pixels, scope, text, rect, false);
        }
        draw_label(pixels, scope, "TYPE", l.kind, true);
        draw_label(pixels, scope, "FILTER", l.filter_order, true);

        let mut panel = Panel::new(ui, host, pixels, *scope, overlay, clip);
        panel.slider(&self.kind, l.kind);
        panel.slider(&self.filter_order, l.filter_order);
        for (slider, rect) in rotaries {
            panel.slider(slider, rect);
        }
        viewer_corners(&mut panel, l.viewer);
        viewer_corners(&mut panel, l.filter_response);
        panel.finish();
    }
}
