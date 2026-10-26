// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `SynthSlider`: knobs, text sliders, curve sliders and linear sliders.
//!
//! Ports `OpenGlSlider` and `SynthSlider`
//! (`vital/src/interface/editor_components/synth_slider.{h,cpp}`), the drawing
//! of `TextLookAndFeel::drawRotarySlider` and `CurveLookAndFeel`
//! (`look_and_feel/`), and the parts of JUCE's `Slider` they rely on: the
//! value range and interval, `RotaryHorizontalVerticalDrag` and linear drags,
//! and double-click to the default value.

use std::f32::consts::PI;

use egui::{Modifiers, PointerButton, Pos2, Sense, Ui};
use sas_preset::registry::{ParamInfo, ValueScale};

use super::format::{self, Format};
use super::wheel::{self, WheelColours};
use crate::colour::Colour;
use crate::fonts::Face;
use crate::host::Host;
use crate::layer::Layer;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `OpenGlSlider::kRotaryAngle`: the knob arc runs from -0.8π to 0.8π.
pub const ROTARY_ANGLE: f32 = 0.8 * PI;
/// `SynthSlider::kDefaultRotaryDragLength`: pixels for a full turn.
const DEFAULT_ROTARY_DRAG_LENGTH: f32 = 200.0;
/// `SynthSlider::kSlowDragMultiplier`, with the command key held.
const SLOW_DRAG_MULTIPLIER: f32 = 0.1;
/// `SynthSlider::kLinearWidthPercent` / `kLinearHandlePercent`.
const LINEAR_WIDTH_PERCENT: f32 = 0.26;
const LINEAR_HANDLE_PERCENT: f32 = 1.2;
/// JUCE's multiple-click rules (`MouseInputSource`): presses within 0.4 s
/// (`MouseEvent::getDoubleClickTimeout`) and 8 points of each other, where
/// the last moved less than 4 points and was held no more than 0.3 s.
pub(crate) const DOUBLE_CLICK_TIMEOUT: f64 = 0.4;
pub(crate) const MULTIPLE_CLICK_DISTANCE: f32 = 8.0;
const MOVED_SIGNIFICANTLY: f32 = 4.0;
const LONG_PRESS: f64 = 0.3;
/// How many frames a released slider keeps drawing its own value while
/// the host catches up with the last set (a few in CLAP).
const HOLD_FRAMES: u8 = 10;
/// `SynthSlider::kTextEntryHeightPercent`, `kDefaultTextEntryWidthPercent`.
const TEXT_ENTRY_HEIGHT_PERCENT: f32 = 0.6;
const TEXT_ENTRY_WIDTH_PERCENT: f32 = 0.9;

/// How a slider looks and is dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliderStyle {
    /// `RotaryHorizontalVerticalDrag` drawn as a knob (`kRotarySliderFragment`).
    Rotary,
    /// A rotary slider drawn by `TextLookAndFeel`: the value as text.
    Text,
    /// A rotary slider drawn by `CurveLookAndFeel`: a power curve.
    Curve,
    /// `LinearBar` (`kHorizontalSliderFragment`).
    Horizontal,
    /// `LinearBarVertical` (`kVerticalSliderFragment`).
    Vertical,
    /// `LinearBar` painted as an arrow under the value (`VolumeSlider` in
    /// `volume_section.cpp`): its tip at `point_y` and base at `end_y`
    /// pixels below the slider's top.
    Arrow { point_y: i32, end_y: i32 },
    /// `LinearBarVertical` painted as a wheel (`ControlWheel`); the pitch
    /// wheel springs back to 0 on release (`PitchWheel::mouseUp`).
    Wheel { spring_back: bool },
    /// A modulation's amount knob (`ModulationAmountKnob`, an `OpenGlSlider`
    /// with `setModulationKnob`): `arc` is its rotary arc colour, and a
    /// bypassed modulation draws in grey.
    ModulationKnob { arc: Colour, bypass: bool },
}

impl SliderStyle {
    fn is_rotary(self) -> bool {
        matches!(
            self,
            SliderStyle::Rotary
                | SliderStyle::Text
                | SliderStyle::Curve
                | SliderStyle::ModulationKnob { .. }
        )
    }
}

/// Where the value popup appears (`BubbleComponent::BubblePlacement`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Above,
    Below,
    Left,
    Right,
}

