// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the wrapper needs from the sound engine.
//!
//! These are the calls `SynthPlugin` (`vital/src/plugin/synth_plugin.cpp`)
//! makes on `SynthBase`, its `SoundEngine` and its `MidiManager`. The shipped
//! plugin runs on [`crate::SoundEngineHost`]; [`SilentEngine`] outputs
//! silence, for tests that need no engine.

use sas_engine::Tuning;
use sas_preset::PresetState;

/// The sound engine as the plugin drives it. Every method is called from the
/// audio thread.
pub trait Engine: Send + 'static {
    /// `SoundEngine::setSampleRate` and `MidiManager::setSampleRate`
    /// (`SynthPlugin::prepareToPlay`).
    fn set_sample_rate(&mut self, sample_rate: f32);

    /// Silences every voice and clears effect tails (the host reset the
    /// plugin, e.g. on transport jumps or reactivation).
    fn reset(&mut self);

    /// Loads a whole preset: controls, modulation routing, wavetables,
    /// sample, LFO shapes (`LoadSave::jsonToState` on the engine). Right
    /// after, the wrapper calls [`set_control`](Self::set_control) for every
    /// host parameter that no longer matches the preset.
    fn load_preset(&mut self, preset: &PresetState);

    /// The tuning the engine plays with (`SoundEngine::setTuning` with
    /// `SynthBase::tuning_`). Called right after every
    /// [`load_preset`](Self::load_preset), as upstream's
    /// `setStateInformation` loads the tuning after the preset.
    fn set_tuning(&mut self, tuning: &Tuning);

    /// Sets control `name` (a registry name) to `value`, as
    /// `SynthBase::valueChangedExternal` does for a host parameter change.
    fn set_control(&mut self, name: &'static str, value: f32);

    /// `SoundEngine::setBpm` from the host transport.
    fn set_bpm(&mut self, bpm: f64);

    /// `SoundEngine::correctToTime`: the song position in seconds at the
    /// start of the next [`process`](Self::process) call.
    fn correct_to_time(&mut self, seconds: f64);

    /// One short MIDI message (`MidiManager::processMidiMessage`), at
    /// `sample_offset` into the next [`process`](Self::process) call. MPE
    /// arrives as ordinary per-channel messages.
    fn process_midi(&mut self, message: [u8; 3], sample_offset: usize);

    /// Renders `left.len()` samples (at most `MAX_BUFFER_SIZE`, 128) of
    /// stereo output (`SynthBase::processAudio`).
    fn process(&mut self, left: &mut [f32], right: &mut [f32]);
}

/// An engine that outputs silence (tests that need no sound engine).
#[derive(Debug, Default)]
pub struct SilentEngine;

impl Engine for SilentEngine {
    fn set_sample_rate(&mut self, _sample_rate: f32) {}

    fn reset(&mut self) {}

    fn load_preset(&mut self, _preset: &PresetState) {}

    fn set_tuning(&mut self, _tuning: &Tuning) {}

    fn set_control(&mut self, _name: &'static str, _value: f32) {}

    fn set_bpm(&mut self, _bpm: f64) {}

    fn correct_to_time(&mut self, _seconds: f64) {}

    fn process_midi(&mut self, _message: [u8; 3], _sample_offset: usize) {}

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
    }
}
