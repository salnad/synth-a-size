// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! An LFO section: the mode, the rate (free, tempo or keytracked), phase,
//! fade or smooth time, delay and stereo. Ports `LfoSection`
//! (`vital/src/interface/editor_sections/lfo_section.cpp`).
//!
//! The LFO editor (`controls::line_editor`) sits above, with its grid size,
//! paint, paint pattern and smooth controls and the shape's name. The
//! LFO browser comes later. The sync
//! selector is the shared `TempoSelector`.

use std::path::{Path as FsPath, PathBuf};

use sas_preset::user_folders::{self, Kind};

use super::{popup_browser, MenuOwner, Panel};
use crate::colour::Colour;
use crate::controls::icon::fill_path;
use crate::controls::line_editor::{
    paint_pattern as line_paint_pattern, LineEditor, LineEditorEvent, LineSettings, LineTarget,
    PAINT_PATTERN_NAMES,
};
use crate::controls::path::{Path, Transform};
use crate::controls::paths::Icon;
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::controls::slider::{Placement, Slider, SliderStyle, Wheel};
use crate::controls::tempo::{self, TempoSelector};
use crate::fonts::Face;
use crate::interface::EditorModel;
use crate::layout::{PresetSelectorLayout, Rect};
use crate::local;
use crate::paint::PixelPainter;
use crate::popup::{Menu, MenuItem};
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::section::{
    draw_label, draw_text_component_background, keytrack_divider, label_background_bounds,
    label_font, paint_widget_background, place_knobs_in_area, place_tempo_controls, slider_overlap,
    slider_width, title_width,
};
use crate::skin::{ColorId, Section, ValueId};

/// The section's skin override (`setSkinOverride(Skin::kLfo)`).
pub const SKIN: &[Section] = &[Section::Lfo];

/// `vital::kNumLfos`.
pub const NUM_LFOS: usize = 8;

/// `strings::kSyncNames`: the mode list.
pub const SYNC_NAMES: [&str; 6] = [
    "Trigger",
    "Sync",
    "Envelope",
    "Sustain Envelope",
    "Loop Point",
    "Loop Hold",
];

/// `strings::kSmoothModeNames`.
pub const SMOOTH_MODE_NAMES: [&str; 2] = ["FADE IN", "SMOOTH"];

/// The parameter names of LFO `index` (0-based).
#[derive(Clone, Copy, Debug)]
pub struct Names {
    pub frequency: &'static str,
    pub tempo: &'static str,
    pub keytrack_transpose: &'static str,
    pub keytrack_tune: &'static str,
    pub sync: &'static str,
    pub sync_type: &'static str,
    pub phase: &'static str,
    pub fade_time: &'static str,
    pub smooth_time: &'static str,
    pub smooth_mode: &'static str,
    pub delay_time: &'static str,
    pub stereo: &'static str,
}

macro_rules! names {
    ($($n:literal),*) => {
        [$(Names {
            frequency: concat!("lfo_", $n, "_frequency"),
            tempo: concat!("lfo_", $n, "_tempo"),
            keytrack_transpose: concat!("lfo_", $n, "_keytrack_transpose"),
            keytrack_tune: concat!("lfo_", $n, "_keytrack_tune"),
            sync: concat!("lfo_", $n, "_sync"),
            sync_type: concat!("lfo_", $n, "_sync_type"),
            phase: concat!("lfo_", $n, "_phase"),
            fade_time: concat!("lfo_", $n, "_fade_time"),
            smooth_time: concat!("lfo_", $n, "_smooth_time"),
            smooth_mode: concat!("lfo_", $n, "_smooth_mode"),
            delay_time: concat!("lfo_", $n, "_delay_time"),
            stereo: concat!("lfo_", $n, "_stereo"),
        }),*]
    };
}

/// Each LFO's parameter names.
pub const NAMES: [Names; NUM_LFOS] = names!("1", "2", "3", "4", "5", "6", "7", "8");

