//! LFO shapes and wavetables built from a model's description, in the
//! upstream state formats (`LineGenerator` and `WavetableCreator` JSON), so
//! the result loads and renders in the reference exactly as in the port.
//!
//! Wavetables are generated here, never taken from factory content
//! (migration/AGENTS.md rule 3).

use std::f64::consts::TAU;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::json;

use sas_common::{base64, json as upstream_json, LineGenerator};
use sas_preset::constants::NUM_OSCILLATOR_WAVE_FRAMES;
use sas_preset::{InitContent, PresetState};
use sas_wavetable::creator::WavetableCreator;
use sas_wavetable::wave_frame::WAVEFORM_SIZE;

/// Named LFO shapes, as upstream's LFO editor menu builds them.
pub const LFO_SHAPES: &[&str] = &["sine", "triangle", "square", "saw_up", "saw_down"];

/// Basic waveforms a wavetable frame can start from.
pub const WAVE_SHAPES: &[&str] = &["sine", "saw", "square", "triangle"];

/// Highest harmonic a 2048-sample frame can hold.
pub const MAX_HARMONICS: usize = WAVEFORM_SIZE / 2;

/// Last keyframe position (frames are numbered 0..=256).
pub const LAST_FRAME: i32 = NUM_OSCILLATOR_WAVE_FRAMES - 1;

/// An LFO shape by name.
pub fn named_lfo(shape: &str) -> Result<LineGenerator, String> {
    let mut line = LineGenerator::default();
    match shape {
        "sine" => line.init_sin(),
        "triangle" => line.init_triangle(),
        "square" => line.init_square(),
        "saw_up" => line.init_saw_up(),
        "saw_down" => line.init_saw_down(),
        _ => {
            return Err(format!(
                "unknown LFO shape {shape:?}; shapes: {}",
                LFO_SHAPES.join(", ")
            ))
        }
    }
    Ok(line)
}

/// An LFO shape through `points` (x = phase 0..1, y = value 0 bottom ..
/// 1 top), with per-segment curve `powers`. Upstream stores y inverted
/// (0 is the top), which this converts to.
pub fn lfo_from_points(
    points: &[(f32, f32)],
    powers: Option<&[f32]>,
    smooth: bool,
    name: &str,
) -> Result<LineGenerator, String> {
    if !(2..=LineGenerator::MAX_POINTS).contains(&points.len()) {
        return Err(format!(
            "an LFO shape needs 2 to {} points, not {}",
            LineGenerator::MAX_POINTS,
            points.len()
        ));
    }
    // Upstream's LineEditor pins the first point to x = 0 and the last to
    // x = 1 (`getMinX` / `getMaxX`).
    if points[0].0 != 0.0 || points[points.len() - 1].0 != 1.0 {
        return Err("the first point must be at x = 0 and the last at x = 1".to_string());
    }
    let mut last_x = 0.0f32;
    for &(x, y) in points {
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return Err(format!("point ({x}, {y}) is outside 0..1"));
        }
        if x < last_x {
            return Err("point x values must not decrease".to_string());
        }
        last_x = x;
    }
    let powers: Vec<f32> = match powers {
        Some(powers) if powers.len() != points.len() => {
            return Err(format!(
                "{} powers for {} points; give one per point",
                powers.len(),
                points.len()
            ))
        }
        Some(powers) => powers.to_vec(),
        None => vec![0.0; points.len()],
    };
    if let Some(power) = powers.iter().find(|p| !p.is_finite() || p.abs() > 20.0) {
        return Err(format!("power {power} is outside -20..20"));
    }
    let flat: Vec<f32> = points.iter().flat_map(|&(x, y)| [x, 1.0 - y]).collect();
    let data = json!({
        "name": name,
        "num_points": points.len(),
        "points": flat,
        "powers": powers,
        "smooth": smooth,
    });
    let mut line = LineGenerator::default();
    line.from_json(&parse(&data)?).map_err(|e| e.to_string())?;
    Ok(line)
}

/// One keyframe of a generated wavetable: a basic shape, or a harmonic
/// spectrum (amplitude of harmonic 1, 2, ...), at a frame position.
#[derive(Clone, Debug, Deserialize)]
pub struct FrameSpec {
    pub position: i32,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub harmonics: Option<Vec<f32>>,
}

/// One cycle of `spec`, normalized to a peak of 1.
fn frame_samples(spec: &FrameSpec) -> Result<Vec<f32>, String> {
    let n = WAVEFORM_SIZE;
    let samples: Vec<f64> = match (&spec.shape, &spec.harmonics) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(format!(
                "frame at {}: give either a shape or harmonics",
                spec.position
            ))
        }
        (Some(shape), None) => (0..n)
            .map(|i| {
                let phase = i as f64 / n as f64;
                match shape.as_str() {
                    "sine" => Ok((TAU * phase).sin()),
                    "saw" => Ok(2.0 * phase - 1.0),
                    "square" => Ok(if phase < 0.5 { 1.0 } else { -1.0 }),
                    "triangle" => Ok(1.0 - 4.0 * (phase - 0.5).abs()),
                    _ => Err(format!(
                        "unknown wave shape {shape:?}; shapes: {}",
                        WAVE_SHAPES.join(", ")
                    )),
                }
            })
            .collect::<Result<_, _>>()?,
        (None, Some(harmonics)) => {
            if harmonics.is_empty() || harmonics.len() > MAX_HARMONICS {
                return Err(format!("give 1 to {MAX_HARMONICS} harmonic amplitudes"));
            }
            if harmonics.iter().any(|a| !a.is_finite()) {
                return Err("harmonic amplitudes must be finite".to_string());
            }
            (0..n)
                .map(|i| {
                    let phase = TAU * i as f64 / n as f64;
                    harmonics
                        .iter()
                        .enumerate()
                        .map(|(k, &a)| a as f64 * (phase * (k + 1) as f64).sin())
                        .sum()
                })
                .collect()
        }
    };
    let peak = samples.iter().fold(0.0f64, |m, s| m.max(s.abs()));
    if peak <= 1e-9 {
        return Err(format!("frame at {} is silent", spec.position));
    }
    Ok(samples.iter().map(|s| (s / peak) as f32).collect())
}

