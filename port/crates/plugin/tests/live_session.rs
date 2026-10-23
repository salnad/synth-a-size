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
    let (a, b) = (
        live.session().snapshot().unwrap(),
        offline.session().snapshot().unwrap(),
    );
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

#[test]
fn closing_the_bridge_wakes_a_waiting_call_and_refuses_later_ones() {
    let rig = rig();
    let bridge = ChatBridge::new();
    let mut remote = bridge.remote(Duration::from_secs(10));
    let worker = std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let result = remote.apply(&sas_llm_control::Edit::SetControl {
            name: "volume",
            value: 0.0,
        });
        (result.unwrap_err().to_string(), started.elapsed(), remote)
    });
    std::thread::sleep(Duration::from_millis(50));
    bridge.close();
    let (error, waited, remote) = worker.join().unwrap();
    assert!(error.contains("closed"), "{error}");
    assert!(waited < Duration::from_secs(1), "{waited:?}");
    let started = std::time::Instant::now();
    assert!(remote.snapshot().is_err());
    assert!(started.elapsed() < Duration::from_millis(100));
    assert_eq!(bridge.run_pending(&mut rig.session()), 0);
    assert!((rig.value("volume") - 5473.0404).abs() < 1e-3);
}

/// A session on the GUI side whose edits take a while, or report busy a
/// few times first, around a live session.
struct Slow<'a> {
    inner: &'a mut LiveSession<TestHost>,
    delay: Duration,
    busy: usize,
    applied: usize,
}

impl SynthSession for Slow<'_> {
    fn snapshot(&self) -> Result<sas_preset::PresetState, sas_llm_control::Error> {
        self.inner.snapshot()
    }
    fn control(&self, name: &str) -> Option<f32> {
        self.inner.control(name)
    }
    fn apply(
        &mut self,
        edit: &sas_llm_control::Edit,
    ) -> Result<sas_llm_control::Applied, sas_llm_control::Error> {
        if self.busy > 0 {
            self.busy -= 1;
            return Err(sas_llm_control::Error::Busy("queue full".into()));
        }
        std::thread::sleep(self.delay);
        self.applied += 1;
        self.inner.apply(edit)
    }
    fn restore(&mut self, state: &sas_preset::PresetState) -> Result<(), sas_llm_control::Error> {
        self.inner.restore(state)
    }
}

#[test]
fn a_call_that_starts_before_the_timeout_is_awaited_not_abandoned() {
    let rig = rig();
    let bridge = ChatBridge::new();
    let mut remote = bridge.remote(Duration::from_millis(200));
    let worker = std::thread::spawn(move || {
        remote.apply(&sas_llm_control::Edit::SetControl {
            name: "osc_1_transpose",
            value: 5.0,
        })
    });
    // The GUI thread picks the call up at 50 ms; the edit takes 300 ms, past
    // the worker's 200 ms timeout.
    std::thread::sleep(Duration::from_millis(50));
    let mut live = rig.session();
    let mut slow = Slow {
        inner: &mut live,
        delay: Duration::from_millis(300),
        busy: 0,
        applied: 0,
    };
    assert_eq!(bridge.run_pending(&mut slow), 1);
    assert_eq!(slow.applied, 1);
    // The worker waited for it and reports what happened.
    assert!(worker.join().unwrap().is_ok());
    assert_eq!(rig.value("osc_1_transpose"), 5.0);
}