/// One slider bound to a synth control.
#[derive(Clone, Copy, Debug)]
pub struct Slider {
    pub name: &'static str,
    pub info: &'static ParamInfo,
    pub style: SliderStyle,
    pub bipolar: bool,
    /// `sensitivity_`: drags move the value this much faster.
    pub sensitivity: f32,
    pub format: Format,
    /// `knob_size_scale_`.
    pub knob_size_scale: f32,
    /// `text_height_percentage_`; 0 uses the skin's text component font size.
    pub text_height_percentage: f32,
    pub popup_placement: Placement,
    /// `setModulationPlacement`: which side the selected source's amount
    /// knob goes for this control.
    pub modulation_placement: Placement,
    pub show_popup_on_hover: bool,
    /// `OpenGlSlider::active_`: drawn in the disabled colours when false.
    pub active: bool,
    /// A `TextSelector`: a press opens the list of options instead of
    /// dragging.
    pub text_selector: bool,
    /// `TextSelector::long_lookup_`: the names the list shows, when they
    /// differ from the ones the slider shows.
    pub long_lookup: Option<&'static [&'static str]>,
    /// `text_entry_width_percent_` (`setTextEntrySizePercent`).
    pub text_entry_width_percent: f32,
    /// `overrideValue(Skin::kTextComponentOffset, ...)`.
    pub text_component_offset: Option<f32>,
    /// `setDoubleClickReturnValue`: the value a double click sets, when it
    /// isn't the parameter's default.
    pub double_click_value: Option<f32>,
    /// `setDoubleClickReturnValue(false, ...)` makes this false: a double
    /// click changes nothing.
    pub double_click_returns: bool,
    /// Tells apart sliders on the same control shown at once (a
    /// modulation's amount knobs).
    pub instance: u8,
    /// `setPopupPrefix`.
    pub popup_prefix: Option<&'static str>,
    /// `snapToValue`: a drag within 5% of the range of this value lands on
    /// it, unless Command is held.
    pub snap_to: Option<f32>,
    /// `SynthSection::setSliderHasHzAlternateDisplay`: with the hertz
    /// setting on, the value reads in hertz (see [`Slider::displayed`]).
    pub hz_alternate: bool,
}

impl Slider {
    pub fn new(name: &'static str, style: SliderStyle) -> Slider {
        Slider {
            name,
            info: Host::info(name),
            style,
            bipolar: false,
            sensitivity: 1.0,
            format: Format::default(),
            knob_size_scale: 1.0,
            text_height_percentage: 0.0,
            popup_placement: Placement::Below,
            modulation_placement: Placement::Below,
            show_popup_on_hover: false,
            active: true,
            text_selector: false,
            long_lookup: None,
            text_entry_width_percent: TEXT_ENTRY_WIDTH_PERCENT,
            text_component_offset: None,
            double_click_value: None,
            double_click_returns: true,
            instance: 0,
            popup_prefix: None,
            hz_alternate: false,
            snap_to: None,
        }
    }

    /// `snapToValue(true, value)`.
    pub fn snap_to_value(mut self, value: f32) -> Slider {
        self.snap_to = Some(value);
        self
    }

    /// `SynthSlider::snapValue` for a drag.
    fn snapped(&self, value: f32, sensitive: bool) -> f32 {
        const PERCENT: f32 = 0.05;
        match self.snap_to {
            Some(snap) if !sensitive => {
                let radius = PERCENT * (self.info.max - self.info.min);
                if (value - snap).abs() <= radius {
                    snap
                } else {
                    value
                }
            }
            _ => value,
        }
    }

    /// `setSliderHasHzAlternateDisplay`.
    pub fn hz_alternate(mut self) -> Slider {
        self.hz_alternate = true;
        self
    }

    /// The slider as it displays its value now (`getDisplayDetails`): an
    /// hertz-alternate slider with the setting on reads its value through
    /// the hertz details, exponential in semitones from MIDI note 0's
    /// frequency.
    pub fn displayed(&self, hz_frequency: bool) -> Slider {
        let mut slider = *self;
        if self.hz_alternate && hz_frequency {
            slider.info = hz_details(self.info);
            slider.format.exponential_base = 2.0f32.powf(1.0 / 12.0);
        }
        slider
    }

    /// `setPopupPrefix`: the popup reads "prefix value" (a text slider's
    /// reads only the prefix).
    pub fn popup_prefix(mut self, prefix: &'static str) -> Slider {
        self.popup_prefix = Some(prefix);
        self
    }

    /// `setMaxDisplayCharacters` and `setMaxDecimalPlaces`.
    pub fn display(mut self, characters: usize, decimal_places: usize) -> Slider {
        self.format.max_characters = characters;
        self.format.max_decimal_places = decimal_places;
        self
    }

    /// `setTextEntrySizePercent(width, ...)` (upstream reads only the
    /// width) and `overrideValue(Skin::kTextComponentOffset, offset)`, as
    /// the oscillator and sample text sliders set them.
    pub fn text_entry(mut self, width_percent: f32, offset: Option<f32>) -> Slider {
        self.text_entry_width_percent = width_percent;
        self.text_component_offset = offset;
        self
    }

    pub fn knob_size_scale(mut self, scale: f32) -> Slider {
        self.knob_size_scale = scale;
        self
    }

    pub fn popup_placement(mut self, placement: Placement) -> Slider {
        self.popup_placement = placement;
        self
    }

