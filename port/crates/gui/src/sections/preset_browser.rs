// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preset browser, shown in place of the main sections: the preset's
//! name, author and comments, the search box, the style filters, the folder
//! list and the preset list. Ports `PresetBrowser` and `PresetList`
//! (`vital/src/interface/editor_sections/preset_browser.cpp`).

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use egui::Key;
use sas_preset::preset_files;
use sas_preset::user_folders::{self, compare_natural, Kind, PRESET_EXTENSION};

use super::{MenuOwner, Panel};
use crate::controls::button::SynthButton;
use crate::controls::icon;
use crate::controls::path::{EndCapStyle, JointStyle, Transform};
use crate::controls::paths;
use crate::controls::scroll_bar::ScrollBar;
use crate::controls::text_editor;
use crate::fonts::Face;
use crate::interface::EditorModel;
use crate::layout::Rect;
use crate::local::{PRESET_STYLE_FILTERS, PRESET_STYLE_NAMES};
use crate::paint::{PixelPainter, RectF};
use crate::popup::{Menu, MenuItem};
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::scope::Scope;
use crate::section::{paint_body, paint_border, paint_tab_shadow};
use crate::sections::selection_list::{Selection, SelectionEvent, SelectionList};
use crate::skin::{ColorId, Section, ValueId};
use crate::text::{Justify, Overflow};

/// The browser's skin override (`setSkinOverride(Skin::kPresetBrowser)`).
pub const SKIN: &[Section] = &[Section::PresetBrowser];

/// `PresetBrowser`'s padding constants, before the size ratio.
const LEFT_PADDING: f32 = 24.0;
const TOP_PADDING: f32 = 24.0;
const MIDDLE_PADDING: f32 = 15.0;
const NAME_FONT_HEIGHT: f32 = 26.0;
const AUTHOR_FONT_HEIGHT: f32 = 19.0;
const COMMENTS_FONT_HEIGHT: f32 = 15.0;
/// `PresetList::resized`'s `kScrollBarWidth`.
const SCROLL_BAR_WIDTH: f32 = 15.0;

/// `PresetList`'s column widths, as fractions of the list's width.
const STAR_WIDTH_PERCENT: f32 = 0.04;
const NAME_WIDTH_PERCENT: f32 = 0.35;
const STYLE_WIDTH_PERCENT: f32 = 0.18;
const AUTHOR_WIDTH_PERCENT: f32 = 0.25;
const DATE_WIDTH_PERCENT: f32 = 0.18;
/// `PresetList::kRowSizeHeightPercent`.
const ROW_SIZE_HEIGHT_PERCENT: f32 = 0.04;

/// `PresetBrowser::resized` and `PresetList`'s geometry, in interface
/// coordinates.
#[derive(Clone, Copy, Debug)]
pub struct PresetBrowserLayout {
    pub bounds: Rect,
    /// `getInfoRect`: the preset's name, author and comments.
    pub info: Rect,
    /// `getSearchRect`: the search box, style filters and folder list.
    pub search_area: Rect,
    /// `preset_text_` and `author_text_`, with their text sizes.
    pub preset_text: Rect,
    pub preset_text_size: f32,
    pub author_text: Rect,
    pub author_text_size: f32,
    /// The divider lines' left edge, width and rows.
    pub text_x: i32,
    pub text_width: i32,
    pub author_line_y: i32,
    pub comments_line_y: i32,
    pub search: Rect,
    pub styles: [Rect; 9],
    pub folders: Rect,
    pub comments: Rect,
    /// `preset_list_`, with its title strip's height and its row height.
    pub list: Rect,
    pub title_width: i32,
    pub row_height: i32,
}

