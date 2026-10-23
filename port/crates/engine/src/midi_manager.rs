// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `MidiManager`: turns short MIDI messages into [`SoundEngine`] calls.
//!
//! Ports `vital/src/common/midi_manager.{h,cpp}` (`processMidiMessage` and
//! its helpers, MIDI learn, bank and folder selection) and the parts of
//! JUCE's `MPEZoneLayout` it reads. The GUI callbacks (pitch and mod wheel
//! display) are not part of the engine; a learned controller's value reaches
//! the caller through `process_midi_message`'s listener
//! (`Listener::valueChangedThroughMidi`). Saving the learn map to the user
//! config (`LoadSave::saveMidiMapConfig`) is the caller's job: the manager
//! reports when the map changed.

use std::collections::BTreeMap;

use sas_preset::{lookup, ParamInfo, ValueScale};

use crate::sound_engine::{SoundEngine, NUM_MIDI_CHANNELS};

const MIDI_CONTROL_BITS: i32 = 7;
/// `kHighResolutionMax`.
const HIGH_RESOLUTION_MAX: f32 = ((1 << (2 * MIDI_CONTROL_BITS)) - 1) as f32;
/// `kControlMax`.
const CONTROL_MAX: f32 = ((1 << MIDI_CONTROL_BITS) - 1) as f32;

const NOTE_OFF: u8 = 0x80;
const NOTE_ON: u8 = 0x90;
const AFTERTOUCH: u8 = 0xa0;
const CONTROLLER: u8 = 0xb0;
const PROGRAM_CHANGE: u8 = 0xc0;
const CHANNEL_PRESSURE: u8 = 0xd0;
const PITCH_WHEEL: u8 = 0xe0;

const BANK_SELECT: u8 = 0x00;
const MOD_WHEEL: u8 = 0x01;
const FOLDER_SELECT: u8 = 0x20;
const SUSTAIN_PEDAL: u8 = 0x40;
const SOSTENUTO_PEDAL: u8 = 0x42;
const SOFT_PEDAL_ON: u8 = 0x43;
const SLIDE: u8 = 0x4a;
const LSB_PRESSURE: u8 = 0x66;
const LSB_SLIDE: u8 = 0x6a;
const ALL_SOUNDS_OFF: u8 = 0x78;
const ALL_CONTROLLERS_OFF: u8 = 0x79;
const ALL_NOTES_OFF: u8 = 0x7b;

const RPN_MSB: u8 = 101;
const RPN_LSB: u8 = 100;
const DATA_ENTRY_MSB: u8 = 6;
const MPE_CONFIGURATION_RPN: i32 = 6;

/// `toHighResolutionValue(msb, lsb)`.
fn to_high_resolution_value(msb: i32, lsb: i32) -> f32 {
    if lsb < 0 {
        return msb as f32 / CONTROL_MAX;
    }
    ((msb << MIDI_CONTROL_BITS) + lsb) as f32 / HIGH_RESOLUTION_MAX
}

/// One MPE zone of JUCE's `MPEZoneLayout` (channels are 1-based, as in
/// JUCE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Zone {
    lower: bool,
    num_member_channels: i32,
}

impl Zone {
    fn is_active(&self) -> bool {
        self.num_member_channels > 0
    }

    fn master_channel(&self) -> i32 {
        if self.lower {
            1
        } else {
            16
        }
    }

    fn first_member_channel(&self) -> i32 {
        if self.lower {
            2
        } else {
            15
        }
    }

    fn last_member_channel(&self) -> i32 {
        if self.lower {
            1 + self.num_member_channels
        } else {
            16 - self.num_member_channels
        }
    }
}

/// The parts of `juce::MPEZoneLayout` that `MidiManager` uses: two zones,
/// set by `setLowerZone`/`setUpperZone` or by the MPE configuration RPN
/// (RPN 6) on channel 1 or 16.
#[derive(Clone, Debug)]
struct MpeZoneLayout {
    lower: Zone,
    upper: Zone,
    rpn_msb: [i32; 16],
    rpn_lsb: [i32; 16],
}

impl MpeZoneLayout {
    fn new() -> Self {
        Self {
            lower: Zone {
                lower: true,
                num_member_channels: 0,
            },
            upper: Zone {
                lower: false,
                num_member_channels: 0,
            },
            rpn_msb: [-1; 16],
            rpn_lsb: [-1; 16],
        }
    }