    /// `setModulationPlacement`.
    pub fn modulation_placement(mut self, placement: Placement) -> Slider {
        self.modulation_placement = placement;
        self
    }

    fn text_offset(&self, scope: &Scope) -> f32 {
        self.text_component_offset
            .unwrap_or_else(|| scope.value(ValueId::TextComponentOffset))
    }

    /// A `TextSelector` (a text slider whose press opens its options),
    /// listing `long_lookup` if given, else the parameter's option names.
    pub fn selector(name: &'static str, long_lookup: Option<&'static [&'static str]>) -> Slider {
        let mut slider = Slider::new(name, SliderStyle::Text);
        slider.text_selector = true;
        slider.long_lookup = long_lookup;
        slider
    }

    /// The options a `TextSelector` lists (`TextSelector::mouseDown`): one
    /// per value up to the maximum.
    pub fn selector_options(&self) -> Vec<&'static str> {
        let lookup = self
            .long_lookup
            .or_else(|| self.info.options())
            .unwrap_or(&[]);
        lookup
            .iter()
            .take(self.info.max as usize + 1)
            .copied()
            .collect()
    }

    pub fn bipolar(mut self) -> Slider {
        self.bipolar = true;
        self
    }

    pub fn sensitivity(mut self, sensitivity: f32) -> Slider {
        self.sensitivity = sensitivity;
        self
    }

    pub fn without_suffix(mut self) -> Slider {
        self.format.use_suffix = false;
        self
    }

    /// The JUCE slider interval: 1 for indexed controls, else continuous.
    fn interval(&self) -> f64 {
        if self.info.value_scale == ValueScale::Indexed {
            1.0
        } else {
            0.0
        }
    }

    /// `Slider::valueToProportionOfLength` (no skew).
    pub fn proportion(&self, value: f32) -> f64 {
        let (min, max) = (self.info.min as f64, self.info.max as f64);
        if max > min {
            (value as f64 - min) / (max - min)
        } else {
            0.0
        }
    }

    /// `Slider::proportionOfLengthToValue` then `constrainedValue`.
    pub fn value_from_proportion(&self, proportion: f64) -> f32 {
        let (min, max) = (self.info.min as f64, self.info.max as f64);
        self.constrain(min + (max - min) * proportion) as f32
    }

    /// `Slider::Pimpl::constrainedValue`: snaps to the interval and clamps.
    pub fn constrain(&self, value: f64) -> f64 {
        let (min, max) = (self.info.min as f64, self.info.max as f64);
        let interval = self.interval();
        let value = if interval > 0.0 {
            min + interval * ((value - min) / interval + 0.5).floor()
        } else {
            value
        };
        value.clamp(min, max)
    }

    /// `SynthSlider::getTextFromValue`, which the popup shows: a text
    /// slider already shows its value, so its popup names the parameter.
    pub fn popup_text(&self, value: f32) -> String {
        if self.style == SliderStyle::Text {
            if let Some(prefix) = self.popup_prefix {
                return prefix.to_string();
            }
            if self.info.local_description.is_empty() {
                return self.info.display_name.clone();
            }
            return self.info.local_description.clone();
        }
        // getSliderTextFromValue: a string lookup shows no prefix.
        let text = format::slider_text(self.info, &self.format, value);
        match self.popup_prefix {
            Some(prefix) if self.info.options().is_none() => format!("{prefix}{text}"),
            _ => text,
        }
    }

    fn selected_color(&self, scope: &Scope) -> Colour {
        match (self.style.is_rotary(), self.active) {
            (true, true) => scope.color(ColorId::RotaryArc),
            (true, false) => scope.color(ColorId::RotaryArcDisabled),
            (false, true) => scope.color(ColorId::LinearSlider),
            (false, false) => scope.color(ColorId::LinearSliderDisabled),
        }
    }

    fn unselected_color(&self, scope: &Scope) -> Colour {
        match (self.style.is_rotary(), self.active) {
            (true, true) => scope.color(ColorId::RotaryArcUnselected),
            (true, false) => scope.color(ColorId::RotaryArcUnselectedDisabled),
            (false, _) => scope.color(ColorId::LinearSliderUnselected),
        }
    }

    fn thumb_color(&self, scope: &Scope) -> Colour {
        match (self.style.is_rotary(), self.active) {
            (true, _) => scope.color(ColorId::RotaryHand),
            (false, true) => scope.color(ColorId::LinearSliderThumb),
            (false, false) => scope.color(ColorId::LinearSliderThumbDisabled),
        }
    }

    /// `SynthSlider::drawShadow`: the knob body and the shadow under its
    /// arc, or the background of a curve slider. Painted with the section
    /// background.
    pub fn paint_shadow(&self, pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
        match self.style {
            SliderStyle::Rotary => self.paint_rotary_shadow(pixels, scope, bounds),
            SliderStyle::Curve => pixels.fill_rounded_rect(
                bounds,
                scope.value(ValueId::WidgetRoundedCorner),
                scope.color(ColorId::WidgetBackground),
            ),
            _ => {}
        }
    }

