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

/// The arrows step through the active preset's folder in natural order,
/// wrapping round (`PresetBrowser::jumpToPreset`).
#[test]
fn the_arrows_step_through_the_active_presets_folder() {
    let folder = Folder::new(
        "arrows",
        &[preset("Bass 10"), preset("bass 2"), preset("Lead")],
    );
    let fixture = Fixture::new();
    *fixture.synth.active_file.lock().unwrap() =
        Some(folder.0.join("Presets").join(preset("bass 2")));
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

/// Without an active preset, the arrows start from the user preset folder
/// under the data folder: previous loads its last preset.
#[test]
fn without_an_active_preset_the_arrows_start_from_the_user_folder() {
    let folder = Folder::new("user", &[preset("b"), preset("a"), "c.txt".to_string()]);
    let fixture = Fixture::new();
    *fixture.synth.data_directory.lock().unwrap() = Some(folder.0.clone());
    let mut harness = fixture.harness();
    let header = fixture.header();
    click(&mut harness, centre(header.preset.previous));
    assert_eq!(
        *fixture.synth.calls.lock().unwrap(),
        [format!("load {}", preset("b"))]
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
