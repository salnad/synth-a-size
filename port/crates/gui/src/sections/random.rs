// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! A random LFO section: sync and stereo switches, the viewer, the style
//! and the rate. Ports `RandomSection` and `RandomViewer`'s background
//! (`vital/src/interface/editor_sections/random_section.cpp`).
//!
//! The viewer scrolls the random value's left and right channels from left
//! to right while the source modulates something (`RandomViewer::render`).
//! The sync selector is the shared `TempoSelector`.

use super::Panel;
use crate::controls::button::SynthButton;
use crate::controls::slider::{Slider, SliderStyle};
use crate::controls::tempo::{self, TempoSelector};
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::section::{
    draw_label, draw_text_component_background, keytrack_divider, paint_widget_background,
    place_tempo_controls,
};
use crate::skin::{ColorId, Section, ValueId};

/// The section's skin override (`setSkinOverride(Skin::kRandomLfo)`).
pub const SKIN: &[Section] = &[Section::RandomLfo];

/// `vital::kNumRandomLfos`.
pub const NUM_RANDOM_LFOS: usize = 4;

/// `strings::kRandomNames`: the style list.
pub const RANDOM_NAMES: [&str; 4] = [
    "Perlin",
    "Sample & Hold",
    "Sine Interpolate",
    "Lorenz Attractor",
];

/// The parameter names of random LFO `index` (0-based).
#[derive(Clone, Copy, Debug)]
pub struct Names {
    pub frequency: &'static str,
    pub tempo: &'static str,
    pub keytrack_transpose: &'static str,
    pub keytrack_tune: &'static str,
    pub sync: &'static str,
    pub style: &'static str,
    pub stereo: &'static str,
    pub sync_type: &'static str,
}

macro_rules! names {
    ($($n:literal),*) => {
        [$(Names {
            frequency: concat!("random_", $n, "_frequency"),
            tempo: concat!("random_", $n, "_tempo"),
            keytrack_transpose: concat!("random_", $n, "_keytrack_transpose"),
            keytrack_tune: concat!("random_", $n, "_keytrack_tune"),
            sync: concat!("random_", $n, "_sync"),
            style: concat!("random_", $n, "_style"),
            stereo: concat!("random_", $n, "_stereo"),
            sync_type: concat!("random_", $n, "_sync_type"),
        }),*]
    };
}

/// Each random LFO's parameter names.
pub const NAMES: [Names; NUM_RANDOM_LFOS] = names!("1", "2", "3", "4");

/// `RandomSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RandomLayout {
    pub bounds: Rect,
    pub sync_type: Rect,
    pub stereo: Rect,
    pub viewer: Rect,
    pub style: Rect,
    pub rate: Rect,
    pub sync: Rect,
    pub keytrack_transpose: Rect,
    pub keytrack_tune: Rect,
    pub divider: Rect,
}

impl RandomLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> RandomLayout {
        let (width, height) = (bounds.width, bounds.height);
        let knob_section_height = scope.knob_section_height() as i32;
        let text_button_height = scope.value(ValueId::TextButtonHeight) as i32;
        let widget_margin = scope.widget_margin() as i32;
        let button_width = (width - 3 * widget_margin) / 2;
        let sync_type = Rect::new(
            widget_margin,
            widget_margin,
            button_width,
            text_button_height,
        );
        let stereo_x = sync_type.right() + widget_margin;
        let stereo = Rect::new(
            stereo_x,
            widget_margin,
            width - stereo_x - widget_margin,
            text_button_height,
        );
        let viewer_y = text_button_height + 2 * widget_margin;
        let viewer = Rect::new(
            widget_margin,
            viewer_y,
            width - 2 * widget_margin,
            height - knob_section_height - viewer_y,
        );
        let component_width = (width - 3 * widget_margin) / 2;
        let control_y = height - knob_section_height + widget_margin;
        let style = Rect::new(
            widget_margin,
            control_y,
            component_width,
            knob_section_height - 2 * widget_margin,
        );
        let frequency_x = style.right() + widget_margin;
        let (rate, sync, modulation_area) = place_tempo_controls(
            scope,
            frequency_x,
            control_y,
            width - widget_margin - frequency_x,
            knob_section_height - 2 * widget_margin,
        );
        let divider = keytrack_divider(rate, modulation_area);
        let keytrack_transpose = Rect::new(rate.x, rate.y, rate.width / 2, rate.height);
        let keytrack_tune = Rect::new(
            keytrack_transpose.right(),
            rate.y,
            rate.right() - keytrack_transpose.right(),
            rate.height,
        );
        let at = |r: Rect| r.translated(bounds.x, bounds.y);
        RandomLayout {
            bounds,
            sync_type: at(sync_type),
            stereo: at(stereo),
            viewer: at(viewer),
            style: at(style),
            rate: at(rate),
            sync: at(sync),
            keytrack_transpose: at(keytrack_transpose),
            keytrack_tune: at(keytrack_tune),
            divider: at(divider),
        }
    }
}