    /// `SynthSlider::drawRotaryShadow`. The default skin has no knob shadow
    /// width, so the radial shadow gradient is not drawn.
    fn paint_rotary_shadow(&self, pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
        let shadow_color = scope.color(ColorId::Shadow);
        let center_x = bounds.x as f32 + bounds.width as f32 / 2.0;
        let center_y =
            bounds.y as f32 + bounds.height as f32 / 2.0 + scope.value(ValueId::KnobOffset);
        let stroke_width = scope.value(ValueId::KnobArcThickness);
        let radius = self.knob_size_scale * scope.value(ValueId::KnobArcSize) / 2.0;
        let body_radius = self.knob_size_scale * scope.value(ValueId::KnobBodySize) / 2.0;
        if body_radius >= 0.0 && body_radius < bounds.width as f32 {
            let ellipse = RectF::new(
                center_x - body_radius,
                center_y - body_radius,
                2.0 * body_radius,
                2.0 * body_radius,
            );
            pixels.fill_ellipse(ellipse, scope.color(ColorId::RotaryBody));
            pixels.draw_ellipse(
                ellipse.reduced(0.5),
                1.0,
                scope.color(ColorId::RotaryBodyBorder),
            );
        }
        let unselected = if self.active {
            scope.color(ColorId::RotaryArcUnselected)
        } else {
            scope.color(ColorId::RotaryArcUnselectedDisabled)
        };
        if unselected.alpha() != 0 {
            pixels.stroke_arc(
                (center_x, center_y),
                radius,
                -ROTARY_ANGLE,
                ROTARY_ANGLE,
                stroke_width + 1.0,
                shadow_color,
            );
        }
    }

    /// The slider itself: its quad (`OpenGlSlider::redoImage`), or the text
    /// or curve its look and feel draws. `hover` is `isMouseOverOrDragging`.
    pub fn paint(
        &self,
        pixels: &PixelPainter,
        layer: &mut Layer,
        scope: &Scope,
        bounds: Rect,
        value: f32,
        hover: bool,
    ) {
        let t = self.proportion(value) as f32;
        match self.style {
            SliderStyle::Rotary => {
                let thickness = scope.value(ValueId::KnobArcThickness);
                let size = scope.value(ValueId::KnobArcSize) * self.knob_size_scale + thickness;
                let offset = scope.value(ValueId::KnobOffset);
                let side = size + 0.5;
                let center_x = bounds.x as f32 + bounds.width as f32 / 2.0;
                let center_y = bounds.y as f32 + bounds.height as f32 / 2.0 + offset;
                let rect = RectF::new(center_x - side / 2.0, center_y - side / 2.0, side, side);
                let mut quad = Quad::new(QuadKind::RotarySlider, rect, self.selected_color(scope));
                quad.alt_color = self.unselected_color(scope);
                quad.thumb_color = self.thumb_color(scope);
                quad.thumb_amount = scope.value(ValueId::KnobHandleLength);
                quad.max_arc = ROTARY_ANGLE;
                quad.shader_values[0] = -ROTARY_ANGLE + (2.0 * ROTARY_ANGLE) * t;
                quad.start_pos = if self.bipolar { 0.0 } else { -PI };
                quad.thickness = if hover { thickness * 1.4 } else { thickness };
                layer.quad(quad);
            }
            SliderStyle::Horizontal | SliderStyle::Vertical => {
                let horizontal = self.style == SliderStyle::Horizontal;
                let margin = scope.value(ValueId::WidgetMargin) - 0.5;
                let rect = if horizontal {
                    RectF::new(
                        bounds.x as f32 + margin,
                        bounds.y as f32,
                        bounds.width as f32 - 2.0 * margin,
                        bounds.height as f32,
                    )
                } else {
                    RectF::new(
                        bounds.x as f32,
                        bounds.y as f32 + margin,
                        bounds.width as f32,
                        bounds.height as f32 - 2.0 * margin,
                    )
                };
                let kind = if horizontal {
                    QuadKind::HorizontalSlider
                } else {
                    QuadKind::VerticalSlider
                };
                let total_width = if horizontal {
                    bounds.height
                } else {
                    bounds.width
                };
                let slider_width = linear_slider_width(total_width) as f32;
                let mut quad = Quad::new(kind, rect, self.selected_color(scope));
                quad.alt_color = self.unselected_color(scope);
                quad.thumb_color = self.thumb_color(scope);
                quad.shader_values[0] = t;
                quad.start_pos = if self.bipolar { 0.0 } else { -1.0 };
                quad.thickness = if hover {
                    let boost = (slider_width / 8.0).round() + 1.0;
                    slider_width + 2.0 * boost
                } else {
                    slider_width
                };
                quad.rounding = slider_width * 0.4;
                quad.thumb_amount = LINEAR_HANDLE_PERCENT * total_width as f32;
                layer.quad(quad);
            }
            SliderStyle::Arrow { point_y, end_y } => {
                // VolumeSlider::paint at getPositionOfValue: a LinearBar's
                // value runs across the whole width.
                let x = bounds.x as f32 + t * bounds.width as f32;
                let tip = (bounds.y + point_y) as f32;
                let base = (bounds.y + end_y) as f32;
                let half = (end_y - point_y) as f32 / 2.0;
                layer.polygon(
                    vec![(x, tip), (x + half, base), (x - half, base)],
                    scope.color(ColorId::LinearSliderThumb),
                );
            }
            SliderStyle::Wheel { .. } => {
                let colours = WheelColours {
                    background: scope.color(ColorId::WidgetSecondary1),
                    line: scope.color(ColorId::WidgetAccent1),
                    marker: scope.color(ColorId::WidgetPrimary1),
                    body: scope.color(ColorId::Body),
                    shadow: scope.color(ColorId::Shadow),
                };
                let bounds = RectF::from(bounds);
                layer.custom(move |pixels| wheel::paint(pixels, bounds, t, colours));
            }
            SliderStyle::Text => self.paint_text(layer, scope, bounds, value),
            SliderStyle::ModulationKnob { arc, bypass } => {
                // OpenGlSlider::redoImage with modulation_knob_, and
                // ModulationAmountKnob's bypass desaturation.
                let saturation = |colour: Colour| {
                    if bypass {
                        colour.desaturated()
                    } else {
                        colour
                    }
                };
                let background = scope.color(ColorId::WidgetBackground);
                let selected = saturation(arc.interpolated_with(background, 0.5));
                let unselected = saturation(background);
                let rect = RectF::from(bounds).reduced(0.5);
                let mut quad = Quad::new(QuadKind::ModulationKnob, rect, unselected);
                quad.alt_color = selected;
                if value > 0.0 {
                    quad.shader_values[0] = PI + (-PI - PI) * value;
                } else {
                    quad.shader_values[0] = -PI + (PI + PI) * -value;
                    quad.color = selected;
                    quad.alt_color = unselected;
                }
                quad.thumb_color = saturation(arc);
                quad.thickness = if hover { 1.8 } else { 1.0 };
                layer.quad(quad);
            }
            SliderStyle::Curve => self.paint_curve(pixels, scope, bounds, value),
        }
    }

