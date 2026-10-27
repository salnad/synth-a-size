//! The Voice and Portamento controls against real host parameters, driven
//! through egui_kittest (no GPU needed). A recording `GuiContextInner`
//! stands in for the host: it logs every gesture and applies each value as
//! nice-plug's wrapper would.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use nice_plug::context::gui::{GuiContext, GuiContextInner};
use nice_plug::context::PluginApi;
use nice_plug::prelude::PluginState;
use nice_plug::prelude::{FloatParam, FloatRange, IntParam, IntRange, Param, ParamPtr};
use sas_common::line_generator::LineGenerator;
use sas_gui::controls::button::SynthButton;
use sas_gui::controls::envelope_editor::{EnvelopeNames, Shape, Values as EnvelopeValues};
use sas_gui::controls::slider::{Slider, SliderStyle};
use sas_gui::controls::tab_selector::TabSelector;
use sas_gui::controls::wavetable_3d::{self, Wavetable3d};
use sas_gui::layout::{FullLayout, Rect};
use sas_gui::modulation::{AmountKnob, KnobKind};
use sas_gui::paint::PixelPainter;
use sas_gui::scope::Scope;
use sas_gui::sections::envelope::{self, EnvelopeLayout};
use sas_gui::sections::extra_mod::ExtraModLayout;
use sas_gui::sections::filter::{self, FilterLayout, Kind};
use sas_gui::sections::lfo::{self, LfoLayout};
use sas_gui::sections::modulation::ModulationLayout;
use sas_gui::sections::oscillator::{self, OscillatorLayout};
use sas_gui::sections::portamento::{self, PortamentoLayout};
use sas_gui::sections::random::{self, RandomLayout};
use sas_gui::sections::sample::{self, SampleLayout};
use sas_gui::sections::synthesis::SynthesisLayout;
use sas_gui::sections::voice::{self, VoiceLayout};
use sas_gui::sections::{MenuOwner, Overlay, Panel};
use sas_gui::{EditorModel, Host, HostParam, Interface, OpenGestures, Route};
use sas_preset::registry::{registry, ValueScale};
use sas_preset::{GuiConnection, InitContent, PresetState, SampleState, SlotValues};

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
    "osc_1_on",
    "osc_1_destination",
    "osc_1_distortion_type",
    "osc_1_distortion_amount",
    "osc_1_spectral_morph_type",
    "osc_1_view_2d",
    "osc_1_wave_frame",
    "osc_1_spectral_morph_amount",
    "osc_1_distortion_phase",
    "osc_2_distortion_type",
    "sample_destination",
    "env_2_attack",
    "lfo_1_smooth_mode",
    "lfo_1_sync",
    "lfo_1_frequency",
    "random_1_stereo",
    "filter_1_on",
    "filter_1_model",
    "filter_1_style",
    "filter_1_cutoff",
    "filter_1_resonance",
    "filter_1_filter_input",
    "filter_2_filter_input",
    "osc_2_destination",
    "macro_control_1",
    "macro_control_2",
    "modulation_1_power",
    "modulation_1_amount",
    "modulation_1_bipolar",
    "modulation_1_stereo",
    "modulation_1_bypass",
    "modulation_2_power",
    "modulation_2_amount",
    "modulation_2_bipolar",
    "modulation_2_stereo",
    "modulation_2_bypass",
    "modulation_3_power",
    "modulation_3_amount",
    "modulation_3_bipolar",
    "modulation_3_stereo",
    "modulation_3_bypass",
    "env_1_delay",
    "env_1_attack",
    "env_1_hold",
    "env_1_decay",
    "env_1_sustain",
    "env_1_release",
    "env_1_attack_power",
    "env_1_decay_power",
    "env_1_release_power",
    "lfo_1_stereo",
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
    /// The LFO shapes the editor set.
    lfos: Mutex<HashMap<usize, LineGenerator>>,
    /// Wavetable loads and resets the editor asked for.
    wavetable_edits: Mutex<Vec<String>>,
    /// The "hz_frequency" setting.
    hz: AtomicBool,
    /// The sample, its content revision, and how often the editor read it.
    sample: Mutex<Option<SampleState>>,
    revision: AtomicU64,
    sample_reads: AtomicUsize,
    /// The modulation routing, as the plugin's document mirror keeps it.
    routing: Mutex<PresetState>,
    /// The modulation edits the editor sent the engine, in order.
    bus: Mutex<Vec<String>>,
    /// Bumped by a simulated preset load or state restore.
    generation: AtomicU64,
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
            lfos: Mutex::default(),
            wavetable_edits: Mutex::new(Vec::new()),
            hz: AtomicBool::new(false),
            sample: Mutex::default(),
            revision: AtomicU64::new(0),
            sample_reads: AtomicUsize::new(0),
            routing: Mutex::new(PresetState::new(&InitContent { sample_seed: 0 })),
            bus: Mutex::default(),
            generation: AtomicU64::new(0),
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

    /// Sets a parameter as a preset load would, without a gesture.
    fn set(&self, name: &str, plain: f32) {
        unsafe {
            let ptr = self.params[name].ptr();
            ptr._internal_set_normalized_value(ptr.preview_normalized(plain));
        }
    }
}

impl EditorModel for Synth {
    fn preset_name(&self) -> String {
        String::new()
    }

    fn lfo_shape(&self, index: usize) -> Option<LineGenerator> {
        self.lfos.lock().unwrap().get(&index).cloned()
    }

    fn set_lfo_shape(&self, index: usize, line: LineGenerator) -> bool {
        self.lfos.lock().unwrap().insert(index, line);
        true
    }

    fn hz_frequency_display(&self) -> bool {
        self.hz.load(Ordering::Relaxed)
    }

    fn with_sample(&self, f: &mut dyn FnMut(&SampleState)) {
        self.sample_reads.fetch_add(1, Ordering::Relaxed);
        if let Some(sample) = self.sample.lock().unwrap().as_ref() {
            f(sample);
        }
    }

    fn content_revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
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

    fn wavetable_json(&self, oscillator: usize) -> Option<String> {
        self.wavetable_edits
            .lock()
            .unwrap()
            .push(format!("json {oscillator}"));
        Some(format!(
            r#"{{"version":"1.0.0","name":"Osc {oscillator}","groups":[]}}"#
        ))
    }

    fn load_wavetable_json(&self, oscillator: usize, json: &str) -> bool {
        self.wavetable_edits
            .lock()
            .unwrap()
            .push(format!("load {oscillator} {json}"));
        true
    }

    fn audio_file_extensions(&self) -> &'static [&'static str] {
        &["wav"]
    }

    fn load_audio_as_wavetable(
        &self,
        oscillator: usize,
        path: &std::path::Path,
        style: sas_gui::controls::wavetable_3d::AudioFileLoadStyle,
    ) -> bool {
        self.wavetable_edits
            .lock()
            .unwrap()
            .push(format!("audio {oscillator} {} {style:?}", path.display()));
        true
    }

    fn load_default_wavetable(&self, oscillator: usize) {
        self.wavetable_edits
            .lock()
            .unwrap()
            .push(format!("init {oscillator}"));
    }

    fn modulations(&self) -> Arc<[Route]> {
        let routing = self.routing.lock().unwrap();
        routing
            .connection_order()
            .iter()
            .map(|&slot| {
                let connection = &routing.modulations()[slot];
                Route {
                    slot,
                    source: connection.source.clone(),
                    destination: connection.destination.clone(),
                }
            })
            .collect()
    }

    fn edit_generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    fn connect_modulation(
        &self,
        source: &str,
        destination: &str,
        values: SlotValues,
    ) -> Option<GuiConnection> {
        let connection = self
            .routing
            .lock()
            .unwrap()
            .connect_modulation_from_gui(source, destination, values)
            .ok()?;
        self.bus.lock().unwrap().push(format!(
            "connect {source} {destination} {:.2} {}",
            values.amount,
            if values.bipolar {
                "bipolar"
            } else {
                "unipolar"
            }
        ));
        Some(connection)
    }

    fn set_connection_values(&self, source: &str, destination: &str, values: SlotValues) -> bool {
        let routing = self.routing.lock().unwrap();
        let Some(slot) = routing.modulation_index(source, destination) else {
            return true;
        };
        self.bus.lock().unwrap().push(format!(
            "values {slot} {:.2} {}",
            values.amount,
            if values.bipolar {
                "bipolar"
            } else {
                "unipolar"
            }
        ));
        true
    }

    fn disconnect_modulation(&self, source: &str, destination: &str) -> Option<usize> {
        let slot = self
            .routing
            .lock()
            .unwrap()
            .remove_modulation(source, destination)?;
        self.bus
            .lock()
            .unwrap()
            .push(format!("disconnect {source} {destination}"));
        Some(slot)
    }

    fn resynthesize_to_wavetable(&self, oscillator: usize) {
        self.wavetable_edits
            .lock()
            .unwrap()
            .push(format!("resynthesize {oscillator}"));
    }
}

/// The host: logs gestures and sets values.
struct Recorder {
    synth: Arc<Synth>,
    log: Mutex<Vec<String>>,
    /// Sets the host hasn't applied yet. nice-plug's wrappers don't update
    /// a parameter at once (CLAP queues an event; VST3 waits while a block
    /// is processing), so by default the sets apply at the start of the
    /// next frame, as if a block ran in between.
    pending: Mutex<Vec<(ParamPtr, f32)>>,
    deferred: bool,
}

