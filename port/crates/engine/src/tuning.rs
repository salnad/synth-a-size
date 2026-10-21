// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `Tuning`: maps MIDI notes to (fractional) tuned notes from Scala scales,
//! keyboard maps and AnaMark `.tun` files.
//!
//! Ports `vital/src/common/tuning.h` and `tuning.cpp`. File text is parsed
//! with the JUCE string semantics upstream relies on ([`juce_text`]):
//! `File::readLines`, `String::trim`, whitespace tokens that keep empty
//! entries, and the lenient `getIntValue` / `getFloatValue` number readers.

use std::path::Path;

use sas_simd::common::{CENTS_PER_NOTE, MIDI_0_FREQUENCY, MIDI_SIZE, NOTES_PER_OCTAVE};
use sas_simd::utils;
use serde_json::{json, Value};

const SCALA_FILE_EXTENSION: &str = ".scl";
const KEYBOARD_MAP_EXTENSION: &str = ".kbm";
const TUN_FILE_EXTENSION: &str = ".tun";
const DEFAULT_MIDI_REFERENCE: i32 = 60;
const SCALA_KBM_COMMENT: char = '!';
const TUN_COMMENT: char = ';';

const KBM_MAP_SIZE_POSITION: usize = 0;
const KBM_MIDI_MAP_MIDDLE_POSITION: usize = 3;
const KBM_REFERENCE_NOTE_POSITION: usize = 4;
const KBM_REFERENCE_FREQUENCY_POSITION: usize = 5;

/// JUCE string helpers with the exact semantics `tuning.cpp` depends on.
pub mod juce_text {
    /// `CharacterFunctions::isWhitespace` for the C locale.
    pub fn is_whitespace(c: char) -> bool {
        c == ' ' || ('\t'..='\r').contains(&c)
    }

    /// `String::trim()`.
    pub fn trim(text: &str) -> &str {
        text.trim_matches(is_whitespace)
    }

    /// `StringArray::addLines`: splits at `\n`, `\r\n` and `\r`; a non-empty
    /// text always yields its last segment, even when it is empty.
    pub fn lines(text: &str) -> Vec<String> {
        let mut lines = Vec::new();
        if text.is_empty() {
            return lines;
        }
        let bytes: Vec<char> = text.chars().collect();
        let mut start = 0;
        let mut i = 0;
        loop {
            if i == bytes.len() {
                lines.push(bytes[start..i].iter().collect());
                break;
            }
            match bytes[i] {
                '\n' => {
                    lines.push(bytes[start..i].iter().collect());
                    i += 1;
                    start = i;
                }
                '\r' => {
                    lines.push(bytes[start..i].iter().collect());
                    i += 1;
                    if i < bytes.len() && bytes[i] == '\n' {
                        i += 1;
                    }
                    start = i;
                }
                _ => i += 1,
            }
        }
        lines
    }

