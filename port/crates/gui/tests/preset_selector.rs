// SPDX-License-Identifier: GPL-3.0-or-later

//! The header's preset selector with a recording synth: the arrows step
//! through the active preset's folder, and the menu reaches the synth.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use nice_plug::context::gui::{GuiContext, GuiContextInner};
use nice_plug::context::PluginApi;
use nice_plug::prelude::{FloatParam, FloatRange, Param, ParamPtr, PluginState};
use sas_engine::Tuning;
use sas_gui::controls::{file_dialog, text_editor};
use sas_gui::layout::{FullLayout, HeaderLayout, Rect};
use sas_gui::scope::Scope;
use sas_gui::sections::delete_section::DeleteLayout;
use sas_gui::sections::oscillator::{self, OscillatorLayout};
use sas_gui::sections::popup_browser::{self, PopupBrowserLayout};
use sas_gui::sections::preset_browser::PresetBrowserLayout;
use sas_gui::sections::save_section::{OverwriteLayout, SaveLayout};
use sas_gui::sections::selection_list::Selection;
use sas_gui::sections::synthesis::SynthesisLayout;
use sas_gui::skin::Skin;
use sas_gui::{EditorModel, Host, HostParam, Interface, OpenGestures, Tab};
use sas_preset::registry::registry;
use sas_preset::user_folders::PRESET_EXTENSION;

/// Every registry control, linear over its range, and the editor settings.
struct Synth {
    params: BTreeMap<&'static str, FloatParam>,
    hz_frequency: AtomicBool,
    tuning: Mutex<Tuning>,
    data_directory: Mutex<Option<PathBuf>>,
    active_file: Mutex<Option<PathBuf>>,
    /// What the editor asked the synth to do with presets.
    calls: Mutex<Vec<String>>,
    favourites: Mutex<Vec<PathBuf>>,
    generation: AtomicU64,
    default_author: Mutex<String>,
}

impl Synth {
    fn new() -> Arc<Synth> {
        let params = registry()
            .parameters()
            .iter()
            .map(|info| {
                let range = FloatRange::Linear {
                    min: info.min,
                    max: info.max,
                };
                let name: &'static str = info.name.as_str();
                (name, FloatParam::new(name, info.default_value, range))
            })
            .collect();
        Arc::new(Synth {
            params,
            hz_frequency: AtomicBool::new(false),
            tuning: Mutex::new(Tuning::new()),
            data_directory: Mutex::new(None),
            active_file: Mutex::new(None),
            calls: Mutex::new(Vec::new()),
            favourites: Mutex::new(Vec::new()),
            generation: AtomicU64::new(0),
            default_author: Mutex::new("Someone".to_string()),
        })
    }

    fn name_of(&self, ptr: ParamPtr) -> &'static str {
        self.params
            .iter()
            .find(|(_, param)| param.as_ptr() == ptr)
            .map(|(name, _)| *name)
            .unwrap()
    }
}

impl EditorModel for Synth {
    fn preset_name(&self) -> String {
        String::new()
    }

    fn param(&self, name: &str) -> Option<HostParam> {
        self.params.get(name).map(|param| param.as_ptr())
    }

    fn hz_frequency_display(&self) -> bool {
        self.hz_frequency.load(Ordering::Relaxed)
    }

    fn set_hz_frequency_display(&self, hz_frequency: bool) {
        self.hz_frequency.store(hz_frequency, Ordering::Relaxed);
    }

    fn tuning_name(&self) -> String {
        self.tuning.lock().unwrap().name()
    }

    fn tuning(&self) -> Tuning {
        self.tuning.lock().unwrap().clone()
    }

    fn load_tuning(&self, tuning: Tuning) {
        *self.tuning.lock().unwrap() = tuning;
    }

    fn data_directory(&self) -> Option<PathBuf> {
        self.data_directory.lock().unwrap().clone()
    }

    fn active_preset_file(&self) -> Option<PathBuf> {
        self.active_file.lock().unwrap().clone()
    }

    fn load_preset_file(&self, path: &Path) -> Result<(), String> {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        self.calls.lock().unwrap().push(format!("load {name}"));
        if name.contains("broken") {
            return Err("Preset file is corrupted.".to_string());
        }
        *self.active_file.lock().unwrap() = Some(path.to_path_buf());
        Ok(())
    }

    fn init_preset(&self) {
        self.calls.lock().unwrap().push("init".to_string());
        *self.active_file.lock().unwrap() = None;
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn load_wavetable_json(&self, oscillator: usize, json: &str) -> bool {
        let data = sas_common::json::parse(json).unwrap();
        let name = data.get("name").unwrap().to_str().unwrap().to_string();
        self.calls
            .lock()
            .unwrap()
            .push(format!("wavetable {oscillator} {name}"));
        true
    }

    fn load_audio_as_wavetable(
        &self,
        oscillator: usize,
        path: &Path,
        _style: sas_engine::sas_oscillators::sas_wavetable::creator::AudioFileLoadStyle,
    ) -> bool {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        self.calls
            .lock()
            .unwrap()
            .push(format!("audio {oscillator} {name}"));
        true
    }

    fn favourites(&self) -> Vec<PathBuf> {
        self.favourites.lock().unwrap().clone()
    }

    fn set_favourite(&self, path: &Path, favourite: bool) {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let state = if favourite { "on" } else { "off" };
        self.calls
            .lock()
            .unwrap()
            .push(format!("favourite {name} {state}"));
    }

    fn preset_generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    fn preset_comments(&self) -> String {
        "Old comments".to_string()
    }

    fn preset_style(&self) -> String {
        "Keys".to_string()
    }

    fn set_preset_info(&self, author: &str, comments: &str, style: &str) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("info {author}|{comments}|{style}"));
    }

    fn default_author(&self) -> String {
        self.default_author.lock().unwrap().clone()
    }

    fn save_default_author(&self, author: &str) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("default author {author}"));
        *self.default_author.lock().unwrap() = author.to_string();
    }

    fn save_preset_file(&self, path: &Path) -> Result<PathBuf, String> {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        self.calls.lock().unwrap().push(format!("save {name}"));
        std::fs::write(path, "").map_err(|error| error.to_string())?;
        Ok(path.to_path_buf())
    }

    fn wavetable_json(&self, oscillator: usize) -> Option<String> {
        Some(format!(
            r#"{{"version":"1.0.0","name":"Table {oscillator}","groups":[]}}"#
        ))
    }
}

