// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Matrix tab: `ModulationMatrix` and its `ModulationMatrixRow`s
//! (`vital/src/interface/editor_sections/modulation_matrix.cpp`).
//!
//! The connections are rows in a scrolled list, connected ones first: each
//! with its bypass toggle (its number), source, bipolar, stereo, morph
//! (power curve), amount and destination. Below the list is the remap
//! section, whose controls come in G4c-3.

use egui::Ui;
use sas_preset::constants::MAX_MODULATION_CONNECTIONS;
use sas_preset::registry::ValueScale;

use std::path::{Path as FsPath, PathBuf};

use sas_preset::user_folders::{self, Kind};

use super::lfo::{grid_slider, show_line_controls, LineControls, LineControlsLayout};
use super::popup_browser;
use crate::controls::button::SynthButton;
use crate::controls::line_editor::{LineEditor, LineTarget};
use crate::controls::modulation_button::menu_source_display_name;
use crate::controls::paths::Icon;
use crate::controls::scroll_bar::ScrollBar;
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::controls::slider::{Slider, SliderStyle};
use crate::fonts::Face;
use crate::host::Host;
use crate::interface::EditorModel;
use crate::interface::Route;
use crate::layer::Layer;
use crate::layout::{PresetSelectorLayout, Rect};
use crate::local;
use crate::modulation::slot_control;
use crate::paint::{PixelPainter, RectF};
use crate::popup::{DualMenu, DualMenuGroup, Menu, MenuItem};
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::scope::Scope;
use crate::section::{label_font, paint_widget_background};
use crate::section::{paint_body, paint_border, paint_tab_shadow, title_width};
use crate::sections::{MenuOwner, Overlay, Panel};
use crate::skin::{ColorId, Section, ValueId};

/// `setSkinOverride(Skin::kModulationMatrix)`.
pub const SKIN: &[Section] = &[Section::ModulationMatrix];

/// `kSourceWidthPercent`, `kDestinationWidthPercent`, `kPaddingWidthPercent`.
const SOURCE_WIDTH_PERCENT: f32 = 0.2;
const DESTINATION_WIDTH_PERCENT: f32 = 0.2;
const PADDING_WIDTH_PERCENT: f32 = 0.04;
/// `kMatrixHeightInRows`.
const MATRIX_HEIGHT_IN_ROWS: i32 = 12;
/// `ModulationMatrix::kRowPadding`.
const ROW_PADDING: i32 = 1;
/// `ModulationMatrix::getRowHeight`: 34 at size ratio 1.
const ROW_HEIGHT: f32 = 34.0;
/// `kNoConnectionString`.
const NO_CONNECTION: &str = "-";
/// `StatusOutput::kClearValue`.
const CLEAR_VALUE: f32 = i32::MIN as f32;

/// The tab's areas (`ModulationMatrix::resized`), in editor pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatrixLayout {
    pub bounds: Rect,
    pub title_width: i32,
    pub row_height: i32,
    /// The scrolled list of rows (`viewport_`).
    pub viewport: Rect,
    /// The list's heading and rows (`paintBody(g, matrix_bounds)`).
    pub matrix: Rect,
    /// The remap section below it.
    pub remap: Rect,
    pub scroll_bar: Rect,
}

/// The remap section: its header (`remap_name_`, `paint_`,
/// `paint_pattern_`, the grid sizes, `smooth_` and the LFO file strip) and
/// the selected row's curve editor (`map_editors_`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemapLayout {
    pub name: Rect,
    /// `remap_name_->setTextSize`.
    pub name_size: f32,
    pub paint: Rect,
    pub paint_pattern: Rect,
    pub grid_size_x: Rect,
    pub grid_size_y: Rect,
    pub smooth: Rect,
    pub preset_selector: PresetSelectorLayout,
    pub editor: Rect,
}

impl MatrixLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> MatrixLayout {
        /// `resized`' `kScrollBarWidth`.
        const SCROLL_BAR_WIDTH: f32 = 13.0;
        let size_ratio = scope.size_ratio();
        let row_height = (size_ratio * ROW_HEIGHT) as i32;
        let title_width = title_width(scope) as i32;
        let padding = scope.value(ValueId::Padding) as i32;
        let matrix_height = (row_height + ROW_PADDING) * MATRIX_HEIGHT_IN_ROWS + ROW_PADDING;
        let viewport = Rect::new(
            bounds.x,
            bounds.y + title_width,
            bounds.width,
            matrix_height,
        );
        let matrix_bottom = title_width + matrix_height;
        let remap_y = matrix_bottom + padding;
        let scroll_bar_width = (size_ratio * SCROLL_BAR_WIDTH) as i32;
        MatrixLayout {
            bounds,
            title_width,
            row_height,
            viewport,
            matrix: Rect::new(bounds.x, bounds.y, bounds.width, matrix_bottom),
            remap: Rect::new(
                bounds.x,
                bounds.y + remap_y,
                bounds.width,
                bounds.height - remap_y,
            ),
            scroll_bar: Rect::new(
                bounds.x + bounds.width - scroll_bar_width - 1,
                bounds.y + title_width,
                scroll_bar_width,
                matrix_height,
            ),
        }
    }

    /// The rows' column height (`container_`): the shown rows with the
    /// padding around each, at least the viewport's.
    pub fn list_height(&self, shown: usize) -> i32 {
        (ROW_PADDING + shown as i32 * (self.row_height + ROW_PADDING)).max(self.viewport.height)
    }

    /// The remap section's parts (`resized`, `setRowPositions`).
    pub fn remap_layout(&self, scope: &Scope) -> RemapLayout {
        let (x, width) = (self.bounds.x, self.bounds.width);
        let widget_margin = scope.widget_margin() as i32;
        let title_width = self.title_width;
        let preset_x = width / 2;
        let top_bar_height = title_width - 2 * widget_margin;
        let top_bar_y = self.remap.y + widget_margin;
        let at = |left: i32, w: i32| Rect::new(x + left, top_bar_y, w, top_bar_height);
        let smooth_x = preset_x - title_width - widget_margin;
        let grid_y_x = smooth_x - title_width - widget_margin;
        let grid_x_x = grid_y_x - title_width - widget_margin;
        let paint_pattern_width = 3 * top_bar_height;
        let paint_pattern_x = grid_x_x - paint_pattern_width - widget_margin;
        let paint_x = paint_pattern_x - top_bar_height;
        let editor_y = self.remap.y + title_width;
        RemapLayout {
            name: at(widget_margin, paint_x - 2 * widget_margin),
            name_size: title_width as f32 * 0.45,
            paint: at(paint_x, top_bar_height),
            paint_pattern: at(paint_pattern_x, paint_pattern_width),
            grid_size_x: at(grid_x_x, title_width),
            grid_size_y: at(grid_y_x, title_width),
            smooth: at(smooth_x, title_width),
            preset_selector: PresetSelectorLayout::new(at(
                preset_x,
                width - preset_x - widget_margin,
            )),
            editor: Rect::new(
                x + widget_margin,
                editor_y,
                width - 2 * widget_margin,
                self.bounds.bottom() - editor_y - widget_margin,
            ),
        }
    }

    /// The row at `position` in the list (`setRowPositions`), scrolled up by
    /// `scroll` pixels.
    pub fn row(&self, position: usize, scroll: i32) -> Rect {
        Rect::new(
            self.viewport.x,
            self.viewport.y + ROW_PADDING + position as i32 * (self.row_height + ROW_PADDING)
                - scroll,
            self.viewport.width,
            self.row_height,
        )
    }

    /// The headings' columns (`paintBackground`): where the number, source,
    /// bipolar, stereo, morph, amount and destination headings start, in
    /// the section's coordinates.
    fn columns(&self, scope: &Scope) -> [i32; 6] {
        let width = self.bounds.width;
        let x_padding = (width as f32 * PADDING_WIDTH_PERCENT) as i32;
        let y_padding = (scope.size_ratio() * 3.0) as i32;
        let source_width = (width as f32 * SOURCE_WIDTH_PERCENT) as i32;
        let destination_width = (width as f32 * DESTINATION_WIDTH_PERCENT) as i32;
        let component_height = self.row_height - 2 * y_padding;
        let bipolar_x = source_width + self.row_height;
        let stereo_x = bipolar_x + component_height + x_padding;
        let morph_x = stereo_x + component_height + x_padding;
        let amount_x = morph_x + component_height + x_padding;
        let destination_x = width - destination_width - x_padding;
        [
            self.row_height,
            bipolar_x,
            stereo_x,
            morph_x,
            amount_x,
            destination_x,
        ]
    }
}

