// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Compressor section and its editor. Ports `CompressorSection`
//! (`vital/src/interface/editor_sections/compressor_section.cpp`) and
//! `CompressorEditor` (`editor_components/compressor_editor.cpp`).

use super::{divided_area_buffered, divided_area_unbuffered, with_left};
use crate::layout::Rect;
use crate::scope::Scope;
use crate::section::place_knobs_in_area;

/// `CompressorSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompressorLayout {
    pub bounds: Rect,
    pub on: Rect,
    pub enabled_bands: Rect,
    pub mix: Rect,
    pub low_gain: Rect,
    pub band_gain: Rect,
    pub high_gain: Rect,
    pub attack: Rect,
    pub release: Rect,
    pub editor: Rect,
}

impl CompressorLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> CompressorLayout {
        let widget_margin = scope.widget_margin() as i32;
        let title = crate::section::title_width(scope) as i32;
        let section_height = scope.knob_section_height() as i32;
        let (width, height) = (bounds.width, bounds.height);

        let area = with_left(Rect::new(0, 0, width, height), title);
        let knobs_area = divided_area_buffered(area, 3, 0, widget_margin);
        let time_area = divided_area_unbuffered(area, 6, 5, widget_margin);
        let settings_area = divided_area_unbuffered(area, 3, 0, widget_margin);

        let editor_x = knobs_area.right();
        let editor_width = time_area.x - editor_x - widget_margin;
        let knob_y2 = section_height - widget_margin;
        let gains = place_knobs_in_area(
            scope,
            Rect::new(knobs_area.x, knob_y2, knobs_area.width, section_height),
            3,
        );
        let bands_width = gains[1].right() - gains[0].x;
        let enabled_bands = Rect::new(
            settings_area.x,
            widget_margin,
            bands_width,
            section_height - 2 * widget_margin,
        );
        let mix_x = enabled_bands.right();
        let mix = place_knobs_in_area(
            scope,
            Rect::new(mix_x, 0, knobs_area.right() - mix_x, section_height),
            1,
        )[0];
        let attack = Rect::new(
            time_area.x,
            0,
            time_area.width,
            section_height - widget_margin,
        );
        let release = Rect::new(
            time_area.x,
            knob_y2,
            time_area.width,
            section_height - widget_margin,
        );
        let editor = Rect::new(
            editor_x,
            widget_margin,
            editor_width,
            height - 2 * widget_margin,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        CompressorLayout {
            bounds,
            on: Rect::new(dx, dy, title, title),
            enabled_bands: enabled_bands.translated(dx, dy),
            mix: mix.translated(dx, dy),
            low_gain: gains[0].translated(dx, dy),
            band_gain: gains[1].translated(dx, dy),
            high_gain: gains[2].translated(dx, dy),
            attack: attack.translated(dx, dy),
            release: release.translated(dx, dy),
            editor: editor.translated(dx, dy),
        }
    }
}

use egui::{CursorIcon, Sense, Ui};
use sas_engine::sas_dsp_core::sas_simd::{poly_utils, PolyFloat};
use sas_preset::strings::StringTable;

use super::viewers::Live;
use super::{paint_section_background, viewer_corners, Effect};
use crate::colour::Colour;
use crate::controls::slider::{Placement, Slider, SliderStyle};
use crate::host::Host;
use crate::paint::{PixelPainter, RectF};
use crate::popup::ValuePopup;
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::section::{draw_label, draw_text_component_background};
use crate::sections::{Overlay, Panel};
use crate::skin::{ColorId, ValueId};

/// The section's controls (`CompressorSection`'s constructor).
pub struct CompressorSection {
    enabled_bands: Slider,
    mix: Slider,
    attack: Slider,
    release: Slider,
    low_gain: Slider,
    band_gain: Slider,
    high_gain: Slider,
}