    /// `StringArray::addTokens(text, breakCharacters, "")`: empty entries are
    /// kept, and an empty text has no tokens.
    pub fn tokens<'a>(text: &'a str, break_characters: &str) -> Vec<&'a str> {
        if text.is_empty() {
            return Vec::new();
        }
        text.split(|c| break_characters.contains(c)).collect()
    }

    /// `StringArray::addTokens(text, false)`: whitespace tokens.
    pub fn whitespace_tokens(text: &str) -> Vec<&str> {
        tokens(text, " \n\r\t")
    }

    /// `StringArray::operator[]`: out-of-range reads give an empty string.
    pub fn token<'a>(tokens: &[&'a str], index: usize) -> &'a str {
        tokens.get(index).copied().unwrap_or("")
    }

    /// `String::getIntValue()`: optional leading whitespace and `-`, then
    /// digits, with unsigned wrap-around.
    pub fn int_value(text: &str) -> i32 {
        let mut chars = text.trim_start_matches(is_whitespace).chars().peekable();
        let negative = chars.peek() == Some(&'-');
        if negative {
            chars.next();
        }
        let mut v: u32 = 0;
        for c in chars {
            match c.to_digit(10) {
                Some(d) => v = v.wrapping_mul(10).wrapping_add(d),
                None => break,
            }
        }
        if negative {
            (v as i32).wrapping_neg()
        } else {
            v as i32
        }
    }

    /// `String::getDoubleValue()` (`CharacterFunctions::readDoubleValue`,
    /// non-MinGW): copies at most 18 significant digits, the decimal point
    /// and the exponent into a buffer and reads it with `strtod` in the C
    /// locale (correctly rounded, as Rust's parser is).
    pub fn double_value(text: &str) -> f64 {
        const MAX_SIGNIFICANT_DIGITS: i32 = 17 + 1;
        let chars: Vec<char> = text.trim_start_matches(is_whitespace).chars().collect();
        let mut pos = 0;
        let at = |p: usize| chars.get(p).copied().unwrap_or('\0');
        let mut buffer = String::new();

        let mut c = at(pos);
        if c == '-' {
            buffer.push('-');
        }
        if c == '-' || c == '+' {
            pos += 1;
            c = at(pos);
        }

        let lower = |p: usize| at(p).to_ascii_lowercase();
        match c {
            'n' | 'N' if lower(pos + 1) == 'a' && lower(pos + 2) == 'n' => return f64::NAN,
            'i' | 'I' if lower(pos + 1) == 'n' && lower(pos + 2) == 'f' => return f64::INFINITY,
            _ => {}
        }

        let mut num_sig_figs = 0;
        let mut decimal_point_found = false;
        let mut extra_exponent = 0;
        loop {
            let c = at(pos);
            if c.is_ascii_digit() {
                pos += 1;
                let digit = c as i32 - '0' as i32;
                if decimal_point_found {
                    if num_sig_figs >= MAX_SIGNIFICANT_DIGITS {
                        continue;
                    }
                } else {
                    if num_sig_figs >= MAX_SIGNIFICANT_DIGITS {
                        extra_exponent += 1;
                        continue;
                    }
                    if num_sig_figs == 0 && digit == 0 {
                        continue;
                    }
                }
                buffer.push(c);
                num_sig_figs += 1;
            } else if !decimal_point_found && c == '.' {
                pos += 1;
                buffer.push('.');
                decimal_point_found = true;
            } else {
                break;
            }
        }

        let write_exponent = |buffer: &mut String, exponent: i32| {
            buffer.push_str(&format!("{exponent:03}"));
        };

        let c = at(pos);
        if (c == 'e' || c == 'E') && num_sig_figs > 0 {
            buffer.push('e');
            pos += 1;
            let mut positive = true;
            match at(pos) {
                '-' => {
                    positive = false;
                    pos += 1;
                }
                '+' => pos += 1,
                _ => {}
            }
            let mut exponent: i32 = 0;
            while at(pos).is_ascii_digit() {
                let digit = at(pos) as i32 - '0' as i32;
                pos += 1;
                if digit != 0 || exponent != 0 {
                    exponent = exponent.wrapping_mul(10).wrapping_add(digit);
                }
            }
            exponent = extra_exponent + if positive { exponent } else { -exponent };
            if exponent < 0 {
                buffer.push('-');
            }
            let exponent = exponent.abs();
            if exponent > f64::MAX_10_EXP {
                return f64::NAN;
            }
            write_exponent(&mut buffer, exponent);
        } else if extra_exponent > 0 {
            buffer.push('e');
            write_exponent(&mut buffer, extra_exponent);
        }

        strtod(&buffer)
    }

    /// `strtod` on the buffer `double_value` builds: the longest valid
    /// prefix, or 0.
    fn strtod(buffer: &str) -> f64 {
        let mut text = buffer.to_string();
        // A trailing exponent marker without digits is not part of the number.
        if text.ends_with('e') || text.ends_with("e-") {
            text.truncate(text.rfind('e').unwrap());
        }
        let digits = text.chars().filter(char::is_ascii_digit).count();
        let mantissa_digits = text
            .split('e')
            .next()
            .unwrap()
            .chars()
            .filter(char::is_ascii_digit)
            .count();
        if digits == 0 || mantissa_digits == 0 {
            return 0.0;
        }
        text.parse::<f64>().unwrap_or(0.0)
    }

    /// `String::getFloatValue()`.
    pub fn float_value(text: &str) -> f32 {
        double_value(text) as f32
    }

    /// `String::upToFirstOccurrenceOf(sub, false, true)`.
    pub fn up_to_first(text: &str, sub: char) -> &str {
        match text.find(sub) {
            Some(index) => &text[..index],
            None => text,
        }
    }

    /// `String::fromLastOccurrenceOf(sub, false, true)`.
    pub fn from_last(text: &str, sub: char) -> &str {
        match text.rfind(sub) {
            Some(index) => &text[index + sub.len_utf8()..],
            None => text,
        }
    }
}

