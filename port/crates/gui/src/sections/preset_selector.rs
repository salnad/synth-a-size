// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The header's preset selector: the previous and next arrows, the menu,
//! and the save button. Ports `SynthPresetSelector`
//! (`vital/src/interface/editor_components/synth_preset_selector.cpp`) and
//! `PresetBrowser::jumpToPreset`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use egui::{PointerButton, Sense};
use sas_engine::Tuning;
use sas_preset::registry::registry;
use sas_preset::user_folders::{self, Kind, PRESET_EXTENSION};

use super::advanced;
use super::preset_browser::{self, PresetBrowser};
use super::{MenuOwner, Panel};
use crate::controls::file_dialog::{self, FileRequest};
use crate::controls::path::Path;
use crate::controls::paths;
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::host::Host;
use crate::interface::user_folder;
use crate::layout::{HeaderLayout, Rect};
use crate::popup::{Menu, MenuItem};

/// The menu's choices (`SynthPresetSelector`'s menu ids, for the items
/// ported so far).
const BROWSE: i32 = 6;
const OPEN_EXTERNAL: i32 = 0;
const EXPORT: i32 = 1;
const INIT: i32 = 2;
const LOAD_TUNING: i32 = 3;
const CLEAR_TUNING: i32 = 4;
const DATA_FOLDER: i32 = 5;

fn open_dialog() -> egui::Id {
    egui::Id::unique("open external preset")
}

fn export_dialog() -> egui::Id {
    egui::Id::unique("export preset")
}

fn data_folder_dialog() -> egui::Id {
    egui::Id::unique("data folder")
}

/// Set by the menu's "Initialize Preset" for [`show`] to answer.
fn init_request() -> egui::Id {
    egui::Id::unique("initialize preset")
}

/// Set by the menu's "Browse Presets" for [`show`] to answer.
fn browse_request() -> egui::Id {
    egui::Id::unique("browse presets")
}

/// The pixel-snapped menu and save icons (`Paths::menu(height)`,
/// `Paths::save(height)`) for a button `height` pixels tall, with their
/// cache keys. One of each is kept per height the header has had.
fn sized_icon(save: bool, height: i32) -> (&'static str, &'static Path) {
    type Icons = HashMap<(bool, i32), (&'static str, &'static Path)>;
    static ICONS: OnceLock<Mutex<Icons>> = OnceLock::new();
    let mut icons = ICONS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *icons.entry((save, height)).or_insert_with(|| {
        let (name, path) = if save {
            ("header save", paths::save(height))
        } else {
            ("header menu", paths::menu(height))
        };
        let key: &'static str = Box::leak(format!("{name} {height}").into_boxed_str());
        (key, Box::leak(Box::new(path)))
    })
}

/// `SynthPresetSelector::modified_`, the `*` before the preset's name.
/// `HeaderSection::notifyChange` sets it when a control's value changes by
/// any route (the host, the editor, MIDI), except the pitch wheel; edits
/// that change no control (routing, wavetables, tuning) leave it alone.
/// `notifyFresh` clears it on a load or a save.
#[derive(Default)]
pub struct Modified {
    modified: bool,
    /// The synth's preset generation when the values were last read.
    generation: Option<u64>,
    /// Set for the frame after a load: a host's restore may finish setting
    /// values after its generation moves.
    settling: bool,
    values: Vec<f32>,
}

impl Modified {
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// `notifyFresh`.
    pub fn fresh(&mut self) {
        self.modified = false;
        self.settling = true;
    }

    /// Reads every control's value, marking the preset modified if one
    /// changed since the last frame at the same generation.
    pub fn update(&mut self, host: &Host) {
        let generation = host.model().preset_generation();
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            self.fresh();
        }
        let controls = registry().parameters();
        let unread = self.values.len() != controls.len();
        if unread {
            self.values = vec![0.0; controls.len()];
        }
        let mut changed = false;
        for (info, kept) in controls.iter().zip(&mut self.values) {
            if info.name == "pitch_wheel" {
                continue;
            }
            let value = host.value(&info.name);
            if value.to_bits() != kept.to_bits() {
                changed = true;
                *kept = value;
            }
        }
        if unread || self.settling {
            self.settling = false;
        } else if changed {
            self.modified = true;
        }
    }
}

fn preset_filters() -> Vec<(String, Vec<String>)> {
    vec![("Preset".to_string(), vec![PRESET_EXTENSION.to_string()])]
}

