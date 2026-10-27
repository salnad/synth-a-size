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

use egui::{PointerButton, Pos2, Sense};
use sas_common::random::RandomGenerator;
use sas_engine::sas_oscillators::sas_wavetable::wave_frame::{
    WaveFrame, NUM_REAL_COMPLEX, WAVEFORM_SIZE,
};
use sas_engine::sas_oscillators::sas_wavetable::Complex;

use super::wavetable_edit::{to_pixels, EditMenu, KeyframeRef, WavetableEditLayout, GL_TIE_NUDGE};
use super::{MenuOwner, Panel};
use crate::colour::Colour;
use crate::controls::bars::BarRenderer;
use crate::controls::slider::{Slider, SliderStyle};
use crate::fonts::Face;
use crate::host::Host;
use crate::layout::Rect;
use crate::local::{WaveSourceControls, WAVE_SOURCE_INTERPOLATIONS};
use crate::paint::RectF;
use crate::popup::{Menu, MenuItem};
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
        let (strip, padding) = edit_strip(scope, edit);

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

/// `WavetableEditSection::setOverlayPosition`'s `edit_bounds_`, the strip
/// below the waveform an overlay's controls sit in, and the overlay's
/// padding (an int: `setPadding(getPadding())`).
pub fn edit_strip(scope: &Scope, edit: &WavetableEditLayout) -> (Rect, i32) {
    let bounds = edit.bounds;
    let padding = scope.padding() as i32;
    let edit_height = (edit.amplitudes.height as f32 * 0.33) as i32;
    let edit_y = ((edit.waveform.bottom() - bounds.y) as f32 + scope.padding()) as i32;
    let strip = Rect::new(bounds.x, bounds.y + edit_y, bounds.width, edit_height);
    (strip, padding)
}

/// The controls background's title rects for divider `lines` (x in the
/// background) in `controls`, `title_height` high (`setPositions`).
pub fn title_bounds(controls: Rect, lines: &[i32], title_height: i32) -> Vec<Rect> {
    (0..=lines.len())
        .map(|i| {
            let start = if i > 0 { lines[i - 1] } else { 0 };
            let end = lines.get(i).copied().unwrap_or(controls.width);
            Rect::new(controls.x + start, controls.y, end - start, title_height)
        })
        .collect()
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
    // The background's own bounds clip its quads, so a line at its right
    // edge (the Line Source's last) doesn't show.
    for &line in lines.iter().filter(|&&line| line < controls.width) {
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

/// The waveform editor's menu (`WaveSourceEditor::WaveSourceMenu`).
pub const WAVEFORM_FLIP_HORIZONTAL: i32 = 1;
pub const WAVEFORM_FLIP_VERTICAL: i32 = 2;
pub const WAVEFORM_CLEAR: i32 = 3;
/// The bar editors' menu (`BarEditor::BarEditorMenu`).
pub const BARS_CLEAR: i32 = 1;
pub const BARS_CLEAR_RIGHT: i32 = 2;
pub const BARS_CLEAR_LEFT: i32 = 3;
pub const BARS_CLEAR_EVEN: i32 = 4;
pub const BARS_CLEAR_ODD: i32 = 5;
pub const BARS_RANDOMIZE: i32 = 6;

/// One of the overlay's three editors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveSourcePart {
    /// `oscillator_`, the `WaveSourceEditor` over the waveform.
    Waveform,
    /// `frequency_amplitudes_` and `frequency_phases_`, the `BarEditor`s.
    Amplitudes,
    Phases,
}

/// What an edit asks of the editor section (`notifyChanged`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameEdit {
    /// `frameChanged`, during a drag: the frame under the playhead renders.
    Changed,
    /// `frameDoneEditing`: the whole table renders, and the edit commits.
    Done,
}

impl FrameEdit {
    fn of(mouse_up: bool) -> FrameEdit {
        if mouse_up {
            FrameEdit::Done
        } else {
            FrameEdit::Changed
        }
    }
}

/// A press on one of the editors, while it lasts.
#[derive(Clone, Copy, Debug, Default)]
struct Press {
    down: bool,
    /// Pressed with the popup-menu button.
    popup: bool,
}

