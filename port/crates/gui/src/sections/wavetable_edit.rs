// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wavetable editor, shown in place of the interface when an
//! oscillator's edit button is clicked. Ports `WavetableEditSection`
//! (`vital/src/interface/editor_sections/wavetable_edit_section.cpp`) and
//! the parts it lays out from `vital/src/interface/wavetable/`:
//! `WavetableOrganizer`, `WavetableComponentList`, `WavetablePlayhead` and
//! `WavetablePlayheadInfo`.
//!
//! This part shows the oscillator's wavetable as it is: the frame under the
//! playhead in the waveform, amplitude and phase displays, the components in
//! the list and their keyframes in the organizer, with the first source's
//! first keyframe selected. The exit button closes the editor. Editing, the
//! component overlays and the menus come in the parts that follow.
//!
//! `WavetablePlayheadInfo` draws its number with JUCE's component painting,
//! which `FullInterface` turns off for its OpenGL context
//! (`setComponentPaintingEnabled(false)`), so upstream never shows it, and
//! neither does the port.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use sas_engine::sas_oscillators::sas_wavetable::creator::WavetableCreator;
use sas_engine::sas_oscillators::sas_wavetable::wave_frame::{NUM_REAL_COMPLEX, WAVEFORM_SIZE};
use sas_engine::sas_oscillators::sas_wavetable::{
    WaveFrame, Wavetable, NUM_OSCILLATOR_WAVE_FRAMES,
};

use super::Panel;
use crate::controls::bars::BarRenderer;
use crate::controls::icon;
use crate::controls::path::Path;
use crate::controls::paths;
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::fonts::Face;
use crate::interface::EditorModel;
use crate::layout::{PresetSelectorLayout, Rect};
use crate::paint::{PixelPainter, RectF};
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::section::{paint_body, paint_tab_shadow};
use crate::skin::{ColorId, Section, ValueId};
use crate::text::{Justify, Overflow};

/// The editor's skin override (`setSkinOverride(Skin::kWavetableEditor)`).
pub const SKIN: &[Section] = &[Section::WavetableEditor];

/// `getTopHeight`'s `kTopHeight`, before the size ratio.
const TOP_HEIGHT: f32 = 48.0;
/// `getFrameEditBounds`'s `kHeightRatio`.
const FRAME_EDIT_HEIGHT_RATIO: f32 = 0.66;
/// `WavetableOrganizer::kHandleHeightPercent`.
const HANDLE_HEIGHT_PERCENT: f32 = 1.0 / 8.0;
/// `vital::kNumOscillatorWaveFrames`: the organizer's and playhead's
/// positions.
const NUM_FRAMES: usize = NUM_OSCILLATOR_WAVE_FRAMES as usize;
/// `WavetableOrganizer::kDrawSkip` and `kDrawSkipLarge`.
const ORGANIZER_DRAW_SKIP: usize = 4;
const ORGANIZER_DRAW_SKIP_LARGE: usize = 32;
/// `WavetablePlayhead::kLineSkip` and `kBigLineSkip`.
const PLAYHEAD_LINE_SKIP: usize = 4;
const PLAYHEAD_BIG_LINE_SKIP: usize = 16;
/// The keyframe diamonds' border (`setThickness(2.0f)`).
const DIAMOND_THICKNESS: f32 = 2.0;
/// The section's starting `zoom_`.
const DEFAULT_ZOOM: f32 = 8.0;
/// `updateFrequencyDomain`'s epsilons.
const AMPLITUDE_EPSILON: f32 = 0.000_000_1;
const PHASE_EPSILON: f32 = 0.0001;

/// `WavetableEditSection::resized` and the geometry of its parts, in
/// interface coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WavetableEditLayout {
    pub bounds: Rect,
    /// `getFrameEditBounds` and `getTimelineBounds`, the two bodies.
    pub frame_edit: Rect,
    pub timeline: Rect,
    pub exit: Rect,
    /// `oscillator_waveform_`, `frequency_amplitudes_` and
    /// `frequency_phases_`.
    pub waveform: Rect,
    pub amplitudes: Rect,
    pub phases: Rect,
    /// `frequency_amplitude_settings_`, the gear over the amplitudes.
    pub settings: Rect,
    pub playhead_info: Rect,
    pub playhead: Rect,
    pub component_list: Rect,
    pub organizer: Rect,
    /// `preset_selector_`, its round amount, and the menu button beside it.
    pub preset: PresetSelectorLayout,
    pub preset_round_amount: f32,
    pub menu: Rect,
    /// `WavetableOrganizer::handleWidth`: the organizer's row height and
    /// keyframe size, and the component list's row height.
    pub handle_width: i32,
}