impl Recorder {
    fn apply_pending(&self) {
        for (param, normalized) in self.pending.lock().unwrap().drain(..) {
            unsafe { param._internal_set_normalized_value(normalized) };
        }
    }
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
        if self.deferred {
            self.pending.lock().unwrap().push((param, normalized));
        } else {
            unsafe { param._internal_set_normalized_value(normalized) };
        }
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
        Fixture::with_sets(true)
    }

    /// A fixture whose host applies sets at once (`deferred` false) or at
    /// the next frame.
    fn with_sets(deferred: bool) -> Fixture {
        let synth = Synth::new();
        let recorder = Arc::new(Recorder {
            synth: synth.clone(),
            log: Mutex::new(Vec::new()),
            pending: Mutex::new(Vec::new()),
            deferred,
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
                self.recorder.apply_pending();
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

/// Each input is followed by a second frame, as if an audio block ran in
/// between: the recording host applies the GUI's sets at the start of a
/// frame, so a set shows from the next one.
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
    harness.step();
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
            fixture.recorder.apply_pending();
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
            fixture.recorder.apply_pending();
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
    fixture.recorder.apply_pending();
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
    fixture.recorder.apply_pending();
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

/// A chat that is on and idle, for the composed editor.
struct IdleChat;

impl sas_gui::ChatHost for IdleChat {
    fn run_frame(&self) {}
    fn view(&self) -> sas_gui::ChatView {
        sas_gui::ChatView {
            enabled: true,
            ..Default::default()
        }
    }
    fn send(&self, _text: String) {}
    fn stop(&self) {}
    fn undo(&self) {}
    fn compare(&self) {}
    fn confirm(&self, _accept: bool) {}
    fn settings(&self) -> sas_gui::ChatSettingsView {
        Default::default()
    }
    fn save_settings(
        &self,
        _settings: sas_gui::ChatSettingsView,
        _api_key: Option<String>,
    ) -> Result<(), String> {
        Ok(())
    }
}

fn intersects(a: egui::Rect, b: Rect) -> bool {
    let b = egui::Rect::from_min_size(
        Pos2::new(b.x as f32, b.y as f32),
        egui::vec2(b.width as f32, b.height as f32),
    );
    a.intersects(b) && a.intersect(b).area() > 0.0
}

#[test]
fn the_chat_toggle_and_panel_stay_clear_of_upstream_and_inside_the_editor() {
    let interface = Interface::default();
    for (width, height) in [
        (1400, 820),
        (700, 410),
        (350, 205),
        (1400, 700),
        (1000, 820),
    ] {
        let area = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(width as f32, height as f32));
        let placement = interface.chat_placement(area, 1.0);
        let layout = FullLayout::new(interface.skin(), width, height);
        let header = layout.header;
        for (name, part) in [
            ("tabs", header.tabs),
            ("preset selector", header.preset_selector),
            ("save", header.save_button),
            ("menu", header.menu_button),
            ("volume", header.volume),
            ("oscilloscope", header.oscilloscope),
        ] {
            assert!(
                !intersects(placement.toggle, part),
                "{width}x{height}: the toggle covers the {name}"
            );
        }
        let bounds = egui::Rect::from_min_size(
            Pos2::new(header.bounds.x as f32, header.bounds.y as f32),
            egui::vec2(header.bounds.width as f32, header.bounds.height as f32),
        );
        assert!(bounds.contains_rect(placement.toggle), "{width}x{height}");
        assert!(
            placement.editor.contains_rect(placement.panel()),
            "{width}x{height}"
        );
        assert!(placement.panel().min.y >= bounds.max.y, "{width}x{height}");
    }
}

#[test]
fn the_open_chat_panel_composes_with_the_editor() {
    let fixture = Fixture::new();
    let panel = RefCell::new(sas_gui::ChatPanel::default());
    let mut harness = Harness::builder()
        .with_size((1400.0, 820.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .build_ui(|ui| {
            // As `App::ui` composes them.
            let screen = ui.ctx().content_rect();
            fixture.recorder.apply_pending();
            let host = Host::tracking(
                fixture.synth.as_ref(),
                Some(&fixture.context),
                &fixture.open,
            );
            fixture.interface.borrow_mut().show_in(ui, screen, &host);
            let placement = fixture
                .interface
                .borrow()
                .chat_placement(screen, ui.ctx().pixels_per_point());
            panel
                .borrow_mut()
                .show(ui.ctx(), &placement, &IdleChat, &mut || {});
        });
    harness.step();
    let layout = FullLayout::new(fixture.interface.borrow().skin(), 1400, 820);
    let logo = centre(layout.header.logo);
    let tabs = layout.header.tabs;
    let placement = fixture.interface.borrow().chat_placement(
        egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 820.0)),
        1.0,
    );

    // The toggle opens the panel.
    click(&mut harness, logo);
    harness.step();
    assert!(panel.borrow().is_open());

    // The tabs still switch.
    let effects = Pos2::new(
        tabs.x as f32 + tabs.width as f32 * 1.5 / 4.0,
        tabs.y as f32 + 20.0,
    );
    click(&mut harness, effects);
    assert_eq!(fixture.interface.borrow().tab(), sas_gui::Tab::Effects);
    let voice = Pos2::new(
        tabs.x as f32 + tabs.width as f32 * 0.5 / 4.0,
        tabs.y as f32 + 20.0,
    );
    click(&mut harness, voice);
    assert_eq!(fixture.interface.borrow().tab(), sas_gui::Tab::Voice);
    fixture.take_log();

    // The portamento time knob lies under the open panel: a drag there
    // reaches the panel, not the knob.
    let (_, portamento) = fixture.layouts();
    let knob = centre(portamento.portamento_time);
    assert!(placement.panel().contains(knob));
    let before = fixture.synth.value("portamento_time");
    drag(
        &mut harness,
        knob,
        knob + egui::vec2(0.0, -40.0),
        Modifiers::NONE,
    );
    assert!(fixture.take_log().is_empty());
    assert_eq!(fixture.synth.value("portamento_time"), before);

    // Closed, the same drag reaches the knob.
    click(&mut harness, logo);
    harness.step();
    assert!(!panel.borrow().is_open());
    drag(
        &mut harness,
        knob,
        knob + egui::vec2(0.0, -40.0),
        Modifiers::NONE,
    );
    assert!(fixture
        .take_log()
        .contains(&"begin portamento_time".to_string()));
}

/// With the drawer docked in a window widened for it, upstream's layout is
/// where it is in a window without it: its controls answer at the same
/// positions, and a drag in the drawer reaches none of them.
#[test]
fn a_docked_drawer_leaves_upstream_its_layout_and_input() {
    let fixture = Fixture::new();
    fixture.interface.borrow_mut().set_drawer_open(true);
    let panel = RefCell::new(sas_gui::ChatPanel::default());
    let mut harness = Harness::builder()
        .with_size((1760.0, 820.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .build_ui(|ui| {
            let screen = ui.ctx().content_rect();
            fixture.recorder.apply_pending();
            let host = Host::tracking(
                fixture.synth.as_ref(),
                Some(&fixture.context),
                &fixture.open,
            );
            fixture.interface.borrow_mut().show_in(ui, screen, &host);
            sas_gui::show_chat(
                &mut fixture.interface.borrow_mut(),
                &mut panel.borrow_mut(),
                ui.ctx(),
                screen,
                &IdleChat,
                &mut || {},
            );
        });
    harness.step();
    let placement = fixture.interface.borrow().chat_placement(
        egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1760.0, 820.0)),
        1.0,
    );
    assert_eq!(
        placement.editor,
        egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 820.0))
    );
    assert_eq!(
        placement.mode,
        sas_gui::ChatMode::Docked(egui::Rect::from_min_max(
            Pos2::new(1400.0, 0.0),
            Pos2::new(1760.0, 820.0)
        ))
    );
    assert!(panel.borrow().is_open());
    // The panel fills the drawer, entirely beside upstream's layout, and
    // draws there (its title, Undo and the message field).
    assert_eq!(
        placement.panel(),
        egui::Rect::from_min_max(Pos2::new(1400.0, 0.0), Pos2::new(1760.0, 820.0))
    );
    assert!(!placement.panel().intersects(placement.editor.shrink(0.5)));
    for label in ["Sound design chat", "Undo", "Message"] {
        let node = harness.get_by_label(label);
        let bounds = node.rect();
        assert!(bounds.min.x >= 1400.0, "{label} at {bounds:?}");
    }
    fixture.take_log();

    let inside = Pos2::new(1580.0, 600.0);
    drag(
        &mut harness,
        inside,
        inside + egui::vec2(0.0, -40.0),
        Modifiers::NONE,
    );
    assert!(fixture.take_log().is_empty());

    let knob = centre(oscillator_layout(&fixture, 0).distortion_amount);
    drag(
        &mut harness,
        knob,
        knob + egui::vec2(0.0, -40.0),
        Modifiers::NONE,
    );
    let log = fixture.take_log();
    assert!(
        log.contains(&"begin osc_1_distortion_amount".to_string()),
        "{log:?}"
    );
}

fn oscillator_layout(fixture: &Fixture, index: usize) -> OscillatorLayout {
    let interface = fixture.interface.borrow();
    let layout = FullLayout::new(interface.skin(), 1400, 820);
    let top = Scope::top(interface.skin(), layout.size_ratio);
    let synthesis = SynthesisLayout::new(&top, layout.main);
    OscillatorLayout::new(
        &top.within(oscillator::SKIN),
        synthesis.oscillators[index],
        false,
    )
}

fn sample_layout(fixture: &Fixture) -> SampleLayout {
    let interface = fixture.interface.borrow();
    let layout = FullLayout::new(interface.skin(), 1400, 820);
    let top = Scope::top(interface.skin(), layout.size_ratio);
    let synthesis = SynthesisLayout::new(&top, layout.main);
    SampleLayout::new(&top.within(sample::SKIN), synthesis.sample)
}

/// Clicks row `row` (0-based, dividers count) of a menu that opened below
/// `selector`.
fn choose_row(harness: &mut Harness, selector: Rect, row: usize) {
    // Rows are 24 pixels from the top of the list, under the body rounding.
    let pos = Pos2::new(
        selector.x as f32 + 20.0,
        selector.bottom() as f32 + 12.0 + 24.0 * row as f32,
    );
    harness.event(Event::PointerMoved(pos));
    harness.step();
    click(harness, pos);
}

#[test]
fn an_oscillator_destination_is_chosen_from_its_menu_or_arrows() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let osc = oscillator_layout(&fixture, 0);
    assert_eq!(fixture.synth.value("osc_1_destination"), 0.0);
    // The selector opens on mouse down, below itself, and choosing sets the
    // value in one gesture.
    press(
        &mut harness,
        centre(osc.destination),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        centre(osc.destination),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    {
        let interface = fixture.interface.borrow();
        let menu = interface.overlay().menu.as_ref().unwrap();
        let names: Vec<_> = menu.items.iter().map(|item| item.name.as_str()).collect();
        assert_eq!(names, oscillator::DESTINATION_MENU_NAMES);
        assert_eq!(
            menu.position,
            Pos2::new(osc.destination.x as f32, osc.destination.bottom() as f32)
        );
    }
    fixture.take_log();
    choose_row(&mut harness, osc.destination, 3);
    assert_eq!(fixture.synth.value("osc_1_destination"), 3.0);
    assert_eq!(
        fixture.take_log(),
        [
            "begin osc_1_destination",
            "set osc_1_destination",
            "end osc_1_destination"
        ]
    );

    // The arrows step through the destinations and wrap round.
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, centre(osc.next_destination));
    assert_eq!(fixture.synth.value("osc_1_destination"), 4.0);
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, centre(osc.next_destination));
    assert_eq!(fixture.synth.value("osc_1_destination"), 0.0);
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, centre(osc.prev_destination));
    assert_eq!(fixture.synth.value("osc_1_destination"), 4.0);
}

#[test]
fn the_wave_morph_menu_has_dividers_and_names_the_modulator() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let osc = oscillator_layout(&fixture, 1);
    press(
        &mut harness,
        centre(osc.distortion_selector),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        centre(osc.distortion_selector),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let interface = fixture.interface.borrow();
    let menu = interface.overlay().menu.as_ref().unwrap();
    let names: Vec<_> = menu
        .items
        .iter()
        .map(|item| (item.id, item.name.as_str()))
        .collect();
    assert_eq!(names[0], (0, "None"));
    assert_eq!(names[1], (-1, ""));
    assert_eq!(names[2], (1, "Sync"));
    assert_eq!(names[8], (-1, ""));
    // Oscillator 2's first modulator is oscillator 1, its second is 3.
    assert_eq!(names[9], (7, "FM <- Osc 1"));
    assert_eq!(names[10], (8, "FM <- Osc 3"));
    assert_eq!(names[13], (11, "RM <- Osc 3"));
}

#[test]
fn an_fm_loop_between_oscillators_is_broken() {
    for deferred in [true, false] {
        fm_loop_from_the_arrows(deferred);
    }
}

fn fm_loop_from_the_arrows(deferred: bool) {
    let fixture = Fixture::with_sets(deferred);
    let mut harness = fixture.harness();
    // Oscillator 1 reads oscillator 2 (FM <- Osc 2); oscillator 2 is on
    // Pulse, one before FM <- Osc 1.
    fixture.synth.set("osc_1_distortion_type", 7.0);
    fixture.synth.set("osc_2_distortion_type", 6.0);
    harness.step();
    let osc = oscillator_layout(&fixture, 1);
    click(&mut harness, centre(osc.next_distortion));
    // Oscillator 2 now reads oscillator 1, so oscillator 1, the last in the
    // loop, stops reading (SynthesisInterface::distortionTypeChanged).
    assert_eq!(fixture.synth.value("osc_2_distortion_type"), 7.0);
    assert_eq!(fixture.synth.value("osc_1_distortion_type"), 0.0);
}

#[test]
fn morph_knobs_return_to_their_type_centre_on_double_click() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let osc = oscillator_layout(&fixture, 0);
    let knob = centre(osc.distortion_amount);
    // With no wave morph the knob is bipolar and returns to the centre.
    drag(
        &mut harness,
        knob,
        knob + egui::vec2(0.0, -40.0),
        Modifiers::NONE,
    );
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, knob);
    click(&mut harness, knob);
    assert_eq!(fixture.synth.value("osc_1_distortion_amount"), 0.5);
    // Quantize is one-sided: back to the start.
    fixture.synth.set("osc_1_distortion_type", 3.0);
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, knob);
    click(&mut harness, knob);
    assert_eq!(fixture.synth.value("osc_1_distortion_amount"), 0.0);
}

#[test]
fn the_dimension_button_steps_back_through_the_views() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let osc = oscillator_layout(&fixture, 0);
    let start = fixture.synth.value("osc_1_view_2d") as i32;
    click(&mut harness, centre(osc.dimension));
    assert_eq!(fixture.synth.value("osc_1_view_2d") as i32, (start + 2) % 3);
    assert_eq!(
        fixture.take_log(),
        [
            "begin osc_1_view_2d",
            "set osc_1_view_2d",
            "end osc_1_view_2d"
        ]
    );
}

