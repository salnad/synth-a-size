// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Reverb section. Ports `ReverbSection`
//! (`vital/src/interface/editor_sections/reverb_section.cpp`).

use egui::Ui;

use super::equalizer_response::{EqBand, EqualizerResponse};
use super::viewers::Live;
use super::{paint_section_background, viewer_corners, Effect};
use crate::controls::slider::{Slider, SliderStyle};
use crate::controls::tab_selector::TabSelector;
use crate::host::Host;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::section::{draw_label_with_size, place_knobs_in_area};
use crate::sections::{Overlay, Panel};
use crate::skin::ValueId;

/// `ReverbSection::kFeedbackFilterBuffer`: the feedback EQ's headroom above
/// and below its gain range.
const FEEDBACK_FILTER_BUFFER: f32 = 0.4;

/// `ReverbSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReverbLayout {
    pub bounds: Rect,
    pub on: Rect,
    pub response: Rect,
    pub selected_band: Rect,
    pub low_pre_cutoff: Rect,
    pub high_pre_cutoff: Rect,
    /// The selected shelf's knobs (both shelves' sit in the same places).
    pub cutoff: Rect,
    pub gain: Rect,
    pub chorus_amount: Rect,
    pub chorus_frequency: Rect,
    pub delay: Rect,
    pub size: Rect,
    pub dry_wet: Rect,
    pub decay_time: Rect,
}

impl ReverbLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> ReverbLayout {
        let widget_margin = scope.widget_margin() as i32;
        let title = crate::section::title_width(scope) as i32;
        let (width, height) = (bounds.width, bounds.height);
        let eq_width = height - 2 * widget_margin;
        let feedback_widget_x = (width - title - eq_width - 2 * widget_margin) as f32 / 5.0
            + title as f32
            + widget_margin as f32;
        let section_height = scope.knob_section_height() as i32;
        let widget_rounding = scope.value(ValueId::WidgetRoundedCorner) as i32;
        let band_height = (FEEDBACK_FILTER_BUFFER * 0.5 * eq_width as f32) as i32;
        let selected_x = (feedback_widget_x + widget_rounding as f32) as i32;
        let selected_width = eq_width - 2 * widget_rounding;
        let selected_band = Rect::new(selected_x, widget_margin, selected_width, band_height);
        let feedback_x = feedback_widget_x as i32;
        let response = Rect::new(feedback_x, widget_margin, eq_width, eq_width);

        let pre_cutoff_x = title + widget_margin;
        let pre_cutoff_width =
            (feedback_widget_x - pre_cutoff_x as f32 - widget_margin as f32) as i32;
        let knob_y2 = section_height - widget_margin;
        let low_pre_cutoff = Rect::new(
            pre_cutoff_x,
            0,
            pre_cutoff_width,
            section_height - widget_margin,
        );
        let high_pre_cutoff = Rect::new(
            pre_cutoff_x,
            knob_y2,
            pre_cutoff_width,
            section_height - widget_margin,
        );
        let knobs_x = (feedback_widget_x + eq_width as f32) as i32;
        let knobs_width = width - knobs_x;
        let row1 =
            place_knobs_in_area(scope, Rect::new(knobs_x, 0, knobs_width, section_height), 4);
        let row2 = place_knobs_in_area(
            scope,
            Rect::new(knobs_x, knob_y2, knobs_width, section_height),
            4,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        let at = |rect: Rect| rect.translated(dx, dy);
        ReverbLayout {
            bounds,
            on: Rect::new(dx, dy, title, title),
            response: at(response),
            selected_band: at(selected_band),
            low_pre_cutoff: at(low_pre_cutoff),
            high_pre_cutoff: at(high_pre_cutoff),
            cutoff: at(row1[0]),
            chorus_amount: at(row1[1]),
            delay: at(row1[2]),
            dry_wet: at(row1[3]),
            gain: at(row2[0]),
            chorus_frequency: at(row2[1]),
            size: at(row2[2]),
            decay_time: at(row2[3]),
        }
    }
}

/// The feedback EQ's shelves: low, high.
const SHELVES: [EqBand; 2] = [
    EqBand {
        cutoff: "reverb_low_shelf_cutoff",
        resonance: None,
        gain: "reverb_low_shelf_gain",
    },
    EqBand {
        cutoff: "reverb_high_shelf_cutoff",
        resonance: None,
        gain: "reverb_high_shelf_gain",
    },
];

/// The section's controls (`ReverbSection`'s constructor), and the shelf
/// its knobs show (`selected_eq_band_`, not a parameter).
pub struct ReverbSection {
    low_pre_cutoff: Slider,
    high_pre_cutoff: Slider,
    cutoffs: [Slider; 2],
    gains: [Slider; 2],
    chorus_amount: Slider,
    chorus_frequency: Slider,
    delay: Slider,
    size: Slider,
    dry_wet: Slider,
    decay_time: Slider,
    /// The shelf the knobs show: 0 low, 2 high, as the response numbers
    /// them (`setSelectedBand(value * 2)`).
    selected_band: usize,
    /// The band the response draws selected (none after a press away from
    /// its handles).
    response_band: Option<usize>,
}