/// One Wave Source overlay between frames: its controls, its editors'
/// mouse states and its bar editors' bars. Upstream keeps the bars between
/// edits rather than reading them from the frame each time: they load when
/// a keyframe is selected or the waveform is drawn, and a bar edit leaves
/// them as drawn while the frame normalises.
pub struct WaveSourceOverlay {
    pub sliders: WaveSourceSliders,
    /// The interpolation selector's value as last applied.
    interpolation_seen: f32,
    amplitudes: BarRenderer,
    phases: BarRenderer,
    /// The selection and keyframe the bars were loaded for.
    loaded: Option<(u64, KeyframeRef)>,
    /// `WaveSourceEditor`'s press, `editing_`, `last_edit_position_` and
    /// `current_mouse_position_` (unset until the pointer first moves over
    /// it, so the hover circle starts hidden).
    waveform_press: Press,
    editing: bool,
    last_edit: (i32, i32),
    mouse: Option<(i32, i32)>,
    /// Each `BarEditor`'s press, `last_edit_position_` and
    /// `current_mouse_position_`.
    bar_press: [Press; 2],
    bar_last_edit: [(i32, i32); 2],
    bar_mouse: [(i32, i32); 2],
    /// Each `BarEditor`'s `random_generator_`, seeded when first used.
    random: [Option<RandomGenerator>; 2],
}

/// `BarEditor`'s `current_mouse_position_` away from it.
const BAR_MOUSE_AWAY: (i32, i32) = (-10, -10);

impl WaveSourceOverlay {
    pub fn new(controls: WaveSourceControls) -> WaveSourceOverlay {
        let mut amplitudes = BarRenderer::new(NUM_REAL_COMPLEX, true);
        amplitudes.square_scale = true;
        WaveSourceOverlay {
            sliders: WaveSourceSliders::new(controls),
            interpolation_seen: 0.0,
            amplitudes,
            phases: BarRenderer::new(NUM_REAL_COMPLEX, true),
            loaded: None,
            waveform_press: Press::default(),
            editing: false,
            last_edit: (0, 0),
            mouse: None,
            bar_press: [Press::default(); 2],
            bar_last_edit: [(0, 0); 2],
            bar_mouse: [BAR_MOUSE_AWAY; 2],
            random: [None, None],
        }
    }

    /// The interpolation selector's value, when it changed since last asked
    /// (`sliderValueChanged`).
    pub fn interpolation_changed(&mut self, host: &Host) -> Option<i32> {
        let value = host.value(self.sliders.interpolation.name);
        if value == self.interpolation_seen {
            return None;
        }
        self.interpolation_seen = value;
        Some(value as i32)
    }

    /// Whether a press on one of the editors is under way.
    pub fn pressed(&self) -> bool {
        self.waveform_press.down || self.bar_press.iter().any(|press| press.down)
    }

    /// `frameSelected` for keyframe `keyframe` of selection `selection`,
    /// `setPowerScale` and `setFrequencyZoom`: the bars load from `frame`
    /// when the selection changed since they last did.
    pub fn sync(
        &mut self,
        selection: u64,
        keyframe: KeyframeRef,
        frame: &WaveFrame,
        power_scale: bool,
        zoom: f32,
    ) {
        if self.amplitudes.power_scale != power_scale {
            // BarRenderer::setPowerScale keeps each bar's value.
            let values: Vec<f32> = (0..NUM_REAL_COMPLEX)
                .map(|i| self.amplitudes.scaled_y_at(i))
                .collect();
            self.amplitudes.power_scale = power_scale;
            for (i, value) in values.into_iter().enumerate() {
                self.amplitudes.set_scaled_y(i, value);
            }
        }
        self.amplitudes.scale = zoom;
        self.phases.scale = zoom;
        if self.loaded != Some((selection, keyframe)) {
            self.loaded = Some((selection, keyframe));
            self.update_frequency_domain(frame);
        }
    }

    /// `updateFrequencyDomain`: the bars from the frame's harmonics.
    fn update_frequency_domain(&mut self, frame: &WaveFrame) {
        for (i, (amplitude, phase)) in harmonics(frame).into_iter().enumerate() {
            self.amplitudes.set_scaled_y(i, amplitude);
            self.phases.set_y(i, phase);
        }
    }

