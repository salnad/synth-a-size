// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! A filter section: the response curve with its cutoff, resonance and
//! blend sliders, drive, mix and key tracking (or the formant and comb
//! controls in their places), the model and style, and on the Voice tab the
//! sources it filters. Ports `FilterSection`
//! (`vital/src/interface/editor_sections/filter_section.cpp`) for the Voice
//! tab's FILTER 1 and 2 (`positionTopBottom`) and the Effects tab's filter
//! (`positionLeftRight`, [`FilterSection::fx`]).
//!
//! The blend arrows' icons come with `controls::icon`.

use std::cell::RefCell;

use egui::Sense;

use super::oscillator::{NAMES as OSCILLATOR_NAMES, NUM_OSCILLATORS};
use super::{MenuOwner, Panel, FILTER_STYLE_IDS};
use crate::controls::button::SynthButton;
use crate::controls::filter_view::{self, model, FilterInputs, Response};
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::controls::slider::{Placement, Slider, SliderStyle};
use crate::fonts::Face;
use crate::host::Host;
use crate::layout::{PresetSelectorLayout, Rect};
use crate::paint::{PixelPainter, RectF};
use crate::popup::{DualMenu, DualMenuGroup, MenuItem};
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadBatch};
use crate::scope::Scope;
use crate::section::{
    draw_label, draw_label_background, draw_text_component_background, label_background_bounds,
    paint_border, paint_container, paint_heading_text, paint_sideways_heading_text,
    paint_widget_background, place_knobs_in_area, slider_overlap, slider_overlap_with_space,
    slider_width, title_bounds, title_width, Heading,
};
use crate::skin::{ColorId, Section, ValueId};

/// The Voice tab's filters' skin override (`setSkinOverride(Skin::kFilter)`).
pub const SKIN: &[Section] = &[Section::Filter];
/// The effects filter's (`setSkinOverride(Skin::kFxFilter)`).
pub const FX_SKIN: &[Section] = &[Section::FxFilter];

/// `FilterSection::kBlendLabelWidth`.
const BLEND_LABEL_WIDTH: f32 = 30.0;
/// `SynthSection::kDefaultDualPopupMenuWidth`.
const DUAL_POPUP_WIDTH: f32 = 340.0;
/// `strings::kFilterModelNames`.
pub const MODEL_NAMES: [&str; model::COUNT] = [
    "Analog", "Dirty", "Ladder", "Digital", "Diode", "Formant", "Comb", "Phaser",
];
const FILTER_STYLE_NAMES: [&str; 5] = ["12dB", "24dB", "Notch Blend", "Notch Spread", "B/P/N"];
const DIODE_STYLE_NAMES: [&str; 2] = ["Low Shelf", "Low Cut"];
const COMB_STYLE_NAMES: [&str; 6] = [
    "Low High Comb",
    "Low High Flange+",
    "Low High Flange-",
    "Band Spread Comb",
    "Band Spread Flange+",
    "Band Spread Flange-",
];
/// `FormantFilter::kVocalTract`.
const VOCAL_TRACT: i32 = 2;

/// `getNumStyles`.
pub fn num_styles(model: usize) -> i32 {
    match model {
        model::DIODE | model::FORMANT | model::PHASE => 2,
        model::COMB => 6,
        model::ANALOG | model::DIRTY | model::LADDER | model::DIGITAL => 5,
        _ => 0,
    }
}

/// `getStyleName`.
pub fn style_name(model: usize, style: i32) -> &'static str {
    let style = style.max(0) as usize;
    match model {
        model::DIODE => DIODE_STYLE_NAMES.get(style).copied().unwrap_or(""),
        model::FORMANT => match style as i32 {
            VOCAL_TRACT => "The Mouth",
            1 => "AIUO",
            _ => "AOIE",
        },
        model::COMB => COMB_STYLE_NAMES.get(style).copied().unwrap_or(""),
        model::PHASE => {
            if style != 0 {
                "Negative"
            } else {
                "Positive"
            }
        }
        model::ANALOG | model::DIRTY | model::LADDER | model::DIGITAL => {
            FILTER_STYLE_NAMES.get(style).copied().unwrap_or("")
        }
        _ => "",
    }
}

/// `SourceDestination` values, and `constants::toggleFilter1` / `2`.
pub mod destination {
    pub const FILTER_1: usize = 0;
    pub const FILTER_2: usize = 1;
    pub const DUAL_FILTERS: usize = 2;
    pub const EFFECTS: usize = 3;

