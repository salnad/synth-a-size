// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sample section: pitch, level and pan, and the destination. Ports
//! `SampleSection` (`vital/src/interface/editor_sections/sample_section.cpp`).
//!
//! The sample's waveform and name, the browser, and the keytrack, loop,
//! random phase and bounce icon buttons come later.

use super::{MenuOwner, Panel};
use crate::controls::button::SynthButton;
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::controls::slider::{Slider, SliderStyle};
use crate::controls::transpose_quantize::{self, QuantizeColours};
use crate::fonts::Face;
use crate::layout::{PresetSelectorLayout, Rect};
use crate::paint::PixelPainter;
use crate::popup::{Menu, MenuItem};
use crate::scope::Scope;
use crate::section::{
    draw_label, label_font, paint_border, paint_container, paint_joint_control,
    paint_sideways_heading_text, place_joint_controls, place_knobs_in_area,
    slider_overlap_with_space, title_width, Heading,
};
use crate::skin::{ColorId, Section, ValueId};

use super::oscillator::{DESTINATION_MENU_NAMES, DESTINATION_NAMES};

/// The section's skin override (`setSkinOverride(Skin::kSample)`).
pub const SKIN: &[Section] = &[Section::Sample];

/// The section's width ratio for its pitch and level areas (`0.19f`).
const SECTION_WIDTH_RATIO: f32 = 0.19;
/// `SynthSection::kTransposeMouseSensitivity`.
const TRANSPOSE_MOUSE_SENSITIVITY: f32 = 0.2;

/// `SampleSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleLayout {
    pub bounds: Rect,
    pub transpose: Rect,
    pub transpose_quantize: Rect,
    pub tune: Rect,
    pub level: Rect,
    pub pan: Rect,
    pub sample_viewer: Rect,
    pub preset_selector: PresetSelectorLayout,
    /// The destination text and its selector.
    pub destination: Rect,
    pub prev_destination: Rect,
    pub next_destination: Rect,
    pub keytrack: Rect,
    pub loop_button: Rect,
    pub random_phase: Rect,
    pub bounce: Rect,
    /// `paintJointControl`'s pitch area.
    pub pitch_area: Rect,
    /// The text-component background behind the four buttons.
    pub buttons_area: Rect,
}

impl SampleLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> SampleLayout {
        let width = bounds.width;
        let height = bounds.height;
        let title_width = title_width(scope) as i32;
        let widget_margin = scope.widget_margin() as i32;
        let pitch_x = title_width;
        let pitch_width = (width as f32 * SECTION_WIDTH_RATIO) as i32;
        let sample_x = pitch_x + pitch_width;
        let section2_x = width - 2 * pitch_width + widget_margin;
        // getSliderWidth() is a float here, truncated with the sum.
        let slider_x = (section2_x as f32 + widget_margin as f32
            - scope.value(ValueId::SliderWidth)
            + slider_overlap_with_space(scope) as f32) as i32;
        let sample_width = slider_x - sample_x + slider_overlap_with_space(scope);
        let label_height = scope.value(ValueId::LabelBackgroundHeight) as i32;
        let top_row_y = widget_margin;
        let level_pan_width = pitch_width;

        let destination_x = pitch_x + widget_margin;
        let destination_y = height - label_height - widget_margin;
        let destination = Rect::new(
            destination_x,
            destination_y,
            pitch_width - 2 * widget_margin,
            label_height,
        );
        let prev_destination = Rect::new(destination_x, destination_y, label_height, label_height);
        let next_destination = Rect::new(
            destination.right() - label_height,
            destination_y,
            label_height,
            label_height,
        );

