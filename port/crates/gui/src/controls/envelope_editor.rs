// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `EnvelopeEditor`: the envelope's shape over its time ruler, with points
//! to drag. Ports `vital/src/interface/editor_components/envelope_editor.cpp`.
//!
//! The delay, attack, hold, sustain and release points move their times
//! (the sustain point also the decay time and sustain level); the dots
//! halfway along the attack, decay and release bend their powers, and a
//! double click on one straightens it. Dragging past the window's end
//! widens it.
//!
//! Not yet: the playing voices' positions and the animated line (from the
//! engine's telemetry), and zooming with the wheel or the magnifying glass.

use egui::{Pos2, Sense};
use sas_common::scalar::power_scale;
use sas_preset::ParamInfo;

use crate::controls::format::{self, Format};
use crate::controls::slider::{DOUBLE_CLICK_TIMEOUT, MULTIPLE_CLICK_DISTANCE};
use crate::fonts::Face;
use crate::host::Host;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::scope::Scope;
use crate::sections::Panel;
use crate::skin::{ColorId, ValueId};

const MARKER_WIDTH: f32 = 9.0;
const RING_THICKNESS: f32 = 0.45;
const POWER_MARKER_WIDTH: f32 = 7.0;
const MARKER_HOVER_RADIUS: f32 = 12.0;
const MARKER_GRAB_RADIUS: f32 = 20.0;
const PADDING_X: f32 = 0.018;
const PADDING_Y: f32 = 0.06;
const MIN_POINT_DISTANCE_FOR_POWER: f32 = 3.0;
const POWER_MOUSE_MULTIPLIER: f32 = 0.06;
const TIME_DISPLAY_SIZE: f32 = 0.05;
const TIME_DISPLAY_BUFFER: f32 = 0.025;
const TIME_DRAW_WIDTH: f32 = 0.1;
const RULER_DIVISION_SIZE: i32 = 4;
const MAX_TIMES_SHOWN: i32 = 24;
const NUM_POINTS_PER_SECTION: usize = 98;
const NUM_SECTIONS: usize = 4;
const TOTAL_POINTS: usize = NUM_SECTIONS * NUM_POINTS_PER_SECTION + 1;
const MIN_WINDOW_SIZE: f32 = 0.125;
const MAX_WINDOW_SIZE: f32 = 64.0;
const DEFAULT_WINDOW_TIME: f32 = 4.0;
/// Frames the editor keeps its own values after a release, until the host
/// has them.
const HOLD_FRAMES: u32 = 10;

/// An envelope's parameter names.
#[derive(Clone, Copy, Debug)]
pub struct EnvelopeNames {
    pub delay: &'static str,
    pub attack: &'static str,
    pub hold: &'static str,
    pub decay: &'static str,
    pub sustain: &'static str,
    pub release: &'static str,
    pub attack_power: &'static str,
    pub decay_power: &'static str,
    pub release_power: &'static str,
}

/// What the pointer is nearest (`pickHoverPosition`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hover {
    #[default]
    None,
    Delay,
    Attack,
    Hold,
    Sustain,
    Release,
    AttackPower,
    DecayPower,
    ReleasePower,
}

/// The envelope's raw parameter values.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Values {
    pub delay: f32,
    pub attack: f32,
    pub hold: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub attack_power: f32,
    pub decay_power: f32,
    pub release_power: f32,
}

/// The editor's geometry for some values, in pixels from its top left.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub width: f32,
    pub height: f32,
    pub window_time: f32,
    pub delay_x: f32,
    pub attack_x: f32,
    pub hold_x: f32,
    pub decay_x: f32,
    pub sustain_y: f32,
    pub release_x: f32,
    values: Values,
}

fn info(name: &str) -> &'static ParamInfo {
    sas_preset::lookup(name).unwrap_or_else(|| panic!("no parameter {name}"))
}

