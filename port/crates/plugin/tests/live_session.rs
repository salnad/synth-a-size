//! The LLM tools on the running plugin: `LiveSession` over the host
//! parameters and the edit bus, and `RemoteSession` over the chat bridge.

use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

use nice_plug::params::InternalParamMut;
use nice_plug::plugin::PluginState;
use serde_json::{json, Value};

use sas_engine::document::PresetDocument;
use sas_engine::edit_bus::{EditBus, EditConsumer};
use sas_llm_control::{
    Controller, OfflineSession, Policy, PresetLibrary, Side, SynthSession, ToolOutput,
};
use sas_plugin::chat_bridge::ChatBridge;
use sas_plugin::live::{HostWriter, LiveSession};
use sas_plugin::params::{Control, ControlParam, SynthParams};
use sas_preset::InitContent;

/// Sets parameters and restores states as nice-plug's wrapper does without
/// a host, counting what the session asked for.
struct TestHost {
    params: Arc<SynthParams>,
    gestures: Cell<usize>,
    reloads: Cell<usize>,
}

fn set_plain(control: &Control, value: f32) {
    // SAFETY: the test is the only one setting parameters.
    unsafe {
        match &control.param {
            ControlParam::Float(param) => {
                param._internal_set_plain_value(value);
            }
            ControlParam::Stepped(param) => {
                param._internal_set_plain_value(value.round() as i32);
            }
            ControlParam::Bypass(param) => {
                param._internal_set_plain_value(value != 0.0);
            }
        }
    }
}

impl HostWriter for TestHost {
    fn set_control(&self, control: &Control, value: f32) {
        self.gestures.set(self.gestures.get() + 1);
        set_plain(control, value);
    }

    fn set_state(&self, state: PluginState) {
        self.reloads.set(self.reloads.get() + 1);
        self.params.restore_state(state);
    }
}

struct Rig {
    params: Arc<SynthParams>,
    consumer: EditConsumer,
}

fn rig() -> Rig {
    let (edits, consumer) = EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
    Rig {
        params: Arc::new(SynthParams::new(edits)),
        consumer,
    }
}

impl Rig {
    fn session(&self) -> LiveSession<TestHost> {
        LiveSession::new(
            self.params.clone(),
            TestHost {
                params: self.params.clone(),
                gestures: Cell::new(0),
                reloads: Cell::new(0),
            },
        )
    }

    fn controller(&self) -> Controller<LiveSession<TestHost>> {
        Controller::new(self.session(), Policy::default())
    }

    fn value(&self, name: &str) -> f32 {
        self.params.control(name).unwrap().param.saved_value()
    }

    /// What the audio thread does at a block: drain the bus.
    fn block(&mut self) {
        self.consumer.drain(&mut ());
    }
}

fn ok(output: ToolOutput) -> Value {
    assert!(!output.is_error, "tool failed: {}", output.content);
    serde_json::from_str(&output.content).unwrap_or(Value::String(output.content))
}

#[test]
fn controls_change_through_host_gestures_and_routes_through_the_bus() {
    let mut rig = rig();
    let mut controller = rig.controller();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "filter_1_on", "value": "On"}, {"name": "filter_1_cutoff", "value": 12}]}),
    ));
    assert_eq!(rig.value("filter_1_on"), 1.0);
    assert_eq!(rig.value("filter_1_cutoff"), 72.0);

    let reply = ok(controller.call(
        "connect_modulation",
        &json!({"source": "lfo_1", "destination": "filter_1_cutoff", "amount": 0.3, "bipolar": true}),
    ));
    assert_eq!(reply["slot"], 1);
    rig.block();
    let state = rig.params.preset_state();
    assert_eq!(state.modulations()[0].source, "lfo_1");
    assert_eq!(state.modulations()[0].destination, "filter_1_cutoff");
    // The created slot's final values went to the host parameters too.
    assert!((rig.value("modulation_1_amount") - 0.3).abs() < 1e-6);
    assert_eq!(rig.value("modulation_1_bipolar"), 1.0);
    assert_eq!(rig.value("modulation_1_stereo"), 0.0);
    assert_eq!(rig.value("modulation_1_bypass"), 0.0);
    // Two control gestures, then the connection's five parameter changes.
    assert_eq!(controller.session().writer().gestures.get(), 7);

    ok(controller.call("set_lfo_shape", &json!({"lfo": 3, "shape": "saw_down"})));
    ok(controller.call(
        "set_wavetable",
        &json!({"oscillator": 2, "name": "Bus", "frames": [{"position": 0, "shape": "square"}]}),
    ));
    rig.block();
    let state = rig.params.preset_state();
    assert_eq!(state.lfos[2].name(), "Saw Down");
    assert_eq!(state.wavetables[1].name, "Bus");
    assert_eq!(controller.session().writer().reloads.get(), 0);
}