use juce_text as jt;

fn extract_first_token(source: &str) -> String {
    jt::token(&jt::whitespace_tokens(source), 0).to_string()
}

fn read_cents_to_transpose(cents: &str) -> f32 {
    jt::float_value(cents) / CENTS_PER_NOTE as f32
}

fn read_ratio_to_transpose(ratio: &str) -> f32 {
    let tokens = jt::tokens(ratio, "/");
    let mut value = jt::int_value(jt::token(&tokens, 0)) as f32;
    if tokens.len() == 2 {
        value /= jt::int_value(tokens[1]) as f32;
    }
    utils::ratio_to_midi_transpose(value)
}

fn read_tun_section(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let end = chars.len().saturating_sub(1).max(1);
    chars[1.min(chars.len())..end.min(chars.len())]
        .iter()
        .collect::<String>()
        .to_lowercase()
}

fn is_base_frequency_assignment(line: &str) -> bool {
    jt::trim(&jt::up_to_first(line, '=').to_lowercase()) == "basefreq"
}

fn get_note_assignment_index(line: &str) -> i32 {
    let variable = jt::up_to_first(line, '=');
    let tokens = jt::whitespace_tokens(variable);
    if tokens.len() <= 1 || tokens[0].to_lowercase() != "note" {
        return -1;
    }
    let index = jt::int_value(tokens[1]);
    if !(0..MIDI_SIZE).contains(&index) {
        return -1;
    }
    index
}

fn get_assignment_value(line: &str) -> f32 {
    jt::float_value(jt::trim(jt::from_last(line, '=')))
}

/// `File::getFileExtension()`.
fn file_extension(path: &Path) -> String {
    let full = path.to_string_lossy();
    let last_slash = full.rfind('/').map_or(-1, |i| i as isize);
    match full.rfind('.') {
        Some(dot) if dot as isize > last_slash => full[dot..].to_string(),
        _ => String::new(),
    }
}

/// `File::getFileNameWithoutExtension()`.
fn file_name_without_extension(path: &Path) -> String {
    let full = path.to_string_lossy();
    let last_slash = full.rfind('/').map_or(0, |i| i + 1);
    match full.rfind('.') {
        Some(dot) if dot > last_slash => full[last_slash..dot].to_string(),
        _ => full[last_slash..].to_string(),
    }
}

/// `File::readLines` (an unreadable file reads as empty).
fn read_lines(path: &Path) -> Vec<String> {
    let bytes = std::fs::read(path).unwrap_or_default();
    jt::lines(&String::from_utf8_lossy(&bytes))
}

/// `Tuning`.
#[derive(Clone, Debug)]
pub struct Tuning {
    scale_start_midi_note: i32,
    reference_midi_note: f32,
    scale: Vec<f32>,
    keyboard_mapping: Vec<i32>,
    tuning: [f32; Self::TUNING_SIZE],
    tuning_name: String,
    mapping_name: String,
    default: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Self::new()
    }
}

