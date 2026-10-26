// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `LineEditor` as the LFOs use it (`LfoEditor`): an LFO's shape, its points
//! and powers to drag, and its grid. Ports
//! `vital/src/interface/editor_components/line_editor.cpp` and
//! `lfo_editor.cpp`.
//!
//! - A point drags within its neighbours, snapping near the grid (Alt
//!   doesn't snap); the first and last points move together while level. A
//!   power dot bends its segment (Shift bends all, Alt alternates).
//! - A double click removes a point, straightens a power (Shift: all), or
//!   adds a point.
//! - Paint mode (the paint button, or held Command) draws the paint
//!   pattern into the grid cell under the pointer.
//! - The right-click menu sets the start phase, removes a point, resets a
//!   power, copies the shape and flips it.
//!
//! Not yet: entering a point's phase or value as text, pasting, saving and
//! importing or exporting shapes (with the browsers), and the playing
//! voices' positions and the animated line.

use egui::{Modifiers, Pos2, Sense};
use sas_common::line_generator::LineGenerator;
use sas_common::scalar::power_scale;

use crate::controls::slider::{DOUBLE_CLICK_TIMEOUT, MULTIPLE_CLICK_DISTANCE};
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::popup::{Menu, MenuItem};
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::scope::Scope;
use crate::sections::{MenuOwner, Panel};
use crate::skin::{ColorId, ValueId};

const POSITION_WIDTH: f32 = 9.0;
const POWER_WIDTH: f32 = 7.0;
const RING_THICKNESS: f32 = 0.45;
const GRAB_RADIUS: f32 = 12.0;
const DRAG_RADIUS: f32 = 20.0;
const RESOLUTION: usize = 64;
const NUM_WRAP_POINTS: usize = 8;
const DRAW_POINTS: usize = RESOLUTION + LineGenerator::MAX_POINTS;
const TOTAL_POINTS: usize = DRAW_POINTS + 2 * NUM_WRAP_POINTS;
const PADDING_Y: f32 = 6.0;
const POWER_MOUSE_MULTIPLIER: f32 = 9.0;
const MIN_POINT_DISTANCE_FOR_POWER: f32 = 3.0;
/// `SynthLfo::kMaxPower`.
const MAX_POWER: f32 = 20.0;
/// Frames the editor keeps drawing its own shape after an edit, until the
/// synth's copy has it.
const HOLD_FRAMES: u32 = 10;

/// `LfoSection::getPaintPattern`.
pub fn paint_pattern(pattern: usize) -> &'static [(f32, f32)] {
    match pattern {
        1 => &[(0.0, 1.0), (0.5, 1.0), (0.5, 0.0), (1.0, 0.0)],
        2 => &[(0.0, 1.0), (1.0, 0.0)],
        3 => &[(0.0, 0.0), (1.0, 1.0)],
        4 => &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)],
        _ => &[(0.0, 1.0), (1.0, 1.0)],
    }
}

/// `strings::kPaintPatternNames`.
pub const PAINT_PATTERN_NAMES: [&str; 5] = ["Step", "Half", "Down", "Up", "Tri"];

/// The right-click menu's choices (`LineEditor::MenuOptions` and
/// `LfoEditor`'s).
pub mod menu {
    pub const REMOVE_POINT: i32 = 1;
    pub const RESET_POWER: i32 = 4;
    pub const COPY: i32 = 5;
    pub const FLIP_HORIZONTAL: i32 = 9;
    pub const FLIP_VERTICAL: i32 = 10;
    pub const SET_PHASE_TO_POINT: i32 = 11;
    pub const SET_PHASE_TO_POWER: i32 = 12;
    pub const SET_PHASE_TO_GRID: i32 = 13;
}

/// A choice from an editor's menu, for the editor to carry out next frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineAction {
    pub lfo: usize,
    pub choice: i32,
    pub point: i32,
    pub power: i32,
    pub grid_section: i32,
}

/// What a section tells its editor this frame.
#[derive(Clone, Copy, Debug)]
pub struct LineSettings {
    pub grid_size_x: usize,
    pub grid_size_y: usize,
    /// The paint button.
    pub paint: bool,
    pub paint_pattern: usize,
    /// The LFO's phase control, which "Set Start Point" sets.
    pub phase: &'static str,
}

#[derive(Clone, Debug)]
struct LineState {
    /// The editor's own copy while it is editing or until the synth's copy
    /// catches up.
    line: Option<LineGenerator>,
    pending: bool,
    holding: u32,
    active_point: i32,
    active_power: i32,
    active_grid_section: i32,
    dragging: bool,
    painting_down: bool,
    temporary_paint: bool,
    last: (f32, f32),
    mouse_down: bool,
    /// The last press's time and place, for JUCE's double-click rules.
    last_press: Option<(f64, Pos2)>,
    /// This press is a double click's second, acted on at its release.
    double_click: bool,
}

