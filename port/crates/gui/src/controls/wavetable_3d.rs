// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The oscillator's wavetable view: the frame playing as a 2D wave, a 3D
//! stack of every frame, or its spectrum, after the oscillator's spectral
//! morph and phase distortion.
//!
//! Ports `Wavetable3d`
//! (`vital/src/interface/editor_components/wavetable_3d.cpp`). It reads its
//! oscillator's controls from the host parameters and, while animating,
//! their modulated values from the telemetry, so the oscillator section only
//! places it and its frame slider.

use std::sync::Arc;

use egui::{PointerButton, Sense};
use sas_engine::sas_oscillators::sas_wavetable::wave_frame::{
    WaveFrame, WAVEFORM_BITS, WAVEFORM_SIZE,
};
use sas_engine::sas_oscillators::sas_wavetable::wavetable::FREQUENCY_BINS;
use sas_engine::sas_oscillators::sas_wavetable::{FourierTransform, Wavetable};
use sas_engine::sas_oscillators::spectral_morph::SPECTRAL_BUFFER_FLOATS;
use sas_engine::sas_oscillators::synth_oscillator::{
    adjust_phase, get_phase_window, interpolate, run_spectral_morph, set_distortion_values_for,
    set_spectral_morph_values_for, uses_distortion_phase,
};
use sas_engine::telemetry::{
    destination_id, status_id, DestinationId, StatusId, TelemetrySnapshot,
};
use sas_simd::{futils, poly_utils, utils, PolyFloat, PolyInt};

use crate::colour::Colour;
use crate::controls::slider::Slider;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::popup::ValuePopup;
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::sections::Panel;
use crate::skin::{ColorId, ValueId};

/// `vital::kNumOscillatorWaveFrames`.
const NUM_FRAMES: usize = 257;
/// `kResolution`: one line point per waveform sample.
const RESOLUTION: usize = WAVEFORM_SIZE;
/// `kBackgroundResolution`: points per frame in the 3D background.
const BACKGROUND_RESOLUTION: usize = 128;
/// `kColorJump`: every 16th background frame takes the first accent colour.
const COLOR_JUMP: usize = 16;
/// `k2dWaveHeightPercent`.
const WAVE_2D_HEIGHT_PERCENT: f32 = 0.25;
/// `kPositionLineWidthRatio`: the end caps' size against the line width.
const POSITION_LINE_WIDTH_RATIO: f32 = 1.8;
/// `UINT_MAX` and `INT_MAX` as `float`.
const UINT_MAX_F: f32 = u32::MAX as f32;
const INT_MAX_F: f32 = i32::MAX as f32;

/// `Wavetable3d::RenderType`, the values of `osc_N_view_2d`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderType {
    Wave3d,
    Wave2d,
    FrequencyAmplitudes,
}

impl RenderType {
    pub fn from_value(value: f32) -> RenderType {
        match value as i32 {
            0 => RenderType::Wave3d,
            1 => RenderType::Wave2d,
            _ => RenderType::FrequencyAmplitudes,
        }
    }
}

/// `setViewSettings`'s arguments, from the skin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewSettings {
    pub horizontal_angle: f32,
    pub vertical_angle: f32,
    pub draw_width: f32,
    pub wave_height: f32,
    pub y_offset: f32,
}

impl ViewSettings {
    /// `OscillatorSection::setSkinValues`.
    pub fn from_skin(scope: &Scope) -> ViewSettings {
        ViewSettings {
            horizontal_angle: scope.value(ValueId::WavetableHorizontalAngle),
            vertical_angle: scope.value(ValueId::WavetableVerticalAngle),
            draw_width: scope.value(ValueId::WavetableDrawWidth),
            wave_height: scope.value(ValueId::WavetableWaveHeight),
            y_offset: scope.value(ValueId::WavetableYOffset),
        }
    }
}

/// `setDimensionValues`: the 3D projection, as fractions of the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dimensions {
    pub wave_range_x: f32,
    pub frame_range_x: f32,
    pub wave_range_y: f32,
    pub frame_range_y: f32,
    pub start_x: f32,
    pub start_y: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub wave_height: f32,
    pub y_offset: f32,
}

impl Dimensions {
    pub fn new(view: &ViewSettings) -> Dimensions {
        let wave_range_x = view.horizontal_angle.cos() * view.draw_width;
        let frame_range_x = -view.horizontal_angle.sin() * view.draw_width;
        let wave_range_y = 2.0 * frame_range_x * view.vertical_angle.cos();
        let frame_range_y = -2.0 * wave_range_x * view.vertical_angle.cos();
        let start_x = 0.5 * (1.0 - wave_range_x - frame_range_x);
        let start_y = 0.5 * (1.0 - wave_range_y - frame_range_y) + view.y_offset;
        let draw_angle = (wave_range_y / wave_range_x).atan();
        Dimensions {
            wave_range_x,
            frame_range_x,
            wave_range_y,
            frame_range_y,
            start_x,
            start_y,
            offset_x: -draw_angle.sin() * 1.5 * view.wave_height,
            offset_y: draw_angle.cos() * 1.5 * view.wave_height,
            wave_height: view.wave_height,
            y_offset: view.y_offset,
        }
    }
}

