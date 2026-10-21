//! Bounded comparison of JSON state artifacts (owner decision of
//! 2026-10-21, migration/decisions/owner-decisions-json-and-morphs.md).
//!
//! Structure must match exactly: object keys (order ignored), value types
//! (null, bool, integer, float, string, array, object), strings, integers
//! and array lengths. Floats must agree within the bound: `ulp` f32 ulp
//! (after rounding both to f32) or `abs` absolute, whichever is looser;
//! -0.0 equals 0.0. String values under a declared base64-f32 key are
//! decoded as little-endian f32 arrays, which must have the same length and
//! agree element-wise under the same bound. Mirrored in run_corpus.py
//! (`json_bound_problems`).

use serde_json::Value;

/// Float bound for JSON artifacts: `ulp` f32 ulp or `abs` absolute.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JsonBound {
    pub ulp: u64,
    pub abs: f64,
}

impl Default for JsonBound {
    fn default() -> Self {
        crate::cases::JSON_BOUND
    }
}

/// Maps f32 bits to integers whose difference is the ulp distance (-0.0 and
/// 0.0 both map to 0).
fn order(bits: u32) -> i64 {
    if bits & 0x8000_0000 != 0 {
        -i64::from(bits & 0x7fff_ffff)
    } else {
        i64::from(bits)
    }
}

fn ulp_distance(a: u32, b: u32) -> u64 {
    order(a).abs_diff(order(b))
}

fn f32_close(a: f32, b: f32, bound: &JsonBound) -> bool {
    if a.to_bits() == b.to_bits() || a == b {
        return true;
    }
    if !(a.is_finite() && b.is_finite()) {
        return false;
    }
    (f64::from(a) - f64::from(b)).abs() <= bound.abs
        || ulp_distance(a.to_bits(), b.to_bits()) <= bound.ulp
}

/// JSON floats (doubles): equal, within `abs`, or within `ulp` after
/// rounding both to f32.
pub fn number_close(a: f64, b: f64, bound: &JsonBound) -> bool {
    if a == b {
        return true;
    }
    if !(a.is_finite() && b.is_finite()) {
        return false;
    }
    if (a - b).abs() <= bound.abs {
        return true;
    }
    let (fa, fb) = (a as f32, b as f32);
    fa.is_finite() && fb.is_finite() && ulp_distance(fa.to_bits(), fb.to_bits()) <= bound.ulp
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "integer",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Strict standard-alphabet base64 with `=` padding (as JUCE writes it).
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index + 1 == bytes.len() / 4;
        let padding = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if padding > 2 || (padding > 0 && !last) {
            return None;
        }
        let mut word = 0u32;
        for &c in &chunk[..4 - padding] {
            word = (word << 6) | value(c)?;
        }
        word <<= 6 * padding as u32;
        let decoded = [(word >> 16) as u8, (word >> 8) as u8, word as u8];
        out.extend_from_slice(&decoded[..3 - padding]);
    }
    Some(out)
}

fn decode_f32(text: &str) -> Option<Vec<f32>> {
    let raw = decode_base64(text)?;
    if !raw.len().is_multiple_of(4) {
        return None;
    }
    Some(
        raw.as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect(),
    )
}

fn base64_f32_diff(
    reference: &str,
    port: &str,
    bound: &JsonBound,
    path: &str,
    problems: &mut Vec<String>,
) {
    let (Some(expected), Some(actual)) = (decode_f32(reference), decode_f32(port)) else {
        let which = if decode_f32(reference).is_none() {
            "reference"
        } else {
            "port"
        };
        problems.push(format!(
            "{path}: {which} value is not base64 of little-endian f32"
        ));
        return;
    };
    if expected.len() != actual.len() {
        problems.push(format!(
            "{path}: {} f32 values in the reference, {} in the port",
            expected.len(),
            actual.len()
        ));
        return;
    }
    for (index, (a, b)) in expected.iter().zip(&actual).enumerate() {
        if !f32_close(*a, *b, bound) {
            problems.push(format!(
                "{path}[{index}]: reference {a:?}, port {b:?} ({} ulp, abs {:.3e})",
                ulp_distance(a.to_bits(), b.to_bits()),
                (f64::from(*a) - f64::from(*b)).abs()
            ));
        }
    }
}