impl Default for LineState {
    fn default() -> LineState {
        LineState {
            line: None,
            pending: false,
            holding: 0,
            active_point: -1,
            active_power: -1,
            active_grid_section: -1,
            dragging: false,
            painting_down: false,
            temporary_paint: false,
            last: (0.0, 0.0),
            mouse_down: false,
            last_press: None,
            double_click: false,
        }
    }
}

/// The geometry of a shape in an editor of a given size (pixels).
struct Geometry<'a> {
    line: &'a LineGenerator,
    width: f32,
    height: f32,
    size_ratio: f32,
}

impl Geometry<'_> {
    /// `padY` (`kPaddingX` is 0 and LFO lines loop, so x isn't padded).
    fn pad_y(&self, y: f32) -> f32 {
        let pad = self.size_ratio * PADDING_Y;
        y * (self.height - 2.0 * pad) / self.height + pad
    }

    fn unpad_y(&self, y: f32) -> f32 {
        let pad = self.size_ratio * PADDING_Y;
        (y - pad) * self.height / (self.height - 2.0 * pad)
    }

    fn point(&self, index: usize) -> (f32, f32) {
        self.line.point(index)
    }

    /// `getPowerPosition`.
    fn power_position(&self, index: usize) -> (f32, f32) {
        let num_points = self.line.num_points();
        let from = self.point(index);
        let to = if index < num_points - 1 {
            self.point(index + 1)
        } else {
            let first = self.point(0);
            (first.0 + 1.0, first.1)
        };
        let mut x = (from.0 + to.0) / 2.0;
        if x >= 1.0 {
            x -= 1.0;
        }
        let power_t = power_scale(0.5, self.line.power(index));
        (x, from.1 + (to.1 - from.1) * power_t)
    }

    /// `powerActive`.
    fn power_active(&self, index: usize) -> bool {
        let num_points = self.line.num_points();
        let delta = if index < num_points - 1 {
            let (a, b) = (self.point(index + 1), self.point(index));
            (a.0 - b.0, a.1 - b.1)
        } else {
            let (a, b) = (self.point(0), self.line.last_point());
            (a.0 - b.0 + 1.0, a.1 - b.1)
        };
        self.width * delta.0 >= MIN_POINT_DISTANCE_FOR_POWER && delta.1 != 0.0
    }

    /// `getHoverPoint` at `position` (pixels from the editor's top left).
    fn hover_point(&self, position: (f32, f32)) -> i32 {
        let p = (position.0, self.unpad_y(position.1));
        let grab = GRAB_RADIUS * self.size_ratio;
        let mut min_distance = grab * grab;
        let mut hover = -1;
        for i in 0..self.line.num_points() {
            let (x, y) = self.point(i);
            let d = (p.0 - x * self.width).powi(2) + (p.1 - y * self.height).powi(2);
            if d < min_distance {
                min_distance = d;
                hover = i as i32;
            }
        }
        hover
    }

    /// `getHoverPower`.
    fn hover_power(&self, position: (f32, f32)) -> i32 {
        let p = (position.0, self.unpad_y(position.1));
        let grab = GRAB_RADIUS * self.size_ratio;
        let mut min_distance = grab * grab;
        let mut hover = -1;
        for i in 0..self.line.num_points() {
            if self.power_active(i) {
                let (x, y) = self.power_position(i);
                let d = (p.0 - x * self.width).powi(2) + (p.1 - y * self.height).powi(2);
                if d < min_distance {
                    min_distance = d;
                    hover = i as i32;
                }
            }
        }
        hover
    }

    /// `resetWavePath`: the drawn line's points, with the loop's wrap
    /// points on each side.
    fn wave_path(&self) -> Vec<(f32, f32)> {
        let line = self.line;
        let num_points = line.num_points();
        let intermediate_points = DRAW_POINTS - num_points;
        let mut prev = line.last_point();
        let mut power = line.last_power();
        prev.0 -= 1.0;
        let mut points = vec![(0.0, 0.0); TOTAL_POINTS];
        let mut draw_index = 0;
        let mut point_index = 0;
        for i in 0..intermediate_points {
            let t = i as f32 / (intermediate_points as f32 - 1.0);
            while point_index < num_points && t >= line.point(point_index).0 {
                prev = line.point(point_index);
                power = line.power(point_index);
                points[NUM_WRAP_POINTS + draw_index] =
                    (self.width * prev.0, self.pad_y(self.height * prev.1));
                point_index += 1;
                draw_index += 1;
            }
            let next = if point_index < num_points {
                line.point(point_index)
            } else {
                let first = line.point(0);
                (first.0 + 1.0, first.1)
            };
            let x_distance = next.0 - prev.0;
            let mut point_t = 0.0;
            if x_distance > 0.0 {
                point_t = (t - prev.0) / x_distance;
            }
            if line.smooth() {
                point_t = LineGenerator::smooth_transition(point_t);
            }
            point_t = power_scale(point_t, power);
            let value = prev.1 + (next.1 - prev.1) * point_t;
            points[NUM_WRAP_POINTS + draw_index] =
                (self.width * t, self.pad_y(self.height * value));
            draw_index += 1;
        }
        let end_value = line.point(num_points - 1).1;
        while draw_index < DRAW_POINTS {
            points[NUM_WRAP_POINTS + draw_index] =
                (self.width, self.pad_y(self.height * end_value));
            draw_index += 1;
        }
        for i in 0..NUM_WRAP_POINTS {
            let end = points[DRAW_POINTS + i];
            points[i] = (end.0 - self.width, end.1);
            let start = points[NUM_WRAP_POINTS + i];
            points[i + DRAW_POINTS + NUM_WRAP_POINTS] = (start.0 + self.width, start.1);
        }
        points
    }
}