    /// `setLowerZone(num_member_channels)`: a zone may hold at most 15
    /// members, and an overlapping upper zone shrinks to fit.
    fn set_lower_zone(&mut self, num_member_channels: i32) {
        let num = num_member_channels.clamp(0, 15);
        self.lower.num_member_channels = num;
        if num + self.upper.num_member_channels >= 15 {
            self.upper.num_member_channels = (14 - num).max(0);
        }
    }

    /// `setUpperZone(num_member_channels)`.
    fn set_upper_zone(&mut self, num_member_channels: i32) {
        let num = num_member_channels.clamp(0, 15);
        self.upper.num_member_channels = num;
        if num + self.lower.num_member_channels >= 15 {
            self.lower.num_member_channels = (14 - num).max(0);
        }
    }

    /// `processNextMidiEvent(message)` for controller messages: tracks RPN
    /// selection per channel and applies the MPE configuration message.
    fn process_controller(&mut self, channel: usize, controller: u8, value: u8) {
        match controller {
            RPN_MSB => self.rpn_msb[channel] = value as i32,
            RPN_LSB => self.rpn_lsb[channel] = value as i32,
            DATA_ENTRY_MSB => {
                if self.rpn_msb[channel] < 0 || self.rpn_lsb[channel] < 0 {
                    return;
                }
                let rpn = (self.rpn_msb[channel] << 7) + self.rpn_lsb[channel];
                if rpn != MPE_CONFIGURATION_RPN || value >= 16 {
                    return;
                }
                if channel == 0 {
                    self.set_lower_zone(value as i32);
                } else if channel == 15 {
                    self.set_upper_zone(value as i32);
                }
            }
            _ => {}
        }
    }
}

/// `MidiManager::midi_map`: controller number → the parameters it sets, by
/// name. A parameter may be learned on several controllers.
pub type MidiLearnMap = BTreeMap<i32, BTreeMap<&'static str, &'static ParamInfo>>;

/// `MidiManager`'s state.
#[derive(Clone, Debug)]
pub struct MidiManager {
    // Upstream writes these (and resets them on arm) but never reads them.
    #[allow(dead_code)]
    current_bank: i32,
    #[allow(dead_code)]
    current_folder: i32,
    #[allow(dead_code)]
    current_preset: i32,
    armed_value: Option<&'static ParamInfo>,
    midi_learn_map: MidiLearnMap,
    /// Set when the learn map changes on a controller message, where
    /// upstream calls `LoadSave::saveMidiMapConfig`.
    midi_learn_changed: bool,
    mpe_enabled: bool,
    mpe_zone_layout: MpeZoneLayout,
    msb_pressure_values: [i32; 16],
    lsb_pressure_values: [i32; 16],
    msb_slide_values: [i32; 16],
    lsb_slide_values: [i32; 16],
}

impl Default for MidiManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiManager {
    /// `MidiManager(...)`: MPE off, a 15-member lower zone.
    pub fn new() -> Self {
        let mut mpe_zone_layout = MpeZoneLayout::new();
        mpe_zone_layout.set_lower_zone(NUM_MIDI_CHANNELS - 1);
        Self {
            current_bank: -1,
            current_folder: -1,
            current_preset: -1,
            armed_value: None,
            midi_learn_map: MidiLearnMap::new(),
            midi_learn_changed: false,
            mpe_enabled: false,
            mpe_zone_layout,
            msb_pressure_values: [0; 16],
            lsb_pressure_values: [-1; 16],
            msb_slide_values: [0; 16],
            lsb_slide_values: [-1; 16],
        }
    }

    /// `armMidiLearn(name)`: the next controller message learns `name`.
    /// Unknown names arm nothing (upstream's `Parameters::getDetails` has no
    /// entry to return for them).
    pub fn arm_midi_learn(&mut self, name: &str) {
        self.current_bank = -1;
        self.current_folder = -1;
        self.current_preset = -1;
        self.armed_value = lookup(name);
    }

    /// `cancelMidiLearn()`.
    pub fn cancel_midi_learn(&mut self) {
        self.armed_value = None;
    }

