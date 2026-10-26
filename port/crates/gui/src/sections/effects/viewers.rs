// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effect sections' displays: the chorus and delay bars, the delay
//! filters' responses, the distortion's transfer curve and the distortion,
//! flanger and phaser filter responses.
//!
//! Ports `ChorusViewer` (`chorus_section.cpp`), `DelayViewer`
//! (`delay_section.cpp`), `DelayFilterViewer` (`delay_section.h`),
//! `DistortionViewer` and `DistortionFilterResponse`
//! (`distortion_section.cpp`), `FlangerResponse` (`flanger_section.cpp`) and
//! `PhaserResponse` (`phaser_section.cpp`). Each reads its controls as
//! upstream's `getValue(output, slider)` does: the engine's modulated value
//! while that processor is enabled, else the control's own. Each draws two
//! passes, one per stereo lane.

use std::cell::RefCell;

use egui::{Sense, Ui};
use sas_engine::sas_dsp_core::sas_simd::common::CENTS_PER_NOTE;
use sas_engine::sas_dsp_core::sas_simd::{futils, poly_utils, utils, PolyFloat};
use sas_engine::sas_dsp_core::ProcessorRef;
use sas_engine::sas_effects::delay::StereoDelay;
use sas_engine::sas_effects::distortion::Distortion;
use sas_engine::sas_filters::character::{CombFilter, PhaserFilter};
use sas_engine::sas_filters::{DigitalSvf, FilterState, SynthFilter};
use sas_engine::telemetry::{destination_id, Telemetry};

use crate::controls::bars::BarRenderer;
use crate::controls::filter_response::{self, FilterResponseUniforms};
use crate::controls::tempo::TempoMode;
use crate::host::Host;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::line::LineRenderer;
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// What the viewers read: controls through the host, modulated values and
/// status outputs from the engine's telemetry.
pub struct Live<'a> {
    host: &'a Host<'a>,
    telemetry: Option<Telemetry>,
}

impl<'a> Live<'a> {
    pub fn new(host: &'a Host<'a>) -> Live<'a> {
        Live {
            host,
            telemetry: host.model().telemetry(),
        }
    }

    pub fn host(&self) -> &'a Host<'a> {
        self.host
    }

    /// `getValue(output, slider)`: the mono modulation total of control
    /// `name` while its processor is enabled, else the control's value.
    pub fn value(&self, name: &str) -> PolyFloat {
        if let (Some(telemetry), Some(id)) = (&self.telemetry, destination_id(name)) {
            let total = telemetry.read(|snapshot| snapshot.modulation_total(id));
            if total.enabled {
                return PolyFloat::from_array(total.mono);
            }
        }
        PolyFloat::splat(self.host.value(name))
    }

    /// A status output's value; zero without an engine, as upstream's are
    /// before the engine first runs.
    pub fn status(&self, name: &str) -> PolyFloat {
        let value = self
            .telemetry
            .as_ref()
            .and_then(|telemetry| telemetry.read(|snapshot| snapshot.status_by_name(name)))
            .unwrap_or_default();
        PolyFloat::from_array(value)
    }

    /// Whether status outputs come from a running engine (upstream's
    /// `getStatusOutput` found them).
    pub fn has_engine(&self) -> bool {
        self.telemetry.is_some()
    }
}

/// `setColor` and `setFillColors(from, to)` with the fill fading in from
/// `to` less `kWidgetFillFade`.
fn set_colours(line: &mut LineRenderer, scope: &Scope, color: ColorId, fill: ColorId) {
    line.color = scope.color(color);
    let fill_to = scope.color(fill);
    let fill_fade = scope.value(ValueId::WidgetFillFade);
    line.set_fill_colors(fill_to.with_multiplied_alpha(1.0 - fill_fade), fill_to);
}

/// A filled line with the skin's line width and fill centre.
fn filled_line(scope: &Scope, resolution: usize) -> LineRenderer {
    let mut line = LineRenderer::new(resolution, false);
    line.fill = true;
    line.line_width = scope.value(ValueId::WidgetLineWidth);
    line.fill_center = scope.value(ValueId::WidgetFillCenter);
    line
}

