//! Chat sessions against recorded exchanges: no test calls a model API.
//!
//! Each recording in `tests/replays/` was written from the authored
//! responses in its test, in the provider's documented wire format, by
//! `SAS_LLM_REPLAY_WRITE=1 cargo test -p sas-llm-agent --test replay`.
//! A normal run replays the file and checks that every request the agent
//! sends matches the recorded one (conversation, model and tool names), so a
//! change in how the agent talks to a provider fails here. A recording made
//! against a real provider with `sas-llm --record` replays the same way.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use sas_llm_agent::{
    Agent, AgentConfig, AgentError, ProviderConfig, ProviderKind, RecordingTransport,
    ReplayTransport, ScriptedTransport, Transport,
};
use sas_llm_control::{Controller, OfflineSession, Policy, SynthSession};

fn replay_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/replays")
        .join(format!("{name}.jsonl"))
}

/// The recording `name`, or, with `SAS_LLM_REPLAY_WRITE`, a transport that
/// answers with `responses` and writes the recording.
fn transport(name: &str, responses: Vec<Value>) -> Box<dyn Transport> {
    let path = replay_path(name);
    if std::env::var_os("SAS_LLM_REPLAY_WRITE").is_some() {
        Box::new(RecordingTransport::new(ScriptedTransport::new(responses), path).unwrap())
    } else {
        Box::new(ReplayTransport::open(&path).unwrap())
    }
}

fn agent(kind: ProviderKind, transport: Box<dyn Transport>) -> Agent<OfflineSession> {
    let mut provider = ProviderConfig::new(kind, Some("test-key".into()));
    provider.model = "test-model".into();
    Agent::new(
        Controller::new(OfflineSession::init(), Policy::default()),
        provider.build(),
        transport,
        AgentConfig::default(),
    )
}

fn anthropic(content: Value, stop: &str) -> Value {
    json!({
        "id": "msg", "type": "message", "role": "assistant", "model": "test-model",
        "content": content, "stop_reason": stop,
        "usage": {"input_tokens": 900, "output_tokens": 60, "cache_read_input_tokens": 5200}
    })
}

fn openai(message: Value, finish: &str) -> Value {
    json!({
        "id": "chatcmpl", "object": "chat.completion", "model": "test-model",
        "choices": [{"index": 0, "message": message, "finish_reason": finish}],
        "usage": {"prompt_tokens": 6100, "completion_tokens": 55}
    })
}

#[test]
fn anthropic_session_edits_and_undoes() {
    let responses = vec![
        anthropic(
            json!([
                {"type": "text", "text": "Slowing the envelope and adding movement."},
                {"type": "tool_use", "id": "toolu_1", "name": "set_params", "input": {"changes": [
                    {"name": "env_1_attack", "value": "0.8 secs"},
                    {"name": "env_1_release", "value": 2},
                    {"name": "filter_1_on", "value": "On"}
                ]}},
                {"type": "tool_use", "id": "toolu_2", "name": "connect_modulation",
                 "input": {"source": "lfo_1", "destination": "filter_1_cutoff", "amount": 0.3, "bipolar": true}}
            ]),
            "tool_use",
        ),
        anthropic(
            json!([{"type": "text", "text": "Done: a slow fade-in, a long tail, and LFO 1 sweeping the filter."}]),
            "end_turn",
        ),
        anthropic(
            json!([{"type": "tool_use", "id": "toolu_3", "name": "undo", "input": {}}]),
            "tool_use",
        ),
        anthropic(
            json!([{"type": "text", "text": "Back to where we started."}]),
            "end_turn",
        ),
    ];
    let mut agent = agent(
        ProviderKind::Anthropic,
        transport("anthropic_pad", responses),
    );
    let mut calls = Vec::new();
    let report = agent
        .send(
            "Make this a slow pad with some filter movement.",
            &mut |event| {
                if let sas_llm_agent::AgentEvent::ToolCall { name, .. } = event {
                    calls.push(name.to_string());
                }
            },
        )
        .unwrap();
    assert_eq!(calls, ["set_params", "connect_modulation"]);
    assert_eq!(report.model_calls, 2);
    assert_eq!(report.tool_calls, 2);
    assert!(report.text.contains("LFO 1 sweeping"));
    assert_eq!(report.usage.cache_read_tokens, 2 * 5200);
    let state = agent.controller().session().state();
    assert!((state.value("env_1_attack").unwrap().powi(4) - 0.8).abs() < 1e-4);
    assert_eq!(state.modulations()[0].source, "lfo_1");

    let report = agent.send("Actually, undo that.", &mut |_| {}).unwrap();
    assert_eq!(report.tool_calls, 1);
    let state = agent.controller().session().state();
    assert!(state.modulations()[0].is_free());
    assert_eq!(state.value("filter_1_on"), Some(0.0));
    assert_eq!(agent.usage().input_tokens, 4 * 900);
}

