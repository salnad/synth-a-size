// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The header's volume control: two peak meters and the volume arrow.
//!
//! Ports `VolumeSection` (`vital/src/interface/editor_sections/volume_section.cpp`)
//! and `PeakMeterViewer` (`editor_components/peak_meter_viewer.cpp`). The
//! meters read the engine's `peak_meter` and `peak_meter_memory` status
//! outputs from the telemetry snapshot.

use std::cell::Cell;

use super::Panel;
use crate::colour::Colour;
use crate::controls::slider::{Slider, SliderStyle};
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `PeakMeterViewer`'s range and how fast the clip colour fades per frame.
const MIN_DB: f32 = -80.0;
const MAX_DB: f32 = 6.0;
const CLAMP_DECAY: f32 = 0.014;

/// `VolumeSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeLayout {
    pub bounds: Rect,
    pub peak_meter_left: Rect,
    pub peak_meter_right: Rect,
    pub volume: Rect,
    /// The arrow's tip and base, from the volume slider's top.
    pub point_y: i32,
    pub end_y: i32,
}

impl VolumeLayout {
    pub fn new(bounds: Rect) -> VolumeLayout {
        let meter_height = bounds.height / 8;
        let volume_height = (meter_height as f32 * 6.0) as i32;
        let end_volume = (meter_height as f32 * 3.5) as i32;
        let padding = 1;
        let buffer = bounds.height / 2 - meter_height;
        let left = Rect::new(0, buffer, bounds.width, meter_height);
        let right = Rect::new(0, left.bottom() + padding, bounds.width, meter_height);
        let volume = Rect::new(0, buffer, bounds.width, volume_height);
        VolumeLayout {
            bounds,
            peak_meter_left: left.translated(bounds.x, bounds.y),
            peak_meter_right: right.translated(bounds.x, bounds.y),
            volume: volume.translated(bounds.x, bounds.y),
            point_y: right.bottom() + padding - buffer,
            end_y: end_volume,
        }
    }
}

/// What the meters show this frame: `peak_meter` and `peak_meter_memory`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PeakLevels {
    pub peak: [f32; 2],
    pub memory: [f32; 2],
}

pub struct VolumeSection {
    volume: Slider,
    /// `PeakMeterViewer::clamped_` for each meter.
    clamped: Cell<[f32; 2]>,
    /// The egui frame the clamp last decayed in.
    clamped_frame: Cell<Option<u64>>,
}

impl Default for VolumeSection {
    fn default() -> VolumeSection {
        VolumeSection {
            volume: Slider::new(
                "volume",
                SliderStyle::Arrow {
                    point_y: 0,
                    end_y: 1,
                },
            ),
            clamped: Cell::new([0.0; 2]),
            clamped_frame: Cell::new(None),
        }
    }
}

/// A meter position in the component's device x for a magnitude.
fn meter_position(value: f32) -> f32 {
    let db = 20.0 * value.log10();
    let mut t = ((db - MIN_DB) / (MAX_DB - MIN_DB)).max(0.0);
    t *= t;
    -1.0 + 2.0 * t
}