/// `LfoSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LfoLayout {
    pub bounds: Rect,
    /// The LFO editor.
    pub editor: Rect,
    pub phase: Rect,
    pub sync_type: Rect,
    /// The rate slider (frequency or tempo) and the sync selector.
    pub rate: Rect,
    pub sync: Rect,
    pub keytrack_transpose: Rect,
    pub keytrack_tune: Rect,
    /// The keytrack sliders' divider.
    pub divider: Rect,
    /// Fade time, or smooth time in its place.
    pub fade: Rect,
    pub delay: Rect,
    pub stereo: Rect,
    /// The smooth mode's text and selector, over the fade knob's label.
    pub smooth_mode: Rect,
    pub preset_selector: PresetSelectorLayout,
    pub paint: Rect,
    pub paint_pattern: Rect,
    pub grid_size_x: Rect,
    pub grid_size_y: Rect,
    pub smooth: Rect,
}

impl LfoLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> LfoLayout {
        let (width, height) = (bounds.width, bounds.height);
        let title_width = title_width(scope) as i32;
        let knob_section_height = scope.knob_section_height() as i32;
        let slider_width = slider_width(scope);
        let slider_overlap = slider_overlap(scope);
        let widget_margin = scope.widget_margin() as i32;

        let wave_height = height - slider_width - widget_margin - title_width - knob_section_height
            + 2 * slider_overlap;
        let editor = Rect::new(
            widget_margin,
            title_width,
            width - 2 * widget_margin,
            wave_height,
        );
        let phase = Rect::new(
            0,
            editor.bottom() - slider_overlap + widget_margin,
            width,
            slider_width,
        );

        let knobs_width = (4.0 * scope.value(ValueId::ModulationButtonWidth).trunc()
            + widget_margin as f32
            + scope.value(ValueId::Padding)) as i32;
        let style_width = width - knobs_width;
        let knob_y = height - knob_section_height;
        let text_component_width = style_width / 2 - widget_margin;
        let sync_type = Rect::new(
            widget_margin,
            knob_y + widget_margin,
            text_component_width,
            knob_section_height - 2 * widget_margin,
        );
        let tempo_x = sync_type.right() + widget_margin;
        let (rate, sync, modulation_area) = place_tempo_controls(
            scope,
            tempo_x,
            knob_y + widget_margin,
            style_width - tempo_x,
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

        let knobs = place_knobs_in_area(
            scope,
            Rect::new(style_width, knob_y, knobs_width, knob_section_height),
            3,
        );
        let smooth_mode = label_background_bounds(scope, knobs[0], false);

        // SynthSection::getPresetBrowserBounds.
        let browser_x = (width as f32 * 0.4) as i32;
        let browser = Rect::new(
            browser_x,
            widget_margin,
            width - browser_x - widget_margin,
            title_width - 2 * widget_margin,
        );
        let top_height = title_width - 2 * widget_margin;
        let smooth = Rect::new(
            browser.x - title_width - widget_margin,
            widget_margin,
            title_width,
            top_height,
        );
        let grid_size_y = Rect::new(
            smooth.x - title_width - widget_margin,
            widget_margin,
            title_width,
            top_height,
        );
        let grid_size_x = Rect::new(
            grid_size_y.x - title_width - widget_margin,
            widget_margin,
            title_width,
            top_height,
        );
        let paint = Rect::new(widget_margin, widget_margin, top_height, top_height);
        let paint_pattern = Rect::new(
            paint.right(),
            widget_margin,
            grid_size_x.x - paint.right() - widget_margin,
            top_height,
        );

        let at = |r: Rect| r.translated(bounds.x, bounds.y);
        LfoLayout {
            bounds,
            editor: at(editor),
            phase: at(phase),
            sync_type: at(sync_type),
            rate: at(rate),
            sync: at(sync),
            keytrack_transpose: at(keytrack_transpose),
            keytrack_tune: at(keytrack_tune),
            divider: at(divider),
            fade: at(knobs[0]),
            delay: at(knobs[1]),
            stereo: at(knobs[2]),
            smooth_mode: at(smooth_mode),
            preset_selector: PresetSelectorLayout::new(at(browser)),
            paint: at(paint),
            paint_pattern: at(paint_pattern),
            grid_size_x: at(grid_size_x),
            grid_size_y: at(grid_size_y),
            smooth: at(smooth),
        }
    }
}