/// The init wavetable's format version, which generated wavetables carry.
fn wavetable_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let init = PresetState::new(&InitContent { sample_seed: 0 });
        let dump = init.wavetables[0].to_json().dump();
        let value: serde_json::Value = serde_json::from_str(&dump).expect("valid JSON");
        value["version"].as_str().unwrap_or_default().to_string()
    })
}

/// A wavetable whose keyframes are `frames`, morphed in the frequency
/// domain like the init wavetable's single Wave Source.
pub fn wavetable(name: &str, frames: &[FrameSpec]) -> Result<WavetableCreator, String> {
    if frames.is_empty() || frames.len() > 64 {
        return Err("give 1 to 64 frames".to_string());
    }
    let mut frames: Vec<&FrameSpec> = frames.iter().collect();
    frames.sort_by_key(|frame| frame.position);
    let mut keyframes = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        if !(0..=LAST_FRAME).contains(&frame.position) {
            return Err(format!(
                "frame position {} is outside 0..{LAST_FRAME}",
                frame.position
            ));
        }
        if index > 0 && frames[index - 1].position == frame.position {
            return Err(format!("two frames at position {}", frame.position));
        }
        keyframes.push(json!({
            "position": frame.position,
            "wave_data": base64::encode_f32s(&frame_samples(frame)?),
        }));
    }
    let data = json!({
        "author": "",
        "full_normalize": true,
        "groups": [{"components": [{
            "interpolation": 1,
            "interpolation_style": 1,
            "keyframes": keyframes,
            "type": "Wave Source",
        }]}],
        "name": name,
        "remove_all_dc": true,
        "version": wavetable_version(),
    });
    WavetableCreator::from_json(&parse(&data)?).map_err(|e| e.to_string())
}

fn parse(data: &serde_json::Value) -> Result<upstream_json::Value, String> {
    upstream_json::parse(&data.to_string()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_shapes_match_the_line_generator() {
        let mut saw = LineGenerator::default();
        saw.init_saw_up();
        assert_eq!(
            named_lfo("saw_up").unwrap().to_json().dump(),
            saw.to_json().dump()
        );
        assert!(named_lfo("wobble").unwrap_err().contains("sine"));
    }

    #[test]
    fn points_are_stored_with_upstream_y() {
        let line = lfo_from_points(&[(0.0, 0.0), (1.0, 1.0)], None, false, "ramp").unwrap();
        assert_eq!(line.point(0), (0.0, 1.0));
        assert_eq!(line.point(1), (1.0, 0.0));
        // Upstream's "Saw Up" rises the same way.
        let mut saw = LineGenerator::default();
        saw.init_saw_up();
        assert_eq!(saw.point(0), (0.0, 1.0));
        assert!(lfo_from_points(
            &[(0.0, 0.0), (0.6, 0.5), (0.2, 1.0), (1.0, 0.0)],
            None,
            false,
            ""
        )
        .is_err());
        assert!(lfo_from_points(&[(0.0, 2.0), (1.0, 1.0)], None, false, "").is_err());
        assert!(lfo_from_points(&[(0.0, 0.0)], None, false, "").is_err());
        assert!(lfo_from_points(&[(0.0, 0.0), (1.0, 1.0)], Some(&[1.0]), false, "").is_err());
        assert!(lfo_from_points(&[(0.1, 0.0), (1.0, 1.0)], None, false, "").is_err());
        assert!(lfo_from_points(&[(0.0, 0.0), (0.9, 1.0)], None, false, "").is_err());
    }

    #[test]
    fn generated_wavetables_load_as_upstream_json() {
        let frames = [
            FrameSpec {
                position: 0,
                shape: Some("sine".into()),
                harmonics: None,
            },
            FrameSpec {
                position: 256,
                shape: None,
                harmonics: Some(vec![1.0, 0.5, 0.33]),
            },
        ];
        let creator = wavetable("test", &frames).unwrap();
        assert_eq!(creator.name, "test");
        assert_eq!(creator.last_keyframe_position(), 256);
        let dump = creator.to_json().dump();
        let reloaded = WavetableCreator::from_json(&upstream_json::parse(&dump).unwrap()).unwrap();
        assert_eq!(reloaded.to_json().dump(), dump);
    }

    #[test]
    fn bad_frames_are_refused() {
        let frame = |position, shape: Option<&str>, harmonics: Option<Vec<f32>>| FrameSpec {
            position,
            shape: shape.map(str::to_string),
            harmonics,
        };
        assert!(wavetable("", &[]).is_err());
        assert!(wavetable("", &[frame(300, Some("sine"), None)]).is_err());
        assert!(wavetable(
            "",
            &[frame(0, Some("sine"), None), frame(0, Some("saw"), None)]
        )
        .is_err());
        assert!(wavetable("", &[frame(0, None, None)]).is_err());
        assert!(wavetable("", &[frame(0, None, Some(vec![0.0]))]).is_err());
        assert!(wavetable("", &[frame(0, Some("noise"), None)]).is_err());
    }
}