/// A row's controls (`ModulationMatrixRow::resized`), in editor pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowLayout {
    pub bounds: Rect,
    pub bypass: Rect,
    pub source: Rect,
    pub bipolar: Rect,
    pub stereo: Rect,
    pub power: Rect,
    pub destination: Rect,
    pub amount: Rect,
}

impl RowLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> RowLayout {
        let width = bounds.width;
        let height = bounds.height;
        let x_padding = (width as f32 * PADDING_WIDTH_PERCENT) as i32;
        let y_padding = (scope.size_ratio() * 3.0) as i32;
        let source_width = (width as f32 * SOURCE_WIDTH_PERCENT) as i32;
        let destination_width = (width as f32 * DESTINATION_WIDTH_PERCENT) as i32;
        let component_height = height - 2 * y_padding;
        let slider_height = crate::section::slider_width(scope);
        let text_component_height = (component_height as f32 * 0.7) as i32;
        let text_y = ((height - text_component_height) as f32 / 2.0) as i32;

        let bypass = Rect::new(
            y_padding,
            text_y,
            height - 2 * y_padding,
            text_component_height,
        );
        let source = Rect::new(
            bypass.x + bypass.width + y_padding,
            y_padding,
            source_width,
            component_height,
        );
        let bipolar_x = (source.x + source.width) as f32 + x_padding as f32 / 2.0;
        let bipolar = Rect::new(
            bipolar_x as i32,
            y_padding,
            component_height,
            component_height,
        );
        let stereo_x = bipolar.x + bipolar.width + x_padding;
        let expand = x_padding / 2;
        let stereo = Rect::new(
            stereo_x - expand,
            text_y,
            component_height + 2 * expand,
            text_component_height,
        );
        let power_x = stereo_x + component_height + x_padding;
        let power = Rect::new(power_x, y_padding, component_height, component_height);
        let destination_x = width - x_padding - destination_width;
        let destination = Rect::new(
            destination_x,
            y_padding,
            (destination_width as f32 + x_padding as f32 / 2.0) as i32,
            component_height,
        );
        let widget_margin = scope.value(ValueId::WidgetMargin) as i32;
        let amount_x = power.x + power.width + x_padding - widget_margin;
        let amount_width = (destination_x as f32 - amount_x as f32 - x_padding as f32 / 2.0
            + 2.0 * widget_margin as f32) as i32;
        let amount = Rect::new(
            amount_x,
            (height - slider_height + 1) / 2,
            amount_width,
            slider_height,
        );
        let at = |rect: Rect| rect.translated(bounds.x, bounds.y);
        RowLayout {
            bounds,
            bypass: at(bypass),
            source: at(source),
            bipolar: at(bipolar),
            stereo: at(stereo),
            power: at(power),
            destination: at(destination),
            amount: at(amount),
        }
    }
}

/// One slot's controls (`ModulationMatrixRow`).
struct Row {
    amount: Slider,
    power: Slider,
    bipolar: SynthButton,
    stereo: SynthButton,
    bypass: SynthButton,
}

impl Row {
    fn new(slot: usize) -> Row {
        let number: &'static str = Box::leak((slot + 1).to_string().into_boxed_str());
        Row {
            amount: Slider::new(slot_control(slot, "amount"), SliderStyle::Horizontal).bipolar(),
            power: Slider::new(slot_control(slot, "power"), SliderStyle::Curve),
            bipolar: SynthButton::icon(slot_control(slot, "bipolar"), Icon::Bipolar),
            stereo: SynthButton::just_text(slot_control(slot, "stereo"), "L/R", "L/R"),
            bypass: SynthButton::just_text(slot_control(slot, "bypass"), number, "X"),
        }
    }
}

/// `ModulationMatrix::SortColumn`: the heading the rows are sorted by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortColumn {
    Number,
    Source,
    Bipolar,
    Stereo,
    Morph,
    Amount,
    Destination,
}

/// What `sort` compares a row by (`ModulationMatrixRow`'s accessors).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowValues {
    pub slot: usize,
    /// The selectors' values: indices into [`source_names`] and
    /// [`destination_names`].
    pub source: usize,
    pub destination: usize,
    pub bipolar: bool,
    pub stereo: bool,
    pub morph: f32,
    pub amount: f32,
    pub connected: bool,
}

