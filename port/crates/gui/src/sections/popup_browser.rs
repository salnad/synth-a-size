// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The popup browser that a section opens beside itself to choose a file:
//! a folder list and a file list with a search box. Ports `PopupBrowser`
//! (`vital/src/interface/editor_sections/popup_browser.cpp`). G7d-1 opens
//! it for an oscillator's wavetables.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use egui::{Key, Sense};
use sas_common::line_generator::LineGenerator;
use sas_engine::sas_oscillators::sas_wavetable::creator::AudioFileLoadStyle;
use sas_preset::user_folders::{self, Kind, LFO_EXTENSION, WAVETABLE_EXTENSION};

use super::selection_list::{Selection, SelectionEvent, SelectionList};
use super::Panel;
use crate::controls::paths;
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::controls::text_editor;
use crate::interface::EditorModel;
use crate::layout::Rect;
use crate::section::{paint_body, paint_border};
use crate::skin::{ColorId, Section, ValueId};

/// The browser's skin override (`Skin::kPopupBrowser`).
pub const SKIN: &[Section] = &[Section::PopupBrowser];

/// `PopupBrowser::resized`'s `kBrowseWidthRatio`: the file list's share.
const BROWSE_WIDTH_RATIO: f32 = 0.5;
/// `kTopHeight`, before the size ratio.
const TOP_HEIGHT: f32 = 38.0;

/// `vital::kWavetableExtensionsList`, without the dots.
pub const WAVETABLE_EXTENSIONS: &[&str] = &[WAVETABLE_EXTENSION, "wav", "flac"];

/// The LFO browser's `"*." + vital::kLfoExtension`.
pub const LFO_EXTENSIONS: &[&str] = &[LFO_EXTENSION];

/// The section a browser chooses a file for (`owner_`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserOwner {
    /// An oscillator's wavetable (`OscillatorSection::textMouseDown`).
    Oscillator(usize),
    /// An LFO's shape (`LfoSection::textMouseDown`).
    Lfo(usize),
}

impl BrowserOwner {
    /// The user folder kind it browses (`LoadSave::getWavetableDirectories`).
    pub fn kind(self) -> Kind {
        match self {
            BrowserOwner::Oscillator(_) => Kind::Wavetables,
            BrowserOwner::Lfo(_) => Kind::Lfos,
        }
    }

    /// The file extensions it lists (`vital::kWavetableExtensionsList`).
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            BrowserOwner::Oscillator(_) => WAVETABLE_EXTENSIONS,
            BrowserOwner::Lfo(_) => LFO_EXTENSIONS,
        }
    }
}

/// `PopupBrowser::resized`, for a browser at `browser`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupBrowserLayout {
    pub body: Rect,
    pub folders: Rect,
    pub files: Rect,
    pub horizontal_divider: Rect,
    pub vertical_divider: Rect,
    pub search: Rect,
    pub exit: Rect,
}

impl PopupBrowserLayout {
    pub fn new(size_ratio: f32, padding: f32, browser: Rect) -> PopupBrowserLayout {
        let selection_list_width = (browser.width as f32 * BROWSE_WIDTH_RATIO) as i32;
        let top_height = (TOP_HEIGHT * size_ratio) as i32;
        let folder_list_width = browser.width - selection_list_width;
        let list_height = browser.height - top_height - 2;
        let (x, y) = (browser.x, browser.y);
        let padding = padding as i32;
        let text_height = top_height - 2 * padding;
        PopupBrowserLayout {
            body: browser,
            folders: Rect::new(x, y + top_height + 1, folder_list_width - 1, list_height),
            files: Rect::new(
                x + folder_list_width,
                y + top_height + 1,
                selection_list_width - 3,
                list_height,
            ),
            horizontal_divider: Rect::new(x + 1, y + top_height - 1, browser.width - 2, 1),
            vertical_divider: Rect::new(x + folder_list_width, y + top_height, 1, list_height),
            search: Rect::new(
                x + padding,
                y + padding,
                selection_list_width - 2 * padding,
                text_height,
            ),
            exit: Rect::new(x + browser.width - top_height, y, top_height, top_height),
        }
    }
}

/// What the browser asks of its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserEvent {
    /// `owner_->loadFile`: a file chosen in the list.
    Load(BrowserOwner, PathBuf),
}

