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
use sas_gui::controls::button::SynthButton;
use sas_gui::controls::slider::Slider;
use sas_gui::controls::tab_selector::TabSelector;
use sas_gui::layout::{FullLayout, Rect};
use sas_gui::paint::PixelPainter;
use sas_gui::scope::Scope;
use sas_gui::sections::portamento::{self, PortamentoLayout};
use sas_gui::sections::voice::{self, VoiceLayout};
use sas_gui::sections::{MenuOwner, Overlay, Panel};
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
    "pitch_wheel",
    "mod_wheel",
    "volume",
    "chorus_on",
    "distortion_type",
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
    /// Notes the on-screen keyboard sent.
    notes: Mutex<Vec<String>>,
    /// Controls a MIDI-learned controller moved, as the plugin reports them.
    midi_moved: Mutex<Vec<&'static str>>,
    /// The values the editor said it forwards for MIDI moves.
    midi_forwarded: Mutex<Vec<(String, f32)>>,
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
        Arc::new(Synth {
            params,
            notes: Mutex::new(Vec::new()),
            midi_moved: Mutex::new(Vec::new()),
            midi_forwarded: Mutex::new(Vec::new()),
        })
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

    fn note_on(&self, note: u8, velocity: f32) {
        self.notes
            .lock()
            .unwrap()
            .push(format!("on {note} {velocity:.2}"));
    }

    fn note_off(&self, note: u8) {
        self.notes.lock().unwrap().push(format!("off {note}"));
    }

    fn drain_midi_moved(&self, f: &mut dyn FnMut(&'static str)) {
        for name in std::mem::take(&mut *self.midi_moved.lock().unwrap()) {
            f(name);
        }
    }

    fn midi_move_forwarded(&self, name: &str, normalized: f32) {
        self.midi_forwarded
            .lock()
            .unwrap()
            .push((name.to_string(), normalized));
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

#[test]
fn the_keyboard_plays_notes() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let layout = {
        let interface = fixture.interface.borrow();
        FullLayout::new(interface.skin(), 1400, 820).keyboard
    };
    let keys = sas_gui::sections::keyboard::KeyboardLayout::new(layout);
    // Three quarters down the white key holding middle C (note 60).
    let white = layout.width as f32 / 75.0;
    let c4 = Pos2::new(
        layout.x as f32 + white * 35.5,
        layout.y as f32 + 0.75 * layout.height as f32,
    );
    let local_y = 0.75 * layout.height as f32;
    assert_eq!(keys.note_at(white * 35.5, local_y), 60);
    press(&mut harness, c4, PointerButton::Primary, Modifiers::NONE);
    // Dragging onto the next white key (D, note 62) moves the note.
    move_to(&mut harness, c4 + egui::vec2(white, 0.0));
    release(
        &mut harness,
        c4 + egui::vec2(white, 0.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let velocity = keys.velocity_for(60, local_y);
    assert_eq!(
        *fixture.synth.notes.lock().unwrap(),
        [
            format!("on 60 {velocity:.2}"),
            "off 60".to_string(),
            format!("on 62 {velocity:.2}"),
            "off 62".to_string(),
        ]
    );
}

fn bend_and_volume(
    fixture: &Fixture,
) -> (
    sas_gui::sections::bend::BendLayout,
    sas_gui::sections::volume::VolumeLayout,
) {
    let interface = fixture.interface.borrow();
    let layout = FullLayout::new(interface.skin(), 1400, 820);
    let top = Scope::top(interface.skin(), layout.size_ratio);
    (
        sas_gui::sections::bend::BendLayout::new(
            &top.within(sas_gui::sections::bend::SKIN),
            layout.bend,
        ),
        sas_gui::sections::volume::VolumeLayout::new(layout.header.volume),
    )
}

#[test]
fn the_pitch_wheel_springs_back_in_its_own_gesture() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (bend, _) = bend_and_volume(&fixture);
    let wheel = centre(bend.pitch_wheel);
    // A vertical drag at sensitivity 0.5: the full range takes twice the
    // wheel's height.
    let extent = bend.pitch_wheel.width.max(bend.pitch_wheel.height) as f32 / 0.5;
    press(&mut harness, wheel, PointerButton::Primary, Modifiers::NONE);
    move_to(&mut harness, wheel + egui::vec2(0.0, -30.0));
    let expected = -1.0 + 2.0 * (0.5 + 30.0 / extent);
    assert!((fixture.synth.value("pitch_wheel") - expected).abs() < 1e-4);
    release(
        &mut harness,
        wheel + egui::vec2(0.0, -30.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert_eq!(fixture.synth.value("pitch_wheel"), 0.0);
    assert_eq!(
        fixture.take_log(),
        [
            "begin pitch_wheel",
            "set pitch_wheel",
            "end pitch_wheel",
            "begin pitch_wheel",
            "set pitch_wheel",
            "end pitch_wheel"
        ]
    );

    // A click on the wheel at rest changes nothing, so nothing is set.
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, wheel);
    assert_eq!(fixture.take_log(), ["begin pitch_wheel", "end pitch_wheel"]);
}

#[test]
fn the_mod_wheel_stays_where_it_is_released() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (bend, _) = bend_and_volume(&fixture);
    let wheel = centre(bend.mod_wheel);
    let extent = bend.mod_wheel.width.max(bend.mod_wheel.height) as f32 / 0.5;
    drag(
        &mut harness,
        wheel,
        wheel + egui::vec2(0.0, -30.0),
        Modifiers::NONE,
    );
    assert!((fixture.synth.value("mod_wheel") - 30.0 / extent).abs() < 1e-4);
    assert_eq!(
        fixture.take_log(),
        ["begin mod_wheel", "set mod_wheel", "end mod_wheel"]
    );
}

#[test]
fn the_volume_arrow_drags_across() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (_, volume) = bend_and_volume(&fixture);
    let info = registry().get("volume").unwrap();
    let start = centre(volume.volume);
    // A horizontal LinearBar: the full range across the slider's width.
    drag(
        &mut harness,
        start,
        start + egui::vec2(20.0, 0.0),
        Modifiers::NONE,
    );
    let extent = volume.volume.width.max(volume.volume.height) as f32;
    let expected = info.default_value + (info.max - info.min) * 20.0 / extent;
    assert!((fixture.synth.value("volume") - expected).abs() < 1e-3);
    assert_eq!(
        fixture.take_log(),
        ["begin volume", "set volume", "end volume"]
    );
}

#[test]
fn a_held_key_is_released_when_the_editor_closes() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let layout = {
        let interface = fixture.interface.borrow();
        FullLayout::new(interface.skin(), 1400, 820).keyboard
    };
    let white = layout.width as f32 / 75.0;
    let c4 = Pos2::new(
        layout.x as f32 + white * 35.5,
        layout.y as f32 + 0.75 * layout.height as f32,
    );
    press(&mut harness, c4, PointerButton::Primary, Modifiers::NONE);
    // What the editor's close hook does.
    let playing = fixture.interface.borrow().playing_note();
    playing.release(fixture.synth.as_ref());
    let notes = fixture.synth.notes.lock().unwrap().clone();
    assert_eq!(notes.last().map(String::as_str), Some("off 60"));
    assert_eq!(playing.get(), None);
}

