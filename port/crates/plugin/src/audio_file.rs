// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Audio files dropped onto a wavetable view: the readers and the wavetable
//! data an audio file can carry.
//!
//! Ports `WavetableEditSection::getWavetableDataString`,
//! `getFadeStyleFromWavetableString` and `getAuthorFromWavetableString`
//! (`vital/src/interface/editor_sections/wavetable_edit_section.cpp`). The
//! WAV reader is the port's own: PCM at 8, 16, 24 and 32 bits and IEEE
//! float at 32 and 64 bits (plain or `WAVE_FORMAT_EXTENSIBLE`), giving the
//! same samples as JUCE's WAV reader and `AudioData` float conversion.
//! The other formats decode with symphonia (D23 records what can differ
//! from JUCE's and CoreAudio's readers).

use sas_engine::sas_oscillators::sas_wavetable::creator::component::{
    FADE_FREQ_INTERPOLATE, FADE_NO_INTERPOLATE, FADE_TIME_INTERPOLATE,
};

/// The extensions the wavetable view takes: the wildcards of JUCE's
/// `registerBasicFormats()` on macOS (WAV, AIFF, FLAC, Ogg Vorbis, and
/// CoreAudio's MP3 and AAC/M4A), without CAF and the rarer CoreAudio types
/// (D23).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "bwf", "aif", "aiff", "flac", "ogg", "mp3", "m4a", "aac",
];

/// An audio file's first channel and its sample rate, as
/// `WavetableEditSection::loadAudioFile` passes `getReadPointer(0)` on.
#[derive(Clone, Debug, PartialEq)]
pub struct Audio {
    pub first_channel: Vec<f32>,
    pub sample_rate: i32,
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// `getWavetableDataString`: the text of the RIFF file's `clm ` chunk, empty
/// when there is none. Like upstream, chunks are walked without the RIFF
/// pad byte after odd sizes.
pub fn wavetable_data_string(bytes: &[u8]) -> String {
    if bytes.get(0..4) != Some(b"RIFF") {
        return String::new();
    }
    let Some(length) = u32_at(bytes, 4) else {
        return String::new();
    };
    let data_end = 8usize.saturating_add(length as usize);
    if bytes.get(8..12) != Some(b"WAVE") {
        return String::new();
    }
    let mut position = 12;
    while position < bytes.len() && position < data_end {
        let (Some(label), Some(chunk_length)) = (
            bytes.get(position..position + 4),
            u32_at(bytes, position + 4),
        ) else {
            return String::new();
        };
        position += 8;
        let chunk_length = chunk_length as usize;
        if label == b"clm " {
            let end = position.saturating_add(chunk_length).min(bytes.len());
            let text = &bytes[position..end];
            // MemoryBlock::toString reads UTF-8 up to the first nul.
            let text = text.split(|&byte| byte == 0).next().unwrap_or(&[]);
            return String::from_utf8_lossy(text).into_owned();
        }
        position = position.saturating_add(chunk_length);
    }
    String::new()
}

/// `getFadeStyleFromWavetableString`: `<!>` and a second token starting
/// with 0 (no interpolation) or 1 (time interpolation); frequency
/// interpolation otherwise.
pub fn fade_style_from_wavetable_string(data: &str) -> i32 {
    let Some(rest) = data.strip_prefix("<!>") else {
        return FADE_FREQ_INTERPOLATE;
    };
    // StringArray::addTokens(text, " ", ""): every space separates, so empty
    // tokens count.
    let tokens: Vec<&str> = rest.split(' ').collect();
    match tokens.get(1).and_then(|token| token.chars().next()) {
        Some('0') => FADE_NO_INTERPOLATE,
        Some('1') => FADE_TIME_INTERPOLATE,
        _ => FADE_FREQ_INTERPOLATE,
    }
}

/// `getAuthorFromWavetableString`: what's between the first `[` and the
/// first `]` after `<!>`.
pub fn author_from_wavetable_string(data: &str) -> String {
    if !data.starts_with("<!>") {
        return String::new();
    }
    match (data.find('['), data.find(']')) {
        (Some(start), Some(end)) if start < end => data[start + 1..end].to_string(),
        _ => String::new(),
    }
}

/// Reads the audio file `bytes`, whose name has extension `extension`:
/// WAV with the port's reader, the other formats with symphonia.
/// `None` when it can't be read.
pub fn read_audio(bytes: &[u8], extension: &str) -> Option<Audio> {
    let extension = extension.to_ascii_lowercase();
    match extension.as_str() {
        "wav" | "bwf" => read_wav(bytes),
        _ if AUDIO_EXTENSIONS.contains(&extension.as_str()) => decode(bytes.to_vec(), &extension),
        _ => None,
    }
}

/// Decodes `bytes` with symphonia, the format guessed from `extension` and
/// the contents, and keeps the first channel. Integer samples convert as
/// JUCE's readers do (exact powers of two); see D23 for the lossy codecs.
fn decode(bytes: Vec<u8>, extension: &str) -> Option<Audio> {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::errors::Error;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let source = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(extension);
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;
    let track = format.default_track(TrackType::Audio)?;
    let track_id = track.id;
    let parameters = track.codec_params.as_ref()?.audio()?.clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&parameters, &AudioDecoderOptions::default())
        .ok()?;