    /// Whether a source going to `destination` feeds filter `index` (1 or 2).
    pub fn feeds(destination: usize, index: usize) -> bool {
        destination == DUAL_FILTERS || destination == index - 1
    }

    /// The destination after turning filter `index`'s input on or off.
    pub fn toggle(destination: usize, index: usize, on: bool) -> usize {
        let (this, other) = if index == 1 {
            (FILTER_1, FILTER_2)
        } else {
            (FILTER_2, FILTER_1)
        };
        if on {
            if destination == other {
                DUAL_FILTERS
            } else {
                this
            }
        } else if destination == DUAL_FILTERS {
            other
        } else if destination == this {
            EFFECTS
        } else {
            destination
        }
    }
}

/// Which filter a section shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The Voice tab's FILTER 1 or 2.
    Voice(usize),
    /// The Effects tab's filter.
    Fx,
}

/// A filter's parameter names.
#[derive(Clone, Copy, Debug)]
pub struct Names {
    pub on: &'static str,
    pub model: &'static str,
    pub style: &'static str,
    pub cutoff: &'static str,
    pub resonance: &'static str,
    pub drive: &'static str,
    pub mix: &'static str,
    pub blend: &'static str,
    pub blend_transpose: &'static str,
    pub keytrack: &'static str,
    pub formant_x: &'static str,
    pub formant_y: &'static str,
    pub formant_transpose: &'static str,
    pub formant_resonance: &'static str,
    pub formant_spread: &'static str,
    pub filter_input: &'static str,
}

macro_rules! names {
    ($n:literal) => {
        Names {
            on: concat!("filter_", $n, "_on"),
            model: concat!("filter_", $n, "_model"),
            style: concat!("filter_", $n, "_style"),
            cutoff: concat!("filter_", $n, "_cutoff"),
            resonance: concat!("filter_", $n, "_resonance"),
            drive: concat!("filter_", $n, "_drive"),
            mix: concat!("filter_", $n, "_mix"),
            blend: concat!("filter_", $n, "_blend"),
            blend_transpose: concat!("filter_", $n, "_blend_transpose"),
            keytrack: concat!("filter_", $n, "_keytrack"),
            formant_x: concat!("filter_", $n, "_formant_x"),
            formant_y: concat!("filter_", $n, "_formant_y"),
            formant_transpose: concat!("filter_", $n, "_formant_transpose"),
            formant_resonance: concat!("filter_", $n, "_formant_resonance"),
            formant_spread: concat!("filter_", $n, "_formant_spread"),
            filter_input: concat!("filter_", $n, "_filter_input"),
        }
    };
}

const VOICE_NAMES: [Names; 2] = [names!("1"), names!("2")];
const FX_NAMES: Names = names!("fx");

/// `positionTopBottom` / `positionLeftRight`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilterLayout {
    pub bounds: Rect,
    pub kind: Kind,
    pub response: Rect,
    pub blend: Rect,
    pub cutoff: Rect,
    pub resonance: Rect,
    pub drive: Rect,
    pub mix: Rect,
    pub keytrack: Rect,
    /// The labels under drive and key tracking (`label1`, `label2`).
    pub drive_label: Rect,
    pub keytrack_label: Rect,
    pub preset_selector: PresetSelectorLayout,
    /// The Voice filters' inputs: OSC1, OSC2, OSC3, SMP, and the other
    /// filter.
    pub inputs: Option<[Rect; 5]>,
    pub title: Rect,
}

impl FilterLayout {
    pub fn new(scope: &Scope, bounds: Rect, kind: Kind) -> FilterLayout {
        match kind {
            Kind::Voice(_) => Self::top_bottom(scope, bounds, kind),
            Kind::Fx => Self::left_right(scope, bounds),
        }
    }

