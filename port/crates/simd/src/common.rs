// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Engine-wide constants and the voice event enum.
//!
//! Ports `vital/src/synthesis/framework/common.h`. Names follow the crate
//! rule `kFooBar` → `FOO_BAR`.

/// `mono_float`.
pub type MonoFloat = f32;

pub use sas_common::scalar::PI;
pub const SQRT2: f32 = 1.414_213_562_373_095_048_801_688_724_209_698_f32;
pub const EPSILON: f32 = 1e-16;
/// Largest engine sub-block, in samples at the base rate.
pub const MAX_BUFFER_SIZE: usize = 128;
pub const MAX_OVERSAMPLE: usize = 8;
pub const DEFAULT_SAMPLE_RATE: i32 = 44100;
pub const MIN_NYQUIST_MULT: f32 = 0.453_514_739_23;
pub const MAX_SAMPLE_RATE: i32 = 192_000;
pub const MIDI_SIZE: i32 = 128;
pub const MIDI_TRACK_CENTER: i32 = 60;

pub const MIDI_0_FREQUENCY: f32 = 8.175_798_915_6;
pub const DBFS_INCREASE: f32 = 6.0;
pub const DEGREES_PER_CYCLE: i32 = 360;
pub const MS_PER_SEC: i32 = 1000;
pub const NOTES_PER_OCTAVE: i32 = 12;
pub const CENTS_PER_NOTE: i32 = 100;
pub const CENTS_PER_OCTAVE: i32 = NOTES_PER_OCTAVE * CENTS_PER_NOTE;

/// Pulses per quarter note.
pub const PPQ: i32 = 960;
pub const VOICE_KILL_TIME: f32 = 0.05;
pub const NUM_MIDI_CHANNELS: i32 = 16;
pub const FIRST_MIDI_CHANNEL: i32 = 0;
pub const LAST_MIDI_CHANNEL: i32 = NUM_MIDI_CHANNELS - 1;

/// `vital::VoiceEvent`. Trigger values carry these as floats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum VoiceEvent {
    Invalid = 0,
    VoiceIdle,
    VoiceOn,
    VoiceHold,
    VoiceDecay,
    VoiceOff,
    VoiceKill,
}

impl VoiceEvent {
    /// `kNumVoiceEvents`.
    pub const NUM_VOICE_EVENTS: i32 = 7;

    /// The event as a trigger value (C++ converts the enum to `float`).
    #[inline(always)]
    pub const fn as_f32(self) -> f32 {
        self as i32 as f32
    }
}