/// `PopupBrowser`'s state; it shows while it has an owner.
pub struct PopupBrowser {
    owner: Option<BrowserOwner>,
    /// `browser_bounds_`.
    browser: Rect,
    /// `passthrough_bounds_`: the owner's section, whose clicks reach it.
    passthrough: Rect,
    folders: SelectionList,
    files: SelectionList,
    search: String,
    directories: Vec<PathBuf>,
    extensions: &'static [&'static str],
}

impl Default for PopupBrowser {
    fn default() -> PopupBrowser {
        let mut folders = SelectionList::with_favourites_option();
        folders.set_passthrough_folder_name(Kind::Wavetables.folder_name());
        PopupBrowser {
            owner: None,
            browser: Rect::new(0, 0, 0, 0),
            passthrough: Rect::new(0, 0, 0, 0),
            folders,
            files: SelectionList::default(),
            search: String::new(),
            directories: Vec::new(),
            extensions: &[],
        }
    }
}

impl PopupBrowser {
    pub fn owner(&self) -> Option<BrowserOwner> {
        self.owner
    }

    pub fn is_open(&self) -> bool {
        self.owner.is_some()
    }

    /// `setVisible(false)`.
    pub fn close(&mut self) {
        self.owner = None;
        self.search.clear();
    }

    /// The file list as it shows it.
    pub fn file_rows(&self) -> &[Selection] {
        self.files.rows()
    }

    /// The folder list as it shows it.
    pub fn folder_rows(&self) -> &[Selection] {
        self.folders.rows()
    }

    /// `FullInterface::popupBrowser`: shows the browser at `browser` for
    /// `owner`, whose section is at `passthrough`, with `current` (the
    /// owner's `getCurrentFile`) selected. Its own `visibilityChanged`
    /// lists the selected folder's files; `loadPresets` then sets the
    /// folders and, for All or Favorites, lists every file.
    pub fn open(
        &mut self,
        model: &dyn EditorModel,
        owner: BrowserOwner,
        browser: Rect,
        passthrough: Rect,
        current: Option<&Path>,
    ) {
        self.owner = Some(owner);
        self.browser = browser;
        self.passthrough = passthrough;
        self.extensions = owner.extensions();
        let favourites: BTreeSet<PathBuf> = model.favourites().into_iter().collect();
        self.files.set_favourites(favourites);

        // visibilityChanged.
        self.search.clear();
        if let Some(Selection::Path(folder)) = self.folders.selected().cloned() {
            if folder.is_dir() {
                self.list_folder(&folder);
            }
        }

        // loadPresets.
        self.directories = user_folders::root(model.data_directory().as_deref())
            .map(|root| user_folders::directories(&root, owner.kind()))
            .unwrap_or_default();
        self.folders
            .set_passthrough_folder_name(owner.kind().folder_name());
        self.folders.set_selections(self.directories.clone());
        self.files.set_selected(None);
        self.folders.set_filter("");
        let folder_selected =
            matches!(self.folders.selected(), Some(Selection::Path(folder)) if folder.exists());
        if !folder_selected {
            let all = user_folders::files_of_type(&self.directories, self.extensions);
            self.files.set_selections(all);
        }
        self.files.set_filter("");
        self.files
            .set_selected(current.map(|file| Selection::Path(file.to_path_buf())));
    }

    /// `newSelection` for a folder: its files, from the top.
    fn list_folder(&mut self, folder: &Path) {
        let files = user_folders::files_of_type(&[folder.to_path_buf()], self.extensions);
        self.files.set_selections(files);
        self.files.reset_scroll_position();
    }

    /// `allSelected` and `favoritesSelected`: every file under the folders,
    /// or the favourite ones.
    fn list_all(&mut self, favourites: Option<&BTreeSet<PathBuf>>) {
        let mut files = user_folders::files_of_type(self.folders.selections(), self.extensions);
        if let Some(favourites) = favourites {
            files.retain(|file| favourites.contains(file));
        }
        self.files.set_selections(files);
        self.files.reset_scroll_position();
    }