/// `SynthSlider::getAdjustedValue`.
fn adjusted(name: &str, value: f32) -> f32 {
    format::adjusted_value(info(name), &Format::default(), value as f64) as f32
}

/// `setValueFromAdjusted`, clamped to the range as `Slider::setValue` does.
fn from_adjusted(name: &str, adjusted: f32) -> f32 {
    let info = info(name);
    (format::value_from_adjusted(info, &Format::default(), adjusted as f64) as f32)
        .clamp(info.min, info.max)
}

/// `getEnvelopeValue`.
fn envelope_value(t: f32, power: f32, start: f32, end: f32) -> f32 {
    start + (end - start) * power_scale(t, power)
}

impl Shape {
    /// `getSliderDelayX` and the rest, from the slider values.
    pub fn new(
        names: &EnvelopeNames,
        values: Values,
        width: f32,
        height: f32,
        window_time: f32,
    ) -> Shape {
        let x = |name: &str, value: f32| width * adjusted(name, value) / window_time;
        let delay_x = x(names.delay, values.delay);
        let attack_x = delay_x + x(names.attack, values.attack);
        let hold_x = attack_x + x(names.hold, values.hold);
        let decay_x = hold_x + x(names.decay, values.decay);
        let sustain_info = info(names.sustain);
        let percent = (values.sustain - sustain_info.min) / (sustain_info.max - sustain_info.min);
        let sustain_y = height * (1.0 - percent);
        let release_x = decay_x + x(names.release, values.release);
        Shape {
            width,
            height,
            window_time,
            delay_x,
            attack_x,
            hold_x,
            decay_x,
            sustain_y,
            release_x,
            values,
        }
    }

    /// `getSliderAttackValue`.
    pub fn attack_value(&self, t: f32) -> f32 {
        self.height - envelope_value(1.0 - t, self.values.attack_power, self.height, 0.0)
    }

    /// `getSliderDecayValue`.
    pub fn decay_value(&self, t: f32) -> f32 {
        envelope_value(t, self.values.decay_power, 0.0, self.sustain_y)
    }

    /// `getSliderReleaseValue`.
    pub fn release_value(&self, t: f32) -> f32 {
        envelope_value(t, self.values.release_power, self.sustain_y, self.height)
    }

    /// `padX`: a point's x as drawn.
    pub fn pad_x(&self, x: f32) -> f32 {
        x * (1.0 - PADDING_X) + PADDING_X * self.width / 2.0
    }

    /// `padY`: a point's y as drawn.
    pub fn pad_y(&self, y: f32) -> f32 {
        y * (1.0 - PADDING_Y / 2.0) + PADDING_Y * self.height / 2.0
    }

    fn unpad_x(&self, x: f32) -> f32 {
        (x - PADDING_X * self.width / 2.0) / (1.0 - PADDING_X)
    }

    fn unpad_y(&self, y: f32) -> f32 {
        (y - PADDING_Y * self.height / 2.0) / (1.0 - PADDING_Y / 2.0)
    }

    /// `resetEnvelopeLine(-1)`: the line's points, padded.
    pub fn line(&self) -> Vec<(f32, f32)> {
        let interpolate = |from: f32, to: f32, t: f32| from + (to - from) * t;
        let mut points = Vec::with_capacity(TOTAL_POINTS);
        for i in 0..NUM_POINTS_PER_SECTION {
            let t = i as f32 / NUM_POINTS_PER_SECTION as f32;
            points.push((
                interpolate(self.delay_x, self.attack_x, t),
                self.attack_value(t),
            ));
        }
        for i in 0..NUM_POINTS_PER_SECTION {
            let t = i as f32 / NUM_POINTS_PER_SECTION as f32;
            points.push((interpolate(self.attack_x, self.hold_x, t), 0.0));
        }
        for i in 0..NUM_POINTS_PER_SECTION {
            let t = i as f32 / NUM_POINTS_PER_SECTION as f32;
            points.push((
                interpolate(self.hold_x, self.decay_x, t),
                self.decay_value(t),
            ));
        }
        for i in 0..=NUM_POINTS_PER_SECTION {
            let t = i as f32 / NUM_POINTS_PER_SECTION as f32;
            points.push((
                interpolate(self.decay_x, self.release_x, t),
                self.release_value(t),
            ));
        }
        points
            .into_iter()
            .map(|(x, y)| (self.pad_x(x), self.pad_y(y)))
            .collect()
    }

