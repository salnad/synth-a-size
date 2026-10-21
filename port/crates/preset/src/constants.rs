// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Engine-wide counts the preset format depends on.
//!
//! Ports the parts of `vital/src/common/synth_constants.h` and
//! `vital/src/synthesis/framework/common.h` the registry and loader use, plus
//! the enum sizes `synth_parameters.cpp` takes ranges from.

pub const NUM_LFOS: usize = 8;
pub const NUM_OSCILLATORS: usize = 3;
pub const NUM_OSCILLATOR_WAVE_FRAMES: i32 = 257;
pub const NUM_ENVELOPES: usize = 6;
pub const NUM_RANDOM_LFOS: usize = 4;
pub const NUM_MACROS: usize = 4;
pub const NUM_FILTERS: usize = 2;
pub const MAX_POLYPHONY: i32 = 33;
pub const MAX_MODULATION_CONNECTIONS: usize = 64;
pub const NOTES_PER_OCTAVE: i32 = 12;
pub const DEGREES_PER_CYCLE: i32 = 360;

/// `constants::SourceDestination` (filter 1, filter 2, both, effects, direct out).
pub const NUM_SOURCE_DESTINATIONS: i32 = 5;
/// `constants::Effect`, in effect-chain order.
pub const NUM_EFFECTS: usize = 9;
/// `constants::kFilterFx`.
pub const EFFECT_FILTER_FX: i32 = 5;
/// `constants::FilterModel`.
pub const NUM_FILTER_MODELS: i32 = 8;

/// `PredefinedWaveFrames::kNumShapes`.
pub const NUM_PREDEFINED_SHAPES: i32 = 6;
/// `MultibandCompressor::kNumBandOptions`.
pub const NUM_COMPRESSOR_BAND_OPTIONS: i32 = 4;
/// `VoiceHandler::kNumVoicePriorities` and `VoiceHandler::kRoundRobin`.
pub const NUM_VOICE_PRIORITIES: i32 = 5;
pub const VOICE_PRIORITY_ROUND_ROBIN: i32 = 4;
/// `VoiceHandler::kNumVoiceOverrides` and `VoiceHandler::kKill`.
pub const NUM_VOICE_OVERRIDES: i32 = 2;
pub const VOICE_OVERRIDE_KILL: i32 = 0;
/// `SynthLfo::kNumSyncTypes` and `SynthLfo::kNumSyncOptions`.
pub const NUM_LFO_SYNC_TYPES: i32 = 6;
pub const NUM_LFO_SYNC_OPTIONS: i32 = 5;
/// `RandomLfo::kNumStyles`.
pub const NUM_RANDOM_LFO_STYLES: i32 = 4;
/// `SynthOscillator` enum sizes.
pub const NUM_UNISON_STACK_TYPES: i32 = 11;
pub const NUM_DISTORTION_TYPES: i32 = 13;
pub const NUM_SPECTRAL_MORPH_TYPES: i32 = 12;
/// `SynthOscillator::DistortionType` values the upgrade chain tests for.
pub const DISTORTION_SYNC: f32 = 1.0;
pub const DISTORTION_FORMANT: f32 = 2.0;
/// `SynthOscillator::kLowPass` (spectral morph).
pub const SPECTRAL_MORPH_LOW_PASS: i32 = 7;

/// `Distortion::kMinDrive` / `kMaxDrive` (dB).
pub const DISTORTION_MIN_DRIVE: f64 = -30.0;
pub const DISTORTION_MAX_DRIVE: f64 = 30.0;
/// `DigitalSvf::kMinGain` / `kMaxGain` (dB).
pub const DIGITAL_SVF_MIN_GAIN: f64 = -15.0;
pub const DIGITAL_SVF_MAX_GAIN: f64 = 15.0;

/// `utils::factorial`.
pub const fn factorial(value: usize) -> usize {
    let mut result = 1;
    let mut i = 2;
    while i <= value {
        result *= i;
        i += 1;
    }
    result
}
