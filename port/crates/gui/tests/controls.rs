//! The Voice and Portamento controls against real host parameters, driven
//! through egui_kittest (no GPU needed). A recording `GuiContextInner`
//! stands in for the host: it logs every gesture and applies each value as
//! nice-plug's wrapper would.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use nice_plug::context::gui::{GuiContext, GuiContextInner};
use nice_plug::context::PluginApi;
use nice_plug::prelude::PluginState;
use nice_plug::prelude::{FloatParam, FloatRange, IntParam, IntRange, Param, ParamPtr};
use sas_gui::layout::{FullLayout, Rect};
use sas_gui::scope::Scope;
use sas_gui::sections::portamento::{self, PortamentoLayout};
use sas_gui::sections::voice::{self, VoiceLayout};
use sas_gui::{EditorModel, Host, HostParam, Interface, OpenGestures};
use sas_preset::registry::{registry, ValueScale};

const CONTROLS: &[&str] = &[
    "polyphony",
    "pitch_bend_range",
    "velocity_track",
    "stereo_routing",
    "stereo_mode",
    "portamento_time",
    "portamento_slope",
    "portamento_force",
    "portamento_scale",
    "legato",
];

enum TestParam {
    Float(FloatParam),
    Int(IntParam),
}

impl TestParam {
    fn ptr(&self) -> ParamPtr {
        match self {
            TestParam::Float(param) => param.as_ptr(),
            TestParam::Int(param) => param.as_ptr(),
        }
    }
}

/// The synth's parameters as the plugin makes them: continuous controls
/// are linear over the registry range, indexed ones are integers.
struct Synth {
    params: HashMap<&'static str, Box<TestParam>>,
}

impl Synth {
    fn new() -> Arc<Synth> {
        let params = CONTROLS
            .iter()
            .map(|&name| {
                let info = registry().get(name).unwrap();
                let param = if info.value_scale == ValueScale::Indexed {
                    TestParam::Int(IntParam::new(
                        name,
                        info.default_value as i32,
                        IntRange::Linear {
                            min: info.min as i32,
                            max: info.max as i32,
                        },
                    ))
                } else {
                    TestParam::Float(FloatParam::new(
                        name,
                        info.default_value,
                        FloatRange::Linear {
                            min: info.min,
                            max: info.max,
                        },
                    ))
                };
                (name, Box::new(param))
            })
            .collect();
        Arc::new(Synth { params })
    }

    fn name_of(&self, ptr: ParamPtr) -> &'static str {
        self.params
            .iter()
            .find(|(_, param)| param.ptr() == ptr)
            .map(|(name, _)| *name)
            .unwrap()
    }

    fn value(&self, name: &str) -> f32 {
        unsafe { self.params[name].ptr().unmodulated_plain_value() }
    }
}

impl EditorModel for Synth {
    fn preset_name(&self) -> String {
        String::new()
    }

    fn param(&self, name: &str) -> Option<HostParam> {
        self.params.get(name).map(|param| param.ptr())
    }
}

/// The host: logs gestures and sets values.
struct Recorder {
    synth: Arc<Synth>,
    log: Mutex<Vec<String>>,
}

impl GuiContextInner for Recorder {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }

    unsafe fn raw_begin_set_parameter(&self, param: ParamPtr) {
        self.log
            .lock()
            .unwrap()
            .push(format!("begin {}", self.synth.name_of(param)));
    }

    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
        unsafe { param._internal_set_normalized_value(normalized) };
        self.log
            .lock()
            .unwrap()
            .push(format!("set {}", self.synth.name_of(param)));
    }

    unsafe fn raw_end_set_parameter(&self, param: ParamPtr) {
        self.log
            .lock()
            .unwrap()
            .push(format!("end {}", self.synth.name_of(param)));
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
}

impl Fixture {
    fn new() -> Fixture {
        let synth = Synth::new();
        let recorder = Arc::new(Recorder {
            synth: synth.clone(),
            log: Mutex::new(Vec::new()),
        });
        let context = GuiContext::new(recorder.clone());
        Fixture {
            synth,
            recorder,
            context,
            open: OpenGestures::default(),
            interface: RefCell::new(Interface::default()),
        }
    }