/// A viewer's `mouseDown` / `mouseDrag`, which nudge controls by the
/// pointer's movement: `nudges` gives each control's share of a movement
/// of (dx, dy) pixels, as a fraction of its range. The controls hold a host
/// gesture for the length of the drag.
///
/// Upstream adds each movement to the slider's value and clamps it
/// (`slider->setValue(slider->getValue() + …)`). The values are kept here
/// between frames rather than read back from the host, which may apply a
/// set a frame later.
fn drag<const N: usize>(
    ui: &mut Ui,
    host: &Host,
    pixels: &PixelPainter,
    bounds: Rect,
    id: &str,
    names: [&str; N],
    nudges: impl Fn(f32, f32) -> [f32; N],
) {
    let id = ui.scope_id().with(("effect viewer", id));
    let response = ui.interact(pixels.rect(bounds.into()), id, Sense::drag());
    if response.drag_started() {
        let values = names.map(|name| host.value(name));
        ui.data_mut(|data| data.insert_temp(id, values));
        for name in names {
            host.begin(name);
        }
    }
    if response.dragged() {
        let delta = response.drag_delta() * pixels.pixels_per_point();
        if delta.x != 0.0 || delta.y != 0.0 {
            let mut values: [f32; N] = ui
                .data(|data| data.get_temp(id))
                .unwrap_or_else(|| names.map(|name| host.value(name)));
            for ((value, name), amount) in
                values.iter_mut().zip(names).zip(nudges(delta.x, delta.y))
            {
                let info = Host::info(name);
                *value = (*value + amount * (info.max - info.min)).clamp(info.min, info.max);
                host.set(name, *value);
            }
            ui.data_mut(|data| data.insert_temp(id, values));
        }
    }
    if response.drag_stopped() {
        for name in names {
            host.end(name);
        }
    }
}

/// `ChorusViewer`: one bar per chorus voice at its delay time.
pub fn chorus(live: &Live, pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    const DELAYS: usize = 4;
    const MAX_DELAY: f32 = 0.07;
    const MIN_DELAY: f32 = 0.0;
    const DELAY_RANGE: f32 = MAX_DELAY - MIN_DELAY;
    if !live.has_engine() {
        return;
    }
    let mut bars = BarRenderer::new(DELAYS * 4, true);
    bars.bar_width = 0.3;
    bars.scale = 1.0;
    bars.additive = true;
    let color = scope.color(ColorId::WidgetSecondary1);
    let alpha = color.float_alpha();
    bars.color = color.with_float_alpha(alpha + (1.0 - alpha) * alpha);

    let num_voices = (live.host().value("chorus_voices") * 4.0) as usize;
    let delays: Vec<PolyFloat> = (1..=DELAYS)
        .map(|i| live.status(&format!("chorus_delays{i}")))
        .collect();
    for i in 0..bars.total_points() {
        if i < num_voices {
            let delay = 1.0 / delays[i / 4].get(i % 4);
            bars.set_x(i, 2.0 * (delay - MIN_DELAY) / DELAY_RANGE - 1.0);
            bars.set_y(i, 0.5);
            bars.set_bottom(i, -0.5);
        } else {
            bars.set_x(i, -2.0);
        }
    }
    bars.paint(pixels, RectF::from(bounds));
}