/// The oscillator's controls the view follows: the slider values, and the
/// telemetry readouts of their modulated values.
struct Controls {
    wave_frame: &'static str,
    spectral_morph_type: &'static str,
    spectral_morph_amount: &'static str,
    distortion_type: &'static str,
    distortion_amount: &'static str,
    distortion_phase: &'static str,
    view_2d: &'static str,
    on: &'static str,
    wave_frame_total: Option<DestinationId>,
    spectral_morph_total: Option<DestinationId>,
    distortion_total: Option<DestinationId>,
    distortion_phase_total: Option<DestinationId>,
    num_voices: Option<StatusId>,
}

impl Controls {
    fn new(index: usize) -> Controls {
        let name = |control: &str| -> &'static str {
            Box::leak(format!("osc_{}_{control}", index + 1).into_boxed_str())
        };
        let wave_frame = name("wave_frame");
        let spectral_morph_amount = name("spectral_morph_amount");
        let distortion_amount = name("distortion_amount");
        let distortion_phase = name("distortion_phase");
        Controls {
            wave_frame,
            spectral_morph_type: name("spectral_morph_type"),
            spectral_morph_amount,
            distortion_type: name("distortion_type"),
            distortion_amount,
            distortion_phase,
            view_2d: name("view_2d"),
            on: name("on"),
            wave_frame_total: destination_id(wave_frame),
            spectral_morph_total: destination_id(spectral_morph_amount),
            distortion_total: destination_id(distortion_amount),
            distortion_phase_total: destination_id(distortion_phase),
            num_voices: status_id("num_voices"),
        }
    }
}

/// What the lines are drawn from (`updateRenderValues`'s state): the four
/// lanes of each modulated control, and the types they were built with.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RenderValues {
    wave_frame: [f32; 4],
    spectral_morph: [f32; 4],
    distortion: [f32; 4],
    distortion_phase: [u32; 4],
    spectral_morph_type: i32,
    distortion_type: i32,
    render_type: RenderType,
}

/// The cached 3D background: the frame strokes, and what they were built
/// from.
struct Background {
    key: (usize, i32, Rect, bool, [u32; 3], [u32; 2]),
    shapes: Vec<egui::Shape>,
}

/// A drag on the view moving the frame slider (`mouseDown` / `mouseDrag`).
#[derive(Clone, Copy, Debug)]
struct Drag {
    /// `current_value_`: the frame the drag has reached, unclamped steps
    /// accumulated from the slider value at the press.
    value: f32,
    /// `last_edit_position_`'s y, in pixels.
    last_y: f32,
    /// Whether the host gesture has begun: on the first movement, so a
    /// click that doesn't move sends nothing.
    begun: bool,
}

/// `Wavetable3d`.
pub struct Wavetable3d {
    index: usize,
    controls: Controls,
    view: ViewSettings,
    dimensions: Dimensions,
    left: LineRenderer,
    right: LineRenderer,
    transform: FourierTransform,
    process_wave_data: Vec<f32>,
    process_frame: WaveFrame,
    last: Option<RenderValues>,
    last_bounds: Option<Rect>,
    /// The wavetable the lines were drawn from (its address and version).
    last_table: Option<(usize, i32)>,
    background: Option<Background>,
    drag: Option<Drag>,
}