#[test]
fn the_sample_destination_wraps_round() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let smp = sample_layout(&fixture);
    assert_eq!(fixture.synth.value("sample_destination"), 3.0);
    click(&mut harness, centre(smp.prev_destination));
    assert_eq!(fixture.synth.value("sample_destination"), 2.0);
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, centre(smp.next_destination));
    click(&mut harness, centre(smp.next_destination));
    assert_eq!(fixture.synth.value("sample_destination"), 4.0);
}

#[test]
fn an_fm_loop_chosen_from_the_menu_is_broken() {
    for deferred in [true, false] {
        fm_loop_from_the_menu(deferred);
    }
}

fn fm_loop_from_the_menu(deferred: bool) {
    let fixture = Fixture::with_sets(deferred);
    let mut harness = fixture.harness();
    fixture.synth.set("osc_1_distortion_type", 7.0);
    harness.step();
    let osc = oscillator_layout(&fixture, 1);
    press(
        &mut harness,
        centre(osc.distortion_selector),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        centre(osc.distortion_selector),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    // Row 9 is "FM <- Osc 1" (rows 1 and 8 are dividers). The host applies
    // the choice a frame later; the follow-up must not read it back.
    choose_row(&mut harness, osc.distortion_selector, 9);
    assert_eq!(fixture.synth.value("osc_2_distortion_type"), 7.0);
    assert_eq!(fixture.synth.value("osc_1_distortion_type"), 0.0);
}

fn modulation_layout(fixture: &Fixture) -> ModulationLayout {
    let interface = fixture.interface.borrow();
    let layout = FullLayout::new(interface.skin(), 1400, 820);
    let top = Scope::top(interface.skin(), layout.size_ratio);
    ModulationLayout::new(&top, layout.modulation)
}

fn section_scope<'a>(
    skin: &'a sas_gui::skin::Skin,
    chain: &'a [sas_gui::skin::Section],
) -> Scope<'a> {
    Scope::top(skin, 1.0).within(chain)
}

#[test]
fn a_modulation_tab_shows_its_source() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let m = modulation_layout(&fixture);
    // ENV 2's tab, the second of three in the column.
    let tab = Pos2::new(centre(m.envelope_tabs).x, m.envelope_tabs.y as f32 + 100.0);
    click(&mut harness, tab);
    let attack = {
        let interface = fixture.interface.borrow();
        EnvelopeLayout::new(&section_scope(interface.skin(), envelope::SKIN), m.envelope).knobs[1]
    };
    drag(
        &mut harness,
        centre(attack),
        centre(attack) + egui::vec2(0.0, -20.0),
        Modifiers::NONE,
    );
    assert!(
        fixture.synth.value("env_2_attack") > registry().get("env_2_attack").unwrap().default_value
    );
    assert!(fixture
        .take_log()
        .iter()
        .all(|entry| entry.ends_with("env_2_attack")));
}

#[test]
fn the_lfo_smooth_mode_swaps_fade_for_smooth() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let m = modulation_layout(&fixture);
    let lfo_layout = {
        let interface = fixture.interface.borrow();
        LfoLayout::new(&section_scope(interface.skin(), lfo::SKIN), m.lfo)
    };
    let start = fixture.synth.value("lfo_1_smooth_mode");
    press(
        &mut harness,
        centre(lfo_layout.smooth_mode),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        centre(lfo_layout.smooth_mode),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    {
        let interface = fixture.interface.borrow();
        let names: Vec<_> = interface
            .overlay()
            .menu
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|i| i.name.clone())
            .collect();
        assert_eq!(names, lfo::SMOOTH_MODE_NAMES);
    }
    let other = 1 - start as usize;
    choose_row(&mut harness, lfo_layout.smooth_mode, other);
    assert_eq!(fixture.synth.value("lfo_1_smooth_mode"), other as f32);
}

#[test]
fn a_free_running_lfo_shows_its_frequency() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    fixture.synth.set("lfo_1_sync", 0.0);
    harness.step();
    let m = modulation_layout(&fixture);
    let rate = {
        let interface = fixture.interface.borrow();
        LfoLayout::new(&section_scope(interface.skin(), lfo::SKIN), m.lfo).rate
    };
    let before = fixture.synth.value("lfo_1_frequency");
    drag(
        &mut harness,
        centre(rate),
        centre(rate) + egui::vec2(0.0, -30.0),
        Modifiers::NONE,
    );
    assert!(fixture.synth.value("lfo_1_frequency") > before);
}

#[test]
fn the_random_stereo_switch_toggles() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let m = modulation_layout(&fixture);
    let stereo = {
        let interface = fixture.interface.borrow();
        RandomLayout::new(&section_scope(interface.skin(), random::SKIN), m.random).stereo
    };
    let before = fixture.synth.value("random_1_stereo");
    click(&mut harness, centre(stereo));
    assert_eq!(fixture.synth.value("random_1_stereo"), 1.0 - before);
    assert_eq!(
        fixture.take_log(),
        [
            "begin random_1_stereo",
            "set random_1_stereo",
            "end random_1_stereo"
        ]
    );
}

fn filter_layout(fixture: &Fixture, index: usize) -> FilterLayout {
    let interface = fixture.interface.borrow();
    let layout = FullLayout::new(interface.skin(), 1400, 820);
    let top = Scope::top(interface.skin(), layout.size_ratio);
    let synthesis = SynthesisLayout::new(&top, layout.main);
    let bounds = [synthesis.filter_1, synthesis.filter_2][index - 1];
    FilterLayout::new(&top.within(filter::SKIN), bounds, Kind::Voice(index))
}

#[test]
fn a_filter_input_routes_its_oscillator() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let f = filter_layout(&fixture, 1);
    // Oscillator 2 starts on filter 2; adding filter 1 makes it both.
    assert_eq!(fixture.synth.value("osc_2_destination"), 1.0);
    click(&mut harness, centre(f.inputs.unwrap()[1]));
    assert_eq!(fixture.synth.value("osc_2_destination"), 2.0);
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, centre(f.inputs.unwrap()[1]));
    assert_eq!(fixture.synth.value("osc_2_destination"), 1.0);
}

#[test]
fn one_filter_feeds_the_other_at_a_time() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    fixture.synth.set("filter_2_filter_input", 1.0);
    harness.step();
    let f = filter_layout(&fixture, 1);
    click(&mut harness, centre(f.inputs.unwrap()[4]));
    assert_eq!(fixture.synth.value("filter_1_filter_input"), 1.0);
    assert_eq!(fixture.synth.value("filter_2_filter_input"), 0.0);
}

#[test]
fn filter_arrows_step_through_models_and_styles() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let f = filter_layout(&fixture, 1);
    // Analog 12dB, one back: the last style of the last model.
    click(&mut harness, centre(f.preset_selector.previous));
    assert_eq!(fixture.synth.value("filter_1_model"), 7.0);
    assert_eq!(fixture.synth.value("filter_1_style"), 1.0);
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, centre(f.preset_selector.next));
    assert_eq!(fixture.synth.value("filter_1_model"), 0.0);
    assert_eq!(fixture.synth.value("filter_1_style"), 0.0);
}

#[test]
fn the_filter_menu_picks_a_model_then_a_style() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let f = filter_layout(&fixture, 1);
    press(
        &mut harness,
        centre(f.preset_selector.text),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        centre(f.preset_selector.text),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let (left, right) = {
        let interface = fixture.interface.borrow();
        let menu = interface
            .overlay()
            .dual_menu
            .as_ref()
            .expect("the model list");
        assert_eq!(menu.groups.len(), 8);
        assert_eq!(menu.groups[6].items.len(), 6);
        let top = Scope::top(interface.skin(), 1.0);
        let scope = top.within(&[sas_gui::skin::Section::PopupBrowser]);
        let bounds = menu.bounds(&scope, Rect::new(0, 0, 1400, 820));
        menu.lists(&scope, bounds)
    };
    let row = |list: Rect, row: usize| {
        Pos2::new(
            list.x as f32 + 20.0,
            list.y as f32 + 12.0 + 24.0 * row as f32,
        )
    };
    // Comb, the seventh model: its first style.
    harness.event(Event::PointerMoved(row(left, 6)));
    harness.step();
    click(&mut harness, row(left, 6));
    assert_eq!(fixture.synth.value("filter_1_model"), 6.0);
    assert_eq!(fixture.synth.value("filter_1_style"), 0.0);
    // Then Low High Flange-; the list stays open.
    harness.event(Event::PointerMoved(row(right, 2)));
    harness.step();
    click(&mut harness, row(right, 2));
    assert_eq!(fixture.synth.value("filter_1_style"), 2.0);
    assert!(fixture.interface.borrow().overlay().dual_menu.is_some());
}

#[test]
fn dragging_the_response_moves_cutoff_and_resonance() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let f = filter_layout(&fixture, 1);
    let cutoff = fixture.synth.value("filter_1_cutoff");
    let resonance = fixture.synth.value("filter_1_resonance");
    fixture.take_log();
    let start = centre(f.response);
    drag(
        &mut harness,
        start,
        start + egui::vec2(30.0, -20.0),
        Modifiers::NONE,
    );
    assert!(fixture.synth.value("filter_1_cutoff") > cutoff);
    assert!(fixture.synth.value("filter_1_resonance") > resonance);
    let log = fixture.take_log();
    assert_eq!(
        log.first().map(String::as_str),
        Some("begin filter_1_cutoff")
    );
    assert_eq!(
        log.last().map(String::as_str),
        Some("end filter_1_resonance")
    );
}

/// A release over FIL2 after a press elsewhere isn't a click
/// (`buttonClicked` needs both on the button).
#[test]
fn a_release_over_the_filter_input_after_a_press_elsewhere_changes_nothing() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    fixture.synth.set("filter_2_filter_input", 1.0);
    harness.step();
    let f = filter_layout(&fixture, 1);
    drag(
        &mut harness,
        centre(f.title),
        centre(f.inputs.unwrap()[4]),
        Modifiers::NONE,
    );
    assert_eq!(fixture.synth.value("filter_1_filter_input"), 0.0);
    assert_eq!(fixture.synth.value("filter_2_filter_input"), 1.0);
}

const ENV_1: EnvelopeNames = EnvelopeNames {
    delay: "env_1_delay",
    attack: "env_1_attack",
    hold: "env_1_hold",
    decay: "env_1_decay",
    sustain: "env_1_sustain",
    release: "env_1_release",
    attack_power: "env_1_attack_power",
    decay_power: "env_1_decay_power",
    release_power: "env_1_release_power",
};

/// The first envelope's editor and its shape at the synth's values.
fn envelope_shape(fixture: &Fixture) -> (Rect, Shape) {
    let editor = {
        let m = modulation_layout(fixture);
        let interface = fixture.interface.borrow();
        EnvelopeLayout::new(&section_scope(interface.skin(), envelope::SKIN), m.envelope).editor
    };
    let s = &fixture.synth;
    let values = EnvelopeValues {
        delay: s.value("env_1_delay"),
        attack: s.value("env_1_attack"),
        hold: s.value("env_1_hold"),
        decay: s.value("env_1_decay"),
        sustain: s.value("env_1_sustain"),
        release: s.value("env_1_release"),
        attack_power: s.value("env_1_attack_power"),
        decay_power: s.value("env_1_decay_power"),
        release_power: s.value("env_1_release_power"),
    };
    let shape = Shape::new(
        &ENV_1,
        values,
        editor.width as f32,
        editor.height as f32,
        4.0,
    );
    (editor, shape)
}

fn editor_point(editor: Rect, shape: &Shape, x: f32, y: f32) -> Pos2 {
    Pos2::new(
        editor.x as f32 + shape.pad_x(x),
        editor.y as f32 + shape.pad_y(y),
    )
}

