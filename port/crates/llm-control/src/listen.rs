// SPDX-License-Identifier: GPL-3.0-or-later

//! Listening to the patch (`audition`, L6 of the LLM plan; D13).
//!
//! The tool renders the current patch on a preview engine, separate from
//! the one the host plays, and gives the model a compact analysis of the
//! result: levels, envelope, pitch, stereo image and a coarse spectrogram.
//! Audio never leaves the renderer. This module holds the request, its
//! limits (which bound the preview's CPU), and the [`Auditioner`] interface;
//! the renderer is in `sas-llm-listen`, since this crate has no engine.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::sas_preset::PresetState;
use crate::tools::ToolSpec;

/// The most notes one audition plays at once.
pub const MAX_NOTES: usize = 8;
/// The longest a note is held, in seconds.
pub const MAX_HOLD_SECONDS: f64 = 4.0;
/// The longest tail rendered after the notes are released, in seconds.
pub const MAX_RELEASE_SECONDS: f64 = 2.0;

/// What to play: `notes` (MIDI note numbers, 60 = middle C) together at
/// `velocity`, held for `hold` seconds, then `release` seconds of tail.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditionRequest {
    pub notes: Vec<u8>,
    pub hold: f64,
    pub release: f64,
    pub velocity: f32,
}

impl Default for AuditionRequest {
    fn default() -> Self {
        AuditionRequest {
            notes: vec![60],
            hold: 1.0,
            release: 1.0,
            velocity: 0.8,
        }
    }
}

impl AuditionRequest {
    /// The request in the tool's arguments, with lengths clamped to the
    /// limits above. Notes outside 0–127, too many notes or a velocity
    /// outside 0–1 are errors.
    pub fn from_args(args: &Value) -> Result<AuditionRequest, String> {
        #[derive(Deserialize)]
        struct Args {
            notes: Option<Vec<i64>>,
            seconds: Option<f64>,
            release: Option<f64>,
            velocity: Option<f32>,
        }
        let args: Args = serde_json::from_value(args.clone())
            .map_err(|error| format!("invalid arguments for audition: {error}"))?;
        let defaults = AuditionRequest::default();
        let notes = match args.notes {
            None => defaults.notes,
            Some(notes) if notes.is_empty() => return Err("notes is empty".to_string()),
            Some(notes) if notes.len() > MAX_NOTES => {
                return Err(format!("at most {MAX_NOTES} notes at once"))
            }
            Some(notes) => notes
                .into_iter()
                .map(|note| {
                    u8::try_from(note)
                        .ok()
                        .filter(|note| *note <= 127)
                        .ok_or_else(|| format!("note {note} is not a MIDI note (0-127)"))
                })
                .collect::<Result<_, _>>()?,
        };
        let velocity = args.velocity.unwrap_or(defaults.velocity);
        if !(0.0..=1.0).contains(&velocity) || velocity.is_nan() {
            return Err("velocity must be between 0 and 1".to_string());
        }
        let clamp = |value: Option<f64>, default: f64, max: f64, min: f64| {
            let value = value.filter(|value| value.is_finite()).unwrap_or(default);
            value.clamp(min, max)
        };
        Ok(AuditionRequest {
            notes,
            hold: clamp(args.seconds, defaults.hold, MAX_HOLD_SECONDS, 0.05),
            release: clamp(args.release, defaults.release, MAX_RELEASE_SECONDS, 0.0),
            velocity,
        })
    }

    /// The length rendered, in seconds.
    pub fn total_seconds(&self) -> f64 {
        self.hold + self.release
    }
}

/// Renders a patch off the audio thread and analyses it.
pub trait Auditioner: Send {
    /// Plays `request` on `state` and returns the analysis (never audio).
    fn audition(&mut self, state: &PresetState, request: &AuditionRequest)
        -> Result<Value, String>;
}

/// The `audition` tool, offered only where an [`Auditioner`] exists.
pub fn audition_spec() -> ToolSpec {
    ToolSpec {
        name: "audition",
        description: "Listen to the current patch: plays notes on a separate preview engine and \
                      returns an analysis (no audio): peak and RMS level, clipping, attack, \
                      decay, sustain and release, pitch and harmonic-to-noise ratio, stereo \
                      correlation and width, spectral centroid and rolloff over time, and a \
                      16-band x 8-slice spectrogram (one digit per band, 0-9 = 60 dB range). \
                      For a chord, the pitch is the chord's common period. Use it to check a \
                      change sounds as intended. Random modulation and noise differ from what \
                      the host plays.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "notes": {
                    "type": "array",
                    "items": {"type": "integer", "minimum": 0, "maximum": 127},
                    "maxItems": MAX_NOTES,
                    "description": "MIDI note numbers played together (60 = middle C); default [60]"
                },
                "seconds": {
                    "type": "number",
                    "description": format!("how long the notes are held (default 1, at most {MAX_HOLD_SECONDS})")
                },
                "release": {
                    "type": "number",
                    "description": format!("seconds rendered after release (default 1, at most {MAX_RELEASE_SECONDS})")
                },
                "velocity": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.8}
            }
        }),
    }
}