        let text_component_height = destination.y - top_row_y - widget_margin;
        let pitch_area = Rect::new(
            pitch_x + widget_margin,
            top_row_y,
            pitch_width - 2 * widget_margin,
            text_component_height,
        );
        let (transpose, tune, transpose_quantize) = place_joint_controls(
            scope,
            pitch_area.x,
            pitch_area.y,
            pitch_area.width,
            pitch_area.height,
        );
        let knobs =
            place_knobs_in_area(scope, Rect::new(section2_x, 0, level_pan_width, height), 2);
        let sample_viewer = Rect::new(
            sample_x,
            title_width - widget_margin,
            sample_width,
            height - title_width,
        );
        let preset_selector = Rect::new(
            sample_x,
            widget_margin,
            sample_width,
            title_width - 2 * widget_margin,
        );

        let buttons_x = section2_x + level_pan_width;
        let buttons_width = width - buttons_x - widget_margin;
        let buttons_height = height - 2 * widget_margin;
        let button_width = (buttons_height / 2).min(buttons_width / 2);
        let button_padding_x = (buttons_width - 2 * button_width) / 3;
        let button_x1 = buttons_x + button_padding_x;
        let button_x2 = buttons_x + buttons_width - button_width - button_padding_x;
        let lower_y = widget_margin + buttons_height / 2;
        let button = |x, y| Rect::new(x, y, button_width, button_width);

        // paintBackground's buttons background, from its own section width.
        let component_width = pitch_width - 2 * widget_margin;
        let buttons_area = Rect::new(
            buttons_x,
            widget_margin,
            component_width,
            height - 2 * widget_margin,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        let at = |r: Rect| r.translated(dx, dy);
        SampleLayout {
            bounds,
            transpose: at(transpose),
            transpose_quantize: at(transpose_quantize),
            tune: at(tune),
            level: at(knobs[0]),
            pan: at(knobs[1]),
            sample_viewer: at(sample_viewer),
            preset_selector: PresetSelectorLayout::new(at(preset_selector)),
            destination: at(destination),
            prev_destination: at(prev_destination),
            next_destination: at(next_destination),
            keytrack: at(button(button_x1, widget_margin)),
            loop_button: at(button(button_x1, lower_y)),
            random_phase: at(button(button_x2, widget_margin)),
            bounce: at(button(button_x2, lower_y)),
            pitch_area: at(pitch_area),
            buttons_area: at(buttons_area),
        }
    }
}

/// The section's controls (`SampleSection`'s constructor).
pub struct SampleSection {
    on: SynthButton,
    transpose: Slider,
    tune: Slider,
    level: Slider,
    pan: Slider,
}

impl Default for SampleSection {
    fn default() -> SampleSection {
        let text = |name| Slider::new(name, SliderStyle::Text).text_entry(1.0, Some(0.0));
        SampleSection {
            on: SynthButton::power("sample_on"),
            transpose: text("sample_transpose").sensitivity(TRANSPOSE_MOUSE_SENSITIVITY),
            tune: text("sample_tune").display(3, 0),
            level: Slider::new("sample_level", SliderStyle::Rotary),
            pan: Slider::new("sample_pan", SliderStyle::Rotary).bipolar(),
        }
    }
}