/// Records gestures and applies sets at the start of the next frame.
struct Recorder {
    synth: Arc<Synth>,
    log: Mutex<Vec<String>>,
    pending: Mutex<Vec<(ParamPtr, f32)>>,
}

impl GuiContextInner for Recorder {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }

    unsafe fn raw_begin_set_parameter(&self, param: ParamPtr) {
        let name = self.synth.name_of(param);
        self.log.lock().unwrap().push(format!("begin {name}"));
    }

    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
        self.pending.lock().unwrap().push((param, normalized));
        let name = self.synth.name_of(param);
        self.log.lock().unwrap().push(format!("set {name}"));
    }

    unsafe fn raw_end_set_parameter(&self, param: ParamPtr) {
        let name = self.synth.name_of(param);
        self.log.lock().unwrap().push(format!("end {name}"));
    }

    fn get_state(&self) -> PluginState {
        unimplemented!("the editor does not read the plugin state")
    }

    fn set_state(&self, _state: PluginState) {}

    fn request_restart(&self) {}
}

struct Fixture {
    synth: Arc<Synth>,
    recorder: Arc<Recorder>,
    context: GuiContext,
    open: OpenGestures,
    interface: RefCell<Interface>,
    skin: Skin,
}

impl Fixture {
    fn new() -> Fixture {
        let synth = Synth::new();
        let recorder = Arc::new(Recorder {
            synth: synth.clone(),
            log: Mutex::new(Vec::new()),
            pending: Mutex::new(Vec::new()),
        });
        let context = GuiContext::new(recorder.clone());
        let mut interface = Interface::default();
        interface.set_tab(Tab::Voice);
        Fixture {
            synth,
            recorder,
            context,
            open: OpenGestures::default(),
            interface: RefCell::new(interface),
            skin: Skin::default(),
        }
    }

    fn harness(&self) -> Harness<'_> {
        let mut harness = Harness::builder()
            .with_size((1400.0, 820.0))
            .with_step_dt(1.0 / 60.0)
            .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
            .build_ui(|ui| {
                let screen = ui.ctx().content_rect();
                for (param, normalized) in self.recorder.pending.lock().unwrap().drain(..) {
                    unsafe { param._internal_set_normalized_value(normalized) };
                }
                let host = Host::tracking(self.synth.as_ref(), Some(&self.context), &self.open);
                self.interface.borrow_mut().show_in(ui, screen, &host)
            });
        harness.step();
        harness
    }

    fn header(&self) -> HeaderLayout {
        FullLayout::new(&self.skin, 1400, 820).header
    }
}

fn centre(rect: Rect) -> Pos2 {
    Pos2::new(
        rect.x as f32 + rect.width as f32 / 2.0,
        rect.y as f32 + rect.height as f32 / 2.0,
    )
}