    /// `loadFrequencyDomain`: the frame from the bars, normalised.
    fn load_frequency_domain(&self, frame: &mut WaveFrame) {
        for i in 0..NUM_REAL_COMPLEX {
            let amplitude = self.amplitudes.scaled_y_at(i) * WAVEFORM_SIZE as f32;
            let phase = std::f32::consts::PI * self.phases.y_at(i);
            // std::polar.
            frame.frequency_domain[i] =
                Complex::new(amplitude * phase.cos(), amplitude * phase.sin());
        }
        frame.to_time_domain();
        frame.normalize(false);
        frame.to_frequency_domain();
    }

    /// `valuesChanged`: the frame's samples were drawn; its spectrum and the
    /// bars follow.
    fn values_changed(&mut self, frame: &mut WaveFrame, mouse_up: bool) -> FrameEdit {
        frame.to_frequency_domain();
        self.update_frequency_domain(frame);
        FrameEdit::of(mouse_up)
    }

    /// `barsChanged`: the frame from the bars.
    fn bars_changed(&mut self, frame: &mut WaveFrame, mouse_up: bool) -> FrameEdit {
        self.load_frequency_domain(frame);
        FrameEdit::of(mouse_up)
    }

    /// The editors' mice over `edit`'s displays, drawing on `frame`
    /// (`mouseDown`, `mouseDrag`, `mouseUp`, `mouseMove`, `mouseExit`). A
    /// popup-menu press opens the editor's menu for `owner`. The controls
    /// background at `controls` lies over the top of the amplitudes and
    /// takes the presses and the pointer there, as upstream's component on
    /// top of the bar editor does.
    pub fn handle_mouse(
        &mut self,
        panel: &mut Panel,
        edit: &WavetableEditLayout,
        controls: Rect,
        frame: &mut WaveFrame,
        owner: (usize, usize),
    ) -> Option<FrameEdit> {
        let grid = (
            panel.host.value(self.sliders.grid_x.name) as i32,
            panel.host.value(self.sliders.grid_y.name) as i32,
        );
        let mut result = None;
        let mut note = |edit: Option<FrameEdit>| {
            if let Some(edit) = edit {
                result = Some(match result {
                    Some(FrameEdit::Done) => FrameEdit::Done,
                    _ => edit,
                });
            }
        };
        let waveform = self.waveform_mouse(panel, edit.waveform, frame, grid, owner);
        note(waveform);
        for (bar, bounds) in [edit.amplitudes, edit.phases].into_iter().enumerate() {
            let bars = self.bar_mouse(panel, bounds, bar, controls, frame, owner);
            note(bars);
        }
        result
    }

    fn waveform_mouse(
        &mut self,
        panel: &mut Panel,
        bounds: Rect,
        frame: &mut WaveFrame,
        grid: (i32, i32),
        owner: (usize, usize),
    ) -> Option<FrameEdit> {
        let input = editor_input(panel, bounds, ("wave source editor", owner));
        let (width, height) = (bounds.width, bounds.height);
        let editor = WaveformGeometry {
            width,
            height,
            grid,
        };
        if !self.waveform_press.down {
            if input.pressed {
                let pos = input.pos?;
                self.mouse = Some(pos);
                self.waveform_press = Press {
                    down: true,
                    popup: input.popup,
                };
                if input.popup {
                    open_menu(panel, bounds, pos, owner, WaveSourcePart::Waveform);
                    return None;
                }
                self.last_edit = editor.snapped_point(pos);
                let range = editor.change_values(pos, self.last_edit, frame);
                self.editing = true;
                return range.map(|_| self.values_changed(frame, false));
            }
            match input.pos {
                Some(pos) if input.hovered => self.mouse = Some(pos),
                // mouseExit.
                _ if self.mouse.is_some() => self.mouse = Some((-width, 0)),
                _ => {}
            }
            return None;
        }
        let pos = input.pos.unwrap_or(self.mouse.unwrap_or((0, 0)));
        if input.held {
            if self.waveform_press.popup || Some(pos) == self.mouse {
                return None;
            }
            // mouseDrag.
            self.mouse = Some(pos);
            let range = editor.change_values(pos, self.last_edit, frame);
            self.last_edit = editor.snap_to_grid(pos, self.last_edit);
            return range.map(|_| self.values_changed(frame, false));
        }
        // mouseUp, after a popup-menu press too.
        self.waveform_press = Press::default();
        self.editing = false;
        self.mouse = Some(pos);
        Some(self.values_changed(frame, true))
    }