impl WavetableEditLayout {
    /// `resized` with the section at `bounds`; `top` is the interface's
    /// scope.
    pub fn new(top: &Scope, bounds: Rect) -> WavetableEditLayout {
        let scope = top.within(SKIN);
        let ratio = scope.size_ratio();
        let (width, height) = (bounds.width, bounds.height);
        let padding = scope.padding() as i32;
        let large_padding = scope.value(ValueId::LargePadding) as i32;
        let widget_margin = scope.widget_margin() as i32;
        let title_width = (ratio * TOP_HEIGHT) as i32;
        let button_height = (20.0 * ratio) as i32;

        let exit = Rect::new(width - title_width, padding, title_width, title_width);

        let frame_edit = Rect::new(
            large_padding,
            title_width,
            width - 2 * large_padding,
            ((height - title_width) as f32 * FRAME_EDIT_HEIGHT_RATIO) as i32,
        );
        // getTimelineBounds subtracts the unrounded padding.
        let timeline_height =
            ((height - frame_edit.bottom() - large_padding) as f32 - scope.padding()) as i32;
        let timeline = Rect::new(
            large_padding,
            frame_edit.bottom() + large_padding,
            frame_edit.width,
            timeline_height,
        );

        let edit_x = frame_edit.x + widget_margin;
        let edit_width = frame_edit.width - 2 * widget_margin;
        let osc_height = (frame_edit.height as f32 * 0.58) as i32;
        let amp_height = (frame_edit.height as f32 * 0.26) as i32;
        let phase_height = frame_edit.height - osc_height - amp_height - 4 * widget_margin;
        let waveform = Rect::new(edit_x, frame_edit.y + widget_margin, edit_width, osc_height);
        let amplitudes = Rect::new(
            edit_x,
            waveform.bottom() + widget_margin,
            edit_width,
            amp_height,
        );
        let settings = Rect::new(edit_x, amplitudes.y, button_height, button_height);
        let phases = Rect::new(
            edit_x,
            amplitudes.bottom() + widget_margin,
            edit_width,
            phase_height,
        );

        let playhead_height = (timeline.height as f32 * HANDLE_HEIGHT_PERCENT) as i32;
        let organizer_x = timeline.x + timeline.width / 4;
        let organizer_width = timeline.right() - organizer_x;
        let info_width = organizer_x - timeline.x;
        let playhead_info = Rect::new(timeline.x, timeline.y, info_width, playhead_height);
        let playhead = Rect::new(organizer_x, timeline.y, organizer_width, playhead_height);
        let below_playhead = timeline.height - playhead_height;
        let component_list = Rect::new(
            timeline.x,
            timeline.y + playhead_height,
            info_width,
            below_playhead,
        );
        let organizer = Rect::new(
            organizer_x,
            timeline.y + playhead_height,
            organizer_width,
            below_playhead,
        );
        let handle_width = 1 + 2 * (organizer.height as f32 * HANDLE_HEIGHT_PERCENT * 0.5) as i32;

        let preset_selector_width = width / 3;
        let preset_selector_height = (title_width as f32 * 0.6) as i32;
        let preset_selector_buffer = ((title_width - preset_selector_height) as f32 * 0.5) as i32;
        let preset_selector_x = (width - preset_selector_width + 2 * preset_selector_height) / 2;
        let preset_selector = Rect::new(
            preset_selector_x,
            preset_selector_buffer,
            preset_selector_width - preset_selector_height,
            preset_selector_height,
        );
        let menu = Rect::new(
            preset_selector.right(),
            preset_selector_buffer,
            preset_selector_height,
            preset_selector_height,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        WavetableEditLayout {
            bounds,
            frame_edit: frame_edit.translated(dx, dy),
            timeline: timeline.translated(dx, dy),
            exit: exit.translated(dx, dy),
            waveform: waveform.translated(dx, dy),
            amplitudes: amplitudes.translated(dx, dy),
            phases: phases.translated(dx, dy),
            settings: settings.translated(dx, dy),
            playhead_info: playhead_info.translated(dx, dy),
            playhead: playhead.translated(dx, dy),
            component_list: component_list.translated(dx, dy),
            organizer: organizer.translated(dx, dy),
            preset: PresetSelectorLayout::new(preset_selector.translated(dx, dy)),
            preset_round_amount: preset_selector_height as f32 / 2.0,
            menu: menu.translated(dx, dy),
            handle_width,
        }
    }

    /// `WavetableComponentList::positionGroups`'s cell height.
    pub fn cell_height(&self) -> i32 {
        self.handle_width - 2
    }

    /// The top of component list row `row`.
    fn row_y(&self, row: usize) -> i32 {
        self.component_list.y + row as i32 * self.handle_width + 2
    }

    /// A component's name in row `row`: sources start one row height in,
    /// modifiers two (`names_`).
    pub fn component_name(&self, row: usize, modifier: bool) -> Rect {
        let x = self.handle_width * if modifier { 2 } else { 1 };
        Rect::new(
            self.component_list.x + x,
            self.row_y(row),
            self.component_list.width - x,
            self.cell_height(),
        )
    }

    /// A component's menu button in row `row` (`menu_buttons_`).
    pub fn component_menu(&self, row: usize) -> Rect {
        let cell = self.cell_height();
        Rect::new(
            self.component_list.right() - cell,
            self.row_y(row),
            cell,
            cell,
        )
    }

    /// The "Add Modifier" (`modifier`) or "Add Source" button in row `row`,
    /// running past the list's right edge as upstream's do.
    pub fn add_button(&self, scope: &Scope, row: usize, modifier: bool) -> Rect {
        let indent = self.handle_width * if modifier { 2 } else { 1 };
        let rounding = scope.within(SKIN).value(ValueId::LabelBackgroundRounding);
        Rect::new(
            self.component_list.x + (indent as f32 - rounding) as i32,
            self.row_y(row),
            self.component_list.width,
            self.cell_height(),
        )
    }

    /// The plus icon on an add button in row `row` (`plus_icons_`).
    pub fn add_plus(&self, scope: &Scope, row: usize, modifier: bool) -> Rect {
        let indent = self.handle_width * if modifier { 2 } else { 1 };
        let rounding = scope.within(SKIN).value(ValueId::LabelBackgroundRounding);
        let cell = self.cell_height();
        Rect::new(
            self.component_list.x + (indent as f32 - rounding * 0.5) as i32,
            self.row_y(row),
            cell,
            cell,
        )
    }

    /// `WavetablePlayhead::padding_` (`setPadding(handleWidth() / 2.0f)`).
    fn playhead_padding(&self) -> f32 {
        self.handle_width as f32 / 2.0
    }

    /// `WavetableOrganizer::frame_width_`: the organizer's pixels per
    /// position.
    fn frame_width(&self) -> f32 {
        (self.organizer.width - self.handle_width + 1) as f32 / (NUM_FRAMES as f32 - 1.0)
    }
}

/// Where a keyframe is in the creator (upstream's `WavetableKeyframe*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyframeRef {
    pub group: usize,
    pub component: usize,
    pub keyframe: usize,
}