    let mut first_channel = Vec::new();
    let mut sample_rate = parameters.sample_rate;
    let mut interleaved: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(_) => break,
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let channels = decoded.spec().channels().count().max(1);
                sample_rate = sample_rate.or(Some(decoded.spec().rate()));
                interleaved.clear();
                decoded.copy_to_vec_interleaved(&mut interleaved);
                // No input may abort the process: refuse what doesn't fit.
                first_channel
                    .try_reserve(interleaved.len() / channels)
                    .ok()?;
                first_channel.extend(interleaved.iter().step_by(channels));
            }
            // A damaged packet is skipped, as symphonia's players do.
            Err(Error::DecodeError(_)) => continue,
            Err(_) => return None,
        }
    }
    let sample_rate = sample_rate? as i32;
    if first_channel.is_empty() || sample_rate <= 0 {
        return None;
    }
    Some(Audio {
        first_channel,
        sample_rate,
    })
}

const FORMAT_PCM: u16 = 1;
const FORMAT_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xfffe;

/// `WAVE_FORMAT_EXTENSIBLE` sub-formats JUCE's WAV reader takes: PCM, IEEE
/// float, and PCM in the ambisonic B-format form.
const EXTENSIBLE_SUB_FORMATS: [&[u8]; 3] = [
    &[
        1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
    ],
    &[
        3, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
    ],
    &[
        1, 0, 0, 0, 0x21, 0x07, 0xd3, 0x11, 0x86, 0x44, 0xc8, 0xc1, 0xca, 0, 0, 0,
    ],
];

/// A `data` chunk length meaning "unknown" (streamed WAVs from pipes write
/// it, as they write 0): the samples run to the end of the file.
const UNKNOWN_LENGTH: usize = 0xffff_ffff;
/// The most frames a truncated `data` chunk is zero-filled to (about six
/// minutes at 44.1 kHz); a longer declared length reads only the bytes
/// present (D23).
const MAX_ZERO_FILLED_FRAMES: usize = 1 << 24;

/// Whether the bytes at `position` look like a RIFF chunk header: a
/// printable four-character label and a length the file can hold.
fn chunk_header_at(bytes: &[u8], position: usize) -> bool {
    let Some(label) = bytes.get(position..position + 4) else {
        return false;
    };
    let printable = label.iter().all(|&byte| (b' '..=b'~').contains(&byte)) && label[0] != b' ';
    printable
        && u32_at(bytes, position + 4)
            .is_some_and(|length| (position + 8).saturating_add(length as usize) <= bytes.len())
}