    /// Draws the browser and handles its clicks and keys over the panel's
    /// whole interface. Returns the file to load, if one was chosen.
    pub fn show(&mut self, panel: &mut Panel, interface: Rect) -> Option<BrowserEvent> {
        let owner = self.owner?;
        let scope = panel.scope;
        let model = panel.host.model();
        let layout = PopupBrowserLayout::new(
            scope.size_ratio(),
            scope.value(ValueId::Padding),
            self.browser,
        );

        // The closing areas around the owner's section, under the browser:
        // a click there outside the browser closes it and goes no further.
        let mut close = false;
        for (index, area) in closing_areas(interface, self.passthrough)
            .into_iter()
            .enumerate()
        {
            if area.width <= 0 || area.height <= 0 {
                continue;
            }
            let rect = panel.pixels.rect(area.into());
            let id = panel.ui.scope_id().with(("popup closing area", index));
            // Drags too: egui gives a press to the top widget that senses
            // drags as well as to the one that senses clicks.
            let response = panel.ui.interact(rect, id, Sense::click_and_drag());
            let pressed = panel.ui.input(|input| input.pointer.primary_pressed());
            if response.hovered() && pressed {
                let at = panel.ui.input(|input| input.pointer.interact_pos());
                let outside =
                    at.is_some_and(|at| !panel.pixels.rect(self.browser.into()).contains(at));
                close |= outside;
            }
        }

        paint_body(panel.pixels, &scope, layout.body);
        paint_border(panel.pixels, &scope, layout.body);
        let border = scope.color(ColorId::Border);
        panel.pixels.fill_rect(layout.horizontal_divider, border);
        panel.pixels.fill_rect(layout.vertical_divider, border);

        text_editor::paint_background(panel.pixels, &scope, layout.search);
        let search = text_editor::single_line(
            panel.ui,
            panel.pixels,
            &scope,
            "popup browser search",
            layout.search,
            &mut self.search,
            "Search",
        );
        if search.escape {
            // textEditorEscapeKeyPressed.
            self.search.clear();
        }
        if search.changed || search.escape {
            // filterPresets.
            self.files.set_filter(&self.search);
        }

        let mut set_favourite = |path: &Path, favourite: bool| model.set_favourite(path, favourite);
        let mut event = None;
        match self
            .folders
            .show(panel, layout.folders, "popup folders", &mut set_favourite)
        {
            Some(SelectionEvent::Path(folder)) if folder.is_dir() => self.list_folder(&folder),
            Some(SelectionEvent::All) => self.list_all(None),
            Some(SelectionEvent::Favourites) => {
                let favourites: BTreeSet<PathBuf> = model.favourites().into_iter().collect();
                self.list_all(Some(&favourites));
            }
            _ => {}
        }
        let mut chosen = self
            .files
            .show(panel, layout.files, "popup files", &mut set_favourite);

        let exit = ShapeButton {
            shape: Shape::Icon {
                key: "popup browser exit",
                path: paths::exit_x(),
            },
            colours: ShapeColours::icon(&scope),
        };
        close |= panel
            .shape_button("popup browser exit", exit, layout.exit)
            .clicked;

        // keyPressed, while the search box doesn't have the keyboard.
        if !search.has_focus && !search.escape {
            let (escape, previous, next) = panel.ui.input(|input| {
                (
                    input.key_pressed(Key::Escape),
                    input.key_pressed(Key::ArrowUp) || input.key_pressed(Key::ArrowLeft),
                    input.key_pressed(Key::ArrowDown) || input.key_pressed(Key::ArrowRight),
                )
            });
            close |= escape;
            if previous {
                chosen = self.files.select_previous();
            } else if next {
                chosen = self.files.select_next();
            }
        }

        match chosen {
            // newSelection: a file loads into the owner.
            Some(SelectionEvent::Path(file)) if file.is_file() => {
                event = Some(BrowserEvent::Load(owner, file));
            }
            // doubleClickedSelected.
            Some(SelectionEvent::DoubleClicked(_)) => close = true,
            _ => {}
        }
        if close {
            self.close();
        }
        event
    }
}

/// `closing_areas_`: the interface left of, right of, above and below
/// `passthrough`.
pub fn closing_areas(interface: Rect, passthrough: Rect) -> [Rect; 4] {
    [
        Rect::new(
            interface.x,
            interface.y,
            passthrough.x - interface.x,
            interface.height,
        ),
        Rect::new(
            passthrough.right(),
            interface.y,
            interface.right() - passthrough.right(),
            interface.height,
        ),
        Rect::new(
            interface.x,
            interface.y,
            interface.width,
            passthrough.y - interface.y,
        ),
        Rect::new(
            interface.x,
            passthrough.bottom(),
            interface.width,
            interface.bottom() - passthrough.bottom(),
        ),
    ]
}

