// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Distortion section. Ports `DistortionSection`
//! (`vital/src/interface/editor_sections/distortion_section.cpp`).

use super::{divided_area_buffered, divided_area_unbuffered, title_width, with_left};
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
        let title = title_width(scope);
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
