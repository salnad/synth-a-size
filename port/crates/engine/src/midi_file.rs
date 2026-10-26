// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_audio_basics, ISC).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Standard MIDI File reading for the offline render, matching how
//! `ref-render` reads a file through JUCE: `MidiFile::readFrom`,
//! `convertTimestampTicksToSeconds`, every track merged with
//! `addSequence` and stable-sorted by time, meta and sysex events skipped.

/// A short MIDI message at a time in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimedMessage {
    pub seconds: f64,
    pub message: [u8; 3],
}

/// Why a MIDI file could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiFileError(pub String);

impl std::fmt::Display for MidiFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot parse MIDI file: {}", self.0)
    }
}

impl std::error::Error for MidiFileError {}

struct RawEvent {
    ticks: f64,
    /// `None` for meta and sysex events (kept only for tempo).
    message: Option<[u8; 3]>,
    tempo_seconds_per_quarter: Option<f64>,
    /// A tempo or time-signature event (JUCE's tempo map holds both).
    tempo_map_point: bool,
}

fn read_u16(data: &[u8], at: usize) -> Result<u16, MidiFileError> {
    data.get(at..at + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or_else(|| MidiFileError("truncated".into()))
}

fn read_u32(data: &[u8], at: usize) -> Result<u32, MidiFileError> {
    data.get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| MidiFileError("truncated".into()))
}