/// `DelayViewer`: the dry tap and the echoes fading by the feedback, the
/// left lane above the centre and the right below. Dragging changes the
/// feedback.
pub fn delay(ui: &mut Ui, live: &Live, pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    const NUM_BARS: usize = 50;
    let host = live.host();
    drag(
        ui,
        host,
        pixels,
        bounds,
        "delay",
        ["delay_feedback"],
        |_, dy| [-dy / bounds.width as f32],
    );

    let style = host.value("delay_style") as i32;
    let mono = style == 0;
    let sync = TempoMode::from_value(host.value("delay_sync"));
    let aux_sync = TempoMode::from_value(host.value("delay_aux_sync"));
    let raw_frequency = |index: usize| {
        if index == 1 && !mono {
            live.value("delay_aux_frequency").get(0)
        } else {
            live.value("delay_frequency").get(index)
        }
    };
    let multiplier = |index: usize| {
        let mode = if index == 0 || mono { sync } else { aux_sync };
        match mode {
            TempoMode::TempoDotted => 3.0 / 2.0,
            TempoMode::TempoTriplet => 2.0 / 3.0,
            _ => 1.0,
        }
    };
    let tempo_frequency = |index: usize| {
        const DEFAULT_POWER_OFFSET: f32 = -6.0;
        if index == 1 && !mono {
            live.value("delay_aux_tempo").get(0).round() + DEFAULT_POWER_OFFSET
        } else {
            live.value("delay_tempo").get(index).round() + DEFAULT_POWER_OFFSET
        }
    };
    let frequency = |index: usize| {
        let frequency_mode = sync == TempoMode::Seconds;
        let aux_frequency_mode = aux_sync == TempoMode::Seconds;
        if (index == 0 && frequency_mode)
            || (index == 1 && aux_frequency_mode && !mono)
            || (index == 1 && frequency_mode && mono)
        {
            raw_frequency(index)
        } else {
            tempo_frequency(index)
        }
    };
    let mix = poly_utils::clamp(live.value("delay_dry_wet"), 0.0, 1.0);
    let wet_fade = futils::equal_power_fade(mix);
    let dry_fade = futils::equal_power_fade(-mix + 1.0);

    let mut bars = BarRenderer::new(NUM_BARS, true);
    bars.scale = 1.0;
    bars.additive = false;
    for (index, color) in [(0, ColorId::WidgetPrimary1), (1, ColorId::WidgetPrimary2)] {
        const MAX_SECONDS: f32 = 4.0;
        bars.color = scope.color(color);
        let feedback = live
            .value("delay_feedback")
            .get(index)
            .clamp(-1.0, 1.0)
            .abs();
        let mut wet = wet_fade.get(index);
        let dry = dry_fade.get(index);
        let increment = 2.0 * 2.0f32.powf(-frequency(index)) * multiplier(index) / MAX_SECONDS;
        let other_increment =
            2.0 * 2.0f32.powf(-frequency(1 - index)) * multiplier(1 - index) / MAX_SECONDS;
        let mut even_increment = increment;
        let mut odd_increment = increment;
        let mut x = -1.0;
        if style == 2 {
            if index == 1 {
                x -= other_increment;
            }
            even_increment = increment + other_increment;
            odd_increment = even_increment;
        } else if style == 3 {
            if index == 0 {
                odd_increment = other_increment + increment;
                even_increment = other_increment;
            } else {
                odd_increment = increment;
                even_increment = other_increment + increment;
            }
        }
        bars.bar_width = (NUM_BARS as f32 * increment / 2.0).min(1.0);
        let (mult_top, mult_bottom) = if index == 1 { (0.0, -1.0) } else { (1.0, 0.0) };
        bars.set_y(0, mult_top * dry);
        bars.set_bottom(0, mult_bottom * dry);
        for i in 1..NUM_BARS {
            x += if i % 2 == 1 {
                odd_increment
            } else {
                even_increment
            };
            bars.set_x(i, x);
            bars.set_y(i, mult_top * wet);
            bars.set_bottom(i, mult_bottom * wet);
            wet *= feedback;
        }
        bars.paint(pixels, RectF::from(bounds));
    }
}

