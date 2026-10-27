//! The Effects tab against real host parameters, driven through
//! egui_kittest (no GPU needed): the effect order's drag and power
//! buttons, a display drag and the column's scrolling. A recording
//! `GuiContextInner` stands in for the host, applying sets at once, at the
//! next frame, or only when the test says (a host that waits for the
//! gesture to end).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use nice_plug::context::gui::{GuiContext, GuiContextInner};
use nice_plug::context::PluginApi;
use nice_plug::prelude::{FloatParam, FloatRange, Param, ParamPtr, PluginState};
use sas_gui::layout::{FullLayout, Rect};
use sas_gui::scope::Scope;
use sas_gui::sections::effects::compressor::CompressorLayout;
use sas_gui::sections::effects::delay::DelayLayout;
use sas_gui::sections::effects::equalizer::EqualizerLayout;
use sas_gui::sections::effects::order::OrderLayout;
use sas_gui::sections::effects::{
    self, decode_order, encode_order, move_effect, Effect, EffectsLayout, ORDER_CONTROL,
};
use sas_gui::skin::Skin;
use sas_gui::{EditorModel, Host, HostParam, Interface, OpenGestures, Tab};
use sas_preset::registry::registry;

/// Every registry control, linear over its range.
struct Synth {
    params: BTreeMap<&'static str, FloatParam>,
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
        Arc::new(Synth { params })
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

    /// Sets a parameter as a preset load would, without a gesture.
    fn set(&self, name: &str, plain: f32) {
        unsafe {
            let ptr = self.params[name].as_ptr();
            ptr._internal_set_normalized_value(ptr.preview_normalized(plain));
        }
    }
}

impl EditorModel for Synth {
    fn preset_name(&self) -> String {
        String::new()
    }

    fn param(&self, name: &str) -> Option<HostParam> {
        self.params.get(name).map(|param| param.as_ptr())
    }
}

/// When the host applies the editor's sets.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Apply {
    /// At the start of the next frame, as if a block ran in between.
    NextFrame,
    /// Only when the test calls `apply_pending`.
    OnRequest,
}

/// The host: logs gestures and applies values.
struct Recorder {
    synth: Arc<Synth>,
    log: Mutex<Vec<String>>,
    pending: Mutex<Vec<(ParamPtr, f32)>>,
    apply: Apply,
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
    /// The interface's skin, for laying out where to point.
    skin: Skin,
}