impl Default for CompressorSection {
    fn default() -> CompressorSection {
        CompressorSection {
            enabled_bands: Slider::selector(
                "compressor_enabled_bands",
                Some(StringTable::CompressorBand.values()),
            ),
            mix: Slider::new("compressor_mix", SliderStyle::Rotary),
            attack: Slider::new("compressor_attack", SliderStyle::Rotary),
            release: Slider::new("compressor_release", SliderStyle::Rotary),
            low_gain: Slider::new("compressor_low_gain", SliderStyle::Rotary).bipolar(),
            band_gain: Slider::new("compressor_band_gain", SliderStyle::Rotary).bipolar(),
            high_gain: Slider::new("compressor_high_gain", SliderStyle::Rotary).bipolar(),
        }
    }
}

/// `MultibandCompressor`'s band modes (`compressor_enabled_bands`).
const MULTIBAND: i32 = 0;
const LOW_BAND: i32 = 1;
const HIGH_BAND: i32 = 2;

impl CompressorSection {
    /// `CompressorSection::paintBackground`, then its controls.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        scope: &Scope,
        overlay: &mut Overlay,
        bounds: Rect,
        clip: Rect,
    ) {
        let l = CompressorLayout::new(scope, bounds);
        // setCompressorActiveBands.
        let enabled_bands = host.value("compressor_enabled_bands") as i32;
        let low_active = enabled_bands == LOW_BAND || enabled_bands == MULTIBAND;
        let high_active = enabled_bands == HIGH_BAND || enabled_bands == MULTIBAND;
        let low_gain = Slider {
            active: low_active,
            ..self.low_gain
        };
        let high_gain = Slider {
            active: high_active,
            ..self.high_gain
        };
        let rotaries = [
            (&self.mix, l.mix),
            (&self.attack, l.attack),
            (&self.release, l.release),
            (&low_gain, l.low_gain),
            (&self.band_gain, l.band_gain),
            (&high_gain, l.high_gain),
        ];
        paint_section_background(
            pixels,
            scope,
            Effect::Compressor,
            bounds,
            &rotaries,
            &[l.editor],
        );
        draw_text_component_background(pixels, scope, l.enabled_bands, true);
        draw_label(pixels, scope, "MODE", l.enabled_bands, true);
        for (text, rect) in [
            ("ATTACK", l.attack),
            ("RELEASE", l.release),
            ("MIX", l.mix),
            ("LOW", l.low_gain),
            ("BAND", l.band_gain),
            ("HIGH", l.high_gain),
        ] {
            draw_label(pixels, scope, text, rect, false);
        }
        let live = Live::new(host);
        CompressorEditor {
            low_active,
            high_active,
        }
        .show(ui, &live, pixels, scope, overlay, l.editor);

        let mut panel = Panel::new(ui, host, pixels, *scope, overlay, clip);
        panel.slider(&self.enabled_bands, l.enabled_bands);
        for (slider, rect) in rotaries {
            panel.slider(slider, rect);
        }
        viewer_corners(&mut panel, l.editor);
        panel.finish();
    }
}

const GRAB_RADIUS: f32 = 8.0;
const MIN_DB: f32 = -80.0;
const MAX_DB: f32 = 0.0;
const DB_EDIT_BUFFER: f32 = 1.0;
const MIN_EDIT_DB: f32 = MIN_DB + DB_EDIT_BUFFER;
const MAX_EDIT_DB: f32 = MAX_DB - DB_EDIT_BUFFER;
const MIN_LOWER_RATIO: f32 = -1.0;
const MAX_LOWER_RATIO: f32 = 1.0;
const MIN_UPPER_RATIO: f32 = 0.0;
const MAX_UPPER_RATIO: f32 = 1.0;
const RATIO_EDIT_MULTIPLIER: f32 = 0.6;
const COMPRESSOR_AREA_BUFFER: f32 = 0.05;
const BAR_WIDTH: f32 = 1.0 / 5.0;
const MOUSE_MULTIPLIER: f32 = 1.0;
const MAX_BANDS: usize = 3;
const DB_LINE_SECTIONS: i32 = 8;
const EXTRA_DB_LINES: i32 = 6;
const RATIO_DB_LINES: i32 = DB_LINE_SECTIONS + EXTRA_DB_LINES;

/// The bands, low to high.
const BANDS: [&str; MAX_BANDS] = ["low", "band", "high"];