/// `getMinX` / `getMaxX`.
fn x_range(line: &LineGenerator, index: usize) -> (f32, f32) {
    let last = line.num_points() - 1;
    if index == 0 {
        (0.0, 0.0)
    } else if index == last {
        (1.0, 1.0)
    } else {
        (line.point(index - 1).0, line.point(index + 1).0)
    }
}

/// `getSnappedX` with the grid's snap radius (`getSnapRadiusX`).
fn snap(value: f32, grid: usize, radius: f32) -> f32 {
    if grid <= 1 {
        return value;
    }
    let snapped = (value * grid as f32).round() / grid as f32;
    if (snapped - value).abs() < radius {
        snapped
    } else {
        value
    }
}

/// `addPointAt`: inserts `position` (fractions) before the first point to
/// its right.
fn add_point_at(line: &mut LineGenerator, position: (f32, f32)) {
    if line.num_points() >= LineGenerator::MAX_POINTS {
        return;
    }
    let index = (0..line.num_points())
        .find(|&i| line.point(i).0 > position.0)
        .unwrap_or(line.num_points());
    line.add_point(index, position);
    line.render();
}

/// `movePoint`.
fn move_point(
    line: &mut LineGenerator,
    index: usize,
    position: (f32, f32),
    size: (f32, f32),
    grid: (usize, usize),
    snapping: bool,
) {
    let (min_x, max_x) = x_range(line, index);
    let last = line.num_points() - 1;
    let mut x = (position.0 / size.0).clamp(min_x, max_x);
    let mut y = (position.1 / size.1).clamp(0.0, 1.0);
    if snapping {
        if grid.0 > 1 {
            let snapped = ((x * grid.0 as f32).round() / grid.0 as f32).clamp(min_x, max_x);
            if (snapped - x).abs() < 0.02 {
                x = snapped;
            }
        }
        y = snap(y, grid.1, 0.04);
    }
    if line.point(0).1 == line.point(last).1 {
        if index == 0 {
            line.set_point(last, (1.0, y));
        } else if index == last {
            line.set_point(0, (0.0, y));
        }
    }
    line.set_point(index, (x, y));
    line.render();
}

/// `movePower`: `delta` is the pointer's vertical move as a fraction of the
/// height.
fn move_power(line: &mut LineGenerator, index: usize, delta: f32, all: bool, alternate: bool) {
    let num_points = line.num_points();
    let (start, end) = if all {
        (0, num_points as i32 - 2)
    } else {
        (index as i32, index as i32)
    };
    let mut alternate_mult = 1.0;
    if !alternate && line.point(index).1 < line.point((index + 1) % num_points).1 {
        alternate_mult = -1.0;
    }
    for i in start..=end {
        let i = i as usize;
        let from_y = line.point(i).1;
        let to_y = line.point((i + 1) % num_points).1;
        if from_y == to_y {
            continue;
        }
        let mut delta_amount = delta * alternate_mult;
        if from_y < to_y && alternate {
            delta_amount *= -1.0;
        }
        let power = line.power(i) + delta_amount * POWER_MOUSE_MULTIPLIER;
        line.set_power(i, power.clamp(-MAX_POWER, MAX_POWER));
    }
    line.render();
}