impl Fixture {
    /// The Effects tab with the effects in `enabled` on.
    fn new(apply: Apply, enabled: &[Effect]) -> Fixture {
        let synth = Synth::new();
        for effect in enabled {
            synth.set(effect.enable_control(), 1.0);
        }
        let recorder = Arc::new(Recorder {
            synth: synth.clone(),
            log: Mutex::new(Vec::new()),
            pending: Mutex::new(Vec::new()),
            apply,
        });
        let context = GuiContext::new(recorder.clone());
        let mut interface = Interface::default();
        interface.set_tab(Tab::Effects);
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
                if self.recorder.apply == Apply::NextFrame {
                    self.recorder.apply_pending();
                }
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

    fn order(&self) -> [usize; effects::NUM_EFFECTS] {
        decode_order(self.synth.value(ORDER_CONTROL))
    }

    fn effects_layout(&self) -> (EffectsLayout, Scope<'_>) {
        let layout = FullLayout::new(&self.skin, 1400, 820);
        let top = Scope::top(&self.skin, layout.size_ratio);
        let enabled = Effect::ALL.map(|effect| self.synth.value(effect.enable_control()) != 0.0);
        let fx = EffectsLayout::new(
            &top.within(effects::SKIN),
            layout.effects,
            self.order(),
            enabled,
        );
        (fx, top)
    }

    fn order_layout(&self) -> OrderLayout {
        let (fx, top) = self.effects_layout();
        OrderLayout::new(&top.within(effects::SKIN), fx.order, self.order())
    }
}

fn centre(rect: Rect) -> Pos2 {
    Pos2::new(
        rect.x as f32 + rect.width as f32 / 2.0,
        rect.y as f32 + rect.height as f32 / 2.0,
    )
}

fn press(harness: &mut Harness, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

fn release(harness: &mut Harness, pos: Pos2) {
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    harness.step();
}

/// Drags from `from` to `to` in `steps` pointer moves, a frame each.
fn drag(harness: &mut Harness, from: Pos2, to: Pos2, steps: usize) {
    press(harness, from);
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        harness.event(Event::PointerMoved(from + (to - from) * t));
        harness.step();
    }
    release(harness, to);
}

/// Where a tile's centre would be after the drag: the middle of the tile at
/// place `place` in the order now.
fn place_centre(fixture: &Fixture, place: usize) -> Pos2 {
    let layout = fixture.order_layout();
    centre(layout.tiles[fixture.order()[place]])
}

/// The tile pressed is the one that moves, the order changes as the
/// pointer crosses each tile, and the whole drag is one host gesture.
fn check_order_drag(apply: Apply, steps: usize) {
    let fixture = Fixture::new(apply, &[]);
    let mut harness = fixture.harness();
    let start = fixture.order();
    let from = place_centre(&fixture, 0);
    let to = place_centre(&fixture, 3);
    drag(&mut harness, from, to, steps);
    if apply == Apply::OnRequest {
        assert_eq!(fixture.order(), start, "nothing applied before the request");
        fixture.recorder.apply_pending();
    }
    let mut expected = start;
    move_effect(&mut expected, 0, 3);
    assert_eq!(fixture.order(), expected);
    assert_eq!(
        fixture.take_log(),
        [
            format!("begin {ORDER_CONTROL}"),
            format!("set {ORDER_CONTROL}"),
            format!("end {ORDER_CONTROL}"),
        ]
    );
    assert_eq!(fixture.synth.value(ORDER_CONTROL), encode_order(expected));
}

#[test]
fn a_slow_order_drag_moves_the_pressed_tile_in_one_gesture() {
    check_order_drag(Apply::NextFrame, 24);
}

#[test]
fn a_fast_order_drag_moves_the_pressed_tile() {
    // One move, far past egui's drag threshold, before the drag starts.
    check_order_drag(Apply::NextFrame, 1);
}

#[test]
fn an_order_drag_works_with_a_host_that_applies_at_release() {
    check_order_drag(Apply::OnRequest, 24);
    check_order_drag(Apply::OnRequest, 1);
}

#[test]
fn an_order_power_button_toggles_its_effect_in_one_gesture() {
    let fixture = Fixture::new(Apply::NextFrame, &[]);
    let mut harness = fixture.harness();
    let button = fixture.order_layout().power_buttons[Effect::Reverb as usize];
    press(&mut harness, centre(button));
    release(&mut harness, centre(button));
    assert_eq!(fixture.synth.value("reverb_on"), 1.0);
    assert_eq!(
        fixture.take_log(),
        ["begin reverb_on", "set reverb_on", "end reverb_on"]
    );
    assert_eq!(
        fixture.order(),
        decode_order(registry().get(ORDER_CONTROL).unwrap().default_value)
    );
}

/// Dragging the delay's filter display moves its cutoff across and its
/// spread up and down, each in one gesture.
#[test]
fn a_display_drag_is_one_gesture_per_control() {
    let fixture = Fixture::new(Apply::NextFrame, &[Effect::Delay]);
    let mut harness = fixture.harness();
    let (fx, top) = fixture.effects_layout();
    let bounds = fx.effects[Effect::Delay as usize].unwrap();
    let delay = DelayLayout::new(&top.within(Effect::Delay.skin()), bounds, 0);
    let cutoff = fixture.synth.value("delay_filter_cutoff");
    let spread = fixture.synth.value("delay_filter_spread");
    let from = centre(delay.filter_viewer);
    // Right and down: the spread starts at its maximum.
    drag(&mut harness, from, from + egui::vec2(20.0, 10.0), 8);
    assert!(fixture.synth.value("delay_filter_cutoff") > cutoff);
    assert!(fixture.synth.value("delay_filter_spread") < spread);
    let log = fixture.take_log();
    for name in ["delay_filter_cutoff", "delay_filter_spread"] {
        let count = |what: &str| {
            log.iter()
                .filter(|line| **line == format!("{what} {name}"))
                .count()
        };
        assert_eq!((count("begin"), count("end")), (1, 1), "{name}: {log:?}");
        let begin = log.iter().position(|line| *line == format!("begin {name}"));
        let end = log.iter().position(|line| *line == format!("end {name}"));
        assert!(begin < end);
    }
}

/// The wheel scrolls the column only as far as its contents go, and not
/// at all when they fit.
#[test]
fn scrolling_stays_within_the_column() {
    let fixture = Fixture::new(Apply::NextFrame, &Effect::ALL);
    let mut harness = fixture.harness();
    let (fx, _) = fixture.effects_layout();
    let max_scroll = (fx.container.height - fx.viewport.height) as f32;
    assert!(max_scroll > 0.0);
    let inside = centre(fx.viewport);
    harness.event(Event::PointerMoved(inside));
    for _ in 0..40 {
        harness.event(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -200.0),
            modifiers: Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        });
        harness.step();
    }
    harness.run_steps(30);
    assert_eq!(fixture.interface.borrow().effects_scroll(), max_scroll);
    for _ in 0..40 {
        harness.event(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 200.0),
            modifiers: Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
        });
        harness.step();
    }
    harness.run_steps(30);
    assert_eq!(fixture.interface.borrow().effects_scroll(), 0.0);

    let few = Fixture::new(Apply::NextFrame, &[Effect::Chorus]);
    few.interface.borrow_mut().set_effects_scroll(500.0);
    let mut harness = few.harness();
    harness.step();
    assert_eq!(few.interface.borrow().effects_scroll(), 0.0);
}