impl Tuning {
    pub const TUNING_SIZE: usize = 2 * MIDI_SIZE as usize;
    pub const TUNING_CENTER: i32 = MIDI_SIZE;

    /// `Tuning()`.
    pub fn new() -> Tuning {
        let mut tuning = Tuning {
            scale_start_midi_note: DEFAULT_MIDI_REFERENCE,
            reference_midi_note: 0.0,
            scale: Vec::new(),
            keyboard_mapping: Vec::new(),
            tuning: [0.0; Self::TUNING_SIZE],
            tuning_name: String::new(),
            mapping_name: String::new(),
            default: true,
        };
        tuning.set_default_tuning();
        tuning
    }

    /// `Tuning(File)` / `getTuningForFile(file)`.
    pub fn from_file(path: &Path) -> Tuning {
        let mut tuning = Tuning::new();
        tuning.load_file(path);
        tuning
    }

    /// `allFileExtensions()`.
    pub fn all_file_extensions() -> String {
        format!("*{SCALA_FILE_EXTENSION};*{KEYBOARD_MAP_EXTENSION};*{TUN_FILE_EXTENSION}")
    }

    /// `noteToMidiKey(note_text)`: "C4" → 60; -1 if unparseable.
    pub fn note_to_midi_key(note_text: &str) -> i32 {
        const NOTES_IN_SCALE: i32 = 7;
        const OCTAVE_START: i32 = -1;
        const SCALE: [i32; NOTES_IN_SCALE as usize] = [-3, -1, 0, 2, 4, 5, 7];

        let mut text: Vec<char> = note_text
            .to_lowercase()
            .chars()
            .filter(|&c| c != ' ')
            .collect();
        if note_text.chars().count() < 2 {
            return -1;
        }
        let at = |text: &Vec<char>, i: usize| text.get(i).copied().unwrap_or('\0');

        let note_in_scale = at(&text, 0) as i32 - 'a' as i32;
        if !(0..NOTES_IN_SCALE).contains(&note_in_scale) {
            return -1;
        }

        let mut offset = SCALE[note_in_scale as usize];
        text.remove(0);
        if at(&text, 0) == '#' {
            text.remove(0);
            offset += 1;
        } else if at(&text, 0) == 'b' {
            text.remove(0);
            offset -= 1;
        }

        if text.is_empty() {
            return -1;
        }

        let mut negative = false;
        if text[0] == '-' {
            text.remove(0);
            negative = true;
            if text.is_empty() {
                return -1;
            }
        }
        let mut octave = text[0] as i32 - '0' as i32;
        if negative {
            octave = -octave;
        }
        octave -= OCTAVE_START;
        NOTES_PER_OCTAVE * octave + offset
    }

    /// `loadFile(file)`: by extension (`.scl`, `.tun`, `.kbm`).
    pub fn load_file(&mut self, path: &Path) {
        let extension = file_extension(path).to_lowercase();
        if extension == SCALA_FILE_EXTENSION {
            self.load_scala_file(path);
        } else if extension == TUN_FILE_EXTENSION {
            self.load_tun_file(path);
        } else if extension == KEYBOARD_MAP_EXTENSION {
            self.load_keyboard_map_file(path);
        }
        self.default = false;
    }

    /// `loadScalaFile(const StringArray&)`.
    pub fn load_scala_lines(&mut self, scala_lines: &[String]) {
        #[derive(PartialEq)]
        enum State {
            Description,
            ScaleLength,
            ScaleRatios,
        }
        let mut state = State::Description;

        let mut scale_length: i32 = 1;
        let mut scale = vec![0.0f32];

        for line in scala_lines {
            let trimmed_line = jt::trim(line);
            if trimmed_line.starts_with(SCALA_KBM_COMMENT) {
                continue;
            }

            // `scale.size() >= scale_length + 1` compares as size_t.
            if scale.len() as u64 >= scale_length.wrapping_add(1) as i64 as u64 {
                break;
            }

            match state {
                State::Description => state = State::ScaleLength,
                State::ScaleLength => {
                    scale_length = jt::int_value(&extract_first_token(trimmed_line));
                    state = State::ScaleRatios;
                }
                State::ScaleRatios => {
                    let tuning = extract_first_token(trimmed_line);
                    if tuning.contains('.') {
                        scale.push(read_cents_to_transpose(&tuning));
                    } else {
                        scale.push(read_ratio_to_transpose(&tuning));
                    }
                }
            }
        }

        self.keyboard_mapping.clear();
        for i in 0..scale.len() - 1 {
            self.keyboard_mapping.push(i as i32);
        }
        self.scale_start_midi_note = DEFAULT_MIDI_REFERENCE;
        self.reference_midi_note = 0.0;

        self.load_scale(scale);
        self.default = false;
    }

