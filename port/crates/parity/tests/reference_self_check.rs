//! Self-checks of the parity harness against the real reference captures.

use std::fs;
use std::panic;

use sas_parity::{
    all_case_ids, assert_artifact, assert_parity, cases, parity_report, voice_event, ModuleCase,
    RenderCase,
};

#[test]
fn every_case_matches_its_own_reference() {
    let ids = all_case_ids();
    assert!(ids.iter().any(|id| id.starts_with("render/")));
    assert!(ids.iter().any(|id| !id.starts_with("render/")));
    for id in ids {
        if id.starts_with("render/") {
            let case = RenderCase::load(&id);
            let reference = case.reference();
            assert_eq!(reference.channels, 2);
            assert_eq!(reference.sample_rate, Some(case.sample_rate()));
            case.assert_parity(&reference);
        } else {
            let case = ModuleCase::load(&id);
            assert!(
                !case.output_names().is_empty() || !case.artifact_names().is_empty(),
                "{id} has no outputs"
            );
            for output in case.output_names() {
                let reference = case.reference_audio(&output);
                if case.num_samples() > 0 {
                    assert_eq!(reference.frames, case.num_samples(), "{id}:{output}");
                }
                assert_parity!(format!("{id}:{output}").as_str(), reference);
            }
            for artifact in case.artifact_names() {
                let bytes = case.artifact_bytes(&artifact);
                case.assert_artifact(&artifact, &bytes);
            }
        }
    }
}

#[test]
fn a_perturbed_output_fails_with_the_case_tolerances() {
    let id = "filters/digital_svf/lowpass_saw_sweep";
    let case = ModuleCase::load(id);
    assert_eq!(case.tolerances().residual_db, Some(-80.0));
    assert_eq!(case.tolerances().max_abs_error, Some(1e-4));

    let mut output = case.reference("audio");
    for frame in output.iter_mut().skip(1000).take(64) {
        frame[0] += 1e-3;
    }
    let report = parity_report(id, &output);
    assert!(!report.pass);
    assert!(report
        .checks
        .iter()
        .any(|c| c.metric == "max_abs_error" && !c.pass));

    let result = panic::catch_unwind(|| assert_parity!(id, output));
    let message = *result.unwrap_err().downcast::<String>().unwrap();
    assert!(
        message.contains("parity FAILED for filters/digital_svf/lowpass_saw_sweep:audio"),
        "{message}"
    );
    assert!(message.contains("port_output.npy"), "{message}");
}

#[test]
fn wrong_length_fails() {
    let id = "effects/distortion/sin_fold_drive_ramp:audio";
    let mut output = ModuleCase::load(id).reference("audio");
    output.pop();
    assert!(panic::catch_unwind(|| assert_parity!(id, output)).is_err());
}

#[test]
fn inputs_and_triggers_match_the_spec() {
    let case = ModuleCase::load("filters/digital_svf/lowpass_saw_sweep");
    assert_eq!(case.module(), "filter");
    assert_eq!(case.params()["model"], "digital");
    assert_eq!(case.sample_rate(), 44100);
    assert_eq!(case.block_size(), 64);
    let cutoff = case.input("midi_cutoff");
    assert_eq!(cutoff.len(), case.num_samples());
    assert!((cutoff[0][0] - 30.0).abs() < 1e-4);
    assert!((cutoff[cutoff.len() - 1][3] - 110.0).abs() < 1e-3);
    let reset = case.triggers("reset");
    assert_eq!(reset.len(), 1);
    assert_eq!(reset[0].sample, 0);
    assert_eq!(reset[0].value, voice_event::ON);
    assert!(case.triggers("audio").is_empty());

    let ladder = ModuleCase::load("filters/ladder/noise_resonant_2x");
    assert_eq!(ladder.oversample(), 2);
    assert_eq!(ladder.internal_sample_rate(), 96000);

    let envelope = ModuleCase::load("modulation/envelope/adsr");
    assert_eq!(
        envelope.output_names(),
        vec!["phase".to_string(), "value".to_string()]
    );
    assert_eq!(envelope.tolerances().residual_db, Some(-100.0));
    let trigger = envelope.triggers("trigger");
    assert_eq!(
        trigger
            .iter()
            .map(|t| (t.sample, t.value))
            .collect::<Vec<_>>(),
        vec![(441, voice_event::ON), (44100, voice_event::OFF)]
    );
}

#[test]
fn multi_output_modules_require_an_output_name() {
    let reference = ModuleCase::load("modulation/envelope/adsr").reference("value");
    assert!(panic::catch_unwind(|| assert_parity!("modulation/envelope/adsr", reference)).is_err());
}

#[test]
fn render_cases_expose_their_settings() {
    let case = RenderCase::load("render/init_saw__single_note_48k_b128");
    assert_eq!(case.sample_rate(), 48000);
    assert_eq!(case.block_size(), 128);
    assert!(case.preset_path().is_file());
    assert!(case.midi_path().is_file());
    assert_eq!(case.tolerances().residual_db, Some(-50.0));
    assert_eq!(case.tolerances().max_abs_error, None);
}

const ECHO: &str = "harness/echo/params_and_file";

#[test]
fn artifacts_are_exact_and_cover_corpus_inputs() {
    let case = ModuleCase::load(ECHO);
    assert_eq!(case.module(), "harness_echo");
    assert_eq!(
        case.artifact_names(),
        vec!["corpus_file.txt", "params.json"]
    );

    // The hook read the corpus file through --corpus-dir, byte for byte.
    let preset = fs::read(cases::corpus_dir().join("presets/init_saw.vital")).unwrap();
    assert_artifact!(ECHO, "corpus_file.txt", preset);

    // nlohmann dump(2) of the params, reproduced exactly from Rust.
    let params = serde_json::to_string_pretty(case.params()).unwrap() + "\n";
    assert_artifact!(ECHO, "params.json", params);

    // Arrays from an artifact hook keep their own channel count.
    let ramp = case.reference_audio("ramp");
    assert_eq!((ramp.channels, ramp.frames), (2, 16));
    assert_parity!("harness/echo/params_and_file:ramp", ramp);
}

#[test]
fn artifact_mismatches_fail_with_the_first_difference() {
    let case = ModuleCase::load(ECHO);
    let text = case.artifact("params.json");

    let changed = text.replace("2.5", "2.50");
    let message = *panic::catch_unwind(|| case.assert_artifact("params.json", &changed))
        .unwrap_err()
        .downcast::<String>()
        .unwrap();
    assert!(
        message.contains(
            "artifact parity FAILED for harness/echo/params_and_file:artifact/params.json"
        ),
        "{message}"
    );
    assert!(message.contains("first difference at byte"), "{message}");

    // Whitespace counts: no normalisation.
    let trailing = text.trim_end().to_string();
    assert!(panic::catch_unwind(|| case.assert_artifact("params.json", &trailing)).is_err());
    assert!(panic::catch_unwind(|| case.assert_artifact("missing.json", "")).is_err());
}