    /// `pickHoverPosition` at `position` (pixels from the editor's top left).
    pub fn hover_at(&self, position: (f32, f32)) -> Hover {
        let p = (self.unpad_x(position.0), self.unpad_y(position.1));
        let distance = |q: (f32, f32)| (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2);
        let delay = (self.delay_x, self.height);
        let attack_power = ((self.delay_x + self.attack_x) / 2.0, self.attack_value(0.5));
        let top = (self.attack_x, 0.0);
        let hold = (self.hold_x, 0.0);
        let decay_power = ((self.hold_x + self.decay_x) / 2.0, self.decay_value(0.5));
        let sustain = (self.decay_x, self.sustain_y);
        let release_power = (
            (self.decay_x + self.release_x) / 2.0,
            self.release_value(0.5),
        );
        let release = (self.release_x, self.height);

        let mut points = vec![top, sustain, release];
        if self.delay_x > 0.0 {
            points.push(delay);
        }
        if self.hold_x > self.attack_x {
            points.push(hold);
        }
        if self.release_x - self.decay_x > MIN_POINT_DISTANCE_FOR_POWER
            && self.sustain_y < self.height
        {
            points.push(release_power);
        }
        if self.decay_x - self.attack_x > MIN_POINT_DISTANCE_FOR_POWER && self.sustain_y > 0.0 {
            points.push(decay_power);
        }
        if self.attack_x - self.delay_x > MIN_POINT_DISTANCE_FOR_POWER {
            points.push(attack_power);
        }
        let mut closest = self.height * self.height;
        for &point in &points {
            closest = closest.min(distance(point));
        }
        // mouseDrag checks them in this order.
        if distance(delay) == closest {
            Hover::Delay
        } else if distance(release) <= closest {
            Hover::Release
        } else if distance(sustain) <= closest {
            Hover::Sustain
        } else if distance(top) <= closest {
            Hover::Attack
        } else if self.hold_x > self.attack_x && distance(hold) == closest {
            Hover::Hold
        } else if distance(attack_power) == closest {
            Hover::AttackPower
        } else if distance(decay_power) == closest {
            Hover::DecayPower
        } else if distance(release_power) == closest {
            Hover::ReleasePower
        } else {
            Hover::None
        }
    }

    /// Where `hover`'s marker sits, in pixels (`setEditingCircleBounds`'s
    /// grab point).
    fn grab_point(&self, hover: Hover) -> Option<(f32, f32)> {
        let x = |x: f32| self.pad_x(x);
        let y = |y: f32| self.pad_y(y);
        Some(match hover {
            Hover::None => return None,
            Hover::Delay => (x(self.delay_x), self.height),
            Hover::Release => (x(self.release_x), self.height),
            Hover::Sustain => (x(self.decay_x), y(self.sustain_y)),
            Hover::Attack => (x(self.attack_x), y(0.0)),
            Hover::Hold => (x(self.hold_x), y(0.0)),
            Hover::AttackPower => (
                (x(self.delay_x) + x(self.attack_x)) / 2.0,
                y(self.attack_value(0.5)),
            ),
            Hover::DecayPower => (
                (x(self.hold_x) + x(self.decay_x)) / 2.0,
                y(self.decay_value(0.5)),
            ),
            Hover::ReleasePower => (
                (x(self.decay_x) + x(self.release_x)) / 2.0,
                y(self.release_value(0.5)),
            ),
        })
    }