    /// `clearMidiLearn(name)`: forgets every controller learned for `name`.
    /// Returns whether any was, where upstream saves the config.
    pub fn clear_midi_learn(&mut self, name: &str) -> bool {
        let mut cleared = false;
        for controls in self.midi_learn_map.values_mut() {
            cleared |= controls.remove(name).is_some();
        }
        cleared
    }

    /// `isMidiMapped(name)`.
    pub fn is_midi_mapped(&self, name: &str) -> bool {
        self.midi_learn_map
            .values()
            .any(|controls| controls.contains_key(name))
    }

    /// `getMidiLearnMap()`.
    pub fn midi_learn_map(&self) -> &MidiLearnMap {
        &self.midi_learn_map
    }

    /// `setMidiLearnMap(map)`.
    pub fn set_midi_learn_map(&mut self, midi_learn_map: MidiLearnMap) {
        self.midi_learn_map = midi_learn_map;
    }

    /// Whether a controller message changed the learn map since the last
    /// call (upstream saves the config there).
    pub fn take_midi_learn_changed(&mut self) -> bool {
        std::mem::take(&mut self.midi_learn_changed)
    }

    /// `midiInput(midi_id, value)`: a controller learns the armed parameter,
    /// then sets every parameter learned on it, scaled from 0–127 to the
    /// parameter's range (rounded for indexed ones), through `listener`.
    pub fn midi_input(
        &mut self,
        midi_id: i32,
        value: f32,
        listener: &mut dyn FnMut(&'static str, f32),
    ) {
        if let Some(armed) = self.armed_value.take() {
            self.midi_learn_map
                .entry(midi_id)
                .or_default()
                .insert(armed.name.as_str(), armed);
            self.midi_learn_changed = true;
        }

        if let Some(controls) = self.midi_learn_map.get(&midi_id) {
            for (name, details) in controls {
                let percent = value / CONTROL_MAX;
                let range = details.max - details.min;
                let mut translated = percent * range + details.min;
                if details.value_scale == ValueScale::Indexed {
                    translated = translated.round();
                }
                listener(name, translated);
            }
        }
    }

    /// `setMpeEnabled(enabled)`.
    pub fn set_mpe_enabled(&mut self, enabled: bool) {
        self.mpe_enabled = enabled;
    }

    fn lower_zone_start_channel(&self) -> i32 {
        self.mpe_zone_layout.lower.first_member_channel() - 1
    }

    fn upper_zone_start_channel(&self) -> i32 {
        self.mpe_zone_layout.upper.last_member_channel() - 1
    }

    fn lower_zone_end_channel(&self) -> i32 {
        self.mpe_zone_layout.lower.last_member_channel() - 1
    }

    fn upper_zone_end_channel(&self) -> i32 {
        self.mpe_zone_layout.upper.first_member_channel() - 1
    }

    fn lower_master_channel(&self) -> i32 {
        self.mpe_zone_layout.lower.master_channel() - 1
    }

    fn upper_master_channel(&self) -> i32 {
        self.mpe_zone_layout.upper.master_channel() - 1
    }

    fn is_mpe_channel_master_lower_zone(&self, channel: i32) -> bool {
        self.mpe_enabled
            && self.mpe_zone_layout.lower.is_active()
            && self.lower_master_channel() == channel
    }

    fn is_mpe_channel_master_upper_zone(&self, channel: i32) -> bool {
        self.mpe_enabled
            && self.mpe_zone_layout.upper.is_active()
            && self.upper_master_channel() == channel
    }

    fn process_all_notes_off(&self, engine: &SoundEngine, sample: i32, channel: i32) {
        if self.is_mpe_channel_master_lower_zone(channel) {
            engine.all_notes_off_range(
                sample,
                self.lower_zone_start_channel(),
                self.lower_zone_end_channel(),
            );
        } else if self.is_mpe_channel_master_upper_zone(channel) {
            engine.all_notes_off_range(
                sample,
                self.upper_zone_start_channel(),
                self.upper_zone_end_channel(),
            );
        } else {
            engine.all_notes_off_channel(sample, channel);
        }
    }

    fn process_sustain(&self, engine: &SoundEngine, on: bool, sample: i32, channel: i32) {
        if self.is_mpe_channel_master_lower_zone(channel) {
            let (from, to) = (
                self.lower_zone_start_channel(),
                self.lower_zone_end_channel(),
            );
            if on {
                engine.sustain_on_range(from, to);
            } else {
                engine.sustain_off_range(sample, from, to);
            }
        } else if self.is_mpe_channel_master_upper_zone(channel) {
            let (from, to) = (
                self.upper_zone_start_channel(),
                self.upper_zone_end_channel(),
            );
            if on {
                engine.sustain_on_range(from, to);
            } else {
                engine.sustain_off_range(sample, from, to);
            }
        } else if on {
            engine.sustain_on(channel);
        } else {
            engine.sustain_off(sample, channel);
        }
    }

    fn process_sostenuto(&self, engine: &SoundEngine, on: bool, sample: i32, channel: i32) {
        if self.is_mpe_channel_master_lower_zone(channel) {
            let (from, to) = (
                self.lower_zone_start_channel(),
                self.lower_zone_end_channel(),
            );
            if on {
                engine.sostenuto_on_range(from, to);
            } else {
                engine.sostenuto_off_range(sample, from, to);
            }
        } else if self.is_mpe_channel_master_upper_zone(channel) {
            let (from, to) = (
                self.upper_zone_start_channel(),
                self.upper_zone_end_channel(),
            );
            if on {
                engine.sostenuto_on_range(from, to);
            } else {
                engine.sostenuto_off_range(sample, from, to);
            }
        } else if on {
            engine.sostenuto_on(channel);
        } else {
            engine.sostenuto_off(sample, channel);
        }
    }

    fn process_pitch_bend(&self, engine: &SoundEngine, pitch_wheel_value: i32, channel: i32) {
        let percent = pitch_wheel_value as f32 / HIGH_RESOLUTION_MAX;
        let value = 2.0 * percent - 1.0;

        if self.is_mpe_channel_master_lower_zone(channel) {
            let master = self.lower_master_channel();
            engine.set_zoned_pitch_wheel(value, master, master + 1);
            engine.set_zoned_pitch_wheel(
                value,
                self.lower_zone_start_channel(),
                self.lower_zone_end_channel(),
            );
        } else if self.is_mpe_channel_master_upper_zone(channel) {
            let master = self.upper_master_channel();
            engine.set_zoned_pitch_wheel(value, master, master + 1);
            engine.set_zoned_pitch_wheel(
                value,
                self.upper_zone_start_channel(),
                self.upper_zone_end_channel(),
            );
        } else if self.mpe_enabled {
            engine.set_pitch_wheel(value, channel);
        } else {
            engine.set_zoned_pitch_wheel(value, channel, channel);
        }
    }

    fn process_pressure(&self, engine: &SoundEngine, sample: i32, channel: i32) {
        let ch = channel as usize;
        let value =
            to_high_resolution_value(self.msb_pressure_values[ch], self.lsb_pressure_values[ch]);
        if self.is_mpe_channel_master_lower_zone(channel) {
            engine.set_channel_range_aftertouch(
                self.lower_zone_start_channel(),
                self.lower_zone_end_channel(),
                value,
                0,
            );
        } else if self.is_mpe_channel_master_upper_zone(channel) {
            engine.set_channel_range_aftertouch(
                self.upper_zone_start_channel(),
                self.upper_zone_end_channel(),
                value,
                0,
            );
        } else {
            engine.set_channel_aftertouch(channel, value, sample);
        }
    }

    /// `processSlide`. For a zone's master channel upstream passes
    /// `(value, start, end, 0)` to `setChannelRangeSlide(from, to, value,
    /// sample)`, so the value is truncated into `from_channel` and the end
    /// channel becomes the value; that call is reproduced as written.
    fn process_slide(&self, engine: &SoundEngine, sample: i32, channel: i32) {
        let ch = channel as usize;
        let value = to_high_resolution_value(self.msb_slide_values[ch], self.lsb_slide_values[ch]);
        if self.is_mpe_channel_master_lower_zone(channel) {
            engine.set_channel_range_slide(
                value as i32,
                self.lower_zone_start_channel(),
                self.lower_zone_end_channel() as f32,
                0,
            );
        } else if self.is_mpe_channel_master_upper_zone(channel) {
            engine.set_channel_range_slide(
                value as i32,
                self.upper_zone_start_channel(),
                self.upper_zone_end_channel() as f32,
                0,
            );
        } else {
            engine.set_channel_slide(channel, value, sample);
        }
    }

    /// `processMidiMessage(message, sample_position)` for one short message.
    /// `listener` receives the parameters a learned controller sets
    /// (`Listener::valueChangedThroughMidi`).
    pub fn process_midi_message(
        &mut self,
        engine: &SoundEngine,
        message: [u8; 3],
        sample: i32,
        listener: &mut dyn FnMut(&'static str, f32),
    ) {
        let status = message[0];
        let kind = status & 0xf0;
        let channel = (status & 0x0f) as i32;
        let ch = channel as usize;
        if kind == CONTROLLER {
            self.mpe_zone_layout
                .process_controller(ch, message[1], message[2]);
        }

        match kind {
            PROGRAM_CHANGE => {}
            NOTE_ON => {
                let velocity = message[2];
                let note = message[1] as i32;
                if velocity != 0 {
                    engine.note_on(note, velocity as f32 / CONTROL_MAX, sample, channel);
                } else {
                    engine.note_off(note, velocity as f32 / CONTROL_MAX, sample, channel);
                }
            }
            NOTE_OFF => {
                let velocity = message[2] as f32 / CONTROL_MAX;
                engine.note_off(message[1] as i32, velocity, sample, channel);
            }
            AFTERTOUCH => {
                let value = message[2] as f32 / CONTROL_MAX;
                engine.set_aftertouch(message[1] as i32, value, sample, channel);
            }
            CHANNEL_PRESSURE => {
                self.msb_pressure_values[ch] = message[1] as i32;
                self.process_pressure(engine, sample, channel);
            }
            PITCH_WHEEL => {
                let value = (message[1] as i32) | ((message[2] as i32) << 7);
                self.process_pitch_bend(engine, value, channel);
            }
            CONTROLLER => {
                let value = message[2];
                match message[1] {
                    SLIDE => {
                        self.msb_slide_values[ch] = value as i32;
                        self.process_slide(engine, sample, channel);
                    }
                    LSB_PRESSURE => {
                        self.lsb_pressure_values[ch] = value as i32;
                        self.process_pressure(engine, sample, channel);
                    }
                    LSB_SLIDE => {
                        self.lsb_slide_values[ch] = value as i32;
                        self.process_slide(engine, sample, channel);
                    }
                    SUSTAIN_PEDAL => self.process_sustain(engine, value >= 64, sample, channel),
                    SOSTENUTO_PEDAL => self.process_sostenuto(engine, value >= 64, sample, channel),
                    SOFT_PEDAL_ON => {}
                    MOD_WHEEL => engine.set_mod_wheel(value as f32 / CONTROL_MAX, channel),
                    ALL_NOTES_OFF | ALL_CONTROLLERS_OFF => {
                        self.process_all_notes_off(engine, sample, channel);
                        return;
                    }
                    ALL_SOUNDS_OFF => engine.all_sounds_off(),
                    BANK_SELECT => {
                        self.current_bank = value as i32;
                        return;
                    }
                    FOLDER_SELECT => {
                        self.current_folder = value as i32;
                        return;
                    }
                    _ => {}
                }
                // Every controller that did not return above, handled or
                // not, also goes to MIDI learn.
                self.midi_input(message[1] as i32, value as f32, listener);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_resolution_values() {
        assert_eq!(to_high_resolution_value(127, -1), 1.0);
        assert_eq!(to_high_resolution_value(127, 127), 1.0);
        assert_eq!(to_high_resolution_value(64, 0), 8192.0 / 16383.0);
    }

    #[test]
    fn default_lower_zone_has_fifteen_members() {
        let manager = MidiManager::new();
        assert_eq!(manager.lower_master_channel(), 0);
        assert_eq!(manager.lower_zone_start_channel(), 1);
        assert_eq!(manager.lower_zone_end_channel(), 15);
        assert!(!manager.mpe_zone_layout.upper.is_active());
    }

    #[test]
    fn mpe_configuration_rpn_sets_the_zones() {
        let mut layout = MpeZoneLayout::new();
        layout.set_lower_zone(15);
        for (controller, value) in [(RPN_MSB, 0), (RPN_LSB, 6), (DATA_ENTRY_MSB, 7)] {
            layout.process_controller(15, controller, value);
        }
        assert_eq!(layout.upper.num_member_channels, 7);
        assert_eq!(layout.lower.num_member_channels, 7);
    }
}