    /// `positionTopBottom`.
    fn top_bottom(scope: &Scope, bounds: Rect, kind: Kind) -> FilterLayout {
        let (width, height) = (bounds.width, bounds.height);
        let title_width = title_width(scope) as i32;
        let knob_section_height = scope.knob_section_height() as i32;
        let slider_width = slider_width(scope);
        let blend_label_width = (scope.size_ratio() * BLEND_LABEL_WIDTH) as i32;
        let widget_margin = scope.widget_margin() as i32;
        let slider_overlap = slider_overlap(scope);
        let slider_overlap_space = slider_overlap_with_space(scope);
        let response_width =
            width - slider_width + slider_overlap + slider_overlap_space - 2 * widget_margin;
        let response_y = title_width + slider_width - slider_overlap_space - slider_overlap;
        let response_height = height - 2 * slider_width - title_width - knob_section_height
            + 2 * slider_overlap_space
            + 2 * slider_overlap;
        let blend_y = title_width - slider_overlap;
        let blend = Rect::new(
            blend_label_width - widget_margin,
            blend_y,
            width - 2 * (blend_label_width - widget_margin),
            slider_width,
        );
        let response = Rect::new(widget_margin, response_y, response_width, response_height);
        let resonance_x = response.right() - slider_overlap + widget_margin;
        let resonance = Rect::new(
            resonance_x,
            response_y - widget_margin,
            slider_width,
            response_height + 2 * widget_margin,
        );
        let cutoff_y = response.bottom() - slider_overlap + widget_margin;
        let cutoff = Rect::new(
            0,
            cutoff_y,
            response_width + 2 * widget_margin,
            slider_width,
        );

        let component_width = width as f32 / 5.0;
        let knob_y = height - knob_section_height;
        let inputs_width = (2.0 * component_width) as i32;
        let internal_margin = widget_margin / 2;
        let input_width = (inputs_width - 2 * widget_margin - internal_margin) / 2;
        let input_height =
            (knob_section_height - 2 * (widget_margin + internal_margin)) as f32 / 3.0;
        let ih = input_height as i32;
        let osc_y = knob_y + widget_margin;
        let other_y = (knob_y as f32 + (knob_section_height as f32 - input_height) / 2.0) as i32;
        let right_x = inputs_width - widget_margin - input_width;
        let filter_x = (inputs_width - input_width) / 2;
        let filter_y = (height as f32 - input_height - widget_margin as f32) as i32;
        let inputs = [
            Rect::new(widget_margin, osc_y, input_width, ih),
            Rect::new(right_x, osc_y, input_width, ih),
            Rect::new(widget_margin, other_y, input_width, ih),
            Rect::new(right_x, other_y, input_width, ih),
            Rect::new(filter_x, filter_y, input_width, ih),
        ];

        let knobs_x = (2.0 * component_width - widget_margin as f32) as i32;
        let knobs = place_knobs_in_area(
            scope,
            Rect::new(knobs_x, knob_y, width - knobs_x, knob_section_height),
            3,
        );

        // SynthSection::getPresetBrowserBounds.
        let browser_x = (width as f32 * 0.4) as i32;
        let preset = Rect::new(
            browser_x,
            widget_margin,
            width - browser_x - widget_margin,
            title_width - 2 * widget_margin,
        );
        let title = title_bounds(scope, Rect::new(0, 0, width, height), true, browser_x);

        let at = |r: Rect| r.translated(bounds.x, bounds.y);
        FilterLayout {
            bounds,
            kind,
            response: at(response),
            blend: at(blend),
            cutoff: at(cutoff),
            resonance: at(resonance),
            drive: at(knobs[0]),
            mix: at(knobs[1]),
            keytrack: at(knobs[2]),
            drive_label: at(label_background_bounds(scope, knobs[0], false)),
            keytrack_label: at(label_background_bounds(scope, knobs[2], false)),
            preset_selector: PresetSelectorLayout::new(at(preset)),
            inputs: Some(inputs.map(at)),
            title: at(title),
        }
    }