/// `paintLine`: the pattern drawn into the grid cell at `position`
/// (pixels). Returns the cell.
fn paint_line(
    line: &mut LineGenerator,
    position: (f32, f32),
    size: (f32, f32),
    grid: (usize, usize),
    pattern: &[(f32, f32)],
    snapping: bool,
) -> i32 {
    let percent_x = position.0 / size.0;
    let mut percent_y = (position.1 / size.1).clamp(0.0, 1.0);
    let grid_x = grid.0 as i32;
    let section = ((grid_x as f32 * percent_x) as i32).clamp(0, grid_x - 1);
    let from_x = section as f32 / grid_x as f32;
    let to_x = (section as f32 + 1.0) / grid_x as f32;
    if snapping && grid.1 > 0 {
        percent_y = snap(percent_y, grid.1, 0.04);
    }

    let mut from_index: i32 = -1;
    let mut start_num_points = line.num_points() as i32;
    let mut to_index = start_num_points;
    for i in 0..start_num_points {
        if line.point(i as usize).0 < from_x {
            from_index = i;
        }
        let reverse_index = start_num_points - i - 1;
        if line.point(reverse_index as usize).0 > to_x {
            to_index = reverse_index;
        }
    }
    let mut new_points = Vec::new();
    let from_intersect = line.value_at_phase(from_x);
    let to_intersect = line.value_at_phase(to_x);
    if line.point((from_index + 1) as usize).0 != from_x {
        new_points.push((from_x, from_intersect));
        to_index += 1;
        start_num_points += 1;
    }
    from_index += 1;
    if line.point((to_index - 1) as usize).0 != to_x {
        new_points.push((to_x, to_intersect));
        start_num_points += 1;
    } else {
        to_index -= 1;
    }

    let pattern_length = pattern.len() as i32;
    let delta_size = pattern_length - 0.max(to_index - from_index - 1);
    let num_points = start_num_points + delta_size;
    if num_points >= LineGenerator::MAX_POINTS as i32 {
        return section;
    }
    for point in new_points {
        add_point_at(line, point);
    }
    line.set_num_points(num_points as usize);
    if delta_size > 0 {
        let mut i = num_points - 1;
        while i >= delta_size.max(to_index) {
            let source = (i - delta_size) as usize;
            line.set_point(i as usize, line.point(source));
            line.set_power(i as usize, line.power(source));
            i -= 1;
        }
    } else if delta_size < 0 {
        for i in (from_index + 1)..num_points {
            let source = (i - delta_size) as usize;
            line.set_point(i as usize, line.point(source));
            line.set_power(i as usize, line.power(source));
        }
    }
    for (i, &(t, y)) in pattern.iter().enumerate() {
        let x = from_x * (1.0 - t) + to_x * t;
        let y = 1.0 - y * (1.0 - percent_y);
        let index = (from_index + 1 + i as i32) as usize;
        line.set_point(index, (x, y));
        line.set_power(index, 0.0);
    }
    line.render();
    section
}

/// `mouseDoubleClick` at `p`: removes the point there (not an end),
/// straightens the power there (Shift: all), or adds a point. Returns
/// whether the shape changed.
fn double_click_at(
    line: &mut LineGenerator,
    p: (f32, f32),
    size: (f32, f32),
    size_ratio: f32,
    shift: bool,
) -> bool {
    let g = geometry(line, size, size_ratio);
    let hover_point = g.hover_point(p);
    let hover_power = g.hover_power(p);
    let num_points = line.num_points() as i32;
    if hover_point >= 0 {
        if hover_point == 0 || hover_point == num_points - 1 || num_points <= 1 {
            return false;
        }
        line.remove_point(hover_point as usize);
        line.render();
        true
    } else if hover_power >= 0 {
        if shift {
            for i in 0..line.num_points() - 1 {
                line.set_power(i, 0.0);
            }
        } else {
            line.set_power(hover_power as usize, 0.0);
        }
        line.render();
        true
    } else if line.num_points() < LineGenerator::MAX_POINTS {
        add_point_at(line, (p.0 / size.0, p.1 / size.1));
        true
    } else {
        false
    }
}

/// An LFO's editor.
#[derive(Clone, Copy, Debug)]
pub struct LineEditor {
    /// The LFO (0-based).
    pub lfo: usize,
}

/// What the editor tells its section after a frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct LineEditorEvent {
    /// Paint mode as shown: the paint button, flipped while Command is held
    /// (`togglePaintMode`).
    pub painting: bool,
    /// The shape's `smooth` flag.
    pub smooth: bool,
}

/// The init LFO shape (`LineGenerator`'s constructor).
fn init_shape() -> LineGenerator {
    let mut line = LineGenerator::new(LineGenerator::DEFAULT_RESOLUTION);
    line.init_triangle();
    line
}