    /// `setTimePositions` / `setGridPositions`: the ruler's chunk sizes and
    /// how far into the next division the window is.
    fn ruler(&self) -> (f32, f32, f32) {
        let powers = self.window_time.ln() / (RULER_DIVISION_SIZE as f32).ln();
        let current_division = powers.floor();
        let transition = powers - current_division;
        let big_time_chunk = (RULER_DIVISION_SIZE as f32).powf(current_division) / 2.0;
        (
            big_time_chunk,
            big_time_chunk / RULER_DIVISION_SIZE as f32,
            transition,
        )
    }
}

/// `formatTime`.
pub fn format_time(time: f32) -> String {
    if time < 1.0 {
        return format!("{}ms", (time * 1000.0) as i32);
    }
    // JUCE's String(float) writes whole numbers without a decimal point.
    let seconds = (time * 10.0) as i32 as f32 / 10.0;
    format!("{seconds}s")
}

#[derive(Clone, Copy, Debug)]
struct EditorState {
    window_time: f32,
    hover: Hover,
    mouse_down: bool,
    /// A drag's host gestures are open (from its first move).
    gesture: bool,
    last: Pos2,
    /// The last press's time and place, for JUCE's double-click rules.
    last_press: Option<(f64, Pos2)>,
    values: Values,
    holding: u32,
}

impl Default for EditorState {
    fn default() -> EditorState {
        EditorState {
            window_time: DEFAULT_WINDOW_TIME,
            hover: Hover::None,
            mouse_down: false,
            gesture: false,
            last: Pos2::ZERO,
            last_press: None,
            values: Values::default(),
            holding: 0,
        }
    }
}

/// An envelope's editor.
#[derive(Clone, Copy, Debug)]
pub struct EnvelopeEditor {
    pub names: EnvelopeNames,
}

impl EnvelopeEditor {
    fn host_values(&self, host: &Host) -> Values {
        let n = &self.names;
        Values {
            delay: host.value(n.delay),
            attack: host.value(n.attack),
            hold: host.value(n.hold),
            decay: host.value(n.decay),
            sustain: host.value(n.sustain),
            release: host.value(n.release),
            attack_power: host.value(n.attack_power),
            decay_power: host.value(n.decay_power),
            release_power: host.value(n.release_power),
        }
    }