/// The gestures logged for `name`: how many begins and ends.
fn gesture_count(log: &[String], name: &str) -> (usize, usize) {
    let count = |what: &str| {
        log.iter()
            .filter(|line| **line == format!("{what} {name}"))
            .count()
    };
    (count("begin"), count("end"))
}

/// Dragging the compressor's low upper threshold down moves only it (and
/// its lower threshold, if pushed), each in one gesture.
#[test]
fn a_compressor_threshold_drag_is_one_gesture_for_what_it_moves() {
    let fixture = Fixture::new(Apply::NextFrame, &[Effect::Compressor]);
    let mut harness = fixture.harness();
    let (fx, top) = fixture.effects_layout();
    let bounds = fx.effects[Effect::Compressor as usize].unwrap();
    let editor = CompressorLayout::new(&top.within(Effect::Compressor.skin()), bounds).editor;
    let threshold = fixture.synth.value("compressor_low_upper_threshold");
    // The low band's middle, at its upper threshold handle (getYForDb, kept
    // a grab radius inside the top).
    let x = editor.x as f32 + 0.05 * editor.width as f32 + 0.4 / 3.0 * editor.width as f32;
    let t = (threshold - -80.0) / 80.0;
    let y_for_db = editor.height as f32 * (1.0 - (2.0 * t - 1.0)) * 0.5;
    let y = editor.y as f32 + y_for_db.max(8.0 * top.size_ratio());
    let from = Pos2::new(x, y);
    drag(&mut harness, from, from + egui::vec2(0.0, 20.0), 8);
    assert!(fixture.synth.value("compressor_low_upper_threshold") < threshold);
    let log = fixture.take_log();
    assert_eq!(
        gesture_count(&log, "compressor_low_upper_threshold"),
        (1, 1),
        "{log:?}"
    );
    for untouched in [
        "compressor_band_upper_threshold",
        "compressor_low_upper_ratio",
    ] {
        assert_eq!(gesture_count(&log, untouched), (0, 0), "{log:?}");
    }
}

