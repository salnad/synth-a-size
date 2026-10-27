// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wavetable editor's Wave Source overlay
//! (`vital/src/interface/wavetable/overlays/wave_source_overlay.cpp`), shown
//! while a Wave Source or Shepard Tone source keyframe is selected, with the
//! overlays' shared controls background
//! (`wavetable_component_overlay.cpp`'s `ControlsBackground`).
//!
//! Over the waveform it draws the keyframe's own waveform
//! (`WaveSourceEditor`) with its snap grid; over the amplitude and phase
//! displays, the keyframe's harmonics (`BarEditor`s); and in the edit strip
//! between them, the interpolation selector and the grid sizes. The grid
//! sizes' `IncrementerButtons` paint with JUCE's component painting, which
//! `FullInterface` turns off for its OpenGL context, so upstream never shows
//! them, and the always-on-top grid sliders over them take every press, so
//! they never act either; the port only lays them out.
//!
//! Upstream draws the overlay twice each frame: `SynthSection`'s
//! `renderOpenGlComponents` renders it as a visible sub-section before the
//! editor's own displays, and `WavetableEditSection::renderOpenGlComponents`
//! renders the current overlay again after them. Its translucent parts (the
//! additive bars, the waveform's fill, the lighter strips) show doubled, so
//! the port draws it twice in the same order: [`paint_wave_source_overlay`]
//! before the displays, [`show_wave_source_overlay`] after.

use sas_engine::sas_oscillators::sas_wavetable::wave_frame::{
    WaveFrame, NUM_REAL_COMPLEX, WAVEFORM_SIZE,
};

use super::wavetable_edit::WavetableEditLayout;
use super::Panel;
use crate::colour::Colour;
use crate::controls::bars::BarRenderer;
use crate::controls::slider::{Slider, SliderStyle};
use crate::fonts::Face;
use crate::layout::Rect;
use crate::local::{WaveSourceControls, WAVE_SOURCE_INTERPOLATIONS};
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `WavetableComponentOverlay::kTitleHeightRatio`.
const TITLE_HEIGHT_RATIO: f32 = 0.4;
/// `ControlsBackground::setPositions`'s `kTextHeightRatio`.
const TITLE_TEXT_HEIGHT_RATIO: f32 = 0.6;
/// `setEditBounds`' `kInterpolationWidthHeightRatio` and
/// `kGridWidthHeightRatio`.
const INTERPOLATION_WIDTH_HEIGHT_RATIO: f32 = 8.0;
const GRID_WIDTH_HEIGHT_RATIO: f32 = 2.0;
/// `interpolation_type_->setTextHeightPercentage`.
const INTERPOLATION_TEXT_HEIGHT: f32 = 0.4;
/// `WaveSourceOverlay::kDefaultPhase`: a silent harmonic's phase.
const DEFAULT_PHASE: f32 = -0.5;
/// `WaveSourceEditor::getSnapRadius`'s `kGridProximity`.
const GRID_PROXIMITY: f32 = 0.2;

/// How far the waveform is nudged down so that ties between rows go the way
/// GL's do, in pixels.
const GL_TIE_NUDGE: f32 = 0.01;

/// The controls background's titles: the interpolation selector has none.
const TITLES: [&str; 3] = ["", "GRID X", "GRID Y"];

/// `WavetableEditSection::setOverlayPosition` and
/// `WaveSourceOverlay::setEditBounds`, in interface coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayLayout {
    /// The overlay: the section's width, down to the playhead.
    pub overlay: Rect,
    /// `edit_bounds_`: the strip below the waveform the controls sit in.
    pub edit: Rect,
    /// `controls_background_`, centred in the strip.
    pub controls: Rect,
    /// The background's divider lines, as x in it (`addLine`).
    pub lines: [i32; 2],
    /// The background's title texts.
    pub titles: [Rect; 3],
    /// `interpolation_type_`, `horizontal_grid_` and `vertical_grid_`.
    pub interpolation: Rect,
    pub grid_x: Rect,
    pub grid_y: Rect,
    /// `horizontal_incrementers_` and `vertical_incrementers_`: each one's
    /// increment (top) and decrement (bottom) halves, laid out but inert.
    pub grid_x_buttons: [Rect; 2],
    pub grid_y_buttons: [Rect; 2],
}