/// `ModulationMatrix::sort`: a stable sort of `order` by `column`, then a
/// stable sort putting connected rows first (`compareConnected`).
pub fn sort_rows(order: &mut [usize], values: &[RowValues], column: SortColumn, ascending: bool) {
    use std::cmp::Ordering;
    let compare = |a: &usize, b: &usize| -> Ordering {
        let (a, b) = (&values[*a], &values[*b]);
        let ordering = match column {
            SortColumn::Number => a.slot.cmp(&b.slot),
            SortColumn::Source => a.source.cmp(&b.source),
            SortColumn::Bipolar => a.bipolar.cmp(&b.bipolar),
            SortColumn::Stereo => a.stereo.cmp(&b.stereo),
            SortColumn::Morph => a.morph.partial_cmp(&b.morph).unwrap_or(Ordering::Equal),
            SortColumn::Amount => a.amount.partial_cmp(&b.amount).unwrap_or(Ordering::Equal),
            SortColumn::Destination => a.destination.cmp(&b.destination),
        };
        if ascending {
            ordering
        } else {
            ordering.reverse()
        }
    };
    order.sort_by(compare);
    order.sort_by_key(|slot| !values[*slot].connected);
}

/// `checkNumModulationsShown`: the rows up to the last connected one in
/// `order`, and one more.
pub fn rows_shown(order: &[usize], values: &[RowValues]) -> usize {
    let last = order.iter().rposition(|slot| values[*slot].connected);
    last.map_or(1, |last| last + 2)
        .min(MAX_MODULATION_CONNECTIONS)
}

/// `source_strings_`: "-" and every modulation source, in natural order.
pub fn source_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names: Vec<&'static str> = std::iter::once(NO_CONNECTION)
            .chain(
                sas_preset::engine::modulation_sources()
                    .into_iter()
                    .map(|name| &*Box::leak(name.into_boxed_str())),
            )
            .collect();
        names.sort_by(|a, b| sas_preset::user_folders::compare_natural(a, b));
        names
    })
}

/// `destination_strings_`: "-" and every modulation destination, in
/// natural order.
pub fn destination_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names: Vec<&'static str> = std::iter::once(NO_CONNECTION)
            .chain(sas_preset::engine::modulation_destinations())
            .collect();
        names.sort_by(|a, b| sas_preset::user_folders::compare_natural(a, b));
        names
    })
}

/// `kDestinationSubMenuPrefixes`: the destination list's groups, an empty
/// prefix being a divider, and whether a group's names are local
/// (`local_description`).
const DESTINATION_GROUPS: &[(&str, &str, bool)] = &[
    ("", "", false),
    ("osc_1_", "Oscillator 1", true),
    ("osc_2_", "Oscillator 2", true),
    ("osc_3_", "Oscillator 3", true),
    ("sample_", "Sample", false),
    ("filter_1_", "Filter 1", true),
    ("filter_2_", "Filter 2", true),
    ("filter_fx_", "Filter FX", true),
    ("", "", false),
    ("lfo_", "LFOs", false),
    ("random_", "Randoms", false),
    ("env_", "Envelopes", false),
    ("modulation_", "Mod Matrix", false),
    ("", "", false),
    ("chorus_", "Chorus", false),
    ("compressor_", "Compressor", false),
    ("delay_", "Delay", false),
    ("distortion_", "Distortion", false),
    ("phaser_", "Phaser", false),
    ("flanger_", "Flanger", false),
    ("reverb_", "Reverb", false),
    ("delay_", "Delay", false),
    ("eq_", "Equalizer", false),
];

/// `source_popup_items_`: "-" then each source's menu name, ids being
/// indices into [`source_names`].
pub fn source_menu() -> Vec<MenuItem> {
    source_names()
        .iter()
        .enumerate()
        .map(|(index, name)| {
            if index == 0 {
                MenuItem::new(0, NO_CONNECTION)
            } else {
                MenuItem::new(index as i32, menu_source_display_name(name))
            }
        })
        .collect()
}

/// `destination_popup_items_`: "-", the "Global" group of destinations
/// without a group's prefix (`createMiscSubMenu`), then each group
/// (`createSubMenuForParameterPrefix`) or divider. Ids are indices into
/// [`destination_names`].
pub fn destination_menu() -> Vec<DualMenuGroup> {
    let names = destination_names();
    let display = |name: &str, local: bool| {
        let info = Host::info(name);
        if local {
            info.local_description.clone()
        } else {
            info.display_name.clone()
        }
    };
    let mut groups = vec![DualMenuGroup::item(0, NO_CONNECTION)];
    let misc = names
        .iter()
        .enumerate()
        .filter(|(_, name)| {
            **name != NO_CONNECTION
                && !DESTINATION_GROUPS
                    .iter()
                    .any(|(prefix, _, _)| !prefix.is_empty() && name.starts_with(prefix))
        })
        .map(|(index, name)| MenuItem::new(index as i32, display(name, false)))
        .collect();
    groups.push(DualMenuGroup::new("Global", misc));
    for &(prefix, group, local) in DESTINATION_GROUPS {
        if prefix.is_empty() {
            groups.push(DualMenuGroup::divider());
            continue;
        }
        let items = names
            .iter()
            .enumerate()
            .filter(|(_, name)| name.starts_with(prefix))
            .map(|(index, name)| MenuItem::new(index as i32, display(name, local)))
            .collect();
        groups.push(DualMenuGroup::new(group, items));
    }
    groups
}

/// The tab's state and controls.
pub struct MatrixTab {
    rows: Vec<Row>,
    /// `selected_index_`: the row whose remap shows.
    selected: usize,
    scroll: ScrollBar,
    /// `row_order_`, sorted only when the routes change or a heading is
    /// pressed (`sort`).
    order: Vec<usize>,
    sort_column: SortColumn,
    sort_ascending: bool,
    /// Each row's source and destination selector values, indices into the
    /// names. A row can hold one without the other, unconnected, until the
    /// routes next change (`updateDisplay`).
    selectors: Vec<[usize; 2]>,
    /// The routes as the rows last showed them (`updateModulations` runs
    /// when they change).
    routes: Option<Vec<Route>>,
    /// Each row's bipolar, bypass and stereo, and the synth state's
    /// generation, as last drawn: a state load that changes a row's toggle
    /// selects that row (`setAllValues` → `buttonClicked` → `rowSelected`).
    toggles: Vec<[bool; 3]>,
    generation: Option<u64>,
    /// Whether each row's morph curve was last drawn bipolar.
    curves_bipolar: std::cell::RefCell<Vec<bool>>,
    grid_size_x: Slider,
    grid_size_y: Slider,
    /// `current_file_`: the LFO file last loaded into a curve.
    current_file: Option<PathBuf>,
    /// The popup browser asked for since the last call: the slot, the
    /// browser's bounds and the section's (`textMouseDown`).
    browse_request: Option<(usize, Rect, Rect)>,
}