/// Dragging the EQ's middle handle moves its cutoff across and its gain up
/// and down, each in one gesture.
#[test]
fn an_eq_handle_drag_is_one_gesture_per_control() {
    let fixture = Fixture::new(Apply::NextFrame, &[Effect::Eq]);
    let mut harness = fixture.harness();
    let (fx, top) = fixture.effects_layout();
    let bounds = fx.effects[Effect::Eq as usize].unwrap();
    let response = EqualizerLayout::new(&top.within(Effect::Eq.skin()), bounds).response;
    let cutoff_info = registry().get("eq_band_cutoff").unwrap();
    let gain_info = registry().get("eq_low_gain").unwrap();
    let buffer = (gain_info.max - gain_info.min) * 0.2;
    let (min_db, max_db) = (gain_info.min - buffer, gain_info.max + buffer);
    let cutoff = fixture.synth.value("eq_band_cutoff");
    let gain = fixture.synth.value("eq_band_gain");
    let from = Pos2::new(
        response.x as f32
            + response.width as f32 * (cutoff - cutoff_info.min)
                / (cutoff_info.max - cutoff_info.min),
        response.y as f32 + response.height as f32 * (max_db - gain) / (max_db - min_db),
    );
    drag(&mut harness, from, from + egui::vec2(15.0, -10.0), 8);
    assert!(fixture.synth.value("eq_band_cutoff") > cutoff);
    assert!(fixture.synth.value("eq_band_gain") > gain);
    let log = fixture.take_log();
    for name in ["eq_band_cutoff", "eq_band_gain"] {
        assert_eq!(gesture_count(&log, name), (1, 1), "{name}: {log:?}");
    }
    assert_eq!(gesture_count(&log, "eq_low_cutoff"), (0, 0), "{log:?}");
}

/// The EQ's mode buttons toggle their mode in one gesture.
#[test]
fn an_eq_mode_button_toggles_in_one_gesture() {
    let fixture = Fixture::new(Apply::NextFrame, &[Effect::Eq]);
    let mut harness = fixture.harness();
    let (fx, top) = fixture.effects_layout();
    let bounds = fx.effects[Effect::Eq as usize].unwrap();
    let button = EqualizerLayout::new(&top.within(Effect::Eq.skin()), bounds).band_mode;
    press(&mut harness, centre(button));
    release(&mut harness, centre(button));
    assert_eq!(fixture.synth.value("eq_band_mode"), 1.0);
    assert_eq!(
        fixture.take_log(),
        ["begin eq_band_mode", "set eq_band_mode", "end eq_band_mode"]
    );
}

/// A press on the scroll bar's track beside the thumb pages towards it,
/// within the column; held, it keeps paging (after 0.4 s, then every
/// 0.04 s) but never past the end. The column is under two pages tall here,
/// so one page reaches its end.
#[test]
fn track_presses_page_within_the_column() {
    let fixture = Fixture::new(Apply::NextFrame, &Effect::ALL);
    let mut harness = fixture.harness();
    let (fx, _) = fixture.effects_layout();
    let max_scroll = (fx.container.height - fx.viewport.height) as f32;
    let page = fx.viewport.height as f32;
    let x = fx.scroll_bar.x as f32 + 1.0;
    let below_thumb = Pos2::new(x, (fx.scroll_bar.y + fx.scroll_bar.height) as f32 - 2.0);
    let above_thumb = Pos2::new(x, fx.scroll_bar.y as f32 + 2.0);
    press(&mut harness, below_thumb);
    release(&mut harness, below_thumb);
    assert_eq!(
        fixture.interface.borrow().effects_scroll(),
        page.min(max_scroll)
    );
    press(&mut harness, above_thumb);
    release(&mut harness, above_thumb);
    assert_eq!(fixture.interface.borrow().effects_scroll(), 0.0);
    press(&mut harness, below_thumb);
    harness.run_steps(60);
    release(&mut harness, below_thumb);
    assert_eq!(fixture.interface.borrow().effects_scroll(), max_scroll);
}

