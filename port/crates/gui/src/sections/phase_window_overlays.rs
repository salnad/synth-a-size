// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wavetable editor's Phase Shift and Wave Window overlays
//! (`vital/src/interface/wavetable/overlays/phase_modifier_overlay.cpp`,
//! `wave_window_overlay.cpp`, with `editors/phase_editor.cpp` and
//! `editors/wave_window_editor.cpp`), shown while a keyframe of their
//! modifier is selected. Each draws twice around the editor's displays, as
//! the Wave Source overlay does (see `wave_source_overlay`).
//!
//! Their bar sliders (the phase mix, the window positions) aren't synth
//! parameters: they are local controls set from the keyframe when it is
//! selected. A change writes the keyframe and auditions it; the commit
//! comes once no button is held (`sliderDragEnded`).

use std::f32::consts::PI;

use sas_engine::sas_oscillators::sas_wavetable::creator::component::{
    PhaseModifier, WaveWindowModifier,
};
use sas_engine::sas_oscillators::sas_wavetable::creator::render::apply_window;

use super::wave_source_overlay::{
    edit_strip, editor_input, paint_controls_background, title_bounds, FrameEdit,
};
use super::wavetable_edit::{KeyframeRef, WavetableEditLayout, GL_TIE_NUDGE};
use super::Panel;
use crate::controls::slider::{Slider, SliderStyle};
use crate::host::Host;
use crate::layout::Rect;
use crate::local::{ModifierControls, PHASE_STYLES, WINDOW_SHAPES};
use crate::paint::RectF;
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `WavetableComponentOverlay::kTitleHeightRatio`.
const TITLE_HEIGHT_RATIO: f32 = 0.4;
/// The selectors' `setTextHeightPercentage`.
const SELECTOR_TEXT_HEIGHT: f32 = 0.4;
/// `PhaseEditor::kNumLines` and `kDefaultHeightPercent`; the controls'
/// editor has `setMaxTickHeight(1.0f)`.
const PHASE_TICKS: usize = 16;
const DEFAULT_TICK_HEIGHT: f32 = 0.2;
const CONTROLS_TICK_HEIGHT: f32 = 1.0;
/// `WaveWindowEditor::kGrabRadius` (of the width) and `kPointsPerSection`.
const GRAB_RADIUS: f32 = 0.05;
const WINDOW_POINTS_PER_SECTION: usize = 50;

/// The edit strip's rows: the strip, its title height and the rows below
/// the titles (`y_title`, `height_title`).
struct Strip {
    strip: Rect,
    padding: i32,
    title_height: i32,
    y_title: i32,
    height_title: i32,
}

impl Strip {
    fn new(scope: &Scope, edit: &WavetableEditLayout) -> Strip {
        let (strip, padding) = edit_strip(scope, edit);
        let title_height = (TITLE_HEIGHT_RATIO * strip.height as f32) as i32;
        Strip {
            strip,
            padding,
            title_height,
            y_title: strip.y + title_height,
            height_title: strip.height - title_height,
        }
    }

    /// `setControlsWidth` and the background's centring.
    fn controls(&self, total_width: i32) -> Rect {
        let x = self.strip.x + (self.strip.width - total_width) / 2;
        Rect::new(x, self.strip.y, total_width, self.strip.height)
    }

    /// `bounds.getHeight() * ratio`, truncated.
    fn times(&self, ratio: f32) -> i32 {
        (self.strip.height as f32 * ratio) as i32
    }
}

/// `PhaseModifierOverlay::setEditBounds`.
#[derive(Clone, Debug, PartialEq)]
pub struct PhaseLayout {
    pub controls: Rect,
    pub lines: [i32; 2],
    pub titles: Vec<Rect>,
    /// `phase_style_`, `slider_` (the controls' phase editor) and `mix_`.
    pub style: Rect,
    pub slider: Rect,
    pub mix: Rect,
}