impl Wavetable3d {
    /// The view of oscillator `index` (0-based).
    pub fn new(index: usize) -> Wavetable3d {
        let view = ViewSettings {
            horizontal_angle: -0.28,
            vertical_angle: 1.132,
            draw_width: 0.728,
            wave_height: 0.083,
            y_offset: 0.0,
        };
        Wavetable3d {
            index,
            controls: Controls::new(index),
            dimensions: Dimensions::new(&view),
            view,
            left: LineRenderer::new(RESOLUTION + 2, false),
            right: LineRenderer::new(RESOLUTION + 2, false),
            transform: FourierTransform::new(WAVEFORM_BITS),
            process_wave_data: vec![0.0; SPECTRAL_BUFFER_FLOATS],
            process_frame: WaveFrame::default(),
            last: None,
            last_bounds: None,
            last_table: None,
            background: None,
            drag: None,
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// `setViewSettings`.
    pub fn set_view_settings(&mut self, view: ViewSettings) {
        if view != self.view {
            self.view = view;
            self.dimensions = Dimensions::new(&view);
            self.last = None;
        }
    }

    pub fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    /// The render type the oscillator's `view_2d` control selects.
    pub fn render_type(&self, panel: &Panel) -> RenderType {
        RenderType::from_value(panel.host.value(self.controls.view_2d))
    }

    /// `OpenGlComponent::paintBackground` and `Wavetable3d::paintBackground`:
    /// the widget background and, in 3D, every frame of the table.
    pub fn paint_background(
        &mut self,
        pixels: &PixelPainter,
        scope: &Scope,
        bounds: Rect,
        render_type: RenderType,
        active: bool,
        wavetable: Option<&Arc<Wavetable>>,
    ) {
        pixels.fill_rect(bounds, scope.color(ColorId::WidgetBackground));
        if render_type != RenderType::Wave3d {
            return;
        }
        let Some(wavetable) = wavetable else {
            return;
        };
        let background_color = scope.color(ColorId::Body);
        let wave_color1 = scope.color(ColorId::WidgetAccent1);
        let wave_color2 = scope.color(ColorId::WidgetAccent2);
        let key = (
            Arc::as_ptr(wavetable) as usize,
            wavetable.version(),
            bounds,
            active,
            [background_color.0, wave_color1.0, wave_color2.0],
            [
                self.dimensions.wave_range_x.to_bits(),
                self.dimensions.wave_range_y.to_bits(),
            ],
        );
        if self
            .background
            .as_ref()
            .is_none_or(|background| background.key != key)
        {
            let shapes = paint_3d_background(
                pixels,
                wavetable,
                active,
                (background_color, wave_color1, wave_color2),
                bounds,
                &self.dimensions,
            );
            self.background = Some(Background { key, shapes });
        }
        if let Some(background) = &self.background {
            pixels.add_shapes(background.shapes.iter().cloned());
        }
    }

    /// The mouse and the lines (`mouseDown`, `mouseDrag`, `mouseExit`,
    /// `render`). `frame_slider` is the oscillator's wave frame slider and
    /// its bounds: a drag on the view moves it and shows its popup.
    /// `telemetry` is this frame's snapshot, if the engine publishes one;
    /// `animate` is the interface's animate switch.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        panel: &mut Panel,
        bounds: Rect,
        frame_slider: (&Slider, Rect),
        wavetable: Option<&Arc<Wavetable>>,
        telemetry: Option<&TelemetrySnapshot>,
        animate: bool,
    ) {
        self.interact(panel, bounds, frame_slider);

        let render_type = self.render_type(panel);
        let active = panel.host.value(self.controls.on) != 0.0;
        let values = self.render_values(panel, telemetry, animate, render_type);
        let table = wavetable.map(|table| (Arc::as_ptr(table) as usize, table.version()));
        let changed = self.last != Some(values)
            || self.last_bounds != Some(bounds)
            || self.last_table != table;
        if changed {
            if let Some(wavetable) = wavetable {
                match render_type {
                    RenderType::FrequencyAmplitudes => {
                        self.load_spectrum_data(wavetable, &values, bounds, 0);
                        self.load_spectrum_data(wavetable, &values, bounds, 1);
                    }
                    _ => {
                        self.load_wave_data(wavetable, &values, bounds, 0);
                        self.load_wave_data(wavetable, &values, bounds, 1);
                    }
                }
            }
            self.last = Some(values);
            self.last_bounds = Some(bounds);
            self.last_table = table;
        }

        if wavetable.is_some() || render_type == RenderType::FrequencyAmplitudes {
            self.render(panel, bounds, render_type, active, wavetable.is_some());
        }

        // OpenGlLineRenderer::renderCorners: the body colour outside the
        // widget's rounded corners.
        let rounding = panel.scope.value(ValueId::WidgetRoundedCorner);
        let body = panel.scope.color(ColorId::Body);
        for corner in Quad::corners(bounds.into(), rounding, body) {
            panel.quad(corner);
        }
    }

    fn interact(
        &mut self,
        panel: &mut Panel,
        bounds: Rect,
        (frame, frame_bounds): (&Slider, Rect),
    ) {
        let id = panel.ui.scope_id().with(("wavetable 3d", self.index));
        let rect = panel.pixels.rect(bounds.into());
        let response = panel.ui.interact(rect, id, Sense::click_and_drag());
        let ppp = panel.pixels.pixels_per_point();
        let pointer_y = panel
            .ui
            .input(|input| input.pointer.interact_pos())
            .map(|pos| (pos.y - panel.pixels.area().min.y) * ppp);
        let primary_down = panel
            .ui
            .input(|input| input.pointer.button_down(PointerButton::Primary));

        match (
            self.drag,
            response.is_pointer_button_down_on() && primary_down,
            pointer_y,
        ) {
            (None, true, Some(y)) => {
                // mouseDown: from the slider's value, with its popup.
                let value = panel.host.value(frame.name);
                self.drag = Some(Drag {
                    value,
                    last_y: y,
                    begun: false,
                });
                show_popup(panel, frame, frame_bounds, value);
            }
            (Some(mut drag), true, Some(y)) => {
                // mouseDrag: the view's height spans the slider's range.
                let delta = y - drag.last_y;
                if delta != 0.0 {
                    let range = frame.info.max - frame.info.min;
                    drag.value -= delta * range / bounds.height as f32;
                    drag.value = drag.value.clamp(frame.info.min, frame.info.max);
                    if !drag.begun {
                        panel.host.begin(frame.name);
                        drag.begun = true;
                    }
                    panel.host.set(frame.name, drag.value);
                    drag.last_y = y;
                    self.drag = Some(drag);
                }
                show_popup(panel, frame, frame_bounds, drag.value);
            }
            (Some(drag), false, _) => {
                if drag.begun {
                    panel.host.end(frame.name);
                }
                self.drag = None;
            }
            _ => {}
        }

        let hovered = response.hovered() || self.drag.is_some();
        let was_hovered = panel.ui.data_mut(|data| {
            let was = data.get_temp::<bool>(id).unwrap_or(false);
            data.insert_temp(id, hovered);
            was
        });
        if was_hovered
            && !hovered
            && panel
                .overlay
                .value_popup
                .as_ref()
                .is_some_and(|(name, _)| *name == frame.name)
        {
            // mouseExit hides the frame slider's popup.
            panel.overlay.value_popup = None;
        }
    }

    /// `getOutputsTotal` for each control, and the morph and distortion
    /// values `setSpectralMorphValues` / `setDistortionValues` make of them.
    fn render_values(
        &self,
        panel: &Panel,
        telemetry: Option<&TelemetrySnapshot>,
        animate: bool,
        render_type: RenderType,
    ) -> RenderValues {
        let host = panel.host;
        let controls = &self.controls;
        let voices_on = telemetry
            .zip(controls.num_voices)
            .is_some_and(|(snapshot, id)| snapshot.status(id)[0] > 0.0);
        let total = |id: Option<DestinationId>, slider: &str| -> PolyFloat {
            let default = PolyFloat::splat(host.value(slider));
            let (Some(snapshot), Some(id)) = (telemetry, id) else {
                return default;
            };
            let total = snapshot.modulation_total(id);
            if !total.enabled || !animate {
                return default;
            }
            let mono = PolyFloat::from_array(total.mono);
            match (voices_on, total.poly) {
                (true, Some(poly)) => mono + PolyFloat::from_array(poly),
                _ => mono,
            }
        };

        let spectral_morph_type = host.value(controls.spectral_morph_type) as i32;
        let distortion_type = host.value(controls.distortion_type) as i32;

        let wave_frame = total(controls.wave_frame_total, controls.wave_frame);
        let mut spectral_morph = [poly_utils::clamp(
            total(
                controls.spectral_morph_total,
                controls.spectral_morph_amount,
            ),
            0.0,
            1.0,
        )];
        set_spectral_morph_values_for(spectral_morph_type, &mut spectral_morph, false);
        let mut distortion = [poly_utils::clamp(
            total(controls.distortion_total, controls.distortion_amount),
            0.0,
            1.0,
        )];
        set_distortion_values_for(distortion_type, &mut distortion, false);
        let distortion_phase = if uses_distortion_phase(distortion_type) {
            let phase = total(controls.distortion_phase_total, controls.distortion_phase);
            poly_utils::to_int(phase * PolyFloat::splat(UINT_MAX_F) - PolyFloat::splat(INT_MAX_F))
        } else {
            PolyInt::zero()
        };

        RenderValues {
            wave_frame: wave_frame.to_array(),
            spectral_morph: spectral_morph[0].to_array(),
            distortion: distortion[0].to_array(),
            distortion_phase: distortion_phase.to_array(),
            spectral_morph_type,
            distortion_type,
            render_type,
        }
    }

    /// `loadIntoTimeDomain`: `loadFrequencyData`, `warpSpectrumToWave` and
    /// `warpPhase` for lane `index`, into the process frame's time domain.
    fn load_into_time_domain(
        &mut self,
        wavetable: &Wavetable,
        values: &RenderValues,
        index: usize,
    ) {
        let data = wavetable.data();
        let frame = (values.wave_frame[index].round() as i32).clamp(0, data.num_frames as i32 - 1);

        self.process_wave_data.fill(0.0);
        run_spectral_morph(
            values.spectral_morph_type,
            values.spectral_morph[index],
            data,
            frame as usize,
            &mut self.process_wave_data,
            &mut self.transform,
        );

        let distortion = PolyFloat::splat(values.distortion[index]);
        let distortion_phase = PolyInt::from_array(values.distortion_phase);
        let spread = PolyFloat::new(1.0, 2.0, 3.0, 4.0);
        let delta = PolyFloat::splat(1.0 / RESOLUTION as f32);
        let buffer = &self.process_wave_data[4..];
        let time_domain = &mut self.process_frame.time_domain;
        for i in (0..RESOLUTION).step_by(4) {
            let t = (spread + PolyFloat::splat(i as f32)) * delta;
            let original_phase =
                poly_utils::to_int(t * PolyFloat::splat(UINT_MAX_F) - PolyFloat::splat(INT_MAX_F))
                    + PolyInt::splat(i32::MAX as u32);
            let mut adjusted_phase = adjust_phase(
                values.distortion_type,
                original_phase,
                distortion,
                distortion_phase,
            );
            let window = get_phase_window(values.distortion_type, original_phase, adjusted_phase);
            adjusted_phase += distortion_phase;
            let value = window * interpolate(buffer, adjusted_phase);
            time_domain[i..i + 4].copy_from_slice(&value.to_array());
        }
    }

    /// `loadWaveData`: lane `index`'s wave, flat in 2D or at its frame's
    /// place in the stack in 3D.
    fn load_wave_data(
        &mut self,
        wavetable: &Wavetable,
        values: &RenderValues,
        bounds: Rect,
        index: usize,
    ) {
        let width = bounds.width as f32;
        let height = bounds.height as f32;
        let d = self.dimensions;
        let mut wave_height = WAVE_2D_HEIGHT_PERCENT * height;
        let mut wave_width = width;
        let mut wave_range_y = 0.0;
        let mut start_x = 0.0;
        let mut start_y = height / 2.0;
        if values.render_type == RenderType::Wave3d {
            let frame_t = (values.wave_frame[index] / (NUM_FRAMES as f32 - 1.0)).clamp(0.0, 1.0);
            start_x = (0.5 * (1.0 - d.wave_range_x - d.frame_range_x) + d.frame_range_x * frame_t)
                * width;
            start_y = (0.5 * (1.0 - d.wave_range_y - d.frame_range_y)
                + d.y_offset
                + d.frame_range_y * frame_t)
                * height;
            wave_width = d.wave_range_x * width;
            wave_range_y = d.wave_range_y * height;
            wave_height = d.wave_height * height;
        }

        self.load_into_time_domain(wavetable, values, index);

        let renderer = if index == 0 {
            &mut self.left
        } else {
            &mut self.right
        };
        let delta = 1.0 / RESOLUTION as f32;
        let time_domain = &self.process_frame.time_domain;
        for i in 0..RESOLUTION {
            let t = (i + 1) as f32 * delta;
            renderer.set_x_at(i + 1, start_x + t * wave_width);
            renderer.set_y_at(
                i + 1,
                start_y - time_domain[i] * wave_height + t * wave_range_y,
            );
        }
        let average = (renderer.y_at(1) + renderer.y_at(RESOLUTION) - wave_range_y) * 0.5;
        renderer.set_x_at(0, start_x);
        renderer.set_y_at(0, average);
        renderer.set_x_at(RESOLUTION + 1, start_x + wave_width);
        renderer.set_y_at(RESOLUTION + 1, average + wave_range_y);
    }

    /// `loadSpectrumData`: lane `index`'s harmonics on a log frequency scale,
    /// in dB with 3 dB per octave of tilt, mirrored about the centre.
    fn load_spectrum_data(
        &mut self,
        wavetable: &Wavetable,
        values: &RenderValues,
        bounds: Rect,
        index: usize,
    ) {
        const MIN_DB: f32 = -30.0;
        const MAX_DB: f32 = 50.0;
        const DB_RANGE: f32 = MAX_DB - MIN_DB;
        const DB_BOOST_PER_OCTAVE: f32 = 3.0;

        self.load_into_time_domain(wavetable, values, index);
        self.process_frame.to_frequency_domain();
        let frequency_domain = &self.process_frame.frequency_domain;

        let renderer = if index == 0 {
            &mut self.left
        } else {
            &mut self.right
        };
        let width = bounds.width;
        let height = bounds.height;
        let center = height as f32 * 0.5;
        let num_points = (width as usize).min(WAVEFORM_SIZE / 2);
        let scale = 1.0 / num_points as f32;
        let mut last_frequency = 0;
        for i in 0..=num_points {
            let invert_i = WAVEFORM_SIZE + 1 - i;
            let t = i as f32 / num_points as f32;
            let x = t * width as f32;
            renderer.set_x_at(i, x);
            renderer.set_x_at(invert_i, x);

            let position = futils::exp2(PolyFloat::splat(t * (FREQUENCY_BINS as f32 - 1.0))).get(0);
            let frequency = (position as usize).min(WAVEFORM_SIZE / 2 - 1);
            let frequency_t = position - frequency as f32;

            let amplitude_from = frequency_domain[frequency].abs();
            let amplitude_to = frequency_domain[frequency + 1].abs();
            let mut amplitude =
                (amplitude_from + (amplitude_to - amplitude_from) * frequency_t) * scale;
            for f in last_frequency + 1..frequency {
                amplitude = amplitude.max(frequency_domain[f].abs() * scale);
            }
            last_frequency = frequency;

            let db =
                utils::magnitude_to_db(amplitude) + t * FREQUENCY_BINS as f32 * DB_BOOST_PER_OCTAVE;
            let y = (db - MIN_DB).max(0.0) / DB_RANGE;
            renderer.set_y_at(i, y * center + center);
            renderer.set_y_at(invert_i, -y * center + center);
        }

        let end = width as f32 * 1.5;
        for i in width.max(0) as usize..=WAVEFORM_SIZE / 2 {
            let invert_i = WAVEFORM_SIZE + 1 - i;
            renderer.set_x_at(i, end);
            renderer.set_x_at(invert_i, end);
            renderer.set_y_at(i, center);
            renderer.set_y_at(invert_i, center);
        }
    }

    /// `renderWave` / `renderSpectrum`, then the end caps in 3D.
    fn render(
        &mut self,
        panel: &mut Panel,
        bounds: Rect,
        render_type: RenderType,
        active: bool,
        has_table: bool,
    ) {
        let scope = &panel.scope;
        let fill_fade = scope.value(ValueId::WidgetFillFade);
        let (line_left, line_right, fill_left, fill_right) = if active {
            (
                scope.color(ColorId::WidgetPrimary1),
                scope.color(ColorId::WidgetPrimary2),
                scope.color(ColorId::WidgetSecondary1),
                scope.color(ColorId::WidgetSecondary2),
            )
        } else {
            let line = scope.color(ColorId::WidgetPrimaryDisabled);
            let fill = scope.color(ColorId::WidgetSecondaryDisabled);
            (line, line, fill, fill)
        };
        let area = RectF::from(bounds);

        if render_type == RenderType::FrequencyAmplitudes {
            for renderer in [&mut self.left, &mut self.right] {
                renderer.fill = true;
                renderer.line_width = 2.5;
            }
            self.right.color = line_right;
            self.left.color = line_left;
            self.right.set_fill_colors(
                fill_right.with_multiplied_alpha(1.0 - fill_fade),
                fill_right,
            );
            self.left
                .set_fill_colors(fill_left.with_multiplied_alpha(1.0 - fill_fade), fill_left);
            if has_table {
                self.right.paint(panel.pixels, area, true);
                self.left.paint(panel.pixels, area, true);
            }
            return;
        }

        let line_width = scope.value(ValueId::WidgetLineWidth);
        let fill_center = scope.value(ValueId::WidgetFillCenter);
        for renderer in [&mut self.left, &mut self.right] {
            renderer.fill = render_type == RenderType::Wave2d;
            renderer.line_width = line_width;
            renderer.fill_center = fill_center;
        }
        self.left.color = line_left;
        self.right.color = line_right;
        self.left
            .set_fill_colors(fill_left.with_multiplied_alpha(1.0 - fill_fade), fill_left);
        self.right.set_fill_colors(
            fill_right.with_multiplied_alpha(1.0 - fill_fade),
            fill_right,
        );
        self.left.paint(panel.pixels, area, true);
        self.right.paint(panel.pixels, area, true);

        if render_type == RenderType::Wave3d {
            let line_left = scope.color(ColorId::WidgetPrimary1);
            let line_right = scope.color(ColorId::WidgetPrimary2);
            let body = scope.color(ColorId::Body);
            let background = scope.color(ColorId::WidgetBackground);
            for (renderer, color) in [(&self.right, line_right), (&self.left, line_left)] {
                for quad in position_caps(
                    renderer, bounds, color, active, body, background, line_width,
                ) {
                    panel.quad(quad);
                }
            }
        }
    }
}

/// `SynthSlider::showPopup` for the frame slider, at `value`.
fn show_popup(panel: &mut Panel, frame: &Slider, frame_bounds: Rect, value: f32) {
    let popup = ValuePopup {
        text: frame.popup_text(value),
        source: frame_bounds,
        placement: frame.popup_placement,
    };
    panel.overlay.value_popup = Some((frame.name, popup));
}

/// `drawPosition`: rings on the first and last point of a 3D wave line.
fn position_caps(
    renderer: &LineRenderer,
    bounds: Rect,
    color: Colour,
    active: bool,
    body: Colour,
    background: Colour,
    line_width: f32,
) -> [Quad; 2] {
    let color = if active {
        color
    } else {
        color.desaturated().interpolated_with(body, 0.5)
    };
    let size = POSITION_LINE_WIDTH_RATIO * line_width;
    let cap = |x: f32, y: f32| {
        let rect = RectF::new(
            bounds.x as f32 + x - 0.5 * size,
            bounds.y as f32 + y - 0.5 * size,
            size,
            size,
        );
        let mut quad = Quad::new(QuadKind::Ring, rect, color);
        quad.alt_color = color.interpolated_with(background, 0.5);
        quad.thickness = size / 5.0;
        quad
    };
    [
        cap(renderer.x_at(0), renderer.y_at(0)),
        cap(renderer.x_at(RESOLUTION), renderer.y_at(RESOLUTION)),
    ]
}

/// `paint3dBackground`: every frame from the last to the first, stroked
/// with a gradient from its accent colour into the body.
fn paint_3d_background(
    pixels: &PixelPainter,
    wavetable: &Wavetable,
    active: bool,
    (background_color, wave_color1, wave_color2): (Colour, Colour, Colour),
    bounds: Rect,
    d: &Dimensions,
) -> Vec<egui::Shape> {
    let width = bounds.width as f32;
    let height = bounds.height as f32;
    let (left, top) = (bounds.x as f32, bounds.y as f32);
    let increment = WAVEFORM_SIZE / BACKGROUND_RESOLUTION;
    let mut shapes = Vec::with_capacity(NUM_FRAMES);
    for f in (0..NUM_FRAMES).rev() {
        let frame_t = f as f32 / (NUM_FRAMES as f32 - 1.0);
        let wave_start_x = d.start_x + frame_t * d.frame_range_x;
        let wave_start_y = d.start_y + frame_t * d.frame_range_y;
        let buffer = wavetable.buffer(f);

        let loop_value = 0.5 * (buffer[0] + buffer[WAVEFORM_SIZE - 1]);
        let loop_y_offset = -d.wave_height * loop_value;
        let mut points = Vec::with_capacity(BACKGROUND_RESOLUTION + 2);
        points.push((
            left + wave_start_x * width,
            top + (wave_start_y + loop_y_offset) * height,
        ));
        for i in (0..WAVEFORM_SIZE).step_by(increment) {
            let wave_t = i as f32 / (WAVEFORM_SIZE as f32 - 1.0);
            let y_offset = -d.wave_height * buffer[i];
            let x = wave_start_x + wave_t * d.wave_range_x;
            let y = wave_start_y + wave_t * d.wave_range_y + y_offset;
            points.push((left + x * width, top + y * height));
        }
        points.push((
            left + (wave_start_x + d.wave_range_x) * width,
            top + (wave_start_y + d.wave_range_y + loop_y_offset) * height,
        ));

        let mut wave_color = if f % COLOR_JUMP == 0 {
            wave_color1
        } else {
            wave_color2
        };
        if !active {
            wave_color = wave_color
                .desaturated()
                .interpolated_with(background_color, 0.5);
        }
        let wave_dip = background_color.with_alpha(wave_color.alpha());
        shapes.push(pixels.gradient_polyline(
            &points,
            1.0,
            (
                wave_color,
                (left + wave_start_x * width, top + d.start_y * height),
            ),
            (
                wave_dip,
                (
                    left + (wave_start_x + d.offset_x) * width,
                    top + (d.start_y + d.offset_y) * height,
                ),
            ),
        ));
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_view_dimensions_follow_set_dimension_values() {
        let view = ViewSettings {
            horizontal_angle: -0.28,
            vertical_angle: 1.132,
            draw_width: 0.728,
            wave_height: 0.083,
            y_offset: 0.0,
        };
        let d = Dimensions::new(&view);
        let wave_range_x = (-0.28f32).cos() * 0.728;
        let frame_range_x = -(-0.28f32).sin() * 0.728;
        assert_eq!(d.wave_range_x, wave_range_x);
        assert_eq!(d.frame_range_x, frame_range_x);
        assert_eq!(d.wave_range_y, 2.0 * frame_range_x * 1.132f32.cos());
        assert_eq!(d.frame_range_y, -2.0 * wave_range_x * 1.132f32.cos());
        assert_eq!(d.start_x, 0.5 * (1.0 - d.wave_range_x - d.frame_range_x));
    }

    #[test]
    fn render_types_follow_view_2d() {
        assert_eq!(RenderType::from_value(0.0), RenderType::Wave3d);
        assert_eq!(RenderType::from_value(1.0), RenderType::Wave2d);
        assert_eq!(RenderType::from_value(2.0), RenderType::FrequencyAmplitudes);
    }

    /// One frame of a fundamental and a third harmonic at a quarter of its
    /// level.
    fn test_wavetable() -> Wavetable {
        let mut frame = WaveFrame::default();
        for i in 0..WAVEFORM_SIZE {
            let phase = std::f32::consts::TAU * i as f32 / WAVEFORM_SIZE as f32;
            frame.time_domain[i] = 0.8 * phase.sin() + 0.2 * (3.0 * phase).sin();
        }
        frame.to_frequency_domain();
        let mut wavetable = Wavetable::new(NUM_FRAMES);
        wavetable.load_wave_frame(&frame);
        wavetable
    }

    fn values(render_type: RenderType, wave_frame: f32) -> RenderValues {
        RenderValues {
            wave_frame: [wave_frame; 4],
            spectral_morph: [0.0; 4],
            distortion: [0.0; 4],
            distortion_phase: [0; 4],
            spectral_morph_type: 0,
            distortion_type: 0,
            render_type,
        }
    }

    /// With no morph and no distortion the line is the frame itself, two
    /// samples on: point i + 1 sits at phase (i + 1) / 2048, and the
    /// oscillator's interpolation reads the morph buffer from its padded
    /// start, as it does for upstream's view.
    #[test]
    fn the_2d_wave_is_the_frame_a_quarter_of_the_height_high() {
        let wavetable = test_wavetable();
        let mut view = Wavetable3d::new(0);
        let bounds = Rect::new(10, 20, 300, 120);
        view.load_wave_data(&wavetable, &values(RenderType::Wave2d, 0.0), bounds, 0);
        let buffer = wavetable.buffer(0);
        let line = &view.left;
        for i in 0..RESOLUTION {
            let x = (i + 1) as f32 / RESOLUTION as f32 * 300.0;
            assert!((line.x_at(i + 1) - x).abs() < 1e-3, "x at {i}");
            let expected = 60.0 - buffer[(i + 2) % WAVEFORM_SIZE] * 0.25 * 120.0;
            assert!(
                (line.y_at(i + 1) - expected).abs() < 0.05,
                "y at {i}: {} vs {expected}",
                line.y_at(i + 1)
            );
        }
        // The ends meet at the loop's average.
        assert_eq!(line.x_at(0), 0.0);
        assert_eq!(line.x_at(RESOLUTION + 1), 300.0);
        assert_eq!(line.y_at(0), line.y_at(RESOLUTION + 1));
    }

    #[test]
    fn the_3d_wave_starts_at_its_frame_in_the_stack() {
        let wavetable = test_wavetable();
        let mut view = Wavetable3d::new(0);
        let bounds = Rect::new(0, 0, 300, 120);
        let d = view.dimensions();
        for frame in [0.0, 128.0, 256.0] {
            view.load_wave_data(&wavetable, &values(RenderType::Wave3d, frame), bounds, 1);
            let frame_t = frame / 256.0;
            let start_x = (0.5 * (1.0 - d.wave_range_x - d.frame_range_x)
                + d.frame_range_x * frame_t)
                * 300.0;
            assert_eq!(view.right.x_at(0), start_x);
            assert_eq!(
                view.right.x_at(RESOLUTION + 1),
                start_x + d.wave_range_x * 300.0
            );
            let rise = view.right.y_at(RESOLUTION + 1) - view.right.y_at(0);
            assert!((rise - d.wave_range_y * 120.0).abs() < 1e-3);
        }
    }

    #[test]
    fn the_spectrum_mirrors_about_the_centre() {
        let wavetable = test_wavetable();
        let mut view = Wavetable3d::new(0);
        let bounds = Rect::new(0, 0, 200, 100);
        view.load_spectrum_data(
            &wavetable,
            &values(RenderType::FrequencyAmplitudes, 0.0),
            bounds,
            0,
        );
        let line = &view.left;
        for i in 0..=200 {
            let invert = WAVEFORM_SIZE + 1 - i;
            assert_eq!(line.x_at(i), line.x_at(invert));
            assert!((line.y_at(i) + line.y_at(invert) - 100.0).abs() < 1e-3);
            assert!(line.y_at(i) >= 50.0);
        }
        // The fundamental (bin 1, at the left edge) stands well above the
        // floor; the empty bins between the harmonics sit on it.
        assert!(line.y_at(0) > 75.0, "{}", line.y_at(0));
        let floor = (0..=200).map(|i| line.y_at(i)).fold(f32::MAX, f32::min);
        assert_eq!(floor, 50.0);
        // Points past the width park off the right edge, at the centre.
        assert_eq!(line.x_at(300), 300.0);
        assert_eq!(line.y_at(300), 50.0);
    }
}