/// A component's row in the list and in the organizer.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentRow {
    pub group: usize,
    pub component: usize,
    /// `WavetableComponentFactory::getComponentName`.
    pub name: &'static str,
    /// The row in both views: each group's components, then a row for its
    /// "Add Modifier" button.
    pub row: usize,
    /// Its keyframes' positions, in order.
    pub positions: Vec<i32>,
    /// `hasKeyframes`: false for the Shepard tone source, whose one frame
    /// spans the organizer.
    pub has_keyframes: bool,
}

/// The components of `creator`, row by row.
pub fn component_rows(creator: &WavetableCreator) -> Vec<ComponentRow> {
    let mut rows = Vec::new();
    let mut row = 0;
    for (group_index, group) in creator.groups.iter().enumerate() {
        for (component_index, component) in group.components.iter().enumerate() {
            rows.push(ComponentRow {
                group: group_index,
                component: component_index,
                name: component.component_type().name(),
                row,
                positions: component.keyframe_positions(),
                has_keyframes: component.has_keyframes(),
            });
            row += 1;
        }
        row += 1;
    }
    rows
}

/// The editor of one oscillator's wavetable. Like upstream's three
/// sections, it keeps its playhead, selection and display settings while
/// hidden.
pub struct WavetableEditSection {
    oscillator: usize,
    /// The model's copy of the creator, and the content revision it was
    /// read at.
    creator: Option<(u64, WavetableCreator)>,
    /// `WavetablePlayhead::position_`.
    playhead: usize,
    /// `WavetableOrganizer::currently_selected_`.
    selected: Vec<KeyframeRef>,
    /// `zoom_` and `power_scale_`: the frequency displays' settings.
    zoom: f32,
    power_scale: bool,
    /// Whether the section showed last frame (`visibilityChanged`).
    visible: bool,
    /// The model's preset generation when the creator was last read: a
    /// load since then resets the section (`FullInterface::reset`).
    generation: Option<u64>,
}