impl PresetBrowserLayout {
    /// `PresetBrowser::resized` with the browser at `bounds`; `top` is the
    /// interface's scope.
    pub fn new(top: &Scope, bounds: Rect) -> PresetBrowserLayout {
        const BROWSE_WIDTH_RATIO: f32 = 0.68;
        const SEARCH_BOX_ROW_HEIGHT_RATIO: f32 = 1.3;
        const INFO_HEIGHT_RATIO: f32 = 0.43;
        let scope = top.within(SKIN);
        let ratio = scope.size_ratio();
        let (width, height) = (bounds.width, bounds.height);
        let padding = scope.value(ValueId::LargePadding) as i32;

        let list_width = (width as f32 * BROWSE_WIDTH_RATIO) as i32;
        let list = Rect::new(width - list_width - padding, 0, list_width, height);
        let row_height = (height as f32 * ROW_SIZE_HEIGHT_PERCENT) as i32;
        let title_width = scope.value(ValueId::TitleWidth) as i32;

        // getInfoRect subtracts the unrounded padding.
        let info = Rect::new(
            0,
            0,
            (list.x as f32 - scope.value(ValueId::LargePadding)) as i32,
            (height as f32 * INFO_HEIGHT_RATIO) as i32,
        );
        let search_y = info.bottom() + padding;
        let search_area = Rect::new(0, search_y, info.width, height - search_y);

        let top_padding = (TOP_PADDING * ratio) as i32;
        let left_padding = (LEFT_PADDING * ratio) as i32;
        let middle_padding = (MIDDLE_PADDING * ratio) as i32;
        let name_y = info.y + top_padding;
        let name_height = (NAME_FONT_HEIGHT * ratio) as i32;
        let author_y = name_y + name_height + middle_padding;
        let author_height = (AUTHOR_FONT_HEIGHT * ratio) as i32;
        let text_x = info.x + left_padding;
        let text_width = info.width - 2 * left_padding;
        let preset_text = Rect::new(
            text_x,
            name_y - middle_padding,
            text_width,
            name_height + 2 * middle_padding,
        );
        let author_text = Rect::new(
            text_x,
            author_y,
            text_width / 2,
            author_height + 2 * middle_padding,
        );

        let search_height = (SEARCH_BOX_ROW_HEIGHT_RATIO * row_height as f32) as i32;
        let search = Rect::new(
            search_area.x + left_padding,
            search_area.y + top_padding,
            text_width,
            search_height,
        );
        let style_y = search.bottom() + top_padding;

        let widget_margin = scope.widget_margin() as i32;
        let style_height = row_height;
        let style_x = search_area.x + left_padding;
        let style_width = search_area.width - 2 * left_padding + widget_margin;
        let per_row = (PRESET_STYLE_NAMES.len() / 3) as i32;
        let styles = std::array::from_fn(|i| {
            let i = i as i32;
            let column = i % per_row;
            let x = style_x + (style_width * column) / per_row;
            let next_x = style_x + (style_width * (column + 1)) / per_row;
            let y = style_y + (i / per_row) * (style_height + widget_margin);
            Rect::new(x, y, next_x - x - widget_margin, style_height)
        });

        let folder_y = style_y + 3 * style_height + 2 * widget_margin + top_padding + 1;
        let folders = Rect::new(
            style_x,
            folder_y,
            text_width,
            search_area.bottom() - top_padding - folder_y - 1,
        );

        // setCommentsBounds, with the store button hidden.
        let top_info_height =
            ((NAME_FONT_HEIGHT + AUTHOR_FONT_HEIGHT + MIDDLE_PADDING * 4.0) * ratio) as i32;
        let comments_y = info.y + top_info_height + top_padding;
        let comments = Rect::new(
            text_x,
            comments_y,
            text_width,
            info.bottom() - comments_y - top_padding,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        PresetBrowserLayout {
            bounds,
            info: info.translated(dx, dy),
            search_area: search_area.translated(dx, dy),
            preset_text: preset_text.translated(dx, dy),
            preset_text_size: name_height as f32,
            author_text: author_text.translated(dx, dy),
            author_text_size: author_height as f32,
            text_x: text_x + dx,
            text_width,
            author_line_y: author_y + dy,
            comments_line_y: author_y + author_height + 2 * middle_padding + dy,
            search: search.translated(dx, dy),
            styles: styles.map(|style| style.translated(dx, dy)),
            folders: folders.translated(dx, dy),
            comments: comments.translated(dx, dy),
            list: list.translated(dx, dy),
            title_width,
            row_height,
        }
    }
}

/// `PresetList::paintBackground`: the list's background, its title strip
/// with the column dividers, and the column titles.
fn paint_list_background(pixels: &PixelPainter, scope: &Scope, layout: &PresetBrowserLayout) {
    let list = layout.list;
    let title_width = layout.title_width;
    let rounding = scope.value(ValueId::BodyRounding);
    pixels.fill_rounded_rect(list, rounding, scope.color(ColorId::WidgetBackground));

    let width = list.width as f32;
    let star_width = (STAR_WIDTH_PERCENT * width) as i32;
    let name_width = (NAME_WIDTH_PERCENT * width) as i32;
    let style_width = (STYLE_WIDTH_PERCENT * width) as i32;
    let author_width = (AUTHOR_WIDTH_PERCENT * width) as i32;
    let date_width = (DATE_WIDTH_PERCENT * width) as i32;
    let text_padding = layout.row_height / 2;

    // The body colour's rounded top, clipped to the title strip.
    let strip = Rect::new(list.x, list.y, list.width, title_width);
    pixels.with_clip(RectF::from(strip), |pixels| {
        let top = Rect::new(list.x, list.y, list.width, title_width * 2);
        pixels.fill_rounded_rect(top, rounding, scope.color(ColorId::Body));
    });

    let lighten = scope.color(ColorId::LightenScreen);
    for x in [
        star_width,
        star_width + name_width,
        star_width + name_width + style_width,
        list.width - date_width,
    ] {
        pixels.fill_rect(Rect::new(list.x + x, list.y, 1, title_width), lighten);
    }

    let text = scope.color(ColorId::TextComponentText);
    let size = title_width as f32 * 0.5;
    let star_draw_width = title_width as f32 * 0.8;
    let star_y = (title_width as f32 - star_draw_width) / 2.0;
    let star = RectF::new(
        list.x as f32 + (star_width as f32 - star_draw_width) / 2.0,
        list.y as f32 + star_y,
        star_draw_width,
        star_draw_width,
    );
    icon::fill_path_to_fit(pixels, "preset list star", paths::star(), star, text);

    let style = (Face::Regular, size, text);
    let title = |x: i32, w: i32| Rect::new(list.x + x, list.y, w, title_width);
    let left = |label: &str, rect: Rect| {
        pixels.text_line(label, rect, style, Justify::Left, Overflow::Ellipsis);
    };
    left("Name", title(text_padding + star_width, name_width));
    let style_x = star_width + name_width + text_padding;
    left("Style", title(style_x, style_width));
    left("Author", title(style_x + style_width, author_width));
    pixels.text_line(
        "Date",
        title(list.width - date_width, date_width - text_padding),
        style,
        Justify::Right,
        Overflow::Ellipsis,
    );

    paint_border(pixels, scope, list);
}

/// `PresetBrowser::paintBackgroundShadow` and `paintBackground`, with its
/// children's backgrounds.
pub fn paint_background(pixels: &PixelPainter, top: &Scope, layout: &PresetBrowserLayout) {
    let scope = top.within(SKIN);
    paint_tab_shadow(pixels, &scope, layout.search_area);
    paint_tab_shadow(pixels, &scope, layout.info);
    // PresetList::paintBackgroundShadow.
    paint_tab_shadow(pixels, &scope, layout.list);
    for area in [layout.search_area, layout.info] {
        paint_body(pixels, &scope, area);
        paint_border(pixels, &scope, area);
    }

    let lighten = scope.color(ColorId::LightenScreen);
    for y in [layout.author_line_y, layout.comments_line_y] {
        pixels.fill_rect(Rect::new(layout.text_x, y, layout.text_width, 1), lighten);
    }

    pixels.fill_rounded_rect(
        RectF::from(layout.folders).reduced(-1.0),
        scope.value(ValueId::WidgetRoundedCorner),
        scope.color(ColorId::WidgetBackground),
    );

    paint_list_background(pixels, &scope, layout);
    text_editor::paint_background(pixels, &scope, layout.search);
    text_editor::paint_background(pixels, &scope, layout.comments);
}

/// `PresetList::Column`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Star,
    Name,
    Style,
    Author,
    Date,
}

