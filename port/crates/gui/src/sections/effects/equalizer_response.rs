// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The EQ's and the reverb's response display, with a handle per band.
//! Ports `EqualizerResponse`
//! (`vital/src/interface/editor_components/equalizer_response.cpp`).

use std::cell::RefCell;

use egui::{Sense, Ui};
use sas_engine::sas_dsp_core::sas_simd::{utils, PolyFloat};
use sas_engine::sas_dsp_core::ProcessorRef;
use sas_engine::sas_filters::synth_filter::style;
use sas_engine::sas_filters::{DigitalSvf, FilterState, SynthFilter};

use super::viewers::Live;
use crate::controls::filter_response::{self, EqResponseUniforms};
use crate::controls::slider::Slider;
use crate::host::Host;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::popup::ValuePopup;
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::scope::Scope;
use crate::sections::Overlay;
use crate::skin::{ColorId, ValueId};

/// `kResolution`.
const RESOLUTION: usize = 128;
/// `kViewSampleRate`.
const VIEW_SAMPLE_RATE: i32 = 100_000;
/// `kDefaultDbBufferRatio`.
pub const DEFAULT_DB_BUFFER_RATIO: f32 = 0.2;

/// One band's controls (`setLowSliders` and the like).
#[derive(Clone, Copy, Debug)]
pub struct EqBand {
    pub cutoff: &'static str,
    pub resonance: Option<&'static str>,
    pub gain: &'static str,
}

/// One `EqualizerResponse`, set up for this frame.
#[derive(Clone, Copy, Debug)]
pub struct EqualizerResponse {
    pub low: EqBand,
    /// The EQ's middle band; the reverb has none.
    pub band: Option<EqBand>,
    pub high: EqBand,
    /// `high_pass_`, `notch_` and `low_pass_`: the EQ's band modes.
    pub high_pass: bool,
    pub notch: bool,
    pub low_pass: bool,
    pub active: bool,
    pub draw_frequency_lines: bool,
    /// `db_buffer_ratio_`: the headroom above and below the gain range.
    pub db_buffer_ratio: f32,
}

/// A handle held since a press on it: the band, its cutoff and gain as the
/// drag has set them (the host may apply a set a frame later), and whether
/// the drag has moved it yet, which opens its gestures.
#[derive(Clone, Copy, Debug)]
struct Drag {
    band: usize,
    cutoff: f32,
    gain: f32,
    moved: bool,
}

/// Each band's handle position, in pixels from the display's top left
/// (none for a band the display doesn't have).
type Handles = [Option<(f32, f32)>; 3];

/// What the display keeps between frames.
#[derive(Clone, Copy, Debug, Default)]
struct ResponseState {
    drag: Option<Drag>,
    /// The handles as last drawn, and the band drawn selected. With no band
    /// selected upstream draws its control points where they last were.
    drawn: Option<(usize, Handles)>,
}

thread_local! {
    /// The response's own filters (`low_filter_`, `band_filter_`,
    /// `high_filter_`), kept on the GUI thread.
    static FILTERS: RefCell<Option<[ProcessorRef; 3]>> = const { RefCell::new(None) };
}

fn with_filters<R>(f: impl FnOnce(&[ProcessorRef; 3]) -> R) -> R {
    FILTERS.with(|filters| {
        let mut filters = filters.borrow_mut();
        let filters = filters.get_or_insert_with(|| {
            [true, false, true].map(|compensation_off| {
                let mut svf = DigitalSvf::new();
                if compensation_off {
                    svf.set_drive_compensation(false);
                }
                let processor = DigitalSvf::create_with(svf);
                processor.set_sample_rate(VIEW_SAMPLE_RATE);
                processor
            })
        });
        f(filters)
    })
}

impl EqualizerResponse {
    fn bands(&self) -> [Option<EqBand>; 3] {
        [Some(self.low), self.band, Some(self.high)]
    }

    /// `min_db_` and `max_db_`: the low gain's range and its buffer.
    fn db_range(&self) -> (f32, f32) {
        let info = Host::info(self.low.gain);
        let buffer = (info.max - info.min) * self.db_buffer_ratio;
        (info.min - buffer, info.max + buffer)
    }

