//! The preview engine on real patches, and the `audition` tool around it.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use sas_llm_control::{AuditionRequest, Auditioner, Controller, OfflineSession, Policy};
use sas_llm_listen::PreviewEngine;
use sas_preset::{InitContent, PresetState};

fn init() -> PresetState {
    PresetState::new(&InitContent { sample_seed: 0 })
}

fn request(notes: &[u8]) -> AuditionRequest {
    AuditionRequest {
        notes: notes.to_vec(),
        hold: 0.6,
        release: 0.6,
        velocity: 0.8,
    }
}

#[test]
fn the_init_patch_plays_its_note_at_pitch() {
    let mut engine = PreviewEngine::new();
    assert!(
        !engine.is_started(),
        "nothing is built before the first audition"
    );
    let a4 = engine.audition(&init(), &request(&[69])).unwrap();
    assert!(engine.is_started());
    assert_eq!(a4.get("silent"), None, "{a4}");
    assert!(
        (a4["pitch"]["hz"].as_f64().unwrap() - 440.0).abs() < 2.0,
        "{a4}"
    );
    assert!(a4["level"]["peak_db"].as_f64().unwrap() < 0.0, "{a4}");
    assert_eq!(a4["level"]["non_finite_samples"], 0);
    assert!(a4["envelope"]["release_ms"].is_number(), "{a4}");
    assert_eq!(a4["notes"], json!([69]));
    assert!(a4.get("cut_short").is_none());

    // The same engine plays the next audition, an octave down.
    let a3 = engine.audition(&init(), &request(&[57])).unwrap();
    assert!(
        (a3["pitch"]["hz"].as_f64().unwrap() - 220.0).abs() < 1.5,
        "{a3}"
    );
}

#[test]
fn auditions_repeat_and_follow_the_patch() {
    let mut engine = PreviewEngine::new();
    let first = engine.audition(&init(), &request(&[60])).unwrap();
    let again = engine.audition(&init(), &request(&[60])).unwrap();
    let level = |value: &Value| value["level"]["rms_db"].as_f64().unwrap();
    assert!(
        (level(&first) - level(&again)).abs() < 0.5,
        "{first}\n{again}"
    );

    // Closing the filter darkens the sound.
    let mut dark = init();
    dark.set_value("filter_1_on", 1.0).unwrap();
    dark.set_value("filter_1_cutoff", 30.0).unwrap();
    let darker = engine.audition(&dark, &request(&[60])).unwrap();
    let centroid = |value: &Value| {
        value["spectrum"]["centroid_hz"][3]
            .as_f64()
            .expect("an audible slice")
    };
    assert!(
        centroid(&darker) < 0.6 * centroid(&first),
        "{first}\n{darker}"
    );
}

#[test]
fn no_source_in_this_crate_sets_the_seed_counter() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(src).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("set_next_seed("), "{}", path.display());
    }
}

#[test]
fn a_render_past_its_budget_stops_and_says_so() {
    let mut engine = PreviewEngine::with_budget(Duration::ZERO);
    let started = Instant::now();
    let analysis = engine
        .audition(
            &init(),
            &AuditionRequest {
                hold: 4.0,
                release: 2.0,
                ..request(&[60])
            },
        )
        .unwrap();
    assert!(analysis["cut_short"].is_string(), "{analysis}");
    assert!(analysis["rendered_s"].as_f64().unwrap() < 0.1, "{analysis}");
    // The budget covers rendering; building the synth is the rest.
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_full_length_preview_renders_within_the_default_budget() {
    let mut engine = PreviewEngine::new();
    let analysis = engine
        .audition(
            &init(),
            &AuditionRequest {
                notes: vec![48, 52, 55, 59, 62, 64, 67, 71],
                hold: 4.0,
                release: 2.0,
                velocity: 1.0,
            },
        )
        .unwrap();
    assert!(analysis.get("cut_short").is_none(), "{analysis}");
    assert_eq!(analysis["rendered_s"], 6.0);
}

#[test]
fn dropping_the_engine_ends_its_thread() {
    let mut engine = PreviewEngine::new();
    engine.audition(&init(), &request(&[60])).unwrap();
    let started = Instant::now();
    drop(engine);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn the_audition_tool_is_offered_only_with_a_preview_engine() {
    let mut plain = Controller::new(OfflineSession::init(), Policy::default());
    assert!(!plain.specs().iter().any(|spec| spec.name == "audition"));
    let output = plain.call("audition", &json!({}));
    assert!(
        output.is_error && output.content.contains("not available"),
        "{output:?}"
    );

    let mut listening = Controller::new(OfflineSession::init(), Policy::default())
        .with_auditioner(Box::new(PreviewEngine::new()));
    assert!(listening.specs().iter().any(|spec| spec.name == "audition"));
    let output = listening.call("audition", &json!({"notes": [69], "seconds": 0.5}));
    assert!(!output.is_error, "{}", output.content);
    let analysis: Value = serde_json::from_str(&output.content).unwrap();
    assert!((analysis["pitch"]["hz"].as_f64().unwrap() - 440.0).abs() < 2.0);
    // Auditioning changes nothing in the patch, and is not an undo step.
    assert_eq!(listening.undo_depth(), 0);

    let output = listening.call("audition", &json!({"notes": [200]}));
    assert!(output.is_error && output.content.contains("0-127"));
}

#[test]
#[ignore = "prints an example analysis"]
fn print_an_example() {
    let mut engine = PreviewEngine::new();
    let analysis = engine.audition(&init(), &request(&[60, 64, 67])).unwrap();
    println!("{} chars\n{analysis}", analysis.to_string().len());
}

#[test]
fn an_owner_shutting_down_stops_the_preview() {
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut engine = PreviewEngine::new().stopped_by(stop);
    let started = Instant::now();
    let error = engine.audition(&init(), &request(&[60])).unwrap_err();
    assert!(error.contains("stopped"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(1));
}