impl LineEditor {
    /// The shape as the editor shows it this frame.
    pub fn shape(&self, panel: &Panel) -> LineGenerator {
        let state = self.state(panel);
        state.line.unwrap_or_else(|| {
            panel
                .host
                .model()
                .lfo_shape(self.lfo)
                .unwrap_or_else(init_shape)
        })
    }

    fn id(&self, panel: &Panel) -> egui::Id {
        panel.ui.scope_id().with(("line editor", self.lfo))
    }

    fn state(&self, panel: &Panel) -> LineState {
        let id = self.id(panel);
        panel.ui.data(|data| data.get_temp(id)).unwrap_or_default()
    }

    /// Sets the shape's smooth flag (`LfoSection`'s smooth button,
    /// `LineEditor::setSmooth`).
    pub fn set_smooth(&self, panel: &mut Panel, smooth: bool) {
        let mut line = self.shape(panel);
        line.set_smooth(smooth);
        line.render();
        let id = self.id(panel);
        let mut state = self.state(panel);
        state.line = Some(line);
        state.pending = true;
        self.push(panel, &mut state);
        panel.ui.data_mut(|data| data.insert_temp(id, state));
    }

    /// Sends the editor's copy to the synth if it changed.
    fn push(&self, panel: &Panel, state: &mut LineState) {
        if !state.pending {
            return;
        }
        if let Some(line) = &state.line {
            if panel.host.model().set_lfo_shape(self.lfo, line.clone()) {
                state.pending = false;
                state.holding = HOLD_FRAMES;
            }
        }
    }

    /// Handles the mouse and the menu's choices, and draws the editor.
    pub fn show(&self, panel: &mut Panel, bounds: Rect, settings: LineSettings) -> LineEditorEvent {
        let id = self.id(panel);
        let mut state = self.state(panel);
        let busy = state.dragging || state.painting_down || state.pending || state.holding > 0;
        if !busy {
            state.line = None;
        }
        if state.holding > 0 && !state.dragging && !state.painting_down && !state.pending {
            state.holding -= 1;
        }
        let mut line = state.line.take().unwrap_or_else(|| {
            panel
                .host
                .model()
                .lfo_shape(self.lfo)
                .unwrap_or_else(init_shape)
        });
        let mut changed = false;

        let size_ratio = panel.scope.size_ratio();
        let size = (bounds.width as f32, bounds.height as f32);
        let grid = (settings.grid_size_x, settings.grid_size_y);

        // A choice from last frame's menu.
        if let Some(action) = panel.overlay.line_action.take_if(|a| a.lfo == self.lfo) {
            changed |= self.respond(panel, &mut line, action, settings);
            state.active_point = -1;
            state.active_power = -1;
            state.active_grid_section = -1;
        }

        let rect = panel.pixels.rect(bounds.into());
        let response = panel.ui.interact(rect, id, Sense::click_and_drag());
        let ppp = panel.pixels.pixels_per_point();
        let origin = panel.pixels.area().min;
        let local = |pos: Pos2| {
            (
                (pos.x - origin.x) * ppp - bounds.x as f32,
                (pos.y - origin.y) * ppp - bounds.y as f32,
            )
        };
        let pointer = panel.ui.input(|input| input.pointer.interact_pos());
        let modifiers: Modifiers = panel.ui.input(|input| input.modifiers);
        let primary_down = panel
            .ui
            .input(|input| input.pointer.button_down(egui::PointerButton::Primary));
        if response.hovered() || state.dragging || state.painting_down {
            state.temporary_paint = modifiers.command;
        } else {
            state.temporary_paint = false;
        }
        let painting = settings.paint != state.temporary_paint;

        // mouseMove.
        if response.hovered() && !state.dragging && !state.painting_down {
            if let Some(pos) = pointer {
                let p = local(pos);
                if painting {
                    let percent_x = p.0 / size.0;
                    state.active_grid_section =
                        ((grid.0 as f32 * percent_x) as i32).clamp(0, grid.0 as i32 - 1);
                } else {
                    let g = geometry(&line, size, size_ratio);
                    state.active_point = g.hover_point(p);
                    state.active_power = if state.active_point < 0 {
                        g.hover_power(p)
                    } else {
                        -1
                    };
                }
            }
        } else if !response.hovered()
            && !state.dragging
            && !state.painting_down
            && !response.secondary_clicked()
        {
            state.active_point = -1;
            state.active_power = -1;
            state.active_grid_section = -1;
        }

        if response.secondary_clicked() {
            self.open_menu(panel, &line, &state, pointer.unwrap_or_default());
        } else if response.is_pointer_button_down_on() && primary_down {
            if let Some(pos) = pointer {
                let p = local(pos);
                let starting = !state.mouse_down;
                state.mouse_down = true;
                // JUCE's multiple-click rules, as Slider uses them.
                let now = panel.ui.input(|input| input.time);
                let double_click = starting
                    && state.last_press.is_some_and(|(time, at)| {
                        now - time < DOUBLE_CLICK_TIMEOUT
                            && (pos.x - at.x).abs() < MULTIPLE_CLICK_DISTANCE
                            && (pos.y - at.y).abs() < MULTIPLE_CLICK_DISTANCE
                    });
                if starting {
                    state.last_press = Some((now, pos));
                }
                if painting {
                    if starting || p != state.last {
                        state.active_grid_section = paint_line(
                            &mut line,
                            p,
                            size,
                            grid,
                            paint_pattern(settings.paint_pattern),
                            !modifiers.alt,
                        );
                        changed = true;
                    }
                    state.painting_down = true;
                } else if starting {
                    // drawDown.
                    let g = geometry(&line, size, size_ratio);
                    let hover_point = g.hover_point(p);
                    if hover_point >= 0 {
                        state.active_point = hover_point;
                        state.active_power = -1;
                        state.dragging = true;
                    } else {
                        let hover_power = g.hover_power(p);
                        if hover_power >= 0 {
                            state.active_power = hover_power;
                            state.active_point = -1;
                            state.dragging = true;
                        }
                    }
                    state.double_click = double_click;
                } else if state.dragging && p != state.last {
                    // drawDrag.
                    if state.active_point >= 0 {
                        move_point(
                            &mut line,
                            state.active_point as usize,
                            p,
                            size,
                            grid,
                            !modifiers.alt,
                        );
                        changed = true;
                    } else if state.active_power >= 0 {
                        let delta = (p.1 - state.last.1) / size.1;
                        move_power(
                            &mut line,
                            state.active_power as usize,
                            delta,
                            modifiers.shift,
                            modifiers.alt,
                        );
                        changed = true;
                    }
                }
                state.last = p;
            }
        } else {
            // mouseDoubleClick comes after the second click's mouseUp.
            if state.mouse_down && state.double_click && !painting {
                if let Some(pos) = pointer {
                    let p = local(pos);
                    changed |= double_click_at(&mut line, p, size, size_ratio, modifiers.shift);
                    state.active_point = geometry(&line, size, size_ratio).hover_point(p);
                    state.active_power = -1;
                }
            }
            state.double_click = false;
            state.dragging = false;
            state.painting_down = false;
            state.mouse_down = false;
        }

        if changed {
            state.pending = true;
        }
        let smooth = line.smooth();
        state.line = Some(line);
        self.push(panel, &mut state);
        let line = state.line.clone().expect("set above");
        panel
            .ui
            .data_mut(|data| data.insert_temp(id, state.clone()));

        self.paint(panel, bounds, line, &state, settings, painting);
        LineEditorEvent { painting, smooth }
    }