    /// `positionLeftRight`.
    fn left_right(scope: &Scope, bounds: Rect) -> FilterLayout {
        let (width, height) = (bounds.width, bounds.height);
        let title_width = title_width(scope) as i32;
        let knob_section_height = scope.knob_section_height() as i32;
        let slider_width = slider_width(scope);
        let blend_label_width = (scope.size_ratio() * BLEND_LABEL_WIDTH) as i32;
        let widget_margin = scope.widget_margin() as i32;
        // getDividedAreaUnbuffered(bounds.withLeft(title_width), 2, 1, margin).
        let area_width = width - title_width;
        let component_width = (area_width - 3 * widget_margin) as f32 / 2.0;
        let response_x = title_width
            + (component_width + widget_margin as f32 + widget_margin as f32).round() as i32;
        let slider_overlap = slider_overlap_with_space(scope);
        let response_area_width = width - response_x;
        let response_width = response_area_width - slider_width + 2 * slider_overlap;
        let response_y = slider_width - 2 * slider_overlap;
        let response_height = height - 2 * slider_width + 4 * slider_overlap;
        let response = Rect::new(response_x, response_y, response_width, response_height);
        let blend = Rect::new(
            response_x + blend_label_width - 2 * widget_margin,
            -slider_overlap,
            response_area_width - 2 * (blend_label_width - widget_margin),
            slider_width,
        );
        let resonance = Rect::new(
            width - slider_width + slider_overlap,
            response_y - widget_margin,
            slider_width,
            response_height + 2 * widget_margin,
        );
        let cutoff = Rect::new(
            response_x - widget_margin,
            response.bottom() - slider_overlap,
            response_width + 2 * widget_margin,
            slider_width,
        );
        let knob_area_width = response_x - title_width;
        let knobs_y = height - knob_section_height;
        let knobs = place_knobs_in_area(
            scope,
            Rect::new(title_width, knobs_y, knob_area_width, knob_section_height),
            3,
        );
        let preset = Rect::new(
            title_width + widget_margin,
            widget_margin,
            knob_area_width - 2 * widget_margin,
            knob_section_height - 2 * widget_margin,
        );

        let at = |r: Rect| r.translated(bounds.x, bounds.y);
        FilterLayout {
            bounds,
            kind: Kind::Fx,
            response: at(response),
            blend: at(blend),
            cutoff: at(cutoff),
            resonance: at(resonance),
            drive: at(knobs[0]),
            mix: at(knobs[1]),
            keytrack: at(knobs[2]),
            drive_label: at(label_background_bounds(scope, knobs[0], false)),
            keytrack_label: at(label_background_bounds(scope, knobs[2], false)),
            preset_selector: text_component_selector(scope, at(preset)),
            inputs: None,
            title: at(Rect::new(0, 0, title_width, height)),
        }
    }
}