#[test]
fn the_live_session_matches_the_offline_one() {
    let rig = rig();
    let mut live = rig.controller();
    let mut offline = Controller::new(OfflineSession::init(), Policy::default());
    let calls = [
        (
            "set_params",
            json!({"changes": [{"name": "env_1_attack", "value": "0.4 secs"}, {"name": "osc_1_unison_voices", "value": 3}]}),
        ),
        (
            "connect_modulation",
            json!({"source": "env_2", "destination": "osc_1_wave_frame"}),
        ),
        (
            "connect_modulation",
            json!({"source": "macro_control_1", "destination": "reverb_dry_wet", "amount": 0.5, "stereo": true}),
        ),
        (
            "set_lfo_shape",
            json!({"lfo": 1, "points": [[0, 0], [0.5, 1], [1, 0]]}),
        ),
    ];
    for (tool, args) in &calls {
        assert_eq!(
            ok(live.call(tool, args)),
            ok(offline.call(tool, args)),
            "{tool}"
        );
    }
    let (a, b) = (live.session().snapshot(), offline.session().snapshot());
    assert_eq!(a.controls(), b.controls());
    for (x, y) in a.modulations().iter().zip(b.modulations()) {
        assert_eq!((&x.source, &x.destination), (&y.source, &y.destination));
    }
    assert_eq!(a.lfos[0].to_json().dump(), b.lfos[0].to_json().dump());
}

#[test]
fn undo_returns_controls_and_shapes_without_a_reload() {
    let mut rig = rig();
    let mut controller = rig.controller();
    controller.begin_turn();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 7}]}),
    ));
    ok(controller.call("set_lfo_shape", &json!({"lfo": 1, "shape": "square"})));
    rig.block();
    let generation = rig.params.edit_handle().generation();

    assert_eq!(controller.undo(1, None).unwrap(), 1);
    rig.block();
    assert_eq!(rig.value("osc_1_transpose"), 0.0);
    assert_eq!(rig.params.preset_state().lfos[0].name(), "Triangle");
    assert_eq!(rig.params.edit_handle().generation(), generation);
    assert_eq!(controller.session().writer().reloads.get(), 0);
}

#[test]
fn undo_of_a_route_reloads_the_state_and_keeps_the_history() {
    let mut rig = rig();
    let mut controller = rig.controller();
    controller.begin_turn();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 5}]}),
    ));
    controller.begin_turn();
    ok(controller.call(
        "connect_modulation",
        &json!({"source": "lfo_2", "destination": "osc_1_level"}),
    ));
    rig.block();

    assert_eq!(controller.undo(1, None).unwrap(), 1);
    assert_eq!(controller.session().writer().reloads.get(), 1);
    assert!(rig.params.preset_state().modulations()[0].is_free());
    assert_eq!(rig.value("osc_1_transpose"), 5.0);
    // The reload was the session's own: the earlier step is still there.
    assert!(!controller.check_replaced());
    assert_eq!(controller.undo_depth(), 1);
    assert_eq!(controller.undo(1, None).unwrap(), 1);
    assert_eq!(rig.value("osc_1_transpose"), 0.0);
}