impl OverlayLayout {
    /// The Wave Source overlay in the section laid out as `edit`; `scope`
    /// is the section's.
    pub fn wave_source(scope: &Scope, edit: &WavetableEditLayout) -> OverlayLayout {
        let bounds = edit.bounds;
        // The overlay's padding is an int (`setPadding(getPadding())`).
        let padding = scope.padding() as i32;
        let edit_height = (edit.amplitudes.height as f32 * 0.33) as i32;
        let edit_y = ((edit.waveform.bottom() - bounds.y) as f32 + scope.padding()) as i32;
        let strip = Rect::new(bounds.x, bounds.y + edit_y, bounds.width, edit_height);

        let height = strip.height;
        let interpolation_width = (height as f32 * INTERPOLATION_WIDTH_HEIGHT_RATIO) as i32;
        let grid_width = (height as f32 * GRID_WIDTH_HEIGHT_RATIO) as i32;
        let total_width = interpolation_width + 2 * grid_width + 2 * padding;
        let x = strip.x + (strip.width - total_width) / 2;
        let controls = Rect::new(x, strip.y, total_width, height);

        let title_height = (TITLE_HEIGHT_RATIO * height as f32) as i32;
        let y_title = strip.y + title_height;
        let height_title = height - title_height;
        let interpolation = Rect::new(x, strip.y, interpolation_width, height);
        let grid_x = Rect::new(
            interpolation.right() + padding,
            y_title,
            grid_width,
            height_title,
        );
        let grid_y = Rect::new(grid_x.right() + padding, y_title, grid_width, height_title);
        let buttons = |grid: Rect| {
            let square = Rect::new(
                grid.right() - height_title,
                y_title,
                height_title,
                height_title,
            );
            // removeFromBottom(getHeight() / 2).
            let bottom = square.height / 2;
            [
                Rect::new(square.x, square.y, square.width, square.height - bottom),
                Rect::new(square.x, square.bottom() - bottom, square.width, bottom),
            ]
        };

        let lines = [
            interpolation_width,
            interpolation_width + grid_width + padding,
        ];
        let mut titles = [Rect::new(0, 0, 0, 0); 3];
        for (i, title) in titles.iter_mut().enumerate() {
            let start = if i > 0 { lines[i - 1] } else { 0 };
            let end = if i < lines.len() {
                lines[i]
            } else {
                total_width
            };
            *title = Rect::new(controls.x + start, controls.y, end - start, title_height);
        }

        OverlayLayout {
            overlay: Rect::new(bounds.x, bounds.y, bounds.width, edit.playhead.y - bounds.y),
            edit: strip,
            controls,
            lines,
            titles,
            interpolation,
            grid_x,
            grid_y,
            grid_x_buttons: buttons(grid_x),
            grid_y_buttons: buttons(grid_y),
        }
    }
}

/// `ControlsBackground::setPositions` and its quads: the body in a rounded
/// rectangle with a border, a line between each control, and a lighter
/// strip behind each title.
pub fn paint_controls_background(
    panel: &mut Panel,
    controls: Rect,
    lines: &[i32],
    titles: &[(&str, Rect)],
) {
    let scope = &panel.scope;
    let lighten = scope.color(ColorId::LightenScreen);
    let text = scope.color(ColorId::BodyText);
    let background = Quad::new(
        QuadKind::RoundedRectangle,
        controls.into(),
        scope.color(ColorId::Body),
    );
    let border = Quad::new(
        QuadKind::RoundedRectangleBorder,
        controls.into(),
        scope.color(ColorId::WidgetPrimary1),
    );
    let mut quads = vec![background, border];
    for &line in lines {
        quads.push(Quad::new(
            QuadKind::Color,
            Rect::new(controls.x + line, controls.y, 1, controls.height).into(),
            lighten,
        ));
    }
    for &(title, bounds) in titles {
        if !title.is_empty() {
            quads.push(Quad::new(QuadKind::Color, bounds.into(), lighten));
        }
    }
    for quad in quads {
        panel.quad(quad);
    }
    for &(title, bounds) in titles {
        let size = bounds.height as f32 * TITLE_TEXT_HEIGHT_RATIO;
        panel.text_centred(title, bounds, Face::Light, size, text);
    }
}

/// `WaveSourceEditor::getSnapRadius`: how close to a grid point a press
/// snaps to it, in pixels.
pub fn snap_radius(width: f32, height: f32, grid_x: i32, grid_y: i32) -> f32 {
    if grid_x == 0 || grid_y == 0 {
        return 0.0;
    }
    let cell_width = width / grid_x as f32;
    let cell_height = height / grid_y as f32;
    GRID_PROXIMITY * cell_height.min(cell_width)
}

/// `WaveSourceOverlay::updateFrequencyDomain`: each harmonic's amplitude
/// (before the bars' scaling) and phase as a fraction of pi, the phase of a
/// silent one at [`DEFAULT_PHASE`].
pub fn harmonics(frame: &WaveFrame) -> Vec<(f32, f32)> {
    frame.frequency_domain[..NUM_REAL_COMPLEX]
        .iter()
        .map(|&frequency| {
            let amplitude = frequency.abs();
            let phase = if amplitude != 0.0 {
                frequency.arg() / std::f32::consts::PI
            } else {
                DEFAULT_PHASE
            };
            (amplitude / WAVEFORM_SIZE as f32, phase)
        })
        .collect()
}