impl WavetableEditSection {
    pub fn new(oscillator: usize) -> WavetableEditSection {
        WavetableEditSection {
            oscillator,
            creator: None,
            playhead: 0,
            selected: Vec::new(),
            zoom: DEFAULT_ZOOM,
            power_scale: true,
            visible: false,
            generation: None,
        }
    }

    pub fn oscillator(&self) -> usize {
        self.oscillator
    }

    pub fn playhead(&self) -> usize {
        self.playhead
    }

    pub fn selected(&self) -> &[KeyframeRef] {
        &self.selected
    }

    /// The creator as last read from the model.
    pub fn creator(&self) -> Option<&WavetableCreator> {
        self.creator.as_ref().map(|(_, creator)| creator)
    }

    /// Reads the creator again when the model's content has changed, and
    /// keeps the selection to the keyframes that still exist. Returns true
    /// when a preset loaded since the last read.
    fn refresh(&mut self, model: &dyn EditorModel) -> bool {
        let generation = model.preset_generation();
        let loaded = self.generation.is_some_and(|read| read != generation);
        self.generation = Some(generation);
        let revision = model.content_revision();
        if !loaded
            && self
                .creator
                .as_ref()
                .is_some_and(|(read, _)| *read == revision)
        {
            return false;
        }
        self.creator = model
            .wavetable_creator(self.oscillator)
            .map(|creator| (revision, creator));
        let rows = self.creator().map(component_rows).unwrap_or_default();
        self.selected.retain(|selected| {
            rows.iter().any(|row| {
                row.group == selected.group
                    && row.component == selected.component
                    && selected.keyframe < row.positions.len()
            })
        });
        loaded
    }

    /// `WavetableOrganizer::selectDefaultFrame`: the first component's
    /// first keyframe.
    fn select_default_frame(&mut self) {
        let first = self.creator().and_then(|creator| {
            let group = creator.groups.first()?;
            let component = group.components.first()?;
            (component.num_frames() > 0).then_some(KeyframeRef {
                group: 0,
                component: 0,
                keyframe: 0,
            })
        });
        if let Some(keyframe) = first {
            self.selected = vec![keyframe];
        }
    }

    /// `visibilityChanged` and the content checks before a frame: called
    /// each frame the section shows.
    pub fn update(&mut self, model: &dyn EditorModel) {
        if self.refresh(model) {
            // reset: the organizer's selection clears, and the default
            // frame is selected again if the section shows.
            self.selected.clear();
            if self.visible {
                self.select_default_frame();
            }
        }
        if !self.visible && self.selected.is_empty() {
            self.select_default_frame();
        }
        self.visible = true;
    }

    /// The section stopped showing.
    pub fn hidden(&mut self) {
        self.visible = false;
    }

    /// `paintBackgroundShadow` and `paintBackground`, with the children's
    /// backgrounds.
    pub fn paint_background(
        &self,
        pixels: &PixelPainter,
        top: &Scope,
        layout: &WavetableEditLayout,
    ) {
        let scope = top.within(SKIN);
        paint_tab_shadow(pixels, &scope, layout.frame_edit);
        paint_tab_shadow(pixels, &scope, layout.timeline);
        paint_body(pixels, &scope, layout.frame_edit);
        paint_body(pixels, &scope, layout.timeline);

        paint_organizer_background(pixels, &scope, layout);
        paint_playhead_background(pixels, &scope, layout);
        pixels.fill_rounded_rect(
            layout.preset.bounds,
            scope.value(ValueId::WidgetRoundedCorner),
            scope.color(ColorId::PopupSelectorBackground),
        );
        let widget_background = scope.color(ColorId::WidgetBackground);
        for display in [layout.waveform, layout.amplitudes, layout.phases] {
            pixels.fill_rect(display, widget_background);
        }
    }