    fn bar_mouse(
        &mut self,
        panel: &mut Panel,
        bounds: Rect,
        bar: usize,
        controls: Rect,
        frame: &mut WaveFrame,
        owner: (usize, usize),
    ) -> Option<FrameEdit> {
        let mut input = editor_input(panel, bounds, ("wave source bars", owner, bar));
        let over_controls = input.pos.is_some_and(|(x, y)| {
            let (x, y) = (bounds.x + x, bounds.y + y);
            x >= controls.x && x < controls.right() && y >= controls.y && y < controls.bottom()
        });
        if over_controls && !self.bar_press[bar].down {
            input.pressed = false;
            input.hovered = false;
        }
        let part = if bar == 0 {
            WaveSourcePart::Amplitudes
        } else {
            WaveSourcePart::Phases
        };
        if !self.bar_press[bar].down {
            if input.pressed {
                let pos = input.pos?;
                self.bar_mouse[bar] = pos;
                self.bar_last_edit[bar] = pos;
                self.bar_press[bar] = Press {
                    down: true,
                    popup: input.popup,
                };
                if input.popup {
                    open_menu(panel, bounds, pos, owner, part);
                    return None;
                }
                self.change_bars(bar, bounds, pos);
                return Some(self.bars_changed(frame, false));
            }
            match input.pos {
                Some(pos) if input.hovered => self.bar_mouse[bar] = pos,
                _ => self.bar_mouse[bar] = BAR_MOUSE_AWAY,
            }
            return None;
        }
        let pos = input.pos.unwrap_or(self.bar_mouse[bar]);
        let popup = self.bar_press[bar].popup;
        if input.held {
            if popup || pos == self.bar_mouse[bar] {
                return None;
            }
            self.bar_mouse[bar] = pos;
            self.change_bars(bar, bounds, pos);
            self.bar_last_edit[bar] = pos;
            return Some(self.bars_changed(frame, false));
        }
        self.bar_press[bar] = Press::default();
        self.bar_mouse[bar] = pos;
        (!popup).then(|| self.bars_changed(frame, true))
    }

    fn bars(&mut self, bar: usize) -> &mut BarRenderer {
        if bar == 0 {
            &mut self.amplitudes
        } else {
            &mut self.phases
        }
    }

    /// `BarEditor::changeValues`: the bars from the last edit's position to
    /// `pos` along the line between them.
    fn change_bars(&mut self, bar: usize, bounds: Rect, pos: (i32, i32)) {
        let last = self.bar_last_edit[bar];
        let bars = self.bars(bar);
        let (width, height) = (bounds.width as f32, bounds.height as f32);
        let num_points = bars.num_points as i32;
        let scale = bars.scale;
        let from_index = bar_index(last, width, scale, num_points);
        let selected_index = bar_index(pos, width, scale, num_points);
        let x = pos.0 as f32;
        let mut y = pos.1 as f32;
        let x_delta = last.0 as f32 - x;
        let y_delta = last.1 as f32 - y;
        let slope = if y_delta == 0.0 {
            0.0
        } else {
            y_delta / x_delta
        };
        let mut next_x = width * (scale * selected_index as f32) / num_points as f32;
        let mut direction = -1;
        if selected_index < from_index {
            direction = 1;
            next_x += width * scale / num_points as f32;
        }
        let mut inc_x = next_x - x;
        let mut index = selected_index;
        while index != from_index + direction {
            if (0..num_points).contains(&index) {
                bars.set_y(index as usize, juce_clamp(-2.0 * y / height + 1.0));
            }
            y += inc_x * slope;
            inc_x = direction as f32 * scale * width / num_points as f32;
            index += direction;
        }
    }