#[test]
fn dragging_the_attack_point_lengthens_the_attack() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (editor, shape) = envelope_shape(&fixture);
    let attack = fixture.synth.value("env_1_attack");
    let start = editor_point(editor, &shape, shape.attack_x, 0.0);
    harness.event(Event::PointerMoved(start));
    harness.step();
    fixture.take_log();
    drag(
        &mut harness,
        start,
        start + egui::vec2(60.0, 0.0),
        Modifiers::NONE,
    );
    assert!(fixture.synth.value("env_1_attack") > attack);
    let log = fixture.take_log();
    assert_eq!(log.first().map(String::as_str), Some("begin env_1_attack"));
    assert_eq!(log.last().map(String::as_str), Some("end env_1_attack"));
    assert_eq!(
        fixture.synth.value("env_1_decay"),
        registry().get("env_1_decay").unwrap().default_value
    );
}

#[test]
fn dragging_the_sustain_point_sets_decay_and_sustain() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    fixture.synth.set("env_1_sustain", 0.5);
    harness.step();
    let (editor, shape) = envelope_shape(&fixture);
    let decay = fixture.synth.value("env_1_decay");
    let start = editor_point(editor, &shape, shape.decay_x, shape.sustain_y);
    harness.event(Event::PointerMoved(start));
    harness.step();
    drag(
        &mut harness,
        start,
        start + egui::vec2(30.0, 20.0),
        Modifiers::NONE,
    );
    assert!(fixture.synth.value("env_1_decay") > decay);
    assert!(fixture.synth.value("env_1_sustain") < 0.5);
}

#[test]
fn a_double_click_straightens_a_power() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    fixture.synth.set("env_1_attack", 1.0);
    fixture.synth.set("env_1_attack_power", 3.0);
    harness.step();
    let (editor, shape) = envelope_shape(&fixture);
    let dot = editor_point(
        editor,
        &shape,
        (shape.delay_x + shape.attack_x) / 2.0,
        shape.attack_value(0.5),
    );
    harness.event(Event::PointerMoved(dot));
    harness.step();
    fixture.take_log();
    click(&mut harness, dot);
    click(&mut harness, dot);
    assert_eq!(fixture.synth.value("env_1_attack_power"), 0.0);
    // One gesture, from the double click; the clicks themselves open none.
    assert_eq!(
        fixture.take_log(),
        [
            "begin env_1_attack_power",
            "set env_1_attack_power",
            "end env_1_attack_power"
        ]
    );
}

/// A click that doesn't move sends no gesture.
#[test]
fn an_envelope_click_without_moving_sends_nothing() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (editor, shape) = envelope_shape(&fixture);
    let attack = editor_point(editor, &shape, shape.attack_x, 0.0);
    harness.event(Event::PointerMoved(attack));
    harness.step();
    fixture.take_log();
    click(&mut harness, attack);
    assert!(fixture.take_log().is_empty());
}

fn lfo_layout(fixture: &Fixture) -> LfoLayout {
    let m = modulation_layout(fixture);
    let interface = fixture.interface.borrow();
    LfoLayout::new(&section_scope(interface.skin(), lfo::SKIN), m.lfo)
}

/// LFO 1's shape as the editor last set it.
fn lfo_1_shape(fixture: &Fixture) -> LineGenerator {
    fixture.synth.lfo_shape(0).expect("the editor set a shape")
}

/// A point of the editor at fractions of its width and height (the y
/// padded as the editor draws it).
fn lfo_point(editor: Rect, x: f32, y: f32) -> Pos2 {
    let pad = 6.0;
    let height = editor.height as f32;
    Pos2::new(
        editor.x as f32 + x * editor.width as f32,
        editor.y as f32 + y * height * (height - 2.0 * pad) / height + pad,
    )
}

#[test]
fn dragging_an_lfo_point_moves_it() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let editor = lfo_layout(&fixture).editor;
    // The init triangle's peak, halfway across at the top.
    let peak = lfo_point(editor, 0.5, 0.0);
    harness.event(Event::PointerMoved(peak));
    harness.step();
    drag(
        &mut harness,
        peak,
        peak + egui::vec2(0.0, 40.0),
        Modifiers::NONE,
    );
    let line = lfo_1_shape(&fixture);
    assert_eq!(line.num_points(), 3);
    assert!(line.point(1).1 > 0.1, "{:?}", line.point(1));
    assert!((line.point(1).0 - 0.5).abs() < 0.001);
}

#[test]
fn a_double_click_adds_an_lfo_point() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let editor = lfo_layout(&fixture).editor;
    let empty = lfo_point(editor, 0.25, 0.9);
    harness.event(Event::PointerMoved(empty));
    harness.step();
    click(&mut harness, empty);
    click(&mut harness, empty);
    let line = lfo_1_shape(&fixture);
    assert_eq!(line.num_points(), 4);
}

#[test]
fn painting_draws_the_pattern_into_a_grid_cell() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let layout = lfo_layout(&fixture);
    click(&mut harness, centre(layout.paint));
    for _ in 0..3 {
        harness.step();
    }
    // The first of eight cells, with the default pattern (down).
    let cell = lfo_point(layout.editor, 1.0 / 16.0, 0.5);
    harness.event(Event::PointerMoved(cell));
    harness.step();
    click(&mut harness, cell);
    let line = lfo_1_shape(&fixture);
    assert!(line.num_points() > 3);
    assert_eq!(line.point(1).0, 0.0);
    assert!((line.point(2).0 - 0.125).abs() < 0.001);
}

#[test]
fn the_lfo_menu_flips_the_shape() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let editor = lfo_layout(&fixture).editor;
    let at = lfo_point(editor, 0.7, 0.9);
    harness.event(Event::PointerMoved(at));
    harness.step();
    press(&mut harness, at, PointerButton::Secondary, Modifiers::NONE);
    release(&mut harness, at, PointerButton::Secondary, Modifiers::NONE);
    let (items, position) = {
        let interface = fixture.interface.borrow();
        let menu = interface.overlay().menu.as_ref().expect("the LFO menu");
        (menu.items.clone(), menu.position)
    };
    let names: Vec<&str> = items.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(names, ["Copy", "Flip Horizontal", "Flip Vertical"]);
    let row = Pos2::new(position.x + 20.0, position.y + 12.0 + 24.0 * 2.0);
    harness.event(Event::PointerMoved(row));
    harness.step();
    click(&mut harness, row);
    for _ in 0..2 {
        harness.step();
    }
    let line = lfo_1_shape(&fixture);
    assert_eq!(line.point(1), (0.5, 1.0));
}

/// `snapToValue(true, 0)` on the LFO stereo knob: a drag that stays within
/// 5% of the range of 0 lands on 0, except in fine (Command) mode.
#[test]
fn the_lfo_stereo_knob_snaps_to_centre() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let knob = centre(lfo_layout(&fixture).stereo);
    assert_eq!(fixture.synth.value("lfo_1_stereo"), 0.0);
    drag(
        &mut harness,
        knob,
        knob + egui::vec2(0.0, -3.0),
        Modifiers::NONE,
    );
    assert_eq!(fixture.synth.value("lfo_1_stereo"), 0.0);
    for _ in 0..30 {
        harness.step();
    }
    drag(
        &mut harness,
        knob,
        knob + egui::vec2(0.0, -3.0),
        Modifiers::COMMAND,
    );
    let fine = fixture.synth.value("lfo_1_stereo");
    assert!(fine > 0.0 && fine < 0.1, "{fine}");
}

const WAVETABLE: Rect = Rect {
    x: 40,
    y: 40,
    width: 200,
    height: 100,
};
const FRAME_SLIDER: Rect = Rect {
    x: 250,
    y: 40,
    width: 20,
    height: 100,
};

/// Oscillator 1's wavetable view and its frame slider on their own.
fn wavetable_harness<'a>(
    fixture: &'a Fixture,
    overlay: &'a RefCell<Overlay>,
    view: &'a RefCell<Wavetable3d>,
) -> Harness<'a> {
    let mut harness = Harness::builder()
        .with_size((400.0, 300.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .build_ui(move |ui| {
            let area = ui.ctx().content_rect();
            fixture.recorder.apply_pending();
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
            let frame = Slider::new("osc_1_wave_frame", SliderStyle::Vertical);
            view.borrow_mut().show(
                &mut panel,
                WAVETABLE,
                (&frame, FRAME_SLIDER),
                None,
                None,
                false,
            );
            panel.finish();
            overlay.show_menu(ui, &host, &pixels, &top, Rect::new(0, 0, 400, 300));
        });
    harness.step();
    harness
}

#[test]
fn dragging_the_wavetable_moves_the_frame_in_one_gesture() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    let start = centre(WAVETABLE);
    press(&mut harness, start, PointerButton::Primary, Modifiers::NONE);
    // The frame slider's popup shows at the slider, from the press.
    {
        let overlay = overlay.borrow();
        let (name, popup) = overlay.value_popup.as_ref().unwrap();
        assert_eq!(*name, "osc_1_wave_frame");
        assert_eq!(popup.source, FRAME_SLIDER);
    }
    // The view's height spans the 256 frames: 20 pixels up is 51.2.
    move_to(&mut harness, start + egui::vec2(0.0, -10.0));
    move_to(&mut harness, start + egui::vec2(0.0, -20.0));
    release(
        &mut harness,
        start + egui::vec2(0.0, -20.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    harness.step();
    assert!((fixture.synth.value("osc_1_wave_frame") - 51.2).abs() < 1e-3);
    assert_eq!(
        fixture.take_log(),
        [
            "begin osc_1_wave_frame",
            "set osc_1_wave_frame",
            "end osc_1_wave_frame"
        ]
    );
}

#[test]
fn a_wavetable_drag_holds_its_own_value_and_clamps() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    let start = Pos2::new(centre(WAVETABLE).x, WAVETABLE.bottom() as f32 - 5.0);
    press(&mut harness, start, PointerButton::Primary, Modifiers::NONE);
    // Past the top: clamped to the last frame, steps the host hasn't
    // applied yet included.
    for step in 1..=4 {
        move_to(&mut harness, start + egui::vec2(0.0, -40.0 * step as f32));
    }
    // Back down 25 pixels from the clamp: 64 frames below the last.
    move_to(&mut harness, start + egui::vec2(0.0, -135.0));
    release(
        &mut harness,
        start + egui::vec2(0.0, -135.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    harness.step();
    assert!((fixture.synth.value("osc_1_wave_frame") - 192.0).abs() < 1e-3);
}

#[test]
fn leaving_the_wavetable_hides_the_frame_popup() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    let start = centre(WAVETABLE);
    click(&mut harness, start);
    assert!(overlay.borrow().value_popup.is_some());
    // A click that doesn't move edits nothing.
    assert!(fixture.take_log().is_empty());
    move_to(&mut harness, Pos2::new(350.0, 250.0));
    harness.step();
    assert!(overlay.borrow().value_popup.is_none());
}

#[test]
fn a_macro_knob_drags_its_control() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let knob = {
        let interface = fixture.interface.borrow();
        let layout = FullLayout::new(interface.skin(), 1400, 820);
        let top = Scope::top(interface.skin(), layout.size_ratio);
        ExtraModLayout::new(&top, layout.extra_mod).macros[0].knob
    };
    let before = fixture.synth.value("macro_control_1");
    fixture.take_log();
    drag(
        &mut harness,
        centre(knob),
        centre(knob) + egui::vec2(0.0, -30.0),
        Modifiers::NONE,
    );
    assert!(fixture.synth.value("macro_control_1") > before);
    let log = fixture.take_log();
    assert_eq!(
        log.first().map(String::as_str),
        Some("begin macro_control_1")
    );
    assert_eq!(log.last().map(String::as_str), Some("end macro_control_1"));
}

const PASTED: &str = r#"{"version":"1.0.0","name":"Pasted","groups":[]}"#;

fn empty_clipboard() -> Option<String> {
    None
}

fn clipboard_with_wavetable() -> Option<String> {
    Some(PASTED.to_string())
}

fn clipboard_with_text() -> Option<String> {
    Some("just words".to_string())
}

/// Opens the view's right-click menu and returns its item names.
fn open_wavetable_menu(harness: &mut Harness, overlay: &RefCell<Overlay>) -> Vec<String> {
    let at = centre(WAVETABLE);
    press(harness, at, PointerButton::Secondary, Modifiers::NONE);
    release(harness, at, PointerButton::Secondary, Modifiers::NONE);
    let overlay = overlay.borrow();
    let menu = overlay.menu.as_ref().unwrap();
    assert!(matches!(menu.owner, MenuOwner::Wavetable(0)));
    assert_eq!(menu.position, at);
    menu.items.iter().map(|item| item.name.clone()).collect()
}

/// Chooses menu row `row` (rows are 24 pixels from the top of the list).
fn choose_wavetable_row(harness: &mut Harness, row: usize) {
    let at = centre(WAVETABLE) + egui::vec2(20.0, 12.0 + 24.0 * row as f32);
    harness.event(Event::PointerMoved(at));
    harness.step();
    click(harness, at);
    harness.step();
}

#[test]
fn the_wavetable_menu_offers_paste_only_for_wavetable_json() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    view.borrow_mut().set_clipboard_reader(empty_clipboard);
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    assert_eq!(
        open_wavetable_menu(&mut harness, &overlay),
        ["Copy", "", "Initialize", "Resynthesize Preset to Wavetable"]
    );
    // A right-click is no drag: nothing reaches the host.
    assert!(fixture.take_log().is_empty());
    overlay.borrow_mut().menu = None;

    view.borrow_mut().set_clipboard_reader(clipboard_with_text);
    harness.step();
    assert_eq!(
        open_wavetable_menu(&mut harness, &overlay),
        ["Copy", "", "Initialize", "Resynthesize Preset to Wavetable"]
    );
    overlay.borrow_mut().menu = None;

    view.borrow_mut()
        .set_clipboard_reader(clipboard_with_wavetable);
    harness.step();
    assert_eq!(
        open_wavetable_menu(&mut harness, &overlay),
        [
            "Copy",
            "Paste",
            "",
            "Initialize",
            "Resynthesize Preset to Wavetable"
        ]
    );
    assert!(wavetable_3d::is_wavetable_json(PASTED));
}

