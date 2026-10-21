//! Base64 as JUCE implements it (`juce_core/text/juce_Base64.cpp`), which
//! upstream uses for wave data, audio and sample buffers inside JSON.
//!
//! Encoding is standard base64 with `=` padding. Decoding reads groups of
//! four characters and stops at the first invalid character (including the
//! end of a truncated group), keeping the bytes already decoded, which is
//! what callers get when they ignore JUCE's `false` return.

const LOOKUP: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `Base64::toBase64(data, size)`.
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let byte0 = chunk[0];
        out.push(LOOKUP[((byte0 & 0xfc) >> 2) as usize] as char);
        let bits = (byte0 & 0x03) << 4;
        match chunk.len() {
            1 => {
                out.push(LOOKUP[bits as usize] as char);
                out.push_str("==");
            }
            2 => {
                let byte1 = chunk[1];
                out.push(LOOKUP[(bits | ((byte1 & 0xf0) >> 4)) as usize] as char);
                out.push(LOOKUP[((byte1 & 0x0f) << 2) as usize] as char);
                out.push('=');
            }
            _ => {
                let byte1 = chunk[1];
                let byte2 = chunk[2];
                out.push(LOOKUP[(bits | ((byte1 & 0xf0) >> 4)) as usize] as char);
                out.push(LOOKUP[(((byte1 & 0x0f) << 2) | ((byte2 & 0xc0) >> 6)) as usize] as char);
                out.push(LOOKUP[(byte2 & 0x3f) as usize] as char);
            }
        }
    }
    out
}

/// `Base64::convertFromBase64`: the decoded bytes and whether the whole
/// input was valid.
pub fn decode(text: &str) -> (Vec<u8>, bool) {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut chars = text.chars();
    let mut remaining = !text.is_empty();
    while remaining {
        let mut data = [0u8; 4];
        for (i, slot) in data.iter_mut().enumerate() {
            let c = chars.next().unwrap_or('\0');
            *slot = match c {
                'A'..='Z' => c as u8 - b'A',
                'a'..='z' => c as u8 - b'a' + 26,
                '0'..='9' => c as u8 - b'0' + 52,
                '+' => 62,
                '/' => 63,
                '=' if i > 1 => 64,
                _ => return (out, false),
            };
        }

        out.push((data[0] << 2) | (data[1] >> 4));
        if data[2] < 64 {
            out.push((data[1] << 4) | (data[2] >> 2));
            if data[3] < 64 {
                out.push((data[2] << 6) | data[3]);
            }
        }
        remaining = !chars.as_str().is_empty();
    }
    (out, true)
}

/// Encodes `f32`s as their little-endian bytes (upstream memcpy's the float
/// array on little-endian targets).
pub fn encode_f32s(values: &[f32]) -> String {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    encode(&bytes)
}

/// Encodes `i16`s as their little-endian bytes.
pub fn encode_i16s(values: &[i16]) -> String {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    encode(&bytes)
}

/// Decodes to whole `f32`s; trailing bytes that do not fill a value are
/// dropped, as upstream's `size / sizeof(float)` does.
pub fn decode_f32s(text: &str) -> Vec<f32> {
    let (bytes, _) = decode(text);
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

/// Decodes to whole `i16`s.
pub fn decode_i16s(text: &str) -> Vec<i16> {
    let (bytes, _) = decode(text);
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for len in 0..40 {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let encoded = encode(&data);
            assert_eq!(encoded.len() % 4, 0);
            assert_eq!(decode(&encoded), (data, true));
        }
    }

    #[test]
    fn known_vectors() {
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn invalid_input_keeps_decoded_prefix() {
        assert_eq!(decode("Zm9vYmE"), (b"foo".to_vec(), false));
        assert_eq!(decode("Zm9v!mFy"), (b"foo".to_vec(), false));
        assert_eq!(decode("Z==="), (Vec::new(), false));
    }
}