/// One Wave Source overlay's sliders.
pub struct WaveSourceSliders {
    pub interpolation: Slider,
    pub grid_x: Slider,
    pub grid_y: Slider,
}

impl WaveSourceSliders {
    pub fn new(controls: WaveSourceControls) -> WaveSourceSliders {
        let mut interpolation = Slider::selector(controls.interpolation, None);
        interpolation.format.string_lookup = Some(&WAVE_SOURCE_INTERPOLATIONS);
        interpolation.text_height_percentage = INTERPOLATION_TEXT_HEIGHT;
        WaveSourceSliders {
            interpolation,
            grid_x: Slider::new(controls.grid_x, SliderStyle::Text),
            grid_y: Slider::new(controls.grid_y, SliderStyle::Text),
        }
    }
}

/// The overlay's look for one frame: its colours and the bars' scaling
/// (`setPowerScale`, `setFrequencyZoom`).
pub struct WaveSourceView<'a> {
    pub frame: &'a WaveFrame,
    pub power_scale: bool,
    pub zoom: f32,
}

/// The overlay's first drawing, before the editor's displays: its editors
/// paint at once, and its controls go in the section's layer.
pub fn paint_wave_source_overlay(
    panel: &mut Panel,
    edit: &WavetableEditLayout,
    layout: &OverlayLayout,
    sliders: &WaveSourceSliders,
    view: Option<&WaveSourceView>,
) {
    paint_editors(panel, edit, sliders, view);
    paint_controls(panel, layout);
    for (slider, bounds) in [
        (&sliders.interpolation, layout.interpolation),
        (&sliders.grid_x, layout.grid_x),
        (&sliders.grid_y, layout.grid_y),
    ] {
        let value = slider.host_value(panel.host);
        slider.paint(
            panel.pixels,
            &mut panel.layer,
            &panel.scope,
            bounds,
            value,
            false,
        );
    }
}

/// The overlay's second drawing, after the editor's displays, which also
/// handles its controls.
pub fn show_wave_source_overlay(
    panel: &mut Panel,
    edit: &WavetableEditLayout,
    layout: &OverlayLayout,
    sliders: &WaveSourceSliders,
    view: Option<&WaveSourceView>,
) {
    paint_editors(panel, edit, sliders, view);
    paint_controls(panel, layout);
    panel.slider(&sliders.interpolation, layout.interpolation);
    panel.slider(&sliders.grid_x, layout.grid_x);
    panel.slider(&sliders.grid_y, layout.grid_y);
}

/// The overlay's editors over the displays.
fn paint_editors(
    panel: &mut Panel,
    edit: &WavetableEditLayout,
    sliders: &WaveSourceSliders,
    view: Option<&WaveSourceView>,
) {
    let Some(view) = view else {
        return;
    };
    let grid_x = panel.host.value(sliders.grid_x.name) as i32;
    let grid_y = panel.host.value(sliders.grid_y.name) as i32;
    paint_wave_source_editor(panel, edit.waveform, view.frame, grid_x, grid_y);
    paint_bar_editors(panel, edit, view);
}

/// The controls background with its titles.
fn paint_controls(panel: &mut Panel, layout: &OverlayLayout) {
    let titles: Vec<(&str, Rect)> = TITLES.iter().copied().zip(layout.titles).collect();
    paint_controls_background(panel, layout.controls, &layout.lines, &titles);
}