const POWER: Rect = Rect {
    x: 20,
    y: 20,
    width: 30,
    height: 30,
};
const SELECTOR: Rect = Rect {
    x: 100,
    y: 20,
    width: 120,
    height: 24,
};

/// The shared widgets on their own: a power button and a text selector in
/// one panel, with the interface's menu over them.
const TABS: Rect = Rect {
    x: 20,
    y: 100,
    width: 300,
    height: 30,
};

fn widget_harness<'a>(fixture: &'a Fixture, overlay: &'a RefCell<Overlay>) -> Harness<'a> {
    widget_harness_with_tabs(fixture, overlay, None)
}

fn widget_harness_with_tabs<'a>(
    fixture: &'a Fixture,
    overlay: &'a RefCell<Overlay>,
    tabs: Option<&'a std::cell::Cell<usize>>,
) -> Harness<'a> {
    let mut harness = Harness::builder()
        .with_size((400.0, 300.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .build_ui(move |ui| {
            let area = ui.ctx().content_rect();
            let host = Host::tracking(
                fixture.synth.as_ref(),
                Some(&fixture.context),
                &fixture.open,
            );
            let painter = ui.painter_at(area);
            let pixels = PixelPainter::new(&painter, area, 1.0);
            let interface = fixture.interface.borrow();
            let top = Scope::top(interface.skin(), 1.0);
            let mut overlay = overlay.borrow_mut();
            let mut panel = Panel::new(
                ui,
                &host,
                &pixels,
                top,
                &mut overlay,
                Rect::new(0, 0, 400, 300),
            );
            panel.button(&SynthButton::power("chorus_on"), POWER);
            panel.slider(&Slider::selector("distortion_type", None), SELECTOR);
            panel.finish();
            if let Some(tabs) = tabs {
                let selector = TabSelector::new(&["LOW", "BAND", "HIGH"]);
                let mut selected = tabs.get();
                let id = ui.scope_id().with("bands");
                selector.interact(ui, &pixels, id, TABS, &mut selected);
                tabs.set(selected);
                selector.paint(&pixels, &top, TABS, selected);
            }
            overlay.show_menu(ui, &host, &pixels, &top, Rect::new(0, 0, 400, 300));
        });
    harness.step();
    harness
}

#[test]
fn the_power_button_toggles_in_one_gesture() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let mut harness = widget_harness(&fixture, &overlay);
    click(&mut harness, centre(POWER));
    assert_eq!(fixture.synth.value("chorus_on"), 1.0);
    assert_eq!(
        fixture.take_log(),
        ["begin chorus_on", "set chorus_on", "end chorus_on"]
    );
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, centre(POWER));
    assert_eq!(fixture.synth.value("chorus_on"), 0.0);
}