/// Where the EQ's band `band` handle is, for the controls' current values.
fn eq_handle(fixture: &Fixture, band: &str) -> (Pos2, EqualizerLayout) {
    let (fx, top) = fixture.effects_layout();
    let bounds = fx.effects[Effect::Eq as usize].unwrap();
    let layout = EqualizerLayout::new(&top.within(Effect::Eq.skin()), bounds);
    let response = layout.response;
    let cutoff_info = registry().get(&format!("eq_{band}_cutoff")).unwrap();
    let gain_info = registry().get("eq_low_gain").unwrap();
    let buffer = (gain_info.max - gain_info.min) * 0.2;
    let (min_db, max_db) = (gain_info.min - buffer, gain_info.max + buffer);
    let cutoff = fixture.synth.value(&format!("eq_{band}_cutoff"));
    let gain = fixture.synth.value(&format!("eq_{band}_gain"));
    let pos = Pos2::new(
        response.x as f32
            + response.width as f32 * (cutoff - cutoff_info.min)
                / (cutoff_info.max - cutoff_info.min),
        response.y as f32 + response.height as f32 * (max_db - gain) / (max_db - min_db),
    );
    (pos, layout)
}

/// A click beside an EQ handle selects its band without moving it
/// (upstream's mouseDown selects; only mouseDrag moves), so the knobs then
/// turn that band.
#[test]
fn a_click_beside_an_eq_handle_selects_without_moving_it() {
    let fixture = Fixture::new(Apply::NextFrame, &[Effect::Eq]);
    let mut harness = fixture.harness();
    let (handle, layout) = eq_handle(&fixture, "band");
    let (cutoff, gain) = (
        fixture.synth.value("eq_band_cutoff"),
        fixture.synth.value("eq_band_gain"),
    );
    let beside = handle + egui::vec2(6.0, 0.0);
    press(&mut harness, beside);
    release(&mut harness, beside);
    assert_eq!(fixture.synth.value("eq_band_cutoff"), cutoff);
    assert_eq!(fixture.synth.value("eq_band_gain"), gain);
    assert_eq!(fixture.take_log(), Vec::<String>::new());

    let knob = centre(layout.cutoff);
    drag(&mut harness, knob, knob - egui::vec2(0.0, 20.0), 4);
    assert!(
        fixture.synth.value("eq_band_cutoff") > cutoff,
        "the band's knob"
    );
    let log = fixture.take_log();
    assert_eq!(gesture_count(&log, "eq_low_cutoff"), (0, 0), "{log:?}");
}

/// A drag that starts away from the EQ's handles moves nothing: the press
/// selects no band.
#[test]
fn a_drag_away_from_the_eq_handles_moves_nothing() {
    let fixture = Fixture::new(Apply::NextFrame, &[Effect::Eq]);
    let mut harness = fixture.harness();
    let (_, layout) = eq_handle(&fixture, "low");
    let response = layout.response;
    let empty = Pos2::new(
        (response.x + response.width) as f32 - 8.0,
        (response.y + response.height) as f32 - 8.0,
    );
    drag(&mut harness, empty, empty - egui::vec2(30.0, 30.0), 6);
    let log = fixture.take_log();
    assert!(log.iter().all(|line| !line.contains("eq_")), "{log:?}");
}