impl VolumeSection {
    /// `VolumeSection::paintBackground` with the meters'
    /// `PeakMeterViewer::paintBackground`.
    pub fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, layout: &VolumeLayout) {
        for meter in [layout.peak_meter_left, layout.peak_meter_right] {
            let mut t = -MIN_DB / (MAX_DB - MIN_DB);
            t *= t;
            let x = meter.width as f32 * t;
            let background = scope.color(ColorId::WidgetBackground);
            let (left, top) = (meter.x as f32, meter.y as f32);
            let (width, height) = (meter.width as f32, meter.height as f32);
            pixels.fill_rect(
                RectF::new(left + x, top, width - x, height),
                background.interpolated_with(scope.color(ColorId::Background), 0.5),
            );
            pixels.fill_rect(RectF::new(left, top, x, height), background);
            pixels.fill_rect(
                Rect::new(meter.x + x as i32, meter.y, 1, meter.height),
                scope.color(ColorId::LightenScreen),
            );
        }

        let ticks_y = layout.peak_meter_right.bottom() + scope.value(ValueId::Padding) as i32;
        let tick_height = layout.peak_meter_right.height / 2;
        let info = self.volume.info;
        for decibel in (-66..=6).step_by(6) {
            let offset = decibel as f32 - info.post_offset;
            let percent = offset * offset / (info.max - info.min);
            let x = (percent * layout.bounds.width as f32) as i32;
            pixels.fill_rect(
                Rect::new(layout.bounds.x + x, ticks_y, 1, tick_height),
                scope.color(ColorId::LightenScreen),
            );
        }
    }

    /// The meters (`PeakMeterViewer::render`, only while animating) and the
    /// volume arrow.
    pub fn show(&self, panel: &mut Panel, layout: &VolumeLayout, levels: Option<PeakLevels>) {
        if let Some(levels) = levels {
            let mut clamped = self.clamped.get();
            // Once per render, as upstream's: egui may run a frame in more
            // than one pass.
            let frame = panel.ui.ctx().cumulative_frame_nr();
            let decaying = self.clamped_frame.get() != Some(frame);
            self.clamped_frame.set(Some(frame));
            for (index, meter) in [layout.peak_meter_left, layout.peak_meter_right]
                .into_iter()
                .enumerate()
            {
                if levels.peak[index] > 1.0 || levels.memory[index] > 1.0 {
                    clamped[index] = 1.0;
                }
                let (from, to) = if clamped[index] > 0.0 {
                    (ColorId::WidgetAccent1, ColorId::WidgetAccent2)
                } else {
                    (ColorId::WidgetSecondary1, ColorId::WidgetSecondary2)
                };
                let (from, to) = (panel.scope.color(from), panel.scope.color(to));
                let level = meter_position(levels.peak[index]);
                let memory = meter_position(levels.memory[index]);
                let one_pixel = 2.0 / meter.width as f32;
                panel.quad(gain_meter(meter, -1.0, level, from, to));
                panel.quad(gain_meter(meter, memory - one_pixel, memory, from, to));
                if decaying {
                    clamped[index] = (clamped[index] - CLAMP_DECAY).max(0.0);
                }

                let rounding = (meter.height as f32 / 3.0)
                    .min(panel.scope.value(ValueId::WidgetRoundedCorner) * 0.5);
                for corner in Quad::corners(
                    meter.into(),
                    rounding,
                    panel.scope.color(ColorId::Background),
                ) {
                    panel.quad(corner);
                }
            }
            self.clamped.set(clamped);
        }

        let mut volume = self.volume;
        volume.style = SliderStyle::Arrow {
            point_y: layout.point_y,
            end_y: layout.end_y,
        };
        panel.slider(&volume, layout.volume);
    }
}

/// One gain meter quad from device x `from_x` to `to_x` across `meter`.
fn gain_meter(meter: Rect, from_x: f32, to_x: f32, from: Colour, to: Colour) -> Quad {
    let left = meter.x as f32 + (from_x + 1.0) * 0.5 * meter.width as f32;
    let right = meter.x as f32 + (to_x + 1.0) * 0.5 * meter.width as f32;
    let rect = RectF::new(
        left,
        meter.y as f32,
        (right - left).max(0.0),
        meter.height as f32,
    );
    let mut quad = Quad::new(QuadKind::GainMeter, rect, from);
    quad.alt_color = to;
    quad.coordinates = [from_x, -1.0, to_x - from_x, 2.0];
    quad
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meters_square_the_db_position() {
        assert_eq!(meter_position(0.0), -1.0);
        // 0 dB is 80 of 86 dB, squared.
        let t = (80.0f32 / 86.0) * (80.0 / 86.0);
        assert!((meter_position(1.0) - (-1.0 + 2.0 * t)).abs() < 1e-6);
    }
}
