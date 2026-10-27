// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `ModulationMeter` (`vital/src/interface/editor_components/modulation_meter.cpp`):
//! the arc or bar on a modulated control that shows where the engine has
//! moved it, drawn under the controls (`ModulationManager::renderMeters`,
//! before `FullInterface` renders its components).

use sas_preset::registry::ParamInfo;

use crate::controls::slider::{Slider, SliderStyle};
use crate::layout::Rect;
use crate::paint::RectF;
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `SynthSlider::kRotaryAngle`.
const ROTARY_ANGLE: f32 = 0.8 * std::f32::consts::PI;
/// `SynthSlider::kLinearWidthPercent`.
const LINEAR_WIDTH_PERCENT: f32 = 0.26;
/// `SynthSlider::kLinearModulationPercent`.
const LINEAR_MODULATION_PERCENT: f32 = 0.1;
/// `getModulationMeterBounds`' `kTextBarSize`.
const TEXT_BAR_SIZE: i32 = 2;

/// How a control's meter draws.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    /// An arc around a knob (`kRotaryModulationFragment`).
    Rotary,
    /// A bar beside a linear slider, or at a text slider's side
    /// (`kLinearModulationFragment`); `rotated` runs it upwards.
    Linear { horizontal: bool, rotated: bool },
}

/// `slider`'s meter shape, or None for a control without one.
fn shape(style: SliderStyle) -> Option<Shape> {
    match style {
        SliderStyle::Rotary => Some(Shape::Rotary),
        SliderStyle::Text | SliderStyle::Curve => Some(Shape::Linear {
            horizontal: false,
            rotated: true,
        }),
        SliderStyle::Horizontal | SliderStyle::Arrow { .. } => Some(Shape::Linear {
            horizontal: true,
            rotated: false,
        }),
        SliderStyle::Vertical | SliderStyle::Wheel { .. } => Some(Shape::Linear {
            horizontal: false,
            rotated: true,
        }),
        SliderStyle::ModulationKnob { .. } => None,
    }
}

/// `SynthSlider::getModulationMeterBounds`: the meter component's bounds
/// in the slider's (`bounds`) coordinates.
fn component_bounds(slider: &Slider, bounds: Rect, section: &Scope) -> Rect {
    let (width, height) = (bounds.width, bounds.height);
    match slider.style {
        SliderStyle::Text | SliderStyle::Curve => {
            if slider.modulation_bar_right {
                Rect::new(width - TEXT_BAR_SIZE, 0, TEXT_BAR_SIZE, height)
            } else {
                Rect::new(0, 0, TEXT_BAR_SIZE, height)
            }
        }
        SliderStyle::Rotary => Rect::new(0, 0, width, height),
        style => {
            let buffer = section.widget_margin() as i32;
            if matches!(style, SliderStyle::Horizontal | SliderStyle::Arrow { .. }) {
                Rect::new(buffer, 0, width - 2 * buffer, height)
            } else {
                Rect::new(0, buffer, width, height - 2 * buffer)
            }
        }
    }
}

/// `ModulationMeter::getMeterBounds`: the meter in its component
/// (`width` × `height`), for a slider `bounds` big. `manager` is
/// `ModulationManager`'s scope, the meter's parent section.
fn meter_bounds(
    slider: &Slider,
    shape: Shape,
    bounds: Rect,
    width: f32,
    height: f32,
    section: &Scope,
    manager: &Scope,
) -> RectF {
    match (shape, slider.style) {
        (_, SliderStyle::Text | SliderStyle::Curve) => RectF::new(0.0, 0.0, width, height),
        (Shape::Rotary, _) => {
            let knob_scale = slider.knob_size_scale;
            let meter_width = section.value(ValueId::KnobModMeterArcSize) * knob_scale
                + section.value(ValueId::KnobModMeterArcThickness) * (1.0 - knob_scale);
            let offset = section.value(ValueId::KnobOffset);
            let (centre_x, centre_y) = (width * 0.5, height * 0.5);
            RectF::new(
                centre_x - meter_width * 0.5,
                centre_y - meter_width * 0.5 + offset,
                meter_width,
                meter_width,
            )
        }
        (Shape::Linear { horizontal, .. }, _) => {
            let widget_margin = manager.widget_margin() as i32;
            let total_width = if horizontal {
                bounds.height
            } else {
                bounds.width
            };
            let extra = total_width % 2;
            let slider_width = ((LINEAR_WIDTH_PERCENT * total_width as f32 * 0.5).floor() * 2.0
                + extra as f32) as i32;
            let inner_area = (total_width - slider_width) / 2;
            let outer_area = inner_area - widget_margin;
            let meter_width = (LINEAR_MODULATION_PERCENT * total_width as f32) as i32;
            let border = 1.max(((widget_margin - meter_width) as f32 * 0.5) as i32);
            let start = (outer_area + border) as f32;
            let size = (inner_area - outer_area - 2 * border) as f32;
            if horizontal {
                RectF::new(0.0, start, width, size)
            } else {
                RectF::new(start, 0.0, size, height)
            }
        }
    }
}

