// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wavetable editor's Line Source overlay
//! (`vital/src/interface/wavetable/overlays/wave_line_source_overlay.cpp`),
//! shown while a Line Source keyframe is selected: the base `LineEditor`
//! over the waveform, editing the keyframe's own shape, and the pull power
//! and grid sizes below it. It draws twice around the editor's displays, as
//! the Wave Source overlay does (see `wave_source_overlay`).
//!
//! A source's keyframes keep the same number of points: a point added to or
//! removed from the selected keyframe is added to (`addMiddlePoint`) or
//! removed from every other one.

use sas_common::line_generator::LineGenerator;
use sas_engine::sas_oscillators::sas_wavetable::creator::component::WaveLineSource;

use super::wave_source_overlay::{edit_strip, paint_controls_background, title_bounds, FrameEdit};
use super::wavetable_edit::{KeyframeRef, WavetableEditLayout};
use super::Panel;
use crate::controls::line_editor::{LineChange, LineEditor, LineSettings, LineTarget, PointEdit};
use crate::controls::slider::{Slider, SliderStyle};
use crate::host::Host;
use crate::layout::Rect;
use crate::local::LineSourceControls;
use crate::scope::Scope;

/// `WavetableComponentOverlay::kTitleHeightRatio`.
const TITLE_HEIGHT_RATIO: f32 = 0.4;
/// `setEditBounds`' `kPullPowerWidthHeightRatio` and
/// `kGridWidthHeightRatio`.
const PULL_POWER_WIDTH_HEIGHT_RATIO: f32 = 2.0;
const GRID_WIDTH_HEIGHT_RATIO: f32 = 2.0;
/// The base `LineEditor`'s `paint_pattern_`, `LfoSection`'s "Down".
const PAINT_PATTERN: usize = 2;
const TITLES: [&str; 3] = ["PULL POWER", "GRID X", "GRID Y"];
/// The pull power has no parameter behind it, and a range without an
/// interval: JUCE prints it with seven decimal places.
const PULL_POWER_DECIMAL_PLACES: usize = 7;

/// `WaveLineSourceOverlay::setEditBounds`, in interface coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct LineSourceLayout {
    pub controls: Rect,
    /// The background's divider lines: after the pull power, after the
    /// horizontal grid, and at its right edge.
    pub lines: [i32; 3],
    pub titles: Vec<Rect>,
    pub pull_power: Rect,
    pub grid_x: Rect,
    pub grid_y: Rect,
    /// Each grid size's `IncrementerButtons` halves (increment, then
    /// decrement), which upstream never paints and its grid slider covers.
    pub grid_x_buttons: [Rect; 2],
    pub grid_y_buttons: [Rect; 2],
}

impl LineSourceLayout {
    pub fn new(scope: &Scope, edit: &WavetableEditLayout) -> LineSourceLayout {
        let (strip, padding) = edit_strip(scope, edit);
        let height = strip.height;
        let pull_power_width = (height as f32 * PULL_POWER_WIDTH_HEIGHT_RATIO) as i32;
        let grid_width = (height as f32 * GRID_WIDTH_HEIGHT_RATIO) as i32;
        let total_width = pull_power_width + 2 * grid_width + 2 * padding;
        let x = strip.x + (strip.width - total_width) / 2;
        let controls = Rect::new(x, strip.y, total_width, height);
        let title_height = (TITLE_HEIGHT_RATIO * height as f32) as i32;
        let y = strip.y + title_height;
        let row = height - title_height;
        let pull_power = Rect::new(x, y, pull_power_width, row);
        let grid_x = Rect::new(pull_power.right() + padding, y, grid_width, row);
        let grid_y = Rect::new(grid_x.right() + padding, y, grid_width, row);
        let buttons = |grid: Rect| {
            let square = Rect::new(grid.right() - row, y, row, row);
            // removeFromBottom(getHeight() / 2).
            let bottom = square.height / 2;
            [
                Rect::new(square.x, square.y, square.width, square.height - bottom),
                Rect::new(square.x, square.bottom() - bottom, square.width, bottom),
            ]
        };
        let lines = [
            pull_power_width,
            pull_power_width + grid_width + padding,
            pull_power_width + 2 * (grid_width + padding),
        ];
        LineSourceLayout {
            controls,
            lines,
            titles: title_bounds(controls, &lines[..2], title_height),
            pull_power,
            grid_x,
            grid_y,
            grid_x_buttons: buttons(grid_x),
            grid_y_buttons: buttons(grid_y),
        }
    }
}

