//! Shared by the oscillator parity tests: the `{"$wave": ...}` recipe
//! expansion of `migration/reference/src/modules/wavetable/wave_recipes.h`
//! (same as `sas-wavetable`'s test support, whose test modules other crates
//! cannot import), which must produce the same bits as the reference's.

#![allow(dead_code)]

use sas_common::base64;
use sas_common::json::Value;

fn int_or(recipe: &Value, key: &str, default: i32) -> i32 {
    if recipe.count(key) != 0 {
        recipe
            .get(key)
            .and_then(Value::to_i32)
            .expect("recipe integer")
    } else {
        default
    }
}

fn float_or(recipe: &Value, key: &str, default: f32) -> f32 {
    if recipe.count(key) != 0 {
        recipe
            .get(key)
            .and_then(Value::to_f32)
            .expect("recipe float")
    } else {
        default
    }
}

fn str_or<'a>(recipe: &'a Value, key: &str, default: &'a str) -> &'a str {
    if recipe.count(key) != 0 {
        recipe
            .get(key)
            .and_then(Value::to_str)
            .expect("recipe string")
    } else {
        default
    }
}

pub fn samples(recipe: &Value) -> Vec<f32> {
    let length = int_or(recipe, "length", 2048);
    let period = int_or(recipe, "period", length);
    let shift = int_or(recipe, "shift", 0);
    let amp = float_or(recipe, "amp", 1.0);
    let shape = str_or(recipe, "shape", "saw");
    let mut out = vec![0.0f32; length as usize];

    if shape == "mix" {
        let parts = recipe.get("parts").expect("parts");
        let weights = recipe.get("weights").expect("weights");
        for (p, part) in parts.iter_values().enumerate() {
            let mut part = part.clone();
            part.set("length", length);
            let part_samples = samples(&part);
            let weight = weights.at(p).and_then(Value::to_f32).expect("weight");
            for (value, &sample) in out.iter_mut().zip(&part_samples) {
                *value += weight * sample;
            }
        }
        return out;
    }

    let half = period / 2;
    let duty = int_or(recipe, "duty", half);
    let slope = float_or(recipe, "slope", 1.0 / 1024.0);
    for (i, value) in out.iter_mut().enumerate() {
        let phase = ((i as i32 + shift) % period + period) % period;
        let raw = match shape {
            "saw" => (phase - half) as f32 * slope,
            "square" => {
                if phase < duty {
                    1.0
                } else {
                    -1.0
                }
            }
            "triangle" => {
                let steps = if phase < half {
                    phase * 2 - half
                } else {
                    3 * half - phase * 2
                };
                steps as f32 * slope
            }
            _ => 0.0,
        };
        *value = amp * raw;
    }
    out
}

pub fn encode(recipe: &Value) -> String {
    let values = samples(recipe);
    if str_or(recipe, "encoding", "f32") == "pcm" {
        let pcm: Vec<i16> = values.iter().map(|&v| (v * 32767.0) as i16).collect();
        return base64::encode_i16s(&pcm);
    }
    base64::encode_f32s(&values)
}

/// Replaces every `{"$wave": recipe}` object with its base64 buffer.
pub fn expand(data: &Value) -> Value {
    match data {
        Value::Object(map) => {
            if let Some(recipe) = map.get("$wave") {
                return Value::from(encode(recipe));
            }
            Value::Object(map.iter().map(|(k, v)| (k.clone(), expand(v))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(expand).collect()),
        other => other.clone(),
    }
}