    /// Draws the section over its background and handles its buttons.
    /// Returns true when the exit button is clicked
    /// (`FullInterface::hideWavetableEditSection`).
    pub fn show(&mut self, panel: &mut Panel, layout: &WavetableEditLayout) -> bool {
        let model = panel.host.model();
        let wavetable = model.wavetable(self.oscillator);
        let frame = wavetable
            .as_deref()
            .map(|wavetable| frame_samples(wavetable, self.playhead));
        self.show_displays(panel, layout, frame.as_ref());

        let colours = ShapeColours::icon(&panel.scope);
        let shape = |shape| ShapeButton { shape, colours };
        panel.shape_button(
            "wavetable settings",
            shape(Shape::Icon {
                key: "gear",
                path: paths::gear(),
            }),
            layout.settings,
        );

        let preset = layout.preset;
        let text = self.creator().map(preset_selector_text).unwrap_or_default();
        panel.text_centred(
            &text,
            preset.text,
            Face::Title,
            preset.text_size,
            panel.scope.color(ColorId::PresetText),
        );
        panel.shape_button("wavetable previous", shape(Shape::Prev), preset.previous);
        panel.shape_button("wavetable next", shape(Shape::Next), preset.next);
        let (key, path) = sized_path("wavetable menu", layout.menu.height, paths::menu);
        panel.shape_button(
            "wavetable menu",
            shape(Shape::Icon { key, path }),
            layout.menu,
        );
        let exit = panel.shape_button(
            "wavetable exit",
            shape(Shape::Icon {
                key: "exit x",
                path: paths::exit_x(),
            }),
            layout.exit,
        );

        let rows = self.creator().map(component_rows).unwrap_or_default();
        let group_sizes: Vec<usize> = self
            .creator()
            .map(|creator| {
                creator
                    .groups
                    .iter()
                    .map(|group| group.components.len())
                    .collect()
            })
            .unwrap_or_default();
        self.show_component_list(panel, layout, &rows, &group_sizes);
        self.show_organizer(panel, layout, &rows);
        self.show_playhead(panel, layout);
        exit.clicked
    }

    /// The three displays of the frame under the playhead
    /// (`updateTimeDomain`, `updateFrequencyDomain`, `setColors`), with
    /// their rounded corners.
    fn show_displays(
        &self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        frame: Option<&[f32; WAVEFORM_SIZE]>,
    ) {
        let scope = &panel.scope;
        let primary = scope.color(ColorId::WidgetPrimaryDisabled);
        let secondary = scope.color(ColorId::WidgetSecondaryDisabled);
        let fade_alpha = 1.0 - scope.value(ValueId::WidgetFillFade);

        let mut waveform = LineRenderer::new(WAVEFORM_SIZE, false);
        waveform.fill = true;
        waveform.fit = true;
        waveform.line_width = scope.value(ValueId::WidgetLineWidth);
        waveform.color = primary;
        waveform.set_fill_colors(secondary.with_multiplied_alpha(fade_alpha), secondary);
        let mut amplitudes = BarRenderer::new(NUM_REAL_COMPLEX, true);
        amplitudes.square_scale = true;
        amplitudes.power_scale = self.power_scale;
        amplitudes.scale = self.zoom;
        amplitudes.color = secondary;
        let mut phases = BarRenderer::new(NUM_REAL_COMPLEX, true);
        phases.scale = self.zoom;
        phases.color = secondary;

        // `resized` and `setLineValues`: the points in the display's pixels.
        let width = layout.waveform.width as f32;
        let adjust = layout.waveform.height as f32 / 2.0;
        for i in 0..WAVEFORM_SIZE {
            waveform.set_x_at(i, i as f32 * width / (WAVEFORM_SIZE as f32 - 1.0));
            waveform.set_y_at(i, adjust);
        }
        if let Some(frame) = frame {
            for (i, &sample) in frame.iter().enumerate() {
                waveform.set_y_at(i, adjust * (1.0 - sample));
            }
            for (i, (amplitude, phase)) in frequency_domain(frame).into_iter().enumerate() {
                amplitudes.set_scaled_y(i, amplitude);
                phases.set_y(i, phase);
            }
        }
        waveform.paint(panel.pixels, layout.waveform.into(), true);
        amplitudes.paint(panel.pixels, layout.amplitudes.into());
        phases.paint(panel.pixels, layout.phases.into());
        for display in [layout.waveform, layout.amplitudes, layout.phases] {
            panel.widget_corners(display);
        }
    }