/// `pointAdded`, `pointsAdded`, `pointRemoved` and `pointsRemoved`: the
/// selected keyframe's point changes, made to `other`.
pub fn mirror_points(other: &mut LineGenerator, edits: &[PointEdit]) {
    for edit in edits {
        match *edit {
            PointEdit::Added(index) => {
                let index = index.min(other.num_points() - 1);
                other.add_middle_point(index);
            }
            PointEdit::ManyAdded { index, count } => {
                for p in 0..count {
                    other.add_middle_point(index + p);
                }
            }
            PointEdit::Removed(index) => other.remove_point(index),
            PointEdit::ManyRemoved { index, count } => {
                for _ in 0..count {
                    other.remove_point(index);
                }
            }
            // `fileLoaded` does nothing, and the overlay turns file
            // loading off.
            PointEdit::Loaded => {}
        }
    }
}

/// One oscillator's Line Source overlay.
pub struct LineSourceOverlay {
    pull_power: Slider,
    grid_x: Slider,
    grid_y: Slider,
    editor: LineEditor,
    /// The selection and keyframe the pull power was set from.
    loaded: Option<(u64, KeyframeRef)>,
    pull_power_seen: f32,
    /// The pull power changed since the last commit (`sliderDragEnded` to
    /// come).
    pending: bool,
    /// A paint stroke is under way: its point changes audition, and it
    /// commits when it ends.
    painting: bool,
    /// A press that began on the editor is still down.
    editor_pressed: bool,
}

impl LineSourceOverlay {
    pub fn new(controls: LineSourceControls, oscillator: usize) -> LineSourceOverlay {
        let mut pull_power = Slider::new(controls.pull_power, SliderStyle::Text);
        pull_power.format.juce_decimal_places = Some(PULL_POWER_DECIMAL_PLACES);
        LineSourceOverlay {
            pull_power,
            grid_x: Slider::new(controls.grid_x, SliderStyle::Text),
            grid_y: Slider::new(controls.grid_y, SliderStyle::Text),
            editor: LineEditor {
                target: LineTarget::LineSource(oscillator),
            },
            loaded: None,
            pull_power_seen: 0.0,
            pending: false,
            painting: false,
            editor_pressed: false,
        }
    }

    /// Whether a press on the editor, a paint stroke or a pull power drag
    /// is under way, whose edits to the keyframe aren't committed yet.
    pub fn pressed(&self) -> bool {
        self.editor_pressed || self.painting || self.pending
    }

    /// The editor's settings: the grid sizes, no paint button and the base
    /// editor's paint pattern.
    fn settings(&self, host: &Host) -> LineSettings {
        LineSettings {
            grid_size_x: host.value(self.grid_x.name) as usize,
            grid_size_y: host.value(self.grid_y.name) as usize,
            paint: false,
            paint_pattern: PAINT_PATTERN,
            phase: None,
        }
    }

    /// `frameSelected`: the pull power from keyframe `index` of `source`,
    /// when the selection changed since it last was.
    pub fn sync(
        &mut self,
        host: &Host,
        selection: u64,
        keyframe: KeyframeRef,
        source: &WaveLineSource,
    ) {
        if self.loaded == Some((selection, keyframe)) {
            return;
        }
        self.loaded = Some((selection, keyframe));
        if let Some(data) = source.keyframes.keyframes.get(keyframe.keyframe) {
            host.set_once(self.pull_power.name, data.data.pull_power);
        }
        self.pull_power_seen = host.value(self.pull_power.name);
        self.pending = false;
    }