/// A preset the list shows, with what `PresetInfoCache` keeps of it.
#[derive(Clone, Debug)]
struct Entry {
    path: PathBuf,
    /// The file name without its extension.
    name: String,
    author: String,
    /// Lower case, as the cache keeps it.
    style: String,
    created: Option<i64>,
}

impl Entry {
    fn read(path: PathBuf) -> Entry {
        Entry {
            name: path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
            author: preset_files::author_from_file(&path),
            style: preset_files::style_from_file(&path).to_lowercase(),
            created: preset_files::creation_time(&path),
            path,
        }
    }
}

/// JUCE's `compareNatural`, which ignores case.
fn natural(a: &str, b: &str) -> Ordering {
    compare_natural(&a.to_lowercase(), &b.to_lowercase())
}

/// The `PresetList` comparators, ascending. Favourites come first, and the
/// newer file (`FileDateAscendingComparator`).
fn compare(column: Column, favourites: &BTreeSet<PathBuf>, a: &Entry, b: &Entry) -> Ordering {
    match column {
        Column::Star => favourites
            .contains(&b.path)
            .cmp(&favourites.contains(&a.path)),
        Column::Name => natural(&a.name, &b.name),
        Column::Style => natural(&a.style, &b.style),
        Column::Author => natural(&a.author, &b.author),
        Column::Date => b.created.cmp(&a.created),
    }
}

/// `PresetBrowser` with its `PresetList`: what the browser shows and how
/// it answers the mouse and keys.
pub struct PresetBrowser {
    /// Whether `presets` has been read since the editor window opened
    /// (upstream builds the list with the editor).
    loaded: bool,
    /// `presets_`, in the order the last sort left them.
    presets: Vec<Entry>,
    /// `filtered_presets_`, as indices into `presets`.
    filtered: Vec<usize>,
    favourites: BTreeSet<PathBuf>,
    sort_column: Column,
    sort_ascending: bool,
    /// `filter_string_`'s source: the search box's text.
    search: String,
    /// `filter_styles_`.
    filter_styles: BTreeSet<String>,
    scroll: ScrollBar,
    /// `hover_preset_`: the filtered row under the pointer.
    hover: Option<usize>,
    /// `selected_preset_`.
    selected: Option<PathBuf>,
    /// `external_preset_`: the preset loaded from outside the list (Open
    /// External, Export, the arrows stepping through its folder), or none
    /// after a pick from the list or Initialize Preset.
    external: Option<PathBuf>,
    /// `folder_list_`.
    folders: SelectionList,
    /// `PresetList::current_folder_`: the folder last chosen in the folder
    /// list, whose presets a reload reads.
    current_folder: Option<PathBuf>,
    /// `click_preset_`: the row the menu was opened on.
    click_preset: Option<PathBuf>,
    /// `renaming_preset_` and the rename editor's bounds and text.
    rename: Option<Rename>,
    /// A preset the menu asked to delete, for the interface's
    /// `DeleteSection`.
    delete_request: Option<PathBuf>,
}

/// The rename editor over a row's name.
#[derive(Clone, Debug)]
struct Rename {
    file: PathBuf,
    bounds: Rect,
    text: String,
    /// Whether it has been given the keyboard yet.
    focused: bool,
}