    /// `getOutputTotal`: the modulated value while the processor runs and
    /// the display is active, else the control's.
    fn value(&self, live: &Live, name: Option<&str>) -> PolyFloat {
        let Some(name) = name else {
            return PolyFloat::zero();
        };
        if !self.active {
            return PolyFloat::splat(live.host().value(name));
        }
        live.value(name)
    }

    /// `computeFilterCoefficients`, then the shader's uniforms for `lane`.
    fn uniforms(&self, live: &Live) -> [EqResponseUniforms; 2] {
        let bands = self.bands();
        let states: [FilterState; 3] = std::array::from_fn(|i| {
            let band = bands[i];
            let mut state = FilterState {
                midi_cutoff: self.value(live, band.map(|b| b.cutoff)),
                resonance_percent: self.value(live, band.and_then(|b| b.resonance)),
                gain: self.value(live, band.map(|b| b.gain)),
                ..FilterState::default()
            };
            let (filter_style, blend) = match i {
                0 if self.high_pass => (style::K12_DB, 2.0),
                0 => (style::SHELVING, 0.0),
                1 if self.notch => (style::NOTCH_PASS_SWAP, 1.0),
                1 => (style::SHELVING, 1.0),
                _ if self.low_pass => (style::K12_DB, 0.0),
                _ => (style::SHELVING, 2.0),
            };
            state.style = filter_style;
            state.pass_blend = PolyFloat::splat(blend);
            state
        });
        with_filters(|filters| {
            let mut uniforms = [EqResponseUniforms::default(); 2];
            for (i, filter) in filters.iter().enumerate() {
                filter.with(|svf: &mut DigitalSvf, this| {
                    svf.setup_filter(this, &states[i]);
                    for (lane, uniform) in uniforms.iter_mut().enumerate() {
                        uniform.midi_cutoff[i] = svf.get_midi_cutoff().get(lane);
                        uniform.resonance[i] = svf.get_resonance().get(lane);
                        uniform.low_amount[i] = svf.get_low_amount().get(lane);
                        uniform.band_amount[i] = svf.get_band_amount().get(lane);
                        uniform.high_amount[i] = svf.get_high_amount().get(lane);
                    }
                });
            }
            uniforms
        })
    }

    /// `getLowPosition` and the like: where band `band`'s handle is, in
    /// pixels from the display's top left, for its cutoff and gain.
    fn position(&self, bounds: Rect, band: EqBand, cutoff: f32, gain: f32) -> (f32, f32) {
        let info = Host::info(band.cutoff);
        let (min_db, max_db) = self.db_range();
        let x = bounds.width as f32 * (cutoff - info.min) / (info.max - info.min);
        let y = bounds.height as f32 * (max_db - gain) / (max_db - min_db);
        (x, y)
    }

    /// `getHoveredBand`: the band whose handle is nearest the pointer,
    /// within 6% of the width.
    fn hovered_band(&self, positions: &Handles, width: f32, (x, y): (f32, f32)) -> Option<usize> {
        let grab_distance = 0.06 * width;
        let distance = |position: Option<(f32, f32)>| {
            position.map(|(px, py)| (px - x) * (px - x) + (py - y) * (py - y))
        };
        let [low, band, high] = positions.map(distance);
        let (low, high) = (low?, high?);
        let mut min = (grab_distance * grab_distance).min(low).min(high);
        if let Some(band) = band {
            min = min.min(band);
        }
        if low <= min {
            return Some(0);
        }
        if band.is_some_and(|band| band <= min) {
            return Some(1);
        }
        if high <= min {
            return Some(2);
        }
        None
    }