    /// The parameters a drag on `hover` changes.
    fn targets(&self, hover: Hover) -> &'static [fn(&EnvelopeNames) -> &'static str] {
        match hover {
            Hover::None => &[],
            Hover::Delay => &[|n| n.delay],
            Hover::Attack => &[|n| n.attack],
            Hover::Hold => &[|n| n.hold],
            Hover::Sustain => &[|n| n.decay, |n| n.sustain],
            Hover::Release => &[|n| n.release],
            Hover::AttackPower => &[|n| n.attack_power],
            Hover::DecayPower => &[|n| n.decay_power],
            Hover::ReleasePower => &[|n| n.release_power],
        }
    }

    /// Handles the mouse over the editor at `bounds` and draws it.
    pub fn show(&self, panel: &mut Panel, bounds: Rect) {
        let host = panel.host;
        let names = self.names;
        let id = panel.ui.scope_id().with(("envelope editor", names.delay));
        let rect = panel.pixels.rect(bounds.into());
        let response = panel.ui.interact(rect, id, Sense::click_and_drag());
        let mut state: EditorState = panel.ui.data(|data| data.get_temp(id)).unwrap_or_default();
        if !state.mouse_down {
            if state.holding > 0 {
                state.holding -= 1;
            } else {
                state.values = self.host_values(host);
            }
        }
        let (width, height) = (bounds.width as f32, bounds.height as f32);
        let ppp = panel.pixels.pixels_per_point();
        let origin = panel.pixels.area().min;
        let local = |pos: Pos2| {
            (
                (pos.x - origin.x) * ppp - bounds.x as f32,
                (pos.y - origin.y) * ppp - bounds.y as f32,
            )
        };
        let pointer = panel.ui.input(|input| input.pointer.interact_pos());
        let down = response.is_pointer_button_down_on();

        let shape = Shape::new(&names, state.values, width, height, state.window_time);
        if !state.mouse_down {
            state.hover = match pointer {
                Some(pos) if response.hovered() => shape.hover_at(local(pos)),
                _ => Hover::None,
            };
        }
        if down && !state.mouse_down {
            // mouseDown, and mouseDoubleClick on a second press within
            // JUCE's limits.
            let now = panel.ui.input(|input| input.time);
            let pos = pointer.unwrap_or_default();
            let double_click = state.last_press.is_some_and(|(time, at)| {
                now - time < DOUBLE_CLICK_TIMEOUT
                    && (pos.x - at.x).abs() < MULTIPLE_CLICK_DISTANCE
                    && (pos.y - at.y).abs() < MULTIPLE_CLICK_DISTANCE
            });
            state.mouse_down = true;
            state.last = pos;
            state.last_press = Some((now, pos));
            if double_click {
                self.straighten_power(host, &mut state);
            }
        } else if down {
            if let Some(pos) = pointer {
                if pos != state.last {
                    // The gestures open with the first move, so a click
                    // sends none.
                    if !state.gesture {
                        state.gesture = true;
                        for target in self.targets(state.hover) {
                            host.begin(target(&names));
                        }
                    }
                    self.drag(host, &mut state, pos, local(pos), (width, height));
                }
            }
        } else if state.mouse_down {
            state.mouse_down = false;
            state.holding = HOLD_FRAMES;
            if state.gesture {
                state.gesture = false;
                for target in self.targets(state.hover) {
                    host.end(target(&names));
                }
            }
        }
        panel.ui.data_mut(|data| data.insert_temp(id, state));

        let shape = Shape::new(&names, state.values, width, height, state.window_time);
        self.paint(panel, bounds, &shape, &state);
    }

    /// `mouseDoubleClick`: a power dot's power back to 0, as one gesture of
    /// its own (no drag gesture is open on a second press).
    fn straighten_power(&self, host: &Host, state: &mut EditorState) {
        let names = self.names;
        let (name, value) = match state.hover {
            Hover::AttackPower => (names.attack_power, &mut state.values.attack_power),
            Hover::DecayPower => (names.decay_power, &mut state.values.decay_power),
            Hover::ReleasePower => (names.release_power, &mut state.values.release_power),
            _ => return,
        };
        host.set_once(name, 0.0);
        *value = 0.0;
        state.holding = HOLD_FRAMES;
    }

    /// `mouseDrag`: positions are the pointer's, unpadded as upstream's.
    fn drag(
        &self,
        host: &Host,
        state: &mut EditorState,
        pos: Pos2,
        local: (f32, f32),
        size: (f32, f32),
    ) {
        let names = self.names;
        let delta_power = (state.last.y - pos.y) * POWER_MOUSE_MULTIPLIER;
        state.last = pos;
        let (x, y) = local;
        let shape = Shape::new(&names, state.values, size.0, size.1, state.window_time);
        let window_time = state.window_time;
        let time_from = |from_x: f32| (x - from_x) * window_time / size.0;
        let set = |name: &'static str, value: f32| {
            host.set(name, value);
            value
        };
        let power = |value: f32, name: &str| {
            let info = info(name);
            (value + delta_power).clamp(info.min, info.max)
        };
        let values = &mut state.values;
        match state.hover {
            Hover::Delay => {
                values.delay = set(names.delay, from_adjusted(names.delay, time_from(0.0)))
            }
            Hover::Release => {
                values.release = set(
                    names.release,
                    from_adjusted(names.release, time_from(shape.decay_x)),
                );
                state.window_time = grow_window(window_time, x, size.0);
            }
            Hover::Sustain => {
                values.decay = set(
                    names.decay,
                    from_adjusted(names.decay, time_from(shape.hold_x)),
                );
                state.window_time = grow_window(window_time, x, size.0);
                let sustain = info(names.sustain);
                let percent = (1.0 - y / size.1).clamp(0.0, 1.0);
                values.sustain = set(
                    names.sustain,
                    sustain.min + percent * (sustain.max - sustain.min),
                );
            }
            Hover::Attack => {
                values.attack = set(
                    names.attack,
                    from_adjusted(names.attack, time_from(shape.delay_x)),
                )
            }
            Hover::Hold => {
                values.hold = set(
                    names.hold,
                    from_adjusted(names.hold, time_from(shape.attack_x)),
                )
            }
            Hover::AttackPower => {
                values.attack_power = set(
                    names.attack_power,
                    power(values.attack_power, names.attack_power),
                )
            }
            Hover::DecayPower => {
                values.decay_power = set(
                    names.decay_power,
                    power(values.decay_power, names.decay_power),
                )
            }
            Hover::ReleasePower => {
                values.release_power = set(
                    names.release_power,
                    power(values.release_power, names.release_power),
                )
            }
            Hover::None => {}
        }
    }

    /// `render` at rest: the time labels, the grid, the line (right, left
    /// and centre passes), the point rings and power dots, the hover and
    /// drag circles, and the corners.
    fn paint(&self, panel: &mut Panel, bounds: Rect, shape: &Shape, state: &EditorState) {
        let scope: Scope = panel.scope;
        let size_ratio = scope.size_ratio();
        let background = scope.color(ColorId::WidgetBackground);
        let lighten = scope.color(ColorId::LightenScreen);
        let line_left = scope.color(ColorId::WidgetPrimary1);
        let line_right = scope.color(ColorId::WidgetPrimary2);
        let line_center = scope.color(ColorId::WidgetCenterLine);
        let fill_fade = scope.value(ValueId::WidgetFillFade);
        let fill_left = scope.color(ColorId::WidgetSecondary1);
        let fill_right = scope.color(ColorId::WidgetSecondary2);
        let accent1 = scope.color(ColorId::WidgetAccent1);
        let accent2 = scope.color(ColorId::WidgetAccent2);
        let line_width = scope.value(ValueId::WidgetLineWidth);
        let fill_center = scope.value(ValueId::WidgetFillCenter);
        let rounding = scope.value(ValueId::WidgetRoundedCorner);
        let body = scope.color(ColorId::Body);
        let shape = *shape;
        let state = *state;
        let origin = (bounds.x as f32, bounds.y as f32);
        let (width, height) = (shape.width, shape.height);
        let at = move |x: f32, y: f32, w: f32, h: f32| RectF::new(origin.0 + x, origin.1 + y, w, h);
        let centred = move |(x, y): (f32, f32), diameter: f32| {
            at(x - diameter / 2.0, y - diameter / 2.0, diameter, diameter)
        };

        panel.layer().custom(move |pixels: &PixelPainter| {
            let clip = RectF::from(bounds);
            let (big_chunk, little_chunk, transition) = shape.ruler();
            let t = 1.0 - transition;

            // setTimePositions.
            let font_height = TIME_DISPLAY_SIZE * height;
            let font_buffer = TIME_DISPLAY_BUFFER * height;
            let font_draw_height = font_height + font_buffer;
            let font_y = height - font_draw_height;
            let font_draw_width = width * TIME_DRAW_WIDTH;
            let big_colour = background.overlaid_with(lighten);
            let little_colour = background.overlaid_with(lighten.with_multiplied_alpha(t * t));
            let mut index = 1;
            while (index as f32) * little_chunk < shape.window_time && index < MAX_TIMES_SHOWN {
                let colour = if index % RULER_DIVISION_SIZE != 0 {
                    little_colour
                } else {
                    big_colour
                };
                let time = index as f32 * little_chunk;
                let x = shape.pad_x(width * time / shape.window_time) as i32;
                let label = Rect::new(
                    (x as f32 + font_buffer) as i32,
                    font_y as i32,
                    font_draw_width as i32,
                    font_draw_height as i32,
                )
                .translated(bounds.x, bounds.y);
                pixels.with_clip(clip, |pixels| {
                    pixels.text_left(
                        &format_time(time),
                        label,
                        Face::Regular,
                        font_height,
                        colour,
                    )
                });
                index += 1;
            }

            // setGridPositions.
            let gl_x = |time: f32| {
                let x = (2.0 * time / shape.window_time - 1.0) * (1.0 - PADDING_X);
                (x + 1.0) / 2.0 * width
            };
            let mut grid = QuadBatch::new();
            let sub_colour = lighten.with_multiplied_alpha(t * t);
            let mut i = 1;
            while (i as f32) * little_chunk < shape.window_time {
                if i % RULER_DIVISION_SIZE != 0 {
                    let x = gl_x(i as f32 * little_chunk);
                    grid.push(Quad::new(
                        QuadKind::Color,
                        at(x, 0.0, 1.0, height),
                        sub_colour,
                    ));
                }
                i += 1;
            }
            let mut i = 1;
            while (i as f32) * big_chunk < shape.window_time {
                let x = gl_x(i as f32 * big_chunk);
                grid.push(Quad::new(QuadKind::Color, at(x, 0.0, 1.0, height), lighten));
                i += 1;
            }
            grid.paint(pixels, clip);

            // The line: the right channel's fill, the left's, then the
            // centre line.
            let points = shape.line();
            let mut line = LineRenderer::new(TOTAL_POINTS, false);
            line.line_width = line_width;
            line.fill_center = fill_center;
            line.enable_backward_boost = false;
            for (i, (x, y)) in points.into_iter().enumerate() {
                line.set_x_at(i, x);
                line.set_y_at(i, y);
            }
            line.fill = true;
            for (colour, fill) in [(line_right, fill_right), (line_left, fill_left)] {
                line.color = colour;
                line.set_fill_colors(fill.with_multiplied_alpha(1.0 - fill_fade), fill);
                line.paint(pixels, clip, true);
            }
            line.fill = false;
            line.color = line_center;
            line.paint(pixels, clip, true);

            // setPointPositions.
            let mut markers = QuadBatch::new();
            let marker = size_ratio * MARKER_WIDTH;
            let top = shape.pad_y(0.0);
            let mut ring = |point: (f32, f32)| {
                let mut quad = Quad::new(QuadKind::Ring, centred(point, marker), line_center);
                quad.alt_color = background;
                quad.thickness = size_ratio * MARKER_WIDTH * 0.5 * RING_THICKNESS;
                markers.push(quad);
            };
            ring((shape.pad_x(shape.attack_x), top));
            if shape.hold_x != shape.attack_x {
                ring((shape.pad_x(shape.hold_x), top));
            }
            ring((shape.pad_x(shape.decay_x), shape.pad_y(shape.sustain_y)));
            ring((shape.pad_x(shape.release_x), height));

            let power = size_ratio * POWER_MARKER_WIDTH;
            let mut dot = |point: (f32, f32)| {
                markers.push(Quad::new(
                    QuadKind::Circle,
                    centred(point, power),
                    line_center,
                ));
            };
            let (delay_x, attack_x) = (shape.pad_x(shape.delay_x), shape.pad_x(shape.attack_x));
            let (hold_x, decay_x) = (shape.pad_x(shape.hold_x), shape.pad_x(shape.decay_x));
            let release_x = shape.pad_x(shape.release_x);
            // Upstream compares the padded positions with the unpadded distance.
            let min_distance = MIN_POINT_DISTANCE_FOR_POWER;
            if attack_x - delay_x > min_distance {
                dot((
                    (delay_x + attack_x) * 0.5,
                    shape.pad_y(shape.attack_value(0.5)),
                ));
            }
            let sustain_y = shape.pad_y(shape.sustain_y);
            if decay_x - hold_x > min_distance && sustain_y > top {
                dot((
                    (hold_x + decay_x) * 0.5,
                    shape.pad_y(shape.decay_value(0.5)),
                ));
            }
            if release_x - decay_x > min_distance && sustain_y < height {
                dot((
                    (decay_x + release_x) * 0.5,
                    shape.pad_y(shape.release_value(0.5)),
                ));
            }

            // setEditingCircleBounds.
            if let Some(point) = shape.grab_point(state.hover) {
                if state.mouse_down {
                    let grab = MARKER_GRAB_RADIUS * size_ratio * 2.0;
                    markers.push(Quad::new(QuadKind::Circle, centred(point, grab), accent2));
                }
                let hover = MARKER_HOVER_RADIUS * size_ratio * 2.0;
                let mut quad = Quad::new(QuadKind::Ring, centred(point, hover), accent1);
                quad.thickness = 1.0;
                markers.push(quad);
            }
            for corner in Quad::corners(clip, rounding, body) {
                markers.push(corner);
            }
            markers.paint(pixels, clip);
        });
    }
}