/// `SynthPresetSelector::menuCallback`.
fn choose(host: &Host, ctx: &egui::Context, id: i32) {
    let model = host.model();
    match id {
        BROWSE => {
            ctx.data_mut(|data| data.insert_temp(browse_request(), true));
        }
        OPEN_EXTERNAL => {
            // importPreset: starts beside the active file.
            let folder = model
                .active_preset_file()
                .and_then(|file| file.parent().map(|parent| parent.to_path_buf()))
                .or_else(|| user_folder(model, Kind::Presets));
            let mut request = FileRequest::open("Open Preset", preset_filters());
            if let Some(folder) = folder {
                request = request.in_folder(folder);
            }
            file_dialog::open(ctx, open_dialog(), &request);
        }
        EXPORT => {
            let mut name = model.preset_name();
            if name.is_empty() {
                name = "Init Preset".to_string();
            }
            let file_name = format!("{name}.{PRESET_EXTENSION}");
            let request = FileRequest::save("Export Preset", preset_filters(), &file_name);
            file_dialog::open(ctx, export_dialog(), &request);
        }
        INIT => {
            // initPreset: externalPresetLoaded(File()).
            model.init_preset();
            ctx.data_mut(|data| data.insert_temp(init_request(), true));
        }
        LOAD_TUNING => advanced::open_tuning_file(ctx),
        CLEAR_TUNING => {
            let mut tuning: Tuning = model.tuning();
            tuning.set_default_tuning();
            model.load_tuning(tuning);
        }
        DATA_FOLDER => {
            let mut request = FileRequest::folder("Choose Data Folder");
            if let Some(root) = user_folders::root(model.data_directory().as_deref()) {
                request = request.in_folder(root);
            }
            file_dialog::open(ctx, data_folder_dialog(), &request);
        }
        _ => {}
    }
}

/// `SynthPresetSelector::showPopupMenu`, anchored below `anchor`.
fn open_menu(panel: &mut Panel, anchor: Rect) {
    let model = panel.host.model();
    let mut items = vec![
        MenuItem::new(BROWSE, "Browse Presets"),
        MenuItem::new(OPEN_EXTERNAL, "Open External Preset"),
        MenuItem::new(EXPORT, "Export Preset"),
        MenuItem::divider(),
        MenuItem::new(INIT, "Initialize Preset"),
        MenuItem::divider(),
        MenuItem::new(LOAD_TUNING, "Load Tuning File"),
    ];
    let tuning = model.tuning();
    if !tuning.is_default() {
        let name = model.tuning_name();
        items.push(MenuItem::new(CLEAR_TUNING, format!("Clear Tuning: {name}")));
    }
    items.push(MenuItem::divider());
    items.push(MenuItem::new(DATA_FOLDER, "Choose Data Folder..."));
    let pos = egui::pos2(anchor.x as f32, anchor.bottom() as f32);
    panel.overlay.menu = Some(Menu::new(items, pos, MenuOwner::Setting(choose)));
}

/// The arrows, the save and menu buttons, the name's clicks, and the
/// answers from the dialogs the menu opens. Returns the preset browser's
/// new visibility when a click or the menu changes it.
pub fn show(
    panel: &mut Panel,
    header: &HeaderLayout,
    modified: &mut Modified,
    browser: &mut PresetBrowser,
    browser_visible: bool,
) -> Option<bool> {
    let mut visibility = None;
    let model = panel.host.model();
    let preset = header.preset;
    let colours = ShapeColours::icon(&panel.scope);
    let arrow = |shape| ShapeButton { shape, colours };
    if panel
        .shape_button("preset previous", arrow(Shape::Prev), preset.previous)
        .clicked
    {
        browser.jump_to_preset(&panel.ui.ctx().clone(), model, -1);
    }
    if panel
        .shape_button("preset next", arrow(Shape::Next), preset.next)
        .clicked
    {
        browser.jump_to_preset(&panel.ui.ctx().clone(), model, 1);
    }

    // The save button opens the save section (G7c).
    let (key, path) = sized_icon(true, header.save_button.height);
    panel.shape_button(
        "preset save",
        arrow(Shape::Icon { key, path }),
        header.save_button,
    );
    // The menu button is triggered on mouse down.
    let (key, path) = sized_icon(false, header.menu_button.height);
    if panel
        .shape_button(
            "preset menu",
            arrow(Shape::Icon { key, path }),
            header.menu_button,
        )
        .pressed
    {
        open_menu(panel, header.menu_button);
    }
    // textMouseUp: a right-click on the name opens the menu below it, and
    // a left click shows or hides the browser.
    let rect = panel.pixels.rect(preset.text.into());
    let response = panel.ui.interact(
        rect,
        panel.ui.scope_id().with("preset name"),
        Sense::click(),
    );
    if response.clicked_by(PointerButton::Primary) {
        visibility = Some(!browser_visible);
    }
    if response.clicked_by(PointerButton::Secondary) {
        open_menu(panel, preset.text);
    }

    let ctx = panel.ui.ctx().clone();
    // importPreset and exportPreset: the preset is external to the list.
    // importPreset skips a choice that doesn't exist, without a message.
    let opened = file_dialog::take_result(&ctx, open_dialog()).flatten();
    if let Some(path) = opened.filter(|path| path.exists()) {
        match model.load_preset_file(&path) {
            Ok(()) => browser.external_preset_loaded(Some(path)),
            Err(error) => preset_browser::alert_load_error(&ctx, &error),
        }
    }
    if let Some(Some(path)) = file_dialog::take_result(&ctx, export_dialog()) {
        if let Ok(written) = model.save_preset_file(&path) {
            modified.fresh();
            browser.external_preset_loaded(Some(written));
        }
    }
    if ctx
        .data_mut(|data| data.remove_temp::<bool>(init_request()))
        .is_some()
    {
        browser.external_preset_loaded(None);
    }
    if let Some(Some(folder)) = file_dialog::take_result(&ctx, data_folder_dialog()) {
        model.set_data_directory(folder);
    }
    if ctx
        .data_mut(|data| data.remove_temp::<bool>(browse_request()))
        .is_some()
    {
        visibility = Some(true);
    }
    visibility
}