/// Reads a WAV file's first channel as JUCE's `WavAudioFormatReader` and
/// `AudioData` conversions give it: integer samples scaled by
/// `1 / (1 + max)` (8-bit unsigned centred on 128), floats as they are.
/// `None` when it isn't a WAV file this reader handles.
pub fn read_wav(bytes: &[u8]) -> Option<Audio> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return None;
    }
    let mut format = None;
    let mut data = None;
    let mut position = 12usize;
    while position + 8 <= bytes.len() {
        let label = &bytes[position..position + 4];
        let length = u32_at(bytes, position + 4)? as usize;
        let start = position + 8;
        let end = start.saturating_add(length).min(bytes.len());
        match label {
            b"fmt " => format = Some(&bytes[start..end]),
            // An unknown length: the samples are the rest of the file. A 0
            // followed by a chunk header is an empty chunk, as JUCE reads it.
            b"data"
                if length == UNKNOWN_LENGTH || (length == 0 && !chunk_header_at(bytes, start)) =>
            {
                data = Some((&bytes[start..], length));
                break;
            }
            // The declared length counts, even past the end of the file.
            b"data" => data = Some((&bytes[start..end], length)),
            _ => {}
        }
        // JUCE's reader skips the pad byte after an odd-sized chunk.
        position = start.saturating_add(length).saturating_add(length & 1);
    }
    let format = format?;
    let (data, data_length) = data?;

    let mut tag = u16_at(format, 0)?;
    let channels = u16_at(format, 2)? as usize;
    let sample_rate = u32_at(format, 4)? as i32;
    let bits = u16_at(format, 14)?;
    if tag == FORMAT_EXTENSIBLE {
        // JUCE takes an extensible format only with a whole extension and
        // a PCM, float or ambisonic sub-format GUID; its first two bytes are
        // the plain format tag.
        let guid = format.get(24..40)?;
        if !EXTENSIBLE_SUB_FORMATS.contains(&guid) {
            return None;
        }
        tag = u16_at(format, 24)?;
    }
    if channels == 0 || sample_rate <= 0 || bits == 0 || bits > 64 {
        return None;
    }
    let bytes_per_sample = (bits as usize).div_ceil(8);
    let frame_size = bytes_per_sample * channels;
    let present_frames = data.len() / frame_size;
    let frames = if data_length == UNKNOWN_LENGTH || data_length == 0 {
        present_frames
    } else {
        // JUCE reads the declared number of frames and zero-fills what the
        // file doesn't hold, but only within bounds here: a declared length
        // more than double the bytes present, or past the cap, reads only
        // the bytes present (D23).
        let declared = data_length / frame_size;
        if declared <= present_frames
            || (declared <= 2 * present_frames.max(1) && declared <= MAX_ZERO_FILLED_FRAMES)
        {
            declared
        } else {
            present_frames
        }
    };
    let zeros = [0u8; 8];
    let sample = |frame: usize| -> &[u8] {
        let at = frame * frame_size;
        data.get(at..at + bytes_per_sample)
            .unwrap_or(&zeros[..bytes_per_sample])
    };

    let mut first_channel: Vec<f32> = Vec::new();
    first_channel.try_reserve_exact(frames).ok()?;
    let convert: fn(&[u8]) -> f32 = match (tag, bits) {
        (FORMAT_PCM, 8) => |s| (s[0] as i32 - 128) as f32 * (1.0 / 128.0),
        (FORMAT_PCM, 16) => |s| i16::from_le_bytes([s[0], s[1]]) as f32 * (1.0 / 32_768.0),
        (FORMAT_PCM, 24) => {
            |s| (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32 * (1.0 / 8_388_608.0)
        }
        (FORMAT_PCM, 32) => {
            |s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 * (1.0 / 2_147_483_648.0)
        }
        (FORMAT_FLOAT, 32) => |s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
        (FORMAT_FLOAT, 64) => |s| f64::from_le_bytes(s.try_into().unwrap()) as f32,
        _ => return None,
    };
    first_channel.extend((0..frames).map(|frame| convert(sample(frame))));
    Some(Audio {
        first_channel,
        sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A RIFF WAVE file with `chunks` after the header.
    fn riff(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut body = b"WAVE".to_vec();
        for (label, data) in chunks {
            body.extend_from_slice(*label);
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(data);
            if data.len() % 2 == 1 {
                body.push(0);
            }
        }
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&(body.len() as u32).to_le_bytes());
        file.extend_from_slice(&body);
        file
    }

    fn fmt(tag: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
        let block = channels * bits.div_ceil(8);
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&tag.to_le_bytes());
        fmt.extend_from_slice(&channels.to_le_bytes());
        fmt.extend_from_slice(&rate.to_le_bytes());
        fmt.extend_from_slice(&(rate * block as u32).to_le_bytes());
        fmt.extend_from_slice(&block.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        fmt
    }

    #[test]
    fn sixteen_bit_stereo_reads_the_first_channel() {
        let mut data = Vec::new();
        for (left, right) in [(16_384i16, -1i16), (-32_768, 5), (32_767, 0)] {
            data.extend_from_slice(&left.to_le_bytes());
            data.extend_from_slice(&right.to_le_bytes());
        }
        let file = riff(&[(b"fmt ", fmt(FORMAT_PCM, 2, 48_000, 16)), (b"data", data)]);
        let audio = read_wav(&file).unwrap();
        assert_eq!(audio.sample_rate, 48_000);
        assert_eq!(audio.first_channel, [0.5, -1.0, 32_767.0 / 32_768.0]);
    }

    #[test]
    fn every_supported_sample_format_scales_as_juce_does() {
        let cases: [(u16, u16, Vec<u8>, f32); 6] = [
            (FORMAT_PCM, 8, vec![192], 0.5),
            (FORMAT_PCM, 16, (-16_384i16).to_le_bytes().to_vec(), -0.5),
            (FORMAT_PCM, 24, vec![0x00, 0x00, 0xc0], -0.5),
            (FORMAT_PCM, 32, (1i32 << 30).to_le_bytes().to_vec(), 0.5),
            (FORMAT_FLOAT, 32, 0.25f32.to_le_bytes().to_vec(), 0.25),
            (FORMAT_FLOAT, 64, (-0.75f64).to_le_bytes().to_vec(), -0.75),
        ];
        for (tag, bits, data, expected) in cases {
            let file = riff(&[(b"fmt ", fmt(tag, 1, 44_100, bits)), (b"data", data)]);
            assert_eq!(
                read_wav(&file).unwrap().first_channel,
                [expected],
                "{tag} {bits}"
            );
        }
        // WAVE_FORMAT_EXTENSIBLE with a PCM sub-format.
        let mut extensible = fmt(FORMAT_EXTENSIBLE, 1, 44_100, 16);
        extensible.extend_from_slice(&22u16.to_le_bytes());
        extensible.extend_from_slice(&16u16.to_le_bytes());
        extensible.extend_from_slice(&0u32.to_le_bytes());
        extensible.extend_from_slice(EXTENSIBLE_SUB_FORMATS[0]);
        let file = riff(&[
            (b"fmt ", extensible.clone()),
            (b"data", 16_384i16.to_le_bytes().to_vec()),
        ]);
        assert_eq!(read_wav(&file).unwrap().first_channel, [0.5]);

        // JUCE refuses an extension cut short, or a sub-format it doesn't
        // know.
        let short = riff(&[(b"fmt ", extensible[..30].to_vec()), (b"data", vec![0; 2])]);
        assert!(read_wav(&short).is_none());
        let mut unknown = extensible;
        unknown[30] ^= 0xff;
        let unknown = riff(&[(b"fmt ", unknown), (b"data", vec![0; 2])]);
        assert!(read_wav(&unknown).is_none());
    }

    /// A mono 16-bit file holding `frames` frames whose data chunk declares
    /// `declared` bytes.
    fn declaring(declared: u32, frames: &[i16]) -> Vec<u8> {
        let mut file = riff(&[
            (b"fmt ", fmt(FORMAT_PCM, 1, 44_100, 16)),
            (
                b"data",
                frames.iter().flat_map(|s| s.to_le_bytes()).collect(),
            ),
        ]);
        let length_at = file.len() - 2 * frames.len() - 4;
        file[length_at..length_at + 4].copy_from_slice(&declared.to_le_bytes());
        file
    }

    #[test]
    fn unknown_and_absurd_data_lengths_read_the_bytes_present() {
        let frames = [16_384i16, -16_384, 8_192];
        let expected = [0.5, -0.5, 0.25];
        // The streamed-WAV markers: the samples run to the end of the file.
        assert_eq!(
            read_wav(&declaring(0xffff_ffff, &frames))
                .unwrap()
                .first_channel,
            expected
        );
        assert_eq!(
            read_wav(&declaring(0, &frames)).unwrap().first_channel,
            expected
        );
        // A declared length far past the file doesn't zero-fill gigabytes.
        assert_eq!(
            read_wav(&declaring(0x7fff_fff0, &frames))
                .unwrap()
                .first_channel,
            expected
        );
        assert_eq!(
            read_wav(&declaring(1_000_000, &frames))
                .unwrap()
                .first_channel,
            expected
        );
        // A format with no sample size is refused rather than divided by.
        // A genuinely empty chunk followed by another reads no frames.
        let empty = riff(&[
            (b"fmt ", fmt(FORMAT_PCM, 1, 44_100, 16)),
            (b"data", Vec::new()),
            (b"LIST", b"INFOabcd".to_vec()),
        ]);
        assert!(read_wav(&empty).unwrap().first_channel.is_empty());
        let no_bits = riff(&[
            (b"fmt ", fmt(FORMAT_PCM, 1, 44_100, 0)),
            (b"data", vec![0; 4]),
        ]);
        assert!(read_wav(&no_bits).is_none());
    }

    #[test]
    fn a_data_chunk_longer_than_the_file_is_zero_filled() {
        let mut file = riff(&[
            (b"fmt ", fmt(FORMAT_PCM, 1, 44_100, 16)),
            (
                b"data",
                [16_384i16, -16_384]
                    .iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect(),
            ),
        ]);
        // Declare four frames where the file holds two.
        let length_at = file.len() - 4 - 4;
        file[length_at..length_at + 4].copy_from_slice(&8u32.to_le_bytes());
        assert_eq!(
            read_wav(&file).unwrap().first_channel,
            [0.5, -0.5, 0.0, 0.0]
        );
    }

    #[test]
    fn files_that_arent_wav_are_refused() {
        assert!(read_wav(b"not a wav file at all").is_none());
        let compressed = riff(&[(b"fmt ", fmt(0x11, 1, 44_100, 4)), (b"data", vec![0; 8])]);
        assert!(read_wav(&compressed).is_none());
    }

    #[test]
    fn the_clm_chunk_gives_the_fade_style_and_author() {
        let file = riff(&[
            (b"fmt ", fmt(FORMAT_PCM, 1, 44_100, 16)),
            (b"clm ", b"<!>2048 10000000 wavetable [Someone]".to_vec()),
            (b"data", vec![0; 4]),
        ]);
        let text = wavetable_data_string(&file);
        assert_eq!(text, "<!>2048 10000000 wavetable [Someone]");
        assert_eq!(
            fade_style_from_wavetable_string(&text),
            FADE_TIME_INTERPOLATE
        );
        assert_eq!(author_from_wavetable_string(&text), "Someone");

        assert_eq!(
            fade_style_from_wavetable_string("<!>2048 01000000"),
            FADE_NO_INTERPOLATE
        );
        assert_eq!(
            fade_style_from_wavetable_string("<!>2048 2"),
            FADE_FREQ_INTERPOLATE
        );
        assert_eq!(
            fade_style_from_wavetable_string("<!>2048"),
            FADE_FREQ_INTERPOLATE
        );
        assert_eq!(fade_style_from_wavetable_string(""), FADE_FREQ_INTERPOLATE);
        assert_eq!(author_from_wavetable_string("no marker [Name]"), "");

        let plain = riff(&[
            (b"fmt ", fmt(FORMAT_PCM, 1, 44_100, 16)),
            (b"data", vec![0; 4]),
        ]);
        assert_eq!(wavetable_data_string(&plain), "");
    }
}