/// What a press grabs (`CompressorEditor::DragPoint`): a band's upper or
/// lower threshold, or its upper or lower ratio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragPoint {
    UpperThreshold(usize),
    LowerThreshold(usize),
    UpperRatio(usize),
    LowerRatio(usize),
}

/// The twelve values the editor edits, by band (`low`, `band`, `high`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Values {
    upper_threshold: [f32; MAX_BANDS],
    lower_threshold: [f32; MAX_BANDS],
    upper_ratio: [f32; MAX_BANDS],
    lower_ratio: [f32; MAX_BANDS],
}

fn control(band: usize, what: &str) -> &'static str {
    const NAMES: [[&str; 4]; MAX_BANDS] = [
        [
            "compressor_low_upper_threshold",
            "compressor_low_lower_threshold",
            "compressor_low_upper_ratio",
            "compressor_low_lower_ratio",
        ],
        [
            "compressor_band_upper_threshold",
            "compressor_band_lower_threshold",
            "compressor_band_upper_ratio",
            "compressor_band_lower_ratio",
        ],
        [
            "compressor_high_upper_threshold",
            "compressor_high_lower_threshold",
            "compressor_high_upper_ratio",
            "compressor_high_lower_ratio",
        ],
    ];
    let index = match what {
        "upper_threshold" => 0,
        "lower_threshold" => 1,
        "upper_ratio" => 2,
        _ => 3,
    };
    NAMES[band][index]
}

impl Values {
    /// `CompressorEditor::setAllValues`.
    fn read(host: &Host) -> Values {
        let mut values = Values::default();
        for band in 0..MAX_BANDS {
            values.upper_threshold[band] = host.value(control(band, "upper_threshold"));
            values.lower_threshold[band] = host.value(control(band, "lower_threshold"));
            values.upper_ratio[band] = host.value(control(band, "upper_ratio"));
            values.lower_ratio[band] = host.value(control(band, "lower_ratio"));
        }
        values
    }
}

/// A drag in progress: the grabbed point, the editor's own values (which
/// upstream keeps unclamped during a shift drag), and the controls the drag
/// has changed, each holding a host gesture until the release.
#[derive(Clone, Copy, Debug)]
struct DragState {
    point: Option<DragPoint>,
    values: Values,
    /// A bit per control in [`editor_controls`] order.
    begun: u32,
}

/// Sets control `name` during a drag, beginning its gesture the first time.
fn set_control(host: &Host, begun: &mut u32, name: &'static str, value: f32) {
    let index = editor_controls()
        .iter()
        .position(|control| *control == name)
        .expect("an editor control");
    if *begun & (1 << index) == 0 {
        host.begin(name);
        *begun |= 1 << index;
    }
    host.set(name, value);
}

/// `getOpenGlYForDb(float)`.
fn gl_y_for_db(db: f32) -> f32 {
    let t = (db - MIN_DB) / (MAX_DB - MIN_DB);
    2.0 * t - 1.0
}

/// `getOpenGlYForMagnitude`, through the clamping `getOpenGlYForDb(poly)`.
fn gl_y_for_magnitude(magnitude: PolyFloat) -> PolyFloat {
    let db = poly_utils::magnitude_to_db(poly_utils::max(PolyFloat::splat(0.0001), magnitude));
    let t = (db - MIN_DB) / (MAX_DB - MIN_DB);
    poly_utils::clamp(t * 2.0 - 1.0, -1.0, 1.0)
}

/// `getCompressedDb`: levels past a threshold move towards it by its
/// ratio (`utils::interpolate`).
fn compressed_db(
    input_db: f32,
    upper_threshold: f32,
    upper_ratio: f32,
    lower_threshold: f32,
    lower_ratio: f32,
) -> f32 {
    if input_db < lower_threshold {
        return input_db + (lower_threshold - input_db) * lower_ratio;
    }
    if input_db > upper_threshold {
        return input_db + (upper_threshold - input_db) * upper_ratio;
    }
    input_db
}

/// The anonymous `formatString(value, suffix)`: four characters of the
/// value with four decimals (five if negative), without a trailing point.
fn format_string(value: f32, suffix: &str) -> String {
    const MAX_DECIMAL_PLACES: usize = 4;
    let format = format!("{value:.MAX_DECIMAL_PLACES$}");
    let mut display_characters = MAX_DECIMAL_PLACES;
    if format.starts_with('-') {
        display_characters += 1;
    }
    let mut format: String = format.chars().take(display_characters).collect();
    if format.ends_with('.') {
        format = format.replace('.', "");
    }
    format + suffix
}