    /// Handles the mouse and draws the display. `selected` is the band
    /// drawn selected (0 low, 1 band, 2 high), or none after a press away
    /// from the handles. A press on a handle selects its band, which this
    /// returns for the section's tabs (`Listener::lowBandSelected` and the
    /// like). `popup` is the cutoff knob, for its value popup while a handle
    /// is dragged. `under` draws what sits between the frequency lines and
    /// the response (the EQ's spectrogram).
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &self,
        ui: &mut Ui,
        live: &Live,
        pixels: &PixelPainter,
        scope: &Scope,
        overlay: &mut Overlay,
        bounds: Rect,
        selected: &mut Option<usize>,
        popup: Option<(Slider, Rect)>,
        under: &mut dyn FnMut(&PixelPainter),
    ) -> Option<usize> {
        let host = live.host();
        let id = ui.scope_id().with(("equalizer response", self.low.cutoff));
        let rect = pixels.rect(bounds.into());
        let response = ui.interact(rect, id, Sense::click_and_drag());
        let mut state: ResponseState = ui.data(|data| data.get_temp(id)).unwrap_or_default();
        let bands = self.bands();
        let positions = |drag: Option<Drag>| -> Handles {
            std::array::from_fn(|i| {
                let band = bands[i]?;
                let (cutoff, gain) = match drag.filter(|drag| drag.band == i) {
                    Some(drag) => (drag.cutoff, drag.gain),
                    None => (host.value(band.cutoff), host.value(band.gain)),
                };
                Some(self.position(bounds, band, cutoff, gain))
            })
        };
        let ppp = pixels.pixels_per_point();
        let local = |pos: egui::Pos2| ((pos.x - rect.min.x) * ppp, (pos.y - rect.min.y) * ppp);

        let mut chosen = None;
        let (pressed, moved) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.delta() != egui::Vec2::ZERO,
            )
        });
        if pressed && response.is_pointer_button_down_on() {
            // mouseDown: selects the band under the pointer (none away from
            // the handles); only a drag moves it.
            let band = response.interact_pointer_pos().and_then(|pos| {
                self.hovered_band(&positions(None), bounds.width as f32, local(pos))
            });
            *selected = band;
            chosen = band;
            state.drag = band.map(|band| {
                let controls = bands[band].expect("a hovered band exists");
                Drag {
                    band,
                    cutoff: host.value(controls.cutoff),
                    gain: host.value(controls.gain),
                    moved: false,
                }
            });
        } else if let Some(mut drag) = state.drag {
            let controls = bands[drag.band].expect("a dragged band exists");
            if response.is_pointer_button_down_on() {
                if let (true, Some(pos)) = (moved, response.interact_pointer_pos()) {
                    // mouseDrag: moveFilterSettings.
                    if !drag.moved {
                        host.begin(controls.cutoff);
                        host.begin(controls.gain);
                        drag.moved = true;
                    }
                    let (x, y) = local(pos);
                    let cutoff = Host::info(controls.cutoff);
                    let ratio = (x / bounds.width as f32).clamp(0.0, 1.0);
                    drag.cutoff = ratio * (cutoff.max - cutoff.min) + cutoff.min;
                    host.set(controls.cutoff, drag.cutoff);
                    let gain = Host::info(controls.gain);
                    let height = bounds.height as f32;
                    let local_position = y - 0.5 * self.db_buffer_ratio * height;
                    let ratio =
                        (local_position / ((1.0 - self.db_buffer_ratio) * height)).clamp(0.0, 1.0);
                    drag.gain = ratio * (gain.min - gain.max) + gain.max;
                    host.set(controls.gain, drag.gain);
                    if let Some((slider, knob)) = popup {
                        overlay.value_popup = Some((
                            slider.name,
                            ValuePopup {
                                text: slider.popup_text(drag.cutoff),
                                source: knob,
                                placement: slider.popup_placement,
                            },
                        ));
                    }
                }
                state.drag = Some(drag);
            } else {
                // mouseUp.
                if drag.moved {
                    host.end(controls.cutoff);
                    host.end(controls.gain);
                }
                state.drag = None;
            }
        }
        if state.drag.is_none_or(|drag| !drag.moved) {
            // The cutoff's popup shows only while a handle is dragged.
            if let Some((slider, _)) = popup {
                if overlay
                    .value_popup
                    .as_ref()
                    .is_some_and(|(name, _)| *name == slider.name)
                {
                    overlay.value_popup = None;
                }
            }
        }
        if let Some(band) = *selected {
            state.drawn = Some((band, positions(state.drag)));
        }
        ui.data_mut(|data| data.insert_temp(id, state));

        let (band, drawn, dragging) = match state.drawn {
            Some((band, drawn)) => (band, drawn, state.drag.is_some()),
            None => (0, positions(None), false),
        };
        self.paint_background(pixels, scope, bounds);
        under(pixels);
        self.paint(live, pixels, scope, bounds, band, &drawn, dragging);
        chosen
    }

    /// `paintBackground`: the frequency lines.
    fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
        const LINE_SPACING: i32 = 10;
        let width = bounds.width as f32;
        if self.draw_frequency_lines {
            // Lines at 1, 2, … 10, 20, … 100 Hz and so on.
            let cutoff = Host::info(self.low.cutoff);
            let min_frequency = utils::midi_note_to_frequency(cutoff.min);
            let max_frequency = utils::midi_note_to_frequency(cutoff.max);
            let max_octave = (max_frequency / min_frequency).log2();
            let colour = scope
                .color(ColorId::LightenScreen)
                .with_multiplied_alpha(0.5);
            // Graphics clips to the component: lines below the lowest
            // frequency fall to its left and don't show.
            let line_at = |x: i32| {
                if (0..bounds.width).contains(&x) {
                    pixels.fill_rect(Rect::new(bounds.x + x, bounds.y, 1, bounds.height), colour);
                }
            };
            let mut frequency = 0.0f32;
            let mut increment = 1.0f32;
            let mut x = 0;
            while frequency < max_frequency {
                for _ in 0..LINE_SPACING {
                    frequency += increment;
                    let t = (frequency / min_frequency).log2() / max_octave;
                    x = (t * width).round() as i32;
                    line_at(x);
                }
                line_at(x);
                increment *= LINE_SPACING as f32;
            }
        }
    }

    /// `render`: the response for each lane, then the handles.
    #[allow(clippy::too_many_arguments)]
    fn paint(
        &self,
        live: &Live,
        pixels: &PixelPainter,
        scope: &Scope,
        bounds: Rect,
        selected: usize,
        positions: &Handles,
        dragging: bool,
    ) {
        let (width, height) = (bounds.width as f32, bounds.height as f32);
        let uniforms = self.uniforms(live);
        let (min_db, max_db) = self.db_range();
        let mut line = LineRenderer::new(RESOLUTION, false);
        line.fill = true;
        line.line_width = scope.value(ValueId::WidgetLineWidth);
        line.fill_center = 1.0 - 2.0 * max_db / (max_db - min_db);
        let fill_fade = scope.value(ValueId::WidgetFillFade);
        let mut passes = Vec::with_capacity(2);
        if self.active {
            passes.push((1, ColorId::WidgetPrimary2, ColorId::WidgetSecondary2));
            passes.push((0, ColorId::WidgetPrimary1, ColorId::WidgetSecondary1));
        } else {
            passes.push((
                0,
                ColorId::WidgetPrimaryDisabled,
                ColorId::WidgetSecondaryDisabled,
            ));
        }
        let y_mult = height / (max_db - min_db);
        for (lane, colour, fill) in passes {
            line.color = scope.color(colour);
            let fill_to = scope.color(fill);
            line.set_fill_colors(fill_to.with_multiplied_alpha(1.0 - fill_fade), fill_to);
            for i in 0..RESOLUTION {
                let t = i as f32 / (RESOLUTION as f32 - 1.0);
                let db = filter_response::eq(&uniforms[lane], 2.0 * t - 1.0);
                line.set_x_at(i, i as f32 * width / (RESOLUTION as f32 - 1.0));
                line.set_y_at(i, (max_db - db) * y_mult);
            }
            line.paint(pixels, RectF::from(bounds), true);
        }

        // drawControlPoints: the selected band's handle filled, the others
        // as rings, and a large halo while dragging.
        let handle_diameter = 2.0 * 0.06 * height;
        let dragging_diameter = 2.0 * 0.18 * height;
        let circle = |(x, y): (f32, f32), diameter: f32| {
            RectF::new(
                bounds.x as f32 + x - diameter / 2.0,
                bounds.y as f32 + y - diameter / 2.0,
                diameter,
                diameter,
            )
        };
        let primary = scope.color(ColorId::WidgetPrimary1);
        let mut batch = QuadBatch::new();
        if let Some(position) = positions[selected] {
            if dragging {
                batch.push(Quad::new(
                    QuadKind::Circle,
                    circle(position, dragging_diameter),
                    scope.color(ColorId::LightenScreen),
                ));
            }
            batch.push(Quad::new(
                QuadKind::Circle,
                circle(position, handle_diameter),
                primary,
            ));
        }
        for (band, position) in positions.iter().enumerate() {
            if band == selected {
                continue;
            }
            if let Some(position) = position {
                let mut ring =
                    Quad::new(QuadKind::Ring, circle(*position, handle_diameter), primary);
                ring.thickness = 1.0;
                batch.push(ring);
            }
        }
        batch.paint(pixels, RectF::from(bounds));
    }
}
