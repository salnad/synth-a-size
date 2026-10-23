//! The tools end to end on an offline session.

use serde_json::{json, Value};

use sas_llm_control::{specs, Controller, OfflineSession, Policy, SynthSession, ToolOutput};

fn controller() -> Controller<OfflineSession> {
    Controller::new(OfflineSession::init(), Policy::default())
}

fn ok(output: ToolOutput) -> Value {
    assert!(!output.is_error, "tool failed: {}", output.content);
    serde_json::from_str(&output.content).unwrap_or(Value::String(output.content))
}

fn err(output: ToolOutput) -> String {
    assert!(
        output.is_error,
        "tool should have failed: {}",
        output.content
    );
    output.content
}

#[test]
fn every_tool_has_an_object_schema_and_a_handler() {
    let mut controller = controller();
    let mut names = Vec::new();
    for spec in specs() {
        assert_eq!(spec.input_schema["type"], "object", "{}", spec.name);
        assert!(spec.input_schema["properties"].is_object(), "{}", spec.name);
        assert!(!spec.description.is_empty());
        let output = controller.call(spec.name, &json!({}));
        assert!(
            !output.content.starts_with("unknown tool"),
            "{} has no handler",
            spec.name
        );
        names.push(spec.name);
    }
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len());
    assert!(err(controller.call("no_such_tool", &json!({}))).contains("set_params"));
}

#[test]
fn set_params_reads_display_units() {
    let mut controller = controller();
    let reply = ok(controller.call(
        "set_params",
        &json!({"changes": [
            {"name": "volume", "value": -12},
            {"name": "env_1_attack", "value": "0.5 secs"},
            {"name": "delay_style", "value": "ping pong"},
            {"name": "osc_1_unison_voices", "value": 4},
        ]}),
    ));
    assert_eq!(reply["set"][0]["value"], "-12dB");
    assert_eq!(reply["set"][1]["value"], "0.5 secs");
    assert_eq!(reply["set"][2]["value"], "Ping Pong");
    let state = controller.session().state();
    assert!((state.value("volume").unwrap() - 68.0 * 68.0).abs() < 1e-2);
    assert_eq!(state.value("delay_style"), Some(2.0));
    assert_eq!(state.value("osc_1_unison_voices"), Some(4.0));
}

#[test]
fn set_params_clamps_and_reports_it() {
    let mut controller = controller();
    let reply = ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_unison_voices", "value": 99}]}),
    ));
    assert_eq!(reply["set"][0]["clamped"], true);
    assert_eq!(
        controller.session().control("osc_1_unison_voices"),
        Some(16.0)
    );
}

#[test]
fn a_bad_batch_changes_nothing_and_suggests_names() {
    let mut controller = controller();
    let before = controller.session().control("volume");
    let message = err(controller.call(
        "set_params",
        &json!({"changes": [
            {"name": "volume", "value": -20},
            {"name": "filter1_cutoff", "value": 10},
        ]}),
    ));
    assert!(message.contains("nothing was changed"), "{message}");
    assert!(message.contains("filter_1_cutoff"), "{message}");
    assert_eq!(controller.session().control("volume"), before);
    assert_eq!(controller.undo_depth(), 0);
}

#[test]
fn raising_the_volume_past_the_threshold_waits_for_the_user() {
    let mut controller = controller();
    let reply = ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "volume", "value": 3}]}),
    ));
    assert_eq!(reply["status"], "needs_confirmation");
    assert!(controller.pending().is_some());
    assert!((controller.session().control("volume").unwrap() - 5473.0404).abs() < 1e-3);
    assert!(controller.confirm_pending().unwrap());
    let volume = controller.session().control("volume").unwrap();
    assert!((volume.sqrt() - 83.0).abs() < 1e-3, "{volume}");

    // Lowering it never asks, nor does a rejected batch change anything.
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "volume", "value": 1}]}),
    ));
    assert!(controller.pending().is_none());
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "volume", "value": 5}]}),
    ));
    assert!(controller.reject_pending());
    assert!((controller.session().control("volume").unwrap().sqrt() - 81.0).abs() < 1e-3);
}

#[test]
fn the_threshold_is_configurable() {
    let policy = Policy {
        confirm_above: Vec::new(),
        ..Policy::default()
    };
    let mut controller = Controller::new(OfflineSession::init(), policy);
    let reply = ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "volume", "value": 6}]}),
    ));
    assert!(reply.get("set").is_some());
}

#[test]
fn each_turn_is_one_undo_step() {
    let mut controller = controller();
    controller.begin_turn();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 12}]}),
    ));
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_level", "value": 0.5}]}),
    ));
    controller.begin_turn();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": -12}]}),
    ));
    assert_eq!(controller.undo_depth(), 2);

    assert_eq!(ok(controller.call("undo", &json!({})))["undone"], 1);
    assert_eq!(controller.session().control("osc_1_transpose"), Some(12.0));
    // Display 0.5 on the quadratic level scale is engine value sqrt(0.5).
    let level = controller.session().control("osc_1_level").unwrap();
    assert!((level - 0.5f32.sqrt()).abs() < 1e-6, "{level}");

    assert_eq!(
        ok(controller.call("undo", &json!({"steps": 5})))["undone"],
        1
    );
    assert_eq!(controller.session().control("osc_1_transpose"), Some(0.0));
    assert_eq!(ok(controller.call("undo", &json!({})))["undone"], 0);
}