/// One `CompressorEditor`, for this frame's band activity.
struct CompressorEditor {
    low_active: bool,
    high_active: bool,
}

/// A quad in a component's -1 to 1 coordinates (`OpenGlMultiQuad::setQuad`),
/// in pixels; a negative width or height spans the other way.
fn device_rect(bounds: Rect, x: f32, y: f32, w: f32, h: f32) -> RectF {
    let (bw, bh) = (bounds.width as f32, bounds.height as f32);
    let (left, right) = (x.min(x + w), x.max(x + w));
    let (bottom, top) = (y.min(y + h), y.max(y + h));
    RectF::new(
        bounds.x as f32 + (left + 1.0) * 0.5 * bw,
        bounds.y as f32 + (1.0 - top) * 0.5 * bh,
        (right - left) * 0.5 * bw,
        (top - bottom) * 0.5 * bh,
    )
}

impl CompressorEditor {
    /// The bands' pixel spans across the editor (`renderCompressor`), with
    /// an inactive band's moved off the component.
    fn spans(&self, width: i32) -> [(i32, i32); MAX_BANDS] {
        let active_area = 1.0 - 4.0 * COMPRESSOR_AREA_BUFFER;
        let active_section_width = active_area / MAX_BANDS as f32;
        let w = width as f32;
        let mut low_start = (COMPRESSOR_AREA_BUFFER * w).round() as i32;
        let mut low_end = ((COMPRESSOR_AREA_BUFFER + active_section_width) * w).round() as i32;
        let mut band_start =
            ((2.0 * COMPRESSOR_AREA_BUFFER + active_section_width) * w).round() as i32;
        let mut band_end = width - band_start;
        let mut high_start = width - low_end;
        let mut high_end = width - low_start;
        if !self.low_active {
            band_start = low_start;
            low_start = -width;
            low_end = -width;
        }
        if !self.high_active {
            band_end = high_end;
            high_start = -width;
            high_end = -width;
        }
        [
            (low_start, low_end),
            (band_start, band_end),
            (high_start, high_end),
        ]
    }

    /// `getHoverPoint`, for a pointer `(x, y)` pixels from the editor's top
    /// left.
    fn hover_point(
        &self,
        values: &Values,
        bounds: Rect,
        size_ratio: f32,
        (x, y): (f32, f32),
    ) -> Option<DragPoint> {
        let (width, height) = (bounds.width as f32, bounds.height as f32);
        let low_band_high_position = 3.0 * x / ((1.0 - COMPRESSOR_AREA_BUFFER) * width);
        let mut index = low_band_high_position as i32;
        let local_position = low_band_high_position - index as f32;
        if !(0..3).contains(&index) || local_position < 3.0 * COMPRESSOR_AREA_BUFFER {
            return None;
        }
        if index == 0 && !self.low_active {
            index = 1;
        }
        if index == 2 && !self.high_active {
            index = 1;
        }
        let band = index as usize;
        let y_for_db = |db: f32| height * (1.0 - gl_y_for_db(db.clamp(MIN_DB, MAX_DB))) * 0.5;
        let grab_radius = GRAB_RADIUS * size_ratio;
        let upper_handle_y = grab_radius.max(y_for_db(values.upper_threshold[band])) as i32;
        let lower_handle_y =
            (height - grab_radius).min(y_for_db(values.lower_threshold[band])) as i32;
        let delta_upper = y - upper_handle_y as f32;
        let delta_lower = y - lower_handle_y as f32;
        if delta_upper.abs() <= grab_radius && delta_upper.abs() < delta_lower.abs() {
            return Some(DragPoint::UpperThreshold(band));
        }
        if delta_lower.abs() <= grab_radius {
            return Some(DragPoint::LowerThreshold(band));
        }
        if delta_upper < 0.0 {
            return Some(DragPoint::UpperRatio(band));
        }
        if delta_lower > 0.0 {
            return Some(DragPoint::LowerRatio(band));
        }
        None
    }