/// `DelayFilterViewer` for the chorus (`prefix` "chorus") or the delay
/// ("delay_filter"): the band pass of its feedback filters. Dragging moves
/// the cutoff across and the spread up and down
/// (`ChorusSection::deltaMovement`, `DelaySection::deltaMovement`).
pub fn delay_filter(
    ui: &mut Ui,
    live: &Live,
    pixels: &PixelPainter,
    scope: &Scope,
    bounds: Rect,
    prefix: &str,
) {
    const RESOLUTION: usize = 64;
    const MIDI_DRAW_START: f32 = 8.0;
    const MIDI_DRAW_END: f32 = 132.0;
    const MIN_DB: f32 = -18.0;
    const MAX_DB: f32 = 6.0;
    let host = live.host();
    let cutoff_name = format!("{prefix}_cutoff");
    let spread_name = format!("{prefix}_spread");
    let names = [cutoff_name.as_str(), spread_name.as_str()];
    drag(ui, host, pixels, bounds, prefix, names, |dx, dy| {
        [dx / bounds.width as f32, -dy / bounds.height as f32]
    });

    let cutoff = live.value(&cutoff_name);
    let radius = StereoDelay::get_filter_radius(live.value(&spread_name));
    let high_midi_cutoff = cutoff - radius;
    let low_midi_cutoff = cutoff + radius;

    let mut line = filled_line(scope, RESOLUTION);
    let width = bounds.width as f32;
    let height = bounds.height as f32;
    let cents = CENTS_PER_NOTE as f32;
    for (index, color, fill) in [
        (0, ColorId::WidgetPrimary1, ColorId::WidgetSecondary1),
        (1, ColorId::WidgetPrimary2, ColorId::WidgetSecondary2),
    ] {
        set_colours(&mut line, scope, color, fill);
        let high_midi_cutoff = high_midi_cutoff.get(index);
        let low_midi_cutoff = low_midi_cutoff.get(index);
        let midi_increment = (MIDI_DRAW_END - MIDI_DRAW_START) / (RESOLUTION as f32 - 1.0);
        let mult_increment = utils::cents_to_ratio(midi_increment * cents);
        let mut high_ratio = utils::cents_to_ratio((MIDI_DRAW_START - high_midi_cutoff) * cents);
        let mut low_ratio = utils::cents_to_ratio((MIDI_DRAW_START - low_midi_cutoff) * cents);
        let gain = utils::cents_to_ratio((high_midi_cutoff - low_midi_cutoff) * cents) + 1.0;
        for i in 0..RESOLUTION {
            let high_response = high_ratio / (1.0 + high_ratio * high_ratio).sqrt();
            let low_response = 1.0 / (1.0 + low_ratio * low_ratio).sqrt();
            let response = gain * low_response * high_response;
            let db = utils::magnitude_to_db(response);
            let y = (db - MIN_DB) / (MAX_DB - MIN_DB);
            line.set_x_at(i, width * i as f32 / (RESOLUTION as f32 - 1.0));
            line.set_y_at(i, (1.0 - y) * height);
            high_ratio *= mult_increment;
            low_ratio *= mult_increment;
        }
        line.paint(pixels, RectF::from(bounds), true);
    }
}

/// `DistortionViewer`: the distortion's transfer curve at the current
/// drive. Dragging changes the drive.
pub fn distortion(ui: &mut Ui, live: &Live, pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    const RESOLUTION: usize = 124;
    const DRAW_PERCENT: f32 = 0.9;
    let host = live.host();
    drag(
        ui,
        host,
        pixels,
        bounds,
        "distortion",
        ["distortion_drive"],
        |_, dy| [-dy / bounds.width as f32],
    );

    let kind = host.value("distortion_type") as i32;
    let drive = Distortion::get_drive_value(kind, live.value("distortion_drive"));
    let mut line = LineRenderer::new(RESOLUTION, false);
    line.fill = true;
    line.fill_center = 0.0;
    line.line_width = scope.value(ValueId::WidgetLineWidth);
    let width = bounds.width as f32;
    let y_scale = bounds.height as f32 / 2.0;
    for (index, color, fill) in [
        (1, ColorId::WidgetPrimary2, ColorId::WidgetSecondary2),
        (0, ColorId::WidgetPrimary1, ColorId::WidgetSecondary1),
    ] {
        set_colours(&mut line, scope, color, fill);
        for i in 0..RESOLUTION {
            let t = i as f32 / (RESOLUTION as f32 - 1.0);
            let value = PolyFloat::splat(2.0 * t - 1.0);
            line.set_x_at(i, t * width);
            let result = DRAW_PERCENT * Distortion::get_driven_value(kind, value, drive).get(index);
            line.set_y_at(i, (1.0 - result) * y_scale);
        }
        line.paint(pixels, RectF::from(bounds), true);
    }
}

/// The filter models the responses set up as upstream's viewers do: their
/// own `DigitalSvf`, `CombFilter` and `PhaserFilter`, kept on the GUI
/// thread.
struct Models {
    svf: ProcessorRef,
    comb: ProcessorRef,
    phaser: ProcessorRef,
}

thread_local! {
    static MODELS: RefCell<Option<Models>> = const { RefCell::new(None) };
}

