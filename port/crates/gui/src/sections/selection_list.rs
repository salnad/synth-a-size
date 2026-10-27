// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! A scrolling list of folders and files with All and Favorites rows: the
//! preset browser's folder list, and the popup browser's (G7d). Ports
//! `SelectionList` (`vital/src/interface/editor_sections/popup_browser.cpp`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use egui::{PointerButton, Sense};
use sas_preset::user_folders::compare_paths;

use super::{MenuOwner, Panel};
use crate::controls::icon::{self, UpstreamIcon};
use crate::controls::path::{EndCapStyle, JointStyle, Transform};
use crate::controls::paths;
use crate::controls::scroll_bar::ScrollBar;
use crate::fonts::Face;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::popup::{Menu, MenuItem};
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::skin::ColorId;
use crate::text::{Justify, Overflow};

/// `SelectionList::kRowHeight`, before the size ratio.
const ROW_HEIGHT: f32 = 24.0;
/// `SelectionList::kScrollBarWidth`, before the size ratio.
const SCROLL_BAR_WIDTH: f32 = 15.0;

/// A row of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    /// `getAllFile()`: every file.
    All,
    /// `getFavoritesFile()`: the favourite files.
    Favourites,
    /// A folder or file.
    Path(PathBuf),
}

/// What a click chose (`SelectionList::Listener`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionEvent {
    /// `allSelected`.
    All,
    /// `favoritesSelected`.
    Favourites,
    /// `newSelection`: a folder or file.
    Path(PathBuf),
    /// `doubleClickedSelected`: the selected file, clicked again.
    DoubleClicked(PathBuf),
}

/// `SelectionList`'s state.
pub struct SelectionList {
    /// `favorites_option_`: the All and Favorites rows come first.
    favourites_option: bool,
    /// `passthrough_name_`: a folder with this name shows its parent's.
    passthrough_name: Option<&'static str>,
    /// `selections_`, sorted by full path.
    selections: Vec<PathBuf>,
    /// `open_folders_`: open folders and their depths.
    open_folders: BTreeMap<PathBuf, usize>,
    /// `filtered_selections_`.
    filtered: Vec<Selection>,
    /// `filter_string_`'s words, lower-cased.
    filter_words: Vec<String>,
    favourites: BTreeSet<PathBuf>,
    /// `selected_`.
    selected: Option<Selection>,
    /// `hovered_`.
    hovered: Option<usize>,
    scroll: ScrollBar,
}

impl Default for SelectionList {
    /// A plain list, nothing selected (`SelectionList()`).
    fn default() -> SelectionList {
        SelectionList {
            favourites_option: false,
            passthrough_name: None,
            selections: Vec::new(),
            open_folders: BTreeMap::new(),
            filtered: Vec::new(),
            filter_words: Vec::new(),
            favourites: BTreeSet::new(),
            selected: None,
            hovered: None,
            scroll: ScrollBar::default(),
        }
    }
}

impl SelectionList {
    /// A list with All and Favorites first (`addFavoritesOption`), All
    /// selected.
    pub fn with_favourites_option() -> SelectionList {
        SelectionList {
            favourites_option: true,
            selected: Some(Selection::All),
            ..SelectionList::default()
        }
    }

