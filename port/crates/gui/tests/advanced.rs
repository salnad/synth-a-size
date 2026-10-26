// SPDX-License-Identifier: GPL-3.0-or-later

//! The Advanced tab's controls with a host that records gestures: the
//! settings panels, the tuning list and the oscillators' advanced sections.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use nice_plug::context::gui::{GuiContext, GuiContextInner};
use nice_plug::context::PluginApi;
use nice_plug::prelude::{FloatParam, FloatRange, Param, ParamPtr, PluginState};
use sas_engine::Tuning;
use sas_gui::layout::{FullLayout, Rect};
use sas_gui::scope::Scope;
use sas_gui::sections::advanced::{
    self, AdvancedLayout, DisplaySettingsLayout, OversampleSettingsLayout, VoiceSettingsLayout,
};
use sas_gui::sections::oscillator_advanced::OscillatorAdvancedLayout;
use sas_gui::skin::Skin;
use sas_gui::{EditorModel, Host, HostParam, Interface, OpenGestures, Tab};
use sas_preset::registry::registry;

/// Every registry control, linear over its range, and the editor settings.
struct Synth {
    params: BTreeMap<&'static str, FloatParam>,
    hz_frequency: AtomicBool,
    tuning: Mutex<Tuning>,
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
        })
    }

    fn name_of(&self, ptr: ParamPtr) -> &'static str {
        self.params
            .iter()
            .find(|(_, param)| param.as_ptr() == ptr)
            .map(|(name, _)| *name)
            .unwrap()
    }

    fn value(&self, name: &str) -> f32 {
        unsafe { self.params[name].as_ptr().unmodulated_plain_value() }
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
        interface.set_tab(Tab::Advanced);
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

    fn take_log(&self) -> Vec<String> {
        let mut log = std::mem::take(&mut *self.recorder.log.lock().unwrap());
        log.dedup();
        log
    }

    fn layout(&self) -> (AdvancedLayout, Scope<'_>) {
        let layout = FullLayout::new(&self.skin, 1400, 820);
        let top = Scope::top(&self.skin, layout.size_ratio);
        (
            AdvancedLayout::new(&top, layout.main),
            top.within(advanced::SKIN),
        )
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

/// FREQUENCY UNITS lists Semitones and Hz below itself; choosing Hz turns
/// the editor setting on, with no parameter changed.
#[test]
fn the_frequency_units_set_the_editor_setting() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (tab, scope) = fixture.layout();
    let display = DisplaySettingsLayout::new(&scope, tab.display);
    click(&mut harness, centre(display.frequency_display));
    choose_row(
        &mut harness,
        display.frequency_display.x as f32,
        display.frequency_display.bottom() as f32,
        1,
    );
    assert!(fixture.synth.hz_frequency_display());
    assert_eq!(fixture.take_log(), Vec::<String>::new());
}

/// An oversampling button sets `oversampling` to its amount in one gesture.
#[test]
fn an_oversampling_button_sets_the_amount_in_one_gesture() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (tab, scope) = fixture.layout();
    let oversample = OversampleSettingsLayout::new(&scope, tab.oversample);
    click(&mut harness, centre(oversample.buttons[2]));
    harness.step();
    assert_eq!(fixture.synth.value("oversampling"), 2.0);
    assert_eq!(
        fixture.take_log(),
        ["begin oversampling", "set oversampling", "end oversampling"]
    );
}

/// The tuning list opens at the pointer; a built-in tuning loads named
/// after itself.
#[test]
fn a_built_in_tuning_loads_named_after_itself() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (tab, scope) = fixture.layout();
    let voice = VoiceSettingsLayout::new(&scope, tab.voice);
    let at = centre(voice.tuning);
    click(&mut harness, at);
    choose_row(&mut harness, at.x, at.y, 2);
    assert_eq!(fixture.synth.tuning_name(), "Just - 5 Limit");
    let tuning = fixture.synth.tuning();
    assert!(!tuning.is_default());
    // 5-limit's major third, 5/4, above middle C.
    let third = tuning.convert_midi_note(64) - tuning.convert_midi_note(60);
    assert!(
        (third - 12.0 * (5.0f32 / 4.0).log2()).abs() < 1e-3,
        "{third}"
    );
}

/// NOTE TRACK toggles its oscillator's control in one gesture.
#[test]
fn note_track_toggles_in_one_gesture() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (tab, scope) = fixture.layout();
    let osc = OscillatorAdvancedLayout::new(&scope, tab.oscillators[0]);
    let before = fixture.synth.value("osc_1_midi_track");
    click(&mut harness, centre(osc.midi_track));
    harness.step();
    assert_ne!(fixture.synth.value("osc_1_midi_track"), before);
    assert_eq!(
        fixture.take_log(),
        [
            "begin osc_1_midi_track",
            "set osc_1_midi_track",
            "end osc_1_midi_track"
        ]
    );
}