#[test]
fn pasting_and_initializing_reach_the_model() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    view.borrow_mut()
        .set_clipboard_reader(clipboard_with_wavetable);
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    open_wavetable_menu(&mut harness, &overlay);
    choose_wavetable_row(&mut harness, 1);
    for _ in 0..30 {
        harness.step();
    }
    open_wavetable_menu(&mut harness, &overlay);
    choose_wavetable_row(&mut harness, 3);
    assert_eq!(
        *fixture.synth.wavetable_edits.lock().unwrap(),
        [format!("load 0 {PASTED}"), "init 0".to_string()]
    );
}

#[test]
fn copying_puts_the_wavetable_json_on_the_clipboard() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    view.borrow_mut().set_clipboard_reader(empty_clipboard);
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    open_wavetable_menu(&mut harness, &overlay);
    fixture.take_log();
    choose_wavetable_row(&mut harness, 0);
    assert_eq!(*fixture.synth.wavetable_edits.lock().unwrap(), ["json 0"]);
    // The press on the menu didn't reach the view under it.
    assert!(fixture.take_log().is_empty());
}

#[test]
fn save_appears_once_the_editor_can_save() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    view.borrow_mut().set_clipboard_reader(empty_clipboard);
    view.borrow_mut().save_available = true;
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    assert_eq!(
        open_wavetable_menu(&mut harness, &overlay),
        [
            "Save to Wavetables",
            "Copy",
            "",
            "Initialize",
            "Resynthesize Preset to Wavetable"
        ]
    );
    choose_wavetable_row(&mut harness, 0);
    assert!(view.borrow_mut().take_save_request());
    assert!(!view.borrow_mut().take_save_request());
}

/// A double click whose second press drags: the drag bends the power in
/// its gesture, and the straightening follows at the release, in its own
/// (`mouseDoubleClick` after `mouseUp`).
#[test]
fn a_double_click_straightens_after_the_second_presss_drag() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    fixture.synth.set("env_1_attack", 1.0);
    fixture.synth.set("env_1_attack_power", 3.0);
    harness.step();
    let (editor, shape) = envelope_shape(&fixture);
    let dot = editor_point(
        editor,
        &shape,
        (shape.delay_x + shape.attack_x) / 2.0,
        shape.attack_value(0.5),
    );
    harness.event(Event::PointerMoved(dot));
    harness.step();
    click(&mut harness, dot);
    fixture.take_log();
    drag(
        &mut harness,
        dot,
        dot + egui::vec2(0.0, -10.0),
        Modifiers::NONE,
    );
    assert_eq!(fixture.synth.value("env_1_attack_power"), 0.0);
    let log = fixture.take_log();
    let ends: Vec<usize> = log
        .iter()
        .enumerate()
        .filter(|(_, entry)| *entry == "end env_1_attack_power")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(ends.len(), 2, "{log:?}");
    assert_eq!(log[ends[0] + 1], "begin env_1_attack_power", "{log:?}");
}

/// Dragging the magnifying glass up zooms in: the same drag of the attack
/// point then sets a shorter attack.
#[test]
fn the_magnifying_glass_zooms_the_envelope() {
    let attack_after = |zoom: bool| {
        let fixture = Fixture::new();
        let mut harness = fixture.harness();
        if zoom {
            let glass = {
                let m = modulation_layout(&fixture);
                let interface = fixture.interface.borrow();
                EnvelopeLayout::new(&section_scope(interface.skin(), envelope::SKIN), m.envelope)
                    .magnifying_glass
            };
            let at = centre(glass);
            drag(
                &mut harness,
                at,
                at + egui::vec2(0.0, -60.0),
                Modifiers::NONE,
            );
        }
        let (editor, shape) = envelope_shape(&fixture);
        let start = editor_point(editor, &shape, shape.attack_x, 0.0);
        harness.event(Event::PointerMoved(start));
        harness.step();
        drag(
            &mut harness,
            start,
            start + egui::vec2(20.0, 0.0),
            Modifiers::NONE,
        );
        fixture.synth.value("env_1_attack")
    };
    let start = registry().get("env_1_attack").unwrap().default_value;
    let plain = attack_after(false);
    let zoomed = attack_after(true);
    assert!(zoomed > start, "the zoomed drag didn't move the attack");
    assert!(zoomed < plain, "{zoomed} >= {plain}");
}

/// With the hertz setting on, a filter cutoff's popup reads in hertz
/// through the editor (`Panel::slider`'s displayed slider).
#[test]
fn a_cutoff_popup_reads_in_hertz_with_the_setting_on() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    fixture.synth.hz.store(true, Ordering::Relaxed);
    harness.step();
    let cutoff = filter_layout(&fixture, 1).cutoff;
    press(
        &mut harness,
        centre(cutoff),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    {
        let interface = fixture.interface.borrow();
        let (name, popup) = interface.overlay().value_popup.as_ref().unwrap();
        assert_eq!(*name, "filter_1_cutoff");
        assert!(popup.text.starts_with("Cutoff: "), "{}", popup.text);
        assert!(popup.text.ends_with(" Hz"), "{}", popup.text);
    }
    release(
        &mut harness,
        centre(cutoff),
        PointerButton::Primary,
        Modifiers::NONE,
    );
}

/// The sample viewer reads the sample once per content revision.
#[test]
fn the_sample_peaks_refresh_on_a_new_content_revision() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let sample = |level: f32| SampleState {
        name: "test".into(),
        sample_rate: 44100,
        left: vec![level; 512],
        right: None,
    };
    *fixture.synth.sample.lock().unwrap() = Some(sample(0.5));
    fixture.synth.revision.store(1, Ordering::Relaxed);
    for _ in 0..3 {
        harness.step();
    }
    let reads = fixture.synth.sample_reads.load(Ordering::Relaxed);
    assert!(reads >= 1);
    for _ in 0..3 {
        harness.step();
    }
    assert_eq!(fixture.synth.sample_reads.load(Ordering::Relaxed), reads);
    *fixture.synth.sample.lock().unwrap() = Some(sample(0.25));
    fixture.synth.revision.store(2, Ordering::Relaxed);
    harness.step();
    assert_eq!(
        fixture.synth.sample_reads.load(Ordering::Relaxed),
        reads + 1
    );
}

/// Where the drag-to-modulate tests start and land: the LFO 1 tab and the
/// first two macro knobs.
fn modulation_spots(fixture: &Fixture) -> (Pos2, Pos2, Pos2) {
    let interface = fixture.interface.borrow();
    let layout = FullLayout::new(interface.skin(), 1400, 820);
    let top = Scope::top(interface.skin(), layout.size_ratio);
    let modulation = ModulationLayout::new(&top, layout.modulation);
    let lfo_tabs = interface.modulation().tab_selector(1);
    let shown = lfo_tabs.num_shown(|_| false);
    let lfo_tab = lfo_tabs.button_bounds(modulation.lfo_tabs, shown)[0];
    let macros = ExtraModLayout::new(&top, layout.extra_mod).macros;
    (
        centre(lfo_tab),
        centre(macros[0].knob),
        centre(macros[1].knob),
    )
}

const SLOT_1_GESTURES: [&str; 15] = [
    "begin modulation_1_power",
    "set modulation_1_power",
    "end modulation_1_power",
    "begin modulation_1_amount",
    "set modulation_1_amount",
    "end modulation_1_amount",
    "begin modulation_1_bipolar",
    "set modulation_1_bipolar",
    "end modulation_1_bipolar",
    "begin modulation_1_stereo",
    "set modulation_1_stereo",
    "end modulation_1_stereo",
    "begin modulation_1_bypass",
    "set modulation_1_bypass",
    "end modulation_1_bypass",
];

/// Passing over one knob onto another: the first is connected while the
/// pointer is over it and disconnected as it leaves, through the engine
/// only; the host hears of the second alone, once the drag ends (D18).
#[test]
fn a_drag_over_one_knob_onto_another_writes_only_the_last_to_the_host() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (source, first, second) = modulation_spots(&fixture);
    press(
        &mut harness,
        source,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, first);
    assert_eq!(
        *fixture.synth.bus.lock().unwrap(),
        ["connect lfo_1 macro_control_1 1.00 unipolar"]
    );
    move_to(&mut harness, second);
    assert_eq!(fixture.take_log(), Vec::<String>::new());
    release(
        &mut harness,
        second,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    assert_eq!(
        *fixture.synth.bus.lock().unwrap(),
        [
            "connect lfo_1 macro_control_1 1.00 unipolar",
            "disconnect lfo_1 macro_control_1",
            "connect lfo_1 macro_control_2 1.00 unipolar",
        ]
    );
    assert_eq!(fixture.take_log(), SLOT_1_GESTURES);
    harness.step();
    assert_eq!(fixture.synth.value("modulation_1_amount"), 1.0);
    assert_eq!(fixture.synth.value("modulation_1_bipolar"), 0.0);
    assert_eq!(
        fixture.synth.modulations().as_ref(),
        [Route {
            slot: 0,
            source: "lfo_1".into(),
            destination: "macro_control_2".into(),
        }]
    );
}

/// A modifier key pressed while over a knob makes the connection bipolar:
/// its values change in the engine, and the host gets the final ones in the
/// same gestures at the release.
#[test]
fn a_modifier_mid_hover_makes_the_connection_bipolar() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (source, first, _) = modulation_spots(&fixture);
    press(
        &mut harness,
        source,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, first);
    harness.event(Event::ModifiersChanged(Modifiers::SHIFT));
    move_to(&mut harness, first + egui::vec2(1.0, 0.0));
    assert_eq!(fixture.take_log(), Vec::<String>::new());
    release(
        &mut harness,
        first,
        PointerButton::Primary,
        Modifiers::SHIFT,
    );
    assert_eq!(
        *fixture.synth.bus.lock().unwrap(),
        [
            "connect lfo_1 macro_control_1 1.00 unipolar",
            "values 0 0.25 bipolar",
        ]
    );
    assert_eq!(fixture.take_log(), SLOT_1_GESTURES);
    harness.step();
    assert_eq!(fixture.synth.value("modulation_1_amount"), 0.25);
    assert_eq!(fixture.synth.value("modulation_1_bipolar"), 1.0);
}