fn diff(
    reference: &Value,
    port: &Value,
    bound: &JsonBound,
    keys: &[String],
    path: &str,
    problems: &mut Vec<String>,
) {
    let expected_kind = kind(reference);
    if expected_kind != kind(port) {
        problems.push(format!(
            "{path}: reference is {expected_kind}, port is {}",
            kind(port)
        ));
        return;
    }
    match (reference, port) {
        (Value::Object(a), Value::Object(b)) => {
            let mut names: Vec<&String> = a.keys().chain(b.keys()).collect();
            names.sort();
            names.dedup();
            for name in names {
                let sub = format!("{path}.{name}");
                match (a.get(name), b.get(name)) {
                    (Some(_), None) => problems.push(format!("{sub}: missing in the port")),
                    (None, Some(_)) => problems.push(format!("{sub}: not in the reference")),
                    (Some(Value::String(x)), Some(Value::String(y)))
                        if keys.iter().any(|k| k == name) =>
                    {
                        base64_f32_diff(x, y, bound, &sub, problems)
                    }
                    (Some(x), Some(y)) => diff(x, y, bound, keys, &sub, problems),
                    (None, None) => unreachable!(),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                problems.push(format!(
                    "{path}: array length {} in the reference, {} in the port",
                    a.len(),
                    b.len()
                ));
                return;
            }
            for (index, (x, y)) in a.iter().zip(b).enumerate() {
                diff(x, y, bound, keys, &format!("{path}[{index}]"), problems);
            }
        }
        (Value::Number(a), Value::Number(b)) if expected_kind == "float" => {
            let (x, y) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            if !number_close(x, y, bound) {
                problems.push(format!("{path}: reference {x:?}, port {y:?}"));
            }
        }
        _ => {
            if reference != port {
                problems.push(format!("{path}: reference {reference}, port {port}"));
            }
        }
    }
}

/// Differences between two JSON documents under `bound` (empty if they
/// match). `base64_f32_keys` names the object keys whose string values are
/// base64 little-endian f32 arrays.
pub fn problems(
    reference: &[u8],
    port: &[u8],
    bound: &JsonBound,
    base64_f32_keys: &[String],
) -> Vec<String> {
    let parse = |name: &str, bytes: &[u8]| {
        serde_json::from_slice::<Value>(bytes).map_err(|e| format!("{name} is not valid JSON: {e}"))
    };
    let (expected, actual) = match (parse("reference", reference), parse("port", port)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return vec![e],
    };
    let mut problems = Vec::new();
    diff(
        &expected,
        &actual,
        bound,
        base64_f32_keys,
        "$",
        &mut problems,
    );
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn moved(value: f32, ulps: i32) -> f32 {
        f32::from_bits((value.to_bits() as i32 + ulps) as u32)
    }

    fn b64(values: &[f32]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let word = chunk
                .iter()
                .enumerate()
                .fold(0u32, |w, (i, &b)| w | (u32::from(b) << (16 - 8 * i)));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ALPHABET[((word >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn check(reference: Value, port: Value, keys: &[&str]) -> Vec<String> {
        let keys: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        problems(
            reference.to_string().as_bytes(),
            port.to_string().as_bytes(),
            &JsonBound::default(),
            &keys,
        )
    }

    #[test]
    fn base64_round_trips() {
        for values in [&[][..], &[1.5][..], &[0.25, -6.7][..], &[1.0, 2.0, 3.0][..]] {
            assert_eq!(decode_f32(&b64(values)).unwrap(), values);
        }
        assert!(decode_base64("AAA").is_none());
        assert!(decode_base64("A=AA").is_none());
        assert!(decode_base64("AA*A").is_none());
        assert_eq!(decode_base64("AAE=").unwrap(), [0, 1]);
    }

    #[test]
    fn five_ulp_drift_fails_and_four_passes() {
        // Above |x| = 4 the ulp term is the looser one (4 ulp > 1e-6).
        for value in [1000.3f32, -6.5, 4.0] {
            assert!(check(json!({ "x": value }), json!({ "x": moved(value, 4) }), &[]).is_empty());
            let problems = check(json!({ "x": value }), json!({ "x": moved(value, 5) }), &[]);
            assert_eq!(problems.len(), 1, "{value}: {problems:?}");
            assert!(problems[0].starts_with("$.x"), "{problems:?}");
        }
    }

    #[test]
    fn absolute_floor_is_looser_below_four() {
        assert!(check(json!({ "x": 0.3f32 }), json!({ "x": moved(0.3, 5) }), &[]).is_empty());
        let x = f64::from(0.3f32);
        assert!(!check(json!({ "x": x }), json!({ "x": x + 1.01e-6 }), &[]).is_empty());
        assert!(check(json!({ "x": 1e-30 }), json!({ "x": 1e-30 + 7.8e-7 }), &[]).is_empty());
        assert!(!check(json!({ "x": 1e-30 }), json!({ "x": 1e-30 + 1.1e-6 }), &[]).is_empty());
    }

    #[test]
    fn negative_zero_equals_zero() {
        assert!(check(
            json!({ "x": [-0.0, 0.0] }),
            json!({ "x": [0.0, -0.0] }),
            &[]
        )
        .is_empty());
        assert!(check(
            json!({ "w": b64(&[-0.0]) }),
            json!({ "w": b64(&[0.0]) }),
            &["w"]
        )
        .is_empty());
    }

    #[test]
    fn structure_types_integers_and_strings_are_exact() {
        let reference = json!({ "a": 1, "b": "s", "c": [1.0, 2.0], "d": true, "e": null });
        assert!(check(reference.clone(), reference.clone(), &[]).is_empty());
        for (key, value) in [
            ("a", json!(2)),
            ("a", json!(1.0)),
            ("b", json!("t")),
            ("c", json!([1.0])),
            ("d", json!(false)),
            ("e", json!(0)),
            ("f", json!(1)),
        ] {
            let mut port = reference.clone();
            port[key] = value.clone();
            assert!(
                !check(reference.clone(), port, &[]).is_empty(),
                "{key}: {value}"
            );
        }
        let mut missing = reference.clone();
        missing.as_object_mut().unwrap().remove("a");
        assert!(!check(reference, missing, &[]).is_empty());
    }

    #[test]
    fn formatting_and_key_order_do_not_count() {
        let bound = JsonBound::default();
        assert!(problems(
            br#"{"a": 1, "b": 0.5}"#,
            b"{\n  \"b\": 5e-1,\n  \"a\": 1\n}\n",
            &bound,
            &[]
        )
        .is_empty());
        assert!(!problems(br#"{"a": 1}"#, br#"{"a": NaN}"#, &bound, &[]).is_empty());
    }

    #[test]
    fn base64_f32_one_ulp_passes_five_fails() {
        let values = [0.25f32, -6.7, 0.3];
        let reference = json!({ "wave_data": b64(&values) });
        let one: Vec<f32> = values.iter().map(|v| moved(*v, 1)).collect();
        assert!(check(
            reference.clone(),
            json!({ "wave_data": b64(&one) }),
            &["wave_data"]
        )
        .is_empty());

        let drift = [values[0], moved(values[1], 5), values[2]];
        let problems = check(
            reference.clone(),
            json!({ "wave_data": b64(&drift) }),
            &["wave_data"],
        );
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with("$.wave_data[1]"), "{problems:?}");
        assert!(problems[0].contains("5 ulp"), "{problems:?}");

        let shorter = check(
            reference.clone(),
            json!({ "wave_data": b64(&values[..2]) }),
            &["wave_data"],
        );
        assert!(
            shorter[0].contains("3 f32 values in the reference, 2 in the port"),
            "{shorter:?}"
        );
        // Undeclared, the same strings compare exactly.
        assert!(!check(reference, json!({ "wave_data": b64(&one) }), &[]).is_empty());
    }

    #[test]
    fn changed_pcm_byte_fails() {
        let pcm: Vec<u8> = [0i16, 1000, -1000, 32767]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let as_f32 = |bytes: &[u8]| -> Vec<f32> {
            bytes
                .chunks(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect()
        };
        let mut changed = pcm.clone();
        changed[2] ^= 1;
        // Encode the PCM bytes through the f32 helper (8 bytes = 2 "floats").
        let reference = json!({ "audio_file": b64(&as_f32(&pcm)), "wave_data": b64(&[0.5]) });
        let port = json!({ "audio_file": b64(&as_f32(&changed)), "wave_data": b64(&[0.5]) });
        let problems = check(reference, port, &["wave_data"]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with("$.audio_file"), "{problems:?}");
    }
}