    /// `LfoEditor::mouseDown` with the popup menu.
    fn open_menu(&self, panel: &mut Panel, line: &LineGenerator, state: &LineState, at: Pos2) {
        let mut items = Vec::new();
        let num_points = line.num_points() as i32;
        if state.active_point >= 0 {
            items.push(MenuItem::new(menu::SET_PHASE_TO_POINT, "Set Start Point"));
            if state.active_point >= 1 && state.active_point < num_points - 1 {
                items.push(MenuItem::new(menu::REMOVE_POINT, "Remove Point"));
            }
            items.push(MenuItem::new(-1, ""));
        } else if state.active_power >= 0 {
            items.push(MenuItem::new(menu::SET_PHASE_TO_POWER, "Set Start Point"));
            items.push(MenuItem::new(menu::RESET_POWER, "Reset Power"));
            items.push(MenuItem::new(-1, ""));
        } else if state.active_grid_section >= 0 {
            items.push(MenuItem::new(menu::SET_PHASE_TO_GRID, "Set Start Point"));
        }
        items.push(MenuItem::new(menu::COPY, "Copy"));
        items.push(MenuItem::new(menu::FLIP_HORIZONTAL, "Flip Horizontal"));
        items.push(MenuItem::new(menu::FLIP_VERTICAL, "Flip Vertical"));
        let ppp = panel.pixels.pixels_per_point();
        let origin = panel.pixels.area().min;
        let position = Pos2::new((at.x - origin.x) * ppp, (at.y - origin.y) * ppp);
        panel.overlay.menu = Some(Menu::new(
            items,
            position,
            MenuOwner::Line(LineAction {
                lfo: self.lfo,
                choice: 0,
                point: state.active_point,
                power: state.active_power,
                grid_section: state.active_grid_section,
            }),
        ));
    }