    /// A choice from one of the editors' menus (`waveSourceCallback`,
    /// `barEditorCallback`), on `frame`.
    pub fn menu_choice(
        &mut self,
        part: WaveSourcePart,
        choice: i32,
        bounds: Rect,
        frame: &mut WaveFrame,
    ) -> Option<FrameEdit> {
        match part {
            WaveSourcePart::Waveform => {
                let samples = &mut frame.time_domain[..WAVEFORM_SIZE];
                match choice {
                    WAVEFORM_CLEAR => samples.fill(0.0),
                    WAVEFORM_FLIP_VERTICAL => samples.iter_mut().for_each(|value| *value = -*value),
                    WAVEFORM_FLIP_HORIZONTAL => samples.reverse(),
                    _ => return None,
                }
                Some(self.values_changed(frame, true))
            }
            WaveSourcePart::Amplitudes | WaveSourcePart::Phases => {
                let bar = usize::from(part == WaveSourcePart::Phases);
                // setClearValue: the phases clear to the default phase.
                let clear = if bar == 0 { -1.0 } else { DEFAULT_PHASE };
                let width = bounds.width as f32;
                let last = self.bar_last_edit[bar];
                if choice == BARS_RANDOMIZE {
                    let random = self.random[bar]
                        .get_or_insert_with(|| RandomGenerator::from_global_counter(-1.0, 1.0));
                    let mut values = vec![-1.0];
                    values.extend((1..NUM_REAL_COMPLEX).map(|_| random.next()));
                    let bars = self.bars(bar);
                    for (i, value) in values.into_iter().enumerate() {
                        bars.set_y(i, value);
                    }
                    return Some(self.bars_changed(frame, true));
                }
                let bars = self.bars(bar);
                let num_points = bars.num_points as i32;
                let position = bar_index(last, width, bars.scale, num_points) as usize;
                let cleared: Vec<usize> = match choice {
                    BARS_CLEAR => (0..NUM_REAL_COMPLEX).collect(),
                    BARS_CLEAR_RIGHT => (position + 1..NUM_REAL_COMPLEX).collect(),
                    BARS_CLEAR_LEFT => (0..position).collect(),
                    BARS_CLEAR_EVEN => (0..NUM_REAL_COMPLEX).step_by(2).collect(),
                    BARS_CLEAR_ODD => (1..NUM_REAL_COMPLEX).step_by(2).collect(),
                    _ => return None,
                };
                for i in cleared {
                    bars.set_y(i, clear);
                }
                Some(self.bars_changed(frame, true))
            }
        }
    }
}

/// The pointer over one of the editors this frame, in its own pixels.
pub(super) struct EditorInput {
    /// The pointer, rounded as `MouseEvent::getPosition` is.
    pub pos: Option<(i32, i32)>,
    pub hovered: bool,
    /// A button went down over it this frame.
    pub pressed: bool,
    pub popup: bool,
    /// A button is still down.
    pub held: bool,
}

pub(super) fn editor_input(
    panel: &mut Panel,
    bounds: Rect,
    salt: impl std::hash::Hash + std::fmt::Debug,
) -> EditorInput {
    let id = panel.ui.scope_id().with(salt);
    let rect = panel.pixels.rect(bounds.into());
    let response = panel.ui.interact(rect, id, Sense::click_and_drag());
    let (primary, secondary, modifiers, pointer) = panel.ui.input(|input| {
        (
            input.pointer.button_down(PointerButton::Primary),
            input.pointer.button_down(PointerButton::Secondary),
            input.modifiers,
            input.pointer.interact_pos(),
        )
    });
    let popup = secondary || (cfg!(target_os = "macos") && modifiers.ctrl && primary);
    let pos = pointer.map(|pos| {
        let (x, y) = to_pixels(panel.pixels, pos);
        (x.round() as i32 - bounds.x, y.round() as i32 - bounds.y)
    });
    let pressed_here = response.is_pointer_button_down_on() && (primary || secondary);
    EditorInput {
        pos,
        hovered: response.hovered(),
        pressed: pressed_here,
        popup,
        held: primary || secondary,
    }
}

