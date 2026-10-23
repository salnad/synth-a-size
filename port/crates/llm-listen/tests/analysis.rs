//! The analysis features on signals with known answers.

use std::f64::consts::PI;

use sas_llm_listen::analyze;

const RATE: f64 = 44_100.0;

fn seconds(s: f64) -> usize {
    (s * RATE) as usize
}

/// `amplitude(t)` times a sine at `hz`, for `length` seconds.
fn tone(hz: f64, length: f64, amplitude: impl Fn(f64) -> f64) -> Vec<f32> {
    (0..seconds(length))
        .map(|i| {
            let t = i as f64 / RATE;
            (amplitude(t) * (2.0 * PI * hz * t).sin()) as f32
        })
        .collect()
}

/// Deterministic white noise in -0.5..0.5.
fn noise(frames: usize, mut state: u64) -> Vec<f32> {
    (0..frames)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((state >> 40) as f64 / (1u64 << 24) as f64 - 0.5) as f32
        })
        .collect()
}

/// Held at 0.5 for 1 s after a 10 ms ramp, then falling 60 dB per 0.3 s.
fn plucked_envelope(t: f64) -> f64 {
    let level = if t < 0.01 { t / 0.01 } else { 1.0 };
    let release = if t > 1.0 {
        10f64.powf(-3.0 * (t - 1.0) / 0.3)
    } else {
        1.0
    };
    0.5 * level * release
}

#[test]
fn a_sine_reads_as_its_pitch_level_and_envelope() {
    let signal = tone(440.0, 2.0, plucked_envelope);
    let analysis = analyze(&signal, &signal, RATE, seconds(1.0));
    assert!(!analysis.silent);
    assert!(
        (analysis.level.peak_db + 6.0).abs() < 0.2,
        "{:?}",
        analysis.level
    );
    assert_eq!(analysis.level.clipped_samples, 0);

    let pitch = analysis.pitch.expect("a pitch");
    assert!((pitch.hz - 440.0).abs() < 1.0, "{pitch:?}");
    assert_eq!((pitch.midi_note, pitch.cents.abs() <= 3), (69, true));
    assert!(pitch.clarity > 0.99 && pitch.hnr_db > 20.0, "{pitch:?}");

    let envelope = analysis.envelope.unwrap();
    assert!(envelope.attack_ms <= 20.0, "{envelope:?}");
    assert!(envelope.sustain_db.abs() < 0.5 && envelope.decay_ms == 0.0);
    // 40 dB of the 60 dB per 0.3 s fall.
    let release = envelope.release_ms.expect("released within the render");
    assert!((release - 200.0).abs() <= 20.0, "{envelope:?}");

    let stereo = analysis.stereo.unwrap();
    assert_eq!(stereo.correlation, 1.0);
    assert!(stereo.side_to_mid_db < -100.0);

    let spectrum = analysis.spectrum.unwrap();
    assert!(
        (spectrum.centroid_hz[2].unwrap() - 440.0).abs() < 60.0,
        "{spectrum:?}"
    );
    // Band 6 spans 40 Hz * 400^(6/16) = 378 Hz to 40 Hz * 400^(7/16) = 548 Hz.
    let row = spectrum.spectrogram[2].as_bytes();
    let loudest = (0..row.len()).max_by_key(|&i| row[i]).unwrap();
    assert_eq!(row[loudest], b'9');
    assert_eq!(loudest, 6, "{:?}", spectrum.spectrogram[2]);
}

#[test]
fn a_slow_attack_and_a_decay_to_sustain_are_timed() {
    // 500 ms linear attack to 0.8, a 300 ms fall to 0.4 (-6 dB), held.
    let envelope = |t: f64| {
        if t < 0.5 {
            0.8 * t / 0.5
        } else if t < 0.8 {
            0.8 - 0.4 * (t - 0.5) / 0.3
        } else {
            0.4
        }
    };
    let signal = tone(220.0, 2.0, envelope);
    let analysis = analyze(&signal, &signal, RATE, seconds(2.0));
    let envelope = analysis.envelope.unwrap();
    // Within 3 dB of the peak at 0.8 / sqrt(2): 71% of the ramp.
    assert!((envelope.attack_ms - 350.0).abs() <= 30.0, "{envelope:?}");
    assert!((envelope.sustain_db + 6.0).abs() < 0.5, "{envelope:?}");
    assert!((envelope.decay_ms - 230.0).abs() <= 60.0, "{envelope:?}");
    assert_eq!(envelope.release_ms, None, "still sounding at the end");
}

#[test]
fn noise_has_no_pitch_a_high_centroid_and_independent_channels_are_wide() {
    let left = noise(seconds(1.0), 1);
    let right = noise(seconds(1.0), 2);
    let analysis = analyze(&left, &right, RATE, seconds(0.5));
    assert!(analysis.pitch.is_none(), "{:?}", analysis.pitch);
    let spectrum = analysis.spectrum.unwrap();
    assert!(
        spectrum.centroid_hz.iter().all(|hz| hz.unwrap() > 8_000.0),
        "{spectrum:?}"
    );
    let stereo = analysis.stereo.unwrap();
    assert!(stereo.correlation.abs() < 0.05, "{stereo:?}");
    assert!(stereo.side_to_mid_db.abs() < 0.5, "{stereo:?}");
}

#[test]
fn inverted_channels_correlate_negatively() {
    let left = tone(330.0, 0.5, |_| 0.3);
    let right: Vec<f32> = left.iter().map(|x| -x).collect();
    let stereo = analyze(&left, &right, RATE, seconds(0.5)).stereo.unwrap();
    assert_eq!(stereo.correlation, -1.0);
}

#[test]
fn silence_is_reported_without_features() {
    let silence = vec![0.0f32; seconds(1.0)];
    let analysis = analyze(&silence, &silence, RATE, seconds(0.5));
    assert!(analysis.silent);
    assert!(analysis.envelope.is_none() && analysis.pitch.is_none());
    assert!(analysis.spectrum.is_none() && analysis.stereo.is_none());
}

#[test]
fn clipping_and_non_finite_samples_are_counted() {
    let mut signal = tone(440.0, 0.5, |_| 1.5);
    signal[100] = f32::NAN;
    signal[200] = f32::INFINITY;
    let analysis = analyze(&signal, &signal, RATE, seconds(0.5));
    assert_eq!(analysis.level.non_finite_samples, 4);
    assert!(analysis.level.clipped_samples > 1000);
    assert!(analysis.level.peak_db > 3.0);
}

#[test]
fn the_analysis_is_compact() {
    let signal = tone(440.0, 3.0, plucked_envelope);
    let analysis = analyze(&signal, &signal, RATE, seconds(1.0));
    let text = serde_json::to_string(&analysis).unwrap();
    // About 300 tokens at roughly 4 characters per token.
    assert!(text.len() < 1300, "{} chars: {text}", text.len());
}