/// `SynthPresetSelector::newPresetSelected` and `importPreset`: a preset
/// that didn't load says why in a native message box.
pub fn alert_load_error(ctx: &egui::Context, error: &str) {
    let message = format!("There was an error open the preset. {error}");
    crate::controls::file_dialog::alert(ctx, "Error opening preset", &message);
}

/// The row menu's choices (`PresetList::MenuOptions`).
const OPEN_FILE_LOCATION: i32 = 1;
const RENAME: i32 = 2;
const DELETE: i32 = 3;

/// The row menu's choice, for the list to answer.
fn row_menu_choice() -> egui::Id {
    egui::Id::unique("preset row menu")
}

fn row_menu_chosen(_: &crate::host::Host, ctx: &egui::Context, id: i32) {
    ctx.data_mut(|data| data.insert_temp(row_menu_choice(), id));
}

/// `FileNameFilter`: characters a file name can't have.
fn file_name_filter(text: &mut String) {
    text.retain(|c| !"<>?*/|\\[]\":".contains(c));
}

impl Default for PresetBrowser {
    fn default() -> PresetBrowser {
        PresetBrowser {
            loaded: false,
            presets: Vec::new(),
            filtered: Vec::new(),
            favourites: BTreeSet::new(),
            sort_column: Column::Name,
            sort_ascending: true,
            search: String::new(),
            filter_styles: BTreeSet::new(),
            scroll: ScrollBar::default(),
            hover: None,
            selected: None,
            external: None,
            folders: {
                let mut folders = SelectionList::with_favourites_option();
                folders.set_passthrough_folder_name(Kind::Presets.folder_name());
                folders
            },
            current_folder: None,
            click_preset: None,
            rename: None,
            delete_request: None,
        }
    }
}

impl PresetBrowser {
    /// Reads the presets again when next needed, as a new editor window's
    /// browser does.
    pub fn reload(&mut self) {
        self.loaded = false;
    }

    fn ensure_loaded(&mut self, model: &dyn EditorModel) {
        if self.loaded {
            return;
        }
        self.load_presets(model);
    }

    /// `PresetBrowser::loadPresets`: the search cleared, every preset read
    /// again, sorted and shown unfiltered (the style buttons keep their
    /// state until they next filter).
    pub fn load_presets(&mut self, model: &dyn EditorModel) {
        self.loaded = true;
        self.search.clear();
        self.favourites = model.favourites().into_iter().collect();
        self.reload_presets(model);
        self.filter(&BTreeSet::new());
        let root = user_folders::root(model.data_directory().as_deref());
        self.folders.set_favourites(self.favourites.clone());
        self.folders.set_selections(
            root.map(|root| user_folders::directories(&root, Kind::Presets))
                .unwrap_or_default(),
        );
    }

    /// `LoadSave::getAllPresets`.
    fn all_presets(model: &dyn EditorModel) -> Vec<PathBuf> {
        user_folders::root(model.data_directory().as_deref())
            .map(|root| preset_files::all_files(&root, Kind::Presets, PRESET_EXTENSION))
            .unwrap_or_default()
    }

    /// `PresetList::setPresets`.
    fn set_presets(&mut self, presets: Vec<PathBuf>) {
        self.presets = presets.into_iter().map(Entry::read).collect();
        self.sort();
    }

    /// `PresetList::reloadPresets`: the current folder's presets, or every
    /// preset without one.
    fn reload_presets(&mut self, model: &dyn EditorModel) {
        let presets = match &self.current_folder {
            Some(folder) if folder.is_dir() => preset_files::files_under(folder, PRESET_EXTENSION),
            _ => PresetBrowser::all_presets(model),
        };
        self.set_presets(presets);
    }

    /// The folder list's choice (`newSelection`, `allSelected`,
    /// `favoritesSelected`).
    fn folder_chosen(&mut self, model: &dyn EditorModel, event: SelectionEvent) {
        match event {
            SelectionEvent::Path(folder) => {
                if folder.is_dir() {
                    self.current_folder = Some(folder);
                    self.reload_presets(model);
                }
            }
            // The folder list holds folders only, and takes no added folders.
            SelectionEvent::DoubleClicked(_)
            | SelectionEvent::AddFolder
            | SelectionEvent::RemoveFolder(_) => {}
            SelectionEvent::All => self.set_presets(PresetBrowser::all_presets(model)),
            SelectionEvent::Favourites => {
                let favourites = model.favourites().into_iter().collect::<BTreeSet<_>>();
                let presets = PresetBrowser::all_presets(model)
                    .into_iter()
                    .filter(|path| favourites.contains(path))
                    .collect();
                self.set_presets(presets);
            }
        }
    }

    /// A preset the row menu asked to delete, once.
    pub fn take_delete_request(&mut self) -> Option<PathBuf> {
        self.delete_request.take()
    }

    /// `PresetBrowser::fileDeleted`.
    pub fn file_deleted(&mut self, model: &dyn EditorModel) {
        self.load_presets(model);
    }