fn with_models<R>(f: impl FnOnce(&Models) -> R) -> R {
    MODELS.with(|models| {
        let mut models = models.borrow_mut();
        let models = models.get_or_insert_with(|| {
            let mut svf = DigitalSvf::new();
            svf.set_drive_compensation(false);
            let phaser = PhaserFilter::create(false);
            phaser.set_sample_rate(200_000);
            Models {
                svf: DigitalSvf::create_with(svf),
                comb: CombFilter::create(CombFilter::MIN_PERIOD),
                phaser,
            }
        });
        f(models)
    })
}

/// `DistortionFilterResponse::loadShader` for each lane: the digital
/// filter set up from `state`, the cutoff kept above `min_cutoff`.
fn distortion_filter_uniforms(state: &FilterState, min_cutoff: f32) -> [FilterResponseUniforms; 2] {
    with_models(|models| {
        models.svf.with(|svf: &mut DigitalSvf, this| {
            svf.setup_filter(this, state);
            [0, 1].map(|index| FilterResponseUniforms {
                midi_cutoff: min_cutoff.max(state.midi_cutoff.get(index)),
                resonance: svf.get_resonance().get(index).clamp(0.0, 2.0),
                mix: 1.0,
                drive: svf.get_drive().get(index),
                db24: if state.style != 0 { 1.0 } else { 0.0 },
                stage0: svf.get_low_amount().get(index),
                stage1: svf.get_band_amount().get(index),
                stage2: svf.get_high_amount().get(index),
                stage3: svf.get_low_amount24(state.style).get(index),
                stage4: svf.get_high_amount24(state.style).get(index),
            })
        })
    })
}

/// `FlangerResponse::loadShader` for each lane: the comb filter set up
/// from `state`, at `mix`.
fn flanger_uniforms(state: &FilterState, mix: PolyFloat) -> [FilterResponseUniforms; 2] {
    with_models(|models| {
        models.comb.with(|comb: &mut CombFilter, this| {
            comb.setup_filter(this, state);
            [0, 1].map(|index| FilterResponseUniforms {
                midi_cutoff: state.midi_cutoff.get(index),
                resonance: comb.resonance().get(index).clamp(-0.99, 0.99),
                drive: 1.0,
                stage0: comb.low_amount().get(index),
                stage1: comb.high_amount().get(index),
                stage2: comb.filter_midi_cutoff().get(index),
                stage3: comb.filter2_midi_cutoff().get(index),
                mix: mix.get(index),
                ..FilterResponseUniforms::default()
            })
        })
    })
}

/// `PhaserResponse::loadShader` for each lane: the phaser filter set up
/// from `state`, at `mix`.
fn phaser_uniforms(state: &FilterState, mix: PolyFloat) -> [FilterResponseUniforms; 2] {
    with_models(|models| {
        models.phaser.with(|phaser: &mut PhaserFilter, this| {
            phaser.setup_filter(this, state);
            [0, 1].map(|index| FilterResponseUniforms {
                midi_cutoff: state.midi_cutoff.get(index),
                resonance: phaser.resonance().get(index),
                db24: if state.style != 0 { 1.0 } else { 0.0 },
                stage0: phaser.peak1_amount().get(index),
                stage1: phaser.peak3_amount().get(index),
                stage2: phaser.peak5_amount().get(index),
                mix: mix.get(index),
                ..FilterResponseUniforms::default()
            })
        })
    })
}

/// Draws a filter response: the right lane's line first (when active),
/// then the left's, through `evaluate`, at `x_positions` (pixels from the
/// left) for the evaluator's own x positions.
#[allow(clippy::too_many_arguments)]
fn paint_response(
    pixels: &PixelPainter,
    scope: &Scope,
    bounds: Rect,
    active: bool,
    uniforms: [FilterResponseUniforms; 2],
    positions: &[(f32, f32)],
    x_offset: [f32; 2],
    x_span: f32,
    evaluate: fn(&FilterResponseUniforms, f32, f32) -> f32,
) {
    let resolution = positions.len();
    let mut line = filled_line(scope, resolution);
    let y_adjust = bounds.height as f32 / 2.0;
    let mut passes = Vec::with_capacity(2);
    if active {
        passes.push((1, ColorId::WidgetPrimary2, ColorId::WidgetSecondary2));
        passes.push((0, ColorId::WidgetPrimary1, ColorId::WidgetSecondary1));
    } else {
        passes.push((
            0,
            ColorId::WidgetPrimaryDisabled,
            ColorId::WidgetSecondaryDisabled,
        ));
    }
    for (index, color, fill) in passes {
        if !x_offset[index].is_finite() {
            continue;
        }
        set_colours(&mut line, scope, color, fill);
        for (i, &(x, y)) in positions.iter().enumerate() {
            let response = evaluate(&uniforms[index], x, y);
            line.set_x_at(
                i,
                x_span * i as f32 / (resolution as f32 - 1.0) + x_offset[index],
            );
            line.set_y_at(i, y_adjust * (1.0 - response));
        }
        line.paint(pixels, RectF::from(bounds), true);
    }
}