    /// `TextLookAndFeel::drawRotarySlider`.
    fn paint_text(&self, layer: &mut Layer, scope: &Scope, bounds: Rect, value: f32) {
        let font_size = if self.text_height_percentage == 0.0 {
            scope.value(ValueId::TextComponentFontSize)
        } else {
            bounds.height as f32 * self.text_height_percentage
        };
        let offset = self.text_offset(scope).round();
        let mut colour = scope.color(ColorId::TextComponentText);
        if !self.active {
            colour = colour.with_multiplied_alpha(0.5);
        }
        let text = format::slider_text(self.info, &self.format, value);
        let area = RectF::new(
            bounds.x as f32,
            bounds.y as f32 + offset,
            bounds.width as f32,
            bounds.height as f32,
        );
        layer.text_centred(&text, area, Face::Light, font_size, colour);
    }

    /// `CurveLookAndFeel::drawRotarySlider` / `drawCurve`.
    fn paint_curve(&self, pixels: &PixelPainter, scope: &Scope, bounds: Rect, value: f32) {
        const RESOLUTION: usize = 16;
        const LINE_WIDTH: f32 = 2.0;
        let rounding = scope.value(ValueId::WidgetRoundedCorner);
        let short_side = bounds.width.min(bounds.height) as f32;
        let max_width = short_side.min(scope.value(ValueId::KnobArcSize));
        let inset = (rounding / 2.0f32.sqrt() + (short_side - max_width) / 2.0) as i32;
        let (x, y) = ((bounds.x + inset) as f32, (bounds.y + inset) as f32);
        let width = (bounds.width as f32 - 2.0 * inset as f32) as i32 as f32;
        let height = (bounds.height as f32 - 2.0 * inset as f32) as i32 as f32;

        let curve_width = width.min(height);
        let x_offset = (width - curve_width) / 2.0;
        let power = -value;
        let start_x = x + x_offset + LINE_WIDTH / 2.0;
        let start_y = y + height - LINE_WIDTH / 2.0;
        let active_width = curve_width - LINE_WIDTH;
        let active_height = curve_width - LINE_WIDTH;
        let mut points = vec![(start_x, start_y)];
        if self.bipolar {
            let half_width = active_width / 2.0;
            let half_height = active_height / 2.0;
            for i in 0..RESOLUTION / 2 {
                let t = 2.0 * (i as f32 + 1.0) / RESOLUTION as f32;
                let power_t = power_scale(t, -power);
                points.push((start_x + t * half_width, start_y - power_t * half_height));
            }
            for i in 0..RESOLUTION / 2 {
                let t = 2.0 * (i as f32 + 1.0) / RESOLUTION as f32;
                let power_t = power_scale(t, power);
                points.push((
                    start_x + t * half_width + half_width,
                    start_y - power_t * half_height - half_height,
                ));
            }
        } else {
            for i in 0..RESOLUTION {
                let t = (i as f32 + 1.0) / RESOLUTION as f32;
                let power_t = power_scale(t, power);
                points.push((
                    start_x + t * active_width,
                    start_y - power_t * active_height,
                ));
            }
        }
        let colour = if self.active {
            scope.color(ColorId::RotaryArc)
        } else {
            scope.color(ColorId::WidgetPrimaryDisabled)
        };
        pixels.stroke_path(&points, LINE_WIDTH, colour);
    }