/// `OscillatorSection::loadFile`: a `.vitaltable` loads named after its
/// file (`WavetableEditSection::loadFile`: `jsonToState`, then `setName`);
/// any other file loads as audio, spliced. Returns false, changing nothing,
/// when the file can't be read.
pub fn load_wavetable_file(model: &dyn EditorModel, oscillator: usize, path: &Path) -> bool {
    let table = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(WAVETABLE_EXTENSION));
    if !table {
        return model.load_audio_as_wavetable(
            oscillator,
            path,
            AudioFileLoadStyle::WavetableSplice,
        );
    }
    let Some(mut data) = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| sas_common::json::parse(&text).ok())
    else {
        return false;
    };
    if data.is_object() {
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        data.set("name", name);
    }
    model.load_wavetable_json(oscillator, &data.dump())
}

/// `LfoSection::loadFile`: the file's shape, named after the file and
/// remembering it (`setName`, `setLastBrowsedFile`), into LFO `lfo`.
/// Returns false, changing nothing, when the file can't be read as a shape
/// (upstream catches the JSON exception and returns).
pub fn load_lfo_file(model: &dyn EditorModel, lfo: usize, path: &Path) -> bool {
    let Some(data) = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| sas_common::json::parse(&text).ok())
    else {
        return false;
    };
    let mut line = LineGenerator::new(LineGenerator::DEFAULT_RESOLUTION);
    if line.from_json(&data).is_err() {
        return false;
    }
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    line.set_name(&name);
    line.set_last_browsed_file(&path.to_string_lossy());
    line.render();
    model.set_lfo_shape(lfo, line)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `resized` for a 600 × 400 browser at 1x with no padding.
    #[test]
    fn the_layout_follows_upstream() {
        let layout = PopupBrowserLayout::new(1.0, 0.0, Rect::new(100, 50, 600, 400));
        assert_eq!(layout.folders, Rect::new(100, 89, 299, 360));
        assert_eq!(layout.files, Rect::new(400, 89, 297, 360));
        assert_eq!(layout.horizontal_divider, Rect::new(101, 87, 598, 1));
        assert_eq!(layout.vertical_divider, Rect::new(400, 88, 1, 360));
        assert_eq!(layout.search, Rect::new(100, 50, 300, 38));
        assert_eq!(layout.exit, Rect::new(662, 50, 38, 38));
    }

    struct DataFolder(PathBuf);

    impl EditorModel for DataFolder {
        fn preset_name(&self) -> String {
            String::new()
        }

        fn data_directory(&self) -> Option<PathBuf> {
            Some(self.0.clone())
        }
    }

    /// A folder chosen in an oscillator's browser isn't carried into an
    /// LFO's: `loadPresets`' `folder_list_->filter("")` clears a selection
    /// the new folders don't hold, and then every `.vitallfo` file is
    /// listed, as upstream's does.
    #[test]
    fn a_folder_chosen_for_one_owner_is_cleared_for_another() {
        let root = std::env::temp_dir().join(format!("sas-popup-owners-{}", std::process::id()));
        let table = root.join(format!("Wavetables/Table.{WAVETABLE_EXTENSION}"));
        let shape = root.join(format!("LFOs/Shape.{LFO_EXTENSION}"));
        for path in [&table, &shape] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        let model = DataFolder(root.clone());
        let area = Rect::new(0, 0, 100, 100);
        let mut browser = PopupBrowser::default();
        browser.open(&model, BrowserOwner::Oscillator(0), area, area, None);
        let wavetables = root.join("Wavetables");
        browser
            .folders
            .set_selected(Some(Selection::Path(wavetables.clone())));
        browser.list_folder(&wavetables);
        assert_eq!(browser.file_rows(), [Selection::Path(table)]);

        browser.open(&model, BrowserOwner::Lfo(0), area, area, None);
        assert_eq!(browser.folders.selected(), None);
        assert_eq!(browser.file_rows(), [Selection::Path(shape)]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn closing_areas_surround_the_owner() {
        let areas = closing_areas(Rect::new(0, 0, 1400, 820), Rect::new(100, 50, 300, 200));
        assert_eq!(areas[0], Rect::new(0, 0, 100, 820));
        assert_eq!(areas[1], Rect::new(400, 0, 1000, 820));
        assert_eq!(areas[2], Rect::new(0, 0, 1400, 50));
        assert_eq!(areas[3], Rect::new(0, 250, 1400, 570));
    }
}