    /// The organizer's rows, keyframes and playhead line
    /// (`setRowQuads`, `setFrameQuads`, `playheadMoved`).
    fn show_organizer(
        &self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        rows: &[ComponentRow],
    ) {
        let scope = &panel.scope;
        let organizer = layout.organizer;
        let handle = layout.handle_width;
        let lighten = scope.color(ColorId::LightenScreen);
        let edge = scope.color(ColorId::Background);
        let primary = scope.color(ColorId::WidgetPrimary1);
        let unselected_alt = scope.color(ColorId::WidgetPrimaryDisabled);
        let mut quads = Vec::new();
        for row in rows {
            let y = organizer.y + row.row as i32 * handle + 2;
            quads.push(Quad::new(
                QuadKind::Color,
                RectF::new(
                    organizer.x as f32,
                    y as f32,
                    organizer.width as f32,
                    (handle - 2) as f32,
                ),
                lighten,
            ));
        }

        let frame_width = layout.frame_width();
        let mut unselected = Vec::new();
        let mut selected = Vec::new();
        for row in rows {
            let y = organizer.y + row.row as i32 * handle;
            for (index, &position) in row.positions.iter().enumerate() {
                let bounds = if row.has_keyframes {
                    let x = (frame_width * position as f32) as i32;
                    Rect::new(organizer.x + x, y, handle, handle)
                } else {
                    Rect::new(organizer.x, y, organizer.width, handle)
                };
                let keyframe = KeyframeRef {
                    group: row.group,
                    component: row.component,
                    keyframe: index,
                };
                let mut quad = Quad::new(QuadKind::Diamond, bounds.into(), edge);
                quad.thickness = DIAMOND_THICKNESS;
                if self.selected.contains(&keyframe) {
                    quad.alt_color = primary;
                    selected.push(quad);
                } else {
                    quad.alt_color = unselected_alt;
                    unselected.push(quad);
                }
            }
        }
        quads.extend(unselected);
        quads.extend(selected);

        let x = organizer.x + handle / 2 + (self.playhead as f32 * frame_width) as i32;
        quads.push(Quad::new(
            QuadKind::Color,
            RectF::new(x as f32, organizer.y as f32, 1.0, organizer.height as f32),
            primary,
        ));
        let clip = RectF::from(organizer);
        panel.layer().custom(move |pixels| {
            let mut batch = crate::primitives::QuadBatch::new();
            for quad in quads {
                batch.push(quad);
            }
            batch.paint(pixels, clip);
        });
    }

    /// `WavetableComponentList::positionGroups`: a row per component with
    /// its name and menu button, an "Add Modifier" button after each
    /// group's rows and "Add Source" after the last, clipped to the list.
    fn show_component_list(
        &self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        rows: &[ComponentRow],
        group_sizes: &[usize],
    ) {
        let scope = &panel.scope;
        let list = layout.component_list;
        let row_height = layout.handle_width;
        let rounding = scope.value(ValueId::LabelBackgroundRounding);
        let cell_height = layout.cell_height();
        let lighten = scope.color(ColorId::LightenScreen);
        // `setUiButton(false)`: a UI button, its text light at 0.45 of its
        // height.
        let button_background = scope.color(ColorId::UiButton);
        let button_text = scope.color(ColorId::UiButtonText);
        let button_font_size = 0.45 * cell_height as f32;
        let name_size = row_height as f32 * 0.5;

        // `component_backgrounds_`: a rounded rectangle from a row height
        // less the rounding in (two for a modifier) past the right edge.
        let mut backgrounds = Vec::new();
        let mut names = Vec::new();
        for row in rows {
            let modifier = row.component > 0;
            let name = layout.component_name(row.row, modifier);
            let mut quad = Quad::new(
                QuadKind::RoundedRectangle,
                RectF::new(
                    name.x as f32 - rounding,
                    name.y as f32,
                    list.width as f32 + row_height as f32,
                    cell_height as f32,
                ),
                lighten,
            );
            quad.rounding = rounding;
            backgrounds.push(quad);
            names.push((row.name, name));
        }

        // The add buttons (`OpenGlToggleButton`s, their text indented past
        // the plus) and their plus icons.
        let mut buttons = Vec::new();
        let mut row = 0;
        for &size in group_sizes {
            row += size;
            buttons.push((
                "Add Modifier",
                layout.add_button(scope, row, true),
                layout.add_plus(scope, row, true),
            ));
            row += 1;
        }
        buttons.push((
            "Add Source",
            layout.add_button(scope, row, false),
            layout.add_plus(scope, row, false),
        ));
        let (plus_key, plus_path) = sized_path("plus", cell_height, paths::plus);
        let text_buffer = (rounding * 0.5 + cell_height as f32) as i32;
        panel.layer().custom(move |pixels| {
            pixels.with_clip(list, |pixels| {
                let mut batch = crate::primitives::QuadBatch::new();
                for quad in backgrounds {
                    batch.push(quad);
                }
                for &(_, button, _) in &buttons {
                    let mut quad =
                        Quad::new(QuadKind::RoundedRectangle, button.into(), button_background);
                    quad.rounding = rounding;
                    batch.push(quad);
                }
                batch.paint(pixels, list.into());
                for (name, bounds) in names {
                    pixels.text_line(
                        name,
                        bounds,
                        (Face::Light, name_size, crate::colour::Colour(0xffff_ffff)),
                        Justify::Left,
                        Overflow::Fit,
                    );
                }
                for (text, button, plus) in buttons {
                    let text_bounds = Rect::new(
                        button.x + text_buffer,
                        button.y,
                        button.width - 2 * text_buffer,
                        button.height,
                    );
                    pixels.text_line(
                        text,
                        text_bounds,
                        (Face::Light, button_font_size, button_text),
                        Justify::Left,
                        Overflow::Fit,
                    );
                    icon::fill_path_to_fit(pixels, plus_key, plus_path, plus.into(), button_text);
                }
            });
        });

        let colours = ShapeColours::icon(&panel.scope);
        let (menu_key, menu_path) = sized_path("wavetable row menu", cell_height, paths::menu);
        for row in rows {
            panel.shape_button(
                ("wavetable component menu", row.row),
                ShapeButton {
                    shape: Shape::Icon {
                        key: menu_key,
                        path: menu_path,
                    },
                    colours,
                },
                layout.component_menu(row.row),
            );
        }
    }

