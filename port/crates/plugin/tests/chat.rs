//! The editor's chat end to end without a host or a network: `PluginChat`
//! with a scripted provider, the tools running on the GUI thread through a
//! `GuiContext` whose parameter calls set the parameters directly.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nice_plug::context::gui::{GuiContext, GuiContextInner};
use nice_plug::context::PluginApi;
use nice_plug::params::internals::ParamPtr;
use nice_plug::plugin::PluginState;
use serde_json::{json, Value};

use sas_engine::document::PresetDocument;
use sas_engine::edit_bus::EditBus;
use sas_gui::chat::LineKind;
use sas_gui::{ChatHost, ChatSettingsView, FocusMode};
use sas_llm_agent::{ChatSettings, ScriptedTransport, Transport};
use sas_plugin::chat::{PluginChat, TransportFactory};
use sas_plugin::keys::KeyStore;
use sas_plugin::params::SynthParams;
use sas_preset::InitContent;

/// A `GuiContext` without a host: parameter changes land directly, states
/// restore as the wrapper does.
struct TestGui {
    params: Arc<SynthParams>,
}

impl GuiContextInner for TestGui {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Vst3
    }
    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
        // SAFETY: the pointer comes from the plugin's own parameters, which
        // outlive the test.
        unsafe {
            param._internal_set_normalized_value(normalized);
        }
    }
    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        self.params.capture_state()
    }
    fn set_state(&self, state: PluginState) {
        self.params.restore_state(state);
    }
    fn request_restart(&self) {}
}

#[derive(Default)]
struct MemoryKeys(Mutex<Option<String>>, Arc<AtomicUsize>);

impl KeyStore for MemoryKeys {
    fn get(&self, _provider: &str) -> Option<String> {
        self.1.fetch_add(1, Ordering::Relaxed);
        self.0.lock().unwrap().clone()
    }
    fn set(&self, _provider: &str, key: &str) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(key.to_string());
        Ok(())
    }
}

fn anthropic(content: Value, stop: &str) -> Value {
    json!({"content": content, "stop_reason": stop, "usage": {"input_tokens": 10, "output_tokens": 5}})
}

fn settings_path(name: &str, settings: Option<ChatSettings>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sas-plugin-chat-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("chat.json");
    if let Some(settings) = settings {
        settings.save(&path).unwrap();
    }
    path
}

fn scripted(responses: Vec<Value>) -> TransportFactory {
    let responses = Mutex::new(Some(responses));
    Box::new(move |_config| {
        let responses = responses.lock().unwrap().take().unwrap_or_default();
        Ok(Box::new(ScriptedTransport::new(responses)) as Box<dyn Transport>)
    })
}

struct Rig {
    params: Arc<SynthParams>,
    chat: PluginChat,
    key_reads: Arc<AtomicUsize>,
}

fn rig(
    name: &str,
    settings: Option<ChatSettings>,
    key: Option<&str>,
    responses: Vec<Value>,
) -> Rig {
    let (edits, _consumer) = EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
    let params = Arc::new(SynthParams::new(edits));
    let key_reads = Arc::new(AtomicUsize::new(0));
    let keys = MemoryKeys(Mutex::new(key.map(str::to_string)), key_reads.clone());
    let chat = PluginChat::with(
        params.clone(),
        Some(settings_path(name, settings)),
        Box::new(keys),
        scripted(responses),
    );
    chat.attach(
        GuiContext::new(Arc::new(TestGui {
            params: params.clone(),
        })),
        Arc::new(|| {}),
    );
    Rig {
        params,
        chat,
        key_reads,
    }
}