    /// `getPassthroughFolderName`.
    pub fn passthrough_folder_name(&self) -> Option<&'static str> {
        self.passthrough_name
    }

    /// The selections as set (`getSelections`), sorted.
    pub fn selections(&self) -> &[PathBuf] {
        &self.selections
    }

    /// `setSelected`.
    pub fn set_selected(&mut self, selected: Option<Selection>) {
        self.selected = selected;
    }

    /// `resetScrollPosition`.
    pub fn reset_scroll_position(&mut self) {
        self.scroll.position = 0.0;
    }

    /// `filter`: the selections whose names hold every word of `search`,
    /// ignoring case. A selection no longer listed is cleared.
    pub fn set_filter(&mut self, search: &str) {
        self.filter_words = search
            .to_lowercase()
            .split(' ')
            .filter(|word| !word.is_empty())
            .map(str::to_string)
            .collect();
        self.filter();
    }

    /// `selectNext`: the row after the selected one, wrapping round.
    pub fn select_next(&mut self) -> Option<SelectionEvent> {
        if self.filtered.is_empty() {
            return None;
        }
        let index = self.selected_index().map_or(0, |index| index + 1) % self.filtered.len();
        self.select(self.filtered[index].clone())
    }

    /// `selectPrev`: the row before the selected one (the last for none),
    /// wrapping round.
    pub fn select_previous(&mut self) -> Option<SelectionEvent> {
        if self.filtered.is_empty() {
            return None;
        }
        let count = self.filtered.len();
        let index = (self.selected_index().unwrap_or(0) + count - 1) % count;
        self.select(self.filtered[index].clone())
    }

    /// `getSelectedIndex`.
    fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        self.filtered.iter().position(|row| row == selected)
    }

    /// `setPassthroughFolderName`.
    pub fn set_passthrough_folder_name(&mut self, name: &'static str) {
        self.passthrough_name = Some(name);
    }

    pub fn set_favourites(&mut self, favourites: BTreeSet<PathBuf>) {
        self.favourites = favourites;
    }

    /// `setSelections`: the rows, sorted by full path in natural order
    /// ignoring case.
    pub fn set_selections(&mut self, mut selections: Vec<PathBuf>) {
        selections.sort_by(|a, b| compare_paths(a, b));
        self.selections = selections;
        self.filter();
    }

    /// The rows as the list shows them.
    pub fn rows(&self) -> &[Selection] {
        &self.filtered
    }

    pub fn selected(&self) -> Option<&Selection> {
        self.selected.as_ref()
    }

    /// `getFolderDepth`: one deeper than its folder if that folder is open.
    fn folder_depth(&self, path: &Path) -> usize {
        path.parent()
            .and_then(|parent| self.open_folders.get(parent))
            .map_or(0, |depth| depth + 1)
    }

    /// `addSubfolderSelections`, in natural order (upstream takes the file
    /// system's).
    fn add_subfolders(&self, folder: &Path, rows: &mut Vec<Selection>) {
        for child in sub_folders(folder) {
            let open = self.open_folders.contains_key(&child);
            rows.push(Selection::Path(child.clone()));
            if open {
                self.add_subfolders(&child, rows);
            }
        }
    }

    /// `filter`: All and Favorites, then each selection whose name holds
    /// every search word and, under open folders, their folders. A
    /// selection no longer listed is cleared.
    fn filter(&mut self) {
        let mut rows = Vec::new();
        if self.favourites_option {
            rows.push(Selection::All);
            rows.push(Selection::Favourites);
        }
        for selection in &self.selections {
            let name = file_stem(selection).to_lowercase();
            if !self
                .filter_words
                .iter()
                .all(|word| name.contains(word.as_str()))
            {
                continue;
            }
            rows.push(Selection::Path(selection.clone()));
            if self.open_folders.contains_key(selection) {
                self.add_subfolders(selection, &mut rows);
            }
        }
        self.filtered = rows;
        if let Some(selected) = &self.selected {
            if !self.filtered.contains(selected) {
                self.selected = None;
            }
        }
    }

    /// `toggleOpenFolder`: a folder with folders inside opens; an open one
    /// closes.
    fn toggle_open_folder(&mut self, folder: &Path) {
        if self.open_folders.remove(folder).is_none() && !sub_folders(folder).is_empty() {
            let depth = self.folder_depth(folder);
            self.open_folders.insert(folder.to_path_buf(), depth);
        }
        self.filter();
    }

    /// `select`: choosing the selected folder again opens or closes it;
    /// anything else becomes the selection.
    fn select(&mut self, selection: Selection) -> Option<SelectionEvent> {
        if let Selection::Path(path) = &selection {
            if path.is_dir() && self.selected.as_ref() == Some(&selection) {
                let path = path.clone();
                self.toggle_open_folder(&path);
                return None;
            }
        }
        self.selected = Some(selection.clone());
        match selection {
            Selection::All => Some(SelectionEvent::All),
            Selection::Favourites => Some(SelectionEvent::Favourites),
            Selection::Path(path) if path.exists() => Some(SelectionEvent::Path(path)),
            Selection::Path(_) => None,
        }
    }

    /// `selectIcon`: a folder's icon selects it; a file's toggles its
    /// favourite (the All and Favorites icons select them).
    fn select_icon(
        &mut self,
        selection: Selection,
        set_favourite: &mut dyn FnMut(&Path, bool),
    ) -> Option<SelectionEvent> {
        match &selection {
            Selection::Path(path) if !path.is_dir() => {
                let favourite = !self.favourites.contains(path);
                set_favourite(path, favourite);
                if favourite {
                    self.favourites.insert(path.clone());
                } else {
                    self.favourites.remove(path);
                }
                None
            }
            _ => self.select(selection),
        }
    }

    /// The list at `bounds`: its rows, highlights and scroll bar, and its
    /// mouse handling (`mouseMove`, `mouseDown`, the wheel and the scroll
    /// bar). Returns what a click chose; `set_favourite` saves a file's star.
    pub fn show(
        &mut self,
        panel: &mut Panel,
        bounds: Rect,
        salt: &str,
        set_favourite: &mut dyn FnMut(&Path, bool),
    ) -> Option<SelectionEvent> {
        let scope = panel.scope;
        let ratio = scope.size_ratio();
        let row_height = ((ratio * ROW_HEIGHT) as i32).max(1);
        let scroll_bar_width = (SCROLL_BAR_WIDTH * ratio) as i32;
        let bar = Rect::new(
            bounds.right() - scroll_bar_width,
            bounds.y,
            scroll_bar_width,
            bounds.height,
        );
        let total = (row_height * self.filtered.len() as i32) as f32;
        let visible = bounds.height as f32;
        let ppp = panel.pixels.pixels_per_point();
        let id = panel.ui.scope_id().with((salt, "scroll bar"));
        self.scroll
            .handle(panel.ui, panel.pixels, id, bounds, bar, total, visible, ppp);
        let position = self.scroll.position.round() as i32;

        let area = panel.pixels.area();
        let to_pixels = |pos: egui::Pos2| ((pos.x - area.min.x) * ppp, (pos.y - area.min.y) * ppp);
        let row_at =
            |y: f32| ((y - bounds.y as f32 + position as f32) / row_height as f32).floor() as i32;
        let rect = panel.pixels.rect(bounds.into());
        self.hovered = panel
            .ui
            .input(|input| input.pointer.hover_pos())
            .filter(|pos| rect.contains(*pos))
            .map(|pos| row_at(to_pixels(pos).1))
            .filter(|&row| row >= 0 && (row as usize) < self.filtered.len())
            .map(|row| row as usize);

        let response = panel
            .ui
            .interact(rect, panel.ui.scope_id().with(salt), Sense::click());
        let mut event = None;
        // getSelection and leftClick.
        let clicked = |button| {
            response
                .clicked_by(button)
                .then(|| response.interact_pointer_pos())
                .flatten()
                .map(to_pixels)
        };
        // mouseDoubleClick: the selected file, clicked again off its star.
        let double_clicked = response
            .double_clicked()
            .then(|| response.interact_pointer_pos())
            .flatten()
            .map(to_pixels);
        if let Some((x, y)) = double_clicked {
            let star_right = row_height + icon_padding(row_height);
            let row = usize::try_from(row_at(y)).ok();
            if let Some(Selection::Path(path)) = row.and_then(|row| self.filtered.get(row)) {
                let selected = self.selected.as_ref() == Some(&Selection::Path(path.clone()));
                if path.is_file() && selected && x - bounds.x as f32 >= star_right as f32 {
                    event = Some(SelectionEvent::DoubleClicked(path.clone()));
                }
            }
        }
        if let Some((x, y)) = clicked(PointerButton::Primary).filter(|_| event.is_none()) {
            let row = row_at(y);
            if row >= 0 && (row as usize) < self.filtered.len() {
                let selection = self.filtered[row as usize].clone();
                let star_right = row_height + icon_padding(row_height);
                if x - (bounds.x as f32) < star_right as f32 {
                    event = self.select_icon(selection, set_favourite);
                } else {
                    event = self.select(selection);
                }
            }
        }
        // menuClick: "Open File Location".
        if let Some((_, y)) = clicked(PointerButton::Secondary) {
            let row = row_at(y);
            if let Some(Selection::Path(path)) = usize::try_from(row)
                .ok()
                .and_then(|row| self.filtered.get(row))
            {
                let request = reveal_request();
                panel
                    .ui
                    .ctx()
                    .data_mut(|data| data.insert_temp(request, path.clone()));
                let pos = response.interact_pointer_pos().unwrap_or_default();
                panel.overlay.menu = Some(Menu::new(
                    vec![MenuItem::new(0, "Open File Location")],
                    pos,
                    MenuOwner::Setting(reveal_chosen),
                ));
            }
        }

        self.paint(panel.pixels, panel, bounds, row_height, position);
        event
    }

    /// `loadBrowserCache` and `renderOpenGlComponents`.
    fn paint(
        &self,
        pixels: &PixelPainter,
        panel: &Panel,
        bounds: Rect,
        row_height: i32,
        position: i32,
    ) {
        let scope = panel.scope;
        let clipped = pixels.painter().with_clip_rect(pixels.rect(bounds.into()));
        let rows = PixelPainter::new(&clipped, pixels.area(), pixels.pixels_per_point());

        let padding = icon_padding(row_height);
        let icon_x = padding;
        let icon_width = row_height;
        let name_x = icon_x + icon_width + padding;
        let name_width = bounds.width - name_x;
        let style = (
            Face::Light,
            row_height as f32 * 0.55,
            scope.color(ColorId::TextComponentText),
        );
        let text_colour = scope.color(ColorId::TextComponentText);
        let icon_unselected = text_colour.with_multiplied_alpha(0.5);
        let icon_selected = scope.color(ColorId::WidgetPrimary1);

        // The star and folder fitted to their squares, and their one-pixel
        // outlines.
        let star_width = row_height as f32 * 0.8;
        let star = paths::star();
        let star_fit = star.transform_to_fit((0.0, 0.0, star_width, star_width));
        let star_outline =
            star.stroked(1.0 / star_fit.scale, JointStyle::Curved, EndCapStyle::Butt);
        let folder_width = row_height as f32 * 0.6;
        let folder = UpstreamIcon::Folder.path();
        let folder_fit = folder.transform_to_fit((0.0, 0.0, folder_width, folder_width));
        let folder_outline = folder.stroked(
            1.0 / folder_fit.scale,
            JointStyle::Curved,
            EndCapStyle::Butt,
        );

        let first = (position / row_height).max(0) as usize;
        let last = ((position + bounds.height) / row_height + 1).max(0) as usize;
        for row in first..last.min(self.filtered.len()) {
            let y = bounds.y + row as i32 * row_height - position;
            let selection = &self.filtered[row];
            let mut offset = 0.0;
            let name = match selection {
                Selection::All => "All".to_string(),
                Selection::Favourites => "Favorites".to_string(),
                Selection::Path(path) => {
                    let mut name = file_stem(path);
                    if path.is_dir() && self.passthrough_name == Some(name.as_str()) {
                        name = path.parent().map(file_stem).unwrap_or_default();
                    }
                    name
                }
            };
            let icon = |size: f32, fit: Transform, dx: f32| Transform {
                scale: fit.scale,
                dx: bounds.x as f32
                    + dx
                    + icon_x as f32
                    + (icon_width as f32 - size) / 2.0
                    + fit.dx,
                dy: y as f32 + (row_height as f32 - size) / 2.0 + fit.dy,
            };
            match selection {
                Selection::Path(path) if path.is_dir() => {
                    offset = self.folder_depth(path) as f32 * folder_width;
                    let at = icon(folder_width, folder_fit, offset);
                    let key = format!("selection folder {row_height}");
                    if self.open_folders.contains_key(path) {
                        icon::fill_path(&rows, &key, folder, at, icon_unselected);
                    }
                    icon::fill_path(
                        &rows,
                        &format!("{key} outline"),
                        &folder_outline,
                        at,
                        icon_unselected,
                    );
                }
                Selection::All => {}
                _ => {
                    let at = icon(star_width, star_fit, 0.0);
                    let key = format!("selection star {row_height}");
                    let favourite = matches!(selection, Selection::Path(path) if self.favourites.contains(path));
                    let colour = if favourite {
                        icon_selected
                    } else {
                        icon_unselected
                    };
                    if favourite {
                        icon::fill_path(&rows, &key, star, at, colour);
                    }
                    icon::fill_path(&rows, &format!("{key} outline"), &star_outline, at, colour);
                }
            }
            let text = Rect::new(
                bounds.x + name_x + offset as i32,
                y,
                name_width - 2 * padding,
                row_height,
            );
            rows.text_line(&name, text, style, Justify::Left, Overflow::Ellipsis);
        }

        // The highlights fill browse_area_: from two pixels in to three short
        // of the scroll bar.
        let scroll_bar_width = (SCROLL_BAR_WIDTH * scope.size_ratio()) as i32;
        let browse_width = bounds.width - scroll_bar_width - 3;
        let row_rect = |row: usize| {
            RectF::new(
                (bounds.x + 2) as f32,
                (bounds.y + row as i32 * row_height - position) as f32,
                browse_width as f32,
                row_height as f32,
            )
        };
        let mut highlights = QuadBatch::additive();
        if let Some(index) = self
            .selected
            .as_ref()
            .and_then(|selected| self.filtered.iter().position(|row| row == selected))
        {
            let colour = scope.color(ColorId::WidgetPrimary1).darker(0.8);
            highlights.push(Quad::new(QuadKind::Color, row_rect(index), colour));
        }
        if let Some(hovered) = self.hovered {
            highlights.push(Quad::new(
                QuadKind::Color,
                row_rect(hovered),
                scope.color(ColorId::LightenScreen),
            ));
        }
        highlights.paint(&rows, RectF::from(bounds));

        let bar = Rect::new(
            bounds.right() - scroll_bar_width,
            bounds.y,
            scroll_bar_width,
            bounds.height,
        );
        let total = (row_height * self.filtered.len() as i32) as f32;
        self.scroll
            .paint(pixels, &scope, bar, total, bounds.height as f32);
    }
}

/// `getIconPadding`.
fn icon_padding(row_height: i32) -> i32 {
    (row_height as f32 * 0.25) as i32
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The folders directly inside `folder`, in natural order.
fn sub_folders(folder: &Path) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    folders.sort_by(|a, b| compare_paths(a, b));
    folders
}

/// The row the "Open File Location" menu was opened on.
fn reveal_request() -> egui::Id {
    egui::Id::unique("selection list reveal")
}

/// The menu's "Open File Location": `File::revealToUser` for the row it
/// was opened on.
fn reveal_chosen(_: &crate::host::Host, ctx: &egui::Context, _: i32) {
    if let Some(path) = ctx.data_mut(|data| data.remove_temp::<PathBuf>(reveal_request())) {
        crate::controls::file_dialog::reveal(&path);
    }
}