/// The line's device x positions `from` to `to`, with y 0.
fn spread_positions(resolution: usize, from: f32, to: f32) -> Vec<(f32, f32)> {
    (0..resolution)
        .map(|i| {
            let t = i as f32 / (resolution as f32 - 1.0);
            (from + t * (to - from), 0.0)
        })
        .collect()
}

/// `DistortionFilterResponse`: the distortion's digital filter, drawn
/// while its filter order is not "None". Dragging moves the cutoff across
/// and the resonance up and down.
pub fn distortion_filter(
    ui: &mut Ui,
    live: &Live,
    pixels: &PixelPainter,
    scope: &Scope,
    bounds: Rect,
    active: bool,
) {
    const RESOLUTION: usize = 256;
    let host = live.host();
    let names = ["distortion_filter_cutoff", "distortion_filter_resonance"];
    drag(
        ui,
        host,
        pixels,
        bounds,
        "distortion filter",
        names,
        |dx, dy| [dx / bounds.width as f32, -dy / bounds.height as f32],
    );

    let state = FilterState {
        midi_cutoff: live.value("distortion_filter_cutoff"),
        resonance_percent: live.value("distortion_filter_resonance"),
        pass_blend: live.value("distortion_filter_blend"),
        ..FilterState::default()
    };
    let min_cutoff = Host::info("distortion_filter_cutoff").min + 0.001;
    let uniforms = distortion_filter_uniforms(&state, min_cutoff);
    let positions = spread_positions(RESOLUTION, -1.0, 1.0);
    paint_response(
        pixels,
        scope,
        bounds,
        active,
        uniforms,
        &positions,
        [0.0; 2],
        bounds.width as f32,
        |uniforms, x, _| filter_response::digital(uniforms, x),
    );
}

/// `FlangerResponse`: the flanger's comb response, three widths wide and
/// slid by the delay frequency. Dragging moves the centre across and the
/// feedback up and down.
pub fn flanger(ui: &mut Ui, live: &Live, pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    const RESOLUTION: usize = 512;
    const COMB_ALTERNATE_PERIOD: usize = 2;
    const MAX_MIDI: f32 = 128.0;
    let host = live.host();
    let names = ["flanger_center", "flanger_feedback"];
    drag(ui, host, pixels, bounds, "flanger", names, |dx, dy| {
        [dx / bounds.width as f32, -dy / bounds.height as f32]
    });

    let mix = live.value("flanger_dry_wet");
    let state = FilterState {
        midi_cutoff: PolyFloat::zero(),
        resonance_percent: live.value("flanger_feedback") * 0.5 + 0.5,
        pass_blend: PolyFloat::splat(1.0),
        ..FilterState::default()
    };
    let uniforms = flanger_uniforms(&state, mix);
    let positions: Vec<(f32, f32)> = (0..RESOLUTION)
        .map(|i| {
            let t = i as f32 / (RESOLUTION as f32 - 1.0);
            (6.0 * t - 4.0, ((i / COMB_ALTERNATE_PERIOD) % 2) as f32)
        })
        .collect();
    let midi = poly_utils::frequency_to_midi_note(live.status("flanger_delay_frequency"));
    let width = bounds.width as f32;
    let offset = [0, 1].map(|index| midi.get(index) * (width / MAX_MIDI) - width * 1.5);
    paint_response(
        pixels,
        scope,
        bounds,
        true,
        uniforms,
        &positions,
        offset,
        width * 3.0,
        filter_response::comb,
    );
}