impl Rig {
    /// Runs editor frames until the chat is idle.
    fn settle(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.chat.run_frame();
            if !self.chat.view().busy {
                // One more frame for anything queued as the turn ended.
                self.chat.run_frame();
                return;
            }
            assert!(Instant::now() < deadline, "the chat did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn value(&self, name: &str) -> f32 {
        self.params.control(name).unwrap().param.saved_value()
    }
}

fn on() -> ChatSettings {
    ChatSettings {
        enabled: true,
        ..ChatSettings::default()
    }
}

#[test]
fn the_chat_is_off_by_default_and_sends_nothing() {
    let rig = rig("off", None, Some("key"), Vec::new());
    let view = rig.chat.view();
    assert!(!view.enabled);
    assert!(view.problem.is_none());
    rig.chat.send("hello".into());
    let view = rig.chat.view();
    assert!(!view.busy);
    assert_eq!(view.lines.last().unwrap().kind, LineKind::Error);
    assert!(view.lines.last().unwrap().text.contains("off"));
}

#[test]
fn a_hosted_provider_without_a_key_explains_itself() {
    let rig = rig("nokey", Some(on()), None, Vec::new());
    assert!(rig.chat.view().problem.unwrap().contains("API key"));
}

#[test]
fn a_message_drives_the_synth_and_can_be_undone() {
    let responses = vec![
        anthropic(
            json!([
                {"type": "text", "text": "Opening the filter."},
                {"type": "tool_use", "id": "t1", "name": "set_params", "input": {"changes": [
                    {"name": "filter_1_on", "value": "On"}, {"name": "filter_1_cutoff", "value": 24}
                ]}}
            ]),
            "tool_use",
        ),
        anthropic(
            json!([{"type": "text", "text": "Brighter now."}]),
            "end_turn",
        ),
    ];
    let rig = rig("drive", Some(on()), Some("key"), responses);
    rig.chat.send("brighter please".into());
    rig.settle();
    assert_eq!(rig.value("filter_1_on"), 1.0);
    assert_eq!(rig.value("filter_1_cutoff"), 84.0);
    let view = rig.chat.view();
    let kinds: Vec<LineKind> = view.lines.iter().map(|line| line.kind).collect();
    assert_eq!(
        kinds,
        [
            LineKind::User,
            LineKind::Assistant,
            LineKind::Tool,
            LineKind::Assistant
        ]
    );
    assert!(view.can_undo);

    rig.chat.undo();
    rig.settle();
    assert_eq!(rig.value("filter_1_cutoff"), 60.0);
    assert!(!rig.chat.view().can_undo);
}

#[test]
fn a_held_volume_raise_waits_for_the_buttons() {
    let responses = vec![
        anthropic(
            json!([{"type": "tool_use", "id": "t1", "name": "set_params",
                    "input": {"changes": [{"name": "volume", "value": 4}]}}]),
            "tool_use",
        ),
        anthropic(
            json!([{"type": "text", "text": "Shall I raise it to +4 dB?"}]),
            "end_turn",
        ),
    ];
    let rig = rig("confirm", Some(on()), Some("key"), responses);
    rig.chat.send("much louder".into());
    rig.settle();
    assert!(rig.chat.view().pending.is_some());
    assert!((rig.value("volume") - 5473.0404).abs() < 1e-3);
    rig.chat.confirm(true);
    rig.settle();
    assert!(rig.chat.view().pending.is_none());
    assert!((rig.value("volume").sqrt() - 84.0).abs() < 1e-2);
}

#[test]
fn saving_settings_turns_the_chat_on_and_keeps_the_key_out_of_the_file() {
    let rig = rig("settings", None, None, Vec::new());
    let view = ChatSettingsView {
        enabled: true,
        provider: "anthropic".into(),
        focus: FocusMode::Plain,
        ..rig.chat.settings()
    };
    rig.chat
        .save_settings(view, Some("secret-key".into()))
        .unwrap();
    let settings = rig.chat.settings();
    assert!(settings.enabled && settings.has_key);
    assert_eq!(settings.focus, FocusMode::Plain);
    assert!(rig.chat.view().problem.is_none());
    let file = std::env::temp_dir()
        .join(format!("sas-plugin-chat-settings-{}", std::process::id()))
        .join("chat.json");
    let text = std::fs::read_to_string(file).unwrap();
    assert!(text.contains("\"enabled\": true"));
    assert!(!text.contains("secret-key"));
}

#[test]
fn the_key_store_is_read_once_not_every_frame() {
    let rig = rig("reads", Some(on()), Some("key"), Vec::new());
    for _ in 0..100 {
        rig.chat.run_frame();
        let _ = rig.chat.view();
    }
    assert_eq!(rig.key_reads.load(Ordering::Relaxed), 1);
}

#[test]
fn tool_summaries_cut_long_text_on_character_boundaries() {
    let text = "é".repeat(150);
    let short = sas_plugin::chat::summary(&text);
    assert_eq!(short.chars().count(), 100);
    assert!(short.ends_with("..."));
    assert_eq!(sas_plugin::chat::summary("short"), "short");
}

#[test]
fn changing_settings_starts_a_new_conversation() {
    let responses = vec![
        anthropic(
            json!([{"type": "tool_use", "id": "t1", "name": "set_params",
                    "input": {"changes": [{"name": "filter_1_cutoff", "value": 24}]}}]),
            "tool_use",
        ),
        anthropic(json!([{"type": "text", "text": "Done."}]), "end_turn"),
    ];
    let rig = rig("restart", Some(on()), Some("key"), responses);
    rig.chat.send("brighter".into());
    rig.settle();
    assert!(rig.chat.view().can_undo);
    let settings = rig.chat.settings();
    rig.chat.save_settings(settings, None).unwrap();
    let view = rig.chat.view();
    assert!(!view.busy && !view.can_undo);
    assert!(view.lines.last().unwrap().text.contains("new conversation"));
    // The synth keeps the change; only the chat's history starts over.
    assert_eq!(rig.value("filter_1_cutoff"), 84.0);
}

#[test]
fn dropping_the_plugin_with_an_idle_chat_thread_does_not_hang() {
    let responses = vec![anthropic(
        json!([{"type": "text", "text": "Hi."}]),
        "end_turn",
    )];
    let rig = rig("drop", Some(on()), Some("key"), responses);
    rig.chat.send("hello".into());
    rig.settle();
    let started = Instant::now();
    drop(rig);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn an_off_chat_never_reads_the_key_store() {
    let rig = rig("off-keys", None, Some("key"), Vec::new());
    for _ in 0..10 {
        rig.chat.run_frame();
        let _ = rig.chat.view();
    }
    assert!(!rig.chat.settings().has_key);
    rig.chat.send("hello".into());
    assert_eq!(rig.key_reads.load(Ordering::Relaxed), 0);
}

#[test]
fn the_in_memory_chat_stays_off_and_keeps_no_keys() {
    let (edits, _consumer) = EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
    let chat = PluginChat::in_memory(Arc::new(SynthParams::new(edits)));
    assert!(!chat.view().enabled);
    let settings = ChatSettingsView {
        enabled: true,
        ..chat.settings()
    };
    assert!(chat.save_settings(settings, Some("key".into())).is_err());
    assert!(!chat.view().enabled);
}

fn cutoff_then_done() -> Vec<Value> {
    vec![
        anthropic(
            json!([
                {"type": "text", "text": "Opening the filter."},
                {"type": "tool_use", "id": "t1", "name": "set_params",
                 "input": {"changes": [{"name": "filter_1_cutoff", "value": 24}]}}
            ]),
            "tool_use",
        ),
        anthropic(json!([{"type": "text", "text": "Done."}]), "end_turn"),
    ]
}

#[test]
fn dropping_the_plugin_ends_a_worker_blocked_on_the_editor() {
    // The editor never runs a frame (its window is closed), so the worker
    // waits on the bridge for the tool call.
    let rig = rig("blocked", Some(on()), Some("key"), cutoff_then_done());
    rig.chat.send("brighter".into());
    // The turn's first bridge call (checking for outside changes) blocks.
    std::thread::sleep(Duration::from_millis(100));
    assert!(rig.chat.view().busy);
    let started = Instant::now();
    drop(rig);
    // Drop returns early only when the thread has ended.
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_stop_right_after_sending_is_not_lost() {
    let rig = rig("stop", Some(on()), Some("key"), cutoff_then_done());
    rig.chat.send("brighter".into());
    rig.chat.stop();
    rig.settle();
    let lines = rig.chat.view().lines;
    assert!(lines
        .iter()
        .any(|line| line.text.contains("stopped by the user")));
    assert!(!lines.iter().any(|line| line.text == "Done."));
}

#[test]
fn a_retired_worker_stays_out_of_the_new_conversation() {
    let rig = rig("retired", Some(on()), Some("key"), cutoff_then_done());
    rig.chat.send("brighter".into());
    std::thread::sleep(Duration::from_millis(100));
    let settings = rig.chat.settings();
    rig.chat.save_settings(settings, None).unwrap();
    let count = rig.chat.view().lines.len();
    std::thread::sleep(Duration::from_millis(100));
    for _ in 0..5 {
        rig.chat.run_frame();
    }
    let view = rig.chat.view();
    assert_eq!(view.lines.len(), count, "{:?}", view.lines);
    assert!(view.lines.last().unwrap().text.contains("new conversation"));
    assert!(!view.busy);
    // Its tool call was dropped with its bridge, unapplied.
    assert_eq!(rig.value("filter_1_cutoff"), 60.0);
}