impl SampleSection {
    /// `SampleSection::paintBackground`.
    pub fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, layout: &SampleLayout) {
        if layout.bounds.width == 0 {
            return;
        }
        paint_container(pixels, scope, layout.bounds, Heading::Sideways);
        paint_sideways_heading_text(pixels, scope, "SMP", layout.bounds, true);
        self.level.paint_shadow(pixels, scope, layout.level);
        self.pan.paint_shadow(pixels, scope, layout.pan);
        draw_label(pixels, scope, "PAN", layout.pan, false);
        draw_label(pixels, scope, "LEVEL", layout.level, false);

        paint_joint_control(pixels, scope, layout.pitch_area, "");
        let label_height = scope.value(ValueId::LabelBackgroundHeight) as i32;
        let (face, size, colour) = label_font(scope);
        let pitch = layout.pitch_area;
        pixels.text_centred(
            "PITCH",
            Rect::new(
                pitch.x,
                layout.bounds.y + scope.widget_margin() as i32,
                pitch.width,
                label_height,
            ),
            face,
            size,
            colour,
        );

        let rounding = scope.value(ValueId::LabelBackgroundRounding);
        let text_background = scope.color(ColorId::TextComponentBackground);
        pixels.fill_rounded_rect(layout.destination, rounding, text_background);
        pixels.fill_rounded_rect(layout.buttons_area, rounding, text_background);
        let viewer = layout.sample_viewer;
        let margin = scope.widget_margin() as i32;
        pixels.fill_rounded_rect(
            Rect::new(
                viewer.x,
                layout.bounds.y + margin,
                viewer.width,
                layout.bounds.height - 2 * margin,
            ),
            scope.value(ValueId::WidgetRoundedCorner),
            scope.color(ColorId::WidgetBackground),
        );

        // paintChildrenBackgrounds: the sample name's selector.
        pixels.fill_rounded_rect(
            layout.preset_selector.bounds,
            scope.value(ValueId::WidgetRoundedCorner),
            scope.color(ColorId::PopupSelectorBackground),
        );
        paint_border(pixels, scope, layout.bounds);
    }

    /// The controls, the destination and the power button with the
    /// section's off overlay.
    pub fn show(&self, panel: &mut Panel, layout: &SampleLayout) {
        let host = panel.host;
        let active = self.on.is_on(host);
        for (slider, bounds) in [
            (&self.transpose, layout.transpose),
            (&self.tune, layout.tune),
            (&self.level, layout.level),
            (&self.pan, layout.pan),
        ] {
            let mut slider = *slider;
            slider.active = active;
            panel.slider(&slider, bounds);
        }

        let quantize = host.value("sample_transpose_quantize");
        let bounds = layout.transpose_quantize;
        let colours = QuantizeColours::new(&panel.scope);
        panel.layer().custom(move |pixels| {
            transpose_quantize::paint(pixels, colours, bounds, quantize, false)
        });

        let ui_colours = ShapeColours::ui(&panel.scope);
        let preset = layout.preset_selector;
        panel.shape_button(
            "sample prev",
            ShapeButton {
                shape: Shape::Prev,
                colours: ui_colours,
            },
            preset.previous,
        );
        panel.shape_button(
            "sample next",
            ShapeButton {
                shape: Shape::Next,
                colours: ui_colours,
            },
            preset.next,
        );

        let destination =
            (host.value("sample_destination") as usize).min(DESTINATION_NAMES.len() - 1);
        panel.text_centred(
            DESTINATION_NAMES[destination],
            layout.destination,
            Face::Regular,
            panel.scope.value(ValueId::LabelHeight),
            panel.scope.color(ColorId::BodyText),
        );
        let arrows = ShapeColours::icon(&panel.scope);
        let invisible = ShapeButton {
            shape: Shape::None,
            colours: arrows,
        };
        if panel
            .shape_button("sample destination menu", invisible, layout.destination)
            .pressed
        {
            let items = DESTINATION_MENU_NAMES
                .iter()
                .enumerate()
                .map(|(i, name)| MenuItem::new(i as i32, *name))
                .collect();
            panel.overlay.menu = Some(Menu::new(
                items,
                egui::pos2(
                    layout.destination.x as f32,
                    layout.destination.bottom() as f32,
                ),
                MenuOwner::Control {
                    name: "sample_destination",
                    then: None,
                },
            ));
        }
        let count = DESTINATION_NAMES.len();
        let prev = ShapeButton {
            shape: Shape::Prev,
            colours: arrows,
        };
        let next = ShapeButton {
            shape: Shape::Next,
            colours: arrows,
        };
        if panel
            .shape_button("sample prev destination", prev, layout.prev_destination)
            .clicked
        {
            host.set_once(
                "sample_destination",
                ((destination + count - 1) % count) as f32,
            );
        }
        if panel
            .shape_button("sample next destination", next, layout.next_destination)
            .clicked
        {
            host.set_once("sample_destination", ((destination + 1) % count) as f32);
        }

        panel.activator(&self.on, layout.bounds);
    }
}