    /// `LfoEditor::respondToCallback`. Returns whether the shape changed.
    fn respond(
        &self,
        panel: &mut Panel,
        line: &mut LineGenerator,
        action: LineAction,
        settings: LineSettings,
    ) -> bool {
        let num_points = line.num_points() as i32;
        let host = panel.host;
        match action.choice {
            menu::SET_PHASE_TO_POINT => {
                if action.point >= 0 && action.point < num_points {
                    host.set_once(settings.phase, line.point(action.point as usize).0);
                }
                false
            }
            menu::SET_PHASE_TO_POWER => {
                if action.power >= 0 && action.power < num_points - 1 {
                    let from = line.point(action.power as usize).0;
                    let to = line.point(action.power as usize + 1).0;
                    host.set_once(settings.phase, (from + to) / 2.0);
                }
                false
            }
            menu::SET_PHASE_TO_GRID => {
                if action.grid_section >= 0 && settings.grid_size_x > 0 {
                    host.set_once(
                        settings.phase,
                        action.grid_section as f32 / settings.grid_size_x as f32,
                    );
                }
                false
            }
            menu::REMOVE_POINT => {
                if action.point > 0 && action.point < num_points - 1 {
                    line.remove_point(action.point as usize);
                    line.render();
                    return true;
                }
                false
            }
            menu::RESET_POWER => {
                if action.power >= 0 && action.power < num_points - 1 {
                    line.set_power(action.power as usize, 0.0);
                    line.render();
                    return true;
                }
                false
            }
            menu::COPY => {
                panel.ui.ctx().copy_text(line.to_json().dump());
                false
            }
            menu::FLIP_HORIZONTAL => {
                line.flip_horizontal();
                line.render();
                true
            }
            menu::FLIP_VERTICAL => {
                line.flip_vertical();
                line.render();
                true
            }
            _ => false,
        }
    }

