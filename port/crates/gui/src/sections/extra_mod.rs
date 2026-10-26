// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The column left of the main area: the four macro knobs, each over its
//! modulation source button, and the pitch and mod wheel sources. Ports
//! `ExtraModSection` (`vital/src/interface/editor_sections/
//! extra_mod_section.cpp`) and `MacroKnobSection`
//! (`macro_knob_section.cpp`).
//!
//! Dragging a source onto a control (G4b), and the pencil that edits a
//! macro's name (with the icons' follow-up) come later; the names show the
//! synth's.

use super::Panel;
use crate::controls::modulation_button::{ButtonStyle, ModulationButton, ModulationTabSelector};
use crate::controls::slider::{Placement, Slider, SliderStyle};
use crate::fonts::Face;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::section::{
    draw_label_background, label_background_bounds, paint_body, paint_border, paint_tab_shadow,
};
use crate::skin::{ColorId, Section, ValueId};

/// `MacroKnobSection`'s skin override (`setSkinOverride(Skin::kMacro)`).
pub const MACRO_SKIN: &[Section] = &[Section::Macro];

/// `vital::kNumMacros`.
pub const NUM_MACROS: usize = 4;

const MACRO_NAMES: [&str; NUM_MACROS] = [
    "macro_control_1",
    "macro_control_2",
    "macro_control_3",
    "macro_control_4",
];

/// One macro's places (`SingleMacroSection::resized`), in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacroLayout {
    pub bounds: Rect,
    pub knob: Rect,
    /// The macro's modulation source button.
    pub source: Rect,
    pub label: Rect,
    /// The pencil that edits the name (`placeRotaryOption`).
    pub edit: Rect,
}

/// `ExtraModSection::resized` and its children's, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtraModLayout {
    pub bounds: Rect,
    pub macros_area: Rect,
    pub macros: [MacroLayout; NUM_MACROS],
    /// The pitch and mod wheel sources (`other_modulations_`).
    pub other: Rect,
}

/// `SynthSection::placeRotaryOption`: a small square off the knob's centre.
pub fn rotary_option_bounds(scope: &Scope, knob: Rect) -> Rect {
    let width = scope.value(ValueId::RotaryOptionWidth) as i32;
    let offset_x = scope.value(ValueId::RotaryOptionXOffset) as i32 - width / 2;
    let offset_y = scope.value(ValueId::RotaryOptionYOffset) as i32 - width / 2;
    let centre_x = knob.x + knob.width / 2;
    let centre_y = knob.y + knob.height / 2;
    Rect::new(centre_x + offset_x, centre_y + offset_y, width, width)
}

impl ExtraModLayout {
    /// `top` is the interface's scope; the macros take the macro skin.
    pub fn new(top: &Scope, bounds: Rect) -> ExtraModLayout {
        let padding = top.padding() as i32;
        let knob_section_height = top.knob_section_height() as i32;
        let widget_margin = top.widget_margin() as i32;
        let macro_height = 4 * (2 * knob_section_height - widget_margin + padding) - padding;
        let mod_height = bounds.height - macro_height - padding;
        let macros_area = Rect::new(bounds.x, bounds.y, bounds.width, macro_height);
        let other = Rect::new(
            bounds.x,
            bounds.y + macro_height + padding,
            bounds.width,
            mod_height,
        );

        // MacroKnobSection::resized, in the macro skin.
        let scope = top.within(MACRO_SKIN);
        let padding = scope.padding() as i32;
        let knob_section_height = scope.knob_section_height() as i32;
        let widget_margin = scope.widget_margin() as i32;
        let mut sections = [Rect::default(); NUM_MACROS];
        let mut y = 0;
        for (i, section) in sections.iter_mut().enumerate() {
            let next_y =
                (i + 1) as f32 * (2 * knob_section_height - widget_margin + padding) as f32;
            *section = Rect::new(
                0,
                y,
                bounds.width,
                (next_y - y as f32 - padding as f32) as i32,
            );
            y = next_y as i32;
        }
        let last_y = sections[NUM_MACROS - 2].bottom() + padding;
        sections[NUM_MACROS - 1] = Rect::new(0, last_y, bounds.width, macro_height - last_y);

        let macros = sections.map(|section| {
            // SingleMacroSection::resized.
            let knob_height = section.height / 2;
            let knob = Rect::new(0, 0, section.width, knob_height);
            let source = Rect::new(0, knob_height, section.width, section.height - knob_height);
            let at = |r: Rect| r.translated(macros_area.x + section.x, macros_area.y + section.y);
            MacroLayout {
                bounds: at(Rect::new(0, 0, section.width, section.height)),
                knob: at(knob),
                source: at(source),
                label: at(label_background_bounds(&scope, knob, false)),
                edit: at(rotary_option_bounds(&scope, knob)),
            }
        });
        ExtraModLayout {
            bounds,
            macros_area,
            macros,
            other,
        }
    }
}

/// The column's controls.
pub struct ExtraModSection {
    knobs: [Slider; NUM_MACROS],
    sources: [ModulationButton; NUM_MACROS],
    other: ModulationTabSelector,
}

impl Default for ExtraModSection {
    fn default() -> ExtraModSection {
        let mut other = ModulationTabSelector::numbered(&["pitch_wheel", "mod_wheel"]);
        other.buttons[0].text_override = Some("PITCH WHL");
        other.buttons[1].text_override = Some("MOD WHL");
        other.draw_borders = true;
        ExtraModSection {
            knobs: MACRO_NAMES.map(|name| {
                Slider::new(name, SliderStyle::Rotary).popup_placement(Placement::Right)
            }),
            sources: MACRO_NAMES.map(|name| {
                let mut button = ModulationButton::new(name);
                button.text_override = Some("");
                button
            }),
            other,
        }
    }
}

impl ExtraModSection {
    /// `ExtraModSection::paintBackgroundShadow` and `paintBackground`: the
    /// sections' shadows, then each macro (`SingleMacroSection::
    /// paintBackground`) and the wheel sources.
    pub fn paint_background(&self, pixels: &PixelPainter, top: &Scope, layout: &ExtraModLayout) {
        let scope = top.within(MACRO_SKIN);
        paint_tab_shadow(pixels, top, layout.other);
        for single in &layout.macros {
            paint_tab_shadow(pixels, &scope, single.bounds);
        }
        pixels.with_clip(layout.other, |pixels| {
            self.other
                .paint_background(pixels, top, layout.other, 2, None);
        });
        for (i, single) in layout.macros.iter().enumerate() {
            paint_body(pixels, &scope, single.bounds);
            // paintMacroSourceBackground: the source button, without text.
            let style = ButtonStyle {
                selected: false,
                connect_right: false,
                draw_border: false,
                font_size: 0.0,
            };
            self.sources[i].paint_background(pixels, &scope, single.source, style);
            draw_label_background(pixels, &scope, single.knob, false);
            self.knobs[i].paint_shadow(pixels, &scope, single.knob);
            paint_border(pixels, &scope, single.bounds);
        }
    }

    /// The macro knobs and their names.
    pub fn show(&self, panel: &mut Panel, layout: &ExtraModLayout) {
        let host = panel.host;
        let size = panel.scope.value(ValueId::LabelHeight);
        let colour = panel.scope.color(ColorId::BodyText);
        for (i, single) in layout.macros.iter().enumerate() {
            panel.slider(&self.knobs[i], single.knob);
            let name = host.model().macro_name(i);
            // MacroLabel::paint: drawText, not a PlainTextComponent.
            panel.curtailed_text(&name, single.label, Face::Regular, size, colour);
        }
    }
}