    /// The overlay's pull power, the wheel over the editor and the editor
    /// itself, editing keyframe `index` of `source`. The editor draws here,
    /// before the displays: the overlay's first drawing.
    pub fn handle(
        &mut self,
        panel: &mut Panel,
        edit: &WavetableEditLayout,
        source: &mut WaveLineSource,
        index: usize,
    ) -> Option<FrameEdit> {
        let host = panel.host;
        let mut result = None;
        let mut line = source
            .keyframes
            .keyframes
            .get(index)?
            .data
            .line_generator
            .clone();
        let pull_power = host.value(self.pull_power.name);
        if pull_power != self.pull_power_seen {
            self.pull_power_seen = pull_power;
            source.keyframes.keyframes[index].data.pull_power = pull_power;
            self.pending = true;
            combine(&mut result, FrameEdit::Changed);
        }

        // lineEditorScrolled: each wheel event over the editor steps the
        // horizontal grid, up when it scrolls up and down otherwise (a
        // sideways scroll too).
        if hovered(panel, edit.waveform) {
            let wheels: Vec<f32> = panel.ui.input(|input| {
                input
                    .raw
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                        _ => None,
                    })
                    .collect()
            });
            for delta_y in wheels {
                let step = if delta_y > 0.0 { 1.0 } else { -1.0 };
                host.set_once(self.grid_x.name, host.value(self.grid_x.name) + step);
            }
        }

        let settings = self.settings(host);
        let event = self
            .editor
            .show(panel, edit.waveform, settings, Some(&mut line));
        source.keyframes.keyframes[index].data.line_generator = line;
        if !event.edits.is_empty() {
            for (i, keyframe) in source.keyframes.keyframes.iter_mut().enumerate() {
                if i != index {
                    mirror_points(&mut keyframe.data.line_generator, &event.edits);
                }
            }
            source.num_points = source.keyframes.keyframes[index]
                .data
                .line_generator
                .num_points() as i32;
        }

        let down = any_button_down(panel);
        let editor = panel.pixels.rect(edit.waveform.into());
        let origin = panel.ui.input(|input| input.pointer.press_origin());
        self.editor_pressed = down && origin.is_some_and(|pos| editor.contains(pos));
        if event.changed && event.painting && down {
            // A paint stroke's points: upstream commits each, the port
            // when the stroke ends.
            self.painting = true;
            combine(&mut result, FrameEdit::Changed);
        }
        for change in &event.changes {
            if change.moving() {
                combine(&mut result, FrameEdit::Changed);
            } else if change.released() || matches!(change, LineChange::Points(_)) && !self.painting
            {
                combine(&mut result, FrameEdit::Done);
            }
        }
        if event.changed && event.changes.is_empty() && !self.painting {
            // A double click's or the menu's power reset, which upstream
            // leaves for the next commit.
            combine(&mut result, FrameEdit::Done);
        }
        if self.painting && !down {
            self.painting = false;
            combine(&mut result, FrameEdit::Done);
        }
        if self.pending && !down {
            // sliderDragEnded.
            self.pending = false;
            combine(&mut result, FrameEdit::Done);
        }
        result
    }

    /// The overlay's first drawing, before the displays: the controls
    /// (its editor drew in [`handle`](Self::handle)).
    pub fn paint(&self, panel: &mut Panel, layout: &LineSourceLayout) {
        self.paint_background(panel, layout);
        for (slider, bounds) in self.sliders(layout) {
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

    /// The overlay's second drawing, after the displays: the editor over
    /// the waveform while a keyframe is selected, and the controls, handled.
    pub fn show(
        &self,
        panel: &mut Panel,
        edit: &WavetableEditLayout,
        layout: &LineSourceLayout,
        line: Option<&LineGenerator>,
    ) {
        if let Some(line) = line {
            let settings = self.settings(panel.host);
            self.editor.paint_only(panel, edit.waveform, settings, line);
        }
        self.paint_background(panel, layout);
        for (slider, bounds) in self.sliders(layout) {
            panel.slider(slider, bounds);
        }
    }

    fn sliders<'a>(&'a self, layout: &LineSourceLayout) -> [(&'a Slider, Rect); 3] {
        [
            (&self.pull_power, layout.pull_power),
            (&self.grid_x, layout.grid_x),
            (&self.grid_y, layout.grid_y),
        ]
    }

    fn paint_background(&self, panel: &mut Panel, layout: &LineSourceLayout) {
        let titles: Vec<(&str, Rect)> = TITLES
            .iter()
            .copied()
            .zip(layout.titles.iter().copied())
            .collect();
        paint_controls_background(panel, layout.controls, &layout.lines, &titles);
    }
}

/// Combines a frame's edits: a commit outweighs an audition.
fn combine(result: &mut Option<FrameEdit>, edit: FrameEdit) {
    *result = Some(match result {
        Some(FrameEdit::Done) => FrameEdit::Done,
        _ => edit,
    });
}

fn any_button_down(panel: &Panel) -> bool {
    panel.ui.input(|input| input.pointer.any_down())
}

/// Whether the pointer is over `bounds`.
fn hovered(panel: &Panel, bounds: Rect) -> bool {
    let rect = panel.pixels.rect(bounds.into());
    panel
        .ui
        .input(|input| input.pointer.hover_pos())
        .is_some_and(|pos| rect.contains(pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> LineGenerator {
        let mut line = LineGenerator::new(LineGenerator::DEFAULT_RESOLUTION);
        line.init_triangle();
        line
    }

    /// `pointAdded` adds a middle point at the same index (or before the
    /// last point), and `pointRemoved` removes the same index.
    #[test]
    fn point_changes_mirror_into_other_keyframes() {
        let mut other = triangle();
        mirror_points(&mut other, &[PointEdit::Added(1)]);
        assert_eq!(other.num_points(), 4);
        assert_eq!(other.point(1), (0.25, 0.5));
        mirror_points(&mut other, &[PointEdit::Added(9)]);
        assert_eq!(other.num_points(), 5);
        assert_eq!(other.point(4), (1.0, 1.0));
        mirror_points(&mut other, &[PointEdit::Removed(1)]);
        assert_eq!(other.num_points(), 4);
        mirror_points(&mut other, &[PointEdit::ManyAdded { index: 1, count: 2 }]);
        assert_eq!(other.num_points(), 6);
        mirror_points(&mut other, &[PointEdit::ManyRemoved { index: 1, count: 3 }]);
        assert_eq!(other.num_points(), 3);
    }
}