    /// `SynthSlider::setRotaryTextEntryBounds` / `setLinearTextEntryBounds`.
    pub fn text_entry_bounds(&self, scope: &Scope, bounds: Rect) -> Rect {
        let font_size = scope.value(ValueId::TextComponentFontSize);
        let text_height = (font_size / TEXT_ENTRY_HEIGHT_PERCENT) as i32;
        if self.style.is_rotary() {
            let text_width = (bounds.width as f32 * self.text_entry_width_percent) as i32;
            let y_offset = if self.style == SliderStyle::Text {
                self.text_offset(scope)
            } else {
                0.0
            };
            let y = ((bounds.height - text_height + 1) / 2) as f32 + y_offset;
            Rect::new(
                bounds.x + (bounds.width - text_width) / 2,
                bounds.y + y as i32,
                text_width,
                text_height,
            )
        } else {
            let text_width = (text_height as f32 * 3.0) as i32;
            Rect::new(
                bounds.x + (bounds.width - text_width) / 2,
                bounds.y + (bounds.height - text_height) / 2,
                text_width,
                text_height,
            )
        }
    }

    /// Handles the mouse over this slider (`SynthSlider::mouseDown`,
    /// `mouseDrag`, `mouseUp`, `mouseDoubleClick`, `mouseEnter`/`mouseExit`).
    pub fn interact(
        &self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        bounds: Rect,
    ) -> SliderEvent {
        let id = ui
            .scope_id()
            .with(("synth slider", self.name, self.instance));
        let rect = pixels.rect(bounds.into());
        let response = ui.interact(rect, id, Sense::click_and_drag());
        let mut state: DragState = ui.data(|data| data.get_temp(id)).unwrap_or_default();
        let mut event = SliderEvent {
            hover: response.hovered() || state.down,
            ..SliderEvent::default()
        };

        let (pointer, modifiers, primary_down, middle_down, secondary_down) = ui.input(|input| {
            (
                input.pointer.interact_pos(),
                input.modifiers,
                input.pointer.button_down(PointerButton::Primary),
                input.pointer.button_down(PointerButton::Middle),
                input.pointer.button_down(PointerButton::Secondary),
            )
        });
        let pressed_here = response.is_pointer_button_down_on();
        let popup_menu =
            secondary_down || (cfg!(target_os = "macos") && modifiers.ctrl && primary_down);
        let now = ui.input(|input| input.time);

        if pressed_here && !state.down {
            // mouseDown.
            if self.text_selector && !popup_menu {
                // TextSelector::mouseDown: the options, below the selector.
                event.selector = true;
            } else if modifiers.alt {
                event.text_entry = true;
            } else if popup_menu {
                event.menu_at = pointer;
                // A press with other buttons ends a run of clicks.
                if let Some(pointer) = pointer {
                    let button = if secondary_down {
                        PointerButton::Secondary
                    } else {
                        PointerButton::Primary
                    };
                    state.last_press = Some((now, pointer, button));
                }
            } else if let Some(pointer) = pointer {
                let button = if primary_down || !middle_down {
                    PointerButton::Primary
                } else {
                    PointerButton::Middle
                };
                // MouseInputSource::getNumberOfMultipleClicks: a press with
                // the same buttons as the previous one, near it and soon
                // after it.
                let follows_press = state.last_press.is_some_and(|(time, at, last)| {
                    last == button
                        && now - time < DOUBLE_CLICK_TIMEOUT
                        && (pointer.x - at.x).abs() < MULTIPLE_CLICK_DISTANCE
                        && (pointer.y - at.y).abs() < MULTIPLE_CLICK_DISTANCE
                });
                state.down = true;
                state.last = pointer;
                state.second_click = follows_press;
                state.moved = false;
                state.value = host.value(self.name);
                state.proportion = self.proportion(state.value);
                state.last_press = Some((now, pointer, button));
                host.begin(self.name);
                event.popup = true;
            }
        } else if state.down && (primary_down || middle_down) {
            // mouseDrag, with any button but the menu's (juce::Slider drags
            // with the middle one too). SynthSlider::mouseDrag ignores drags
            // with alt held.
            if let (Some(pointer), false) = (pointer, modifiers.alt) {
                if let Some((_, at, _)) = state.last_press {
                    state.moved |= pointer.distance(at) >= MOVED_SIGNIFICANTLY;
                }
                if pointer != state.last {
                    let ppp = pixels.pixels_per_point();
                    state.proportion =
                        self.dragged_proportion(&state, pointer, bounds, ppp, modifiers);
                    state.last = pointer;
                    let value = self.snapped(
                        self.value_from_proportion(state.proportion),
                        modifiers.command,
                    );
                    if value != state.value {
                        host.set(self.name, value);
                        state.value = value;
                    }
                }
                event.popup = true;
            }
        } else if state.down {
            // mouseUp, then Component::internalMouseUp's double-click check:
            // a second press that was neither a drag nor a long press.
            state.down = false;
            host.end(self.name);
            if let SliderStyle::Wheel { spring_back: true } = self.style {
                // PitchWheel::mouseUp: back to the centre after the gesture.
                // JUCE's setValue sends nothing when the value is unchanged.
                if state.value != 0.0 {
                    host.set_once(self.name, 0.0);
                    state.value = 0.0;
                }
            }
            event.value = Some(state.value);
            state.holding = HOLD_FRAMES;
            let long_press = state
                .last_press
                .is_some_and(|(time, _, _)| now - time > LONG_PRESS);
            if self.double_click_returns && state.second_click && !state.moved && !long_press {
                // Slider::mouseDoubleClick: back to the default value.
                let value = self.double_click_value.unwrap_or(self.info.default_value);
                host.set_once(self.name, value);
                state.value = value;
                event.value = Some(value);
                event.popup = true;
            }
        }
        if state.down {
            event.value = Some(state.value);
        } else if state.holding > 0 && event.value.is_none() {
            if host.value(self.name) == state.value {
                state.holding = 0;
            } else {
                state.holding -= 1;
                event.value = Some(state.value);
            }
        }

        if self.show_popup_on_hover && response.hovered() {
            event.popup = true;
        }
        event.hover = response.hovered() || state.down;
        ui.data_mut(|data| data.insert_temp(id, state));
        event
    }