    /// `WavetablePlayhead::setPositionQuad`.
    fn show_playhead(&self, panel: &mut Panel, layout: &WavetableEditLayout) {
        let playhead = layout.playhead;
        let padding = layout.playhead_padding();
        let active_width = playhead.width as f32 - 2.0 * padding + 1.0;
        let x = ((active_width * self.playhead as f32) / (NUM_FRAMES as f32 - 1.0) - 0.5 + padding)
            as i32;
        panel.quad(Quad::new(
            QuadKind::Color,
            RectF::new(
                (playhead.x + x) as f32,
                playhead.y as f32,
                1.0,
                playhead.height as f32,
            ),
            panel.scope.color(ColorId::WidgetPrimary1),
        ));
    }
}

/// `setPresetSelectorText`: the name, then the author after a dash when
/// there is one (`PresetSelector::setText(left, center, right)`).
fn preset_selector_text(creator: &WavetableCreator) -> String {
    if creator.author.is_empty() {
        creator.name.clone()
    } else {
        format!("{}  -  {}", creator.name, creator.author)
    }
}

/// The time-domain samples of frame `position` of `wavetable`
/// (`getBuffer(position)`).
fn frame_samples(wavetable: &Wavetable, position: usize) -> [f32; WAVEFORM_SIZE] {
    *wavetable.buffer(position.min(NUM_FRAMES - 1))
}

/// `updateFrequencyDomain`: each harmonic's amplitude and its phase as a
/// fraction of pi, for the first half of the spectrum.
pub fn frequency_domain(frame: &[f32; WAVEFORM_SIZE]) -> Vec<(f32, f32)> {
    let mut compute = WaveFrame::new();
    compute.load_time_domain(frame);
    (0..WAVEFORM_SIZE / 2)
        .map(|i| {
            let value = compute.frequency_domain[i];
            let amplitude = value.abs() / WAVEFORM_SIZE as f32;
            let mut phase = if amplitude > AMPLITUDE_EPSILON {
                value.arg()
            } else {
                -std::f32::consts::PI / 2.0
            };
            if phase >= std::f32::consts::PI - PHASE_EPSILON {
                phase = -std::f32::consts::PI;
            }
            (amplitude, phase / std::f32::consts::PI)
        })
        .collect()
}

/// `WavetableOrganizer::paintBackground`: the background, the half-handle
/// strips at either end and a line every few positions.
fn paint_organizer_background(pixels: &PixelPainter, scope: &Scope, layout: &WavetableEditLayout) {
    let organizer = layout.organizer;
    pixels.fill_rect(organizer, scope.color(ColorId::Background));
    let lighten = scope.color(ColorId::LightenScreen);
    let half_handle = (0.5 * layout.handle_width as f32) as i32;
    let half_lighten = lighten.with_multiplied_alpha(0.5);
    pixels.fill_rect(
        Rect::new(organizer.x, organizer.y, half_handle, organizer.height),
        half_lighten,
    );
    pixels.fill_rect(
        Rect::new(
            organizer.right() - half_handle,
            organizer.y,
            half_handle,
            organizer.height,
        ),
        half_lighten,
    );
    let frame_width = layout.frame_width();
    for i in (ORGANIZER_DRAW_SKIP..NUM_FRAMES - 1).step_by(ORGANIZER_DRAW_SKIP) {
        let colour = if i % ORGANIZER_DRAW_SKIP_LARGE == 0 {
            lighten
        } else {
            half_lighten
        };
        let x = (i as f32 * frame_width + half_handle as f32) as i32;
        pixels.fill_rect(
            Rect::new(organizer.x + x, organizer.y, 1, organizer.height),
            colour,
        );
    }
}