#[test]
fn a_text_selector_lists_its_options_below_it() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let mut harness = widget_harness(&fixture, &overlay);
    click(&mut harness, centre(SELECTOR));
    // Opening the list is not an edit.
    assert!(fixture.take_log().is_empty());
    {
        let overlay = overlay.borrow();
        let menu = overlay.menu.as_ref().unwrap();
        assert!(
            matches!(menu.owner, MenuOwner::Selector(slider) if slider.name == "distortion_type")
        );
        let names: Vec<_> = menu.items.iter().map(|item| item.name.as_str()).collect();
        let options = registry()
            .get("distortion_type")
            .unwrap()
            .options()
            .unwrap();
        assert_eq!(names, &options[..6]);
        assert_eq!(
            menu.position,
            Pos2::new(SELECTOR.x as f32, SELECTOR.bottom() as f32)
        );
    }

    // Choosing an option sets it in one gesture. Rows are 24 pixels from
    // the top of the list, as in the stereo mode test.
    let third_row = Pos2::new(SELECTOR.x as f32 + 20.0, SELECTOR.bottom() as f32 + 60.0);
    harness.event(Event::PointerMoved(third_row));
    harness.step();
    click(&mut harness, third_row);
    assert_eq!(fixture.synth.value("distortion_type"), 2.0);
    assert_eq!(
        fixture.take_log(),
        [
            "begin distortion_type",
            "set distortion_type",
            "end distortion_type"
        ]
    );
    assert!(overlay.borrow().menu.is_none());
}

#[test]
fn a_right_click_on_a_text_selector_opens_the_slider_menu() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let mut harness = widget_harness(&fixture, &overlay);
    press(
        &mut harness,
        centre(SELECTOR),
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        centre(SELECTOR),
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    let overlay = overlay.borrow();
    assert!(matches!(
        overlay.menu.as_ref().unwrap().owner,
        MenuOwner::Slider(..)
    ));
}

#[test]
fn a_tab_selector_follows_the_pointer_while_down() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let selected = std::cell::Cell::new(0);
    let mut harness = widget_harness_with_tabs(&fixture, &overlay, Some(&selected));
    let y = centre(TABS).y;
    let third = Pos2::new(TABS.x as f32 + 250.0, y);
    let second = Pos2::new(TABS.x as f32 + 150.0, y);
    press(&mut harness, third, PointerButton::Primary, Modifiers::NONE);
    assert_eq!(selected.get(), 2);
    move_to(&mut harness, second);
    assert_eq!(selected.get(), 1);
    // Dragging off the end clamps to the last tab.
    move_to(&mut harness, Pos2::new(TABS.right() as f32 + 40.0, y));
    assert_eq!(selected.get(), 2);
    release(
        &mut harness,
        Pos2::new(TABS.right() as f32 + 40.0, y),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, second);
    assert_eq!(selected.get(), 2);
    // The band is GUI state, not a host edit.
    assert!(fixture.take_log().is_empty());
}

/// A control a MIDI-learned controller moved reaches the host as one
/// gesture with its current value, which the gesture leaves unchanged.
#[test]
fn learned_midi_moves_reach_the_host_as_gestures() {
    let fixture = Fixture::new();
    unsafe {
        fixture.synth.params["volume"]
            .ptr()
            ._internal_set_normalized_value(0.25)
    };
    let moved_to = fixture.synth.value("volume");
    fixture.synth.midi_moved.lock().unwrap().push("volume");
    let host = Host::tracking(
        fixture.synth.as_ref(),
        Some(&fixture.context),
        &fixture.open,
    );
    host.forward_midi_moves();
    assert_eq!(
        *fixture.recorder.log.lock().unwrap(),
        ["begin volume", "set volume", "end volume"]
    );
    assert_eq!(fixture.synth.value("volume"), moved_to);
    assert!(fixture.open.lock().unwrap().is_empty());
    // The plugin is told the exact normalized value sent, for its echo guard.
    assert_eq!(
        *fixture.synth.midi_forwarded.lock().unwrap(),
        [("volume".to_string(), 0.25)]
    );

    // Nothing moved since: nothing sent.
    fixture.recorder.log.lock().unwrap().clear();
    host.forward_midi_moves();
    assert!(fixture.recorder.log.lock().unwrap().is_empty());
}

/// A move of a control the user is dragging joins the drag's gesture
/// instead of nesting another.
#[test]
fn a_learned_move_during_a_drag_sends_only_the_value() {
    let fixture = Fixture::new();
    let host = Host::tracking(
        fixture.synth.as_ref(),
        Some(&fixture.context),
        &fixture.open,
    );
    host.begin("polyphony");
    fixture.recorder.log.lock().unwrap().clear();
    fixture.synth.midi_moved.lock().unwrap().push("polyphony");
    host.forward_midi_moves();
    assert_eq!(*fixture.recorder.log.lock().unwrap(), ["set polyphony"]);
    assert!(fixture.open.lock().unwrap().contains("polyphony"));
    host.end("polyphony");
}