    fn load_scala_file(&mut self, path: &Path) {
        let lines = read_lines(path);
        self.load_scala_lines(&lines);
        self.tuning_name = file_name_without_extension(path);
    }

    fn load_keyboard_map_file(&mut self, path: &Path) {
        const HEADER_SIZE: usize = 7;

        let lines = read_lines(path);
        let mut header_data = [0.0f32; HEADER_SIZE];
        let mut header_position = 0;
        let mut map_size: i32 = 0;
        let mut last_scale_value: i32 = 0;
        self.keyboard_mapping.clear();

        for line in &lines {
            let trimmed_line = jt::trim(line);
            if trimmed_line.starts_with(SCALA_KBM_COMMENT) {
                continue;
            }

            if header_position >= HEADER_SIZE {
                let token = extract_first_token(trimmed_line);
                if !token.to_lowercase().starts_with('x') {
                    last_scale_value = jt::int_value(&token);
                }

                self.keyboard_mapping.push(last_scale_value);

                // Compared as size_t, so a negative map size never stops the read.
                if self.keyboard_mapping.len() as u64 >= map_size as i64 as u64 {
                    break;
                }
            } else {
                header_data[header_position] = jt::float_value(&extract_first_token(trimmed_line));
                if header_position == KBM_MAP_SIZE_POSITION {
                    map_size = header_data[header_position] as i32;
                }
                header_position += 1;
            }
        }

        self.set_start_midi_note(header_data[KBM_MIDI_MAP_MIDDLE_POSITION] as i32);
        self.set_reference_note_frequency(
            header_data[KBM_REFERENCE_NOTE_POSITION] as i32,
            header_data[KBM_REFERENCE_FREQUENCY_POSITION],
        );
        let scale = self.scale.clone();
        self.load_scale(scale);

        self.mapping_name = file_name_without_extension(path);
    }

    fn load_tun_file(&mut self, path: &Path) {
        #[derive(PartialEq)]
        enum State {
            ScanningForSection,
            Tuning,
            ExactTuning,
        }

        self.keyboard_mapping.clear();

        let mut state = State::ScanningForSection;
        let lines = read_lines(path);

        let mut last_read_note: i32 = 0;
        let mut base_frequency = MIDI_0_FREQUENCY;
        let mut scale: Vec<f32> = (0..MIDI_SIZE).map(|i| i as f32).collect();

        for line in &lines {
            let trimmed_line = jt::trim(line);
            if trimmed_line.is_empty() || trimmed_line.starts_with(TUN_COMMENT) {
                continue;
            }

            if trimmed_line.starts_with('[') {
                let section = read_tun_section(trimmed_line);
                state = if section == "tuning" {
                    State::Tuning
                } else if section == "exact tuning" {
                    State::ExactTuning
                } else {
                    State::ScanningForSection
                };
            } else if state == State::Tuning || state == State::ExactTuning {
                if is_base_frequency_assignment(trimmed_line) {
                    base_frequency = get_assignment_value(trimmed_line);
                } else {
                    let index = get_note_assignment_index(trimmed_line);
                    last_read_note = last_read_note.max(index);
                    if index >= 0 {
                        scale[index as usize] =
                            get_assignment_value(trimmed_line) / CENTS_PER_NOTE as f32;
                    }
                }
            }
        }

        scale.resize(last_read_note as usize + 1, 0.0);

        self.load_scale(scale);
        self.set_start_midi_note(0);
        self.set_reference_frequency(base_frequency);
        self.tuning_name = file_name_without_extension(path);
    }