#[test]
fn busy_edits_are_retried_on_the_worker() {
    let rig = rig();
    let bridge = ChatBridge::new();
    let mut remote = bridge.remote(Duration::from_secs(5));
    let worker = std::thread::spawn(move || {
        remote.apply(&sas_llm_control::Edit::SetControl {
            name: "osc_1_transpose",
            value: 2.0,
        })
    });
    let mut live = rig.session();
    let mut slow = Slow {
        inner: &mut live,
        delay: Duration::ZERO,
        busy: 3,
        applied: 0,
    };
    while !worker.is_finished() {
        bridge.run_pending(&mut slow);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(worker.join().unwrap().is_ok());
    assert_eq!(slow.applied, 1);
    assert_eq!(rig.value("osc_1_transpose"), 2.0);
}

#[test]
fn an_unreadable_patch_blocks_edits_instead_of_saving_a_stale_undo_step() {
    let bridge = ChatBridge::new();
    let remote = bridge.remote(Duration::from_millis(20));
    let mut controller = Controller::new(remote, Policy::default());
    controller.begin_turn();
    let output = controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 1}]}),
    );
    assert!(output.is_error, "{}", output.content);
    assert_eq!(controller.undo_depth(), 0);
    let output = controller.call("get_patch", &json!({}));
    assert!(
        output.is_error && output.content.contains("cannot read the patch"),
        "{}",
        output.content
    );
}

#[test]
fn naming_the_patch_changes_the_save_info_without_a_reload() {
    let rig = rig();
    let mut controller = rig.controller();
    let generation = rig.params.edit_handle().generation();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "filter_1_on", "value": "On"}]}),
    ));
    ok(controller.call(
        "set_preset_info",
        &json!({"name": "Glass Pad", "author": "Chat", "macro_names": ["Bloom"]}),
    ));
    let state = rig.params.preset_state();
    assert_eq!(state.info.preset_name, "Glass Pad");
    assert_eq!(state.info.author, "Chat");
    assert_eq!(state.info.macros[0], "Bloom");
    // No reload: the generation is unchanged, so the undo history and the
    // outside-change tracking stay.
    assert_eq!(controller.session().writer().reloads.get(), 0);
    assert_eq!(rig.params.edit_handle().generation(), generation);
    assert!(!controller.session_mut().take_replaced());
    // The host's saved state carries the new name.
    let saved = serde_json::to_string(&rig.params.capture_state().fields).unwrap();
    assert!(saved.contains("Glass Pad"), "{saved}");

    // Undoing the turn restores the old name and the control, still without
    // a reload.
    ok(controller.call("undo", &json!({})));
    let state = rig.params.preset_state();
    assert_eq!(state.info.preset_name, "");
    assert_eq!(rig.value("filter_1_on"), 0.0);
    assert_eq!(controller.session().writer().reloads.get(), 0);
}

#[test]
fn an_undo_whose_batch_is_refused_leaves_the_name_and_shapes_alone() {
    let mut rig = rig();
    let mut controller = rig.controller();
    // One turn: two LFO shapes and a name.
    ok(controller.call("set_lfo_shape", &json!({"lfo": 1, "shape": "saw_down"})));
    ok(controller.call("set_lfo_shape", &json!({"lfo": 2, "shape": "square"})));
    ok(controller.call("set_preset_info", &json!({"name": "Shaped"})));
    rig.block();
    let shaped = rig.params.preset_state();

    // Fill the queue with no-op curve edits (slot 1's own mapping) until a
    // single edit is refused: fewer than the undo batch's two entries remain.
    let edits = rig.params.edit_handle().clone();
    let mapping = shaped.modulations()[0].line_mapping.clone();
    while edits.set_line_mapping(0, mapping.clone()).is_ok() {}

    let output = controller.call("undo", &json!({}));
    assert!(output.is_error, "{}", output.content);
    let state = rig.params.preset_state();
    assert_eq!(
        state.info.preset_name, "Shaped",
        "the name did not move alone"
    );
    assert_eq!(state.lfos[0].name(), shaped.lfos[0].name());
    assert_eq!(state.lfos[1].name(), shaped.lfos[1].name());
    assert_eq!(controller.session().writer().reloads.get(), 0);

    // With room again, the same undo restores both together.
    rig.block();
    ok(controller.call("undo", &json!({})));
    let state = rig.params.preset_state();
    assert_eq!(state.info.preset_name, "");
    assert_ne!(state.lfos[0].name(), shaped.lfos[0].name());
    assert_eq!(controller.session().writer().reloads.get(), 0);
}
