// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wavetable editor's "Export to .wav File"
//! (`WavetableEditSection::exportToWav`): the wavetable rendered to 256
//! frames and written as JUCE's `WavAudioFormat` writer writes a mono
//! 16-bit file, with the `clm ` chunk that tells wavetable importers the
//! frame size.

use crate::interface::PRODUCT_NAME;

/// `exportToWav`'s `kWavetableSampleRate`.
pub const SAMPLE_RATE: u32 = 88_200;
/// `exportToWav`'s `kNumWaveframes`.
pub const NUM_FRAMES: usize = 256;

/// The `clm ` chunk's text: upstream's, with this product's name in the
/// brackets where upstream names its own site (migration/AGENTS.md rule 2).
/// Importers read the frame size and the fade flag from the start.
pub fn clm_text() -> String {
    format!("<!>2048 20000000 wavetable ({PRODUCT_NAME})")
}

/// JUCE's `AudioData::Int16::setAsFloatLE`: the sample times 32768,
/// rounded as `roundToInt` rounds (halves to even), within ±32767.
fn to_int16(sample: f32) -> i16 {
    let scaled = (f64::from(sample) * 32768.0).round_ties_even();
    scaled.clamp(-32767.0, 32767.0) as i16
}

/// The file `WavAudioFormat::createWriterFor(…, sample_rate, 1, 16,
/// {"clm ": clm}, 0)` writes for `samples`: RIFF, the 52-byte `JUNK`
/// padding that keeps JUCE's header one size, a 16-byte PCM `fmt `, the
/// `clm ` chunk (the text and its terminating zero, padded to an even
/// length) and the samples.
pub fn wav_bytes(samples: &[f32], sample_rate: u32, clm: &str) -> Vec<u8> {
    let mut clm_chunk = clm.as_bytes().to_vec();
    clm_chunk.push(0);
    if clm_chunk.len() % 2 == 1 {
        clm_chunk.push(0);
    }
    let audio_size = samples.len() * 2;
    let chunk_size = |data: &[u8]| if data.is_empty() { 0 } else { 8 + data.len() };
    // WavAudioFormatWriter::writeHeader's riffChunkSize.
    let mut riff_size =
        4 + 8 + 40 + 8 + audio_size + (audio_size & 1) + chunk_size(&clm_chunk) + (8 + 28);
    riff_size += riff_size & 1;

    let mut out = Vec::with_capacity(riff_size + 8);
    let chunk_header = |out: &mut Vec<u8>, name: &[u8; 4], size: u32| {
        out.extend_from_slice(name);
        out.extend_from_slice(&size.to_le_bytes());
    };
    chunk_header(&mut out, b"RIFF", riff_size as u32);
    out.extend_from_slice(b"WAVE");
    chunk_header(&mut out, b"JUNK", 28 + 24);
    out.resize(out.len() + 28 + 24, 0);
    chunk_header(&mut out, b"fmt ", 16);
    let bytes_per_frame: u16 = 2;
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(u32::from(bytes_per_frame) * sample_rate).to_le_bytes());
    out.extend_from_slice(&bytes_per_frame.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    chunk_header(&mut out, b"clm ", clm_chunk.len() as u32);
    out.extend_from_slice(&clm_chunk);
    chunk_header(&mut out, b"data", audio_size as u32);
    for &sample in samples {
        out.extend_from_slice(&to_int16(sample).to_le_bytes());
    }
    if audio_size & 1 == 1 {
        out.push(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// The header JUCE writes for a mono 16-bit file with a `clm ` chunk,
    /// the RIFF size matching the file, and samples converted as
    /// `setAsFloatLE` converts them.
    #[test]
    fn the_file_is_laid_out_as_juces_writer_lays_it_out() {
        let clm = clm_text();
        assert_eq!(clm, "<!>2048 20000000 wavetable (synth-a-size)");
        let samples = [0.0, 0.5, -1.0, 1.5, 1.0 / 65536.0, 3.0 / 65536.0];
        let bytes = wav_bytes(&samples, SAMPLE_RATE, &clm);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32_at(&bytes, 4) as usize, bytes.len() - 8);
        assert_eq!(&bytes[8..16], b"WAVEJUNK");
        assert_eq!(u32_at(&bytes, 16), 52);
        assert_eq!(&bytes[72..76], b"fmt ");
        assert_eq!(u32_at(&bytes, 76), 16);
        assert_eq!(&bytes[80..84], [1, 0, 1, 0]);
        assert_eq!(u32_at(&bytes, 84), 88_200);
        assert_eq!(u32_at(&bytes, 88), 176_400);
        assert_eq!(&bytes[92..96], [2, 0, 16, 0]);
        assert_eq!(&bytes[96..100], b"clm ");
        // 41 characters and a zero, even already.
        assert_eq!(u32_at(&bytes, 100), 42);
        assert_eq!(&bytes[104..145], clm.as_bytes());
        assert_eq!(bytes[145], 0);
        assert_eq!(&bytes[146..150], b"data");
        assert_eq!(u32_at(&bytes, 150), 12);
        let values: Vec<i16> = bytes[154..]
            .chunks(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        // Halves round to even: 0.5 → 0, 1.5 → 2.
        assert_eq!(values, [0, 16384, -32767, 32767, 0, 2]);
    }
}