    /// Handles the mouse and draws the editor.
    fn show(
        &self,
        ui: &mut Ui,
        live: &Live,
        pixels: &PixelPainter,
        scope: &Scope,
        overlay: &mut Overlay,
        bounds: Rect,
    ) {
        let host = live.host();
        let id = ui.scope_id().with("compressor editor");
        let rect = pixels.rect(bounds.into());
        let response = ui.interact(rect, id, Sense::click_and_drag());
        let mut state: DragState = ui.data(|data| data.get_temp(id)).unwrap_or(DragState {
            point: None,
            values: Values::read(host),
            begun: 0,
        });
        if state.point.is_none() || !response.is_pointer_button_down_on() {
            state.values = Values::read(host);
        }

        let ppp = pixels.pixels_per_point();
        let local = |pos: egui::Pos2| ((pos.x - rect.min.x) * ppp, (pos.y - rect.min.y) * ppp);
        let held = response.is_pointer_button_down_on();
        if !held {
            // mouseMove / mouseExit.
            state.point = response.hover_pos().and_then(|pos| {
                self.hover_point(&state.values, bounds, scope.size_ratio(), local(pos))
            });
        }
        if state.point.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
        }
        if response.double_clicked() {
            // mouseDoubleClick: a ratio back to 0, in its own gesture.
            let mut begun = 0;
            match state.point {
                Some(DragPoint::UpperRatio(band)) => {
                    set_upper_ratio(host, &mut begun, &mut state.values, band, 0.0)
                }
                Some(DragPoint::LowerRatio(band)) => {
                    set_lower_ratio(host, &mut begun, &mut state.values, band, 0.0)
                }
                _ => {}
            }
            end_gestures(host, &mut begun);
        }
        if !held && state.begun != 0 {
            end_gestures(host, &mut state.begun);
        }
        if response.drag_stopped() {
            // mouseUp: hidePopupDisplay.
            if overlay
                .value_popup
                .as_ref()
                .is_some_and(|(name, _)| *name == "compressor editor")
            {
                overlay.value_popup = None;
            }
        }
        if response.dragged() {
            let delta = response.drag_delta().y * ppp * MOUSE_MULTIPLIER / bounds.height as f32;
            if delta != 0.0 {
                let shift = ui.input(|input| input.modifiers.shift);
                if let Some(text) = drag(host, &mut state, delta, shift) {
                    overlay.value_popup = Some((
                        "compressor editor",
                        ValuePopup {
                            text,
                            source: bounds,
                            placement: Placement::Below,
                        },
                    ));
                }
            }
        }
        ui.data_mut(|data| data.insert_temp(id, state));

