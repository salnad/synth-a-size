//! The `audition` request: defaults, limits and errors.

use serde_json::json;

use sas_llm_control::listen::{MAX_HOLD_SECONDS, MAX_NOTES, MAX_RELEASE_SECONDS};
use sas_llm_control::AuditionRequest;

#[test]
fn defaults_play_middle_c_for_a_second() {
    let request = AuditionRequest::from_args(&json!({})).unwrap();
    assert_eq!(request, AuditionRequest::default());
    assert_eq!(request.notes, [60]);
    assert_eq!(request.total_seconds(), 2.0);
}

#[test]
fn lengths_are_clamped_to_bound_the_preview() {
    let request =
        AuditionRequest::from_args(&json!({"seconds": 100, "release": 100, "notes": [36, 48]}))
            .unwrap();
    assert_eq!(request.hold, MAX_HOLD_SECONDS);
    assert_eq!(request.release, MAX_RELEASE_SECONDS);
    let short = AuditionRequest::from_args(&json!({"seconds": -1, "release": -1})).unwrap();
    assert!(short.hold > 0.0 && short.release == 0.0);
}

#[test]
fn bad_notes_and_velocities_are_errors() {
    let too_many: Vec<u8> = (60..60 + MAX_NOTES as u8 + 1).collect();
    for args in [
        json!({"notes": []}),
        json!({"notes": too_many}),
        json!({"notes": [128]}),
        json!({"notes": [-1]}),
        json!({"velocity": 1.5}),
        json!({"notes": "C4"}),
    ] {
        assert!(AuditionRequest::from_args(&args).is_err(), "{args}");
    }
}