    /// `loadScale(scale)`.
    pub fn load_scale(&mut self, scale: Vec<f32>) {
        self.scale = scale.clone();
        if scale.len() <= 1 {
            self.set_constant_tuning(DEFAULT_MIDI_REFERENCE as f32);
            return;
        }

        let scale_size = scale.len() as i32 - 1;
        let mut mapping_size = scale_size;
        if !self.keyboard_mapping.is_empty() {
            mapping_size = self.keyboard_mapping.len() as i32;
        }

        let octave_offset = scale[scale_size as usize];
        let start_octave = -Self::TUNING_CENTER / mapping_size - 1;
        let mut mapping_position = -Self::TUNING_CENTER - start_octave * mapping_size;

        let mut current_offset = start_octave as f32 * octave_offset;
        for i in 0..Self::TUNING_SIZE {
            if mapping_position >= mapping_size {
                current_offset += octave_offset;
                mapping_position = 0;
            }

            let mut note_in_scale = mapping_position;
            if !self.keyboard_mapping.is_empty() {
                note_in_scale = self.keyboard_mapping[mapping_position as usize];
            }

            self.tuning[i] = current_offset + scale[note_in_scale as usize];
            mapping_position += 1;
        }
    }

    /// `setConstantTuning(note)`.
    pub fn set_constant_tuning(&mut self, note: f32) {
        self.tuning = [note; Self::TUNING_SIZE];
    }

    /// `setDefaultTuning()`.
    pub fn set_default_tuning(&mut self) {
        for (i, value) in self.tuning.iter_mut().enumerate() {
            *value = (i as i32 - Self::TUNING_CENTER) as f32;
        }

        self.scale = (0..=NOTES_PER_OCTAVE).map(|i| i as f32).collect();
        self.keyboard_mapping.clear();

        self.default = true;
        self.tuning_name.clear();
        self.mapping_name.clear();
    }

    /// `convertMidiNote(note)`.
    pub fn convert_midi_note(&self, note: i32) -> f32 {
        let scale_offset = note - self.scale_start_midi_note;
        self.tuning[(Self::TUNING_CENTER + scale_offset) as usize]
            + self.scale_start_midi_note as f32
            + self.reference_midi_note
    }

    pub fn set_start_midi_note(&mut self, start_midi_note: i32) {
        self.scale_start_midi_note = start_midi_note;
    }

    pub fn set_reference_note(&mut self, reference_note: i32) {
        self.reference_midi_note = reference_note as f32;
    }

    /// `setReferenceFrequency(frequency)`.
    pub fn set_reference_frequency(&mut self, frequency: f32) {
        self.set_reference_note_frequency(0, frequency);
    }

    /// `setReferenceNoteFrequency(midi_note, frequency)`.
    pub fn set_reference_note_frequency(&mut self, midi_note: i32, frequency: f32) {
        self.reference_midi_note = utils::frequency_to_midi_note(frequency) - midi_note as f32;
    }

    /// `setReferenceRatio(ratio)`.
    pub fn set_reference_ratio(&mut self, ratio: f32) {
        self.reference_midi_note = utils::ratio_to_midi_transpose(ratio);
    }

    /// `getName()`.
    pub fn name(&self) -> String {
        if self.mapping_name.is_empty() {
            return self.tuning_name.clone();
        }
        if self.tuning_name.is_empty() {
            return self.mapping_name.clone();
        }
        format!("{} / {}", self.tuning_name, self.mapping_name)
    }

    /// `setName(name)`.
    pub fn set_name(&mut self, name: &str) {
        self.mapping_name.clear();
        self.tuning_name = name.to_string();
    }

    pub fn is_default(&self) -> bool {
        self.default
    }