/// `WaveSourceEditor::render`: the grid lines and snap circles, then the
/// keyframe's waveform with its fill (`setLineValues`).
fn paint_wave_source_editor(
    panel: &mut Panel,
    bounds: Rect,
    frame: &WaveFrame,
    grid_x: i32,
    grid_y: i32,
) {
    let scope = &panel.scope;
    let lighten = scope.color(ColorId::LightenScreen);
    let (width, height) = (bounds.width as f32, bounds.height as f32);
    let (left, top) = (bounds.x as f32, bounds.y as f32);

    // setGridPositions: one-pixel lines inside the grid's edges.
    let mut quads = Vec::new();
    for i in 1..grid_x {
        let x = left + i as f32 * width / grid_x as f32;
        quads.push(Quad::new(
            QuadKind::Color,
            crate::paint::RectF::new(x, top, 1.0, height),
            lighten,
        ));
    }
    for i in 1..grid_y {
        // The line's quad runs a pixel up from here. GL fills the row whose
        // centre is in that span, taking the lower one on a tie, where the
        // port's rasterizer would take the upper.
        let bottom = height - i as f32 * height / grid_y as f32;
        let row = (bottom - 0.5).floor();
        quads.push(Quad::new(
            QuadKind::Color,
            crate::paint::RectF::new(left, top + row, width, 1.0),
            lighten,
        ));
    }
    let radius = snap_radius(width, height, grid_x, grid_y);
    let circle = lighten.with_multiplied_alpha(0.5);
    for h in 0..=grid_x {
        for v in 0..=grid_y {
            let x = left + h as f32 * width / grid_x as f32;
            let y = top + height - v as f32 * height / grid_y as f32;
            quads.push(Quad::new(
                QuadKind::Circle,
                crate::paint::RectF::new(x - radius, y - radius, 2.0 * radius, 2.0 * radius),
                circle,
            ));
        }
    }
    let mut batch = QuadBatch::new();
    for quad in quads {
        batch.push(quad);
    }
    batch.paint(panel.pixels, bounds.into());

    let line_color = scope.color(ColorId::WidgetPrimary1);
    let fill_color = scope.color(ColorId::WidgetSecondary1);
    let fade = fill_color.with_multiplied_alpha(1.0 - scope.value(ValueId::WidgetFillFade));
    let mut line = LineRenderer::new(WAVEFORM_SIZE, true);
    line.fill = true;
    line.fit = true;
    line.line_width = scope.value(ValueId::WidgetLineWidth);
    line.color = line_color;
    line.set_fill_colors(fade, fill_color);
    let adjust = height / 2.0;
    for (i, &value) in frame.time_domain[..WAVEFORM_SIZE].iter().enumerate() {
        line.set_x_at(i, i as f32 * width / (WAVEFORM_SIZE as f32 - 1.0));
        line.set_y_at(i, adjust * (1.0 - value));
    }
    // Nudged down so an edge exactly between two rows fills the lower one,
    // as GL's rasterizer does (the fill meets its centre on a row boundary
    // at odd display heights).
    let mut nudged: crate::paint::RectF = bounds.into();
    nudged.y += GL_TIE_NUDGE;
    line.paint(panel.pixels, nudged, true);
}

/// The amplitude and phase `BarEditor`s over the displays, in the overlay's
/// bar colour.
fn paint_bar_editors(panel: &mut Panel, edit: &WavetableEditLayout, view: &WaveSourceView) {
    let color: Colour = panel.scope.color(ColorId::WidgetSecondary2);
    let mut amplitudes = BarRenderer::new(NUM_REAL_COMPLEX, true);
    amplitudes.square_scale = true;
    amplitudes.power_scale = view.power_scale;
    amplitudes.scale = view.zoom;
    amplitudes.color = color;
    let mut phases = BarRenderer::new(NUM_REAL_COMPLEX, true);
    phases.scale = view.zoom;
    phases.color = color;
    for (i, (amplitude, phase)) in harmonics(view.frame).into_iter().enumerate() {
        amplitudes.set_scaled_y(i, amplitude);
        phases.set_y(i, phase);
    }
    amplitudes.paint(panel.pixels, edit.amplitudes.into());
    phases.paint(panel.pixels, edit.phases.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine's one harmonic at half amplitude and a quarter turn behind;
    /// silent bins at the default phase.
    #[test]
    fn a_sines_harmonics() {
        let mut frame = WaveFrame::new();
        let sine: Vec<f32> = (0..WAVEFORM_SIZE)
            .map(|i| (2.0 * std::f32::consts::PI * i as f32 / WAVEFORM_SIZE as f32).sin())
            .collect();
        frame.load_time_domain(&sine);
        frame.to_frequency_domain();
        let harmonics = harmonics(&frame);
        assert_eq!(harmonics.len(), NUM_REAL_COMPLEX);
        let (amplitude, phase) = harmonics[1];
        assert!((amplitude - 0.5).abs() < 1e-4, "{amplitude}");
        assert!((phase + 0.5).abs() < 1e-4, "{phase}");
        for &(amplitude, phase) in &harmonics[3..20] {
            if amplitude == 0.0 {
                assert_eq!(phase, DEFAULT_PHASE);
            }
        }
    }

    /// `getSnapRadius`: a fifth of the smaller grid cell, none without a
    /// grid.
    #[test]
    fn the_snap_radius_is_a_fifth_of_the_smaller_cell() {
        assert_eq!(snap_radius(600.0, 300.0, 6, 4), 0.2 * 75.0);
        assert_eq!(snap_radius(600.0, 300.0, 12, 1), 0.2 * 50.0);
        assert_eq!(snap_radius(600.0, 300.0, 0, 4), 0.0);
        assert_eq!(snap_radius(600.0, 300.0, 6, 0), 0.0);
    }
}