/// `WavetablePlayhead::paintBackground`: a tick every few positions, taller
/// every sixteenth.
fn paint_playhead_background(pixels: &PixelPainter, scope: &Scope, layout: &WavetableEditLayout) {
    let playhead = layout.playhead;
    let padding = layout.playhead_padding();
    let active_width = playhead.width as f32 - 2.0 * padding + 1.0;
    let lighten = scope.color(ColorId::LightenScreen);
    let small_line_height = playhead.height / 3;
    let big_line_height = 2 * small_line_height;
    for i in (0..NUM_FRAMES).step_by(PLAYHEAD_LINE_SKIP) {
        let x = (padding - 0.5 + i as f32 * active_width / (NUM_FRAMES as f32 - 1.0)) as i32;
        let line_height = if i % PLAYHEAD_BIG_LINE_SKIP == 0 {
            big_line_height
        } else {
            small_line_height
        };
        pixels.fill_rect(
            Rect::new(
                playhead.x + x,
                playhead.bottom() - line_height,
                1,
                line_height,
            ),
            lighten,
        );
    }
}

/// A pixel-snapped icon built for a button `height` pixels tall, with its
/// cache key; one is kept per name and height.
fn sized_path(
    name: &'static str,
    height: i32,
    make: fn(i32) -> Path,
) -> (&'static str, &'static Path) {
    type Paths = HashMap<(&'static str, i32), (&'static str, &'static Path)>;
    static PATHS: OnceLock<Mutex<Paths>> = OnceLock::new();
    let mut cache = PATHS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *cache.entry((name, height)).or_insert_with(|| {
        let key: &'static str = Box::leak(format!("{name} {height}").into_boxed_str());
        (key, Box::leak(Box::new(make(height))))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sas_engine::sas_oscillators::sas_wavetable::creator::component::{
        ComponentType, WavetableComponent,
    };
    use sas_engine::sas_oscillators::sas_wavetable::creator::group::WavetableGroup;

    /// A sine at the first harmonic is half its amplitude in bin 1, at a
    /// phase of minus a half turn (`std::arg` of a sine's bin), and the
    /// silent bins show `-kPi / 2` (`updateFrequencyDomain`).
    #[test]
    fn a_sines_frequency_domain() {
        let mut frame = [0.0f32; WAVEFORM_SIZE];
        for (i, sample) in frame.iter_mut().enumerate() {
            *sample = (2.0 * std::f32::consts::PI * i as f32 / WAVEFORM_SIZE as f32).sin();
        }
        let bins = frequency_domain(&frame);
        assert_eq!(bins.len(), WAVEFORM_SIZE / 2);
        assert!((bins[1].0 - 0.5).abs() < 1e-5, "{:?}", bins[1]);
        assert!((bins[1].1 + 0.5).abs() < 1e-4, "{:?}", bins[1]);
        assert!(bins[3].0 < 1e-5);
        assert_eq!(bins[3].1, -0.5);
    }

    /// Each group's components take a row, then a row for its "Add
    /// Modifier" button.
    #[test]
    fn rows_leave_a_row_after_each_group() {
        let mut creator = WavetableCreator::new();
        creator.clear();
        let mut first = WavetableGroup::new();
        let mut source = WavetableComponent::new(ComponentType::WaveSource);
        source.insert_new_keyframe(0);
        source.insert_new_keyframe(128);
        first.add_component(source);
        let mut fold = WavetableComponent::new(ComponentType::WaveFolder);
        fold.insert_new_keyframe(0);
        first.add_component(fold);
        creator.add_group(first);
        let mut second = WavetableGroup::new();
        let mut line = WavetableComponent::new(ComponentType::LineSource);
        line.insert_new_keyframe(64);
        second.add_component(line);
        creator.add_group(second);

        let rows = component_rows(&creator);
        let summary: Vec<_> = rows
            .iter()
            .map(|row| (row.name, row.row, row.positions.clone()))
            .collect();
        assert_eq!(
            summary,
            [
                ("Wave Source", 0, vec![0, 128]),
                ("Wave Folder", 1, vec![0]),
                ("Line Source", 3, vec![64]),
            ]
        );
    }

    #[test]
    fn the_author_follows_the_name() {
        let mut creator = WavetableCreator::new();
        creator.name = "Bells".to_string();
        assert_eq!(preset_selector_text(&creator), "Bells");
        creator.author = "Someone".to_string();
        assert_eq!(preset_selector_text(&creator), "Bells  -  Someone");
    }
}