/// One LFO's controls (`LfoSection`'s constructor).
pub struct LfoSection {
    index: usize,
    names: Names,
    sync: TempoSelector,
    frequency: Slider,
    tempo: Slider,
    keytrack_transpose: Slider,
    keytrack_tune: Slider,
    sync_type: Slider,
    phase: Slider,
    fade: Slider,
    smooth: Slider,
    delay: Slider,
    stereo: Slider,
    editor: LineEditor,
    grid_size_x: Slider,
    grid_size_y: Slider,
    /// `current_file_`: the LFO file last loaded from the browser or the
    /// arrows.
    current_file: std::cell::RefCell<Option<PathBuf>>,
    /// The popup browser this section asked for since the last call: its
    /// bounds and the section's (`textMouseDown`).
    browse_request: std::cell::RefCell<Option<(Rect, Rect)>>,
}

/// `textMouseDown`'s `kBrowserWidth` and `kBrowserHeight`, before the size
/// ratio.
const BROWSER_WIDTH: f32 = 500.0;
const BROWSER_HEIGHT: f32 = 250.0;

/// `LfoSection`'s grid size sliders.
pub(crate) fn grid_slider(name: &'static str, prefix: &'static str) -> Slider {
    let mut slider = Slider::new(name, SliderStyle::Text)
        .sensitivity(0.2)
        .popup_prefix(prefix);
    slider.text_component_offset = Some(0.0);
    slider.text_height_percentage = 0.6;
    slider
}

impl LfoSection {
    /// LFO `index` (0-based).
    pub fn new(index: usize) -> LfoSection {
        /// `LfoSection`'s `kTempoDragSensitivity`.
        const TEMPO_DRAG_SENSITIVITY: f32 = 0.5;
        /// `SynthSection::kTransposeMouseSensitivity`.
        const TRANSPOSE_MOUSE_SENSITIVITY: f32 = 0.2;
        let names = NAMES[index];
        LfoSection {
            index,
            current_file: std::cell::RefCell::new(None),
            browse_request: std::cell::RefCell::new(None),
            names,
            sync: TempoSelector::new(names.sync),
            frequency: Slider::new(names.frequency, SliderStyle::Text),
            tempo: Slider::new(names.tempo, SliderStyle::Text)
                .sensitivity(TEMPO_DRAG_SENSITIVITY)
                .text_entry(1.0, None),
            keytrack_transpose: Slider::new(names.keytrack_transpose, SliderStyle::Text)
                .sensitivity(TRANSPOSE_MOUSE_SENSITIVITY)
                .bipolar(),
            keytrack_tune: Slider::new(names.keytrack_tune, SliderStyle::Text)
                .bipolar()
                .display(3, 0),
            sync_type: Slider::selector(names.sync_type, Some(&SYNC_NAMES)),
            phase: Slider::new(names.phase, SliderStyle::Horizontal)
                .modulation_placement(Placement::Above),
            fade: Slider::new(names.fade_time, SliderStyle::Rotary),
            smooth: Slider::new(names.smooth_time, SliderStyle::Rotary),
            delay: Slider::new(names.delay_time, SliderStyle::Rotary),
            stereo: Slider::new(names.stereo, SliderStyle::Rotary)
                .bipolar()
                .snap_to_value(0.0),
            editor: LineEditor {
                target: LineTarget::Lfo(index),
            },
            grid_size_x: grid_slider(local::GRID_SIZE_X[index], "X Grid"),
            grid_size_y: grid_slider(local::GRID_SIZE_Y[index], "Y Grid"),
        }
    }