    fn harness(&self) -> Harness<'_> {
        let mut harness = Harness::builder()
            .with_size((1400.0, 820.0))
            .with_step_dt(1.0 / 60.0)
            .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
            .build_ui(|ui| {
                let screen = ui.ctx().content_rect();
                let host = Host::tracking(self.synth.as_ref(), Some(&self.context), &self.open);
                self.interface.borrow_mut().show_in(ui, screen, &host)
            });
        harness.step();
        harness
    }

    /// The gestures logged since the last call, with repeated sets of the
    /// same control collapsed.
    fn take_log(&self) -> Vec<String> {
        let mut log = std::mem::take(&mut *self.recorder.log.lock().unwrap());
        log.dedup();
        log
    }

    fn layouts(&self) -> (VoiceLayout, PortamentoLayout) {
        let interface = self.interface.borrow();
        let layout = FullLayout::new(interface.skin(), 1400, 820);
        let top = Scope::top(interface.skin(), layout.size_ratio);
        (
            VoiceLayout::new(&top.within(voice::SKIN), layout.voice),
            PortamentoLayout::new(&top.within(portamento::SKIN), layout.portamento),
        )
    }
}

fn centre(rect: Rect) -> Pos2 {
    Pos2::new(
        rect.x as f32 + rect.width as f32 / 2.0,
        rect.y as f32 + rect.height as f32 / 2.0,
    )
}

fn press(harness: &mut Harness, pos: Pos2, button: PointerButton, modifiers: Modifiers) {
    harness.event(Event::ModifiersChanged(modifiers));
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button,
        pressed: true,
        modifiers,
    });
    harness.step();
}

fn release(harness: &mut Harness, pos: Pos2, button: PointerButton, modifiers: Modifiers) {
    harness.event(Event::PointerButton {
        pos,
        button,
        pressed: false,
        modifiers,
    });
    harness.step();
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
}

fn drag(harness: &mut Harness, from: Pos2, to: Pos2, modifiers: Modifiers) {
    press(harness, from, PointerButton::Primary, modifiers);
    for step in 1..=4 {
        let t = step as f32 / 4.0;
        harness.event(Event::PointerMoved(from + (to - from) * t));
        harness.step();
    }
    release(harness, to, PointerButton::Primary, modifiers);
}

fn move_to(harness: &mut Harness, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.step();
}

fn key(harness: &mut Harness, key: Key) {
    harness.event(Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

fn click(harness: &mut Harness, pos: Pos2) {
    press(harness, pos, PointerButton::Primary, Modifiers::NONE);
    release(harness, pos, PointerButton::Primary, Modifiers::NONE);
}

#[test]
fn dragging_a_knob_is_one_host_gesture() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    let start = centre(voice.velocity_track);

    // Up 20 and right 20 pixels: 40 of the 200 a full turn takes.
    drag(
        &mut harness,
        start,
        start + egui::vec2(20.0, -20.0),
        Modifiers::NONE,
    );
    assert_eq!(
        fixture.take_log(),
        [
            "begin velocity_track",
            "set velocity_track",
            "end velocity_track"
        ]
    );
    assert!((fixture.synth.value("velocity_track") - 0.4).abs() < 1e-4);

    // The command key slows the drag tenfold.
    drag(
        &mut harness,
        start,
        start + egui::vec2(0.0, -40.0),
        Modifiers::COMMAND,
    );
    assert!((fixture.synth.value("velocity_track") - 0.44).abs() < 1e-4);
}

#[test]
fn text_sliders_drag_slowly_and_snap_to_whole_values() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    let start = centre(voice.polyphony);
    let info = registry().get("polyphony").unwrap();
    let range = (info.max - info.min) as f64;

    // Sensitivity 0.2: 50 pixels move 50 / 1000 of the range.
    drag(
        &mut harness,
        start,
        start + egui::vec2(0.0, -50.0),
        Modifiers::NONE,
    );
    let proportion = (info.default_value - info.min) as f64 / range + 0.05;
    let expected = info.min as f64 + (proportion * range + 0.5).floor();
    assert_eq!(fixture.synth.value("polyphony") as f64, expected);
}