/// A Shift-drag from empty compressor space moves nothing (upstream's
/// mouseDrag returns first while nothing is hovered); from a threshold it
/// moves every threshold.
#[test]
fn a_shift_drag_needs_a_grabbed_compressor_point() {
    let fixture = Fixture::new(Apply::NextFrame, &[Effect::Compressor]);
    let mut harness = fixture.harness();
    let (fx, top) = fixture.effects_layout();
    let bounds = fx.effects[Effect::Compressor as usize].unwrap();
    let editor = CompressorLayout::new(&top.within(Effect::Compressor.skin()), bounds).editor;
    harness.event(Event::ModifiersChanged(Modifiers::SHIFT));
    // Between the low and middle bands, nothing is under the pointer.
    let gap = Pos2::new(
        editor.x as f32 + 0.34 * editor.width as f32,
        editor.y as f32 + 0.5 * editor.height as f32,
    );
    drag(&mut harness, gap, gap + egui::vec2(0.0, 20.0), 6);
    let log = fixture.take_log();
    assert!(
        log.iter().all(|line| !line.contains("compressor_")),
        "{log:?}"
    );

    let threshold = fixture.synth.value("compressor_low_upper_threshold");
    let band_threshold = fixture.synth.value("compressor_band_upper_threshold");
    let x = editor.x as f32 + 0.05 * editor.width as f32 + 0.4 / 3.0 * editor.width as f32;
    let t = (threshold - -80.0) / 80.0;
    let y_for_db = editor.height as f32 * (1.0 - (2.0 * t - 1.0)) * 0.5;
    let from = Pos2::new(x, editor.y as f32 + y_for_db.max(8.0 * top.size_ratio()));
    drag(&mut harness, from, from + egui::vec2(0.0, 20.0), 6);
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    assert!(fixture.synth.value("compressor_low_upper_threshold") < threshold);
    assert!(fixture.synth.value("compressor_band_upper_threshold") < band_threshold);
}

/// The compressor's low band, near the bottom: its lower ratio
/// (`DragPoint::LowerRatio`).
fn low_lower_ratio_point(fixture: &Fixture) -> Pos2 {
    let (fx, top) = fixture.effects_layout();
    let bounds = fx.effects[Effect::Compressor as usize].unwrap();
    let editor = CompressorLayout::new(&top.within(Effect::Compressor.skin()), bounds).editor;
    let x = editor.x as f32 + 0.05 * editor.width as f32 + 0.4 / 3.0 * editor.width as f32;
    Pos2::new(x, (editor.bottom() - 3) as f32)
}

/// JUCE's click count (`isLongPressOrDrag`): a second press that drags a
/// ratio 5 pixels is a drag, not a double click, so the ratio keeps its
/// dragged value rather than going back to 0 (egui's rules allowed 6).
#[test]
fn a_dragged_second_press_doesnt_reset_a_compressor_ratio() {
    let fixture = Fixture::new(Apply::NextFrame, &[Effect::Compressor]);
    let mut harness = fixture.harness();
    let point = low_lower_ratio_point(&fixture);
    let before = fixture.synth.value("compressor_low_lower_ratio");
    harness.event(Event::PointerMoved(point));
    harness.step();
    press(&mut harness, point);
    release(&mut harness, point);
    drag(&mut harness, point, point + egui::vec2(0.0, -5.0), 2);
    let after = fixture.synth.value("compressor_low_lower_ratio");
    assert_ne!(after, before, "the drag moved nothing");
    assert_ne!(after, 0.0, "the drag was taken for a double click");
}

/// Two clicks 0.35 s apart are a double click by JUCE's rules (presses
/// within 0.4 s), which resets the ratio (`mouseDoubleClick`); egui's
/// 0.3 s between releases missed them.
#[test]
fn a_double_click_paced_like_juces_resets_a_compressor_ratio() {
    let fixture = Fixture::new(Apply::NextFrame, &[Effect::Compressor]);
    let mut harness = fixture.harness();
    let point = low_lower_ratio_point(&fixture);
    assert_ne!(fixture.synth.value("compressor_low_lower_ratio"), 0.0);
    harness.event(Event::PointerMoved(point));
    harness.step();
    press(&mut harness, point);
    release(&mut harness, point);
    // 21 frames at 60 Hz from press to press.
    for _ in 0..18 {
        harness.step();
    }
    press(&mut harness, point);
    release(&mut harness, point);
    assert_eq!(fixture.synth.value("compressor_low_lower_ratio"), 0.0);
}