    /// `LfoSection::paintBackground`.
    pub fn paint_background(&self, pixels: &PixelPainter, scope: &Scope, layout: &LfoLayout) {
        if layout.bounds.width <= 0 {
            return;
        }
        let frequency = Rect::new(
            layout.rate.x,
            layout.rate.y,
            layout.sync.right() - layout.rate.x,
            layout.rate.height,
        );
        draw_text_component_background(pixels, scope, layout.sync_type, true);
        draw_text_component_background(pixels, scope, frequency, true);
        tempo::paint_divider(pixels, scope, layout.sync);
        draw_label(pixels, scope, "MODE", layout.sync_type, true);
        draw_label(pixels, scope, "FREQUENCY", frequency, true);
        draw_label(pixels, scope, "DELAY", layout.delay, false);
        draw_label(pixels, scope, "STEREO", layout.stereo, false);
        draw_label(pixels, scope, "", layout.fade, false);

        let widget_margin = scope.widget_margin() as i32;
        let rounding = scope.value(ValueId::WidgetRoundedCorner) as i32 as f32;
        let popup_background = scope.color(ColorId::PopupSelectorBackground);
        let grid = Rect::new(
            layout.grid_size_x.x,
            layout.bounds.y + widget_margin,
            layout.grid_size_y.right() - layout.grid_size_x.x,
            layout.grid_size_x.height,
        );
        pixels.fill_rounded_rect(grid, rounding, popup_background);
        pixels.fill_rounded_rect(
            Rect::new(
                layout.bounds.x + widget_margin,
                layout.bounds.y + widget_margin,
                layout.grid_size_x.x - layout.bounds.x - 2 * widget_margin,
                layout.grid_size_x.height,
            ),
            rounding,
            popup_background,
        );
        let (face, size, colour) = label_font(scope);
        pixels.text_centred("-", grid, face, size, colour);

        for (knob, bounds) in [
            (&self.fade, layout.fade),
            (&self.delay, layout.delay),
            (&self.stereo, layout.stereo),
        ] {
            knob.paint_shadow(pixels, scope, bounds);
        }

        // paintChildrenBackgrounds: the name strip and the editor.
        pixels.fill_rounded_rect(
            layout.preset_selector.bounds,
            scope.value(ValueId::WidgetRoundedCorner),
            popup_background,
        );
        paint_widget_background(pixels, scope, layout.editor);
    }

    pub fn show(&self, panel: &mut Panel, layout: &LfoLayout) {
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
        panel.slider(&self.sync_type, layout.sync_type);
        panel.slider(&self.phase, layout.phase);

        // setSmoothModeSelected: smooth time in place of fade in.
        let smooth_mode = (host.value(self.names.smooth_mode) as usize).min(1);
        if smooth_mode == 0 {
            panel.slider(&self.fade, layout.fade);
        } else {
            panel.slider(&self.smooth, layout.fade);
        }
        panel.slider(&self.delay, layout.delay);
        panel.slider(&self.stereo, layout.stereo);
        panel.text_centred(
            SMOOTH_MODE_NAMES[smooth_mode],
            layout.smooth_mode,
            Face::Regular,
            panel.scope.value(ValueId::LabelHeight),
            panel.scope.color(ColorId::BodyText),
        );
        let invisible = ShapeButton {
            shape: Shape::None,
            colours: ShapeColours::icon(&panel.scope),
        };
        if panel
            .shape_button(("smooth mode", self.index), invisible, layout.smooth_mode)
            .pressed
        {
            let items = SMOOTH_MODE_NAMES
                .iter()
                .enumerate()
                .map(|(i, name)| MenuItem::new(i as i32, *name))
                .collect();
            panel.overlay.menu = Some(Menu::new(
                items,
                egui::pos2(
                    layout.smooth_mode.x as f32,
                    layout.smooth_mode.bottom() as f32,
                ),
                MenuOwner::Control {
                    name: self.names.smooth_mode,
                    then: None,
                },
            ));
        }

        let ui_colours = ShapeColours::ui(&panel.scope);
        let preset = layout.preset_selector;
        let prev = ShapeButton {
            shape: Shape::Prev,
            colours: ui_colours,
        };
        let next = ShapeButton {
            shape: Shape::Next,
            colours: ui_colours,
        };
        let mut shift = 0;
        if panel
            .shape_button(("lfo prev", self.index), prev, preset.previous)
            .clicked
        {
            shift = -1;
        }
        if panel
            .shape_button(("lfo next", self.index), next, preset.next)
            .clicked
        {
            shift = 1;
        }
        if shift != 0 {
            // prevClicked and nextClicked: getShiftedFile among the user's
            // LFOs (no added folders).
            let model = panel.host.model();
            let directories = user_folders::root(model.data_directory().as_deref())
                .map(|root| user_folders::directories(&root, Kind::Lfos))
                .unwrap_or_default();
            let files = user_folders::files_of_type(&directories, popup_browser::LFO_EXTENSIONS);
            let current = self.current_file();
            if let Some(file) = user_folders::shifted_file(files, current.as_deref(), shift) {
                self.load_file(model, &file);
            }
        }
        // textMouseDown: the browser above the section, its right edge at
        // the name's.
        let text = panel.pixels.rect(preset.text.into());
        let response = panel.ui.interact(
            text,
            panel.ui.scope_id().with(("lfo name", self.index)),
            egui::Sense::click(),
        );
        if response.hovered() && panel.ui.input(|input| input.pointer.primary_pressed()) {
            let ratio = panel.scope.size_ratio();
            let (width, height) = (
                (BROWSER_WIDTH * ratio) as i32,
                (BROWSER_HEIGHT * ratio) as i32,
            );
            let browser = Rect::new(
                preset.bounds.right() - width,
                layout.bounds.y - height,
                width,
                height,
            );
            *self.browse_request.borrow_mut() = Some((browser, layout.bounds));
        }

        self.show_editor(panel, layout);
    }