#[test]
fn double_click_returns_to_the_default() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (_, portamento) = fixture.layouts();
    let start = centre(portamento.portamento_time);
    drag(
        &mut harness,
        start,
        start + egui::vec2(0.0, -60.0),
        Modifiers::NONE,
    );
    let default = registry().get("portamento_time").unwrap().default_value;
    assert_ne!(fixture.synth.value("portamento_time"), default);
    fixture.take_log();

    // Past the double-click timeout, so the drag's press does not pair
    // with the next one.
    for _ in 0..30 {
        harness.step();
    }

    click(&mut harness, start);
    assert_ne!(fixture.synth.value("portamento_time"), default);
    // JUCE calls mouseDoubleClick after the second release
    // (Component::internalMouseUp), once that click's gesture has ended.
    press(&mut harness, start, PointerButton::Primary, Modifiers::NONE);
    assert_ne!(fixture.synth.value("portamento_time"), default);
    release(&mut harness, start, PointerButton::Primary, Modifiers::NONE);
    assert_eq!(fixture.synth.value("portamento_time"), default);
    assert_eq!(
        fixture.take_log(),
        [
            "begin portamento_time",
            "end portamento_time",
            "begin portamento_time",
            "end portamento_time",
            "begin portamento_time",
            "set portamento_time",
            "end portamento_time"
        ]
    );
}

#[test]
fn the_value_popup_follows_the_drag_and_hides_on_exit() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    let start = centre(voice.velocity_track);
    press(&mut harness, start, PointerButton::Primary, Modifiers::NONE);
    harness.event(Event::PointerMoved(start + egui::vec2(0.0, -50.0)));
    harness.step();
    {
        let interface = fixture.interface.borrow();
        let (name, popup) = interface
            .overlay()
            .value_popup
            .as_ref()
            .expect("a popup while dragging");
        assert_eq!(*name, "velocity_track");
        assert_eq!(popup.text, "50.00%");
        assert_eq!(popup.source, voice.velocity_track);
    }
    release(
        &mut harness,
        start + egui::vec2(0.0, -50.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    harness.event(Event::PointerMoved(Pos2::new(5.0, 5.0)));
    harness.step();
    assert!(fixture.interface.borrow().overlay().value_popup.is_none());
}

#[test]
fn buttons_toggle_on_release_inside_one_gesture() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (_, portamento) = fixture.layouts();
    let before = fixture.synth.value("legato");
    click(&mut harness, centre(portamento.legato));
    assert_eq!(
        fixture.take_log(),
        ["begin legato", "set legato", "end legato"]
    );
    assert_eq!(fixture.synth.value("legato"), 1.0 - before);
}

#[test]
fn the_right_click_menu_sets_the_default() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    let knob = centre(voice.stereo_routing);
    drag(
        &mut harness,
        knob,
        knob + egui::vec2(0.0, 30.0),
        Modifiers::NONE,
    );
    fixture.take_log();

    press(
        &mut harness,
        knob,
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        knob,
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    assert!(fixture.interface.borrow().overlay().menu.is_some());
    // Menus open with their top left corner at the click. The first row
    // ("Set to Default Value") starts below the body's rounding.
    let first_row = knob + egui::vec2(30.0, 14.0);
    harness.event(Event::PointerMoved(first_row));
    harness.step();
    click(&mut harness, first_row);
    assert!(fixture.interface.borrow().overlay().menu.is_none());
    assert_eq!(
        fixture.take_log(),
        [
            "begin stereo_routing",
            "set stereo_routing",
            "end stereo_routing"
        ]
    );
    assert_eq!(
        fixture.synth.value("stereo_routing"),
        registry().get("stereo_routing").unwrap().default_value
    );
}

#[test]
fn the_stereo_mode_selector_sets_the_mode() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    press(
        &mut harness,
        centre(voice.stereo_mode),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        centre(voice.stereo_mode),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(fixture.interface.borrow().overlay().menu.is_some());
    // Rows are 24 pixels from the top of the list, under the body rounding.
    let second_row = Pos2::new(
        voice.stereo_mode.x as f32 + 20.0,
        voice.stereo_mode.bottom() as f32 + 36.0,
    );
    harness.event(Event::PointerMoved(second_row));
    harness.step();
    click(&mut harness, second_row);
    assert_eq!(fixture.synth.value("stereo_mode"), 1.0);
}

