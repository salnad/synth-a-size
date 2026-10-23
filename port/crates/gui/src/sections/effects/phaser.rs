// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Phaser section. Ports `PhaserSection`
//! (`vital/src/interface/editor_sections/phaser_section.cpp`).

use super::flanger::modulated_filter_parts;
use super::title_width;
use crate::layout::Rect;
use crate::scope::Scope;
use crate::skin::ValueId;

/// `SynthSlider::kLinearWidthPercent`.
const LINEAR_WIDTH_PERCENT: f32 = 0.26;

/// `PhaserSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaserLayout {
    pub bounds: Rect,
    pub on: Rect,
    /// `phaser_frequency` and `phaser_tempo` share these bounds.
    pub frequency: Rect,
    pub sync: Rect,
    pub phase_offset: Rect,
    pub blend: Rect,
    pub response: Rect,
    pub feedback: Rect,
    pub dry_wet: Rect,
    pub center: Rect,
    pub mod_depth: Rect,
}

/// `SynthSection::getSliderOverlap`.
pub fn slider_overlap(scope: &Scope) -> f32 {
    let total_width = scope.value(ValueId::SliderWidth) as i32;
    let extra = total_width % 2;
    let slider_width =
        ((LINEAR_WIDTH_PERCENT * total_width as f32 * 0.5).floor() * 2.0) as i32 + extra;
    ((total_width - slider_width) / 2) as f32
}

impl PhaserLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> PhaserLayout {
        let widget_margin = scope.widget_margin() as i32;
        let parts = modulated_filter_parts(scope, bounds.width, bounds.height);
        let overlap = slider_overlap(scope);
        let blend = Rect::new(
            parts.widget_x - widget_margin,
            (widget_margin as f32 - overlap) as i32,
            parts.widget_width + 2 * widget_margin,
            scope.value(ValueId::SliderWidth) as i32,
        );
        // getSliderOverlapWithSpace.
        let overlap_with_space = overlap - widget_margin as f32;
        let widget_y = (blend.bottom() as f32 - overlap_with_space) as i32;
        let response = Rect::new(
            parts.widget_x,
            widget_y,
            parts.widget_width,
            bounds.height - widget_y - widget_margin,
        );
        let (dx, dy) = (bounds.x, bounds.y);
        let title = title_width(scope);
        PhaserLayout {
            bounds,
            on: Rect::new(dx, dy, title, title),
            frequency: parts.frequency.translated(dx, dy),
            sync: parts.sync.translated(dx, dy),
            phase_offset: parts.phase_offset.translated(dx, dy),
            blend: blend.translated(dx, dy),
            response: response.translated(dx, dy),
            feedback: parts.knobs[0].translated(dx, dy),
            dry_wet: parts.knobs[1].translated(dx, dy),
            center: parts.knobs2[0].translated(dx, dy),
            mod_depth: parts.knobs2[1].translated(dx, dy),
        }
    }
}
