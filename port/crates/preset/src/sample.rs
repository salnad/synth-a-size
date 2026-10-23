// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preset's sample (the sample oscillator's audio).
//!
//! Ports `Sample::init`, `Sample::jsonToState` and `Sample::stateToJson` from
//! `vital/src/synthesis/producers/sample_source.cpp`.

use sas_common::json::Value;
use sas_common::scalar::{float_to_pcm, pcm_to_float};
use sas_common::{base64, RandomGenerator};

use crate::json::{read_i32, read_string};
use crate::Error;

/// `Sample::kBufferSamples`: zero padding before the audio in the stored
/// buffer.
const BUFFER_SAMPLES: usize = 4;
/// `kMaxSize` in `Sample::loadSample` (mono only).
const MAX_MONO_LENGTH: usize = 1_764_000;
/// `Sample::kDefaultSampleLength`, `kDefaultSampleRate`, `kDefaultName`.
const DEFAULT_LENGTH: usize = 44100;
const DEFAULT_SAMPLE_RATE: i32 = 44100;
const DEFAULT_NAME: &str = "White Noise";

/// The sample as the engine holds it after a load.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SampleState {
    pub name: String,
    pub sample_rate: i32,
    /// Left (or mono) channel.
    pub left: Vec<f32>,
    /// Right channel of a stereo sample.
    pub right: Option<Vec<f32>>,
}

/// Decodes a base64 int16 buffer of `length` frames. Upstream copies `length`
/// frames out of the decoded block whatever its size; frames the text does
/// not cover read as zero here.
fn decode_pcm(text: &str, length: usize) -> Vec<i16> {
    let mut pcm = base64::decode_i16s(text);
    pcm.resize(length, 0);
    pcm
}

/// `Sample::stateToJson` encodes the stored buffer from its start, which
/// includes `kBufferSamples` frames of zero padding: the saved audio is the
/// loaded audio delayed by four frames, with its last four frames dropped.
fn encode_padded(channel: &[f32]) -> String {
    let mut padded = vec![0.0f32; BUFFER_SAMPLES];
    padded.extend_from_slice(channel);
    padded.truncate(channel.len());
    base64::encode_i16s(&float_to_pcm(&padded))
}

impl SampleState {
    /// `Sample::init`: one second of white noise in [-0.9, 0.9) from the
    /// engine RNG, which upstream seeds with the next value of its global
    /// seed counter.
    pub fn white_noise(seed: i32) -> SampleState {
        let mut random = RandomGenerator::new(-0.9, 0.9, seed);
        let left = (0..DEFAULT_LENGTH).map(|_| random.next()).collect();
        SampleState {
            name: DEFAULT_NAME.to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            left,
            right: None,
        }
    }

    /// `Sample::jsonToState`.
    pub fn from_json(data: &Value) -> Result<SampleState, Error> {
        let name = if data.count("name") != 0 {
            read_string(data, "name")?
        } else {
            String::new()
        };
        let length = read_i32(data, "length")?;
        let sample_rate = read_i32(data, "sample_rate")?;
        let length = usize::try_from(length)
            .map_err(|_| Error::Corrupted(format!("invalid sample length {length}")))?;
        let left = pcm_to_float(&decode_pcm(&read_string(data, "samples")?, length));

        let mut state = if data.count("samples_stereo") != 0 {
            let right = pcm_to_float(&decode_pcm(&read_string(data, "samples_stereo")?, length));
            SampleState {
                name,
                sample_rate,
                left,
                right: Some(right),
            }
        } else {
            SampleState {
                name,
                sample_rate,
                left,
                right: None,
            }
        };
        if state.right.is_none() {
            state.left.truncate(MAX_MONO_LENGTH);
        }
        Ok(state)
    }

    /// Number of frames.
    pub fn length(&self) -> usize {
        self.left.len()
    }

    /// `Sample::stateToJson`, including upstream's four-frame delay (see
    /// `encode_padded`).
    pub fn to_json(&self) -> Value {
        let mut data = Value::object();
        data.set("name", self.name.as_str());
        data.set("length", self.length() as i32);
        data.set("sample_rate", self.sample_rate);
        data.set("samples", encode_padded(&self.left));
        if let Some(right) = &self.right {
            data.set("samples_stereo", encode_padded(right));
        }
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_delays_by_buffer_samples() {
        let state = SampleState {
            name: "t".into(),
            sample_rate: 44100,
            left: vec![0.5; 6],
            right: None,
        };
        let saved = state.to_json();
        let pcm = decode_pcm(saved.get("samples").unwrap().to_str().unwrap(), 6);
        assert_eq!(&pcm[..4], &[0, 0, 0, 0]);
        assert_eq!(pcm[4], (0.5f32 * 32767.0) as i16);
    }

    #[test]
    fn white_noise_is_in_range() {
        let noise = SampleState::white_noise(11);
        assert_eq!(noise.length(), 44100);
        assert!(noise.left.iter().all(|&x| (-0.9..0.9).contains(&x)));
    }
}
