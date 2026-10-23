// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Portamento section: glide time and slope, and the octave scale,
//! always glide and legato switches. Ports `PortamentoSection`
//! (`vital/src/interface/editor_sections/portamento_section.cpp`).

use super::Panel;
use crate::controls::button::SynthButton;
use crate::controls::slider::{Slider, SliderStyle};
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::section::{
    draw_label, draw_text_component_background, label_background_bounds, paint_body, paint_border,
    place_knobs_in_area,
};
use crate::skin::Section;

/// The section's skin override (`setSkinOverride(Skin::kKeyboard)`).
pub const SKIN: &[Section] = &[Section::Keyboard];

/// `PortamentoSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortamentoLayout {
    pub bounds: Rect,
    pub portamento_time: Rect,
    pub portamento_slope: Rect,
    pub portamento_force: Rect,
    pub portamento_scale: Rect,
    pub legato: Rect,
}

impl PortamentoLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> PortamentoLayout {
        let height = bounds.height;
        let buttons_width = 3 * bounds.width / 8;
        let buttons_x = bounds.width - buttons_width;
        let widget_margin = scope.widget_margin() as i32;
        let internal_margin = widget_margin / 2;
        let button_height = (height - 2 * (widget_margin + internal_margin)) as f32 / 3.0;
        let portamento_force = Rect::new(
            buttons_x,
            widget_margin,
            buttons_width - widget_margin,
            button_height as i32,
        );
        let legato = Rect::new(
            buttons_x,
            (height as f32 - widget_margin as f32 - button_height) as i32,
            buttons_width - widget_margin,
            button_height as i32,
        );
        let portamento_scale = Rect::new(
            buttons_x,
            portamento_force.bottom() + internal_margin,
            buttons_width - widget_margin,
            legato.y - portamento_force.bottom() - 2 * internal_margin,
        );

        let knobs = place_knobs_in_area(scope, Rect::new(0, 0, buttons_x, height), 2);
        let portamento_time = knobs[0];
        let slope = knobs[1];
        // `withTop(widget_margin)` then `withBottom(label top)`.
        let top = widget_margin;
        let bottom = label_background_bounds(scope, slope, true).y;
        let portamento_slope = Rect::new(slope.x, top, slope.width, (bottom - top).max(0));

        let (dx, dy) = (bounds.x, bounds.y);
        PortamentoLayout {
            bounds,
            portamento_time: portamento_time.translated(dx, dy),
            portamento_slope: portamento_slope.translated(dx, dy),
            portamento_force: portamento_force.translated(dx, dy),
            portamento_scale: portamento_scale.translated(dx, dy),
            legato: legato.translated(dx, dy),
        }
    }
}

/// The section's controls (`PortamentoSection`'s constructor).
pub struct PortamentoSection {
    portamento: Slider,
    portamento_slope: Slider,
    portamento_scale: SynthButton,
    portamento_force: SynthButton,
    legato: SynthButton,
}

impl Default for PortamentoSection {
    fn default() -> PortamentoSection {
        PortamentoSection {
            portamento: Slider::new("portamento_time", SliderStyle::Rotary),
            portamento_slope: Slider::new("portamento_slope", SliderStyle::Curve),
            portamento_scale: SynthButton::text("portamento_scale", "OCTAVE SCALE"),
            portamento_force: SynthButton::text("portamento_force", "ALWAYS GLIDE"),
            legato: SynthButton::text("legato", "LEGATO"),
        }
    }
}

impl PortamentoSection {
    /// `PortamentoSection::paintBackground`.
    pub fn paint_background(
        &self,
        pixels: &PixelPainter,
        scope: &Scope,
        layout: &PortamentoLayout,
    ) {
        paint_body(pixels, scope, layout.bounds);
        paint_border(pixels, scope, layout.bounds);
        self.portamento
            .paint_shadow(pixels, scope, layout.portamento_time);
        draw_label(pixels, scope, "GLIDE", layout.portamento_time, false);

        // `withBottom(getHeight() - getWidgetMargin())`: the float difference
        // is truncated.
        let slope = layout.portamento_slope;
        let bottom = layout.bounds.y + (layout.bounds.height as f32 - scope.widget_margin()) as i32;
        let slope_bounds = Rect::new(slope.x, slope.y, slope.width, bottom - slope.y);
        draw_text_component_background(pixels, scope, slope_bounds, true);
        draw_label(pixels, scope, "SLOPE", slope_bounds, true);
    }

    pub fn show(&self, panel: &mut Panel, layout: &PortamentoLayout) {
        panel.slider(&self.portamento, layout.portamento_time);
        // The slope only matters while there is a glide
        // (PortamentoSection::sliderValueChanged).
        let mut slope = self.portamento_slope;
        slope.active = panel.host.value("portamento_time") != self.portamento.info.min;
        panel.slider(&slope, layout.portamento_slope);
        panel.button(&self.portamento_force, layout.portamento_force);
        panel.button(&self.portamento_scale, layout.portamento_scale);
        panel.button(&self.legato, layout.legato);
    }
}
