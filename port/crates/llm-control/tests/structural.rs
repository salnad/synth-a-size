//! Modulation, LFO, wavetable and preset tools on an offline session.

use std::path::PathBuf;

use serde_json::{json, Value};

use sas_llm_control::{
    Controller, OfflineSession, Policy, PresetLibrary, SynthSession, ToolOutput,
};

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

/// A fresh, empty folder under the system temp dir.
fn temp_folder(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("sas-llm-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

#[test]
fn connect_sets_the_slot_like_the_gui() {
    let mut controller = controller();
    let reply = ok(controller.call(
        "connect_modulation",
        &json!({"source": "lfo_1", "destination": "filter_1_cutoff", "amount": 0.4, "stereo": true}),
    ));
    assert_eq!(reply["slot"], 1);
    assert_eq!(reply["created"], true);
    let state = controller.session().state();
    assert_eq!(state.modulations()[0].source, "lfo_1");
    assert!((state.value("modulation_1_amount").unwrap() - 0.4).abs() < 1e-6);
    assert_eq!(state.value("modulation_1_stereo"), Some(1.0));
    assert_eq!(state.value("modulation_1_power"), Some(0.0));
    // LFOs are bipolar by default (`isSourceDefaultBipolar`).
    assert_eq!(state.value("modulation_1_bipolar"), Some(1.0));
    assert!(state.modulations()[0].line_mapping.is_linear());

    // Connecting the same pair again reuses the slot.
    let again = ok(controller.call(
        "connect_modulation",
        &json!({"source": "lfo_1", "destination": "filter_1_cutoff"}),
    ));
    assert_eq!(again["slot"], 1);
    assert_eq!(again["created"], false);
}

#[test]
fn update_and_disconnect_routes() {
    let mut controller = controller();
    ok(controller.call(
        "connect_modulation",
        &json!({"source": "env_2", "destination": "osc_1_wave_frame"}),
    ));
    let reply = ok(controller.call(
        "update_modulation",
        &json!({"source": "env_2", "destination": "osc_1_wave_frame", "amount": -0.25, "bipolar": true}),
    ));
    assert_eq!(reply["slot"], 1);
    let state = controller.session().state();
    assert!((state.value("modulation_1_amount").unwrap() + 0.25).abs() < 1e-6);
    assert_eq!(state.value("modulation_1_bipolar"), Some(1.0));

    ok(controller.call(
        "disconnect_modulation",
        &json!({"source": "env_2", "destination": "osc_1_wave_frame"}),
    ));
    assert!(controller.session().state().modulations()[0].is_free());
    assert!(err(controller.call(
        "update_modulation",
        &json!({"source": "env_2", "destination": "osc_1_wave_frame", "amount": 0.1}),
    ))
    .contains("not connected"));
    assert!(err(controller.call(
        "disconnect_modulation",
        &json!({"source": "env_2", "destination": "osc_1_wave_frame"}),
    ))
    .contains("not connected"));
}

#[test]
fn bad_routes_are_explained() {
    let mut controller = controller();
    let message = err(controller.call(
        "connect_modulation",
        &json!({"source": "lfo1", "destination": "filter_1_cutoff"}),
    ));
    assert!(message.contains("lfo_1"), "{message}");
    let message = err(controller.call(
        "connect_modulation",
        &json!({"source": "lfo_1", "destination": "filter_1_cutof"}),
    ));
    assert!(message.contains("filter_1_cutoff"), "{message}");
    assert_eq!(controller.undo_depth(), 0);
}

#[test]
fn lfo_shapes_by_name_and_points() {
    let mut controller = controller();
    ok(controller.call("set_lfo_shape", &json!({"lfo": 2, "shape": "saw_down"})));
    assert_eq!(controller.session().state().lfos[1].name(), "Saw Down");

    let reply = ok(controller.call(
        "set_lfo_shape",
        &json!({"lfo": 3, "points": [[0, 0], [0.1, 1], [1, 0]], "powers": [0, -4, 0], "name": "Pluck"}),
    ));
    assert_eq!(reply["points"], 3);
    let lfo = &controller.session().state().lfos[2];
    assert_eq!(lfo.name(), "Pluck");
    assert_eq!(lfo.point(1), (0.1, 0.0));
    assert!(ok(controller.call("get_patch", &json!({})))
        .as_str()
        .unwrap()
        .contains("lfo_3 shape \"Pluck\""));

    assert!(
        err(controller.call("set_lfo_shape", &json!({"lfo": 9, "shape": "sine"}))).contains("1..8")
    );
    assert!(err(controller.call("set_lfo_shape", &json!({"lfo": 1}))).contains("either"));
}

#[test]
fn wavetables_are_generated() {
    let mut controller = controller();
    let reply = ok(controller.call(
        "set_wavetable",
        &json!({"oscillator": 2, "name": "Organ", "frames": [
            {"position": 0, "shape": "sine"},
            {"position": 128, "harmonics": [1, 0, 0.5, 0, 0.25]},
            {"position": 256, "shape": "saw"},
        ]}),
    ));
    assert_eq!(reply["keyframes"], 3);
    assert_eq!(reply["last_frame"], 256);
    assert_eq!(controller.session().state().wavetables[1].name, "Organ");
    assert!(ok(controller.call("get_patch", &json!({})))
        .as_str()
        .unwrap()
        .contains("osc_2 wavetable \"Organ\""));
    assert!(err(controller.call(
        "set_wavetable",
        &json!({"oscillator": 1, "frames": [{"position": 0}]}),
    ))
    .contains("shape or harmonics"));
}

#[test]
fn presets_save_list_and_load_in_the_folder_only() {
    let folder = temp_folder("presets");
    let mut controller = controller().with_library(PresetLibrary::new(&folder));
    ok(controller.call(
        "set_params",
        &json!({"changes": [{"name": "osc_1_transpose", "value": 12}]}),
    ));
    ok(controller.call("save_preset", &json!({"name": "High Saw", "style": "Lead"})));
    assert!(folder.join("High Saw.vital").is_file());
    assert!(err(controller.call("save_preset", &json!({"name": "High Saw"}))).contains("exists"));
    ok(controller.call(
        "save_preset",
        &json!({"name": "High Saw", "overwrite": true}),
    ));
    assert!(err(controller.call("save_preset", &json!({"name": "../escape"}))).contains("invalid"));

    ok(controller.call("init_patch", &json!({})));
    assert_eq!(controller.session().control("osc_1_transpose"), Some(0.0));
    assert_eq!(
        ok(controller.call("list_presets", &json!({"query": "saw"})))["presets"],
        json!(["High Saw"])
    );
    ok(controller.call("load_preset", &json!({"name": "High Saw"})));
    assert_eq!(controller.session().control("osc_1_transpose"), Some(12.0));
    assert_eq!(controller.session().state().info.preset_name, "High Saw");
    assert!(err(controller.call("load_preset", &json!({"name": "Nope"}))).contains("list_presets"));

    // Loads and init are undoable.
    controller.undo(1, None).unwrap();
    assert_eq!(controller.session().control("osc_1_transpose"), Some(0.0));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn preset_tools_need_a_folder() {
    let mut controller = controller();
    assert!(err(controller.call("list_presets", &json!({}))).contains("no preset folder"));
}

#[test]
fn loading_a_loud_preset_waits_for_the_user() {
    let folder = temp_folder("loud");
    let mut loud = OfflineSession::init().into_state();
    loud.set_value("volume", 86.0 * 86.0).unwrap();
    PresetLibrary::new(&folder)
        .save("Loud", &loud, false)
        .unwrap();
    let mut controller = controller().with_library(PresetLibrary::new(&folder));
    let reply = ok(controller.call("load_preset", &json!({"name": "Loud"})));
    assert_eq!(reply["status"], "needs_confirmation");
    assert!(controller.confirm_pending().unwrap());
    assert_eq!(controller.session().control("volume"), Some(86.0 * 86.0));
    let _ = std::fs::remove_dir_all(&folder);
}