/// A drag that ends away from every control connects nothing and writes
/// nothing; a click on a tab still selects it.
#[test]
fn a_drag_released_off_the_controls_leaves_nothing() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (source, first, _) = modulation_spots(&fixture);
    press(
        &mut harness,
        source,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, first);
    let away = Pos2::new(700.0, 5.0);
    move_to(&mut harness, away);
    release(&mut harness, away, PointerButton::Primary, Modifiers::NONE);
    assert_eq!(
        *fixture.synth.bus.lock().unwrap(),
        [
            "connect lfo_1 macro_control_1 1.00 unipolar",
            "disconnect lfo_1 macro_control_1",
        ]
    );
    assert_eq!(fixture.take_log(), Vec::<String>::new());
    assert!(fixture.synth.modulations().is_empty());
}

/// `hasFreeConnection`: with every slot taken a drag connects nothing.
#[test]
fn no_drag_starts_without_a_free_slot() {
    let fixture = Fixture::new();
    {
        let mut routing = fixture.synth.routing.lock().unwrap();
        let destinations = sas_preset::engine::modulation_destinations();
        for destination in destinations
            .iter()
            .filter(|name| !name.starts_with("macro"))
            .take(64)
        {
            routing.add_modulation("env_1", destination).unwrap();
        }
    }
    let mut harness = fixture.harness();
    let (source, first, _) = modulation_spots(&fixture);
    drag(&mut harness, source, first, Modifiers::NONE);
    assert!(fixture.synth.bus.lock().unwrap().is_empty());
    assert_eq!(fixture.take_log(), Vec::<String>::new());
}

/// A right-click on a connected source offers to disconnect each of its
/// destinations, in the order they were connected, and all of them; the choice disconnects without telling
/// the host.
#[test]
fn a_connected_sources_menu_disconnects_it() {
    let fixture = Fixture::new();
    {
        let mut routing = fixture.synth.routing.lock().unwrap();
        routing.add_modulation("lfo_1", "macro_control_1").unwrap();
        routing.add_modulation("lfo_1", "macro_control_2").unwrap();
        // Connected again in its old slot: now the newer connection, listed
        // last as upstream's `getSourceConnections` lists it.
        routing.remove_modulation("lfo_1", "macro_control_1");
        routing.add_modulation("lfo_1", "macro_control_1").unwrap();
    }
    let mut harness = fixture.harness();
    let (source, ..) = modulation_spots(&fixture);
    press(
        &mut harness,
        source,
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        source,
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    let interface = fixture.interface.borrow();
    let menu = interface.overlay().menu.as_ref().expect("the menu opened");
    let names: Vec<&str> = menu.items.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Disconnect from Macro 2",
            "Disconnect from Macro 1",
            "Disconnect all"
        ]
    );
    assert!(matches!(menu.owner, MenuOwner::ModulationSource("lfo_1")));
    let host = Host::tracking(
        fixture.synth.as_ref(),
        Some(&fixture.context),
        &fixture.open,
    );
    sas_gui::modulation::disconnect_choice(&host, "lfo_1", menu.items[1].id);
    assert_eq!(
        *fixture.synth.bus.lock().unwrap(),
        ["disconnect lfo_1 macro_control_1"]
    );
    sas_gui::modulation::disconnect_choice(&host, "lfo_1", menu.items[2].id);
    assert!(fixture.synth.modulations().is_empty());
    assert_eq!(fixture.take_log(), Vec::<String>::new());
}

/// A preset load or state restore mid-drag replaces the connection the drag
/// made: the release then writes nothing, so the old slot's values never
/// reach the new preset's host parameters.
#[test]
fn a_restore_mid_drag_drops_the_pending_connection() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (source, first, _) = modulation_spots(&fixture);
    press(
        &mut harness,
        source,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, first);
    assert_eq!(
        *fixture.synth.bus.lock().unwrap(),
        ["connect lfo_1 macro_control_1 1.00 unipolar"]
    );
    *fixture.synth.routing.lock().unwrap() = PresetState::new(&InitContent { sample_seed: 0 });
    fixture.synth.generation.fetch_add(1, Ordering::Relaxed);
    let away = Pos2::new(700.0, 5.0);
    move_to(&mut harness, away);
    release(&mut harness, away, PointerButton::Primary, Modifiers::NONE);
    assert_eq!(
        *fixture.synth.bus.lock().unwrap(),
        ["connect lfo_1 macro_control_1 1.00 unipolar"]
    );
    assert_eq!(fixture.take_log(), Vec::<String>::new());
}

/// The same when the restore lands between the last frame and the window
/// closing: the close hook writes nothing either.
#[test]
fn a_restore_before_the_window_closes_drops_the_pending_connection() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (source, first, _) = modulation_spots(&fixture);
    press(
        &mut harness,
        source,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, first);
    let pending = fixture.interface.borrow().pending_connection();
    fixture.synth.generation.fetch_add(1, Ordering::Relaxed);
    let host = Host::tracking(
        fixture.synth.as_ref(),
        Some(&fixture.context),
        &fixture.open,
    );
    pending.commit(&host);
    assert_eq!(fixture.take_log(), Vec::<String>::new());
}

/// Without a restore, the close hook writes the connection the drag made.
#[test]
fn closing_mid_drag_writes_the_pending_connection() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (source, first, _) = modulation_spots(&fixture);
    press(
        &mut harness,
        source,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, first);
    let pending = fixture.interface.borrow().pending_connection();
    let host = Host::tracking(
        fixture.synth.as_ref(),
        Some(&fixture.context),
        &fixture.open,
    );
    pending.commit(&host);
    assert_eq!(fixture.take_log(), SLOT_1_GESTURES);
}

/// The amount knobs shown last frame of `kind`.
fn knobs_of(fixture: &Fixture, kind: KnobKind) -> Vec<AmountKnob> {
    fixture
        .interface
        .borrow()
        .overlay()
        .modulation
        .shown_knobs()
        .iter()
        .filter(|knob| knob.kind == kind)
        .cloned()
        .collect()
}

/// A routing of LFO 1 and envelope 1 onto macro 1, in slots 1 and 2.
fn two_sources_on_macro_1(fixture: &Fixture) {
    let mut routing = fixture.synth.routing.lock().unwrap();
    routing.add_modulation("lfo_1", "macro_control_1").unwrap();
    routing.add_modulation("env_1", "macro_control_1").unwrap();
}

/// A source's connections show as knobs in its button
/// (`positionModulationAmountSlidersInside`); dragging one changes its
/// slot's amount in one host gesture, like any slider.
#[test]
fn a_sources_amount_knob_sets_its_slot_in_one_gesture() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    let mut harness = fixture.harness();
    harness.step();
    let knobs = knobs_of(&fixture, KnobKind::Source);
    let lfo = knobs
        .iter()
        .find(|knob| knob.route.source == "lfo_1")
        .expect("LFO 1's button shows its knob");
    assert_eq!(lfo.slider.name, "modulation_1_amount");
    assert_eq!(lfo.slider.popup_prefix, Some("Macro 1: "));
    let start = centre(lfo.bounds);
    drag(
        &mut harness,
        start,
        start + egui::vec2(0.0, -20.0),
        Modifiers::NONE,
    );
    assert_eq!(
        fixture.take_log(),
        [
            "begin modulation_1_amount",
            "set modulation_1_amount",
            "end modulation_1_amount"
        ]
    );
    assert!(fixture.synth.value("modulation_1_amount") > 0.0);
}

/// Selecting a source shows its knobs beside their destinations, 24 pixels
/// across at this size, below a control that doesn't ask otherwise
/// (`makeCurrentModulatorAmountsVisible`).
#[test]
fn a_selected_sources_knobs_sit_beside_their_destinations() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    let mut harness = fixture.harness();
    let (source, ..) = modulation_spots(&fixture);
    click(&mut harness, source);
    let macro_knob = {
        let interface = fixture.interface.borrow();
        let layout = FullLayout::new(interface.skin(), 1400, 820);
        let top = Scope::top(interface.skin(), layout.size_ratio);
        ExtraModLayout::new(&top, layout.extra_mod).macros[0].knob
    };
    let selected = knobs_of(&fixture, KnobKind::Selected);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].route.source, "lfo_1");
    assert_eq!(selected[0].slider.popup_prefix, Some("Lfo 1: "));
    let width = 24;
    assert_eq!(
        selected[0].bounds,
        Rect::new(
            macro_knob.x + macro_knob.width / 2 - width / 2,
            macro_knob.bottom(),
            width,
            width
        )
    );
}

/// Going over a control shows its other sources' knobs beside it, centred,
/// with a gap where the selected source's knob is (`makeModulationsVisible`);
/// they stay after the pointer leaves.
#[test]
fn going_over_a_control_shows_its_other_sources_knobs() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    let mut harness = fixture.harness();
    let (source, macro_1, _) = modulation_spots(&fixture);
    click(&mut harness, source);
    move_to(&mut harness, macro_1);
    let hover = knobs_of(&fixture, KnobKind::Hover);
    assert_eq!(hover.len(), 1);
    assert_eq!(hover[0].route.source, "env_1");
    // [env 1, gap, gap]: three knobs' width centred below the control.
    let selected = knobs_of(&fixture, KnobKind::Selected)[0].bounds;
    assert_eq!(hover[0].bounds.y, selected.y);
    assert_eq!(
        hover[0].bounds.x,
        selected.x + selected.width / 2 - 3 * 24 / 2
    );
    move_to(&mut harness, Pos2::new(700.0, 5.0));
    assert_eq!(knobs_of(&fixture, KnobKind::Hover).len(), 1);
}

/// An amount knob's menu: remove, bypass, polarity and stereo, and a value.
/// The toggles set the slot's control in one gesture (D18); removing
/// disconnects without the host.
#[test]
fn an_amount_knobs_menu_toggles_and_removes() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    let mut harness = fixture.harness();
    harness.step();
    let knob = knobs_of(&fixture, KnobKind::Source)
        .into_iter()
        .find(|knob| knob.route.slot == 0)
        .unwrap();
    press(
        &mut harness,
        centre(knob.bounds),
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        centre(knob.bounds),
        PointerButton::Secondary,
        Modifiers::NONE,
    );
    let menu = {
        let interface = fixture.interface.borrow();
        let menu = interface.overlay().menu.as_ref().expect("the menu opened");
        assert!(matches!(menu.owner, MenuOwner::ModulationAmount(_, _, 0)));
        menu.items.clone()
    };
    let names: Vec<&str> = menu.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Remove",
            "Bypass",
            "Make Bipolar",
            "Make Stereo",
            "",
            "Enter Value"
        ]
    );
    fixture.take_log();
    let host = Host::tracking(
        fixture.synth.as_ref(),
        Some(&fixture.context),
        &fixture.open,
    );
    assert!(sas_gui::modulation::amount_knob_choice(
        &host, 0, menu[1].id
    ));
    assert_eq!(
        fixture.take_log(),
        [
            "begin modulation_1_bypass",
            "set modulation_1_bypass",
            "end modulation_1_bypass"
        ]
    );
    assert!(sas_gui::modulation::amount_knob_choice(
        &host, 0, menu[0].id
    ));
    assert_eq!(
        *fixture.synth.bus.lock().unwrap(),
        ["disconnect lfo_1 macro_control_1"]
    );
    assert_eq!(fixture.take_log(), Vec::<String>::new());
}

/// A middle click on an amount knob toggles its bypass, in one gesture.
#[test]
fn a_middle_click_on_an_amount_knob_toggles_bypass() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    let mut harness = fixture.harness();
    harness.step();
    let knob = knobs_of(&fixture, KnobKind::Source)
        .into_iter()
        .find(|knob| knob.route.slot == 0)
        .unwrap();
    press(
        &mut harness,
        centre(knob.bounds),
        PointerButton::Middle,
        Modifiers::NONE,
    );
    release(
        &mut harness,
        centre(knob.bounds),
        PointerButton::Middle,
        Modifiers::NONE,
    );
    let log = fixture.take_log();
    assert_eq!(
        log.iter()
            .filter(|line| line.contains("bypass"))
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "begin modulation_1_bypass",
            "set modulation_1_bypass",
            "end modulation_1_bypass"
        ]
    );
    harness.step();
    assert_eq!(fixture.synth.value("modulation_1_bypass"), 1.0);
}