/// `PresetSelector::resized` with a text component (`setTextComponent`):
/// arrows the size of the text component font, centred and offset.
fn text_component_selector(scope: &Scope, bounds: Rect) -> PresetSelectorLayout {
    let button_height = scope.value(ValueId::TextComponentFontSize) as i32;
    let offset = scope.value(ValueId::TextComponentOffset) as i32;
    let button_y = (bounds.height - button_height) / 2 + offset;
    PresetSelectorLayout {
        bounds,
        text: bounds.translated(0, offset),
        text_size: button_height as f32,
        previous: Rect::new(bounds.x, bounds.y + button_y, button_height, button_height),
        next: Rect::new(
            bounds.right() - button_height,
            bounds.y + button_y,
            button_height,
            button_height,
        ),
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DragState {
    down: bool,
    last: egui::Pos2,
    x_value: f32,
    y_value: f32,
}

/// One filter section's controls.
pub struct FilterSection {
    kind: Kind,
    names: Names,
    on: SynthButton,
    cutoff: Slider,
    resonance: Slider,
    drive: Slider,
    mix: Slider,
    blend: Slider,
    blend_transpose: Slider,
    keytrack: Slider,
    formant_x: Slider,
    formant_y: Slider,
    formant_transpose: Slider,
    formant_resonance: Slider,
    formant_spread: Slider,
    filter_input: SynthButton,
    /// The last response drawn and the inputs it was drawn for.
    response: RefCell<Option<(FilterInputs, Response)>>,
}

impl FilterSection {
    /// The Voice tab's FILTER `index` (1 or 2).
    pub fn voice(index: usize) -> FilterSection {
        let text = if index == 1 { "FIL2" } else { "FIL1" };
        FilterSection::new(Kind::Voice(index), VOICE_NAMES[index - 1], text)
    }

    /// The Effects tab's filter (`FilterSection("fx", ...)`).
    pub fn fx() -> FilterSection {
        FilterSection::new(Kind::Fx, FX_NAMES, "")
    }

    fn new(kind: Kind, names: Names, filter_input_text: &'static str) -> FilterSection {
        let linear = |name, prefix| {
            Slider::new(name, SliderStyle::Horizontal)
                .popup_placement(Placement::Below)
                .popup_prefix(prefix)
        };
        let vertical = |name, prefix| {
            Slider::new(name, SliderStyle::Vertical)
                .popup_placement(Placement::Right)
                .popup_prefix(prefix)
        };
        FilterSection {
            kind,
            names,
            on: SynthButton::power(names.on),
            cutoff: linear(names.cutoff, "Cutoff: ")
                .hz_alternate()
                .modulation_placement(Placement::Above),
            formant_x: linear(names.formant_x, "Formant X: ")
                .modulation_placement(Placement::Above),
            mix: Slider::new(names.mix, SliderStyle::Rotary),
            blend: linear(names.blend, "Blend: ")
                .bipolar()
                .popup_placement(Placement::Above)
                .snap_to_value(1.0),
            formant_transpose: linear(names.formant_transpose, "Formant Transpose: ")
                .bipolar()
                .popup_placement(Placement::Above)
                .snap_to_value(0.0),
            resonance: vertical(names.resonance, "Resonance: ")
                .modulation_placement(Placement::Left),
            formant_y: vertical(names.formant_y, "Formant Y: ")
                .modulation_placement(Placement::Left),
            drive: Slider::new(names.drive, SliderStyle::Rotary),
            formant_resonance: Slider::new(names.formant_resonance, SliderStyle::Rotary),
            formant_spread: Slider::new(names.formant_spread, SliderStyle::Rotary)
                .bipolar()
                .snap_to_value(0.0),
            blend_transpose: Slider::new(names.blend_transpose, SliderStyle::Rotary),
            keytrack: Slider::new(names.keytrack, SliderStyle::Rotary)
                .bipolar()
                .snap_to_value(0.0),
            filter_input: SynthButton::text(names.filter_input, filter_input_text),
            response: RefCell::new(None),
        }
    }

    pub fn skin(&self) -> &'static [Section] {
        match self.kind {
            Kind::Voice(_) => SKIN,
            Kind::Fx => FX_SKIN,
        }
    }

    pub fn layout(&self, scope: &Scope, bounds: Rect) -> FilterLayout {
        FilterLayout::new(scope, bounds, self.kind)
    }

    fn model(&self, host: &Host) -> usize {
        (host.value(self.names.model).round() as usize).min(model::COUNT - 1)
    }

    fn style(&self, host: &Host) -> i32 {
        host.value(self.names.style).round() as i32
    }

    /// `SynthSection::paintBackground` with `FilterSection`'s labels.
    pub fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, layout: &FilterLayout) {
        let heading = match self.kind {
            Kind::Voice(_) => Heading::Horizontal,
            Kind::Fx => Heading::Sideways,
        };
        paint_container(pixels, scope, layout.bounds, heading);
        match self.kind {
            Kind::Voice(index) => {
                paint_heading_text(pixels, scope, &format!("FILTER {index}"), layout.title)
            }
            Kind::Fx => paint_sideways_heading_text(pixels, scope, "FILTER", layout.bounds, true),
        }
        // paintKnobShadows: the knobs in the drive, mix and keytrack places.
        for (slider, bounds) in [
            (&self.drive, layout.drive),
            (&self.mix, layout.mix),
            (&self.keytrack, layout.keytrack),
        ] {
            slider.paint_shadow(pixels, scope, bounds);
        }
        // paintChildrenBackgrounds: the model selector and the response.
        let preset = layout.preset_selector.bounds;
        match self.kind {
            Kind::Voice(_) => pixels.fill_rounded_rect(
                preset,
                scope.value(ValueId::WidgetRoundedCorner),
                scope.color(ColorId::PopupSelectorBackground),
            ),
            Kind::Fx => {
                pixels.fill_rect(preset, scope.color(ColorId::Body));
                draw_text_component_background(pixels, scope, preset, true);
                draw_label(pixels, scope, "MODE", preset, true);
            }
        }
        paint_widget_background(pixels, scope, layout.response);
        paint_border(pixels, scope, layout.bounds);

        draw_label(pixels, scope, "MIX", layout.mix, false);
        draw_label_background(pixels, scope, layout.drive, false);
        draw_label_background(pixels, scope, layout.keytrack, false);
    }

    /// The controls `showModelKnobs` leaves visible, the labels, the
    /// selector and inputs, and the response.
    pub fn show(&self, panel: &mut Panel, layout: &FilterLayout) {
        let host = panel.host;
        let active = self.on.is_on(host);
        let model = self.model(host);
        let style = self.style(host);
        let formant = model == model::FORMANT;
        let vocal_tract = formant && style == VOCAL_TRACT;
        let comb = model == model::COMB;
        let with_active = |slider: &Slider| {
            let mut slider = *slider;
            slider.active = active;
            slider
        };
        let mut shown = Vec::new();
        if formant {
            shown.push((&self.formant_x, layout.cutoff));
            shown.push((&self.formant_y, layout.resonance));
            if !vocal_tract {
                shown.push((&self.formant_transpose, layout.blend));
            }
            shown.push((&self.formant_resonance, layout.drive));
            shown.push((&self.formant_spread, layout.keytrack));
        } else {
            shown.push((&self.cutoff, layout.cutoff));
            shown.push((&self.resonance, layout.resonance));
            shown.push((&self.keytrack, layout.keytrack));
            if comb {
                shown.push((&self.blend_transpose, layout.drive));
            } else {
                shown.push((&self.drive, layout.drive));
            }
        }
        if !formant || vocal_tract {
            shown.push((&self.blend, layout.blend));
        }
        shown.push((&self.mix, layout.mix));

        self.show_response(panel, layout, active);
        for (slider, bounds) in shown {
            panel.slider(&with_active(slider), bounds);
        }

        // setLabelText.
        let (label1, label2) = if formant {
            ("PEAK", "SPREAD")
        } else if comb {
            ("CUT", "KEY TRK")
        } else {
            ("DRIVE", "KEY TRK")
        };
        let size = panel.scope.value(ValueId::LabelHeight);
        let colour = panel.scope.color(ColorId::BodyText);
        panel.text_centred(label1, layout.drive_label, Face::Regular, size, colour);
        panel.text_centred(label2, layout.keytrack_label, Face::Regular, size, colour);

        self.show_selector(panel, layout, model, style);
        if let Some(inputs) = layout.inputs {
            self.show_inputs(panel, inputs);
        }
        panel.activator(&self.on, layout.bounds);
    }

    /// The model and style text with its arrows (`prevClicked`,
    /// `nextClicked`) and the two-column list a click on it opens.
    fn show_selector(&self, panel: &mut Panel, layout: &FilterLayout, model: usize, style: i32) {
        let host = panel.host;
        let preset = layout.preset_selector;
        let text = format!("{}  :  {}", MODEL_NAMES[model], style_name(model, style));
        panel.text_centred(
            &text,
            preset.text,
            Face::Title,
            preset.text_size,
            panel.scope.color(ColorId::PresetText),
        );
        let colours = match self.kind {
            Kind::Voice(_) => ShapeColours::ui(&panel.scope),
            Kind::Fx => ShapeColours::icon(&panel.scope),
        };
        let prev = ShapeButton {
            shape: Shape::Prev,
            colours,
        };
        let next = ShapeButton {
            shape: Shape::Next,
            colours,
        };
        let mut chosen = None;
        if panel
            .shape_button(("filter prev", self.names.model), prev, preset.previous)
            .clicked
        {
            let mut style = style - 1;
            let mut model = model;
            if style < 0 {
                model = (model + model::COUNT - 1) % model::COUNT;
                style = num_styles(model) - 1;
            }
            chosen = Some((model, style));
        }
        if panel
            .shape_button(("filter next", self.names.model), next, preset.next)
            .clicked
        {
            let mut style = style + 1;
            let mut model = model;
            if style >= num_styles(model) {
                style = 0;
                model = (model + 1) % model::COUNT;
            }
            chosen = Some((model, style));
        }
        if let Some((model, style)) = chosen {
            host.set_once(self.names.model, model as f32);
            host.set_once(self.names.style, style as f32);
        }

        // PresetSelector::textMouseDown on the text between the arrows.
        let text_area = Rect::new(
            preset.previous.right(),
            preset.bounds.y,
            preset.next.x - preset.previous.right(),
            preset.bounds.height,
        );
        let invisible = ShapeButton {
            shape: Shape::None,
            colours,
        };
        if panel
            .shape_button(("filter menu", self.names.model), invisible, text_area)
            .pressed
        {
            let width = (DUAL_POPUP_WIDTH * panel.scope.size_ratio()) as i32;
            let position = match self.kind {
                Kind::Voice(_) => egui::pos2(layout.bounds.right() as f32, preset.bounds.y as f32),
                Kind::Fx => egui::pos2(
                    (preset.bounds.right() - width) as f32,
                    preset.bounds.bottom() as f32,
                ),
            };
            let groups = (0..model::COUNT)
                .map(|m| DualMenuGroup {
                    name: MODEL_NAMES[m].to_string(),
                    items: (0..num_styles(m))
                        .map(|s| MenuItem::new(m as i32 * FILTER_STYLE_IDS + s, style_name(m, s)))
                        .collect(),
                })
                .collect();
            panel.overlay.dual_menu = Some(DualMenu::new(
                groups,
                model,
                style.max(0) as usize,
                position,
                width,
                MenuOwner::FilterType {
                    model: self.names.model,
                    style: self.names.style,
                },
            ));
        }
    }

    /// The Voice filters' source toggles (`oscInputToggled`,
    /// `sampleInputToggled`) and the other filter's input
    /// (`filterSerialSelected`).
    fn show_inputs(&self, panel: &mut Panel, inputs: [Rect; 5]) {
        let Kind::Voice(index) = self.kind else {
            return;
        };
        let host = panel.host;
        let mut sources: Vec<(&'static str, &'static str)> = (0..NUM_OSCILLATORS)
            .map(|i| (["OSC1", "OSC2", "OSC3"][i], OSCILLATOR_NAMES[i].destination))
            .collect();
        sources.push(("SMP", "sample_destination"));
        for ((text, name), bounds) in sources.into_iter().zip(inputs) {
            let current = host.value(name) as usize;
            let on = destination::feeds(current, index);
            let clicked = panel.text_toggle(("filter input", index, name), text, on, bounds);
            if clicked {
                host.set_once(name, destination::toggle(current, index, !on) as f32);
            }
        }
        // FilterSection::buttonClicked: turning this filter's input on
        // turns the other's off.
        // Read before the click: the host may apply its set a frame later.
        let was_on = self.filter_input.is_on(host);
        if panel.button(&self.filter_input, inputs[4]) && !was_on {
            let other = VOICE_NAMES[2 - index].filter_input;
            if host.value(other) != 0.0 {
                host.set_once(other, 0.0);
            }
        }
    }

    /// `FilterResponse`: the curve for both channels (the right one only
    /// while the filter is on), drawn over the response's backdrop, and its
    /// drag, which moves cutoff (or formant X) across and resonance (or
    /// formant Y) up.
    fn show_response(&self, panel: &mut Panel, layout: &FilterLayout, active: bool) {
        let host = panel.host;
        let model = self.model(host);
        let formant = model == model::FORMANT;
        let (x_slider, y_slider) = if formant {
            (&self.formant_x, &self.formant_y)
        } else {
            (&self.cutoff, &self.resonance)
        };

        let id = panel
            .ui
            .scope_id()
            .with(("filter response", self.names.model));
        let rect = panel.pixels.rect(layout.response.into());
        let response = panel.ui.interact(rect, id, Sense::click_and_drag());
        let mut state: DragState = panel.ui.data(|data| data.get_temp(id)).unwrap_or_default();
        let pointer = panel.ui.input(|input| input.pointer.interact_pos());
        if response.is_pointer_button_down_on() && !state.down {
            if let Some(pos) = pointer {
                state = DragState {
                    down: true,
                    last: pos,
                    x_value: host.value(x_slider.name),
                    y_value: host.value(y_slider.name),
                };
                host.begin(x_slider.name);
                host.begin(y_slider.name);
            }
        } else if state.down && response.is_pointer_button_down_on() {
            if let Some(pos) = pointer {
                // setFilterSettingsFromPosition: a full range across the
                // view, in JUCE's logical pixels.
                let delta = pos - state.last;
                state.last = pos;
                let ppp = panel.pixels.pixels_per_point();
                let width = layout.response.width as f32 / ppp;
                let height = layout.response.height as f32 / ppp;
                let x_range = x_slider.info.max - x_slider.info.min;
                let y_range = y_slider.info.max - y_slider.info.min;
                let x_value = (state.x_value + delta.x * x_range / width)
                    .clamp(x_slider.info.min, x_slider.info.max);
                let y_value = (state.y_value - delta.y * y_range / height)
                    .clamp(y_slider.info.min, y_slider.info.max);
                if x_value != state.x_value {
                    host.set(x_slider.name, x_value);
                    state.x_value = x_value;
                }
                if y_value != state.y_value {
                    host.set(y_slider.name, y_value);
                    state.y_value = y_value;
                }
            }
        } else if state.down {
            state.down = false;
            host.end(x_slider.name);
            host.end(y_slider.name);
        }
        panel.ui.data_mut(|data| data.insert_temp(id, state));

        let mut inputs = FilterInputs {
            model,
            style: self.style(host),
            cutoff: host.value(self.names.cutoff),
            resonance: host.value(self.names.resonance),
            mix: host.value(self.names.mix),
            blend: host.value(self.names.blend),
            blend_transpose: host.value(self.names.blend_transpose),
            formant_x: host.value(self.names.formant_x),
            formant_y: host.value(self.names.formant_y),
            formant_transpose: host.value(self.names.formant_transpose),
            formant_resonance: host.value(self.names.formant_resonance),
            formant_spread: host.value(self.names.formant_spread),
        };
        if state.down {
            // The drag's own values, ahead of the host.
            if formant {
                inputs.formant_x = state.x_value;
                inputs.formant_y = state.y_value;
            } else {
                inputs.cutoff = state.x_value;
                inputs.resonance = state.y_value;
            }
        }
        let curve = {
            let mut cache = self.response.borrow_mut();
            match &*cache {
                Some((cached, curve)) if *cached == inputs => *curve,
                _ => {
                    let curve = filter_view::response(&inputs);
                    *cache = Some((inputs, curve));
                    curve
                }
            }
        };

        let scope = panel.scope;
        let fill_fade = scope.value(ValueId::WidgetFillFade);
        let line_width = scope.value(ValueId::WidgetLineWidth);
        let fill_center = scope.value(ValueId::WidgetFillCenter);
        let colours = |line: ColorId, fill: ColorId| {
            let fill = scope.color(fill);
            (
                scope.color(line),
                fill.with_multiplied_alpha(1.0 - fill_fade),
                fill,
            )
        };
        let mut passes = Vec::new();
        if active {
            passes.push((
                1,
                colours(ColorId::WidgetPrimary2, ColorId::WidgetSecondary2),
            ));
            passes.push((
                0,
                colours(ColorId::WidgetPrimary1, ColorId::WidgetSecondary1),
            ));
        } else {
            passes.push((
                0,
                colours(
                    ColorId::WidgetPrimaryDisabled,
                    ColorId::WidgetSecondaryDisabled,
                ),
            ));
        }
        let bounds = layout.response;
        let rounding = scope.value(ValueId::WidgetRoundedCorner);
        let body = scope.color(ColorId::Body);
        panel.layer().custom(move |pixels| {
            let mut line = LineRenderer::new(filter_view::RESOLUTION, false);
            line.fill = true;
            line.line_width = line_width;
            line.fill_center = fill_center;
            for (channel, (colour, fill_from, fill_to)) in passes {
                for (i, (x, y)) in curve
                    .line(channel, bounds.width as f32, bounds.height as f32)
                    .into_iter()
                    .enumerate()
                {
                    line.set_x_at(i, x);
                    line.set_y_at(i, y);
                }
                line.color = colour;
                line.set_fill_colors(fill_from, fill_to);
                line.paint(pixels, RectF::from(bounds), true);
            }
            // renderCorners, after the curve.
            let mut corners = QuadBatch::new();
            for corner in Quad::corners(bounds.into(), rounding, body) {
                corners.push(corner);
            }
            corners.paint(pixels, RectF::from(bounds));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::Skin;

    /// The effects filter as upstream lays it out in the Effects tab at
    /// 1400×820 with every effect on (ref-gui, `all_effects.vital`).
    #[test]
    fn the_effects_filter_lays_out_left_to_right() {
        let skin = Skin::default();
        let scope = Scope::top(&skin, 1.0);
        let scope = scope.within(FX_SKIN);
        let l = FilterLayout::new(&scope, Rect::new(240, 753, 552, 137), Kind::Fx);
        assert_eq!(l.drive, Rect::new(277, 819, 81, 66));
        assert_eq!(l.mix, Rect::new(363, 819, 81, 66));
        assert_eq!(l.keytrack, Rect::new(449, 819, 81, 66));
        assert_eq!(l.blend, Rect::new(555, 749, 207, 24));
        assert_eq!(l.cutoff, Rect::new(530, 870, 251, 24));
        assert_eq!(l.resonance, Rect::new(772, 764, 24, 115));
        assert_eq!(l.drive_label, Rect::new(277, 867, 81, 18));
        assert_eq!(l.keytrack_label, Rect::new(449, 867, 81, 18));
        assert_eq!(l.preset_selector.bounds, Rect::new(277, 758, 253, 61));
        assert_eq!(l.preset_selector.text, Rect::new(277, 750, 253, 61));
        assert_eq!(l.preset_selector.previous, Rect::new(277, 773, 15, 15));
        assert_eq!(l.preset_selector.next, Rect::new(515, 773, 15, 15));
    }

    #[test]
    fn destinations_toggle_like_upstream() {
        use destination::*;
        assert_eq!(toggle(FILTER_2, 1, true), DUAL_FILTERS);
        assert_eq!(toggle(EFFECTS, 1, true), FILTER_1);
        assert_eq!(toggle(DUAL_FILTERS, 1, false), FILTER_2);
        assert_eq!(toggle(FILTER_1, 1, false), EFFECTS);
        assert_eq!(toggle(4, 2, false), 4);
        assert!(feeds(DUAL_FILTERS, 2) && feeds(FILTER_2, 2) && !feeds(FILTER_1, 2));
    }
}
