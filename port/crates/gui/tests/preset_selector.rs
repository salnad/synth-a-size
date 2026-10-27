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
use sas_gui::layout::{FullLayout, HeaderLayout, Rect};
use sas_gui::scope::Scope;
use sas_gui::sections::preset_browser::PresetBrowserLayout;
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
        *self.active_file.lock().unwrap() = Some(path.to_path_buf());
        Ok(())
    }

    fn init_preset(&self) {
        self.calls.lock().unwrap().push("init".to_string());
        *self.active_file.lock().unwrap() = None;
        self.generation.fetch_add(1, Ordering::Relaxed);
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
/// its fifth row (after a divider).
#[test]
fn the_menu_initializes_the_preset() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let menu = fixture.header().menu_button;
    click(&mut harness, centre(menu));
    choose_row(&mut harness, menu.x as f32, menu.bottom() as f32, 4);
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
    choose_row(&mut harness, menu.x as f32, menu.bottom() as f32, 4);
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
