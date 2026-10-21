// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `Tuning`: maps MIDI notes to (fractional) tuned notes.
//!
//! Ports `vital/src/common/tuning.h` and `tuning.cpp`: the tuning table
//! (`loadScale`, default and constant tunings, the reference note) that the
//! voice handler reads through `convertMidiNote`.

use sas_simd::common::{MIDI_SIZE, NOTES_PER_OCTAVE};
use sas_simd::utils;

const DEFAULT_MIDI_REFERENCE: i32 = 60;

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

    /// `loadScale(scale)`: `scale` holds the degrees in semitones, starting
    /// at 0 and ending with the period.
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

    /// The 256-entry tuning table (`tuning_`), for tests.
    pub fn table(&self) -> &[f32; Self::TUNING_SIZE] {
        &self.tuning
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tuning_is_identity() {
        let tuning = Tuning::new();
        for note in 0..128 {
            assert_eq!(tuning.convert_midi_note(note), note as f32);
        }
    }

    #[test]
    fn constant_tuning_for_a_single_degree() {
        let mut tuning = Tuning::new();
        tuning.load_scale(vec![0.0]);
        assert_eq!(tuning.convert_midi_note(10), 60.0 + 60.0);
    }
}
