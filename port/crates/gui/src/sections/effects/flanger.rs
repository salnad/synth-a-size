// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Flanger section. Ports `FlangerSection`
//! (`vital/src/interface/editor_sections/flanger_section.cpp`).

use super::{
    divided_area_buffered, divided_area_unbuffered, place_tempo_controls, title_width, with_left,
};
use crate::layout::Rect;
use crate::scope::Scope;
use crate::section::place_knobs_in_area;

/// `FlangerSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlangerLayout {
    pub bounds: Rect,
    pub on: Rect,
    /// `flanger_frequency` and `flanger_tempo` share these bounds.
    pub frequency: Rect,
    pub sync: Rect,
    pub phase_offset: Rect,
    pub response: Rect,
    pub feedback: Rect,
    pub dry_wet: Rect,
    pub center: Rect,
    pub mod_depth: Rect,
}

/// The parts the flanger and phaser lay out alike, in section pixels.
pub(crate) struct ModulatedFilterParts {
    pub frequency: Rect,
    pub sync: Rect,
    pub phase_offset: Rect,
    pub knobs: Vec<Rect>,
    pub knobs2: Vec<Rect>,
    /// Between the tempo controls and the knobs.
    pub widget_x: i32,
    pub widget_width: i32,
}

/// `FlangerSection::resized` and `PhaserSection::resized`'s shared part.
pub(crate) fn modulated_filter_parts(
    scope: &Scope,
    width: i32,
    height: i32,
) -> ModulatedFilterParts {
    let title = title_width(scope);
    let widget_margin = scope.widget_margin() as i32;
    let area = with_left(Rect::new(0, 0, width, height), title);
    let knobs_area = divided_area_buffered(area, 3, 2, widget_margin);
    let tempo_area = divided_area_unbuffered(area, 4, 0, widget_margin);
    let section_height = scope.knob_section_height() as i32;

    let knobs_x = knobs_area.x;
    let knob_y2 = section_height - widget_margin;
    let (tempo_x, tempo_width) = (tempo_area.x, tempo_area.width);
    let widget_x = tempo_x + tempo_width + widget_margin;

    let (frequency, sync) = place_tempo_controls(
        scope,
        tempo_x,
        widget_margin,
        tempo_width,
        section_height - 2 * widget_margin,
    );
    let phase_offset = Rect::new(
        title + widget_margin,
        knob_y2,
        tempo_width,
        section_height - widget_margin,
    );
    let knobs = place_knobs_in_area(
        scope,
        Rect::new(knobs_x, 0, knobs_area.width, section_height),
        2,
    );
    let knobs2 = place_knobs_in_area(
        scope,
        Rect::new(knobs_x, knob_y2, knobs_area.width, section_height),
        2,
    );
    ModulatedFilterParts {
        frequency,
        sync,
        phase_offset,
        knobs,
        knobs2,
        widget_x,
        widget_width: knobs_x - widget_x,
    }
}

impl FlangerLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> FlangerLayout {
        let widget_margin = scope.widget_margin() as i32;
        let parts = modulated_filter_parts(scope, bounds.width, bounds.height);
        let response = Rect::new(
            parts.widget_x,
            widget_margin,
            parts.widget_width,
            bounds.height - 2 * widget_margin,
        );
        let (dx, dy) = (bounds.x, bounds.y);
        let title = title_width(scope);
        FlangerLayout {
            bounds,
            on: Rect::new(dx, dy, title, title),
            frequency: parts.frequency.translated(dx, dy),
            sync: parts.sync.translated(dx, dy),
            phase_offset: parts.phase_offset.translated(dx, dy),
            response: response.translated(dx, dy),
            feedback: parts.knobs[0].translated(dx, dy),
            dry_wet: parts.knobs[1].translated(dx, dy),
            center: parts.knobs2[0].translated(dx, dy),
            mod_depth: parts.knobs2[1].translated(dx, dy),
        }
    }
}