/// LFO 1 on seven controls: more knobs than its button holds.
fn seven_on_lfo_1(fixture: &Fixture) {
    let mut routing = fixture.synth.routing.lock().unwrap();
    for destination in [
        "osc_1_level",
        "osc_1_pan",
        "osc_1_wave_frame",
        "osc_1_unison_detune",
        "macro_control_2",
        "reverb_dry_wet",
        "env_1_attack",
    ] {
        routing.add_modulation("lfo_1", destination).unwrap();
    }
}

/// Where LFO 1's expand button is: its button's modulation area.
fn lfo_1_expand_button(fixture: &Fixture) -> Rect {
    let interface = fixture.interface.borrow();
    let layout = FullLayout::new(interface.skin(), 1400, 820);
    let top = Scope::top(interface.skin(), layout.size_ratio);
    let modulation = ModulationLayout::new(&top, layout.modulation);
    let tabs = interface.modulation().tab_selector(1);
    let tab = tabs.button_bounds(modulation.lfo_tabs, tabs.num_shown(|_| false))[0];
    sas_gui::controls::modulation_button::modulation_area_bounds(tab, top.widget_margin() as i32)
}

/// A source with more knobs than fit shows an expand button instead; a
/// press on it opens the knobs in a callout below the source, 30 pixels
/// each from the bottom row up, and a press on it again or outside the
/// callout closes it (`showModulationAmountCallout`, `expansionFocusLost`).
#[test]
fn a_source_with_more_knobs_than_fit_opens_them_in_a_callout() {
    let fixture = Fixture::new();
    seven_on_lfo_1(&fixture);
    let mut harness = fixture.harness();
    harness.step();
    assert!(knobs_of(&fixture, KnobKind::Source).is_empty());
    let expand = lfo_1_expand_button(&fixture);
    click(&mut harness, centre(expand));
    let callout = knobs_of(&fixture, KnobKind::Callout);
    assert_eq!(callout.len(), 7);
    assert!(callout
        .iter()
        .all(|knob| knob.bounds.width == 30 && knob.bounds.height == 30));
    // The first row is the bottom one, left to right.
    assert_eq!(callout[1].bounds.x - callout[0].bounds.x, 30);
    assert!(callout.last().unwrap().bounds.y < callout[0].bounds.y);
    assert_eq!(callout[0].route.destination, "osc_1_level");

    click(&mut harness, centre(expand));
    assert!(knobs_of(&fixture, KnobKind::Callout).is_empty());
    click(&mut harness, centre(expand));
    assert_eq!(knobs_of(&fixture, KnobKind::Callout).len(), 7);
    click(&mut harness, Pos2::new(700.0, 5.0));
    assert!(knobs_of(&fixture, KnobKind::Callout).is_empty());
}

/// A knob in the callout is an amount knob like the others: a drag on it
/// is one gesture on its slot, and the callout stays open.
#[test]
fn a_callout_knob_sets_its_slot() {
    let fixture = Fixture::new();
    seven_on_lfo_1(&fixture);
    let mut harness = fixture.harness();
    harness.step();
    click(&mut harness, centre(lfo_1_expand_button(&fixture)));
    let knob = knobs_of(&fixture, KnobKind::Callout)[1].clone();
    assert_eq!(knob.slider.name, "modulation_2_amount");
    fixture.take_log();
    drag(
        &mut harness,
        centre(knob.bounds),
        centre(knob.bounds) + egui::vec2(0.0, -20.0),
        Modifiers::NONE,
    );
    assert_eq!(
        fixture.take_log(),
        [
            "begin modulation_2_amount",
            "set modulation_2_amount",
            "end modulation_2_amount"
        ]
    );
    assert_eq!(knobs_of(&fixture, KnobKind::Callout).len(), 7);
}

/// A middle drag on an amount knob toggles its bypass and drags its amount
/// too, as upstream's `mouseDown` toggles bypass and then hands the press
/// to the slider, which drags with any button but the menu's.
#[test]
fn a_middle_drag_on_an_amount_knob_bypasses_and_drags() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    let mut harness = fixture.harness();
    harness.step();
    let knob = knobs_of(&fixture, KnobKind::Source)
        .into_iter()
        .find(|knob| knob.route.slot == 0)
        .unwrap();
    let start = centre(knob.bounds);
    press(&mut harness, start, PointerButton::Middle, Modifiers::NONE);
    move_to(&mut harness, start + egui::vec2(0.0, -20.0));
    release(
        &mut harness,
        start + egui::vec2(0.0, -20.0),
        PointerButton::Middle,
        Modifiers::NONE,
    );
    let log = fixture.take_log();
    let of = |name: &str| {
        log.iter()
            .filter(|line| line.ends_with(name))
            .map(|line| line.split(' ').next().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(of("modulation_1_bypass"), ["begin", "set", "end"]);
    assert_eq!(of("modulation_1_amount"), ["begin", "set", "end"]);
    harness.step();
    assert_eq!(fixture.synth.value("modulation_1_bypass"), 1.0);
    assert!(fixture.synth.value("modulation_1_amount") > 0.0);
}

/// JUCE counts a double click only when both presses use the same buttons
/// (`MouseInputSourceImpl::setButtons`), so a middle click then a left one
/// leaves the value, while two middle clicks return it to the default.
#[test]
fn a_double_click_needs_the_same_button_twice() {
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
    let dragged = fixture.synth.value("portamento_time");
    assert_ne!(dragged, default);
    let past_timeout = |harness: &mut egui_kittest::Harness<'_>| {
        for _ in 0..30 {
            harness.step();
        }
    };
    let click_with = |harness: &mut egui_kittest::Harness<'_>, button| {
        press(harness, start, button, Modifiers::NONE);
        release(harness, start, button, Modifiers::NONE);
    };

    past_timeout(&mut harness);
    click_with(&mut harness, PointerButton::Middle);
    click_with(&mut harness, PointerButton::Primary);
    assert_eq!(fixture.synth.value("portamento_time"), dragged);

    past_timeout(&mut harness);
    click_with(&mut harness, PointerButton::Middle);
    click_with(&mut harness, PointerButton::Middle);
    assert_eq!(fixture.synth.value("portamento_time"), default);
}

#[test]
fn resynthesizing_reaches_the_model() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    view.borrow_mut().set_clipboard_reader(empty_clipboard);
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    open_wavetable_menu(&mut harness, &overlay);
    choose_wavetable_row(&mut harness, 3);
    assert_eq!(
        *fixture.synth.wavetable_edits.lock().unwrap(),
        ["resynthesize 0"]
    );
    // The render's result arrives through the model; nothing reaches the host.
    assert!(fixture.take_log().is_empty());
}

/// Drags `paths` over the wavetable view at `y` pixels below its top.
fn hover_files(harness: &mut Harness, paths: &[&str], y: f32) {
    let at = Pos2::new(centre(WAVETABLE).x, WAVETABLE.y as f32 + y);
    harness.input_mut().hovered_files = paths
        .iter()
        .map(|path| egui::HoveredFile {
            path: Some(path.into()),
            mime: String::new(),
        })
        .collect();
    harness.event(Event::PointerMoved(at));
    harness.step();
}

/// Drops `paths` on the wavetable view at `y` pixels below its top.
fn drop_files(harness: &mut Harness, paths: &[&str], y: f32) {
    let at = Pos2::new(centre(WAVETABLE).x, WAVETABLE.y as f32 + y);
    harness.input_mut().hovered_files.clear();
    harness.input_mut().dropped_files = paths
        .iter()
        .map(|path| {
            std::sync::Arc::new(nice_plug_egui::file_drop::DroppedPath(path.into()))
                as egui::DroppedFileHandle
        })
        .collect();
    harness.event(Event::PointerMoved(at));
    harness.step();
    harness.step();
}

#[test]
fn the_wavetable_view_is_a_drop_target_for_the_models_audio_files() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    let _ = nice_plug_egui::file_drop::take_drop_targets(&harness.ctx);
    harness.step();
    let targets = nice_plug_egui::file_drop::take_drop_targets(&harness.ctx);
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].extensions, ["wav"]);
    assert_eq!(
        targets[0].rect.min,
        Pos2::new(WAVETABLE.x as f32, WAVETABLE.y as f32)
    );
}

#[test]
fn a_hovering_file_picks_how_it_loads_by_height() {
    use sas_gui::controls::wavetable_3d::AudioFileLoadStyle;
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    // The view is 100 pixels high: the top 3/8 splices, the bottom 3/8
    // pitch-splices, the middle vocodes.
    for (y, style) in [
        (10.0, AudioFileLoadStyle::WavetableSplice),
        (50.0, AudioFileLoadStyle::Vocoded),
        (90.0, AudioFileLoadStyle::Pitched),
    ] {
        hover_files(&mut harness, &["/tmp/loop.wav"], y);
        assert_eq!(view.borrow().drag_load_style(), Some(style), "at {y}");
    }
    // Files the view doesn't take show no overlay.
    for files in [&["/tmp/a.wav", "/tmp/b.wav"][..], &["/tmp/a.flac"][..]] {
        hover_files(&mut harness, files, 50.0);
        assert_eq!(view.borrow().drag_load_style(), None);
    }
    // Over the view's neighbour: none either.
    harness.input_mut().hovered_files = vec![egui::HoveredFile {
        path: Some("/tmp/loop.wav".into()),
        mime: String::new(),
    }];
    harness.event(Event::PointerMoved(Pos2::new(350.0, 250.0)));
    harness.step();
    assert_eq!(view.borrow().drag_load_style(), None);
}

#[test]
fn a_dropped_file_loads_by_where_it_landed() {
    let fixture = Fixture::new();
    let overlay = RefCell::new(Overlay::default());
    let view = RefCell::new(Wavetable3d::new(0));
    let mut harness = wavetable_harness(&fixture, &overlay, &view);
    drop_files(&mut harness, &["/tmp/top.wav"], 10.0);
    drop_files(&mut harness, &["/tmp/middle.wav"], 50.0);
    drop_files(&mut harness, &["/tmp/bottom.wav"], 90.0);
    // Files the view doesn't take load nothing.
    drop_files(&mut harness, &["/tmp/a.wav", "/tmp/b.wav"], 50.0);
    drop_files(&mut harness, &["/tmp/a.aiff"], 50.0);
    assert_eq!(
        *fixture.synth.wavetable_edits.lock().unwrap(),
        [
            "audio 0 /tmp/top.wav WavetableSplice",
            "audio 0 /tmp/middle.wav Vocoded",
            "audio 0 /tmp/bottom.wav Pitched",
        ]
    );
    assert_eq!(view.borrow().drag_load_style(), None);
    assert!(fixture.take_log().is_empty());
}

/// The editing amount overlay as the editor last drew it.
fn amount_overlay(fixture: &Fixture) -> Option<sas_gui::primitives::Quad> {
    let host = Host::tracking(
        fixture.synth.as_ref(),
        Some(&fixture.context),
        &fixture.open,
    );
    let interface = fixture.interface.borrow();
    let ratio = FullLayout::new(interface.skin(), 1400, 820).size_ratio;
    let top = Scope::top(interface.skin(), ratio);
    interface
        .overlay()
        .modulation
        .amount_overlay(&host, &top)
        .map(|(quad, _)| quad)
}

/// A knob's angle for `percent` of its range (`kRotaryAngle`).
fn knob_angle(percent: f32) -> f32 {
    let angle = 0.8 * std::f32::consts::PI;
    -angle + 2.0 * angle * percent
}