#[test]
fn openai_session_waits_for_confirmation() {
    let responses = vec![
        openai(
            json!({"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_1", "type": "function",
                 "function": {"name": "set_params", "arguments": "{\"changes\":[{\"name\":\"volume\",\"value\":4}]}"}}
            ]}),
            "tool_calls",
        ),
        openai(
            json!({"role": "assistant", "content": "That would raise the master volume to +4 dB. Shall I go ahead?"}),
            "stop",
        ),
        openai(
            json!({"role": "assistant", "content": "Done, it is at +4 dB now."}),
            "stop",
        ),
    ];
    let mut agent = agent(
        ProviderKind::OpenAiCompatible,
        transport("openai_confirm", responses),
    );
    let report = agent.send("Make it a lot louder.", &mut |_| {}).unwrap();
    assert!(report.text.contains("Shall I go ahead"));
    assert!(agent.controller().pending().is_some());
    let before = agent.controller().session().control("volume").unwrap();
    assert!((before - 5473.0404).abs() < 1e-3);

    assert!(agent.resolve_pending(true).unwrap());
    let volume = agent.controller().session().control("volume").unwrap();
    assert!((volume.sqrt() - 84.0).abs() < 1e-3, "{volume}");
    // The next request tells the model the user confirmed (checked against
    // the recording).
    agent.send("Thanks.", &mut |_| {}).unwrap();
}

#[test]
fn a_request_that_differs_from_the_recording_fails() {
    if std::env::var_os("SAS_LLM_REPLAY_WRITE").is_some() {
        return;
    }
    let mut agent = agent(
        ProviderKind::Anthropic,
        Box::new(ReplayTransport::open(&replay_path("anthropic_pad")).unwrap()),
    );
    let error = agent
        .send("Something else entirely.", &mut |_| {})
        .unwrap_err();
    assert!(
        matches!(error, AgentError::Replay(ref message) if message.contains("differs")),
        "{error}"
    );
}

#[test]
fn the_step_limit_stops_a_looping_model() {
    let looping = anthropic(
        json!([{"type": "tool_use", "id": "toolu_x", "name": "get_patch", "input": {}}]),
        "tool_use",
    );
    let mut agent = Agent::new(
        Controller::new(OfflineSession::init(), Policy::default()),
        ProviderConfig::new(ProviderKind::Anthropic, None).build(),
        Box::new(ScriptedTransport::new(vec![looping; 10])),
        AgentConfig {
            max_steps: 3,
            ..AgentConfig::default()
        },
    );
    let report = agent.send("Look around.", &mut |_| {}).unwrap();
    assert_eq!(report.model_calls, 3);
    assert!(report.stopped.unwrap().contains("3 model calls"));
}

#[test]
fn the_session_token_limit_refuses_further_messages() {
    let reply = anthropic(json!([{"type": "text", "text": "Hello."}]), "end_turn");
    let mut agent = Agent::new(
        Controller::new(OfflineSession::init(), Policy::default()),
        ProviderConfig::new(ProviderKind::Anthropic, None).build(),
        Box::new(ScriptedTransport::new(vec![reply.clone(), reply])),
        AgentConfig {
            max_session_tokens: 1000,
            ..AgentConfig::default()
        },
    );
    agent.send("Hi.", &mut |_| {}).unwrap();
    let error = agent.send("Hi again.", &mut |_| {}).unwrap_err();
    assert!(matches!(error, AgentError::Budget(_)), "{error}");
}

#[test]
fn recordings_hold_no_credentials() {
    for entry in
        std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/replays")).unwrap()
    {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for secret in ["test-key", "x-api-key", "authorization", "Bearer"] {
            assert!(
                !text.contains(secret),
                "{} contains {secret:?}",
                path.display()
            );
        }
    }
}

#[test]
fn a_cancelled_message_stops_after_the_call_in_flight() {
    let looping = anthropic(
        json!([{"type": "tool_use", "id": "toolu_x", "name": "get_patch", "input": {}}]),
        "tool_use",
    );
    let mut agent = Agent::new(
        Controller::new(OfflineSession::init(), Policy::default()),
        ProviderConfig::new(ProviderKind::Anthropic, None).build(),
        Box::new(ScriptedTransport::new(vec![looping; 10])),
        AgentConfig::default(),
    );
    let cancel = agent.cancel_flag();
    let report = agent
        .send("Look around.", &mut |event| {
            if let sas_llm_agent::AgentEvent::ToolResult { .. } = event {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .unwrap();
    assert_eq!(report.model_calls, 1);
    assert_eq!(report.stopped.as_deref(), Some("stopped by the user"));
}
