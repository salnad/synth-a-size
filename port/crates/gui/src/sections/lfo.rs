// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! An LFO section: the mode, the rate (free, tempo or keytracked), phase,
//! fade or smooth time, delay and stereo. Ports `LfoSection`
//! (`vital/src/interface/editor_sections/lfo_section.cpp`).
//!
//! The LFO editor (`controls::line_editor`) sits above, with its grid size,
//! paint, paint pattern and smooth controls and the shape's name. The
//! paint and smooth buttons' icons and the LFO browser come later. The sync
//! selector comes with the shared `TempoSelector`; until then its slot is
//! empty and the rate slider follows the sync value.

use super::{MenuOwner, Panel};
use crate::colour::Colour;
use crate::controls::icon::fill_path;
use crate::controls::line_editor::{
    paint_pattern as line_paint_pattern, LineEditor, LineSettings, PAINT_PATTERN_NAMES,
};
use crate::controls::path::{Path, Transform};
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::controls::slider::{Placement, Slider, SliderStyle};
use crate::fonts::Face;
use crate::layout::{PresetSelectorLayout, Rect};
use crate::local;
use crate::paint::PixelPainter;
use crate::popup::{Menu, MenuItem};
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::section::{
    draw_label, draw_tempo_divider, draw_text_component_background, keytrack_divider,
    label_background_bounds, label_font, paint_widget_background, place_knobs_in_area,
    place_tempo_controls, slider_overlap, slider_width, title_width,
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

/// `TempoSelector::MenuOptions`: the sync parameter's values.
pub mod sync {
    pub const SECONDS: usize = 0;
    pub const KEYTRACK: usize = 4;
}

/// An LFO's or random LFO's rate sliders, by the sync value
/// (`TempoSelector::valueChanged`): free in seconds, keytracked as
/// transpose and tune, otherwise tempo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rate {
    Free,
    Tempo,
    Keytrack,
}

impl Rate {
    pub fn from_sync(sync: f32) -> Rate {
        match sync as usize {
            sync::SECONDS => Rate::Free,
            sync::KEYTRACK => Rate::Keytrack,
            _ => Rate::Tempo,
        }
    }
}

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
}

/// `LfoSection`'s grid size sliders.
fn grid_slider(name: &'static str, prefix: &'static str) -> Slider {
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
            names,
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
            editor: LineEditor { lfo: index },
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
        draw_tempo_divider(pixels, scope, layout.sync);
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
        let rate = Rate::from_sync(host.value(self.names.sync));
        match rate {
            Rate::Free => panel.slider(&self.frequency, layout.rate),
            Rate::Tempo => panel.slider(&self.tempo, layout.rate),
            Rate::Keytrack => {
                panel.slider(&self.keytrack_transpose, layout.keytrack_transpose);
                panel.slider(&self.keytrack_tune, layout.keytrack_tune);
                let colour = panel.scope.color(ColorId::LightenScreen);
                panel.quad(Quad::new(QuadKind::Color, layout.divider.into(), colour));
            }
        }
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
        panel.shape_button(("lfo prev", self.index), prev, preset.previous);
        panel.shape_button(("lfo next", self.index), next, preset.next);

        self.show_editor(panel, layout);
    }

    /// The editor and the controls that set it up: the grid sizes, paint
    /// and its pattern, smooth, and the shape's name in the strip.
    fn show_editor(&self, panel: &mut Panel, layout: &LfoLayout) {
        let host = panel.host;
        let settings = LineSettings {
            grid_size_x: host.value(local::GRID_SIZE_X[self.index]) as usize,
            grid_size_y: host.value(local::GRID_SIZE_Y[self.index]) as usize,
            paint: host.value(local::PAINT[self.index]) != 0.0,
            paint_pattern: host.value(local::PAINT_PATTERN[self.index]) as usize,
            phase: self.names.phase,
        };
        let event = self.editor.show(panel, layout.editor, settings);
        panel.slider(&self.grid_size_x, layout.grid_size_x);
        panel.slider(&self.grid_size_y, layout.grid_size_y);

        let colours = ShapeColours::icon(&panel.scope);
        let button = ShapeButton {
            shape: Shape::None,
            colours,
        };
        if panel
            .shape_button(("lfo paint", self.index), button, layout.paint)
            .clicked
        {
            host.set_once(
                local::PAINT[self.index],
                if event.painting { 0.0 } else { 1.0 },
            );
        }
        if panel
            .shape_button(("lfo smooth", self.index), button, layout.smooth)
            .clicked
        {
            self.editor.set_smooth(panel, !event.smooth);
        }

        // PaintPatternSelector: the pattern's shape; a click lists them.
        if panel
            .shape_button(
                ("lfo paint pattern", self.index),
                button,
                layout.paint_pattern,
            )
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
                    name: local::PAINT_PATTERN[self.index],
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
        let name = self.editor.shape(panel).name().to_string();
        panel.text_centred(
            &name,
            preset.text,
            Face::Title,
            preset.text_size,
            panel.scope.color(ColorId::PresetText),
        );
    }
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