#[test]
fn alt_click_enters_a_value_as_text() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    let knob = centre(voice.velocity_track);
    press(&mut harness, knob, PointerButton::Primary, Modifiers::ALT);
    release(&mut harness, knob, PointerButton::Primary, Modifiers::ALT);
    harness.step();
    assert!(fixture.interface.borrow().overlay().text_entry.is_some());

    // The entry starts with its text selected, so typing replaces it.
    harness.event(Event::Text("25".to_string()));
    harness.step();
    harness.event(Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    harness.step();
    assert!(fixture.interface.borrow().overlay().text_entry.is_none());
    assert!((fixture.synth.value("velocity_track") - 0.25).abs() < 1e-6);
}

#[test]
fn command_mid_drag_continues_from_where_the_drag_is() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    let start = centre(voice.velocity_track);
    press(&mut harness, start, PointerButton::Primary, Modifiers::NONE);
    move_to(&mut harness, start + egui::vec2(0.0, -40.0));
    assert!((fixture.synth.value("velocity_track") - 0.4).abs() < 1e-4);
    // Fine adjustment from here on: 40 more pixels at a tenth of the speed.
    harness.event(Event::ModifiersChanged(Modifiers::COMMAND));
    move_to(&mut harness, start + egui::vec2(0.0, -41.0));
    assert!((fixture.synth.value("velocity_track") - 0.401).abs() < 1e-4);
    move_to(&mut harness, start + egui::vec2(0.0, -80.0));
    release(
        &mut harness,
        start + egui::vec2(0.0, -80.0),
        PointerButton::Primary,
        Modifiers::COMMAND,
    );
    assert!((fixture.synth.value("velocity_track") - 0.44).abs() < 1e-4);
}

#[test]
fn dragging_past_the_end_has_no_dead_zone() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    let start = centre(voice.velocity_track);
    press(&mut harness, start, PointerButton::Primary, Modifiers::NONE);
    move_to(&mut harness, start + egui::vec2(0.0, -400.0));
    assert_eq!(fixture.synth.value("velocity_track"), 1.0);
    // Clamped at every step: coming back 20 pixels moves off the end at once.
    move_to(&mut harness, start + egui::vec2(0.0, -380.0));
    release(
        &mut harness,
        start + egui::vec2(0.0, -380.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!((fixture.synth.value("velocity_track") - 0.8).abs() < 1e-4);
}

#[test]
fn alt_mid_drag_pauses_the_drag() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    let start = centre(voice.velocity_track);
    press(&mut harness, start, PointerButton::Primary, Modifiers::NONE);
    move_to(&mut harness, start + egui::vec2(0.0, -20.0));
    harness.event(Event::ModifiersChanged(Modifiers::ALT));
    move_to(&mut harness, start + egui::vec2(0.0, -60.0));
    assert!((fixture.synth.value("velocity_track") - 0.2).abs() < 1e-4);
    // SynthSlider::mouseDrag returns early, so JUCE's drag start stays put:
    // the next drag counts from the last one it handled.
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    move_to(&mut harness, start + egui::vec2(0.0, -70.0));
    release(
        &mut harness,
        start + egui::vec2(0.0, -70.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!((fixture.synth.value("velocity_track") - 0.7).abs() < 1e-4);
}

#[test]
fn text_slider_popups_name_the_parameter() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, portamento) = fixture.layouts();
    for (rect, name, expected) in [
        (voice.polyphony, "polyphony", "Polyphony"),
        (
            voice.pitch_bend_range,
            "pitch_bend_range",
            "Pitch Bend Range",
        ),
    ] {
        press(
            &mut harness,
            centre(rect),
            PointerButton::Primary,
            Modifiers::NONE,
        );
        {
            let interface = fixture.interface.borrow();
            let (popup_for, popup) = interface.overlay().value_popup.as_ref().unwrap();
            assert_eq!(*popup_for, name);
            assert_eq!(popup.text, expected);
        }
        release(
            &mut harness,
            centre(rect),
            PointerButton::Primary,
            Modifiers::NONE,
        );
    }
    // A curve slider is not a text slider: its popup shows the value.
    press(
        &mut harness,
        centre(portamento.portamento_slope),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let interface = fixture.interface.borrow();
    let (_, popup) = interface.overlay().value_popup.as_ref().unwrap();
    assert_eq!(popup.text, "0.000");
}

#[test]
fn escape_keeps_the_text_entry_open() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    let knob = centre(voice.velocity_track);
    press(&mut harness, knob, PointerButton::Primary, Modifiers::ALT);
    release(&mut harness, knob, PointerButton::Primary, Modifiers::ALT);
    harness.step();
    harness.event(Event::Text("50".to_string()));
    harness.step();
    key(&mut harness, Key::Escape);
    harness.step();
    assert!(fixture.interface.borrow().overlay().text_entry.is_some());
    assert_eq!(fixture.synth.value("velocity_track"), 0.0);
    key(&mut harness, Key::Enter);
    harness.step();
    assert!(fixture.interface.borrow().overlay().text_entry.is_none());
    assert!((fixture.synth.value("velocity_track") - 0.5).abs() < 1e-6);
}