    /// `Slider::Pimpl::handleAbsoluteDrag` for one drag event, with
    /// `SynthSlider::mouseDrag`'s sensitivity for the current modifiers: the
    /// movement since the last event moves the unsnapped proportion, clamped
    /// at every step.
    /// Positions are in points (JUCE's logical coordinates), `bounds` in
    /// pixels at `pixels_per_point`.
    fn dragged_proportion(
        &self,
        state: &DragState,
        position: Pos2,
        bounds: Rect,
        pixels_per_point: f32,
        modifiers: Modifiers,
    ) -> f64 {
        let multiply = if modifiers.command {
            SLOW_DRAG_MULTIPLIER
        } else {
            1.0
        };
        let full_drag = if self.style.is_rotary() {
            DEFAULT_ROTARY_DRAG_LENGTH / (self.sensitivity * multiply)
        } else {
            bounds.width.max(bounds.height) as f32
                / pixels_per_point
                / (self.sensitivity * multiply)
        };
        let mouse_diff = match self.style {
            SliderStyle::Horizontal | SliderStyle::Arrow { .. } => position.x - state.last.x,
            SliderStyle::Vertical | SliderStyle::Wheel { .. } => state.last.y - position.y,
            _ => (position.x - state.last.x) + (state.last.y - position.y),
        };
        (state.proportion + mouse_diff as f64 / full_drag as f64).clamp(0.0, 1.0)
    }
}

/// What a slider asks of the interface this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SliderEvent {
    /// `isMouseOverOrDragging`.
    pub hover: bool,
    /// `showPopup(true)`.
    pub popup: bool,
    /// Open the right-click menu at this point (points).
    pub menu_at: Option<Pos2>,
    /// `showTextEntry`.
    pub text_entry: bool,
    /// A `TextSelector` was pressed: open its options.
    pub selector: bool,
    /// The slider's own value while it is held and as it is released, for
    /// drawing it and its popup: the host shows a set only later.
    pub value: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default)]
struct DragState {
    down: bool,
    /// The pointer at the last drag event, in points (JUCE's
    /// `mouseDragStartPos`, reset every event).
    last: Pos2,
    /// The unsnapped position (JUCE's `valueWhenLastDragged`); only the
    /// value sent to the host is snapped.
    proportion: f64,
    /// The slider's own value (JUCE's `Slider::getValue`), which a drag
    /// changes at once. The host applies a set later (a CLAP event, or after
    /// a VST3 block), so its value can lag.
    value: f32,
    /// Frames left to keep drawing `value` after a release, until the host
    /// shows it.
    holding: u8,
    /// The time, place (points) and buttons of the last press.
    last_press: Option<(f64, Pos2, PointerButton)>,
    /// This press followed the previous one closely enough to make a
    /// double click.
    second_click: bool,
    /// The pointer has moved far enough from the press to make it a drag.
    moved: bool,
}