impl Default for ReverbSection {
    fn default() -> ReverbSection {
        let rotary = |name| Slider::new(name, SliderStyle::Rotary);
        ReverbSection {
            low_pre_cutoff: rotary("reverb_pre_low_cutoff").hz_alternate(),
            high_pre_cutoff: rotary("reverb_pre_high_cutoff").hz_alternate(),
            cutoffs: SHELVES.map(|shelf| rotary(shelf.cutoff).hz_alternate()),
            gains: SHELVES.map(|shelf| rotary(shelf.gain)),
            chorus_amount: rotary("reverb_chorus_amount"),
            chorus_frequency: rotary("reverb_chorus_frequency"),
            delay: rotary("reverb_delay"),
            size: rotary("reverb_size"),
            dry_wet: rotary("reverb_dry_wet"),
            decay_time: rotary("reverb_decay_time"),
            selected_band: 0,
            response_band: Some(0),
        }
    }
}

impl ReverbSection {
    /// `ReverbSection::paintBackground`, then its controls.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        scope: &Scope,
        overlay: &mut Overlay,
        bounds: Rect,
        clip: Rect,
    ) {
        let l = ReverbLayout::new(scope, bounds);
        let shelf = self.selected_band / 2;
        let rotaries = [
            (&self.low_pre_cutoff, l.low_pre_cutoff),
            (&self.high_pre_cutoff, l.high_pre_cutoff),
            (&self.cutoffs[shelf], l.cutoff),
            (&self.gains[shelf], l.gain),
            (&self.chorus_amount, l.chorus_amount),
            (&self.chorus_frequency, l.chorus_frequency),
            (&self.delay, l.delay),
            (&self.size, l.size),
            (&self.dry_wet, l.dry_wet),
            (&self.decay_time, l.decay_time),
        ];
        paint_section_background(
            pixels,
            scope,
            Effect::Reverb,
            bounds,
            &rotaries,
            &[l.response],
        );
        for (text, rect) in [
            ("TIME", l.decay_time),
            ("PRE LOW CUT", l.low_pre_cutoff),
            ("PRE HIGH CUT", l.high_pre_cutoff),
            ("CUTOFF", l.cutoff),
            ("GAIN", l.gain),
            ("CHORUS AMT", l.chorus_amount),
            ("CHORUS FRQ", l.chorus_frequency),
            ("DELAY", l.delay),
            ("SIZE", l.size),
            ("MIX", l.dry_wet),
        ] {
            // paintBackground sets proportional_regular at size_ratio * 10
            // rather than setLabelFont's label height.
            draw_label_with_size(pixels, scope, text, rect, false, scope.size_ratio() * 10.0);
        }

        let live = Live::new(host);
        let response = EqualizerResponse {
            low: SHELVES[0],
            band: None,
            high: SHELVES[1],
            high_pass: false,
            notch: false,
            low_pass: false,
            active: true,
            draw_frequency_lines: false,
            db_buffer_ratio: FEEDBACK_FILTER_BUFFER,
        };
        let hz = host.model().hz_frequency_display();
        let popup = Some((self.cutoffs[shelf].displayed(hz), l.cutoff));
        let chosen = response.show(
            ui,
            &live,
            pixels,
            scope,
            overlay,
            l.response,
            &mut self.response_band,
            popup,
            &mut |_| {},
        );
        if let Some(band) = chosen {
            self.selected_band = band;
        }

        let mut panel = Panel::new(ui, host, pixels, *scope, overlay, clip);
        let shelf = self.selected_band / 2;
        for (slider, rect) in [
            (&self.low_pre_cutoff, l.low_pre_cutoff),
            (&self.high_pre_cutoff, l.high_pre_cutoff),
            (&self.cutoffs[shelf], l.cutoff),
            (&self.gains[shelf], l.gain),
            (&self.chorus_amount, l.chorus_amount),
            (&self.chorus_frequency, l.chorus_frequency),
            (&self.delay, l.delay),
            (&self.size, l.size),
            (&self.dry_wet, l.dry_wet),
            (&self.decay_time, l.decay_time),
        ] {
            panel.slider(slider, rect);
        }
        // The shelf tabs over the top of the response.
        let tabs = TabSelector {
            font_height_percent: 0.4,
            ..TabSelector::new(&["LOW", "HIGH"])
        };
        let mut selected = shelf;
        let id = panel.ui.scope_id().with("reverb selected band");
        if tabs.interact(panel.ui, pixels, id, l.selected_band, &mut selected) {
            self.selected_band = selected * 2;
            self.response_band = Some(self.selected_band);
        }
        tabs.paint(
            pixels,
            &panel.scope,
            l.selected_band,
            self.selected_band / 2,
        );
        viewer_corners(&mut panel, l.response);
        panel.finish();
    }
}