impl Default for MatrixTab {
    fn default() -> MatrixTab {
        MatrixTab {
            rows: (0..MAX_MODULATION_CONNECTIONS).map(Row::new).collect(),
            selected: 0,
            scroll: ScrollBar::default(),
            order: (0..MAX_MODULATION_CONNECTIONS).collect(),
            sort_column: SortColumn::Number,
            sort_ascending: true,
            selectors: vec![[0; 2]; MAX_MODULATION_CONNECTIONS],
            routes: None,
            toggles: vec![[false; 3]; MAX_MODULATION_CONNECTIONS],
            generation: None,
            curves_bipolar: std::cell::RefCell::new(vec![false; MAX_MODULATION_CONNECTIONS]),
            grid_size_x: grid_slider(local::MATRIX_GRID_SIZE_X, "X Grid"),
            grid_size_y: grid_slider(local::MATRIX_GRID_SIZE_Y, "Y Grid"),
            current_file: None,
            browse_request: None,
        }
    }
}

impl MatrixTab {
    /// The row whose remap shows.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The rows in list order.
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    pub fn sort_column(&self) -> (SortColumn, bool) {
        (self.sort_column, self.sort_ascending)
    }

    /// `getCurrentFile`.
    pub fn current_file(&self) -> Option<PathBuf> {
        self.current_file.clone()
    }

    /// The popup browser asked for since the last call: the slot whose
    /// curve it loads, the browser's bounds and the section's.
    pub fn take_browse_request(&mut self) -> Option<(usize, Rect, Rect)> {
        self.browse_request.take()
    }

    /// `loadFile`: an existing `file` becomes the current file, and its
    /// shape, named after it, replaces slot `slot`'s curve.
    pub fn load_file(&mut self, model: &dyn EditorModel, slot: usize, file: &FsPath) {
        if !file.exists() {
            return;
        }
        self.current_file = Some(file.to_path_buf());
        popup_browser::load_line_file(model, LineTarget::Remap(slot), file);
    }

    /// The remap section (`paintBackground`'s part for it, then its
    /// controls): the selected row's curve, the header that sets it up, and
    /// the LFO file strip.
    #[allow(clippy::too_many_arguments)]
    fn show_remap(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        scope: Scope,
        overlay: &mut Overlay,
        layout: &MatrixLayout,
        routes: &[Route],
    ) {
        let remap = layout.remap_layout(&scope);
        let rounding = scope.value(ValueId::WidgetRoundedCorner) as i32 as f32;
        let popup_background = scope.color(ColorId::PopupSelectorBackground);
        paint_widget_background(pixels, &scope, remap.editor);
        pixels.fill_rounded_rect(
            remap.preset_selector.bounds,
            scope.value(ValueId::WidgetRoundedCorner),
            popup_background,
        );
        pixels.fill_rounded_rect(
            Rect::new(
                remap.paint.x,
                remap.paint.y,
                remap.paint_pattern.right() - remap.paint.x,
                remap.paint.height,
            ),
            rounding,
            popup_background,
        );
        let grid = Rect::new(
            remap.grid_size_x.x,
            remap.grid_size_x.y,
            (remap.grid_size_y.right() - remap.grid_size_x.x).max(1),
            remap.grid_size_x.height,
        );
        pixels.fill_rounded_rect(grid, rounding, popup_background);
        let (face, size, colour) = label_font(&scope);
        pixels.text_centred("-", grid, face, size, colour);

        let slot = self.selected;
        let editor = LineEditor {
            target: LineTarget::Remap(slot),
        };
        let mut panel = Panel::new(ui, host, pixels, scope, overlay, layout.remap);
        panel.text_centred(
            &format!("MOD REMAP {}", slot + 1),
            remap.name,
            Face::Title,
            remap.name_size,
            scope.color(ColorId::HeadingText),
        );
        let controls = LineControls {
            editor: &editor,
            grid_size_x: &self.grid_size_x,
            grid_size_y: &self.grid_size_y,
            paint: local::MATRIX_PAINT,
            paint_pattern: local::MATRIX_PAINT_PATTERN,
            phase: None,
            active: routes.iter().any(|route| route.slot == slot),
        };
        let controls_layout = LineControlsLayout {
            editor: remap.editor,
            grid_size_x: remap.grid_size_x,
            grid_size_y: remap.grid_size_y,
            paint: remap.paint,
            paint_pattern: remap.paint_pattern,
            smooth: remap.smooth,
            preset_selector: remap.preset_selector,
        };
        show_line_controls(&mut panel, &controls, &controls_layout);

        // prevClicked and nextClicked: getShiftedFile among the user's LFOs.
        let preset = remap.preset_selector;
        let ui_colours = ShapeColours::ui(&panel.scope);
        let mut shift = 0;
        for (shape, rect, step) in [
            (Shape::Prev, preset.previous, -1),
            (Shape::Next, preset.next, 1),
        ] {
            let button = ShapeButton {
                shape,
                colours: ui_colours,
            };
            if panel
                .shape_button(("matrix remap file", step), button, rect)
                .clicked
            {
                shift = step;
            }
        }
        let model = host.model();
        if shift != 0 {
            let directories = user_folders::root(model.data_directory().as_deref())
                .map(|root| user_folders::directories(&root, Kind::Lfos))
                .unwrap_or_default();
            let files = user_folders::files_of_type(&directories, popup_browser::LFO_EXTENSIONS);
            let current = self.current_file();
            if let Some(file) = user_folders::shifted_file(files, current.as_deref(), shift) {
                self.load_file(model, slot, &file);
            }
        }
        // textMouseDown: the browser to the right of the strip, at its top.
        let text = panel.pixels.rect(preset.text.into());
        let response = panel.ui.interact(
            text,
            panel.ui.scope_id().with("matrix remap name"),
            egui::Sense::click(),
        );
        if response.hovered() && panel.ui.input(|input| input.pointer.primary_pressed()) {
            /// `textMouseDown`'s `kBrowserWidth` and `kBrowserHeight`.
            const BROWSER_WIDTH: f32 = 500.0;
            const BROWSER_HEIGHT: f32 = 250.0;
            let ratio = scope.size_ratio();
            let browser = Rect::new(
                preset.bounds.right(),
                preset.bounds.y,
                (BROWSER_WIDTH * ratio) as i32,
                (BROWSER_HEIGHT * ratio) as i32,
            );
            self.browse_request = Some((slot, browser, layout.bounds));
        }
        panel.finish();
    }

    /// Whether row `slot`'s morph curve was last drawn bipolar.
    pub fn curve_bipolar(&self, slot: usize) -> bool {
        self.curves_bipolar.borrow()[slot]
    }