/// Where a control's meter draws (editor pixels) and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeterPlace {
    rect: RectF,
    shape: Shape,
}

/// `slider`'s meter at `bounds` (editor pixels), or None for a control
/// without one. `manager` is `ModulationManager`'s scope, the meter's
/// parent section.
pub fn place(
    slider: &Slider,
    bounds: Rect,
    section: &Scope,
    manager: &Scope,
) -> Option<MeterPlace> {
    let shape = shape(slider.style)?;
    let component = component_bounds(slider, bounds, section);
    let meter = meter_bounds(
        slider,
        shape,
        bounds,
        component.width as f32,
        component.height as f32,
        section,
        manager,
    );
    let rect = RectF::new(
        (bounds.x + component.x) as f32 + meter.x,
        (bounds.y + component.y) as f32 + meter.y,
        meter.width,
        meter.height,
    );
    Some(MeterPlace { rect, shape })
}

/// The quad `ModulationMeter::updateDrawing` sets for `slider` at `bounds`
/// (editor pixels), showing `value` (the slider's own) moved to `current`
/// (`current_value_`: the engine's left and right totals). None for a
/// control without a meter.
pub fn meter_quad(
    slider: &Slider,
    bounds: Rect,
    value: f32,
    current: [f32; 2],
    section: &Scope,
    manager: &Scope,
) -> Option<Quad> {
    let MeterPlace { rect, shape } = place(slider, bounds, section, manager)?;

    let (min, max) = (slider.info.min, slider.info.max);
    let range = max - min;
    let knob_percent = (value - min) / range;
    let mut shader_values = [0.0; 4];
    for (lane, &current) in current.iter().enumerate() {
        let mod_percent = ((current - min) * (1.0 / range)).clamp(0.0, 1.0);
        let mut low = mod_percent.min(knob_percent);
        let mut high = mod_percent.max(knob_percent);
        if shape == Shape::Rotary {
            low = -ROTARY_ANGLE + (2.0 * ROTARY_ANGLE) * low;
            high = -ROTARY_ANGLE + (2.0 * ROTARY_ANGLE) * high;
        }
        shader_values[2 * lane] = low;
        shader_values[2 * lane + 1] = high;
    }

    let kind = match shape {
        Shape::Rotary => QuadKind::RotaryModulation,
        Shape::Linear { .. } => QuadKind::LinearModulation,
    };
    let mut quad = Quad::new(kind, rect, manager.color(ColorId::ModulationMeterLeft));
    quad.alt_color = manager.color(ColorId::ModulationMeterRight);
    quad.mod_color = manager.color(ColorId::ModulationMeter);
    quad.shader_values = shader_values;
    if shape == Shape::Rotary {
        quad.thickness = manager.value(ValueId::KnobModMeterArcThickness);
    }
    if let Shape::Linear { rotated: true, .. } = shape {
        quad.rotated = true;
    }
    Some(quad)
}

