//! Loading WAV and NumPy `.npy` buffers into a common interleaved f32 form.

use std::fs;
use std::path::Path;

/// Interleaved multichannel audio (or any sampled signal) as f32.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub channels: usize,
    pub frames: usize,
    /// Known for WAV input; `None` for `.npy`.
    pub sample_rate: Option<u32>,
    /// `frames * channels` samples, frame-major.
    pub data: Vec<f32>,
}

impl Audio {
    pub fn new(channels: usize, sample_rate: Option<u32>, data: Vec<f32>) -> Self {
        assert!(channels > 0 && data.len().is_multiple_of(channels));
        Self {
            channels,
            frames: data.len() / channels,
            sample_rate,
            data,
        }
    }

    pub fn channel(&self, channel: usize) -> impl Iterator<Item = f32> + '_ {
        self.data
            .iter()
            .skip(channel)
            .step_by(self.channels)
            .copied()
    }

    pub fn truncated(&self, frames: usize) -> Audio {
        let frames = frames.min(self.frames);
        Audio::new(
            self.channels,
            self.sample_rate,
            self.data[..frames * self.channels].to_vec(),
        )
    }
}

#[derive(Debug)]
pub struct LoadError(pub String);

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LoadError {}

/// Loads `.wav` (PCM 8/16/24/32 or IEEE float) or `.npy` (`<f4`, `<f8`,
/// `<i2`, `<i4`; shape `[N]` or `[N, C]`).
pub fn load(path: &Path) -> Result<Audio, LoadError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("npy") => load_npy(path),
        _ => load_wav(path),
    }
}

pub fn load_wav(path: &Path) -> Result<Audio, LoadError> {
    let err = |e: hound::Error| LoadError(format!("{}: {e}", path.display()));
    let mut reader = hound::WavReader::open(path).map_err(err)?;
    let spec = reader.spec();
    let data: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(err)?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f64;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| (v as f64 * scale) as f32))
                .collect::<Result<_, _>>()
                .map_err(err)?
        }
    };
    let channels = spec.channels as usize;
    if !data.len().is_multiple_of(channels) {
        return Err(LoadError(format!("{}: truncated frame", path.display())));
    }
    Ok(Audio::new(channels, Some(spec.sample_rate), data))
}

pub fn load_npy(path: &Path) -> Result<Audio, LoadError> {
    let bytes = fs::read(path).map_err(|e| LoadError(format!("{}: {e}", path.display())))?;
    parse_npy(&bytes).map_err(|e| LoadError(format!("{}: {e}", path.display())))
}

pub fn parse_npy(bytes: &[u8]) -> Result<Audio, String> {
    if bytes.len() < 10 || &bytes[..6] != b"\x93NUMPY" {
        return Err("not a .npy file".into());
    }
    let (header_len, header_start) = match bytes[6] {
        1 => (u16::from_le_bytes([bytes[8], bytes[9]]) as usize, 10),
        2 | 3 => {
            if bytes.len() < 12 {
                return Err("truncated header".into());
            }
            (
                u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize,
                12,
            )
        }
        v => return Err(format!("unsupported .npy version {v}")),
    };
    let header_end = header_start + header_len;
    if bytes.len() < header_end {
        return Err("truncated header".into());
    }
    let header = std::str::from_utf8(&bytes[header_start..header_end])
        .map_err(|_| "header is not UTF-8".to_string())?;

    if !header.contains("'fortran_order': False") {
        return Err("only C-order arrays are supported".into());
    }
    let descr = header_value(header, "'descr':")
        .and_then(|v| v.split('\'').nth(1).map(str::to_string))
        .ok_or("missing descr")?;
    let shape_text = header_value(header, "'shape':").ok_or("missing shape")?;
    let open = shape_text.find('(').ok_or("bad shape")?;
    let close = shape_text.find(')').ok_or("bad shape")?;
    let shape: Vec<usize> = shape_text[open + 1..close]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<usize>().map_err(|_| format!("bad dimension {s}")))
        .collect::<Result<_, _>>()?;

    let (frames, channels) = match shape.as_slice() {
        [n] => (*n, 1),
        [n, c] => (*n, *c),
        _ => return Err(format!("expected shape [N] or [N, C], got {shape:?}")),
    };
    let count = frames * channels;
    let payload = &bytes[header_end..];

    let data: Vec<f32> = match descr.as_str() {
        "<f4" => read_le(payload, count, 4, |b| {
            f32::from_le_bytes([b[0], b[1], b[2], b[3]])
        })?,
        "<f8" => read_le(payload, count, 8, |b| {
            f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32
        })?,
        "<i2" => read_le(payload, count, 2, |b| {
            i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0
        })?,
        "<i4" => read_le(payload, count, 4, |b| {
            (i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64 / 2147483648.0) as f32
        })?,
        other => return Err(format!("unsupported dtype {other}")),
    };
    if channels == 0 {
        return Err("zero channels".into());
    }
    Ok(Audio::new(channels, None, data))
}

fn header_value<'a>(header: &'a str, key: &str) -> Option<&'a str> {
    header.find(key).map(|i| &header[i + key.len()..])
}

fn read_le(
    payload: &[u8],
    count: usize,
    width: usize,
    convert: impl Fn(&[u8]) -> f32,
) -> Result<Vec<f32>, String> {
    if payload.len() < count * width {
        return Err("truncated data".into());
    }
    Ok(payload[..count * width]
        .chunks_exact(width)
        .map(convert)
        .collect())
}

/// Serialises interleaved f32 data as a `.npy` (v1.0, `<f4`, shape `[N, C]`).
pub fn to_npy(audio: &Audio) -> Vec<u8> {
    let mut header = format!(
        "{{'descr': '<f4', 'fortran_order': False, 'shape': ({}, {}), }}",
        audio.frames, audio.channels
    );
    let total = 10 + header.len() + 1;
    header.push_str(&" ".repeat((64 - total % 64) % 64));
    header.push('\n');
    let mut out = Vec::with_capacity(10 + header.len() + audio.data.len() * 4);
    out.extend_from_slice(b"\x93NUMPY\x01\x00");
    out.extend_from_slice(&(header.len() as u16).to_le_bytes());
    out.extend_from_slice(header.as_bytes());
    for sample in &audio.data {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npy_round_trip() {
        let audio = Audio::new(2, None, vec![0.0, 1.0, -0.5, 0.25, 0.125, -1.0]);
        let parsed = parse_npy(&to_npy(&audio)).unwrap();
        assert_eq!(parsed, audio);
    }

    #[test]
    fn npy_one_dimensional() {
        let mut bytes = to_npy(&Audio::new(1, None, vec![0.5, -0.5, 0.25]));
        let start = bytes.windows(6).position(|w| w == b"(3, 1)").unwrap();
        // Rewrite "(3, 1)" as "(3,)  " in place, keeping the header length.
        bytes[start..start + 6].copy_from_slice(b"(3,)  ");
        let parsed = parse_npy(&bytes).unwrap();
        assert_eq!(parsed.channels, 1);
        assert_eq!(parsed.data, vec![0.5, -0.5, 0.25]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_npy(b"RIFF....").is_err());
    }
}
