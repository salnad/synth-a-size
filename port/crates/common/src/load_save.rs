//! The small `LoadSave` helpers that both preset and wavetable JSON upgrades
//! call. Ports parts of `vital/src/common/load_save.cpp` (Copyright 2013-2019
//! Matt Tytel, GPL-3.0-or-later).

use crate::base64;
use crate::json::Value;
use crate::scalar;

/// `ProjectInfo::versionString` of the reference build
/// (`migration/reference/juce_config/JuceHeader.h`), written into saved
/// presets and wavetables.
pub const VERSION_STRING: &str = "1.0.6";

fn up_to_first_dot(s: &str) -> &str {
    s.find('.').map_or(s, |i| &s[..i])
}

fn from_first_dot(s: &str) -> &str {
    s.find('.').map_or("", |i| &s[i + 1..])
}

/// JUCE `String::getIntValue` on a string of ASCII digits (wrapping on
/// overflow).
fn int_value(s: &str) -> i32 {
    s.bytes().take_while(u8::is_ascii_digit).fold(0i32, |v, d| {
        v.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
    })
}

/// `LoadSave::compareVersionStrings(a, b)`: -1, 0 or 1 comparing
/// dot-separated numeric components; a non-numeric component counts as 0.
/// Upstream's `trim()` calls discard their result, so whitespace is not
/// trimmed here either.
pub fn compare_version_strings(a: &str, b: &str) -> i32 {
    if a.is_empty() && b.is_empty() {
        return 0;
    }

    let numeric = |s: &str| -> i32 {
        if s.bytes().all(|c| c.is_ascii_digit()) {
            int_value(s)
        } else {
            0
        }
    };
    let major_a = numeric(up_to_first_dot(a));
    let major_b = numeric(up_to_first_dot(b));

    if major_a > major_b {
        1
    } else if major_a < major_b {
        -1
    } else {
        compare_version_strings(from_first_dot(a), from_first_dot(b))
    }
}

/// `LoadSave::convertBufferToPcm(data, field)`: re-encodes a base64 float
/// buffer as base64 16-bit PCM, in place. Does nothing if `field` is
/// missing.
pub fn convert_buffer_to_pcm(data: &mut Value, field: &str) -> crate::json::Result<()> {
    if data.count(field) == 0 {
        return Ok(());
    }
    let encoded = data.get(field)?.to_str()?;
    let floats = base64::decode_f32s(encoded);
    let pcm = scalar::float_to_pcm(&floats);
    data.set(field, base64::encode_i16s(&pcm));
    Ok(())
}

/// `LoadSave::convertPcmToFloatBuffer(data, field)`: the inverse of
/// [`convert_buffer_to_pcm`].
pub fn convert_pcm_to_float_buffer(data: &mut Value, field: &str) -> crate::json::Result<()> {
    if data.count(field) == 0 {
        return Ok(());
    }
    let encoded = data.get(field)?.to_str()?;
    let pcm = base64::decode_i16s(encoded);
    let floats = scalar::pcm_to_float(&pcm);
    data.set(field, base64::encode_f32s(&floats));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert_eq!(compare_version_strings("1.0.6", "0.3.3"), 1);
        assert_eq!(compare_version_strings("0.3.3", "0.3.3"), 0);
        assert_eq!(compare_version_strings("0.3.2", "0.3.10"), -1);
        assert_eq!(compare_version_strings("0.0.0", "0.3.7"), -1);
        assert_eq!(compare_version_strings("1.0", "1.0.0"), 0);
        assert_eq!(compare_version_strings("1.x", "1.0"), 0);
        assert_eq!(compare_version_strings("", "0.1"), -1);
        assert_eq!(compare_version_strings(" 1.0", "0.9"), -1);
    }

    #[test]
    fn pcm_round_trip() {
        let mut data = Value::Null;
        data.set("buf", base64::encode_f32s(&[0.5, -1.0, 0.25]));
        convert_buffer_to_pcm(&mut data, "buf").unwrap();
        assert_eq!(
            base64::decode_i16s(data.get("buf").unwrap().to_str().unwrap()),
            vec![16383, -32767, 8191]
        );
        convert_pcm_to_float_buffer(&mut data, "buf").unwrap();
        let floats = base64::decode_f32s(data.get("buf").unwrap().to_str().unwrap());
        assert_eq!(floats[1], -1.0);
        convert_buffer_to_pcm(&mut data, "missing").unwrap();
    }
}