#[test]
fn undo_returns_to_a_checkpoint() {
    let mut controller = controller();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 7}]}),
    ));
    ok(controller.call("checkpoint", &json!({"label": "fifth"})));
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 3}]}),
    ));
    controller.begin_turn();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 5}]}),
    ));
    ok(controller.call("undo", &json!({"checkpoint": "fifth"})));
    assert_eq!(controller.session().control("osc_1_transpose"), Some(7.0));
    assert!(err(controller.call("undo", &json!({"checkpoint": "nope"}))).contains("nope"));
}

#[test]
fn undo_history_is_bounded() {
    let policy = Policy {
        max_undo: 3,
        ..Policy::default()
    };
    let mut controller = Controller::new(OfflineSession::init(), policy);
    for step in 1..=5 {
        controller.begin_turn();
        ok(controller.call(
            "set_params",
            &json!({"changes": [{"name": "osc_1_transpose", "value": step}]}),
        ));
    }
    assert_eq!(controller.undo_depth(), 3);
    controller.undo(3, None).unwrap();
    assert_eq!(controller.session().control("osc_1_transpose"), Some(2.0));
}

#[test]
fn reads_report_display_values() {
    let mut controller = controller();
    let values = ok(controller.call("get_params", &json!({"names": ["volume", "delay_style"]})));
    assert_eq!(values["volume"], "-6.02dB");
    assert!(err(controller.call("get_params", &json!({"names": ["volum"]}))).contains("volume"));

    let found = ok(controller.call("find_params", &json!({"query": "reverb size"})));
    assert_eq!(found[0]["name"], "reverb_size");
    assert!(found[0]["range"].as_str().unwrap().contains(".."));

    let patch = ok(controller.call("get_patch", &json!({})));
    assert!(patch.as_str().unwrap().contains("Modulations: none"));
}

#[test]
fn malformed_arguments_are_errors() {
    let mut controller = controller();
    assert!(
        err(controller.call("set_params", &json!({"changes": "loud"})))
            .contains("invalid arguments for set_params")
    );
    assert!(err(controller.call(
        "set_params",
        &json!({"changes": [{"name": "volume", "value": [1]}]})
    ))
    .contains("cannot read"));
}

#[test]
fn compare_switches_between_before_and_after_the_last_turn() {
    use sas_llm_control::Side;
    let mut controller = controller();
    assert_eq!(controller.compare_toggle().unwrap(), None);
    controller.begin_turn();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 12}]}),
    ));

    assert_eq!(controller.compare_toggle().unwrap(), Some(Side::Before));
    assert_eq!(controller.session().control("osc_1_transpose"), Some(0.0));
    assert_eq!(controller.compare_toggle().unwrap(), Some(Side::After));
    assert_eq!(controller.session().control("osc_1_transpose"), Some(12.0));
    assert_eq!(controller.compare_toggle().unwrap(), Some(Side::Before));

    // Undo while comparing undoes the turn, from either side.
    assert_eq!(controller.undo(1, None).unwrap(), 1);
    assert_eq!(controller.comparing(), None);
    assert_eq!(controller.session().control("osc_1_transpose"), Some(0.0));
}

#[test]
fn an_edit_ends_the_comparison_on_the_side_playing() {
    let mut controller = controller();
    controller.begin_turn();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 12}]}),
    ));
    controller.compare_toggle().unwrap();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_level", "value": 0.25}]}),
    ));
    assert_eq!(controller.comparing(), None);
    // The edit applied to the "before" version.
    assert_eq!(controller.session().control("osc_1_transpose"), Some(0.0));
}

/// An offline session whose state another party replaces.
struct Replaceable {
    inner: OfflineSession,
    replaced: bool,
}

impl SynthSession for Replaceable {
    fn snapshot(&self) -> Result<sas_llm_control::sas_preset::PresetState, sas_llm_control::Error> {
        self.inner.snapshot()
    }
    fn control(&self, name: &str) -> Option<f32> {
        self.inner.control(name)
    }
    fn apply(
        &mut self,
        edit: &sas_llm_control::Edit,
    ) -> Result<sas_llm_control::Applied, sas_llm_control::Error> {
        self.inner.apply(edit)
    }
    fn restore(
        &mut self,
        state: &sas_llm_control::sas_preset::PresetState,
    ) -> Result<(), sas_llm_control::Error> {
        self.inner.restore(state)
    }
    fn take_replaced(&mut self) -> bool {
        std::mem::take(&mut self.replaced)
    }
}

#[test]
fn a_state_replaced_outside_the_chat_drops_the_undo_history() {
    let session = Replaceable {
        inner: OfflineSession::init(),
        replaced: false,
    };
    let mut controller = Controller::new(session, Policy::default());
    controller.begin_turn();
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 12}]}),
    ));
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "volume", "value": 3}]}),
    ));
    assert!(controller.pending().is_some());
    assert_eq!(controller.undo_depth(), 1);

    controller.session_mut().replaced = true;
    controller.begin_turn();
    assert_eq!(controller.undo_depth(), 0);
    assert!(controller.pending().is_none());
    assert_eq!(ok(controller.call("undo", &json!({})))["undone"], 0);
}
