// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Flanger section. Ports `FlangerSection`
//! (`vital/src/interface/editor_sections/flanger_section.cpp`).

use super::{divided_area_buffered, divided_area_unbuffered, place_tempo_controls, with_left};
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
    let title = crate::section::title_width(scope) as i32;
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
        let title = crate::section::title_width(scope) as i32;
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

use egui::Ui;

use super::{frequency_bounds, paint_section_background, viewer_corners, Effect};
use crate::controls::slider::{Slider, SliderStyle};
use crate::controls::tempo::{self, TempoMode};
use crate::host::Host;
use crate::paint::PixelPainter;
use crate::section::{draw_label, draw_text_component_background};
use crate::sections::{Overlay, Panel};

/// `FlangerSection` and `PhaserSection`'s shared controls.
pub(crate) struct ModulatedFilterControls {
    pub sync: &'static str,
    pub phase_offset: Slider,
    pub mod_depth: Slider,
    pub center: Slider,
    pub frequency: Slider,
    pub tempo: Slider,
    pub feedback: Slider,
    pub dry_wet: Slider,
}

impl ModulatedFilterControls {
    /// `prefix` is "flanger" or "phaser"; `feedback_bipolar` is the
    /// flanger's.
    pub fn new(names: [&'static str; 8], feedback_bipolar: bool) -> ModulatedFilterControls {
        /// `TEMPO_DRAG_SENSITIVITY`.
        const TEMPO_DRAG_SENSITIVITY: f32 = 0.3;
        let feedback = Slider::new(names[6], SliderStyle::Rotary);
        ModulatedFilterControls {
            sync: names[0],
            phase_offset: Slider::new(names[1], SliderStyle::Rotary),
            mod_depth: Slider::new(names[2], SliderStyle::Rotary),
            center: Slider::new(names[3], SliderStyle::Rotary),
            frequency: Slider::new(names[4], SliderStyle::Text),
            tempo: Slider::new(names[5], SliderStyle::Text).sensitivity(TEMPO_DRAG_SENSITIVITY),
            feedback: if feedback_bipolar {
                feedback.bipolar()
            } else {
                feedback
            },
            dry_wet: Slider::new(names[7], SliderStyle::Rotary),
        }
    }

    /// The rotary knobs and their bounds.
    pub fn rotaries(&self, l: &ModulatedFilterBounds) -> [(&Slider, Rect); 5] {
        [
            (&self.phase_offset, l.phase_offset),
            (&self.mod_depth, l.mod_depth),
            (&self.center, l.center),
            (&self.feedback, l.feedback),
            (&self.dry_wet, l.dry_wet),
        ]
    }

    /// The labels, text background and tempo divider both sections paint.
    pub fn paint_labels(&self, pixels: &PixelPainter, scope: &Scope, l: &ModulatedFilterBounds) {
        let frequency = frequency_bounds(l.frequency, l.sync);
        draw_text_component_background(pixels, scope, frequency, true);
        tempo::paint_divider(pixels, scope, l.sync);
        draw_label(pixels, scope, "FREQUENCY", frequency, true);
        for (text, rect) in [
            ("FEEDBACK", l.feedback),
            ("MIX", l.dry_wet),
            ("CENTER", l.center),
            ("DEPTH", l.mod_depth),
            ("OFFSET", l.phase_offset),
        ] {
            draw_label(pixels, scope, text, rect, false);
        }
    }

    /// The sliders and the tempo selector.
    pub fn show(&self, panel: &mut Panel, l: &ModulatedFilterBounds) {
        let sync = panel.host.value(self.sync);
        let rate = if TempoMode::from_value(sync).shows_free_slider() {
            &self.frequency
        } else {
            &self.tempo
        };
        panel.slider(rate, l.frequency);
        for (slider, rect) in self.rotaries(l) {
            panel.slider(slider, rect);
        }
        tempo::paint(panel.pixels, &panel.scope, l.sync, sync);
    }
}

/// The bounds [`ModulatedFilterControls`] uses.
pub(crate) struct ModulatedFilterBounds {
    pub frequency: Rect,
    pub sync: Rect,
    pub phase_offset: Rect,
    pub feedback: Rect,
    pub dry_wet: Rect,
    pub center: Rect,
    pub mod_depth: Rect,
}

impl FlangerLayout {
    fn shared(&self) -> ModulatedFilterBounds {
        ModulatedFilterBounds {
            frequency: self.frequency,
            sync: self.sync,
            phase_offset: self.phase_offset,
            feedback: self.feedback,
            dry_wet: self.dry_wet,
            center: self.center,
            mod_depth: self.mod_depth,
        }
    }
}

/// The section's controls (`FlangerSection`'s constructor).
pub struct FlangerSection {
    controls: ModulatedFilterControls,
}

impl Default for FlangerSection {
    fn default() -> FlangerSection {
        FlangerSection {
            controls: ModulatedFilterControls::new(
                [
                    "flanger_sync",
                    "flanger_phase_offset",
                    "flanger_mod_depth",
                    "flanger_center",
                    "flanger_frequency",
                    "flanger_tempo",
                    "flanger_feedback",
                    "flanger_dry_wet",
                ],
                true,
            ),
        }
    }
}

impl FlangerSection {
    /// `FlangerSection::paintBackground`, then its controls.
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
        let l = FlangerLayout::new(scope, bounds);
        let shared = l.shared();
        paint_section_background(
            pixels,
            scope,
            Effect::Flanger,
            bounds,
            &self.controls.rotaries(&shared),
            &[l.response],
        );
        self.controls.paint_labels(pixels, scope, &shared);
        let mut panel = Panel::new(ui, host, pixels, *scope, overlay, clip);
        self.controls.show(&mut panel, &shared);
        viewer_corners(&mut panel, l.response);
        panel.finish();
    }
}
