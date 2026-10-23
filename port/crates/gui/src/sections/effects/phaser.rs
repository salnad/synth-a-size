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

use egui::Ui;

use super::flanger::{ModulatedFilterBounds, ModulatedFilterControls};
use super::{paint_section_background, viewer_corners, Effect};
use crate::controls::slider::{Slider, SliderStyle};
use crate::host::Host;
use crate::paint::PixelPainter;
use crate::sections::{Overlay, Panel};

impl PhaserLayout {
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

/// The section's controls (`PhaserSection`'s constructor).
pub struct PhaserSection {
    controls: ModulatedFilterControls,
    blend: Slider,
}

impl Default for PhaserSection {
    fn default() -> PhaserSection {
        PhaserSection {
            controls: ModulatedFilterControls::new(
                [
                    "phaser_sync",
                    "phaser_phase_offset",
                    "phaser_mod_depth",
                    "phaser_center",
                    "phaser_frequency",
                    "phaser_tempo",
                    "phaser_feedback",
                    "phaser_dry_wet",
                ],
                false,
            ),
            blend: Slider::new("phaser_blend", SliderStyle::Horizontal).bipolar(),
        }
    }
}

impl PhaserSection {
    /// `PhaserSection::paintBackground`, then its controls.
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
        let l = PhaserLayout::new(scope, bounds);
        let shared = l.shared();
        paint_section_background(
            pixels,
            scope,
            Effect::Phaser,
            bounds,
            &self.controls.rotaries(&shared),
            &[l.response],
        );
        self.controls.paint_labels(pixels, scope, &shared);
        let mut panel = Panel::new(ui, host, pixels, *scope, overlay, clip);
        self.controls.show(&mut panel, &shared);
        panel.slider(&self.blend, l.blend);
        viewer_corners(&mut panel, l.response);
        panel.finish();
    }
}