/// `showPopupSelector` at a press on one of the editors.
fn open_menu(
    panel: &mut Panel,
    bounds: Rect,
    pos: (i32, i32),
    owner: (usize, usize),
    part: WaveSourcePart,
) {
    let items = match part {
        WaveSourcePart::Waveform => vec![
            MenuItem::new(WAVEFORM_CLEAR, "Clear"),
            MenuItem::new(WAVEFORM_FLIP_VERTICAL, "Flip Vertical"),
            MenuItem::new(WAVEFORM_FLIP_HORIZONTAL, "Flip Horizontal"),
        ],
        _ => vec![
            MenuItem::new(BARS_CLEAR, "Clear"),
            MenuItem::new(BARS_CLEAR_LEFT, "Clear Left"),
            MenuItem::new(BARS_CLEAR_RIGHT, "Clear Right"),
            MenuItem::new(BARS_CLEAR_ODD, "Clear Odd"),
            MenuItem::new(BARS_CLEAR_EVEN, "Clear Even"),
            MenuItem::new(BARS_RANDOMIZE, "Randomize"),
        ],
    };
    let (oscillator, kind) = owner;
    let position = Pos2::new((bounds.x + pos.0) as f32, (bounds.y + pos.1) as f32);
    panel.overlay.menu = Some(Menu::new(
        items,
        position,
        MenuOwner::WavetableEdit(oscillator, EditMenu::WaveSource(kind, part)),
    ));
}

/// `vital::utils::clamp(value, -1, 1)`: `std::min(std::max(value, -1), 1)`.
fn juce_clamp(value: f32) -> f32 {
    let value = if value < -1.0 { -1.0 } else { value };
    if 1.0 < value {
        1.0
    } else {
        value
    }
}

/// `BarEditor::getHoveredIndex`.
fn bar_index(pos: (i32, i32), width: f32, scale: f32, num_points: i32) -> i32 {
    let index = (num_points as f32 * pos.0 as f32 / width / scale).floor() as i32;
    index.clamp(0, num_points - 1)
}

/// The `WaveSourceEditor`'s size and grid, for its geometry.
struct WaveformGeometry {
    width: i32,
    height: i32,
    grid: (i32, i32),
}

impl WaveformGeometry {
    /// `getSnappedPoint`: the grid point nearest `input`, or `input` without
    /// a grid.
    fn snapped_point(&self, input: (i32, i32)) -> (i32, i32) {
        let (grid_x, grid_y) = self.grid;
        if grid_x == 0 || grid_y == 0 {
            return input;
        }
        let x = grid_x as f32 * input.0 as f32 / self.width as f32;
        let y = grid_y as f32 * input.1 as f32 / self.height as f32;
        let snapped_x = self.width as f32 * x.round() / grid_x as f32;
        let snapped_y = self.height as f32 * y.round() / grid_y as f32;
        (snapped_x as i32, snapped_y as i32)
    }

    /// `snapToGrid`: the nearest grid point if `input` is within the snap
    /// radius of it, else the last edit's position.
    fn snap_to_grid(&self, input: (i32, i32), last_edit: (i32, i32)) -> (i32, i32) {
        let snapped = self.snapped_point(input);
        // Point<int>::getDistanceFrom truncates.
        let (dx, dy) = ((input.0 - snapped.0) as f64, (input.1 - snapped.1) as f64);
        let distance = dx.hypot(dy) as i32;
        let radius = snap_radius(
            self.width as f32,
            self.height as f32,
            self.grid.0,
            self.grid.1,
        );
        if distance as f32 > radius {
            last_edit
        } else {
            snapped
        }
    }

    /// `getHoveredIndex`.
    fn index(&self, pos: (i32, i32)) -> i32 {
        let index = (WAVEFORM_SIZE as f32 * pos.0 as f32 / self.width as f32).floor() as i32;
        index.clamp(0, WAVEFORM_SIZE as i32 - 1)
    }

    /// `changeValues`: the samples from the last edit's position to the
    /// snapped pointer along the line between them. Returns the samples it
    /// set (`valuesChanged`'s range).
    fn change_values(
        &self,
        pos: (i32, i32),
        last_edit: (i32, i32),
        frame: &mut WaveFrame,
    ) -> Option<(i32, i32)> {
        let mouse = self.snap_to_grid(pos, last_edit);
        let from_index = self.index(last_edit);
        let selected_index = self.index(mouse);
        let (width, height) = (self.width as f32, self.height as f32);
        let num_points = WAVEFORM_SIZE as i32;
        let x = mouse.0 as f32;
        let mut y = mouse.1 as f32;
        let x_delta = last_edit.0 as f32 - x;
        let y_delta = last_edit.1 as f32 - y;
        let slope = if y_delta == 0.0 {
            0.0
        } else {
            y_delta / x_delta
        };
        let mut next_x = width * selected_index as f32 / num_points as f32;
        let mut direction = -1;
        if selected_index < from_index {
            direction = 1;
            next_x += width / num_points as f32;
        }
        let mut inc_x = next_x - x;
        let mut index = selected_index;
        while index != from_index + direction {
            if (0..num_points).contains(&index) {
                frame.time_domain[index as usize] = juce_clamp(-2.0 * y / height + 1.0);
            }
            y += inc_x * slope;
            inc_x = direction as f32 * width / num_points as f32;
            index += direction;
        }
        Some((
            from_index.min(selected_index),
            from_index.max(selected_index),
        ))
    }
}