#[test]
fn buttons_toggle_on_any_release_over_them() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (_, portamento) = fixture.layouts();
    let button = centre(portamento.legato);
    let before = fixture.synth.value("legato");
    // Out and back before releasing: still a click to JUCE.
    press(
        &mut harness,
        button,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, button + egui::vec2(0.0, 60.0));
    move_to(&mut harness, button);
    release(
        &mut harness,
        button,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert_eq!(fixture.synth.value("legato"), 1.0 - before);
    fixture.take_log();

    // Released away from it: no toggle, but the gesture still ends.
    press(
        &mut harness,
        button,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, button + egui::vec2(0.0, 60.0));
    release(
        &mut harness,
        button + egui::vec2(0.0, 60.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert_eq!(fixture.synth.value("legato"), 1.0 - before);
    assert_eq!(fixture.take_log(), ["begin legato", "end legato"]);

    // A right press opens no host gesture.
    press(
        &mut harness,
        button,
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        button,
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    assert!(fixture.take_log().is_empty());
}

#[test]
fn closing_mid_drag_ends_the_gesture() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (voice, _) = fixture.layouts();
    let start = centre(voice.velocity_track);
    press(&mut harness, start, PointerButton::Primary, Modifiers::NONE);
    move_to(&mut harness, start + egui::vec2(0.0, -20.0));
    assert_eq!(
        fixture.take_log(),
        ["begin velocity_track", "set velocity_track"]
    );
    // What the editor does when its window closes.
    Host::tracking(
        fixture.synth.as_ref(),
        Some(&fixture.context),
        &fixture.open,
    )
    .end_open_gestures();
    assert_eq!(fixture.take_log(), ["end velocity_track"]);
    Host::tracking(
        fixture.synth.as_ref(),
        Some(&fixture.context),
        &fixture.open,
    )
    .end_open_gestures();
    assert!(fixture.take_log().is_empty());
}

#[test]
fn drags_are_measured_in_points() {
    // At two pixels per point, 40 points up is still 40 of the 200 a turn
    // takes, as JUCE's logical coordinates make it upstream.
    let fixture = Fixture::new();
    let mut harness = Harness::builder()
        .with_size((1400.0, 820.0))
        .with_pixels_per_point(2.0)
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .build_ui(|ui| {
            let screen = ui.ctx().content_rect();
            let host = Host::tracking(
                fixture.synth.as_ref(),
                Some(&fixture.context),
                &fixture.open,
            );
            fixture.interface.borrow_mut().show_in(ui, screen, &host)
        });
    harness.step();
    let (voice, _) = fixture.layouts();
    // The layouts are for 1400x820 pixels; at 2x the window is 2800x1640
    // pixels, so the same rectangles in points.
    let start = centre(voice.velocity_track);
    drag(
        &mut harness,
        start,
        start + egui::vec2(0.0, -40.0),
        Modifiers::NONE,
    );
    assert!((fixture.synth.value("velocity_track") - 0.4).abs() < 1e-4);
}