/// One random LFO's controls (`RandomSection`'s constructor).
pub struct RandomSection {
    index: usize,
    /// `RandomViewer`'s two lines, kept from frame to frame.
    viewer: std::cell::RefCell<Viewer>,
    sync: TempoSelector,
    frequency: Slider,
    tempo: Slider,
    keytrack_transpose: Slider,
    keytrack_tune: Slider,
    style: Slider,
    stereo: SynthButton,
    sync_type: SynthButton,
}

impl RandomSection {
    /// Random LFO `index` (0-based).
    pub fn new(index: usize) -> RandomSection {
        /// `RandomSection`'s `kTempoDragSensitivity`.
        const TEMPO_DRAG_SENSITIVITY: f32 = 0.3;
        /// `SynthSection::kTransposeMouseSensitivity`.
        const TRANSPOSE_MOUSE_SENSITIVITY: f32 = 0.2;
        let names = NAMES[index];
        RandomSection {
            index,
            viewer: std::cell::RefCell::new(Viewer::default()),
            sync: TempoSelector::new(names.sync),
            frequency: Slider::new(names.frequency, SliderStyle::Text),
            tempo: Slider::new(names.tempo, SliderStyle::Text).sensitivity(TEMPO_DRAG_SENSITIVITY),
            keytrack_transpose: Slider::new(names.keytrack_transpose, SliderStyle::Text)
                .sensitivity(TRANSPOSE_MOUSE_SENSITIVITY)
                .bipolar(),
            keytrack_tune: Slider::new(names.keytrack_tune, SliderStyle::Text)
                .bipolar()
                .display(3, 0),
            style: Slider::selector(names.style, Some(&RANDOM_NAMES)),
            stereo: SynthButton::text(names.stereo, "STEREO"),
            sync_type: SynthButton::text(names.sync_type, "SYNC"),
        }
    }

    /// `RandomSection::paintBackground`, with the viewer's background.
    pub fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, layout: &RandomLayout) {
        let widget_margin = scope.widget_margin() as i32;
        let frequency_right = layout.bounds.right() - widget_margin;
        let frequency = Rect::new(
            layout.rate.x,
            layout.rate.y,
            frequency_right - layout.rate.x,
            layout.rate.height,
        );
        draw_text_component_background(pixels, scope, layout.style, true);
        draw_text_component_background(pixels, scope, frequency, true);
        tempo::paint_divider(pixels, scope, layout.sync);
        draw_label(pixels, scope, "STYLE", layout.style, true);
        draw_label(pixels, scope, "FREQUENCY", frequency, true);

        // RandomViewer::paintBackground: the backdrop and a centre line.
        let viewer = layout.viewer;
        paint_widget_background(pixels, scope, viewer);
        pixels.fill_rect(
            Rect::new(viewer.x, viewer.y + viewer.height / 2, viewer.width, 1),
            scope.color(ColorId::WidgetPrimaryDisabled),
        );
    }

    pub fn show(&self, panel: &mut Panel, layout: &RandomLayout) {
        let host = panel.host;
        let mode = self.sync.mode(host);
        if mode.shows_free_slider() {
            panel.slider(&self.frequency, layout.rate);
        } else if mode.shows_tempo_slider() {
            panel.slider(&self.tempo, layout.rate);
        } else {
            panel.slider(&self.keytrack_transpose, layout.keytrack_transpose);
            panel.slider(&self.keytrack_tune, layout.keytrack_tune);
            let colour = panel.scope.color(ColorId::LightenScreen);
            panel.quad(Quad::new(QuadKind::Color, layout.divider.into(), colour));
        }
        self.sync.show(panel, layout.sync);
        panel.slider(&self.style, layout.style);
        panel.button(&self.sync_type, layout.sync_type);
        panel.button(&self.stereo, layout.stereo);
        self.show_viewer(panel, layout.viewer);
        panel.widget_corners(layout.viewer);
    }

    /// `RandomViewer::render`: while the source modulates something
    /// (`isModSourceEnabled`), each frame moves both lines one point to the
    /// right and starts them at the value now (`random_n`), hiding the line
    /// where it lies on the bottom edge (a boost of -1); then draws the
    /// right channel's line and the left's, filled. At rest it draws
    /// nothing.
    fn show_viewer(&self, panel: &mut Panel, bounds: Rect) {
        let name = format!("random_{}", self.index + 1);
        let model = panel.host.model();
        let Some(telemetry) = model.telemetry() else {
            return;
        };
        if !model.modulations().iter().any(|route| route.source == name) {
            return;
        }
        let height = bounds.height as f32;
        let frame = panel.ui.ctx().cumulative_frame_nr();
        let mut viewer = self.viewer.borrow_mut();
        if viewer.frame != Some(frame) {
            viewer.frame = Some(frame);
            let value = telemetry
                .read(|snapshot| snapshot.status_by_name(&name))
                .unwrap_or([0.0; 4]);
            viewer.step([value[0], value[1]], height);
        }
        let scope = panel.scope;
        let line_width = scope.value(ValueId::WidgetLineWidth);
        let fill_center = scope.value(ValueId::WidgetFillCenter);
        let fill_fade = scope.value(ValueId::WidgetFillFade);
        let lines = [
            (
                1,
                scope.color(ColorId::WidgetPrimary2),
                scope.color(ColorId::WidgetSecondary2),
            ),
            (
                0,
                scope.color(ColorId::WidgetPrimary1),
                scope.color(ColorId::WidgetSecondary1),
            ),
        ];
        let viewer = viewer.clone();
        let width = bounds.width as f32;
        panel.layer().custom(move |pixels: &PixelPainter| {
            for (channel, colour, fill) in lines {
                let mut line = LineRenderer::new(VIEWER_RESOLUTION, false);
                line.line_width = line_width;
                line.fill_center = fill_center;
                line.fill = true;
                line.boost_amount = VIEWER_BOOST_AMOUNT;
                line.fill_boost_amount = VIEWER_BOOST_AMOUNT;
                line.color = colour;
                line.set_fill_colors(fill.with_multiplied_alpha(1.0 - fill_fade), fill);
                for i in 0..VIEWER_RESOLUTION {
                    let x = match i {
                        0 => -1.0,
                        1 => 0.0,
                        _ => i as f32 * width / (VIEWER_RESOLUTION - 1) as f32,
                    };
                    line.set_x_at(i, x);
                    line.set_y_at(i, viewer.y[channel][i]);
                    line.set_boost_at(true, i, viewer.boost[channel][i]);
                }
                line.paint(pixels, RectF::from(bounds), true);
            }
        });
    }
}