    /// `stateToJson()`.
    pub fn state_to_json(&self) -> Value {
        let mut data = json!({
            "scale_start_midi_note": self.scale_start_midi_note,
            "reference_midi_note": self.reference_midi_note,
            "tuning_name": self.tuning_name,
            "mapping_name": self.mapping_name,
            "default": self.default,
            "scale": self.scale,
        });
        if !self.keyboard_mapping.is_empty() {
            data["mapping"] = json!(self.keyboard_mapping);
        }
        data
    }

    /// `jsonToState(data)`.
    pub fn json_to_state(&mut self, data: &Value) {
        self.scale_start_midi_note = data["scale_start_midi_note"].as_i64().unwrap_or(0) as i32;
        self.reference_midi_note = data["reference_midi_note"].as_f64().unwrap_or(0.0) as f32;
        self.tuning_name = data["tuning_name"].as_str().unwrap_or_default().to_string();
        self.mapping_name = data["mapping_name"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        if let Some(default) = data.get("default").and_then(Value::as_bool) {
            self.default = default;
        }

        self.scale = data["scale"]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                    .collect()
            })
            .unwrap_or_default();

        self.keyboard_mapping.clear();
        if let Some(mapping) = data.get("mapping").and_then(Value::as_array) {
            self.keyboard_mapping = mapping
                .iter()
                .map(|v| v.as_i64().unwrap_or(0) as i32)
                .collect();
        }

        let scale = self.scale.clone();
        self.load_scale(scale);
    }

    /// The 256-entry tuning table (`tuning_`), for tests.
    pub fn table(&self) -> &[f32; Self::TUNING_SIZE] {
        &self.tuning
    }
}

#[cfg(test)]
mod tests {
    use super::juce_text as jt;
    use super::*;

    #[test]
    fn juce_numbers() {
        assert_eq!(jt::int_value("  -12abc"), -12);
        assert_eq!(jt::int_value("+5"), 0);
        assert_eq!(jt::int_value(""), 0);
        assert_eq!(jt::float_value("701.955"), 701.955f32);
        assert_eq!(jt::float_value("  -0.5e1x"), -5.0);
        assert_eq!(jt::float_value("."), 0.0);
        assert_eq!(jt::float_value("1e"), 1.0);
        assert_eq!(jt::double_value("0001.250"), 1.25);
    }

    #[test]
    fn juce_lines_and_tokens() {
        assert_eq!(jt::lines("a\r\nb\rc\n"), vec!["a", "b", "c", ""]);
        assert_eq!(jt::whitespace_tokens("note  5 "), vec!["note", "", "5", ""]);
        assert!(jt::whitespace_tokens("").is_empty());
    }

    #[test]
    fn default_tuning_is_identity() {
        let tuning = Tuning::new();
        for note in 0..128 {
            assert_eq!(tuning.convert_midi_note(note), note as f32);
        }
    }

    #[test]
    fn note_names() {
        assert_eq!(Tuning::note_to_midi_key("C4"), 60);
        assert_eq!(Tuning::note_to_midi_key("a#-1"), -2);
        assert_eq!(Tuning::note_to_midi_key("h4"), -1);
    }

    /// A blank Scala degree line reads as ratio 0, and upstream's
    /// `ratioToMidiTranspose(0)` is `logf(0)`: a −∞ degree. The parity
    /// metrics cannot gate infinities, so the case lives here.
    #[test]
    fn blank_scala_degree_is_negative_infinity() {
        assert_eq!(read_ratio_to_transpose(""), f32::NEG_INFINITY);

        let lines: Vec<String> = ["blank degree", "2", "", "2/1"]
            .iter()
            .map(|l| l.to_string())
            .collect();
        let mut tuning = Tuning::new();
        tuning.load_scala_lines(&lines);
        assert_eq!(tuning.convert_midi_note(61), f32::NEG_INFINITY);
        assert_eq!(tuning.convert_midi_note(62), 72.0);
    }
}