impl PhaseLayout {
    pub fn new(scope: &Scope, edit: &WavetableEditLayout) -> PhaseLayout {
        let rows = Strip::new(scope, edit);
        let style_width = rows.times(4.0);
        let phase_width = rows.times(8.0);
        let mix_width = rows.times(5.0);
        let mix_padding = rows.times(0.5);
        let total = style_width + phase_width + mix_width + 2 * rows.padding;
        let controls = rows.controls(total);
        let style = Rect::new(controls.x, rows.y_title, style_width, rows.height_title);
        let slider = Rect::new(
            style.right() + rows.padding,
            rows.strip.y,
            phase_width,
            rows.strip.height,
        );
        let mix = Rect::new(
            slider.right() + rows.padding + mix_padding,
            rows.y_title,
            mix_width - 2 * mix_padding,
            rows.height_title,
        );
        let lines = [style_width, style_width + phase_width + rows.padding];
        PhaseLayout {
            controls,
            lines,
            titles: title_bounds(controls, &lines, rows.title_height),
            style,
            slider,
            mix,
        }
    }
}

/// `WaveWindowOverlay::setEditBounds`.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowLayout {
    pub controls: Rect,
    pub lines: [i32; 2],
    pub titles: Vec<Rect>,
    /// `window_shape_`, `left_position_` and `right_position_`.
    pub shape: Rect,
    pub left: Rect,
    pub right: Rect,
}

impl WindowLayout {
    pub fn new(scope: &Scope, edit: &WavetableEditLayout) -> WindowLayout {
        let rows = Strip::new(scope, edit);
        let shape_width = rows.times(5.0);
        let position_width = rows.times(5.0);
        let position_padding = rows.times(0.5);
        let total = shape_width + 2 * position_width + 2 * rows.padding;
        let controls = rows.controls(total);
        let shape = Rect::new(controls.x, rows.strip.y, shape_width, rows.strip.height);
        let left = Rect::new(
            shape.right() + rows.padding + position_padding,
            rows.y_title,
            position_width - 2 * position_padding,
            rows.height_title,
        );
        let right = Rect::new(
            left.right() + rows.padding + 2 * position_padding,
            rows.y_title,
            position_width - 2 * position_padding,
            rows.height_title,
        );
        let lines = [shape_width, shape_width + position_width + rows.padding];
        WindowLayout {
            controls,
            lines,
            titles: title_bounds(controls, &lines, rows.title_height),
            shape,
            left,
            right,
        }
    }
}