    /// `LfoEditor::render` at rest.
    fn paint(
        &self,
        panel: &mut Panel,
        bounds: Rect,
        line: LineGenerator,
        state: &LineState,
        settings: LineSettings,
        painting: bool,
    ) {
        let scope: Scope = panel.scope;
        let size_ratio = scope.size_ratio();
        let lighten = scope.color(ColorId::LightenScreen);
        let line_left = scope.color(ColorId::WidgetPrimary1);
        let line_right = scope.color(ColorId::WidgetPrimary2);
        let center = scope.color(ColorId::WidgetCenterLine);
        let background = scope.color(ColorId::WidgetBackground);
        let fill_fade = scope.value(ValueId::WidgetFillFade);
        let fill_left = scope.color(ColorId::WidgetSecondary1);
        let fill_right = scope.color(ColorId::WidgetSecondary2);
        let accent1 = scope.color(ColorId::WidgetAccent1);
        let accent2 = scope.color(ColorId::WidgetAccent2);
        let line_width = scope.value(ValueId::WidgetLineWidth);
        let fill_center = scope.value(ValueId::WidgetFillCenter);
        let rounding = scope.value(ValueId::WidgetRoundedCorner);
        let body = scope.color(ColorId::Body);
        let state = state.clone();
        let (grid_x, grid_y) = (settings.grid_size_x, settings.grid_size_y);

        panel.layer().custom(move |pixels: &PixelPainter| {
            let clip = RectF::from(bounds);
            let (width, height) = (bounds.width as f32, bounds.height as f32);
            let at = |x: f32, y: f32, w: f32, h: f32| {
                RectF::new(bounds.x as f32 + x, bounds.y as f32 + y, w, h)
            };
            let centred = |(x, y): (f32, f32), d: f32| at(x - d / 2.0, y - d / 2.0, d, d);
            let g = Geometry {
                line: &line,
                width,
                height,
                size_ratio,
            };

            // setGridPositions.
            let mut grid = QuadBatch::new();
            for i in 1..grid_x {
                let x = (i as f32 * 2.0 / grid_x as f32 - 1.0 + 1.0) / 2.0 * width;
                grid.push(Quad::new(
                    QuadKind::Color,
                    at(x - 0.5, 0.0, 1.0, height),
                    lighten,
                ));
            }
            let y_scale = 1.0 - size_ratio * 2.0 * PADDING_Y / height;
            for i in 1..grid_y {
                let y = (i as f32 * 2.0 / grid_y as f32 - 1.0) * y_scale;
                let y = (1.0 - y) / 2.0 * height;
                grid.push(Quad::new(
                    QuadKind::Color,
                    at(0.0, y - 0.5, width, 1.0),
                    lighten,
                ));
            }
            if grid_x > 0 && painting && state.active_grid_section >= 0 {
                let w = bounds.width;
                let section = state.active_grid_section;
                let start_x = (section * w) / grid_x as i32 + 1;
                let end_x = ((section + 1) * w) / grid_x as i32;
                grid.push(Quad::new(
                    QuadKind::Color,
                    at(start_x as f32, 0.0, (end_x - start_x) as f32, height),
                    lighten,
                ));
            }
            grid.paint(pixels, clip);

            // The line: the right channel's fill, the left's, the centre.
            let mut renderer = LineRenderer::new(TOTAL_POINTS, false);
            renderer.line_width = line_width;
            renderer.fill_center = fill_center;
            for (i, (x, y)) in g.wave_path().into_iter().enumerate() {
                renderer.set_x_at(i, x);
                renderer.set_y_at(i, y);
            }
            renderer.fill = true;
            for (colour, fill) in [(line_right, fill_right), (line_left, fill_left)] {
                renderer.color = colour;
                renderer.set_fill_colors(fill.with_multiplied_alpha(1.0 - fill_fade), fill);
                renderer.paint(pixels, clip, true);
            }
            renderer.fill = false;
            renderer.color = center;
            renderer.paint(pixels, clip, true);

            // renderPoints.
            let mut markers = QuadBatch::new();
            for i in 0..line.num_points() {
                let (x, y) = line.point(i);
                let mut ring = Quad::new(
                    QuadKind::Ring,
                    centred(
                        (width * x, g.pad_y(height * y)),
                        size_ratio * POSITION_WIDTH,
                    ),
                    center,
                );
                ring.alt_color = background;
                ring.thickness = size_ratio * POSITION_WIDTH * RING_THICKNESS * 0.5;
                markers.push(ring);
            }
            for i in 0..line.num_points() {
                if g.power_active(i) {
                    let (x, y) = g.power_position(i);
                    markers.push(Quad::new(
                        QuadKind::Circle,
                        centred((width * x, g.pad_y(height * y)), size_ratio * POWER_WIDTH),
                        center,
                    ));
                }
            }
            // setEditingCircleBounds.
            let edit = if state.active_point >= 0
                && (state.active_point as usize) < line.num_points()
            {
                Some(line.point(state.active_point as usize))
            } else if state.active_power >= 0 && (state.active_power as usize) < line.num_points() {
                Some(g.power_position(state.active_power as usize))
            } else {
                None
            };
            if let Some((x, y)) = edit {
                let point = (width * x, g.pad_y(height * y));
                if state.dragging {
                    markers.push(Quad::new(
                        QuadKind::Circle,
                        centred(point, size_ratio * DRAG_RADIUS * 2.0),
                        accent2,
                    ));
                }
                if !painting {
                    let mut hover = Quad::new(
                        QuadKind::Ring,
                        centred(point, size_ratio * GRAB_RADIUS * 2.0),
                        accent1,
                    );
                    hover.thickness = 1.0;
                    markers.push(hover);
                }
            }
            for corner in Quad::corners(clip, rounding, body) {
                markers.push(corner);
            }
            markers.paint(pixels, clip);
        });
    }
}

fn geometry(line: &LineGenerator, size: (f32, f32), size_ratio: f32) -> Geometry<'_> {
    Geometry {
        line,
        width: size.0,
        height: size.1,
        size_ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> LineGenerator {
        init_shape()
    }

    /// Level first and last points move together (`movePoint`).
    #[test]
    fn level_ends_move_together() {
        let mut line = triangle();
        let last = line.num_points() - 1;
        assert_eq!(line.point(0).1, line.point(last).1);
        move_point(&mut line, 0, (10.0, 50.0), (100.0, 100.0), (8, 1), true);
        assert_eq!(line.point(0), (0.0, 0.5));
        assert_eq!(line.point(last), (1.0, 0.5));
    }

    /// Painting the step pattern into the second of four cells replaces
    /// that cell with the pattern's points.
    #[test]
    fn paint_fills_one_cell() {
        let mut line = triangle();
        let section = paint_line(
            &mut line,
            (30.0, 0.0),
            (100.0, 100.0),
            (4, 1),
            paint_pattern(0),
            true,
        );
        assert_eq!(section, 1);
        let points: Vec<(f32, f32)> = (0..line.num_points()).map(|i| line.point(i)).collect();
        assert!(points.contains(&(0.25, 0.0)), "{points:?}");
        assert!(points.contains(&(0.5, 0.0)), "{points:?}");
        assert!(points.windows(2).all(|w| w[0].0 <= w[1].0), "{points:?}");
    }

    /// A power drag bends the segment and stops at the LFO's limit.
    #[test]
    fn powers_clamp() {
        let mut line = triangle();
        move_power(&mut line, 0, 10.0, false, false);
        assert_eq!(line.power(0).abs(), MAX_POWER);
    }
}
