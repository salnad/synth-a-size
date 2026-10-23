// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Voice section: polyphony, pitch bend range, velocity tracking and
//! stereo routing. Ports `VoiceSection`
//! (`vital/src/interface/editor_sections/voice_section.cpp`).

use egui::Sense;

use super::{MenuOwner, Panel};
use crate::controls::slider::{Slider, SliderStyle};
use crate::fonts::Face;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::popup::{Menu, MenuItem};
use crate::scope::Scope;
use crate::section::{
    draw_label, draw_text_component_background, label_background_bounds, paint_body, paint_border,
    place_knobs_in_area,
};
use crate::skin::{ColorId, Section, ValueId};

/// The section's skin override (`setSkinOverride(Skin::kKeyboard)`).
pub const SKIN: &[Section] = &[Section::Keyboard];

/// `strings::kStereoModeNames`.
pub const STEREO_MODE_NAMES: [&str; 2] = ["SPREAD", "ROTATE"];

/// `VoiceSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceLayout {
    pub bounds: Rect,
    pub polyphony: Rect,
    pub pitch_bend_range: Rect,
    pub velocity_track: Rect,
    pub stereo_routing: Rect,
    /// The stereo mode text and its selector button, over the stereo
    /// routing label.
    pub stereo_mode: Rect,
}

impl VoiceLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> VoiceLayout {
        let widget_margin = scope.widget_margin() as i32;
        let text_width =
            (scope.value(ValueId::ModulationButtonWidth) - widget_margin as f32) as i32;
        let text_height = bounds.height - 2 * widget_margin;
        let polyphony = Rect::new(widget_margin, widget_margin, text_width, text_height);
        let pitch_bend_range = Rect::new(
            polyphony.right() + widget_margin,
            widget_margin,
            text_width,
            text_height,
        );

        let knobs_x = pitch_bend_range.right();
        let knob_bounds = Rect::new(knobs_x, 0, bounds.width - knobs_x, bounds.height);
        let knobs = place_knobs_in_area(scope, knob_bounds, 2);
        let (velocity_track, stereo_routing) = (knobs[0], knobs[1]);
        let stereo_mode = label_background_bounds(scope, stereo_routing, false);

        let (dx, dy) = (bounds.x, bounds.y);
        VoiceLayout {
            bounds,
            polyphony: polyphony.translated(dx, dy),
            pitch_bend_range: pitch_bend_range.translated(dx, dy),
            velocity_track: velocity_track.translated(dx, dy),
            stereo_routing: stereo_routing.translated(dx, dy),
            stereo_mode: stereo_mode.translated(dx, dy),
        }
    }
}

/// The section's controls (`VoiceSection`'s constructor).
pub struct VoiceSection {
    polyphony: Slider,
    velocity_track: Slider,
    pitch_bend_range: Slider,
    stereo_routing: Slider,
}

impl Default for VoiceSection {
    fn default() -> VoiceSection {
        /// `VoiceSection`'s `kKnobSensitivity`.
        const KNOB_SENSITIVITY: f32 = 0.2;
        VoiceSection {
            polyphony: Slider::new("polyphony", SliderStyle::Text)
                .sensitivity(KNOB_SENSITIVITY)
                .without_suffix(),
            velocity_track: Slider::new("velocity_track", SliderStyle::Rotary).bipolar(),
            pitch_bend_range: Slider::new("pitch_bend_range", SliderStyle::Text)
                .sensitivity(KNOB_SENSITIVITY)
                .without_suffix(),
            stereo_routing: Slider::new("stereo_routing", SliderStyle::Rotary),
        }
    }
}

impl VoiceSection {
    /// `VoiceSection::paintBackground`.
    pub fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, layout: &VoiceLayout) {
        paint_body(pixels, scope, layout.bounds);
        paint_border(pixels, scope, layout.bounds);
        // paintKnobShadows: only the rotary knobs draw one.
        self.velocity_track
            .paint_shadow(pixels, scope, layout.velocity_track);
        self.stereo_routing
            .paint_shadow(pixels, scope, layout.stereo_routing);

        draw_text_component_background(pixels, scope, layout.polyphony, true);
        draw_label(pixels, scope, "VOICES", layout.polyphony, true);
        draw_label(pixels, scope, "VEL TRK", layout.velocity_track, false);
        draw_label(pixels, scope, "", layout.stereo_routing, false);

        draw_text_component_background(pixels, scope, layout.pitch_bend_range, true);
        draw_label(pixels, scope, "BEND", layout.pitch_bend_range, true);
    }

    /// The controls, and the stereo mode text and selector.
    pub fn show(&self, panel: &mut Panel, layout: &VoiceLayout) {
        panel.slider(&self.polyphony, layout.polyphony);
        panel.slider(&self.pitch_bend_range, layout.pitch_bend_range);
        panel.slider(&self.velocity_track, layout.velocity_track);
        panel.slider(&self.stereo_routing, layout.stereo_routing);

        let mode = (panel.host.value("stereo_mode") as usize).min(STEREO_MODE_NAMES.len() - 1);
        let colour = panel.scope.color(ColorId::BodyText);
        let size = panel.scope.value(ValueId::LabelHeight);
        panel.text_centred(
            STEREO_MODE_NAMES[mode],
            layout.stereo_mode,
            Face::Regular,
            size,
            colour,
        );

        // The selector is triggered on mouse down and opens below itself.
        let rect = panel.pixels.rect(layout.stereo_mode.into());
        let response = panel.ui.interact(
            rect,
            panel.ui.scope_id().with("stereo mode"),
            Sense::click(),
        );
        if response.is_pointer_button_down_on()
            && panel.ui.input(|input| input.pointer.any_pressed())
        {
            panel.overlay.menu = Some(Menu::new(
                STEREO_MODE_NAMES
                    .iter()
                    .enumerate()
                    .map(|(index, name)| MenuItem::new(index as i32, *name))
                    .collect(),
                egui::pos2(
                    layout.stereo_mode.x as f32,
                    layout.stereo_mode.bottom() as f32,
                ),
                MenuOwner::StereoMode,
            ));
        }
    }
}