/// `setDecayX` / `setReleaseX`: the window widens to keep the point in it.
fn grow_window(window_time: f32, x: f32, width: f32) -> f32 {
    window_time
        .max(x * window_time / width)
        .clamp(MIN_WINDOW_SIZE, MAX_WINDOW_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::Colour;

    const NAMES: EnvelopeNames = EnvelopeNames {
        delay: "env_1_delay",
        attack: "env_1_attack",
        hold: "env_1_hold",
        decay: "env_1_decay",
        sustain: "env_1_sustain",
        release: "env_1_release",
        attack_power: "env_1_attack_power",
        decay_power: "env_1_decay_power",
        release_power: "env_1_release_power",
    };

    #[test]
    fn times_read_like_upstream() {
        assert_eq!(format_time(0.5), "500ms");
        assert_eq!(format_time(1.0), "1s");
        assert_eq!(format_time(1.5), "1.5s");
        assert_eq!(format_time(2.25), "2.2s");
    }

    #[test]
    fn overlaid_colours_composite_in_juce_steps() {
        let base = Colour(0xff10_2030);
        // JUCE's `>> 8` takes each channel down a step even under a
        // transparent colour.
        assert_eq!(base.overlaid_with(Colour(0x0000_0000)), Colour(0xff0f_1f2f));
        assert_eq!(base.overlaid_with(Colour(0xffff_ffff)), Colour(0xffff_ffff));
    }

    /// A one-second attack in a four-second window reaches a quarter of
    /// the way across; the hover picks the nearest point.
    #[test]
    fn points_sit_at_their_times() {
        let values = Values {
            attack: from_adjusted(NAMES.attack, 1.0),
            decay: from_adjusted(NAMES.decay, 1.0),
            sustain: 0.5,
            release: from_adjusted(NAMES.release, 1.0),
            ..Values::default()
        };
        let shape = Shape::new(&NAMES, values, 400.0, 100.0, 4.0);
        assert!((shape.attack_x - 100.0).abs() < 0.1);
        assert!((shape.decay_x - 200.0).abs() < 0.1);
        assert!((shape.release_x - 300.0).abs() < 0.1);
        assert_eq!(shape.sustain_y, 50.0);
        assert_eq!(
            shape.hover_at((shape.pad_x(100.0), shape.pad_y(0.0))),
            Hover::Attack
        );
        assert_eq!(
            shape.hover_at((shape.pad_x(200.0), shape.pad_y(50.0))),
            Hover::Sustain
        );
        assert_eq!(
            shape.hover_at((shape.pad_x(300.0), shape.pad_y(100.0))),
            Hover::Release
        );
        assert_eq!(shape.line().len(), TOTAL_POINTS);
    }
}