        self.paint(live, pixels, scope, bounds, &state);
    }

    /// `paintBackground` and `renderCompressor`.
    fn paint(
        &self,
        live: &Live,
        pixels: &PixelPainter,
        scope: &Scope,
        bounds: Rect,
        state: &DragState,
    ) {
        const OUTPUT_BAR_HEIGHT: f32 = 2.2;
        let values = &state.values;
        let lighten = scope.color(ColorId::LightenScreen);
        for i in 1..DB_LINE_SECTIONS {
            let t = i as f32 / DB_LINE_SECTIONS as f32;
            let y = (bounds.height as f32 * t) as i32;
            pixels.fill_rect(Rect::new(bounds.x, bounds.y + y, bounds.width, 1), lighten);
        }

        let width = bounds.width;
        let w = width as f32;
        let h = bounds.height as f32;
        let spans = self.spans(width);
        let span_x = |(start, end): (i32, i32)| {
            (start as f32 * 2.0 / w - 1.0, (end - start) as f32 * 2.0 / w)
        };
        let mut batch = QuadBatch::new();
        let mut push = |kind: QuadKind, rect: RectF, colour: Colour, shader_value: f32| {
            let mut quad = Quad::new(kind, rect, colour);
            quad.shader_values[0] = shader_value;
            batch.push(quad);
        };

        // setThresholdPositions, for each ratio sign.
        for ratio_match in [1.0f32, 0.0, -1.0] {
            let colour = match ratio_match {
                r if r > 0.0 => scope.color(ColorId::WidgetSecondary1),
                r if r < 0.0 => scope.color(ColorId::WidgetSecondary2),
                _ => scope.color(ColorId::WidgetSecondaryDisabled),
            };
            let matches = |ratio: f32| {
                ratio == ratio_match
                    || (ratio > 0.0 && ratio_match > 0.0)
                    || (ratio < 0.0 && ratio_match < 0.0)
            };
            for (band, &span) in spans.iter().enumerate() {
                let (x, span_width) = span_x(span);
                if matches(-values.lower_ratio[band]) {
                    let top = gl_y_for_db(values.lower_threshold[band]) + 1.0;
                    let rect = device_rect(bounds, x, -1.0, span_width, top);
                    push(QuadKind::Color, rect, colour, 1.0);
                }
                if matches(values.upper_ratio[band]) {
                    let height = gl_y_for_db(values.upper_threshold[band]) - 1.0;
                    let rect = device_rect(bounds, x, 1.0, span_width, height);
                    push(QuadKind::Color, rect, colour, 1.0);
                }
            }
        }

        // setRatioLinePositions.
        for (band, &span) in spans.iter().enumerate() {
            let (x, span_width) = span_x(span);
            for (threshold, ratio, upper, point) in [
                (
                    values.upper_threshold[band],
                    values.upper_ratio[band],
                    true,
                    DragPoint::UpperRatio(band),
                ),
                (
                    values.lower_threshold[band],
                    values.lower_ratio[band],
                    false,
                    DragPoint::LowerRatio(band),
                ),
            ] {
                let db_position =
                    DB_LINE_SECTIONS as f32 * (threshold - MIN_DB) / (MAX_DB - MIN_DB);
                let (db_index, db_change) = if upper {
                    (
                        db_position.ceil() as i32,
                        (MAX_DB - MIN_DB) / DB_LINE_SECTIONS as f32,
                    )
                } else {
                    (
                        db_position as i32,
                        -(MAX_DB - MIN_DB) / DB_LINE_SECTIONS as f32,
                    )
                };
                let ratio_height = 4.0 / h;
                let mult = if state.point == Some(point) { 5.0 } else { 2.5 };
                let mut db = db_index as f32 * (MAX_DB - MIN_DB) / DB_LINE_SECTIONS as f32 + MIN_DB;
                for i in 0..RATIO_DB_LINES {
                    let adjusted_db = compressed_db(db, threshold, ratio, threshold, ratio);
                    let y = gl_y_for_db(adjusted_db) - ratio_height * 0.5;
                    let rect = device_rect(bounds, x, y, span_width, ratio_height);
                    let fade = (RATIO_DB_LINES - i) as f32 * mult / RATIO_DB_LINES as f32;
                    push(QuadKind::FadeSquare, rect, lighten, fade);
                    db += db_change;
                }
            }
        }

        // renderHover: a line on a hovered threshold.
        let threshold_height = 2.0 / h;
        let hovered = match state.point {
            Some(DragPoint::UpperThreshold(band)) => Some((band, values.upper_threshold[band])),
            Some(DragPoint::LowerThreshold(band)) => Some((band, values.lower_threshold[band])),
            _ => None,
        };
        if let Some((band, threshold)) = hovered {
            let (x, span_width) = span_x(spans[band]);
            let y = gl_y_for_db(threshold) - 0.5 * threshold_height;
            let rect = device_rect(bounds, x, y, span_width, threshold_height);
            push(
                QuadKind::Color,
                rect,
                scope.color(ColorId::WidgetCenterLine),
                1.0,
            );
        }

        // The meters: each band's output level as a bar per channel, its
        // input level as a line over it.
        let active_section_width = (1.0 - 4.0 * COMPRESSOR_AREA_BUFFER) / MAX_BANDS as f32;
        let bar_width = (BAR_WIDTH * active_section_width * w) as i32;
        let gl_bar_width = bar_width as f32 * 2.0 / w;
        let input_height = 2.0 / h;
        let mut outputs = Vec::new();
        let mut inputs = Vec::new();
        for (band, &(start, end)) in BANDS.iter().zip(&spans) {
            let input_y = gl_y_for_magnitude(poly_utils::sqrt(
                live.status(&format!("compressor_{band}_input")),
            ));
            let output_y = gl_y_for_magnitude(poly_utils::sqrt(
                live.status(&format!("compressor_{band}_output")),
            ));
            let middle = ((start + end) as f32 * 0.5) as i32;
            let left = (middle - bar_width) as f32 * 2.0 / w - 1.0;
            let right = (middle + 1) as f32 * 2.0 / w - 1.0;
            for (channel, x) in [(0, left), (1, right)] {
                outputs.push(device_rect(
                    bounds,
                    x,
                    output_y.get(channel) - OUTPUT_BAR_HEIGHT,
                    gl_bar_width,
                    OUTPUT_BAR_HEIGHT,
                ));
                inputs.push(device_rect(
                    bounds,
                    x,
                    input_y.get(channel) - 0.5 * input_height,
                    gl_bar_width,
                    input_height,
                ));
            }
        }
        let primary1 = scope.color(ColorId::WidgetPrimary1);
        for rect in outputs {
            push(QuadKind::RoundedRectangle, rect, primary1, 1.0);
        }
        let primary2 = scope.color(ColorId::WidgetPrimary2);
        for rect in inputs {
            push(QuadKind::Color, rect, primary2, 1.0);
        }
        batch.paint(pixels, RectF::from(bounds));
        let _ = scope.value(ValueId::WidgetRoundedCorner);
    }
}