    /// Each row's values for sorting, from the host and the selectors.
    fn row_values(&self, host: &Host, routes: &[Route]) -> Vec<RowValues> {
        (0..MAX_MODULATION_CONNECTIONS)
            .map(|slot| {
                let controls = &self.rows[slot];
                RowValues {
                    slot,
                    source: self.selectors[slot][0],
                    destination: self.selectors[slot][1],
                    bipolar: host.value(controls.bipolar.name) != 0.0,
                    stereo: host.value(controls.stereo.name) != 0.0,
                    morph: host.value(controls.power.name),
                    amount: host.value(controls.amount.name),
                    connected: routes.iter().any(|route| route.slot == slot),
                }
            })
            .collect()
    }

    fn sort(&mut self, host: &Host, routes: &[Route]) {
        let values = self.row_values(host, routes);
        sort_rows(
            &mut self.order,
            &values,
            self.sort_column,
            self.sort_ascending,
        );
    }

    /// `updateModulations` when the routes change: every row's selectors
    /// show its slot's route (`updateDisplay`), and the rows are sorted.
    fn follow_routes(&mut self, host: &Host, routes: &[Route]) {
        if self.routes.as_deref() == Some(routes) {
            return;
        }
        for slot in 0..MAX_MODULATION_CONNECTIONS {
            let route = routes.iter().find(|route| route.slot == slot);
            self.selectors[slot] = match route {
                Some(route) => [
                    index_of(source_names(), &route.source),
                    index_of(destination_names(), &route.destination),
                ],
                None => [0, 0],
            };
        }
        self.routes = Some(routes.to_vec());
        self.sort(host, routes);
    }

    /// `ModulationMatrixRow::sliderValueChanged` for a selector: the slot's
    /// old route goes and the new one, once both are chosen, takes the same
    /// slot (one engine change; the host hears nothing, D18). The row is
    /// selected.
    fn choose(&mut self, host: &Host, slot: usize, destination: bool, id: i32) {
        let names = if destination {
            destination_names()
        } else {
            source_names()
        };
        let Some(index) = usize::try_from(id)
            .ok()
            .filter(|&index| index < names.len())
        else {
            return;
        };
        // Slider::setValue notifies only a change.
        if self.selectors[slot][usize::from(destination)] == index {
            return;
        }
        let [old_source, old_destination] = self.selectors[slot];
        self.selectors[slot][usize::from(destination)] = index;
        let [source, destination_index] = self.selectors[slot];
        let was_connected = old_source > 0 && old_destination > 0;
        let complete = source > 0 && destination_index > 0;
        if was_connected || complete {
            let route = complete.then(|| {
                (
                    source_names()[source],
                    destination_names()[destination_index],
                )
            });
            let connected = host.model().set_slot_route(slot, route);
            if complete && !connected {
                // isInvalidConnection cleared the names; updateDisplay shows
                // "-" again.
                self.selectors[slot] = [0, 0];
            }
        }
        self.selected = slot;
    }

    /// A state load that changed rows' toggles selects the last of them in
    /// `setAllValues`' order, its button map's: by name, so
    /// `modulation_10_bipolar` comes before `modulation_1_bipolar` and
    /// `modulation_2_bipolar` after `modulation_19_stereo`.
    fn follow_load(&mut self, host: &Host) {
        let generation = host.model().edit_generation();
        let toggles: Vec<[bool; 3]> = self
            .rows
            .iter()
            .map(|row| {
                [&row.bipolar, &row.bypass, &row.stereo]
                    .map(|button| host.value(button.name) != 0.0)
            })
            .collect();
        if self.generation.is_some_and(|last| last != generation) {
            let last = (0..MAX_MODULATION_CONNECTIONS)
                .flat_map(|slot| {
                    let row = &self.rows[slot];
                    let (old, new) = (self.toggles[slot], toggles[slot]);
                    [&row.bipolar, &row.bypass, &row.stereo]
                        .into_iter()
                        .enumerate()
                        .filter(move |&(toggle, _)| old[toggle] != new[toggle])
                        .map(move |(_, button)| (button.name, slot))
                })
                .max_by(|a, b| a.0.cmp(b.0));
            if let Some((_, slot)) = last {
                self.selected = slot;
            }
        }
        self.generation = Some(generation);
        self.toggles = toggles;
    }

    /// `ModulationMatrix::mouseDown` on the headings: the column under `x`
    /// (section pixels), toggling the direction if it is already sorted.
    fn press_heading(&mut self, layout: &MatrixLayout, scope: &Scope, x: i32) {
        let width = layout.bounds.width;
        let x_padding = (width as f32 * PADDING_WIDTH_PERCENT) as i32;
        let y_padding = (scope.size_ratio() * 3.0) as i32;
        let component_height = layout.row_height - 2 * y_padding;
        let source_width = (width as f32 * SOURCE_WIDTH_PERCENT) as i32;
        let destination_width = (width as f32 * DESTINATION_WIDTH_PERCENT) as i32;
        let bipolar_x = (source_width as f32 + 1.5 * x_padding as f32) as i32;
        let stereo_x = bipolar_x + component_height + x_padding;
        let morph_x = stereo_x + component_height + x_padding;
        let amount_x = morph_x + component_height + x_padding;
        let destination_x =
            (width as f32 - destination_width as f32 - 1.5 * x_padding as f32) as i32;
        let column = if x < x_padding {
            SortColumn::Number
        } else if x < bipolar_x {
            SortColumn::Source
        } else if x < stereo_x {
            SortColumn::Bipolar
        } else if x < morph_x {
            SortColumn::Stereo
        } else if x < amount_x {
            SortColumn::Morph
        } else if x < destination_x {
            SortColumn::Amount
        } else {
            SortColumn::Destination
        };
        self.sort_ascending = column != self.sort_column || !self.sort_ascending;
        self.sort_column = column;
    }

