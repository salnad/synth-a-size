// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Delay section. Ports `DelaySection`
//! (`vital/src/interface/editor_sections/delay_section.cpp`).

use super::{
    divided_area_buffered, divided_area_unbuffered, place_tempo_controls, title_width, with_left,
};
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
        let title = title_width(scope);
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