/// A selector over `names` on local control `name`.
fn selector(name: &'static str, names: &'static [&'static str]) -> Slider {
    let mut slider = Slider::selector(name, None);
    slider.format.string_lookup = Some(names);
    slider.text_height_percentage = SELECTOR_TEXT_HEIGHT;
    slider
}

/// Whether any mouse button is down.
fn any_button_down(panel: &Panel) -> bool {
    panel.ui.input(|input| input.pointer.any_down())
}

/// Combines a frame's edits: a commit outweighs an audition.
fn combine(result: &mut Option<FrameEdit>, edit: FrameEdit) {
    *result = Some(match result {
        Some(FrameEdit::Done) => FrameEdit::Done,
        _ => edit,
    });
}

/// Paints `slider` at `bounds` without handling it (an overlay's first
/// drawing).
fn paint_slider(panel: &mut Panel, slider: &Slider, bounds: Rect) {
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

/// `PhaseEditor::updatePositions`: one-pixel ticks from the bottom, one per
/// sixteenth of a turn from `phase`, halving in height at each finer
/// division.
pub fn phase_ticks(bounds: Rect, phase: f32, max_tick_height: f32) -> Vec<RectF> {
    let (width, height) = (bounds.width as f32, bounds.height as f32);
    (0..PHASE_TICKS)
        .map(|i| {
            let mut position = phase / (2.0 * PI) + i as f32 / PHASE_TICKS as f32;
            position -= position.floor();
            let mut tick = max_tick_height * 2.0;
            let mut division = 2;
            while division < PHASE_TICKS {
                if i % division != 0 {
                    tick /= 2.0;
                }
                division *= 2;
            }
            let tick_height = tick * 0.5 * height;
            RectF::new(
                bounds.x as f32 + position * width,
                bounds.y as f32 + height - tick_height,
                1.0,
                tick_height,
            )
        })
        .collect()
}

/// One oscillator's Phase Shift overlay.
pub struct PhaseOverlay {
    style: Slider,
    mix: Slider,
    /// The selection and keyframe the controls were set from.
    loaded: Option<(u64, KeyframeRef)>,
    style_seen: f32,
    mix_seen: f32,
    /// The mix changed since the last commit (`sliderDragEnded` to come).
    pending: bool,
    /// Each `PhaseEditor`'s press (`last_edit_position_`'s x): the one over
    /// the waveform, then the controls' one.
    drags: [Option<i32>; 2],
}

impl PhaseOverlay {
    pub fn new(controls: ModifierControls) -> PhaseOverlay {
        PhaseOverlay {
            style: selector(controls.phase_style, &PHASE_STYLES),
            mix: Slider::new(controls.phase_mix, SliderStyle::Horizontal),
            loaded: None,
            style_seen: 0.0,
            mix_seen: 0.0,
            pending: false,
            drags: [None; 2],
        }
    }

    /// Whether a press on one of its phase editors is under way.
    pub fn pressed(&self) -> bool {
        self.drags.iter().any(Option::is_some)
    }

    /// `frameSelected`: the mix and style from keyframe `index` of
    /// `modifier`, when the selection changed since they last were. Upstream
    /// sets the style with a notification, which re-renders an unchanged
    /// table when it differs; the port sets it quietly.
    pub fn sync(
        &mut self,
        host: &Host,
        selection: u64,
        keyframe: KeyframeRef,
        modifier: &PhaseModifier,
    ) {
        if self.loaded == Some((selection, keyframe)) {
            return;
        }
        self.loaded = Some((selection, keyframe));
        if let Some(data) = modifier.keyframes.keyframes.get(keyframe.keyframe) {
            host.set_once(self.mix.name, data.data.mix);
        }
        host.set_once(self.style.name, modifier.settings.phase_style as f32);
        self.mix_seen = host.value(self.mix.name);
        self.style_seen = host.value(self.style.name);
        self.pending = false;
    }

    /// The overlay's controls and phase editors, editing keyframe `index`
    /// of `modifier`.
    pub fn handle(
        &mut self,
        panel: &mut Panel,
        edit: &WavetableEditLayout,
        layout: &PhaseLayout,
        modifier: &mut PhaseModifier,
        index: usize,
        salt: usize,
    ) -> Option<FrameEdit> {
        let host = panel.host;
        let mut result = None;
        let style = host.value(self.style.name);
        if style != self.style_seen {
            self.style_seen = style;
            modifier.settings.phase_style = style as i32;
            combine(&mut result, FrameEdit::Done);
        }
        let keyframe = &mut modifier.keyframes.keyframes.get_mut(index)?.data;
        let mix = host.value(self.mix.name);
        if mix != self.mix_seen {
            self.mix_seen = mix;
            keyframe.mix = mix;
            self.pending = true;
            combine(&mut result, FrameEdit::Changed);
        }
        for (i, bounds) in [edit.waveform, layout.slider].into_iter().enumerate() {
            let input = editor_input(panel, bounds, ("phase editor", salt, i));
            let Some(last) = self.drags[i] else {
                if let (true, Some(pos)) = (input.pressed, input.pos) {
                    // mouseDown.
                    self.drags[i] = Some(pos.0);
                }
                continue;
            };
            let x = input.pos.map_or(last, |pos| pos.0);
            // updatePhase.
            if x != last {
                keyframe.phase += 2.0 * PI * (x - last) as f32 / bounds.width as f32;
                combine(&mut result, FrameEdit::Changed);
            }
            if input.held {
                self.drags[i] = Some(x);
            } else {
                // mouseUp: phaseChanged(phase_, true).
                self.drags[i] = None;
                combine(&mut result, FrameEdit::Done);
            }
        }
        if self.pending && !any_button_down(panel) {
            // sliderDragEnded.
            self.pending = false;
            combine(&mut result, FrameEdit::Done);
        }
        result
    }

    /// The overlay's drawing: the background, the phase editors (the one
    /// over the waveform while a keyframe is selected) and the controls,
    /// handled when `handle_controls`.
    pub fn paint(
        &self,
        panel: &mut Panel,
        edit: &WavetableEditLayout,
        layout: &PhaseLayout,
        phase: Option<f32>,
        handle_controls: bool,
    ) {
        let titles: Vec<(&str, Rect)> = ["STYLE", "", "MIX"]
            .into_iter()
            .zip(layout.titles.iter().copied())
            .collect();
        paint_controls_background(panel, layout.controls, &layout.lines, &titles);
        let lighten = panel.scope.color(ColorId::LightenScreen);
        let shown = phase.unwrap_or(0.0);
        let mut ticks = Vec::new();
        if phase.is_some() {
            ticks.extend(phase_ticks(edit.waveform, shown, DEFAULT_TICK_HEIGHT));
        }
        ticks.extend(phase_ticks(layout.slider, shown, CONTROLS_TICK_HEIGHT));
        for mut tick in ticks {
            tick.y += GL_TIE_NUDGE;
            panel.quad(Quad::new(QuadKind::Color, tick, lighten));
        }
        if handle_controls {
            panel.slider(&self.style, layout.style);
            panel.slider(&self.mix, layout.mix);
        } else {
            paint_slider(panel, &self.style, layout.style);
            paint_slider(panel, &self.mix, layout.mix);
        }
    }
}

/// `WaveWindowEditor::ActiveSide`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    #[default]
    None,
    Left,
    Right,
}

/// `WaveWindowEditor::getHover`: the window edge within the grab radius of
/// `x`, the nearer one (the left on a tie left of it).
pub fn window_hover(left: f32, right: f32, width: f32, x: i32) -> Side {
    let x = x as f32;
    let left_x = left * width;
    let right_x = right * width;
    let delta_left = (left_x - x).abs();
    let delta_right = (right_x - x).abs();
    let choose_left = delta_left < delta_right || (delta_left == delta_right && x < left_x);
    if delta_left < GRAB_RADIUS * width && choose_left {
        Side::Left
    } else if delta_right < GRAB_RADIUS * width {
        Side::Right
    } else {
        Side::None
    }
}

/// One oscillator's Wave Window overlay.
pub struct WindowOverlay {
    shape: Slider,
    left: Slider,
    right: Slider,
    loaded: Option<(u64, KeyframeRef)>,
    shape_seen: f32,
    left_seen: f32,
    right_seen: f32,
    pending: bool,
    /// `WaveWindowEditor`'s `editing_` and `hovering_`.
    editing: Side,
    hovering: Side,
    pressed: bool,
}

impl WindowOverlay {
    pub fn new(controls: ModifierControls) -> WindowOverlay {
        WindowOverlay {
            shape: selector(controls.window_shape, &WINDOW_SHAPES),
            left: Slider::new(controls.window_left, SliderStyle::Horizontal),
            right: Slider::new(controls.window_right, SliderStyle::Horizontal),
            loaded: None,
            shape_seen: 0.0,
            left_seen: 0.0,
            right_seen: 1.0,
            pending: false,
            editing: Side::None,
            hovering: Side::None,
            pressed: false,
        }
    }

    pub fn pressed(&self) -> bool {
        self.pressed
    }

    /// `frameSelected`: the position sliders from keyframe `index`. The
    /// shape selector, and the editor's shape, keep their own value, as
    /// upstream never sets them from the modifier.
    pub fn sync(
        &mut self,
        host: &Host,
        selection: u64,
        keyframe: KeyframeRef,
        modifier: &WaveWindowModifier,
    ) {
        if self.loaded == Some((selection, keyframe)) {
            return;
        }
        self.loaded = Some((selection, keyframe));
        if let Some(data) = modifier.keyframes.keyframes.get(keyframe.keyframe) {
            host.set_once(self.left.name, data.data.left_position);
            host.set_once(self.right.name, data.data.right_position);
        }
        self.left_seen = host.value(self.left.name);
        self.right_seen = host.value(self.right.name);
        self.shape_seen = host.value(self.shape.name);
        self.pending = false;
    }

    /// The overlay's controls and window editor, editing keyframe `index`
    /// of `modifier`.
    pub fn handle(
        &mut self,
        panel: &mut Panel,
        edit: &WavetableEditLayout,
        modifier: &mut WaveWindowModifier,
        index: usize,
        salt: usize,
    ) -> Option<FrameEdit> {
        let host = panel.host;
        let mut result = None;
        let shape = host.value(self.shape.name);
        if shape != self.shape_seen {
            self.shape_seen = shape;
            modifier.settings.window_shape = shape as i32;
            combine(&mut result, FrameEdit::Done);
        }
        let keyframe = &mut modifier.keyframes.keyframes.get_mut(index)?.data;
        let (left, right) = (host.value(self.left.name), host.value(self.right.name));
        if left != self.left_seen {
            let value = left.min(right);
            host.set_once(self.left.name, value);
            self.left_seen = value;
            keyframe.left_position = value;
            self.pending = true;
            combine(&mut result, FrameEdit::Changed);
        }
        if right != self.right_seen {
            let value = right.max(self.left_seen);
            host.set_once(self.right.name, value);
            self.right_seen = value;
            keyframe.right_position = value;
            self.pending = true;
            combine(&mut result, FrameEdit::Changed);
        }

        let bounds = edit.waveform;
        let width = bounds.width as f32;
        let input = editor_input(panel, bounds, ("wave window editor", salt));
        let (left, right) = (keyframe.left_position, keyframe.right_position);
        let mut change = |x: i32, editing: Side, result: &mut Option<FrameEdit>| {
            // changeValues.
            let position = x as f32 / width;
            match editing {
                Side::Left => keyframe.left_position = position.clamp(0.0, right),
                Side::Right => keyframe.right_position = position.clamp(left, 1.0),
                Side::None => return,
            }
            combine(result, FrameEdit::Changed);
        };
        if !self.pressed {
            if let (true, Some(pos)) = (input.pressed, input.pos) {
                // mouseDown.
                self.pressed = true;
                self.editing = window_hover(left, right, width, pos.0);
                change(pos.0, self.editing, &mut result);
            } else {
                self.hovering = match input.pos {
                    Some(pos) if input.hovered => window_hover(left, right, width, pos.0),
                    _ => Side::None,
                };
            }
        } else if input.held {
            if let Some(pos) = input.pos {
                change(pos.0, self.editing, &mut result);
            }
        } else {
            // mouseUp.
            self.pressed = false;
            if self.editing != Side::None {
                combine(&mut result, FrameEdit::Done);
            }
            self.editing = Side::None;
        }
        if result.is_some() {
            // windowChanged: the sliders follow the editor.
            let (left, right) = (keyframe.left_position, keyframe.right_position);
            host.set_once(self.left.name, left);
            host.set_once(self.right.name, right);
            self.left_seen = host.value(self.left.name);
            self.right_seen = host.value(self.right.name);
        }
        if self.pending && !any_button_down(panel) {
            self.pending = false;
            combine(&mut result, FrameEdit::Done);
        }
        result
    }

    /// The overlay's drawing: the background, the window editor while a
    /// keyframe is selected (`setPoints`, with its edge bars), and the
    /// controls, handled when `handle_controls`.
    pub fn paint(
        &self,
        panel: &mut Panel,
        edit: &WavetableEditLayout,
        layout: &WindowLayout,
        window: Option<(f32, f32)>,
        handle_controls: bool,
    ) {
        let titles: Vec<(&str, Rect)> = ["", "LEFT POSITION", "RIGHT POSITION"]
            .into_iter()
            .zip(layout.titles.iter().copied())
            .collect();
        paint_controls_background(panel, layout.controls, &layout.lines, &titles);
        if let Some((left, right)) = window {
            self.paint_editor(panel, edit.waveform, left, right);
        }
        if handle_controls {
            panel.slider(&self.shape, layout.shape);
            panel.slider(&self.left, layout.left);
            panel.slider(&self.right, layout.right);
        } else {
            paint_slider(panel, &self.shape, layout.shape);
            paint_slider(panel, &self.left, layout.left);
            paint_slider(panel, &self.right, layout.right);
        }
    }

    fn paint_editor(&self, panel: &mut Panel, bounds: Rect, left: f32, right: f32) {
        let scope = &panel.scope;
        let shape = self.shape_seen as i32;
        let (width, height) = (bounds.width as f32, bounds.height as f32);
        let half_height = 0.5 * height;
        let total = 4 * WINDOW_POINTS_PER_SECTION;
        let mut line = LineRenderer::new(total, false);
        line.fill = true;
        line.fit = true;
        line.line_width = scope.value(ValueId::WidgetLineWidth);
        line.color = scope.color(ColorId::WidgetPrimary1);
        let fill = scope
            .color(ColorId::WidgetSecondary1)
            .with_multiplied_alpha(0.5);
        line.set_fill_colors(
            fill.with_multiplied_alpha(1.0 - scope.value(ValueId::WidgetFillFade)),
            fill,
        );
        let sections = [(0.0, left * width, false), (right * width, width, true)];
        for (section, &(from, to, falling)) in sections.iter().enumerate() {
            for i in 0..WINDOW_POINTS_PER_SECTION {
                let t = i as f32 / (WINDOW_POINTS_PER_SECTION as f32 - 1.0);
                let x = from + (to - from) * t;
                let window_t = if falling { 1.0 - t } else { t };
                let y = apply_window(shape, window_t) * half_height;
                let index = section * WINDOW_POINTS_PER_SECTION + i;
                line.set_x_at(index, x);
                line.set_y_at(index, half_height + y);
                line.set_x_at(total - index - 1, x);
                line.set_y_at(total - index - 1, half_height - y);
            }
        }
        // The edge bars, then the edited and hovered edges again.
        let bar = |position: f32| {
            RectF::new(
                bounds.x as f32 + (position - 0.5 * GRAB_RADIUS) * width,
                bounds.y as f32,
                GRAB_RADIUS * width,
                height,
            )
        };
        let mut bars = vec![bar(left), bar(right)];
        for side in [self.editing, self.hovering] {
            match side {
                Side::Left => bars.push(bar(left)),
                Side::Right => bars.push(bar(right)),
                Side::None => {}
            }
        }
        let colour = scope
            .color(ColorId::LightenScreen)
            .with_multiplied_alpha(0.5);
        line.paint(panel.pixels, bounds.into(), true);
        for rect in bars {
            panel.quad(Quad::new(QuadKind::Color, rect, colour));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `updatePositions`: sixteen ticks a sixteenth of the width apart,
    /// the first at the phase. Halving for each of 2, 4 and 8 that doesn't
    /// divide the index leaves the first and the ninth full height.
    #[test]
    fn phase_ticks_follow_the_phase() {
        let bounds = Rect::new(0, 0, 160, 100);
        let ticks = phase_ticks(bounds, PI / 2.0, 0.5);
        assert_eq!(ticks.len(), 16);
        assert!((ticks[0].x - 40.0).abs() < 1e-3);
        assert!((ticks[1].x - 50.0).abs() < 1e-3);
        assert_eq!(ticks[0].height, 50.0);
        assert_eq!(ticks[8].height, 50.0);
        assert_eq!(ticks[4].height, 25.0);
        assert_eq!(ticks[12].height, 25.0);
        assert_eq!(ticks[2].height, 12.5);
        assert_eq!(ticks[1].height, 6.25);
        assert_eq!(ticks[0].y + ticks[0].height, 100.0);
        // Wrapped past the right edge.
        assert!((ticks[15].x - 30.0).abs() < 1e-3);
    }

    /// `getHover`: the nearer edge within the grab radius.
    #[test]
    fn the_nearer_window_edge_is_grabbed() {
        assert_eq!(window_hover(0.25, 0.75, 200.0, 52), Side::Left);
        assert_eq!(window_hover(0.25, 0.75, 200.0, 148), Side::Right);
        assert_eq!(window_hover(0.25, 0.75, 200.0, 100), Side::None);
        // Both within reach: the nearer.
        assert_eq!(window_hover(0.5, 0.55, 200.0, 108), Side::Right);
    }
}