/// Every control the editor edits.
fn editor_controls() -> &'static [&'static str] {
    static CONTROLS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    CONTROLS.get_or_init(|| {
        (0..MAX_BANDS)
            .flat_map(|band| {
                [
                    "upper_threshold",
                    "lower_threshold",
                    "upper_ratio",
                    "lower_ratio",
                ]
                .map(|what| control(band, what))
            })
            .collect()
    })
}

/// Ends the gestures of the controls in `begun`.
fn end_gestures(host: &Host, begun: &mut u32) {
    for (index, name) in editor_controls().iter().enumerate() {
        if *begun & (1 << index) != 0 {
            host.end(name);
        }
    }
    *begun = 0;
}

/// `setLowUpperThreshold` and the like: keeps `db` (clamped when `clamp`),
/// sends it clamped to the edit range, and pushes the other threshold of
/// the band along. Returns the popup text.
fn set_threshold(
    host: &Host,
    begun: &mut u32,
    values: &mut Values,
    band: usize,
    upper: bool,
    db: f32,
    clamp: bool,
) -> String {
    let clamped = db.clamp(MIN_EDIT_DB, MAX_EDIT_DB);
    let kept = if clamp { clamped } else { db };
    if upper {
        values.upper_threshold[band] = kept;
        set_control(host, begun, control(band, "upper_threshold"), clamped);
        if values.upper_threshold[band] < values.lower_threshold[band] && clamp {
            set_threshold(host, begun, values, band, false, clamped, clamp);
        }
        format_string(values.upper_threshold[band], " dB")
    } else {
        values.lower_threshold[band] = kept;
        set_control(host, begun, control(band, "lower_threshold"), clamped);
        if values.lower_threshold[band] > values.upper_threshold[band] && clamp {
            set_threshold(host, begun, values, band, true, clamped, clamp);
        }
        format_string(values.lower_threshold[band], " dB")
    }
}

/// `setLowUpperRatio` and the like: the editor keeps the ratio clamped, and
/// sends it as it is (the control clamps it).
fn set_upper_ratio(host: &Host, begun: &mut u32, values: &mut Values, band: usize, ratio: f32) {
    values.upper_ratio[band] = ratio.clamp(MIN_UPPER_RATIO, MAX_UPPER_RATIO);
    let value = clamp_to_control(band, "upper_ratio", ratio);
    set_control(host, begun, control(band, "upper_ratio"), value);
}

fn set_lower_ratio(host: &Host, begun: &mut u32, values: &mut Values, band: usize, ratio: f32) {
    values.lower_ratio[band] = ratio.clamp(MIN_LOWER_RATIO, MAX_LOWER_RATIO);
    let value = clamp_to_control(band, "lower_ratio", ratio);
    set_control(host, begun, control(band, "lower_ratio"), value);
}