/// `OpenGlSlider::getLinearSliderWidth`.
fn linear_slider_width(total_width: i32) -> i32 {
    let extra = total_width % 2;
    ((LINEAR_WIDTH_PERCENT * total_width as f32 * 0.5).floor() * 2.0) as i32 + extra
}

/// `futils::powerScale`.
pub fn power_scale(value: f32, power: f32) -> f32 {
    const MIN_POWER: f32 = 0.01;
    if power.abs() < MIN_POWER {
        return value;
    }
    let numerator = (power * value).exp() - 1.0;
    let denominator = power.exp() - 1.0;
    numerator / denominator
}

/// `vital::kMidi0Frequency`.
#[allow(clippy::excessive_precision)]
const MIDI_0_FREQUENCY: f32 = 8.175_798_915_6;

/// `setSliderHasHzAlternateDisplay`'s `hz_details`: the parameter's details
/// as hertz. Made once per parameter.
fn hz_details(info: &'static ParamInfo) -> &'static ParamInfo {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static DETAILS: OnceLock<Mutex<HashMap<String, &'static ParamInfo>>> = OnceLock::new();
    let mut details = DETAILS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    details.entry(info.name.clone()).or_insert_with(|| {
        let mut hz = info.clone();
        hz.value_scale = ValueScale::Exponential;
        hz.post_offset = 0.0;
        hz.display_units = " Hz";
        hz.display_multiply = MIDI_0_FREQUENCY;
        Box::leak(Box::new(hz))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_values_snap_like_juce() {
        let polyphony = Slider::new("polyphony", SliderStyle::Text);
        assert_eq!(polyphony.constrain(7.4), 7.0);
        assert_eq!(polyphony.constrain(7.5), 8.0);
        assert_eq!(polyphony.constrain(1000.0), polyphony.info.max as f64);
        let glide = Slider::new("portamento_time", SliderStyle::Rotary);
        assert_eq!(glide.constrain(-3.25), -3.25);
    }

    #[test]
    fn rotary_drags_follow_both_axes() {
        let knob = Slider::new("velocity_track", SliderStyle::Rotary);
        let state = DragState {
            down: true,
            last: Pos2::new(10.0, 10.0),
            proportion: 0.5,
            value: 0.0,
            holding: 0,
            last_press: None,
            second_click: false,
            moved: false,
        };
        let bounds = Rect::new(0, 0, 60, 60);
        // Up 20 and right 20: 40 pixels of a 200-pixel turn.
        let p =
            knob.dragged_proportion(&state, Pos2::new(30.0, -10.0), bounds, 1.0, Modifiers::NONE);
        assert!((p - 0.7).abs() < 1e-9);
        let fine = knob.dragged_proportion(
            &state,
            Pos2::new(30.0, -10.0),
            bounds,
            1.0,
            Modifiers::COMMAND,
        );
        assert!((fine - 0.52).abs() < 1e-9);
        let slow = knob.sensitivity(0.2);
        let p =
            slow.dragged_proportion(&state, Pos2::new(30.0, -10.0), bounds, 1.0, Modifiers::NONE);
        assert!((p - 0.54).abs() < 1e-9);
    }

    #[test]
    fn linear_slider_width_matches_upstream() {
        assert_eq!(linear_slider_width(36), 8);
        assert_eq!(linear_slider_width(37), 9);
    }

    #[test]
    fn power_scale_matches_futils() {
        assert_eq!(power_scale(0.5, 0.0), 0.5);
        let expected = ((2.0f32 * 0.5).exp() - 1.0) / (2.0f32.exp() - 1.0);
        assert_eq!(power_scale(0.5, 2.0), expected);
    }
}

#[cfg(test)]
mod hz_tests {
    use super::*;

    /// A cutoff of MIDI note 60 reads as middle C's frequency with the
    /// hertz setting on, and typing a frequency sets its note.
    #[test]
    fn cutoffs_read_in_hertz_with_the_setting_on() {
        let cutoff = Slider::new("filter_1_cutoff", SliderStyle::Horizontal)
            .popup_prefix("Cutoff: ")
            .hz_alternate();
        let hz = cutoff.displayed(true);
        assert_eq!(hz.popup_text(60.0), "Cutoff: 261.6 Hz");
        let note = format::value_from_text(hz.info, &hz.format, "440");
        assert!((note - 69.0).abs() < 1e-3, "{note}");
        assert!(!cutoff.displayed(false).popup_text(60.0).contains("Hz"));
        // A slider without the alternate never changes.
        let plain = Slider::new("filter_1_cutoff", SliderStyle::Horizontal);
        assert_eq!(plain.displayed(true).info.name, plain.info.name);
        assert!(!plain.displayed(true).popup_text(60.0).contains("Hz"));
    }
}