/// The overlay's first drawing, before the editor's displays: its editors
/// paint at once, and its controls go in the section's layer.
pub fn paint_wave_source_overlay(
    panel: &mut Panel,
    edit: &WavetableEditLayout,
    layout: &OverlayLayout,
    overlay: &WaveSourceOverlay,
    frame: Option<&WaveFrame>,
) {
    let sliders = &overlay.sliders;
    paint_editors(panel, edit, overlay, frame);
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
    overlay: &WaveSourceOverlay,
    frame: Option<&WaveFrame>,
) {
    let sliders = &overlay.sliders;
    paint_editors(panel, edit, overlay, frame);
    paint_controls(panel, layout);
    panel.slider(&sliders.interpolation, layout.interpolation);
    panel.slider(&sliders.grid_x, layout.grid_x);
    panel.slider(&sliders.grid_y, layout.grid_y);
}

/// The overlay's editors over the displays: hidden until a keyframe of its
/// source is selected (`frameSelected`).
fn paint_editors(
    panel: &mut Panel,
    edit: &WavetableEditLayout,
    overlay: &WaveSourceOverlay,
    frame: Option<&WaveFrame>,
) {
    let Some(frame) = frame else {
        return;
    };
    let grid_x = panel.host.value(overlay.sliders.grid_x.name) as i32;
    let grid_y = panel.host.value(overlay.sliders.grid_y.name) as i32;
    paint_wave_source_editor(panel, edit.waveform, frame, overlay, grid_x, grid_y);
    paint_bar_editors(panel, edit, overlay);
}

/// The controls background with its titles.
fn paint_controls(panel: &mut Panel, layout: &OverlayLayout) {
    let titles: Vec<(&str, Rect)> = TITLES.iter().copied().zip(layout.titles).collect();
    paint_controls_background(panel, layout.controls, &layout.lines, &titles);
}

/// `WaveSourceEditor::render`: the grid lines and snap circles, the hover
/// circle, the line being drawn, then the keyframe's waveform with its fill
/// (`setLineValues`).
fn paint_wave_source_editor(
    panel: &mut Panel,
    bounds: Rect,
    frame: &WaveFrame,
    overlay: &WaveSourceOverlay,
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
            RectF::new(x, top, 1.0, height),
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
            RectF::new(left, top + row, width, 1.0),
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
                RectF::new(x - radius, y - radius, 2.0 * radius, 2.0 * radius),
                circle,
            ));
        }
    }
    // setHoverPosition: at the last edit while drawing, else at the grid
    // point nearest the pointer.
    if let Some(mouse) = overlay.mouse {
        let geometry = WaveformGeometry {
            width: bounds.width,
            height: bounds.height,
            grid: (grid_x, grid_y),
        };
        let (x, y) = if overlay.editing {
            overlay.last_edit
        } else {
            geometry.snapped_point(mouse)
        };
        let (x, y) = (left + x as f32, top + y as f32);
        quads.push(Quad::new(
            QuadKind::Circle,
            RectF::new(x - radius, y - radius, 2.0 * radius, 2.0 * radius),
            lighten,
        ));
    }
    let mut batch = QuadBatch::new();
    for quad in quads {
        batch.push(quad);
    }
    batch.paint(panel.pixels, bounds.into());

    let line_width = scope.value(ValueId::WidgetLineWidth);
    if let (true, Some(mouse)) = (overlay.editing, overlay.mouse) {
        let mut editing_line = LineRenderer::new(2, false);
        editing_line.line_width = line_width;
        editing_line.color = lighten;
        let (from, to) = (overlay.last_edit, mouse);
        editing_line.set_x_at(0, from.0 as f32);
        editing_line.set_y_at(0, from.1 as f32);
        editing_line.set_x_at(1, to.0 as f32);
        editing_line.set_y_at(1, to.1 as f32);
        editing_line.paint(panel.pixels, bounds.into(), true);
    }

    let line_color = scope.color(ColorId::WidgetPrimary1);
    let fill_color = scope.color(ColorId::WidgetSecondary1);
    let fade = fill_color.with_multiplied_alpha(1.0 - scope.value(ValueId::WidgetFillFade));
    let mut line = LineRenderer::new(WAVEFORM_SIZE, true);
    line.fill = true;
    line.fit = true;
    line.line_width = line_width;
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
    let mut nudged: RectF = bounds.into();
    nudged.y += GL_TIE_NUDGE;
    line.paint(panel.pixels, nudged, true);
}