#[test]
fn compare_switches_live() {
    let rig = rig();
    let mut controller = rig.controller();
    controller.begin_turn();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_level", "value": 0.25}]}),
    ));
    let after = rig.value("osc_1_level");
    assert_eq!(controller.compare_toggle().unwrap(), Some(Side::Before));
    // The init level, 1/sqrt(2).
    assert!((rig.value("osc_1_level") - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    assert_eq!(controller.compare_toggle().unwrap(), Some(Side::After));
    assert_eq!(rig.value("osc_1_level"), after);
}

#[test]
fn outside_changes_are_reported_once() {
    let rig = rig();
    let mut controller = rig.controller();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "filter_1_cutoff", "value": 12}]}),
    ));
    assert!(controller.session_mut().take_outside_changes().is_empty());
    // Host automation moves the cutoff.
    set_plain(rig.params.control("filter_1_cutoff").unwrap(), 40.0);
    let changes = controller.session_mut().take_outside_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        (changes[0].name, changes[0].set, changes[0].now),
        ("filter_1_cutoff", 72.0, 40.0)
    );
    assert!(controller.session_mut().take_outside_changes().is_empty());
}

#[test]
fn a_host_restore_drops_the_undo_history() {
    let rig = rig();
    let mut controller = rig.controller();
    controller.begin_turn();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 3}]}),
    ));
    assert_eq!(controller.undo_depth(), 1);
    // The host restores a project state.
    let state = rig.params.capture_state();
    rig.params.restore_state(state);
    controller.begin_turn();
    assert_eq!(controller.undo_depth(), 0);
}

#[test]
fn presets_load_through_the_host() {
    let rig = rig();
    let folder = std::env::temp_dir().join(format!("sas-live-presets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut saved = OfflineSession::init().into_state();
    saved.set_value("osc_1_transpose", 12.0).unwrap();
    PresetLibrary::new(&folder)
        .save("Up", &saved, false)
        .unwrap();

    let mut controller = rig.controller().with_library(PresetLibrary::new(&folder));
    controller.begin_turn();
    ok(controller.call("load_preset", &json!({"name": "Up"})));
    assert_eq!(controller.session().writer().reloads.get(), 1);
    assert_eq!(rig.value("osc_1_transpose"), 12.0);
    assert!(!controller.check_replaced());
    controller.undo(1, None).unwrap();
    assert_eq!(rig.value("osc_1_transpose"), 0.0);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_worker_drives_the_session_through_the_bridge() {
    let rig = rig();
    let bridge = ChatBridge::new();
    let remote = bridge.remote(Duration::from_secs(10));
    let worker = std::thread::spawn(move || {
        let mut controller = Controller::new(remote, Policy::default());
        controller.begin_turn();
        let set = controller.call(
            "set_params",
            &json!({"changes": [{"name": "osc_1_transpose", "value": -12}]}),
        );
        let read = controller.call("get_params", &json!({"names": ["osc_1_transpose"]}));
        (set, read)
    });
    let mut session = rig.session();
    while !worker.is_finished() {
        bridge.run_pending(&mut session);
        std::thread::sleep(Duration::from_millis(1));
    }
    let (set, read) = worker.join().unwrap();
    ok(set);
    assert_eq!(ok(read)["osc_1_transpose"], "-12");
    assert_eq!(rig.value("osc_1_transpose"), -12.0);
}

#[test]
fn calls_nobody_runs_time_out_and_are_dropped() {
    let rig = rig();
    let bridge = ChatBridge::new();
    let mut remote = bridge.remote(Duration::from_millis(30));
    let error = remote
        .apply(&sas_llm_control::Edit::SetControl {
            name: "volume",
            value: 0.0,
        })
        .unwrap_err();
    assert!(error.to_string().contains("window"), "{error}");
    std::thread::sleep(Duration::from_millis(10));
    // The editor opens later: the stale call does not run.
    assert_eq!(bridge.run_pending(&mut rig.session()), 0);
    assert!((rig.value("volume") - 5473.0404).abs() < 1e-3);
}