/// `RandomViewer::kResolution` and `kBoostAmount`.
const VIEWER_RESOLUTION: usize = 64;
const VIEWER_BOOST_AMOUNT: f32 = 1.0;

/// `RandomViewer`'s lines: each channel's y and left boost per point, which
/// start off below the viewer (`setYAt(i, 10000)`), and the egui frame they
/// last moved in (upstream moves them once per render).
#[derive(Clone, Debug)]
struct Viewer {
    y: [[f32; VIEWER_RESOLUTION]; 2],
    boost: [[f32; VIEWER_RESOLUTION]; 2],
    frame: Option<u64>,
}

impl Viewer {
    /// One frame: each line moves a point to the right and starts at
    /// `value`'s channel, its newest stretch hidden (a boost of -1) where it
    /// lies on the bottom edge.
    fn step(&mut self, value: [f32; 2], height: f32) {
        for ((y, boost), value) in self.y.iter_mut().zip(&mut self.boost).zip(value) {
            y.copy_within(1..VIEWER_RESOLUTION - 1, 2);
            boost.copy_within(1..VIEWER_RESOLUTION - 1, 2);
            let new_y = (1.0 - value) * height;
            y[0] = new_y;
            y[1] = new_y;
            let hidden = new_y >= height || y[2] >= height || y[3] >= height;
            boost[..3].fill(if hidden { -1.0 } else { 0.0 });
        }
    }
}

impl Default for Viewer {
    fn default() -> Viewer {
        Viewer {
            y: [[10000.0; VIEWER_RESOLUTION]; 2],
            boost: [[0.0; VIEWER_RESOLUTION]; 2],
            frame: None,
        }
    }
}

#[cfg(test)]
mod viewer_tests {
    use super::*;

    /// `RandomViewer::render`'s scroll: the newest value enters at the left
    /// and moves one point right a frame; the stretch on the bottom edge
    /// (value 0, or the offscreen start) is hidden, a lifted one isn't.
    #[test]
    fn the_viewer_scrolls_and_hides_the_bottom_edge() {
        let mut viewer = Viewer::default();
        viewer.step([0.5, 0.25], 100.0);
        assert_eq!((viewer.y[0][0], viewer.y[0][1]), (50.0, 50.0));
        assert_eq!(viewer.y[1][1], 75.0);
        // The start is off the bottom: still hidden.
        assert_eq!(viewer.boost[0][..3], [-1.0; 3]);
        viewer.step([0.75, 0.25], 100.0);
        viewer.step([0.75, 0.25], 100.0);
        assert_eq!(viewer.y[0][..4], [25.0, 25.0, 25.0, 50.0]);
        assert_eq!(viewer.boost[0][..3], [0.0; 3]);
        assert_eq!(viewer.y[0][4], 10000.0);
        viewer.step([0.0, 0.0], 100.0);
        assert_eq!(viewer.boost[1][..3], [-1.0; 3]);
    }
}
