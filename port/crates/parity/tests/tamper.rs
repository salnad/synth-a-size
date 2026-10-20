//! N1 regression: a corrupted or stale capture must not pass as the
//! reference. Runs as its own test binary because it points
//! SAS_PARITY_CAPTURES at a scratch copy.

use std::fs;
use std::panic;
use std::path::Path;

use sas_parity::{captures, ModuleCase, RenderCase};

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn panic_message(result: std::thread::Result<()>) -> String {
    let payload = result.expect_err("expected a panic");
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[test]
fn tampered_captures_are_rejected() {
    // Make sure real captures exist (rendered on demand locally, from the
    // artifact in CI), then work on a scratch copy.
    ModuleCase::load("filters/digital_svf/lowpass_saw_sweep").reference("audio");
    ModuleCase::load("harness/echo/params_and_file").artifact("params.json");
    RenderCase::load("render/init_saw__single_note").reference();
    let source = captures::captures_dir();
    let scratch = std::env::temp_dir().join(format!("sas-parity-tamper-{}", std::process::id()));
    fs::remove_dir_all(&scratch).ok();
    copy_dir(&source, &scratch);
    std::env::set_var("SAS_PARITY_CAPTURES", &scratch);
    std::env::set_var("SAS_PARITY_NO_BUILD", "1");

    // Untouched copy still verifies.
    ModuleCase::load("filters/digital_svf/lowpass_saw_sweep").reference("audio");

    // Zero the tail of a processor output (the reviewer's reproduction).
    let npy = scratch.join("modules/filters/digital_svf/lowpass_saw_sweep/out_audio.npy");
    let mut bytes = fs::read(&npy).unwrap();
    let len = bytes.len();
    bytes[len - 4096..].fill(0);
    fs::write(&npy, bytes).unwrap();
    let message = panic_message(panic::catch_unwind(|| {
        ModuleCase::load("filters/digital_svf/lowpass_saw_sweep").reference("audio");
    }));
    assert!(message.contains("stale or corrupted"), "{message}");

    // Edit a text artifact.
    let artifact = scratch.join("modules/harness/echo/params_and_file/artifact_params.json");
    let edited = fs::read_to_string(&artifact)
        .unwrap()
        .replace("echo", "ECHO");
    fs::write(&artifact, edited).unwrap();
    let message = panic_message(panic::catch_unwind(|| {
        ModuleCase::load("harness/echo/params_and_file").artifact("params.json");
    }));
    assert!(message.contains("stale or corrupted"), "{message}");

    // Edit a render WAV sample.
    let wav = scratch.join("renders/init_saw__single_note.wav");
    let mut bytes = fs::read(&wav).unwrap();
    let len = bytes.len();
    bytes[len - 8] ^= 0x40;
    fs::write(&wav, bytes).unwrap();
    let message = panic_message(panic::catch_unwind(|| {
        RenderCase::load("render/init_saw__single_note").reference();
    }));
    assert!(message.contains("stale or corrupted"), "{message}");

    // An edited input fails against capture.json.
    let input = scratch.join("modules/filters/digital_svf/lowpass_saw_sweep/in_audio.npy");
    let mut bytes = fs::read(&input).unwrap();
    let len = bytes.len();
    bytes[len - 4] ^= 0x01;
    fs::write(&input, bytes).unwrap();
    let message = panic_message(panic::catch_unwind(|| {
        ModuleCase::load("filters/digital_svf/lowpass_saw_sweep").input("audio");
    }));
    assert!(
        message.contains("does not match its capture.json hash"),
        "{message}"
    );

    fs::remove_dir_all(&scratch).ok();
}