/// The editing amount overlay `ModulationManager::showModulationAmountOverlay`
/// draws on a destination's meter at `place`: the range a connection of
/// `amount` (a fraction of the range, `bipolar` either side) sweeps from
/// the control's `value` (`setAmountQuadVertices`, `setModulationAmountQuad`).
pub fn amount_quad(
    place: MeterPlace,
    info: &ParamInfo,
    value: f32,
    amount: f32,
    bipolar: bool,
    manager: &Scope,
) -> Quad {
    let MeterPlace { rect, shape } = place;
    let range = info.max - info.min;
    let knob_percent = (value - info.min) / range;
    let (mut low, mut high) = if bipolar {
        (
            (knob_percent + amount * 0.5).min(knob_percent - amount * 0.5),
            (knob_percent + amount * 0.5).max(knob_percent - amount * 0.5),
        )
    } else {
        (
            (knob_percent + amount).min(knob_percent),
            (knob_percent + amount).max(knob_percent),
        )
    };
    let (kind, rect) = match shape {
        Shape::Rotary => {
            low = (-ROTARY_ANGLE + (2.0 * ROTARY_ANGLE) * low).max(-ROTARY_ANGLE);
            high = (-ROTARY_ANGLE + (2.0 * ROTARY_ANGLE) * high).min(ROTARY_ANGLE);
            let expanded = RectF::new(
                rect.x - 2.0,
                rect.y - 2.0,
                rect.width + 4.0,
                rect.height + 4.0,
            );
            (QuadKind::RotaryModulation, expanded)
        }
        // A one pixel line along the meter's left edge, or its top.
        Shape::Linear { rotated: true, .. } => (
            QuadKind::LinearModulation,
            RectF::new(rect.x, rect.y, 1.0, rect.height),
        ),
        Shape::Linear { .. } => (
            QuadKind::LinearModulation,
            RectF::new(rect.x, rect.y, rect.width, 1.0),
        ),
    };
    let centre = manager.color(ColorId::ModulationMeter);
    let mut quad = Quad::new(kind, rect, centre);
    quad.alt_color = centre;
    quad.mod_color = centre;
    quad.shader_values = [low, high, low, high];
    if shape == Shape::Rotary {
        quad.thickness = 2.0;
    }
    if let Shape::Linear { rotated: true, .. } = shape {
        quad.rotated = true;
    }
    quad
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::Skin;

    fn scopes(skin: &Skin) -> (Scope<'_>, Scope<'_>) {
        (Scope::top(skin, 1.0), Scope::top(skin, 1.0))
    }

    #[test]
    fn a_knobs_meter_is_an_arc_from_its_value_to_the_engines_on_each_side() {
        let skin = Skin::default();
        let (section, manager) = scopes(&skin);
        let slider = Slider::new("osc_1_level", SliderStyle::Rotary);
        let (min, max) = (slider.info.min, slider.info.max);
        let at = |percent: f32| min + percent * (max - min);
        let bounds = Rect::new(100, 50, 40, 60);
        let quad = meter_quad(
            &slider,
            bounds,
            at(0.5),
            [at(0.75), at(0.25)],
            &section,
            &manager,
        )
        .unwrap();
        assert_eq!(quad.kind, QuadKind::RotaryModulation);
        let width = section.value(ValueId::KnobModMeterArcSize);
        let offset = section.value(ValueId::KnobOffset);
        assert_eq!(
            quad.rect,
            RectF::new(
                120.0 - width * 0.5,
                80.0 - width * 0.5 + offset,
                width,
                width
            )
        );
        let angle = ROTARY_ANGLE;
        let expected = [0.0, 0.5 * angle, -0.5 * angle, 0.0];
        for (value, expected) in quad.shader_values.iter().zip(expected) {
            assert!((value - expected).abs() < 1e-5, "{:?}", quad.shader_values);
        }
        assert_eq!(
            quad.thickness,
            manager.value(ValueId::KnobModMeterArcThickness)
        );
        assert_eq!(quad.color, manager.color(ColorId::ModulationMeterLeft));
        assert_eq!(quad.alt_color, manager.color(ColorId::ModulationMeterRight));
        assert_eq!(quad.mod_color, manager.color(ColorId::ModulationMeter));
        assert!(!quad.rotated);
    }

    #[test]
    fn the_engines_value_is_clamped_to_the_range() {
        let skin = Skin::default();
        let (section, manager) = scopes(&skin);
        let slider = Slider::new("osc_1_level", SliderStyle::Rotary);
        let (min, max) = (slider.info.min, slider.info.max);
        let quad = meter_quad(
            &slider,
            Rect::new(0, 0, 40, 40),
            min,
            [max + 10.0, min - 10.0],
            &section,
            &manager,
        )
        .unwrap();
        let angle = ROTARY_ANGLE;
        assert_eq!(quad.shader_values, [-angle, angle, -angle, -angle]);
    }

    #[test]
    fn a_text_sliders_meter_is_a_two_pixel_bar_running_upwards() {
        let skin = Skin::default();
        let (section, manager) = scopes(&skin);
        let bounds = Rect::new(10, 20, 50, 16);
        for (right, x) in [(true, 58.0), (false, 10.0)] {
            let slider =
                Slider::new("osc_1_transpose", SliderStyle::Text).modulation_bar_right(right);
            let value = slider.info.min;
            let quad =
                meter_quad(&slider, bounds, value, [value, value], &section, &manager).unwrap();
            assert_eq!(quad.kind, QuadKind::LinearModulation);
            assert_eq!(quad.rect, RectF::new(x, 20.0, 2.0, 16.0));
            assert!(quad.rotated);
        }
    }

    #[test]
    fn a_linear_sliders_meter_runs_beside_its_track() {
        let skin = Skin::default();
        let (section, manager) = scopes(&skin);
        let slider = Slider::new("osc_1_distortion_phase", SliderStyle::Horizontal);
        let value = slider.info.min;
        let quad = meter_quad(
            &slider,
            Rect::new(100, 40, 80, 30),
            value,
            [value, value],
            &section,
            &manager,
        )
        .unwrap();
        // A 30 pixel slider: a 6 pixel track, 12 pixels either side of it,
        // the outer 7 the margin's; a 3 pixel meter less a 1 pixel border.
        let margin = section.widget_margin() as i32;
        assert_eq!(margin, 5);
        assert_eq!(quad.rect, RectF::new(105.0, 48.0, 70.0, 3.0));
        assert!(!quad.rotated);
        assert_eq!(quad.shader_values, [0.0; 4]);
    }

    #[test]
    fn the_editing_overlay_draws_the_amounts_range_on_the_meter() {
        let skin = Skin::default();
        let (section, manager) = scopes(&skin);
        let centre = manager.color(ColorId::ModulationMeter);

        let knob = Slider::new("osc_1_level", SliderStyle::Rotary);
        let info = knob.info;
        let bounds = Rect::new(100, 50, 40, 60);
        let at = |percent: f32| info.min + percent * (info.max - info.min);
        let meter = place(&knob, bounds, &section, &manager).unwrap();
        let quad = amount_quad(meter, info, at(0.5), 0.25, false, &manager);
        assert_eq!(quad.kind, QuadKind::RotaryModulation);
        // setAmountQuadVertices: two pixels beyond the meter all round.
        assert_eq!(
            quad.rect,
            RectF::new(
                meter.rect.x - 2.0,
                meter.rect.y - 2.0,
                meter.rect.width + 4.0,
                meter.rect.height + 4.0
            )
        );
        assert_eq!(quad.thickness, 2.0);
        assert_eq!([quad.color, quad.alt_color, quad.mod_color], [centre; 3]);
        let angle = ROTARY_ANGLE;
        let expected = [0.0, 0.5 * angle, 0.0, 0.5 * angle];
        for (value, expected) in quad.shader_values.iter().zip(expected) {
            assert!((value - expected).abs() < 1e-5, "{:?}", quad.shader_values);
        }
        // Bipolar: half the amount either side, kept within the arc.
        let quad = amount_quad(meter, info, at(0.9), 0.5, true, &manager);
        let expected = [0.3 * angle, angle, 0.3 * angle, angle];
        for (value, expected) in quad.shader_values.iter().zip(expected) {
            assert!((value - expected).abs() < 1e-5, "{:?}", quad.shader_values);
        }

        // A text slider's: a one pixel line up its bar, in proportions.
        let text = Slider::new("osc_1_transpose", SliderStyle::Text);
        let meter = place(&text, Rect::new(10, 20, 50, 16), &section, &manager).unwrap();
        let quad = amount_quad(meter, text.info, text.info.min, -0.25, false, &manager);
        assert_eq!(quad.kind, QuadKind::LinearModulation);
        assert_eq!(quad.rect, RectF::new(58.0, 20.0, 1.0, 16.0));
        assert!(quad.rotated);
        assert_eq!(quad.shader_values, [-0.25, 0.0, -0.25, 0.0]);

        // A linear slider's: a one pixel line along its meter's top.
        let linear = Slider::new("osc_1_distortion_phase", SliderStyle::Horizontal);
        let meter = place(&linear, Rect::new(100, 40, 80, 30), &section, &manager).unwrap();
        let quad = amount_quad(meter, linear.info, linear.info.min, 1.0, false, &manager);
        assert_eq!(quad.rect, RectF::new(105.0, 48.0, 70.0, 1.0));
        assert!(!quad.rotated);
        assert_eq!(quad.shader_values, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn amount_knobs_have_no_meter() {
        let skin = Skin::default();
        let (section, manager) = scopes(&skin);
        let slider = Slider::new(
            "modulation_1_amount",
            SliderStyle::ModulationKnob {
                arc: crate::colour::Colour(0xffffffff),
                bypass: false,
                aux: false,
            },
        );
        assert!(meter_quad(
            &slider,
            Rect::new(0, 0, 20, 20),
            0.0,
            [0.0; 2],
            &section,
            &manager
        )
        .is_none());
    }
}