/// `PhaserResponse`: the phaser's notches around its modulated cutoff.
/// Dragging moves the centre across and the feedback up and down.
pub fn phaser(ui: &mut Ui, live: &Live, pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    const RESOLUTION: usize = 256;
    let host = live.host();
    let names = ["phaser_center", "phaser_feedback"];
    drag(ui, host, pixels, bounds, "phaser", names, |dx, dy| {
        [dx / bounds.width as f32, -dy / bounds.height as f32]
    });

    let mix = live.value("phaser_dry_wet");
    let state = FilterState {
        midi_cutoff: live.status("phaser_cutoff"),
        resonance_percent: live.value("phaser_feedback"),
        pass_blend: live.value("phaser_blend"),
        ..FilterState::default()
    };
    let uniforms = phaser_uniforms(&state, mix);
    let positions = spread_positions(RESOLUTION, -1.0, 1.0);
    paint_response(
        pixels,
        scope,
        bounds,
        true,
        uniforms,
        &positions,
        [0.0; 2],
        bounds.width as f32,
        |uniforms, x, _| filter_response::phaser(uniforms, x),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(cutoff: f32, resonance: f32, blend: f32) -> FilterState {
        FilterState {
            midi_cutoff: PolyFloat::splat(cutoff),
            resonance_percent: PolyFloat::splat(resonance),
            pass_blend: PolyFloat::splat(blend),
            ..FilterState::default()
        }
    }

    /// `PhaserFilter::setupFilter`: the resonance is the feedback percent
    /// (0 to 1), and the blend crossfades the peak counts 1, 3 and 5.
    #[test]
    fn phaser_uniforms_follow_the_filter_setup() {
        let mix = PolyFloat::splat(0.75);
        let [left, right] = phaser_uniforms(&state(60.0, 0.5, 1.0), mix);
        assert_eq!(left, right);
        assert_eq!(left.midi_cutoff, 60.0);
        assert_eq!(left.resonance, 0.5);
        assert_eq!((left.stage0, left.stage1, left.stage2), (0.0, 1.0, 0.0));
        assert_eq!((left.mix, left.db24), (0.75, 0.0));
        let [blended, _] = phaser_uniforms(&state(60.0, 0.5, 0.25), mix);
        assert_eq!(
            (blended.stage0, blended.stage1, blended.stage2),
            (0.75, 0.25, 0.0)
        );
    }

    /// `FlangerResponse::setupFilterState` maps feedback -1..1 to the comb's
    /// resonance percent 0..1, so no feedback is no comb feedback, and full
    /// feedback is clamped to 0.99 for the shader.
    #[test]
    fn flanger_uniforms_clamp_the_comb_feedback() {
        let mix = PolyFloat::splat(1.0);
        let none = flanger_uniforms(&state(0.0, 0.5, 1.0), mix);
        assert_eq!(none[0].resonance, 0.0);
        let full = flanger_uniforms(&state(0.0, 1.0, 1.0), mix);
        assert_eq!(full[0].resonance, 0.99);
        assert_eq!(full[0].drive, 1.0);
        assert!(full[0].stage2.is_finite() && full[0].stage3.is_finite());
    }

    /// `DistortionFilterResponse::loadShader`: the cutoff never goes below
    /// the slider's minimum (plus 0.001), the mix is full, and the 12 dB
    /// style leaves the 24 dB stages out. The shader's resonance is the
    /// SVF's damping, `1 / interpolate(0.5, 16, percent³)`.
    #[test]
    fn distortion_uniforms_keep_the_cutoff_above_the_minimum() {
        let [low, _] = distortion_filter_uniforms(&state(-200.0, 0.0, 0.0), 8.001);
        assert_eq!(low.midi_cutoff, 8.001);
        assert_eq!((low.mix, low.db24), (1.0, 0.0));
        // Blend 0 is all low pass.
        assert_eq!((low.stage0, low.stage1, low.stage2), (1.0, 0.0, 0.0));
        let [mid, _] = distortion_filter_uniforms(&state(60.0, 0.5, 0.0), 8.001);
        assert_eq!(low.resonance, 2.0);
        assert_eq!(mid.midi_cutoff, 60.0);
        assert!((mid.resonance - 1.0 / (0.5 + 15.5 * 0.125)).abs() < 1e-6);
    }
}
