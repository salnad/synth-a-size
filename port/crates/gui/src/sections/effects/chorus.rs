// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Chorus section. Ports `ChorusSection`
//! (`vital/src/interface/editor_sections/chorus_section.cpp`).

use super::{divided_area_buffered, place_tempo_controls, title_width, with_left, with_top};
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
        let title = title_width(scope);
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