    /// `getCurrentFile`.
    pub fn current_file(&self) -> Option<PathBuf> {
        self.current_file.borrow().clone()
    }

    /// `loadFile`: an existing `file` becomes the current file before it's
    /// read, so the arrows step on past one that doesn't read, and then its
    /// shape loads.
    pub fn load_file(&self, model: &dyn EditorModel, file: &FsPath) {
        if !file.exists() {
            return;
        }
        *self.current_file.borrow_mut() = Some(file.to_path_buf());
        popup_browser::load_lfo_file(model, self.index, file);
    }

    /// The popup browser asked for since the last call: the browser's
    /// bounds and the section's.
    pub fn take_browse_request(&self) -> Option<(Rect, Rect)> {
        self.browse_request.borrow_mut().take()
    }

    /// The editor and the controls that set it up: the grid sizes, paint
    /// and its pattern, smooth, and the shape's name in the strip.
    fn show_editor(&self, panel: &mut Panel, layout: &LfoLayout) {
        let controls = LineControls {
            editor: &self.editor,
            grid_size_x: &self.grid_size_x,
            grid_size_y: &self.grid_size_y,
            paint: local::PAINT[self.index],
            paint_pattern: local::PAINT_PATTERN[self.index],
            phase: Some(self.names.phase),
            active: true,
        };
        let layout = LineControlsLayout {
            editor: layout.editor,
            grid_size_x: layout.grid_size_x,
            grid_size_y: layout.grid_size_y,
            paint: layout.paint,
            paint_pattern: layout.paint_pattern,
            smooth: layout.smooth,
            preset_selector: layout.preset_selector,
        };
        show_line_controls(panel, &controls, &layout);
    }
}

/// A line editor and the controls that set it up (`LfoSection`'s and
/// `ModulationMatrix`'s): the grid sizes and paint pattern (local
/// controls), paint, smooth, and the shape's name in the strip.
pub(crate) struct LineControls<'a> {
    pub editor: &'a LineEditor,
    pub grid_size_x: &'a Slider,
    pub grid_size_y: &'a Slider,
    pub paint: &'static str,
    pub paint_pattern: &'static str,
    /// The LFO's phase control, for "Set Start Point".
    pub phase: Option<&'static str>,
    /// `setActive`: drawn in the disabled colours when off.
    pub active: bool,
}

/// Where [`show_line_controls`] puts its parts.
pub(crate) struct LineControlsLayout {
    pub editor: Rect,
    pub grid_size_x: Rect,
    pub grid_size_y: Rect,
    pub paint: Rect,
    pub paint_pattern: Rect,
    pub smooth: Rect,
    pub preset_selector: PresetSelectorLayout,
}

