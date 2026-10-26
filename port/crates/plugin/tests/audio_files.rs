// SPDX-License-Identifier: GPL-3.0-or-later

//! The audio files a wavetable view takes, read from the authored corpus
//! files (`migration/corpus/audio`, written by `generate.py`). The same
//! tones are written in every lossless format, so each decoder must give
//! exactly the samples the port's WAV reader gives; the reference's JUCE
//! readers are compared in `audio_reference.rs`.

use std::path::{Path, PathBuf};

use sas_plugin::audio_file::{read_audio, AUDIO_EXTENSIONS};

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../migration/corpus/audio")
        .join(name)
}

fn read(name: &str) -> (Vec<f32>, i32) {
    let path = corpus(name);
    let bytes = std::fs::read(&path).unwrap();
    let extension = path.extension().unwrap().to_str().unwrap();
    let audio = read_audio(&bytes, extension).unwrap_or_else(|| panic!("{name} didn't decode"));
    (audio.first_channel, audio.sample_rate)
}

#[test]
fn lossless_formats_decode_to_the_same_samples() {
    let wav16 = read("wav_pcm16_stereo.wav");
    assert_eq!(wav16.1, 44_100);
    assert_eq!(wav16.0.len(), 8192);
    assert_eq!(read("aiff_pcm16_stereo.aiff"), wav16);
    assert_eq!(read("flac_16_stereo.flac"), wav16);

    let wav24 = read("wav_pcm24_mono.wav");
    assert_eq!(wav24.1, 48_000);
    assert_eq!(read("aiff_pcm24_mono.aif"), wav24);
    assert_eq!(read("flac_24_mono.flac"), wav24);
}

#[test]
fn every_authored_file_decodes_its_first_channel() {
    for name in [
        "wav_pcm8_mono.wav",
        "wav_pcm32_mono.wav",
        "wav_float32_stereo.wav",
        "wav_extensible_pcm24.wav",
        "wav_truncated_data.wav",
    ] {
        let (samples, rate) = read(name);
        assert_eq!(rate, 44_100, "{name}");
        assert_eq!(samples.len(), 8192, "{name}");
        // The tone's left channel peaks near 0.85; the right one is another tone.
        let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert!(peak > 0.5 && peak <= 1.0, "{name}: {peak}");
    }
    // The truncated file's second half is zero-filled.
    let (samples, _) = read("wav_truncated_data.wav");
    assert!(samples[4096..].iter().all(|&s| s == 0.0));
    assert!(samples[..4096].iter().any(|&s| s != 0.0));
}

#[test]
fn every_offered_format_refuses_an_empty_or_corrupt_file() {
    let corrupt: Vec<u8> = (0..4096u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    for extension in AUDIO_EXTENSIONS {
        assert!(read_audio(&[], extension).is_none(), "empty .{extension}");
        assert!(
            read_audio(&corrupt, extension).is_none(),
            "corrupt .{extension}"
        );
    }
    assert!(read_audio(
        &std::fs::read(corpus("wav_pcm16_stereo.wav")).unwrap(),
        "caf"
    )
    .is_none());
}

#[test]
fn the_offered_formats_are_upstreams_macos_set_without_caf() {
    for extension in [
        "wav", "bwf", "aif", "aiff", "flac", "ogg", "mp3", "m4a", "aac",
    ] {
        assert!(AUDIO_EXTENSIONS.contains(&extension), "{extension}");
    }
    assert!(!AUDIO_EXTENSIONS.contains(&"caf"));
}