fn button(harness: &mut Harness, pos: Pos2, pressed: bool) {
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

fn click(harness: &mut Harness, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.step();
    button(harness, pos, true);
    button(harness, pos, false);
    harness.step();
}

/// Clicks row `row` (dividers count) of a menu whose top is at `top`.
fn choose_row(harness: &mut Harness, x: f32, top: f32, row: usize) {
    // Rows are 24 pixels from the top of the list, under the body rounding.
    let pos = Pos2::new(x + 20.0, top + 12.0 + 24.0 * row as f32);
    click(harness, pos);
}

/// A folder of presets for the arrows, removed when dropped.
struct Folder(PathBuf);

/// `stem` with the preset extension.
fn preset(stem: &str) -> String {
    format!("{stem}.{PRESET_EXTENSION}")
}

impl Folder {
    fn new(tag: &str, names: &[String]) -> Folder {
        let path = std::env::temp_dir().join(format!("sas-presets-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(path.join("Presets")).unwrap();
        for name in names {
            std::fs::write(path.join("Presets").join(name), "").unwrap();
        }
        Folder(path)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// After an external preset (Open External, Export), the arrows step
/// through its folder in natural order, wrapping round
/// (`PresetBrowser::jumpToPreset`).
#[test]
fn the_arrows_step_through_an_external_presets_folder() {
    let folder = Folder::new(
        "arrows",
        &[preset("Bass 10"), preset("bass 2"), preset("Lead")],
    );
    let fixture = Fixture::new();
    let external = folder.0.join("Presets").join(preset("bass 2"));
    fixture
        .interface
        .borrow_mut()
        .external_preset_loaded(Some(external));
    let mut harness = fixture.harness();
    let header = fixture.header();
    click(&mut harness, centre(header.preset.next));
    click(&mut harness, centre(header.preset.next));
    click(&mut harness, centre(header.preset.previous));
    assert_eq!(
        *fixture.synth.calls.lock().unwrap(),
        [
            format!("load {}", preset("Bass 10")),
            format!("load {}", preset("Lead")),
            format!("load {}", preset("Bass 10")),
        ]
    );
}

/// Without an external preset, the arrows move the browser list's
/// selection (`PresetList::shiftSelectedPreset`), wrapping round; with
/// nothing selected the index starts at -1, so previous goes to the second
/// last.
#[test]
fn without_an_external_preset_the_arrows_step_through_the_list() {
    let folder = Folder::new(
        "list",
        &[preset("b"), preset("a"), preset("c"), "d.txt".to_string()],
    );
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(folder.0.clone());
    let mut harness = fixture.harness();
    let header = fixture.header();
    click(&mut harness, centre(header.preset.previous));
    click(&mut harness, centre(header.preset.previous));
    click(&mut harness, centre(header.preset.next));
    assert_eq!(
        *fixture.synth.calls.lock().unwrap(),
        [
            format!("load {}", preset("b")),
            format!("load {}", preset("a")),
            format!("load {}", preset("b")),
        ]
    );
}

/// The menu opens on mouse down below its button; "Initialize Preset" is
/// its sixth row (after a divider).
#[test]
fn the_menu_initializes_the_preset() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let menu = fixture.header().menu_button;
    click(&mut harness, centre(menu));
    choose_row(&mut harness, menu.x as f32, menu.bottom() as f32, 5);
    assert_eq!(*fixture.synth.calls.lock().unwrap(), ["init"]);
}

/// Sets control `name` as the host would, outside the editor.
fn set_from_host(fixture: &Fixture, name: &str, normalized: f32) {
    let param = fixture.synth.params[name].as_ptr();
    unsafe { param._internal_set_normalized_value(normalized) };
}

/// A control changed from outside the editor marks the preset modified
/// (`HeaderSection::notifyChange`); the pitch wheel does not.
#[test]
fn a_control_change_marks_the_preset_modified_but_the_pitch_wheel_does_not() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    harness.step();
    assert!(!fixture.interface.borrow().preset_modified());
    set_from_host(&fixture, "pitch_wheel", 0.9);
    harness.step();
    assert!(!fixture.interface.borrow().preset_modified());
    set_from_host(&fixture, "mod_wheel", 0.5);
    harness.step();
    assert!(fixture.interface.borrow().preset_modified());
}

/// A load clears the mark (`notifyFresh`), and the values it sets don't
/// mark the new preset modified.
#[test]
fn a_load_clears_the_mark() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    set_from_host(&fixture, "osc_1_level", 0.25);
    harness.step();
    assert!(fixture.interface.borrow().preset_modified());
    let menu = fixture.header().menu_button;
    click(&mut harness, centre(menu));
    set_from_host(&fixture, "osc_1_level", 0.75);
    choose_row(&mut harness, menu.x as f32, menu.bottom() as f32, 5);
    harness.step();
    harness.step();
    assert!(!fixture.interface.borrow().preset_modified());
    set_from_host(&fixture, "osc_1_level", 0.5);
    harness.step();
    assert!(fixture.interface.borrow().preset_modified());
}

fn browser_visible(fixture: &Fixture) -> bool {
    fixture.interface.borrow().preset_browser_visible()
}

/// A left click on the name shows and hides the browser (`textMouseUp`),
/// "Browse Presets" shows it, and the header's exit button hides it
/// (`clearTemporaryTab`).
#[test]
fn the_name_the_menu_and_the_exit_button_show_and_hide_the_browser() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let header = fixture.header();
    click(&mut harness, centre(header.preset.text));
    assert!(browser_visible(&fixture));
    click(&mut harness, centre(header.preset.text));
    assert!(!browser_visible(&fixture));

    let menu = header.menu_button;
    click(&mut harness, centre(menu));
    choose_row(&mut harness, menu.x as f32, menu.bottom() as f32, 0);
    harness.step();
    assert!(browser_visible(&fixture));
    click(&mut harness, centre(header.exit_temporary));
    assert!(!browser_visible(&fixture));
}

/// Escape hides the browser (`PresetBrowser::keyPressed`).
#[test]
fn escape_hides_the_browser() {
    let fixture = Fixture::new();
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    assert!(browser_visible(&fixture));
    harness.event(Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    assert!(!browser_visible(&fixture));
}

/// `migration/corpus/gui/data`: the authored presets the browser's views
/// list.
fn corpus_data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migration/corpus/gui/data")
}

fn browser_layout(fixture: &Fixture) -> PresetBrowserLayout {
    let layout = FullLayout::new(&fixture.skin, 1400, 820);
    PresetBrowserLayout::new(
        &Scope::top(&fixture.skin, layout.size_ratio),
        layout.browser,
    )
}

fn shown(fixture: &Fixture) -> Vec<String> {
    fixture
        .interface
        .borrow()
        .browser_presets()
        .iter()
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect()
}

fn type_text(harness: &mut Harness, text: &str) {
    harness.event(Event::Text(text.to_string()));
    harness.step();
    harness.step();
}

fn press_key(harness: &mut Harness, key: egui::Key) {
    harness.event(Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

/// The list shows every preset under the data folder's Presets folders
/// (banks' too) by name in natural order, ignoring case; a click on a
/// column's title sorts by it, again reversing it, keeping the order of
/// equal entries (`PresetList::leftClick`, `sort`).
#[test]
fn the_list_sorts_by_the_clicked_column() {
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(corpus_data());
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    assert_eq!(
        shown(&fixture),
        [
            "bass 10",
            "Bass One",
            "Keys Soft",
            "Lead 2",
            "Pad",
            "Sequence Run"
        ]
    );
    let list = browser_layout(&fixture).list;
    let width = list.width as f32;
    let author_x = list.x as f32 + width * (0.04 + 0.35 + 0.18 + 0.1);
    let title = Pos2::new(author_x, list.y as f32 + 5.0);
    click(&mut harness, title);
    assert_eq!(
        shown(&fixture),
        [
            "Pad",
            "Sequence Run",
            "Bass One",
            "Keys Soft",
            "Lead 2",
            "bass 10"
        ]
    );
    click(&mut harness, title);
    assert_eq!(
        shown(&fixture),
        [
            "bass 10",
            "Bass One",
            "Keys Soft",
            "Lead 2",
            "Sequence Run",
            "Pad"
        ]
    );
}

/// The style buttons and the search box filter the list: by the styles
/// that are on, and by names or authors containing every word typed
/// (`PresetList::filter`). Escape in the search box clears it and leaves
/// the browser open (`textEditorEscapeKeyPressed`).
#[test]
fn styles_and_the_search_filter_the_list() {
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(corpus_data());
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    let layout = browser_layout(&fixture);
    click(&mut harness, centre(layout.styles[0]));
    assert_eq!(shown(&fixture), ["bass 10", "Bass One"]);
    click(&mut harness, centre(layout.styles[0]));
    assert_eq!(shown(&fixture).len(), 6);

    click(&mut harness, centre(layout.search));
    type_text(&mut harness, "corpus soft");
    assert_eq!(shown(&fixture), ["Keys Soft"]);
    press_key(&mut harness, egui::Key::Escape);
    harness.step();
    assert_eq!(shown(&fixture).len(), 6);
    assert!(browser_visible(&fixture));
}

/// A click on a row loads its preset; a click on its star marks it a
/// favourite instead (`PresetList::leftClick`).
#[test]
fn a_row_loads_and_its_star_marks_a_favourite() {
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(corpus_data());
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    let layout = browser_layout(&fixture);
    let list = layout.list;
    let row_y = |row: i32| {
        (list.y + layout.title_width + row * layout.row_height) as f32
            + layout.row_height as f32 / 2.0
    };
    click(
        &mut harness,
        Pos2::new((list.x + list.width / 2) as f32, row_y(1)),
    );
    let star_x = list.x as f32 + list.width as f32 * 0.02;
    click(&mut harness, Pos2::new(star_x, row_y(0)));
    assert_eq!(
        *fixture.synth.calls.lock().unwrap(),
        [
            format!("load {}", preset("Bass One")),
            format!("favourite {} on", preset("bass 10")),
        ]
    );
}

/// The folder list's row `row`, past its icon.
fn folder_row(fixture: &Fixture, row: i32) -> Pos2 {
    let folders = browser_layout(fixture).folders;
    Pos2::new((folders.x + 100) as f32, (folders.y + row * 24 + 12) as f32)
}

/// A copy of the corpus data folder, removed when dropped.
fn copied_data(tag: &str) -> Folder {
    let folder = Folder::new(tag, &[]);
    for entry in [
        "Presets/Bass One",
        "Presets/Lead 2",
        "Bank/Presets/Sequence Run",
    ] {
        let from = corpus_data().join(format!("{entry}.{PRESET_EXTENSION}"));
        let to = folder.0.join(format!("{entry}.{PRESET_EXTENSION}"));
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(from, to).unwrap();
    }
    folder
}

/// The folder list shows All, Favorites and each Presets folder, named
/// after its parent (`setPassthroughFolderName`); choosing a folder lists
/// its presets, choosing it again opens it, and All and Favorites list
/// every preset and the favourites (`SelectionList::select`).
#[test]
fn the_folder_list_chooses_what_the_list_shows() {
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(corpus_data());
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    let data = corpus_data();
    assert_eq!(
        fixture.interface.borrow().browser_folders(),
        [
            Selection::All,
            Selection::Favourites,
            Selection::Path(data.join("Bank/Presets")),
            Selection::Path(data.join("Presets")),
        ]
    );
    click(&mut harness, folder_row(&fixture, 2));
    assert_eq!(shown(&fixture), ["Sequence Run"]);
    click(&mut harness, folder_row(&fixture, 3));
    assert_eq!(
        shown(&fixture),
        ["bass 10", "Bass One", "Keys Soft", "Lead 2", "Pad"]
    );
    click(&mut harness, folder_row(&fixture, 3));
    assert_eq!(
        fixture.interface.borrow().browser_folders()[4],
        Selection::Path(data.join("Presets/Keys"))
    );
    click(&mut harness, folder_row(&fixture, 1));
    assert!(shown(&fixture).is_empty());
    click(&mut harness, folder_row(&fixture, 0));
    assert_eq!(shown(&fixture).len(), 6);
}

fn right_click(harness: &mut Harness, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.step();
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    harness.step();
}

fn list_row(fixture: &Fixture, row: i32) -> Pos2 {
    let layout = browser_layout(fixture);
    let list = layout.list;
    Pos2::new(
        (list.x + list.width / 2) as f32,
        (list.y + layout.title_width + row * layout.row_height) as f32
            + layout.row_height as f32 / 2.0,
    )
}

/// The row menu's Rename edits the name in place, asking the window for
/// the keyboard as a click on a field does; Return renames the file to the
/// name as typed, keeping its extension (`PresetList::finishRename`).
#[test]
fn the_row_menu_renames_a_preset() {
    let folder = copied_data("rename");
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(folder.0.clone());
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    assert_eq!(shown(&fixture), ["Bass One", "Lead 2", "Sequence Run"]);
    let row = list_row(&fixture, 0);
    right_click(&mut harness, row);
    assert!(!text_editor::take_window_focus_request(&harness.ctx));
    choose_row(&mut harness, row.x, row.y, 1);
    harness.step();
    assert!(text_editor::take_window_focus_request(&harness.ctx));
    type_text(&mut harness, " Deep: Bass ");
    press_key(&mut harness, egui::Key::Enter);
    harness.step();
    assert_eq!(shown(&fixture), [" Deep Bass ", "Lead 2", "Sequence Run"]);
    assert!(folder
        .0
        .join(format!("Presets/ Deep Bass .{PRESET_EXTENSION}"))
        .is_file());
}

/// The row menu's Delete asks first; Delete removes the file and the list
/// reads its presets again (`DeleteSection`, `fileDeleted`), and a click
/// outside the box cancels.
#[test]
fn the_row_menu_deletes_a_preset_after_asking() {
    let folder = copied_data("delete");
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(folder.0.clone());
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    let dialog = DeleteLayout::new(1.0, Rect::new(0, 0, 1400, 820));

    let row = list_row(&fixture, 1);
    right_click(&mut harness, row);
    choose_row(&mut harness, row.x, row.y, 2);
    harness.step();
    assert!(fixture.interface.borrow().deleting().is_some());
    click(&mut harness, Pos2::new(10.0, 10.0));
    assert!(fixture.interface.borrow().deleting().is_none());
    assert_eq!(shown(&fixture).len(), 3);

    right_click(&mut harness, row);
    choose_row(&mut harness, row.x, row.y, 2);
    harness.step();
    click(&mut harness, centre(dialog.delete));
    assert!(fixture.interface.borrow().deleting().is_none());
    assert_eq!(shown(&fixture), ["Bass One", "Sequence Run"]);
    assert!(!folder
        .0
        .join(format!("Presets/Lead 2.{PRESET_EXTENSION}"))
        .exists());
}

/// A preset picked in the list that doesn't load says why
/// (`newPresetSelected`'s "Error opening preset"); stepping through an
/// external preset's folder doesn't (`PresetBrowser::loadFromFile`).
#[test]
fn a_preset_that_doesnt_load_says_why_when_picked() {
    let folder = Folder::new("broken", &[preset("broken"), preset("fine")]);
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(folder.0.clone());
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    click(&mut harness, list_row(&fixture, 0));
    assert_eq!(
        file_dialog::queued_alerts(&harness.ctx),
        [(
            "Error opening preset".to_string(),
            "There was an error open the preset. Preset file is corrupted.".to_string()
        )]
    );

    let fixture = Fixture::new();
    fixture
        .interface
        .borrow_mut()
        .external_preset_loaded(Some(folder.0.join("Presets").join(preset("fine"))));
    let mut harness = fixture.harness();
    click(&mut harness, centre(fixture.header().preset.next));
    assert_eq!(
        *fixture.synth.calls.lock().unwrap(),
        [format!("load {}", preset("broken"))]
    );
    assert!(file_dialog::queued_alerts(&harness.ctx).is_empty());
}

/// The save section's state: (open, asking before overwriting).
fn save_state(fixture: &Fixture) -> (bool, bool) {
    fixture.interface.borrow().save_section_state()
}

fn save_layout(preset: bool) -> SaveLayout {
    SaveLayout::new(1.0, Rect::new(0, 0, 1400, 820), preset)
}

fn calls(fixture: &Fixture) -> Vec<String> {
    std::mem::take(&mut *fixture.synth.calls.lock().unwrap())
}

/// A data folder with an empty Presets folder, as the synth's.
fn data_folder(fixture: &Fixture, tag: &str, presets: &[String]) -> Folder {
    let folder = Folder::new(tag, presets);
    *fixture.synth.data_directory.lock().unwrap() = Some(folder.0.clone());
    folder
}

/// The save button opens the save section with the synth's name, comments
/// and style and the saved author (`savePreset`, `setVisible`). Save sets
/// the author, comments and style, remembers the author and writes the
/// preset into the user's Presets folder, then the section closes and the
/// header's `*` clears (`save`, `HeaderSection::save`). A style button
/// turns the others off.
#[test]
fn the_save_button_saves_the_preset_with_its_details() {
    let fixture = Fixture::new();
    let data = data_folder(&fixture, "save", &[]);
    let mut harness = fixture.harness();
    set_from_host(&fixture, "osc_1_level", 0.25);
    harness.step();
    assert!(fixture.interface.borrow().preset_modified());

    click(&mut harness, centre(fixture.header().save_button));
    assert_eq!(save_state(&fixture), (true, false));
    // The name field has the keyboard.
    type_text(&mut harness, "My Patch");
    let layout = save_layout(true);
    // Keys, from the synth, then Lead, then Pad: one at a time.
    click(&mut harness, centre(layout.styles[1]));
    click(&mut harness, centre(layout.styles[3]));
    click(&mut harness, centre(layout.author));
    type_text(&mut harness, " Else");
    click(&mut harness, centre(layout.save));
    harness.step();

    assert_eq!(save_state(&fixture), (false, false));
    assert_eq!(
        calls(&fixture),
        [
            "default author Someone Else",
            "info Someone Else|Old comments|Pad",
            &format!("save {}", preset("My Patch")),
        ]
    );
    assert!(data.0.join("Presets").join(preset("My Patch")).exists());
    assert!(!fixture.interface.borrow().preset_modified());
}

/// "Save Preset" in the menu opens the save section too.
#[test]
fn the_menus_save_preset_opens_the_save_section() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let menu = fixture.header().menu_button;
    click(&mut harness, centre(menu));
    choose_row(&mut harness, menu.x as f32, menu.bottom() as f32, 1);
    assert_eq!(save_state(&fixture), (true, false));
}

/// Saving over an existing preset asks first (`setOverwriteBounds`), and
/// Overwrite saves; Return in the name field saves as the button does.
#[test]
fn saving_over_a_preset_asks_first() {
    let fixture = Fixture::new();
    let _data = data_folder(&fixture, "overwrite", &[preset("Taken")]);
    let mut harness = fixture.harness();
    click(&mut harness, centre(fixture.header().save_button));
    type_text(&mut harness, "Taken");
    press_key(&mut harness, egui::Key::Enter);
    harness.step();
    assert_eq!(save_state(&fixture), (true, true));
    assert!(calls(&fixture).is_empty());

    let overwrite = OverwriteLayout::new(1.0, Rect::new(0, 0, 1400, 820));
    click(&mut harness, centre(overwrite.overwrite));
    harness.step();
    assert_eq!(save_state(&fixture), (false, false));
    assert!(calls(&fixture).contains(&format!("save {}", preset("Taken"))));
}

/// Without a name, Save does nothing; a click outside the box closes it
/// (`mouseUp`), saving nothing.
#[test]
fn an_unnamed_preset_isnt_saved_and_a_click_outside_closes() {
    let fixture = Fixture::new();
    let _data = data_folder(&fixture, "unnamed", &[]);
    let mut harness = fixture.harness();
    click(&mut harness, centre(fixture.header().save_button));
    type_text(&mut harness, "   ");
    click(&mut harness, centre(save_layout(true).save));
    assert_eq!(save_state(&fixture), (true, false));
    click(&mut harness, Pos2::new(20.0, 400.0));
    assert_eq!(save_state(&fixture), (false, false));
    assert!(calls(&fixture).is_empty());
}

/// "Save to Wavetables" in an oscillator's wavetable view opens the save
/// section for a wavetable (name and author only), which writes the
/// wavetable's JSON with them into the user's Wavetables folder
/// (`FullInterface::saveWavetable`).
#[test]
fn save_to_wavetables_writes_the_wavetable() {
    let fixture = Fixture::new();
    let data = data_folder(&fixture, "wavetable", &[]);
    let mut harness = fixture.harness();
    let skin = Skin::default();
    let layout = FullLayout::new(&skin, 1400, 820);
    let top = Scope::top(&skin, layout.size_ratio);
    let synthesis = SynthesisLayout::new(&top, layout.main);
    let view = OscillatorLayout::new(
        &top.within(oscillator::SKIN),
        synthesis.oscillators[1],
        false,
    )
    .wavetable;
    let at = centre(view);
    harness.event(Event::PointerMoved(at));
    harness.step();
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    choose_row(&mut harness, at.x, at.y, 0);
    assert_eq!(save_state(&fixture), (true, false));
    type_text(&mut harness, "Table");
    click(&mut harness, centre(save_layout(false).save));
    harness.step();

    assert_eq!(save_state(&fixture), (false, false));
    let written =
        std::fs::read_to_string(data.0.join("Wavetables").join("Table.vitaltable")).unwrap();
    assert!(written.contains(r#""name":"Table""#), "{written}");
    assert!(written.contains(r#""author":"Someone""#), "{written}");
    assert!(written.contains(r#""groups":[]"#), "{written}");
    assert!(calls(&fixture).is_empty());
}

/// While the save section has the keyboard, the browser behind it takes no
/// keys: the arrows move the name's caret, not the browser's selection.
#[test]
fn the_browser_takes_no_keys_while_saving() {
    let fixture = Fixture::new();
    let _data = data_folder(&fixture, "keys", &[preset("A"), preset("B")]);
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    harness.step();
    press_key(&mut harness, egui::Key::ArrowDown);
    assert_eq!(calls(&fixture), [format!("load {}", preset("A"))]);

    click(&mut harness, centre(fixture.header().save_button));
    type_text(&mut harness, "Name");
    press_key(&mut harness, egui::Key::ArrowDown);
    press_key(&mut harness, egui::Key::ArrowLeft);
    press_key(&mut harness, egui::Key::Escape);
    assert!(calls(&fixture).is_empty());
    assert!(browser_visible(&fixture));
    assert_eq!(save_state(&fixture), (true, false));
}

/// A data folder holding wavetables: in its Wavetables folder, a folder
/// inside that, and a bank's.
fn wavetable_data(fixture: &Fixture, tag: &str) -> Folder {
    let folder = Folder::new(tag, &[]);
    for file in [
        "Wavetables/Glass 2.vitaltable",
        "Wavetables/glass 10.vitaltable",
        "Wavetables/Loop.wav",
        "Wavetables/Vocal/Vox A.vitaltable",
        "Wavetables/readme.txt",
        "Bank/Wavetables/Bank Table.vitaltable",
    ] {
        let path = folder.0.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"version":"1.0.0","name":"Inside","groups":[]}"#).unwrap();
    }
    *fixture.synth.data_directory.lock().unwrap() = Some(folder.0.clone());
    folder
}

/// Oscillator 1's layout and its popup browser's
/// (`OscillatorSection::textMouseDown`: 600 × 400 from the unison voices'
/// left, level with the name).
fn oscillator_browser(fixture: &Fixture) -> (OscillatorLayout, PopupBrowserLayout) {
    let layout = FullLayout::new(&fixture.skin, 1400, 820);
    let top = Scope::top(&fixture.skin, layout.size_ratio);
    let synthesis = SynthesisLayout::new(&top, layout.main);
    let osc = OscillatorLayout::new(
        &top.within(oscillator::SKIN),
        synthesis.oscillators[0],
        false,
    );
    let browser = Rect::new(osc.unison_voices.x, osc.preset_selector.bounds.y, 600, 400);
    let scope = top.within(popup_browser::SKIN);
    let padding = scope.value(sas_gui::skin::ValueId::Padding);
    (osc, PopupBrowserLayout::new(1.0, padding, browser))
}

/// The centre of row `row` of a list at `list` (rows 24 pixels high).
fn popup_row(list: Rect, row: i32) -> Pos2 {
    Pos2::new(
        list.x as f32 + list.width as f32 / 2.0,
        (list.y + 24 * row + 12) as f32,
    )
}

fn stems(rows: &[Selection]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Selection::All => "All".to_string(),
            Selection::Favourites => "Favorites".to_string(),
            Selection::Path(path) => path.file_stem().unwrap().to_string_lossy().into_owned(),
        })
        .collect()
}

fn browser_rows(fixture: &Fixture) -> Option<(Vec<String>, Vec<String>)> {
    let rows = fixture.interface.borrow().popup_browser_rows();
    rows.map(|(folders, files)| (stems(&folders), stems(&files)))
}

/// A press on oscillator 1's wavetable name opens the popup browser beside
/// it: All, Favorites and the wavetable folders (a folder named Wavetables
/// shows its parent's name), and with All chosen every wavetable file,
/// `.vitaltable`, `.wav` or `.flac`, in any folder, by full path
/// (`loadPresets`; the bank's folder sorts before the root's). A file chosen in it loads into the oscillator, a
/// `.vitaltable` named after its file and audio spliced (`loadFile`).
#[test]
fn the_wavetable_name_opens_the_popup_browser_and_a_file_loads() {
    let fixture = Fixture::new();
    let data = wavetable_data(&fixture, "popup-open");
    let mut harness = fixture.harness();
    let (osc, browser) = oscillator_browser(&fixture);
    assert_eq!(browser_rows(&fixture), None);
    click(&mut harness, centre(osc.preset_selector.text));
    let rows = fixture.interface.borrow().popup_browser_rows().unwrap();
    assert_eq!(
        rows.0,
        [
            Selection::All,
            Selection::Favourites,
            Selection::Path(data.0.join("Bank/Wavetables")),
            Selection::Path(data.0.join("Wavetables")),
        ]
    );
    assert_eq!(
        stems(&rows.1),
        ["Bank Table", "Glass 2", "glass 10", "Loop", "Vox A"]
    );

    click(&mut harness, popup_row(browser.files, 2));
    assert_eq!(calls(&fixture), ["wavetable 0 glass 10"]);
    click(&mut harness, popup_row(browser.files, 3));
    assert_eq!(calls(&fixture), ["audio 0 Loop.wav"]);
    assert!(browser_rows(&fixture).is_some());
}

/// A `.vitaltable` that doesn't parse loads nothing, leaving the oscillator
/// as it was (D21).
#[test]
fn an_unreadable_wavetable_file_leaves_the_oscillator() {
    let fixture = Fixture::new();
    let data = wavetable_data(&fixture, "popup-broken");
    std::fs::write(data.0.join("Wavetables/glass 10.vitaltable"), "{not json").unwrap();
    let mut harness = fixture.harness();
    let (osc, browser) = oscillator_browser(&fixture);
    click(&mut harness, centre(osc.preset_selector.text));
    click(&mut harness, popup_row(browser.files, 2));
    assert!(calls(&fixture).is_empty());
    click(&mut harness, popup_row(browser.files, 1));
    assert_eq!(calls(&fixture), ["wavetable 0 Glass 2"]);
}

/// The arrows load the wavetable after or before the current one among all
/// the user's, wrapping round, and the first one with none current
/// (`nextClicked`, `getShiftedFile`).
#[test]
fn the_wavetable_arrows_step_through_the_users_wavetables() {
    let fixture = Fixture::new();
    let _data = wavetable_data(&fixture, "popup-arrows");
    let mut harness = fixture.harness();
    let (osc, _) = oscillator_browser(&fixture);
    click(&mut harness, centre(osc.preset_selector.next));
    assert_eq!(calls(&fixture), ["wavetable 0 Bank Table"]);
    click(&mut harness, centre(osc.preset_selector.next));
    assert_eq!(calls(&fixture), ["wavetable 0 Glass 2"]);
    click(&mut harness, centre(osc.preset_selector.previous));
    click(&mut harness, centre(osc.preset_selector.previous));
    assert_eq!(
        calls(&fixture),
        ["wavetable 0 Bank Table", "wavetable 0 Vox A"]
    );
}

/// A folder lists its files (`newSelection`); the search keeps the names
/// with every word (`filterPresets`); a double click on the selected file
/// closes the browser (`doubleClickedSelected`).
#[test]
fn folders_and_search_choose_what_the_browser_lists() {
    let fixture = Fixture::new();
    let _data = wavetable_data(&fixture, "popup-search");
    let mut harness = fixture.harness();
    let (osc, browser) = oscillator_browser(&fixture);
    click(&mut harness, centre(osc.preset_selector.text));
    // Sorted by full path, the bank's folder comes before the root's.
    click(&mut harness, popup_row(browser.folders, 2));
    assert_eq!(browser_rows(&fixture).unwrap().1, ["Bank Table"]);
    click(&mut harness, popup_row(browser.folders, 0));
    click(&mut harness, centre(browser.search));
    type_text(&mut harness, "glass 1");
    assert_eq!(browser_rows(&fixture).unwrap().1, ["glass 10"]);

    let row = popup_row(browser.files, 0);
    // Well after the last click, so egui counts no triple click.
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, row);
    assert_eq!(calls(&fixture), ["wavetable 0 glass 10"]);
    // The second click of a double click.
    button(&mut harness, row, true);
    button(&mut harness, row, false);
    harness.step();
    assert_eq!(browser_rows(&fixture), None);
}

/// Escape closes the browser while the search box doesn't have the
/// keyboard; a press outside the browser and the oscillator closes it and
/// reaches nothing else; a press in the oscillator's section reaches the
/// oscillator and leaves the browser open (`PopupClosingArea`).
#[test]
fn the_browser_closes_outside_its_owner_and_on_escape() {
    let fixture = Fixture::new();
    let _data = wavetable_data(&fixture, "popup-close");
    let mut harness = fixture.harness();
    let (osc, _) = oscillator_browser(&fixture);
    let name = centre(osc.preset_selector.text);
    click(&mut harness, name);
    press_key(&mut harness, egui::Key::Escape);
    assert_eq!(browser_rows(&fixture), None);

    click(&mut harness, name);
    // The oscillator's arrows, inside its section, still work.
    click(&mut harness, centre(osc.preset_selector.next));
    assert!(browser_rows(&fixture).is_some());
    assert_eq!(calls(&fixture), ["wavetable 0 Bank Table"]);

    // The header's menu button is outside: the press only closes, so no
    // menu opened there for its first row (Browse Presets) to answer.
    let menu = fixture.header().menu_button;
    click(&mut harness, centre(menu));
    assert_eq!(browser_rows(&fixture), None);
    choose_row(&mut harness, menu.x as f32, menu.bottom() as f32, 0);
    assert!(!browser_visible(&fixture));
}

/// The browser's lists take drags: one on a folder row over oscillator 1's
/// section reaches none of its controls, as upstream's `SelectionList` takes
/// the mouse.
#[test]
fn a_drag_on_the_browsers_lists_reaches_nothing_beneath() {
    let fixture = Fixture::new();
    let _data = wavetable_data(&fixture, "popup-drag");
    let mut harness = fixture.harness();
    let (osc, browser) = oscillator_browser(&fixture);
    click(&mut harness, centre(osc.preset_selector.text));
    let row = popup_row(browser.folders, 0);
    // The row lies over the oscillator's section, its passthrough area.
    assert!(row.y < osc.bounds.bottom() as f32 && row.x < osc.bounds.right() as f32);
    fixture.recorder.log.lock().unwrap().clear();
    harness.event(Event::PointerMoved(row));
    harness.step();
    button(&mut harness, row, true);
    for step in 1..=4 {
        harness.event(Event::PointerMoved(
            row + egui::vec2(0.0, 6.0 * step as f32),
        ));
        harness.step();
    }
    button(&mut harness, row + egui::vec2(0.0, 24.0), false);
    harness.step();
    assert_eq!(*fixture.recorder.log.lock().unwrap(), Vec::<String>::new());
}

/// Drags `from` 40 pixels up, step by step.
fn drag_up(harness: &mut Harness, from: Pos2) {
    harness.event(Event::PointerMoved(from));
    harness.step();
    button(harness, from, true);
    for step in 1..=4 {
        harness.event(Event::PointerMoved(
            from + egui::vec2(0.0, -10.0 * step as f32),
        ));
        harness.step();
    }
    button(harness, from + egui::vec2(0.0, -40.0), false);
    harness.step();
}

/// The save section takes a drag outside its box, as upstream's overlay
/// covers the editor: the knob beneath doesn't move, and the drag's mouse
/// up outside the box closes the section (`mouseUp`).
#[test]
fn the_save_section_takes_drags_meant_for_controls_beneath() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    click(&mut harness, centre(fixture.header().save_button));
    assert_eq!(save_state(&fixture), (true, false));
    let layout = FullLayout::new(&fixture.skin, 1400, 820);
    let top = Scope::top(&fixture.skin, layout.size_ratio);
    let synthesis = SynthesisLayout::new(&top, layout.main);
    let osc = OscillatorLayout::new(
        &top.within(oscillator::SKIN),
        synthesis.oscillators[0],
        false,
    );
    fixture.recorder.log.lock().unwrap().clear();
    drag_up(&mut harness, centre(osc.distortion_amount));
    assert_eq!(*fixture.recorder.log.lock().unwrap(), Vec::<String>::new());
    assert_eq!(save_state(&fixture), (false, false));
}

/// The delete section takes a drag outside its box: the header's volume
/// beneath doesn't move, and the drag's mouse up closes the section.
#[test]
fn the_delete_section_takes_drags_meant_for_controls_beneath() {
    let folder = copied_data("delete-drag");
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(folder.0.clone());
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    let row = list_row(&fixture, 1);
    right_click(&mut harness, row);
    choose_row(&mut harness, row.x, row.y, 2);
    harness.step();
    assert!(fixture.interface.borrow().deleting().is_some());
    fixture.recorder.log.lock().unwrap().clear();
    drag_up(&mut harness, centre(fixture.header().volume));
    assert_eq!(*fixture.recorder.log.lock().unwrap(), Vec::<String>::new());
    assert!(fixture.interface.borrow().deleting().is_none());
    assert_eq!(shown(&fixture).len(), 3);
}

/// The delete section covers the whole interface and closes on a click
/// outside its box, so our logo (the chat's toggle) isn't reachable under it,
/// and is again once it closes.
#[test]
fn the_delete_section_hides_the_chat_toggle_until_it_closes() {
    let folder = copied_data("delete-logo");
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(folder.0.clone());
    fixture
        .interface
        .borrow_mut()
        .set_preset_browser_visible(true);
    let mut harness = fixture.harness();
    assert!(
        fixture.interface.borrow().logo_shown(),
        "the preset browser keeps the logo"
    );
    let row = list_row(&fixture, 1);
    right_click(&mut harness, row);
    choose_row(&mut harness, row.x, row.y, 2);
    harness.step();
    assert!(fixture.interface.borrow().deleting().is_some());
    assert!(!fixture.interface.borrow().logo_shown());
    drag_up(&mut harness, centre(fixture.header().volume));
    assert!(fixture.interface.borrow().deleting().is_none());
    assert!(fixture.interface.borrow().logo_shown());
}