/// Shows a line editor and its controls; the section shows the strip's
/// arrows and handles its browser.
pub(crate) fn show_line_controls(
    panel: &mut Panel,
    controls: &LineControls,
    layout: &LineControlsLayout,
) -> LineEditorEvent {
    let host = panel.host;
    let editor = controls.editor;
    let target = editor.target;
    let settings = LineSettings {
        grid_size_x: host.value(controls.grid_size_x.name) as usize,
        grid_size_y: host.value(controls.grid_size_y.name) as usize,
        paint: host.value(controls.paint) != 0.0,
        paint_pattern: host.value(controls.paint_pattern) as usize,
        phase: controls.phase,
        active: controls.active,
    };
    let event = editor.show(panel, layout.editor, settings, None);
    // lineEditorScrolled: the wheel over the editor moves the paint pattern
    // while painting, else the X grid.
    let editor_rect = panel.pixels.rect(layout.editor.into());
    if panel.ui.rect_contains_pointer(editor_rect) {
        let target = if event.painting {
            Slider::new(controls.paint_pattern, SliderStyle::Text)
        } else {
            *controls.grid_size_x
        };
        let mut value = host.value(target.name);
        for wheel in panel.ui.input(Wheel::events) {
            if let Some(moved) = target.wheel_value(value, wheel) {
                host.set_once(target.name, moved);
                value = moved;
            }
        }
    }
    panel.slider(controls.grid_size_x, layout.grid_size_x);
    panel.slider(controls.grid_size_y, layout.grid_size_y);

    let colours = ShapeColours::icon(&panel.scope);
    let button = ShapeButton {
        shape: Shape::None,
        colours,
    };
    let toggled = |on: bool| {
        if on {
            ShapeColours::icon_on(&panel.scope)
        } else {
            colours
        }
    };
    let paint = ShapeButton {
        shape: Shape::from(Icon::PaintBrush),
        colours: toggled(event.painting),
    };
    let smooth = ShapeButton {
        shape: Shape::from(Icon::HalfSinCurve),
        colours: toggled(event.smooth),
    };
    if panel
        .shape_button(("line paint", target), paint, layout.paint)
        .clicked
    {
        host.set_once(controls.paint, if event.painting { 0.0 } else { 1.0 });
    }
    if panel
        .shape_button(("line smooth", target), smooth, layout.smooth)
        .clicked
    {
        editor.set_smooth(panel, !event.smooth);
    }

    // PaintPatternSelector: the pattern's shape; a click lists them.
    if panel
        .shape_button(("line paint pattern", target), button, layout.paint_pattern)
        .pressed
    {
        let items = PAINT_PATTERN_NAMES
            .iter()
            .enumerate()
            .map(|(i, name)| MenuItem::new(i as i32, *name))
            .collect();
        panel.overlay.menu = Some(Menu::new(
            items,
            egui::pos2(
                layout.paint_pattern.x as f32,
                layout.paint_pattern.bottom() as f32,
            ),
            MenuOwner::Control {
                name: controls.paint_pattern,
                then: None,
            },
        ));
    }
    paint_pattern_selector(
        panel,
        layout.paint_pattern,
        settings.paint_pattern,
        event.painting,
    );

    let preset = layout.preset_selector;
    let name = editor.shape(panel).name().to_string();
    panel.text_centred(
        &name,
        preset.text,
        Face::Title,
        preset.text_size,
        panel.scope.color(ColorId::PresetText),
    );
    event
}

/// `PaintPatternSelector::paint`: the pattern filled and outlined, in the
/// widget colours while painting and the lighten colour otherwise.
fn paint_pattern_selector(panel: &mut Panel, bounds: Rect, pattern: usize, active: bool) {
    const LINE_WIDTH_HEIGHT_RATIO: f32 = 0.05;
    let scope = panel.scope;
    let padding = scope.widget_margin() as i32;
    let height = (bounds.height - 2 * padding - 1) as f32;
    let width = (bounds.width - 2 * padding - 1) as f32;
    let buffer = padding as f32 + 0.5;
    let (x0, y0) = (bounds.x as f32, bounds.y as f32);
    let mut points = vec![(x0 + buffer, y0 + buffer + height)];
    for &(x, y) in line_paint_pattern(pattern) {
        points.push((x0 + buffer + x * width, y0 + buffer + (1.0 - y) * height));
    }
    points.push((x0 + buffer + width, y0 + buffer + height));
    let mut path = Path::new();
    path.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        path.line_to(x, y);
    }
    path.close();
    let fills: Vec<Colour> = if active {
        vec![
            scope.color(ColorId::WidgetSecondary1),
            scope.color(ColorId::WidgetSecondary2),
        ]
    } else {
        vec![scope.color(ColorId::LightenScreen)]
    };
    let stroke = if active {
        scope.color(ColorId::WidgetCenterLine)
    } else {
        scope.color(ColorId::LightenScreen)
    };
    let mut line_width = (bounds.height as f32 * LINE_WIDTH_HEIGHT_RATIO) as i32;
    line_width += (line_width + 1) % 2;
    let key = format!(
        "lfo paint pattern {pattern} {}x{}",
        bounds.width, bounds.height
    );
    panel.layer().custom(move |pixels| {
        for fill in fills {
            fill_path(pixels, &key, &path, Transform::IDENTITY, fill);
        }
        // Upstream strokes with curved joints and rounded ends, which
        // controls::path doesn't port yet; a round-ended polyline matches.
        pixels.polyline(&points, line_width as f32, stroke);
    });
}