    /// `respondToMenuCallback`.
    fn respond_to_menu(&mut self, layout: &PresetBrowserLayout, choice: i32) {
        let Some(file) = self.click_preset.clone() else {
            return;
        };
        match choice {
            OPEN_FILE_LOCATION => crate::controls::file_dialog::reveal(&file),
            RENAME => {
                let Some(row) = self
                    .filtered
                    .iter()
                    .position(|&i| self.presets[i].path == file)
                else {
                    return;
                };
                let list = layout.list;
                let width = list.width as f32;
                let y = list.y + layout.title_width + row as i32 * layout.row_height
                    - self.scroll.position.round() as i32;
                let bounds = Rect::new(
                    list.x + (STAR_WIDTH_PERCENT * width) as i32,
                    y,
                    (NAME_WIDTH_PERCENT * width) as i32,
                    layout.row_height,
                );
                let text = file
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.rename = Some(Rename {
                    file,
                    bounds,
                    text,
                    focused: false,
                });
            }
            DELETE => self.delete_request = Some(file),
            _ => {}
        }
    }

    /// `finishRename`: a name that isn't blank renames the file to it as
    /// typed (blank only is checked trimmed), keeping its extension, and the
    /// list reads its presets again.
    fn finish_rename(&mut self, model: &dyn EditorModel) {
        let Some(rename) = self.rename.take() else {
            return;
        };
        if rename.text.trim().is_empty() || !rename.file.exists() {
            return;
        }
        let mut name = rename.text.clone();
        if let Some(extension) = rename.file.extension() {
            name.push('.');
            name.push_str(&extension.to_string_lossy());
        }
        if let Some(parent) = rename.file.parent() {
            let _ = std::fs::rename(&rename.file, parent.join(name));
        }
        self.reload_presets(model);
    }

    /// The folder list's rows, as it shows them.
    pub fn folder_rows(&self) -> &[Selection] {
        self.folders.rows()
    }

    /// The preset paths in the order the list shows them.
    pub fn shown(&self) -> Vec<PathBuf> {
        self.filtered
            .iter()
            .map(|&index| self.presets[index].path.clone())
            .collect()
    }

    /// `PresetList::sort`: a stable sort of the list's presets by the sort
    /// column, then the filter again.
    fn sort(&mut self) {
        let (column, favourites) = (self.sort_column, &self.favourites);
        if self.sort_ascending {
            self.presets
                .sort_by(|a, b| compare(column, favourites, a, b));
        } else {
            self.presets
                .sort_by(|a, b| compare(column, favourites, b, a));
        }
        let styles = self.filter_styles.clone();
        self.filter(&styles);
    }

    /// `PresetList::filter`: presets whose name or author contains every
    /// word of the search (ignoring case) and, with styles chosen, whose
    /// style is one of them.
    fn filter(&mut self, styles: &BTreeSet<String>) {
        self.filter_styles = styles.clone();
        let search = self.search.to_lowercase();
        let tokens: Vec<&str> = search.split(' ').collect();
        self.filtered = (0..self.presets.len())
            .filter(|&index| {
                let entry = &self.presets[index];
                if !styles.is_empty() && !styles.contains(&entry.style) {
                    return false;
                }
                let name = entry.name.to_lowercase();
                let author = entry.author.to_lowercase();
                tokens
                    .iter()
                    .all(|token| name.contains(token) || author.contains(token))
            })
            .collect();
    }

    /// `PresetBrowser::filterPresets`: the search and the style buttons that
    /// are on.
    fn filter_presets(&mut self, styles: BTreeSet<String>) {
        self.filter(&styles);
    }

    fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        self.filtered
            .iter()
            .position(|&index| &self.presets[index].path == selected)
    }

    /// `PresetBrowser::externalPresetLoaded`.
    pub fn external_preset_loaded(&mut self, file: Option<PathBuf>) {
        self.external = file;
    }

    /// `SynthPresetSelector::newPresetSelected`: a preset picked in the list
    /// (or shifted to) loads, and the arrows step through the list again.
    fn new_preset_selected(&mut self, ctx: &egui::Context, model: &dyn EditorModel, path: PathBuf) {
        self.external = None;
        if let Err(error) = model.load_preset_file(&path) {
            alert_load_error(ctx, &error);
        }
        self.selected = Some(path);
    }

    /// `PresetBrowser::jumpToPreset`: with an external preset whose folder
    /// exists, the preset `by` places from it among those beside it;
    /// otherwise the list's selection moves `by` rows, wrapping round
    /// (`PresetList::shiftSelectedPreset`).
    pub fn jump_to_preset(&mut self, ctx: &egui::Context, model: &dyn EditorModel, by: i32) {
        if let Some(external) = self.external.clone() {
            if external.parent().is_some_and(Path::is_dir) {
                if let Some(next) = user_folders::step(&external, by) {
                    let _ = model.load_preset_file(&next);
                    self.external = Some(next);
                }
                return;
            }
        }
        self.ensure_loaded(model);
        let count = self.filtered.len() as i32;
        if count == 0 {
            return;
        }
        let selected = self.selected_index().map_or(-1, |index| index as i32);
        let index = (selected + count + by).rem_euclid(count) as usize;
        let path = self.presets[self.filtered[index]].path.clone();
        self.new_preset_selected(ctx, model, path);
    }

    /// `visibilityChanged`: the search box empties (and the list refilters)
    /// whenever the browser shows or hides.
    pub fn visibility_changed(&mut self, model: &dyn EditorModel) {
        self.ensure_loaded(model);
        self.search.clear();
        let styles = self.filter_styles.clone();
        self.filter_presets(styles);
    }

    /// Draws the browser's parts over its background and handles the mouse
    /// and keys. Returns true when Escape asks to hide it (`keyPressed`).
    pub fn show(&mut self, panel: &mut Panel, layout: &PresetBrowserLayout, keys: bool) -> bool {
        let scope = panel.scope;
        let model = panel.host.model();
        self.ensure_loaded(model);
        if let Some(choice) = panel
            .ui
            .ctx()
            .data_mut(|data| data.remove_temp::<i32>(row_menu_choice()))
        {
            self.respond_to_menu(layout, choice);
        }
        self.show_info(panel, layout);

        let search = text_editor::single_line(
            panel.ui,
            panel.pixels,
            &scope,
            "preset search",
            layout.search,
            &mut self.search,
            "Search",
        );
        if search.escape {
            // textEditorEscapeKeyPressed.
            self.search.clear();
        }

        let mut styles = BTreeSet::new();
        for ((name, label), bounds) in PRESET_STYLE_FILTERS
            .iter()
            .zip(PRESET_STYLE_NAMES)
            .zip(layout.styles)
        {
            panel.button(&SynthButton::text(name, label), bounds);
            if panel.host.value(name) != 0.0 {
                styles.insert(label.to_lowercase());
            }
        }
        // textEditorTextChanged and buttonClicked: filterPresets.
        if search.changed || search.escape || styles != self.filter_styles {
            self.filter_presets(styles);
        }

        let mut set_favourite = |path: &Path, favourite: bool| model.set_favourite(path, favourite);
        if let Some(event) =
            self.folders
                .show(panel, layout.folders, "preset folders", &mut set_favourite)
        {
            self.folder_chosen(model, event);
        }

        self.show_list(panel, layout);

        // keyPressed, while the search box doesn't have the keyboard and no
        // overlay's field (`keys` false) does.
        if search.has_focus || !keys {
            return false;
        }
        let (escape, previous, next) = panel.ui.input(|input| {
            (
                input.key_pressed(Key::Escape),
                input.key_pressed(Key::ArrowUp) || input.key_pressed(Key::ArrowLeft),
                input.key_pressed(Key::ArrowDown) || input.key_pressed(Key::ArrowRight),
            )
        });
        if previous {
            self.jump_to_preset(&panel.ui.ctx().clone(), model, -1);
        }
        if next {
            self.jump_to_preset(&panel.ui.ctx().clone(), model, 1);
        }
        escape
    }

    /// `loadPresetInfo`: the synth's name and author, or their prompts in
    /// the lightened body colour, and its comments.
    fn show_info(&self, panel: &mut Panel, layout: &PresetBrowserLayout) {
        let scope = panel.scope;
        let model = panel.host.model();
        let lighten = scope
            .color(ColorId::Body)
            .overlaid_with(scope.color(ColorId::LightenScreen));
        let regular = scope.color(ColorId::BodyText);
        let name = model.preset_name();
        let author = model.preset_author();
        for (text, prompt, rect, size) in [
            (
                &name,
                "Preset name",
                layout.preset_text,
                layout.preset_text_size,
            ),
            (
                &author,
                "Author",
                layout.author_text,
                layout.author_text_size,
            ),
        ] {
            let (text, colour) = if text.is_empty() {
                (prompt, lighten)
            } else {
                (text.as_str(), regular)
            };
            panel.pixels.text_line(
                text,
                rect,
                (Face::Light, size, colour),
                Justify::Left,
                Overflow::Fit,
            );
        }

        // comments_: read-only and multi-line, in the light face at
        // kCommentsFontHeight, inside its indents.
        let comments = model.preset_comments();
        if !comments.is_empty() {
            let indent = scope.value(ValueId::LabelBackgroundRounding) as i32;
            let bounds = layout.comments;
            let inner = Rect::new(
                bounds.x + indent,
                bounds.y + indent,
                bounds.width - 2 * indent,
                bounds.height - 2 * indent,
            );
            let pixels = panel.pixels;
            let rect = pixels.rect(inner.into());
            let font = Face::Light.at(pixels.points(COMMENTS_FONT_HEIGHT * scope.size_ratio()));
            let galley = pixels
                .painter()
                .layout(comments, font, regular.to_egui(), rect.width());
            pixels
                .painter()
                .with_clip_rect(pixels.rect(bounds.into()))
                .galley(rect.min, galley, regular.to_egui());
        }
    }

    /// `PresetList`: the rows, their highlights and the scroll bar, and the
    /// list's mouse handling.
    fn show_list(&mut self, panel: &mut Panel, layout: &PresetBrowserLayout) {
        let scope = panel.scope;
        let model = panel.host.model();
        let list = layout.list;
        let title_width = layout.title_width;
        let row_height = layout.row_height.max(1);
        let view = Rect::new(
            list.x,
            list.y + title_width,
            list.width,
            list.height - title_width,
        );
        let scroll_bar_width = (SCROLL_BAR_WIDTH * scope.size_ratio()) as i32;
        let bar = Rect::new(
            list.right() - scroll_bar_width,
            view.y,
            scroll_bar_width,
            view.height,
        );
        let total = (row_height * self.filtered.len() as i32) as f32;
        let visible = view.height as f32;

        let ppp = panel.pixels.pixels_per_point();
        let id = panel.ui.scope_id().with("preset list scroll bar");
        self.scroll
            .handle(panel.ui, panel.pixels, id, view, bar, total, visible, ppp);
        let position = self.scroll.position.round() as i32;

        // getRowFromPosition, for the pointer.
        let pointer = panel.ui.input(|input| input.pointer.hover_pos());
        let list_rect = panel.pixels.rect(list.into());
        let to_pixels = |pos: egui::Pos2| {
            let rect = panel.pixels.area();
            ((pos.x - rect.min.x) * ppp, (pos.y - rect.min.y) * ppp)
        };
        let row_at = |y: f32| {
            ((y - list.y as f32 + position as f32 - title_width as f32) / row_height as f32).floor()
                as i32
        };
        self.hover = pointer
            .filter(|pos| list_rect.contains(*pos))
            .map(|pos| row_at(to_pixels(pos).1))
            .filter(|&row| row >= 0 && (row as usize) < self.filtered.len())
            .map(|row| row as usize);

        let response = panel.ui.interact(
            list_rect,
            panel.ui.scope_id().with("preset list"),
            egui::Sense::click(),
        );
        if response.clicked() {
            if let Some((x, y)) = response.interact_pointer_pos().map(to_pixels) {
                self.left_click(
                    &panel.ui.ctx().clone(),
                    model,
                    list,
                    title_width,
                    x - list.x as f32,
                    y,
                    row_at(y),
                );
            }
        }
        // menuClick: Open File Location, and Rename and Delete for a file
        // that can be written.
        if response.secondary_clicked() {
            if let Some(hover) = self.hover {
                let file = self.presets[self.filtered[hover]].path.clone();
                let mut items = vec![MenuItem::new(OPEN_FILE_LOCATION, "Open File Location")];
                let writable =
                    std::fs::metadata(&file).is_ok_and(|meta| !meta.permissions().readonly());
                if writable {
                    items.push(MenuItem::new(RENAME, "Rename"));
                    items.push(MenuItem::new(DELETE, "Delete"));
                }
                self.click_preset = Some(file);
                let pos = response.interact_pointer_pos().unwrap_or_default();
                panel.overlay.menu =
                    Some(Menu::new(items, pos, MenuOwner::Setting(row_menu_chosen)));
            }
        }
        // The wheel finishes a rename (mouseWheelMove).
        if self.rename.is_some()
            && panel.ui.rect_contains_pointer(list_rect)
            && panel.ui.input(|input| input.smooth_scroll_delta.y != 0.0)
        {
            self.finish_rename(model);
        }

        self.paint_rows(panel, layout, view, position);
        self.show_rename(panel, model);
    }

    /// `PresetList::leftClick`, at `x` from the list's left and `y` in
    /// interface pixels.
    #[allow(clippy::too_many_arguments)]
    fn left_click(
        &mut self,
        ctx: &egui::Context,
        model: &dyn EditorModel,
        list: Rect,
        title_width: i32,
        x: f32,
        y: f32,
        row: i32,
    ) {
        let width = list.width as f32;
        let star_right = (STAR_WIDTH_PERCENT * width) as i32 as f32;
        if y <= (list.y + title_width) as f32 {
            let name_right = star_right + (NAME_WIDTH_PERCENT * width) as i32 as f32;
            let style_right = name_right + (STYLE_WIDTH_PERCENT * width) as i32 as f32;
            let author_right = style_right + (AUTHOR_WIDTH_PERCENT * width) as i32 as f32;
            let column = if x < star_right {
                Column::Star
            } else if x < name_right {
                Column::Name
            } else if x < style_right {
                Column::Style
            } else if x < author_right {
                Column::Author
            } else {
                Column::Date
            };
            self.sort_ascending = column != self.sort_column || !self.sort_ascending;
            self.sort_column = column;
            self.sort();
            return;
        }
        if row < 0 || row as usize >= self.filtered.len() {
            return;
        }
        let path = self.presets[self.filtered[row as usize]].path.clone();
        if x < star_right {
            let favourite = !self.favourites.contains(&path);
            model.set_favourite(&path, favourite);
            if favourite {
                self.favourites.insert(path);
            } else {
                self.favourites.remove(&path);
            }
        } else {
            self.new_preset_selected(ctx, model, path);
        }
    }

    /// The rename editor: Return or losing the keyboard renames
    /// (`textEditorReturnKeyPressed`, `textEditorFocusLost`). Escape hides it,
    /// which takes its keyboard, so it renames too.
    fn show_rename(&mut self, panel: &mut Panel, model: &dyn EditorModel) {
        let Some(rename) = &mut self.rename else {
            return;
        };
        let scope = panel.scope;
        if !rename.focused {
            text_editor::focus_select_all(panel.ui, "preset rename", &rename.text);
            rename.focused = true;
        }
        text_editor::paint_background(panel.pixels, &scope, rename.bounds);
        let events = text_editor::single_line(
            panel.ui,
            panel.pixels,
            &scope,
            "preset rename",
            rename.bounds,
            &mut rename.text,
            "",
        );
        file_name_filter(&mut rename.text);
        if events.return_pressed || events.escape || events.lost_focus {
            if events.escape {
                panel.ui.memory_mut(|memory| memory.stop_text_input());
            }
            self.finish_rename(model);
        }
    }

    /// `loadBrowserCache` and `renderOpenGlComponents`: each visible row's
    /// star, name, style, author and date, then the selected and hovered
    /// rows' additive highlights and the scroll bar.
    fn paint_rows(
        &self,
        panel: &mut Panel,
        layout: &PresetBrowserLayout,
        view: Rect,
        position: i32,
    ) {
        let scope = panel.scope;
        let list = layout.list;
        let row_height = layout.row_height.max(1);
        let clipped = panel
            .pixels
            .painter()
            .with_clip_rect(panel.pixels.rect(view.into()));
        let pixels = PixelPainter::new(
            &clipped,
            panel.pixels.area(),
            panel.pixels.pixels_per_point(),
        );

        let width = list.width as f32;
        let text_padding = (row_height as f32 / 2.0) as i32;
        let star_width = (STAR_WIDTH_PERCENT * width) as i32;
        let name_x = text_padding + star_width;
        let name_width = (NAME_WIDTH_PERCENT * width) as i32;
        let style_x = name_x + name_width;
        let style_width = (STYLE_WIDTH_PERCENT * width) as i32;
        let author_x = style_x + style_width;
        let author_width = (AUTHOR_WIDTH_PERCENT * width) as i32;
        let date_width = (DATE_WIDTH_PERCENT * width) as i32;
        let date_x = list.width - date_width + text_padding;

        let text_colour = scope.color(ColorId::TextComponentText);
        let star_unselected = text_colour.with_multiplied_alpha(0.5);
        let star_selected = scope.color(ColorId::WidgetPrimary1);
        let style = (Face::Light, row_height as f32 * 0.5, text_colour);
        let star_draw_width = row_height as f32 * 0.8;
        let star_y = (row_height as f32 - star_draw_width) / 2.0;
        // The star scaled to fit its square, stroked one pixel wide.
        let star = paths::star();
        let fit = star.transform_to_fit((0.0, 0.0, star_draw_width, star_draw_width));
        let outline = star.stroked(1.0 / fit.scale, JointStyle::Curved, EndCapStyle::Butt);

        let first = (position / row_height).max(0) as usize;
        let last = ((position + view.height) / row_height + 1).max(0) as usize;
        for row in first..last.min(self.filtered.len()) {
            let entry = &self.presets[self.filtered[row]];
            let y = view.y + row as i32 * row_height - position;
            let text = |label: &str, x: i32, w: i32, justify: Justify| {
                let rect = Rect::new(list.x + x, y, w, row_height);
                pixels.text_line(label, rect, style, justify, Overflow::Ellipsis);
            };

            let transform = Transform {
                scale: fit.scale,
                dx: list.x as f32 + (star_width as f32 - star_draw_width) / 2.0 + fit.dx,
                dy: y as f32 + star_y + fit.dy,
            };
            let key = format!("preset row star {row_height}");
            if self.favourites.contains(&entry.path) {
                icon::fill_path(&pixels, &key, star, transform, star_selected);
                icon::fill_path(
                    &pixels,
                    &format!("{key} outline"),
                    &outline,
                    transform,
                    star_selected,
                );
            } else {
                icon::fill_path(
                    &pixels,
                    &format!("{key} outline"),
                    &outline,
                    transform,
                    star_unselected,
                );
            }

            let mut style_name = entry.style.clone();
            if let Some(first) = style_name.get(..1) {
                style_name = first.to_uppercase() + &style_name[1..];
            }
            let date = entry
                .created
                .map(preset_files::date_string)
                .unwrap_or_default();
            text(
                &entry.name,
                name_x,
                name_width - 2 * text_padding,
                Justify::Left,
            );
            text(
                &style_name,
                style_x,
                style_width - 2 * text_padding,
                Justify::Left,
            );
            text(
                &entry.author,
                author_x,
                author_width - 2 * text_padding,
                Justify::Left,
            );
            text(&date, date_x, date_width - 2 * text_padding, Justify::Right);
        }

        // highlight_ and hover_, added to what's under them.
        let row_rect = |row: usize| {
            RectF::new(
                list.x as f32,
                (view.y + row as i32 * row_height - position) as f32,
                list.width as f32,
                row_height as f32,
            )
        };
        let mut highlights = QuadBatch::additive();
        if let Some(selected) = self.selected_index() {
            let colour = scope.color(ColorId::WidgetPrimary1).darker(0.8);
            highlights.push(Quad::new(QuadKind::Color, row_rect(selected), colour));
        }
        if let Some(hover) = self.hover {
            let colour = scope.color(ColorId::LightenScreen);
            highlights.push(Quad::new(QuadKind::Color, row_rect(hover), colour));
        }
        highlights.paint(&pixels, RectF::from(view));

        let total = (row_height * self.filtered.len() as i32) as f32;
        let scroll_bar_width = (SCROLL_BAR_WIDTH * scope.size_ratio()) as i32;
        let bar = Rect::new(
            list.right() - scroll_bar_width,
            view.y,
            scroll_bar_width,
            view.height,
        );
        self.scroll
            .paint(panel.pixels, &scope, bar, total, view.height as f32);
    }
}