    /// `ModulationMatrix::paintBackgroundShadow`, `paintBackground` and its
    /// rows, then the scroll bar and the borders.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        top: &Scope,
        overlay: &mut Overlay,
        bounds: Rect,
    ) {
        let scope = top.within(SKIN);
        let layout = MatrixLayout::new(&scope, bounds);
        let routes = host.model().modulations();
        self.follow_routes(host, &routes);
        self.follow_load(host);
        if let Some((slot, destination, id)) = overlay.matrix_choice.take() {
            self.choose(host, slot, destination, id);
        }

        // The headings take presses (`mouseDown`).
        let headings = Rect::new(bounds.x, bounds.y, bounds.width, layout.title_width);
        let heading_response = ui.interact(
            pixels.rect(headings.into()),
            ui.scope_id().with("modulation matrix headings"),
            egui::Sense::click(),
        );
        if heading_response.is_pointer_button_down_on()
            && ui.input(|input| input.pointer.any_pressed())
        {
            if let Some(pos) = ui.input(|input| input.pointer.interact_pos()) {
                let x = ((pos.x - pixels.area().min.x) * pixels.pixels_per_point()) as i32;
                self.press_heading(&layout, &scope, x - bounds.x);
                self.sort(host, &routes);
            }
        }

        let values = self.row_values(host, &routes);
        let shown = rows_shown(&self.order, &values);

        paint_tab_shadow(pixels, &scope, layout.matrix);
        paint_tab_shadow(pixels, &scope, layout.remap);
        paint_body(pixels, &scope, layout.matrix);
        paint_body(pixels, &scope, layout.remap);
        paint_headings(pixels, &scope, &layout, self.sort_column);
        // The list's background under the heading.
        pixels.fill_rounded_rect_clipped(
            layout.matrix,
            scope.value(ValueId::BodyRounding),
            scope.color(ColorId::Background),
            Rect::new(
                bounds.x,
                bounds.y + layout.title_width,
                bounds.width,
                bounds.height - layout.title_width,
            ),
        );

        let total = layout.list_height(shown) as f32;
        let visible = layout.viewport.height as f32;
        let id = ui.scope_id().with("modulation matrix scroll");
        let ppp = pixels.pixels_per_point();
        self.scroll.handle(
            ui,
            pixels,
            id,
            layout.viewport,
            layout.scroll_bar,
            total,
            visible,
            ppp,
        );
        let scroll = self.scroll.position.round() as i32;

        let order = self.order.clone();
        let mut selected = None;
        pixels.with_clip(RectF::from(layout.viewport), |clipped| {
            let mut panel = Panel::new(ui, host, clipped, scope, overlay, layout.viewport);
            for (position, &slot) in order.iter().take(shown).enumerate() {
                let row = RowLayout::new(&scope, layout.row(position, scroll));
                let route = routes.iter().find(|route| route.slot == slot);
                if self.show_row(&mut panel, &row, slot, route) {
                    selected = Some(slot);
                }
            }
            panel.finish();
        });
        if let Some(slot) = selected {
            self.selected = slot;
        }
        paint_readouts(host, pixels, &scope, &layout, &order, &routes, scroll);

        self.scroll
            .paint(pixels, &scope, layout.scroll_bar, total, visible);
        paint_border(pixels, &scope, layout.matrix);
        paint_border(pixels, &scope, layout.remap);
        self.show_remap(ui, host, pixels, scope, overlay, &layout, &routes);
    }

    /// `ModulationMatrixRow`: its background (`paintBackground`), the
    /// selection highlight, its controls, and the off overlay while it has
    /// no connection (`setActive(connected())`). True when the row is to be
    /// selected: pressed (`mouseDown`), a toggle clicked (`buttonClicked`)
    /// or a slider moved (`sliderValueChanged`).
    fn show_row(
        &self,
        panel: &mut Panel,
        row: &RowLayout,
        slot: usize,
        route: Option<&Route>,
    ) -> bool {
        let scope = panel.scope;
        let pixels = panel.pixels;
        let host = panel.host;
        // mouseDown on the row's background; its toggles select it when
        // clicked (buttonClicked), its sliders when they move
        // (sliderValueChanged), its selectors on a choice.
        let pressed_at = panel.ui.input(|input| {
            input
                .pointer
                .primary_pressed()
                .then(|| input.pointer.interact_pos())
                .flatten()
        });
        let mut select = pressed_at.is_some_and(|pos| {
            let within = |rect: Rect| pixels.rect(rect.into()).contains(pos);
            within(row.bounds)
                && ![
                    row.source,
                    row.destination,
                    row.bypass,
                    row.bipolar,
                    row.stereo,
                    row.power,
                    row.amount,
                ]
                .into_iter()
                .any(within)
        });

        pixels.fill_rect(row.bounds, scope.color(ColorId::Body));
        let rounding = scope.value(ValueId::WidgetRoundedCorner) as i32 as f32;
        for rect in [row.source, row.destination] {
            pixels.fill_rounded_rect(
                rect,
                rounding,
                scope.color(ColorId::TextComponentBackground),
            );
        }
        // paintKnobShadows: SynthSlider::drawShadow behind the power curve.
        pixels.fill_rounded_rect(
            row.power,
            scope.value(ValueId::WidgetRoundedCorner),
            scope.color(ColorId::WidgetBackground),
        );
        let mut layer = Layer::new();
        // highlight_, an OpenGlQuad over the row.
        if slot == self.selected {
            layer.quad(Quad::new(
                QuadKind::Color,
                row.bounds.into(),
                scope.color(ColorId::LightenScreen),
            ));
        }
        let connected = route.is_some();
        let [source, destination] = self.selectors[slot];
        let texts = [
            (menu_source_display_name(source_names()[source]), row.source),
            (
                menu_source_display_name(destination_names()[destination]),
                row.destination,
            ),
        ];
        for (text, rect) in &texts {
            selector_text(&mut layer, &scope, *rect, text);
        }
        layer.paint(panel.pixels, RectF::from(row.bounds));

        // ModulationSelector::mouseDown: its list below it.
        for (rect, is_destination) in [(row.source, false), (row.destination, true)] {
            let response = panel.ui.interact(
                pixels.rect(rect.into()),
                panel
                    .ui
                    .scope_id()
                    .with(("matrix selector", slot, is_destination)),
                egui::Sense::click(),
            );
            let pressed = response.is_pointer_button_down_on()
                && panel.ui.input(|input| input.pointer.any_pressed());
            if !pressed {
                continue;
            }
            let position = egui::pos2(rect.x as f32, rect.bottom() as f32);
            let owner = MenuOwner::MatrixSelector {
                slot,
                destination: is_destination,
            };
            if is_destination {
                /// `ModulationSelector::mouseDown`'s `kWideWidth`.
                const WIDE_WIDTH: f32 = 420.0;
                let width = (WIDE_WIDTH * scope.size_ratio()) as i32;
                panel.overlay.dual_menu = Some(DualMenu::new(
                    destination_menu(),
                    None,
                    0,
                    position,
                    width,
                    owner,
                ));
            } else {
                panel.overlay.menu = Some(Menu::new(source_menu(), position, owner));
            }
        }

        let controls = &self.rows[slot];
        // buttonClicked: the curve follows the toggle at once, whenever the
        // host applies it.
        let was_bipolar = host.value(controls.bipolar.name) != 0.0;
        select |= panel.button(&controls.bypass, row.bypass);
        let bipolar_clicked = panel.button(&controls.bipolar, row.bipolar);
        select |= bipolar_clicked;
        select |= panel.button(&controls.stereo, row.stereo);
        let bipolar = was_bipolar != bipolar_clicked;
        self.curves_bipolar.borrow_mut()[slot] = bipolar;
        let mut power = controls.power;
        power.bipolar = bipolar;
        power.active = connected;
        select |= panel.slider_changed(&power, row.power);

        let mut amount = controls.amount;
        amount.active = connected;
        // updateDisplayValue: a linear or indexed destination's amount
        // reads in its units.
        if let Some(route) = route {
            let info = Host::info(&route.destination);
            if matches!(info.value_scale, ValueScale::Linear | ValueScale::Indexed) {
                amount.format.display_multiply = Some(info.max - info.min);
            }
        }
        select |= panel.slider_changed(&amount, row.amount);

        if !connected {
            panel.off_overlay(row.bounds);
        }
        select
    }
}