/// `valueChangedInternal`'s value as the control holds it: within its range.
fn clamp_to_control(band: usize, what: &str, value: f32) -> f32 {
    let info = Host::info(control(band, what));
    value.clamp(info.min, info.max)
}

/// `mouseDrag` by `delta` (a fraction of the editor's height, down
/// positive), which does nothing unless a point was grabbed. A shift drag
/// moves every threshold, unclamped. Returns the popup text when a
/// threshold moved.
fn drag(host: &Host, state: &mut DragState, delta: f32, shift: bool) -> Option<String> {
    let delta_db = (MIN_DB - MAX_DB) * delta;
    let delta_ratio = delta * RATIO_EDIT_MULTIPLIER;
    let DragState {
        point,
        values,
        begun,
    } = state;
    // mouseDrag returns first when nothing is hovered, shift or not.
    let point = (*point)?;
    if shift {
        let mut text = None;
        for band in 0..MAX_BANDS {
            let upper = values.upper_threshold[band] + delta_db;
            text = Some(set_threshold(host, begun, values, band, true, upper, false));
        }
        for band in 0..MAX_BANDS {
            let lower = values.lower_threshold[band] + delta_db;
            text = Some(set_threshold(
                host, begun, values, band, false, lower, false,
            ));
        }
        return text;
    }
    match point {
        DragPoint::UpperThreshold(band) => {
            let db = values.upper_threshold[band] + delta_db;
            Some(set_threshold(host, begun, values, band, true, db, true))
        }
        DragPoint::LowerThreshold(band) => {
            let db = values.lower_threshold[band] + delta_db;
            Some(set_threshold(host, begun, values, band, false, db, true))
        }
        DragPoint::UpperRatio(band) => {
            let ratio = values.upper_ratio[band] + delta_ratio;
            set_upper_ratio(host, begun, values, band, ratio);
            None
        }
        DragPoint::LowerRatio(band) => {
            let ratio = values.lower_ratio[band] - delta_ratio;
            set_lower_ratio(host, begun, values, band, ratio);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `formatString`: four characters (five with a minus sign), and no
    /// trailing point.
    #[test]
    fn thresholds_format_like_upstream() {
        assert_eq!(format_string(-12.3456, " dB"), "-12.3 dB");
        assert_eq!(format_string(-6.0, " dB"), "-6.00 dB");
        assert_eq!(format_string(0.0, " dB"), "0.00 dB");
        assert_eq!(format_string(-123.5, " dB"), "-123 dB");
    }

    /// `getCompressedDb`: levels past a threshold move towards it by its
    /// ratio.
    #[test]
    fn compression_pulls_levels_towards_the_threshold() {
        assert_eq!(compressed_db(-10.0, -20.0, 0.5, -20.0, 0.5), -15.0);
        assert_eq!(compressed_db(-30.0, -20.0, 0.5, -20.0, 0.5), -25.0);
        assert_eq!(compressed_db(-20.0, -20.0, 0.5, -20.0, 0.5), -20.0);
        assert_eq!(compressed_db(-10.0, -20.0, 0.0, -20.0, 0.0), -10.0);
        // Between the thresholds, levels pass unchanged.
        assert_eq!(compressed_db(-30.0, -10.0, 1.0, -50.0, 1.0), -30.0);
    }

    /// The bands share the editor between them; an inactive band leaves
    /// its span to the middle band.
    #[test]
    fn inactive_bands_give_their_span_to_the_middle() {
        let all = CompressorEditor {
            low_active: true,
            high_active: true,
        };
        // Each band gets a third of the 80% left after four 5% buffers:
        // round(200 * (0.05 + 0.8 / 3)) = 63, round(200 * (0.1 + 0.8 / 3)) = 73.
        assert_eq!(all.spans(200), [(10, 63), (73, 127), (137, 190)]);
        let single = CompressorEditor {
            low_active: false,
            high_active: false,
        };
        assert_eq!(single.spans(200), [(-200, -200), (10, 190), (-200, -200)]);
    }
}