/// Hovering a source's amount knob shows the connection's range on its
/// destination's meter (`hoverStarted` → `showModulationAmountOverlay`),
/// and leaving it hides it (`hoverEnded`).
#[test]
fn hovering_an_amount_knob_shows_its_range_on_the_destination() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    fixture.synth.set("modulation_1_amount", 0.25);
    fixture.synth.set("modulation_1_bipolar", 1.0);
    let mut harness = fixture.harness();
    harness.step();
    assert!(amount_overlay(&fixture).is_none());
    let knob = knobs_of(&fixture, KnobKind::Source)
        .into_iter()
        .find(|knob| knob.route.slot == 0)
        .unwrap();
    move_to(&mut harness, centre(knob.bounds));
    let quad = amount_overlay(&fixture).expect("the overlay shows");
    assert_eq!(quad.kind, sas_gui::primitives::QuadKind::RotaryModulation);
    assert_eq!(quad.thickness, 2.0);
    let info = Host::info("macro_control_1");
    let percent = (fixture.synth.value("macro_control_1") - info.min) / (info.max - info.min);
    // setModulationAmountQuad keeps a knob's range within its arc.
    let angle = 0.8 * std::f32::consts::PI;
    let (low, high) = (
        knob_angle(percent - 0.125).max(-angle),
        knob_angle(percent + 0.125).min(angle),
    );
    for (value, expected) in quad.shader_values.iter().zip([low, high, low, high]) {
        assert!((value - expected).abs() < 1e-5, "{:?}", quad.shader_values);
    }
    move_to(&mut harness, Pos2::new(700.0, 10.0));
    assert!(amount_overlay(&fixture).is_none());
}

/// While a drag holds a temporary connection, the overlay shows its range
/// from the values the drag gave it, which the host hasn't been told yet
/// (D18).
#[test]
fn a_drag_shows_its_temporary_connections_range() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    let (source, first, _) = modulation_spots(&fixture);
    press(
        &mut harness,
        source,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, first);
    assert_eq!(fixture.synth.value("modulation_1_amount"), 0.0);
    let quad = amount_overlay(&fixture).expect("the overlay shows");
    let info = Host::info("macro_control_1");
    let percent = (fixture.synth.value("macro_control_1") - info.min) / (info.max - info.min);
    let (low, high) = (knob_angle(percent), 0.8 * std::f32::consts::PI);
    for (value, expected) in quad.shader_values.iter().zip([low, high, low, high]) {
        assert!((value - expected).abs() < 1e-5, "{:?}", quad.shader_values);
    }
    release(&mut harness, first, PointerButton::Primary, Modifiers::NONE);
    move_to(&mut harness, Pos2::new(700.0, 10.0));
    assert!(amount_overlay(&fixture).is_none());
}

/// ENV 1's knob in its source button for its connection to Macro 1 (slot 2).
fn env_knob(fixture: &Fixture) -> AmountKnob {
    knobs_of(fixture, KnobKind::Source)
        .into_iter()
        .find(|knob| knob.route.slot == 1)
        .expect("ENV 1's button shows its knob")
}

/// A drop on another connection's amount knob
/// (`modulationDraggedToHoverSlider`): the dragged source gets an aux
/// connection to that slot's amount with half of it, and the slot's own
/// amount goes to 0, through the engine only while dragging; the knob now
/// drives the aux slot at half scale and shows its centre dot. The host
/// hears the new slot and the emptied amount once the drag ends (D18).
#[test]
fn a_drop_on_an_amount_knob_makes_an_aux_connection() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    fixture.synth.set("modulation_2_amount", 0.6);
    let mut harness = fixture.harness();
    harness.step();
    let (source, _, _) = modulation_spots(&fixture);
    let knob = env_knob(&fixture);
    fixture.take_log();
    fixture.synth.bus.lock().unwrap().clear();

    press(
        &mut harness,
        source,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, centre(knob.bounds));
    assert_eq!(
        *fixture.synth.bus.lock().unwrap(),
        [
            "values 1 0.00 unipolar",
            "connect lfo_1 modulation_2_amount 0.30 unipolar",
        ]
    );
    assert_eq!(fixture.take_log(), Vec::<String>::new());
    let knob = env_knob(&fixture);
    assert_eq!(knob.slider.name, "modulation_3_amount");
    assert_eq!(knob.slider.value_scale, 0.5);
    assert!(matches!(
        knob.slider.style,
        SliderStyle::ModulationKnob { aux: true, .. }
    ));

    release(
        &mut harness,
        centre(knob.bounds),
        PointerButton::Primary,
        Modifiers::NONE,
    );
    let log = fixture.take_log();
    assert!(
        log.contains(&"set modulation_3_amount".to_string()),
        "{log:?}"
    );
    assert!(
        log.contains(&"set modulation_2_amount".to_string()),
        "{log:?}"
    );
    harness.step();
    assert!((fixture.synth.value("modulation_3_amount") - 0.3).abs() < 1e-6);
    assert_eq!(fixture.synth.value("modulation_2_amount"), 0.0);
    assert!(fixture.synth.modulations().contains(&Route {
        slot: 2,
        source: "lfo_1".into(),
        destination: "modulation_2_amount".into(),
    }));
}

/// Leaving the knob mid-drag (`clearTemporaryHoverModulation`) removes the
/// aux connection and gives the knob's connection its amount back; the host
/// hears nothing.
#[test]
fn leaving_an_amount_knob_mid_drag_undoes_the_aux_connection() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    fixture.synth.set("modulation_2_amount", 0.6);
    let mut harness = fixture.harness();
    harness.step();
    let (source, _, _) = modulation_spots(&fixture);
    let knob = env_knob(&fixture);
    fixture.take_log();
    fixture.synth.bus.lock().unwrap().clear();

    press(
        &mut harness,
        source,
        PointerButton::Primary,
        Modifiers::NONE,
    );
    move_to(&mut harness, centre(knob.bounds));
    let away = Pos2::new(700.0, 10.0);
    move_to(&mut harness, away);
    assert_eq!(
        *fixture.synth.bus.lock().unwrap(),
        [
            "values 1 0.00 unipolar",
            "connect lfo_1 modulation_2_amount 0.30 unipolar",
            "disconnect lfo_1 modulation_2_amount",
            "values 1 0.60 unipolar",
        ]
    );
    release(&mut harness, away, PointerButton::Primary, Modifiers::NONE);
    harness.step();
    assert_eq!(fixture.take_log(), Vec::<String>::new());
    assert_eq!(fixture.synth.modulations().len(), 2);
    assert_eq!(env_knob(&fixture).slider.name, "modulation_2_amount");
}

/// An aux knob drives the end of its chain at its share
/// (`sliderValueChanged`: `getAuxMultiplier`), and reads in the first
/// connection's destination units (`setModulationSliderScale`).
#[test]
fn an_aux_knob_drives_its_aux_slot_at_half_scale() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    fixture
        .synth
        .routing
        .lock()
        .unwrap()
        .add_modulation("lfo_2", "modulation_2_amount")
        .unwrap();
    fixture.synth.set("modulation_3_amount", 0.25);
    let mut harness = fixture.harness();
    harness.step();
    let knob = env_knob(&fixture);
    assert_eq!(knob.slider.name, "modulation_3_amount");
    fixture.take_log();
    let start = centre(knob.bounds);
    drag(
        &mut harness,
        start,
        start + egui::vec2(0.0, -20.0),
        Modifiers::NONE,
    );
    let log = fixture.take_log();
    assert_eq!(
        log,
        [
            "begin modulation_3_amount",
            "set modulation_3_amount",
            "end modulation_3_amount"
        ]
    );
    harness.step();
    let shown = 2.0 * fixture.synth.value("modulation_3_amount");
    assert!(shown > 0.5, "{shown}");
    assert_eq!(fixture.synth.value("modulation_2_amount"), 0.0);
}

/// Dragging an amount knob: the overlay follows the amount every frame
/// (`sliderValueChanged` → `showModulationAmountOverlay`), stays up while
/// the pointer is off the knob mid-drag (`changing_hover_modulation_`), and
/// hides on release away from it (`hoverEnded`).
#[test]
fn the_amount_overlay_follows_a_knob_drag_and_hides_on_release() {
    let fixture = Fixture::new();
    two_sources_on_macro_1(&fixture);
    let mut harness = fixture.harness();
    harness.step();
    let knob = knobs_of(&fixture, KnobKind::Source)
        .into_iter()
        .find(|knob| knob.route.slot == 0)
        .unwrap();
    let start = centre(knob.bounds);
    press(&mut harness, start, PointerButton::Primary, Modifiers::NONE);
    let mut last = None;
    for step in 1..=4 {
        move_to(&mut harness, start + egui::vec2(0.0, -10.0 * step as f32));
        let quad = amount_overlay(&fixture).expect("the overlay shows while dragging");
        let high = quad.shader_values[1];
        if let Some(last) = last {
            assert!(high > last, "step {step}: {high} after {last}");
        }
        last = Some(high);
    }
    let away = Pos2::new(700.0, 10.0);
    move_to(&mut harness, away);
    assert!(
        amount_overlay(&fixture).is_some(),
        "still dragging off the knob"
    );
    release(&mut harness, away, PointerButton::Primary, Modifiers::NONE);
    harness.step();
    harness.step();
    assert!(amount_overlay(&fixture).is_none());
}

/// LFO 1 on Macro 1 (slot 1), with LFO 2 on that slot's amount at 0.3
/// (slot 2), as `modulations_aux` has it: slot 1's knobs show 0.6.
fn lfo_1_aux_on_macro_1(fixture: &Fixture) {
    let mut routing = fixture.synth.routing.lock().unwrap();
    routing.add_modulation("lfo_1", "macro_control_1").unwrap();
    routing
        .add_modulation("lfo_2", "modulation_1_amount")
        .unwrap();
    drop(routing);
    fixture.synth.set("modulation_2_amount", 0.3);
}

/// `addAuxConnection` calls `setAux` on the amount and hover knobs only: a
/// selected knob drives the chain the same way but has no centre dot.
#[test]
fn a_selected_aux_knob_has_no_centre_dot() {
    let fixture = Fixture::new();
    lfo_1_aux_on_macro_1(&fixture);
    let mut harness = fixture.harness();
    harness.step();
    let (source, _, _) = modulation_spots(&fixture);
    click(&mut harness, source);
    harness.step();
    let of = |kind| {
        knobs_of(&fixture, kind)
            .into_iter()
            .find(|knob| knob.route.slot == 0)
            .unwrap()
    };
    let in_button = of(KnobKind::Source);
    let selected = of(KnobKind::Selected);
    for knob in [&in_button, &selected] {
        assert_eq!(knob.slider.name, "modulation_2_amount");
        assert_eq!(knob.slider.value_scale, 0.5);
    }
    assert!(matches!(
        in_button.slider.style,
        SliderStyle::ModulationKnob { aux: true, .. }
    ));
    assert!(matches!(
        selected.slider.style,
        SliderStyle::ModulationKnob { aux: false, .. }
    ));
}

/// `showModulationAmountOverlay` draws `slider->getValue()`: an aux knob's
/// shown amount (twice its aux slot's), not its own slot's, which is 0.
#[test]
fn an_aux_knobs_overlay_shows_its_shown_amount() {
    let fixture = Fixture::new();
    lfo_1_aux_on_macro_1(&fixture);
    let mut harness = fixture.harness();
    harness.step();
    let knob = knobs_of(&fixture, KnobKind::Source)
        .into_iter()
        .find(|knob| knob.route.slot == 0)
        .unwrap();
    move_to(&mut harness, centre(knob.bounds));
    let quad = amount_overlay(&fixture).expect("the overlay shows");
    let info = Host::info("macro_control_1");
    let percent = (fixture.synth.value("macro_control_1") - info.min) / (info.max - info.min);
    let angle = 0.8 * std::f32::consts::PI;
    let shown = 2.0 * fixture.synth.value("modulation_2_amount");
    let (low, high) = (knob_angle(percent), knob_angle(percent + shown).min(angle));
    assert!(high > low + 1.0, "{:?}", quad.shader_values);
    for (value, expected) in quad.shader_values.iter().zip([low, high, low, high]) {
        assert!((value - expected).abs() < 1e-5, "{:?}", quad.shader_values);
    }
}

/// With the drawer open but the window not widened (the host refused), the
/// panel overlays the editor as before: inside its bounds, below the header.
#[test]
fn without_a_wider_window_the_open_drawer_overlays_the_editor() {
    let mut interface = Interface::default();
    interface.set_drawer_open(true);
    let window = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 820.0));
    let placement = interface.chat_placement(window, 1.0);
    assert_eq!(placement.mode, sas_gui::ChatMode::Overlay);
    let panel = placement.panel();
    assert!(placement.editor.contains_rect(panel));
    assert!(panel.min.y >= placement.header_bottom);
    assert!((panel.width() - sas_gui::chat::PANEL_WIDTH).abs() < 1.0);
}