/// `ModulationMeterReadouts`: for each connected row, a bar along the
/// meter strip at the top of its destination box (`getMeterBounds`, 2
/// pixels high), from the box's centre towards the side of the slot's
/// amount now (`modulation_amount_n`), left then right channel, added.
///
/// See [`readout_bar`].
fn paint_readouts(
    host: &Host,
    pixels: &PixelPainter,
    scope: &Scope,
    layout: &MatrixLayout,
    order: &[usize],
    routes: &[Route],
    scroll: i32,
) {
    let Some(telemetry) = host.model().telemetry() else {
        return;
    };
    let values: Vec<Option<[f32; 4]>> = telemetry.read(|snapshot| {
        amount_names()
            .iter()
            .map(|name| snapshot.status_by_name(name))
            .collect()
    });
    let viewport = RectF::from(layout.viewport);
    let mut batch = QuadBatch::additive();
    for (channel, colour) in [
        (0, scope.color(ColorId::ModulationMeterLeft)),
        (1, scope.color(ColorId::ModulationMeterRight)),
    ] {
        for (position, &slot) in order.iter().enumerate() {
            if !routes.iter().any(|route| route.slot == slot) {
                continue;
            }
            let destination = RowLayout::new(scope, layout.row(position, scroll)).destination;
            let value = values[slot].map_or(0.0, |value| value[channel]);
            let Some(rect) = readout_bar(destination, value, layout.viewport) else {
                continue;
            };
            let mut bar = Quad::new(QuadKind::Bar, rect, colour);
            bar.shader_values[0] = rect.width / 2.0;
            bar.shader_values[1] = rect.height;
            batch.push(bar);
        }
    }
    if !batch.is_empty() {
        batch.paint(pixels, viewport);
    }
}

/// One readout's bar for a row's `destination` box, with the amount now
/// `value`, in a list of `viewport`'s size; None when it has no length.
///
/// `positionBar(i, centre, y, x - centre, 0)` at the meter strip's bottom
/// edge, widened downwards by `kBarHorizontalVertex`'s `width_percent`:
/// `bar_width` is `64 · 2 / height`, so the bar is the strip's 2 pixels
/// thick, or 4 / width of the height if that is more.
pub fn readout_bar(destination: Rect, value: f32, viewport: Rect) -> Option<RectF> {
    /// `getMeterBounds`' height.
    const METER_HEIGHT: f32 = 2.0;
    let (width, height) = (viewport.width as f32, viewport.height as f32);
    let width_percent = (4.0 / width).max(2.0 * METER_HEIGHT / height);
    let thickness = width_percent * height / 2.0;
    let value = if value == CLEAR_VALUE { 0.0 } else { value };
    let t = (0.5 * (value + 1.0)).clamp(0.0, 1.0);
    let left = destination.x as f32;
    let right = destination.right() as f32;
    let centre = (left + right) / 2.0;
    let x = left + (right - left) * t;
    let length = (x - centre).abs();
    (length > 0.0).then(|| {
        RectF::new(
            centre.min(x),
            destination.y as f32 + METER_HEIGHT,
            length,
            thickness,
        )
    })
}

/// `"modulation_amount_" + n` for each slot.
fn amount_names() -> &'static [String] {
    static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        (1..=MAX_MODULATION_CONNECTIONS)
            .map(|n| format!("modulation_amount_{n}"))
            .collect()
    })
}

/// The position of `name` in `names` (`setValueFromName`), or 0.
fn index_of(names: &[&str], name: &str) -> usize {
    names
        .iter()
        .position(|candidate| *candidate == name)
        .unwrap_or(0)
}

/// `ModulationMatrix::paintBackground`'s headings: a lighten-screen line
/// before each column and its name centred above it, the `sorted`
/// column's in the regular face, the rest light.
fn paint_headings(pixels: &PixelPainter, scope: &Scope, layout: &MatrixLayout, sorted: SortColumn) {
    let (x, y) = (layout.bounds.x, layout.bounds.y);
    let title = layout.title_width;
    let columns = layout.columns(scope);
    let lighten = scope.color(ColorId::LightenScreen);
    for column in columns {
        pixels.fill_rect(Rect::new(x + column, y, 1, title), lighten);
    }
    let [number_end, bipolar_x, stereo_x, morph_x, amount_x, destination_x] = columns;
    let width = layout.bounds.width;
    let x_padding = (width as f32 * PADDING_WIDTH_PERCENT) as i32;
    let size = title as f32 * 0.4;
    let colour = scope.color(ColorId::TextComponentText);
    let text = |label: &str, left: f32, right: f32, face: Face| {
        pixels.text_centred(
            label,
            RectF::new(x as f32 + left, y as f32, right - left, title as f32),
            face,
            size,
            colour,
        );
    };
    let face = |column: SortColumn| {
        if column == sorted {
            Face::Regular
        } else {
            Face::Light
        }
    };
    text("#", 0.0, number_end as f32, face(SortColumn::Number));
    text(
        "SOURCE",
        number_end as f32,
        bipolar_x as f32,
        face(SortColumn::Source),
    );
    text(
        "BIPOLAR",
        bipolar_x as f32,
        stereo_x as f32,
        face(SortColumn::Bipolar),
    );
    text(
        "STEREO",
        stereo_x as f32,
        morph_x as f32,
        face(SortColumn::Stereo),
    );
    text(
        "MORPH",
        morph_x as f32,
        amount_x as f32,
        face(SortColumn::Morph),
    );
    text(
        "AMOUNT",
        amount_x as f32,
        destination_x as f32,
        face(SortColumn::Amount),
    );
    text(
        "DESTINATION",
        destination_x as f32 - 0.5 * x_padding as f32,
        width as f32,
        face(SortColumn::Destination),
    );
}

