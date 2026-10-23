//! Golden presets built through the tools.
//!
//! Each `migration/corpus/llm/sessions/<name>.json` is a scripted tool
//! session. Replaying it on its base (a corpus preset loaded on the init
//! patch, as the reference loads presets) must succeed call by call and give
//! exactly `migration/corpus/llm/presets/<name>.vital`. The reference
//! renders those presets as corpus render cases, and the port's render
//! parity tests (`sas-engine`, `tests/render.rs`) and preset state cases
//! compare against it: a patch made with the tools sounds the same in the
//! reference and the port (PLAN.md M9 exit).
//!
//! After an intentional change to the tools or a session, rewrite the
//! presets with `SAS_LLM_GOLDEN_WRITE=1 cargo test -p sas-llm-control
//! --test golden`, then re-render the reference hashes for the cases that
//! use them (migration/AGENTS.md).

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use sas_llm_control::sas_preset::{InitContent, PresetState};
use sas_llm_control::{Controller, OfflineSession, Policy, SynthSession};

#[derive(Deserialize)]
struct Session {
    #[allow(dead_code)]
    description: String,
    /// A preset under `migration/corpus`, loaded on the init patch first.
    base: String,
    calls: Vec<Call>,
}

#[derive(Deserialize)]
struct Call {
    tool: String,
    args: Value,
}

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migration/corpus/llm")
}

/// The preset JSON `session` produces on the init patch.
fn replay(name: &str, session: &Session) -> String {
    let policy = Policy {
        confirm_above: Vec::new(),
        ..Policy::default()
    };
    let mut base = PresetState::new(&InitContent { sample_seed: 0 });
    let base_path = corpus().join("..").join(&session.base);
    base.load_file(&base_path)
        .unwrap_or_else(|e| panic!("{name}: base {}: {e}", base_path.display()));
    let mut controller = Controller::new(OfflineSession::new(base), policy);
    for (index, call) in session.calls.iter().enumerate() {
        let output = controller.call(&call.tool, &call.args);
        assert!(
            !output.is_error && !output.content.contains("needs_confirmation"),
            "{name}: call {index} ({}) failed: {}",
            call.tool,
            output.content
        );
        assert!(
            !output.content.contains("\"clamped\":true"),
            "{name}: call {index} ({}) was clamped: {}",
            call.tool,
            output.content
        );
    }
    controller.session().snapshot().to_json_string()
}

#[test]
fn sessions_produce_the_committed_presets() {
    let sessions = corpus().join("sessions");
    let presets = corpus().join("presets");
    let write = std::env::var_os("SAS_LLM_GOLDEN_WRITE").is_some();
    let mut names: Vec<String> = std::fs::read_dir(&sessions)
        .expect("sessions folder")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "json").then(|| path.file_stem()?.to_str().map(str::to_string))?
        })
        .collect();
    names.sort();
    assert!(!names.is_empty(), "no sessions in {}", sessions.display());
    for name in names {
        let text = std::fs::read_to_string(sessions.join(format!("{name}.json"))).unwrap();
        let session: Session =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}.json: {e}"));
        let preset = replay(&name, &session);
        let path = presets.join(format!("{name}.vital"));
        if write {
            std::fs::create_dir_all(&presets).unwrap();
            std::fs::write(&path, &preset).unwrap();
            continue;
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{}: {e} (write it with SAS_LLM_GOLDEN_WRITE=1)",
                path.display()
            )
        });
        assert!(
            committed == preset,
            "{name}: the session no longer produces {} (rewrite it with SAS_LLM_GOLDEN_WRITE=1 if \
             the change is intended, then re-render its reference hashes)",
            path.display()
        );
    }
}

#[test]
fn every_golden_preset_has_a_session() {
    for entry in std::fs::read_dir(corpus().join("presets")).expect("presets folder") {
        let path = entry.unwrap().path();
        let stem = path.file_stem().unwrap().to_str().unwrap();
        assert!(
            corpus()
                .join("sessions")
                .join(format!("{stem}.json"))
                .is_file(),
            "{} has no session",
            path.display()
        );
    }
}