fn read_var_len(data: &[u8], pos: &mut usize) -> Result<u32, MidiFileError> {
    let mut value = 0u32;
    for _ in 0..4 {
        let byte = *data
            .get(*pos)
            .ok_or_else(|| MidiFileError("truncated variable-length value".into()))?;
        *pos += 1;
        value = (value << 7) | (byte & 0x7f) as u32;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Ok(value)
}

fn data_bytes(status: u8) -> usize {
    match status & 0xf0 {
        0xc0 | 0xd0 => 1,
        _ => 2,
    }
}

fn read_track(data: &[u8]) -> Result<Vec<RawEvent>, MidiFileError> {
    let mut events = Vec::new();
    let mut pos = 0;
    let mut ticks = 0.0;
    let mut running_status = 0u8;
    while pos < data.len() {
        ticks += read_var_len(data, &mut pos)? as f64;
        let first = *data
            .get(pos)
            .ok_or_else(|| MidiFileError("truncated event".into()))?;
        match first {
            0xff => {
                let kind = *data
                    .get(pos + 1)
                    .ok_or_else(|| MidiFileError("truncated meta event".into()))?;
                pos += 2;
                let length = read_var_len(data, &mut pos)? as usize;
                let body = data
                    .get(pos..pos + length)
                    .ok_or_else(|| MidiFileError("truncated meta event".into()))?;
                pos += length;
                let tempo = (kind == 0x51 && length >= 3).then(|| {
                    let micros =
                        ((body[0] as u32) << 16) | ((body[1] as u32) << 8) | body[2] as u32;
                    micros as f64 / 1_000_000.0
                });
                events.push(RawEvent {
                    ticks,
                    message: None,
                    tempo_seconds_per_quarter: tempo,
                    tempo_map_point: kind == 0x51 || kind == 0x58,
                });
                if kind == 0x2f {
                    break;
                }
            }
            0xf0 | 0xf7 => {
                pos += 1;
                let length = read_var_len(data, &mut pos)? as usize;
                pos += length;
                events.push(RawEvent {
                    ticks,
                    message: None,
                    tempo_seconds_per_quarter: None,
                    tempo_map_point: false,
                });
            }
            _ => {
                let status = if first & 0x80 != 0 {
                    pos += 1;
                    running_status = first;
                    first
                } else {
                    running_status
                };
                if status & 0x80 == 0 {
                    return Err(MidiFileError("data byte without a status".into()));
                }
                let count = data_bytes(status);
                let body = data
                    .get(pos..pos + count)
                    .ok_or_else(|| MidiFileError("truncated channel message".into()))?;
                pos += count;
                let mut message = [status, 0, 0];
                message[1..1 + count].copy_from_slice(body);
                events.push(RawEvent {
                    ticks,
                    message: Some(message),
                    tempo_seconds_per_quarter: None,
                    tempo_map_point: false,
                });
            }
        }
    }
    Ok(events)
}

/// JUCE's `MidiFileHelpers::convertTicksToSeconds`.
fn ticks_to_seconds(time: f64, tempo_events: &[(f64, Option<f64>)], time_format: i16) -> f64 {
    if time_format < 0 {
        let format = time_format as i32;
        return time / ((-(format >> 8)) * (format & 0xff)) as f64;
    }

    let tick_length = 1.0 / (time_format as i32 & 0x7fff) as f64;
    let mut seconds_per_tick = 0.5 * tick_length;
    let mut last_time = 0.0;
    let mut corrected_time = 0.0;
    let mut i = 0;
    while i < tempo_events.len() {
        let (event_time, tempo) = tempo_events[i];
        if event_time >= time {
            break;
        }
        corrected_time += (event_time - last_time) * seconds_per_tick;
        last_time = event_time;
        if let Some(seconds_per_quarter) = tempo {
            seconds_per_tick = tick_length * seconds_per_quarter;
        }
        while i + 1 < tempo_events.len() {
            let (next_time, next_tempo) = tempo_events[i + 1];
            if next_time != event_time {
                break;
            }
            if let Some(seconds_per_quarter) = next_tempo {
                seconds_per_tick = tick_length * seconds_per_quarter;
            }
            i += 1;
        }
        i += 1;
    }
    corrected_time + (time - last_time) * seconds_per_tick
}

/// Reads every track's short messages, timed in seconds and merged in time
/// order (stable, so equal times keep track then file order).
pub fn read_midi_file(data: &[u8]) -> Result<Vec<TimedMessage>, MidiFileError> {
    if data.get(0..4) != Some(b"MThd") {
        return Err(MidiFileError("no MThd header".into()));
    }
    let header_length = read_u32(data, 4)? as usize;
    let num_tracks = read_u16(data, 10)? as usize;
    let time_format = read_u16(data, 12)? as i16;

    let mut pos = 8 + header_length;
    let mut tracks = Vec::new();
    while tracks.len() < num_tracks && pos + 8 <= data.len() {
        let length = read_u32(data, pos + 4)? as usize;
        let body = data
            .get(pos + 8..pos + 8 + length)
            .ok_or_else(|| MidiFileError("truncated track".into()))?;
        if &data[pos..pos + 4] == b"MTrk" {
            tracks.push(read_track(body)?);
        }
        pos += 8 + length;
    }

    let mut tempo_events: Vec<(f64, Option<f64>)> = tracks
        .iter()
        .flatten()
        .filter(|event| event.tempo_map_point)
        .map(|event| (event.ticks, event.tempo_seconds_per_quarter))
        .collect();
    tempo_events.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut messages: Vec<TimedMessage> = tracks
        .iter()
        .flatten()
        .filter_map(|event| {
            event.message.map(|message| TimedMessage {
                seconds: ticks_to_seconds(event.ticks, &tempo_events, time_format),
                message,
            })
        })
        .collect();
    messages.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(tracks: &[&[u8]]) -> Vec<u8> {
        let mut data = b"MThd".to_vec();
        data.extend_from_slice(&6u32.to_be_bytes());
        data.extend_from_slice(&1u16.to_be_bytes());
        data.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
        data.extend_from_slice(&480u16.to_be_bytes());
        for track in tracks {
            data.extend_from_slice(b"MTrk");
            data.extend_from_slice(&(track.len() as u32).to_be_bytes());
            data.extend_from_slice(track);
        }
        data
    }

    #[test]
    fn tempo_and_running_status() {
        // 100 BPM (600000 us per quarter); a note on at tick 480 and, by
        // running status, its velocity-0 note on at tick 960.
        let tempo: &[u8] = &[0, 0xff, 0x51, 3, 0x09, 0x27, 0xc0, 0, 0xff, 0x2f, 0];
        let notes: &[u8] = &[
            0x83, 0x60, 0x90, 60, 100, 0x83, 0x60, 60, 0, 0, 0xff, 0x2f, 0,
        ];
        let messages = read_midi_file(&file(&[tempo, notes])).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].message, [0x90, 60, 100]);
        assert!((messages[0].seconds - 0.6).abs() < 1e-12);
        assert_eq!(messages[1].message, [0x90, 60, 0]);
        assert!((messages[1].seconds - 1.2).abs() < 1e-12);
    }

    #[test]
    fn default_tempo_is_120_bpm() {
        let notes: &[u8] = &[0x83, 0x60, 0x90, 60, 100, 0, 0xff, 0x2f, 0];
        let messages = read_midi_file(&file(&[notes])).unwrap();
        assert!((messages[0].seconds - 0.5).abs() < 1e-12);
    }
}