/// `ModulationSelector` drawn by `TextLookAndFeel::drawRotarySlider`: its
/// selection's name centred at half its height. It is an `OpenGlSlider`, not
/// a `SynthSlider`, so the skin's text size and offset and the inactive
/// colour don't apply.
fn selector_text(layer: &mut Layer, scope: &Scope, bounds: Rect, text: &str) {
    /// `drawRotarySlider`'s `kTextPercentage`.
    const TEXT_PERCENTAGE: f32 = 0.5;
    let font_size = bounds.height as f32 * TEXT_PERCENTAGE;
    let colour = scope.color(ColorId::TextComponentText);
    layer.text_centred(text, RectF::from(bounds), Face::Light, font_size, colour);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::Skin;

    /// `ModulationMatrixRow::resized` at upstream's default size: the
    /// bounds ref-gui gives row 0 (laid out at the section's 704 width).
    #[test]
    fn a_row_lays_out_as_upstreams() {
        let skin = Skin::default();
        let scope = Scope::top(&skin, 1.0).within(SKIN);
        let row = RowLayout::new(&scope, Rect::new(88, 81, 704, 34));
        assert_eq!(row.bypass, Rect::new(91, 88, 28, 19));
        assert_eq!(row.source, Rect::new(122, 84, 140, 28));
        assert_eq!(row.bipolar, Rect::new(276, 84, 28, 28));
        assert_eq!(row.stereo, Rect::new(318, 88, 56, 19));
        assert_eq!(row.power, Rect::new(388, 84, 28, 28));
        assert_eq!(row.amount, Rect::new(439, 86, 181, 24));
        assert_eq!(row.destination, Rect::new(624, 84, 154, 28));
    }

    fn values(connected: &[usize]) -> Vec<RowValues> {
        (0..MAX_MODULATION_CONNECTIONS)
            .map(|slot| RowValues {
                slot,
                source: 0,
                destination: 0,
                bipolar: false,
                stereo: false,
                morph: 0.0,
                amount: 0.0,
                connected: connected.contains(&slot),
            })
            .collect()
    }

    /// `checkNumModulationsShown` and `sort`'s last `compareConnected`:
    /// connected slots first, then the rest, each by slot; one row more
    /// than connected shows.
    #[test]
    fn connected_rows_come_first_with_one_empty_row_after() {
        let mut order: Vec<usize> = (0..MAX_MODULATION_CONNECTIONS).collect();
        let none = values(&[]);
        sort_rows(&mut order, &none, SortColumn::Number, true);
        assert_eq!(&order[..3], [0, 1, 2]);
        assert_eq!(rows_shown(&order, &none), 1);
        let two = values(&[5, 2]);
        sort_rows(&mut order, &two, SortColumn::Number, true);
        assert_eq!(&order[..4], [2, 5, 0, 1]);
        assert_eq!(rows_shown(&order, &two), 3);
    }

    /// Sorting by a column is stable on the current order, then connected
    /// rows go first: by amount descending, ties keep their places.
    #[test]
    fn rows_sort_by_a_column_then_connected_first() {
        let mut v = values(&[1, 2, 3]);
        v[1].amount = 0.2;
        v[2].amount = 0.9;
        v[3].amount = 0.2;
        v[7].amount = 1.0;
        let mut order: Vec<usize> = (0..MAX_MODULATION_CONNECTIONS).collect();
        sort_rows(&mut order, &v, SortColumn::Amount, false);
        assert_eq!(&order[..4], [2, 1, 3, 7]);
        sort_rows(&mut order, &v, SortColumn::Number, false);
        assert_eq!(&order[..4], [3, 2, 1, 63]);
    }

    /// The selectors' names (`source_strings_`, `destination_strings_`):
    /// "-" first, then natural order; and the destination list's groups.
    #[test]
    fn the_lists_are_in_natural_order_with_upstreams_groups() {
        let sources = source_names();
        assert_eq!(sources[0], "-");
        let position = |name: &str| sources.iter().position(|s| *s == name).unwrap();
        assert!(position("env_2") < position("env_6"));
        assert!(position("lfo_2") < position("lfo_8"));
        let destinations = destination_names();
        let at = |name: &str| destinations.iter().position(|s| *s == name).unwrap();
        assert!(at("modulation_2_amount") < at("modulation_10_amount"));

        let menu = destination_menu();
        assert_eq!(menu[0], DualMenuGroup::item(0, "-"));
        assert_eq!(menu[1].name, "Global");
        assert_eq!(menu[2], DualMenuGroup::divider());
        let osc_1 = &menu[3];
        assert_eq!(osc_1.name, "Oscillator 1");
        let level = osc_1
            .items
            .iter()
            .find(|item| item.id as usize == at("osc_1_level"))
            .unwrap();
        assert_eq!(level.name, Host::info("osc_1_level").local_description);
        let matrix = menu
            .iter()
            .find(|group| group.name == "Mod Matrix")
            .unwrap();
        // Each slot's amount and power.
        assert_eq!(matrix.items.len(), 2 * MAX_MODULATION_CONNECTIONS);
        assert_eq!(menu.iter().filter(|group| group.name == "Delay").count(), 2);
        assert!(menu[1]
            .items
            .iter()
            .all(|item| !destinations[item.id as usize].starts_with("osc_")));
    }

    /// `ModulationMeterReadouts::updatePositions`: the bar runs from the
    /// destination box's centre to the amount's side, 2 pixels thick below
    /// the 2-pixel strip, and a clear or zero amount draws nothing.
    #[test]
    fn readouts_run_from_the_centre_to_the_amount() {
        let box_ = Rect::new(100, 50, 200, 26);
        let viewport = Rect::new(0, 0, 704, 421);
        assert_eq!(
            readout_bar(box_, 1.0, viewport),
            Some(RectF::new(200.0, 52.0, 100.0, 2.0))
        );
        assert_eq!(
            readout_bar(box_, -0.5, viewport),
            Some(RectF::new(150.0, 52.0, 50.0, 2.0))
        );
        assert_eq!(readout_bar(box_, 3.0, viewport).unwrap().width, 100.0);
        assert_eq!(readout_bar(box_, 0.0, viewport), None);
        assert_eq!(readout_bar(box_, CLEAR_VALUE, viewport), None);
        // A wide, short list: 4 / width of the height, more than 2 pixels.
        let wide = readout_bar(box_, 1.0, Rect::new(0, 0, 100, 400)).unwrap();
        assert_eq!(wide.height, 8.0);
    }
}