/// The amplitude and phase `BarEditor`s over the displays, in the overlay's
/// bar colour, each with its hovered bar lightened.
fn paint_bar_editors(panel: &mut Panel, edit: &WavetableEditLayout, overlay: &WaveSourceOverlay) {
    let color: Colour = panel.scope.color(ColorId::WidgetSecondary2);
    let lighten = panel.scope.color(ColorId::LightenScreen);
    for (bar, (bars, bounds)) in [
        (&overlay.amplitudes, edit.amplitudes),
        (&overlay.phases, edit.phases),
    ]
    .into_iter()
    .enumerate()
    {
        let mut bars = bars.clone();
        bars.color = color;
        bars.paint(panel.pixels, bounds.into());
        // BarEditor::render's editing quad: off to the left when the pointer
        // is away.
        let mouse = overlay.bar_mouse[bar];
        let width = bounds.width as f32;
        let num_points = bars.num_points as i32;
        let hovered = if mouse.0 < 0 {
            -1
        } else {
            bar_index(mouse, width, bars.scale, num_points)
        };
        let bar_width = bars.scale * width / num_points as f32;
        let quad = Quad::new(
            QuadKind::Color,
            RectF::new(
                bounds.x as f32 + bar_width * hovered as f32,
                bounds.y as f32,
                bar_width,
                bounds.height as f32,
            ),
            lighten,
        );
        let mut batch = QuadBatch::new();
        batch.push(quad);
        batch.paint(panel.pixels, bounds.into());
    }
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

    /// `getSnappedPoint` and `snapToGrid`: the nearest grid point, taken
    /// only within the snap radius (by the truncated distance), else the last
    /// edit; without a grid, the point itself.
    #[test]
    fn presses_snap_to_grid_points_within_the_radius() {
        let geometry = WaveformGeometry {
            width: 600,
            height: 300,
            grid: (6, 4),
        };
        assert_eq!(geometry.snapped_point((310, 80)), (300, 75));
        // The radius is 15: a distance of 15.6 truncates to 15.
        assert_eq!(geometry.snap_to_grid((311, 86), (0, 0)), (300, 75));
        assert_eq!(geometry.snap_to_grid((312, 87), (7, 9)), (7, 9));
        let free = WaveformGeometry {
            grid: (0, 4),
            ..geometry
        };
        assert_eq!(free.snapped_point((311, 87)), (311, 87));
        assert_eq!(free.snap_to_grid((311, 87), (7, 9)), (311, 87));
    }

    /// `changeValues`: the samples between the last edit and the pointer
    /// follow the line between them.
    #[test]
    fn a_drag_draws_the_line_between_its_points() {
        let geometry = WaveformGeometry {
            width: 2048,
            height: 200,
            grid: (0, 0),
        };
        let mut frame = WaveFrame::new();
        let range = geometry.change_values((200, 0), (100, 200), &mut frame);
        assert_eq!(range, Some((100, 200)));
        // The first step's increment is zero, so the line starts a sample
        // late.
        assert_eq!(frame.time_domain[200], 1.0);
        assert_eq!(frame.time_domain[199], 1.0);
        assert!((frame.time_domain[150] - 0.02).abs() < 1e-6);
        assert!((frame.time_domain[101] + 0.96).abs() < 1e-6);
        assert_eq!(frame.time_domain[99], 0.0);
        assert_eq!(frame.time_domain[201], 0.0);
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
