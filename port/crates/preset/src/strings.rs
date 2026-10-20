// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Display string tables referenced by the parameter registry.
//!
//! Ports the tables of `vital/src/interface/look_and_feel/synth_strings.h`
//! that `synth_parameters.cpp` points `ValueDetails::string_lookup` at.

/// A display string table from upstream `synth_strings.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StringTable {
    CompressorBand,
    DelayStyle,
    Destination,
    DistortionFilterOrder,
    DistortionType,
    EqBandMode,
    EqHighMode,
    EqLowMode,
    FilterModel,
    FilterStyle,
    FrequencySync,
    OffOn,
    Oversampling,
    PhaseDistortion,
    Random,
    SpectralMorph,
    StereoMode,
    Sync,
    SyncedFrequency,
    UnisonStack,
    VoiceOverride,
    VoicePriority,
}

impl StringTable {
    /// Every table, in declaration order.
    pub const ALL: &'static [StringTable] = &[
        StringTable::CompressorBand,
        StringTable::DelayStyle,
        StringTable::Destination,
        StringTable::DistortionFilterOrder,
        StringTable::DistortionType,
        StringTable::EqBandMode,
        StringTable::EqHighMode,
        StringTable::EqLowMode,
        StringTable::FilterModel,
        StringTable::FilterStyle,
        StringTable::FrequencySync,
        StringTable::OffOn,
        StringTable::Oversampling,
        StringTable::PhaseDistortion,
        StringTable::Random,
        StringTable::SpectralMorph,
        StringTable::StereoMode,
        StringTable::Sync,
        StringTable::SyncedFrequency,
        StringTable::UnisonStack,
        StringTable::VoiceOverride,
        StringTable::VoicePriority,
    ];

    /// The upstream identifier of the table (e.g. `kOffOnNames`).
    pub fn upstream_name(self) -> &'static str {
        match self {
            StringTable::CompressorBand => "kCompressorBandNames",
            StringTable::DelayStyle => "kDelayStyleNames",
            StringTable::Destination => "kDestinationNames",
            StringTable::DistortionFilterOrder => "kDistortionFilterOrderNames",
            StringTable::DistortionType => "kDistortionTypeNames",
            StringTable::EqBandMode => "kEqBandModeNames",
            StringTable::EqHighMode => "kEqHighModeNames",
            StringTable::EqLowMode => "kEqLowModeNames",
            StringTable::FilterModel => "kFilterModelNames",
            StringTable::FilterStyle => "kFilterStyleNames",
            StringTable::FrequencySync => "kFrequencySyncNames",
            StringTable::OffOn => "kOffOnNames",
            StringTable::Oversampling => "kOversamplingNames",
            StringTable::PhaseDistortion => "kPhaseDistortionNames",
            StringTable::Random => "kRandomNames",
            StringTable::SpectralMorph => "kSpectralMorphNames",
            StringTable::StereoMode => "kStereoModeNames",
            StringTable::Sync => "kSyncNames",
            StringTable::SyncedFrequency => "kSyncedFrequencyNames",
            StringTable::UnisonStack => "kUnisonStackNames",
            StringTable::VoiceOverride => "kVoiceOverrideNames",
            StringTable::VoicePriority => "kVoicePriorityNames",
        }
    }

    /// The table's entries.
    pub fn values(self) -> &'static [&'static str] {
        match self {
            StringTable::CompressorBand => COMPRESSOR_BAND_NAMES,
            StringTable::DelayStyle => DELAY_STYLE_NAMES,
            StringTable::Destination => DESTINATION_NAMES,
            StringTable::DistortionFilterOrder => DISTORTION_FILTER_ORDER_NAMES,
            StringTable::DistortionType => DISTORTION_TYPE_NAMES,
            StringTable::EqBandMode => EQ_BAND_MODE_NAMES,
            StringTable::EqHighMode => EQ_HIGH_MODE_NAMES,
            StringTable::EqLowMode => EQ_LOW_MODE_NAMES,
            StringTable::FilterModel => FILTER_MODEL_NAMES,
            StringTable::FilterStyle => FILTER_STYLE_NAMES,
            StringTable::FrequencySync => FREQUENCY_SYNC_NAMES,
            StringTable::OffOn => OFF_ON_NAMES,
            StringTable::Oversampling => OVERSAMPLING_NAMES,
            StringTable::PhaseDistortion => PHASE_DISTORTION_NAMES,
            StringTable::Random => RANDOM_NAMES,
            StringTable::SpectralMorph => SPECTRAL_MORPH_NAMES,
            StringTable::StereoMode => STEREO_MODE_NAMES,
            StringTable::Sync => SYNC_NAMES,
            StringTable::SyncedFrequency => SYNCED_FREQUENCY_NAMES,
            StringTable::UnisonStack => UNISON_STACK_NAMES,
            StringTable::VoiceOverride => VOICE_OVERRIDE_NAMES,
            StringTable::VoicePriority => VOICE_PRIORITY_NAMES,
        }
    }
}

const COMPRESSOR_BAND_NAMES: &[&str] = &["Multiband", "Low Band", "High Band", "Single Band"];
const DELAY_STYLE_NAMES: &[&str] = &["Mono", "Stereo", "Ping Pong", "Mid Ping Pong"];
const DESTINATION_NAMES: &[&str] = &[
    "FILTER 1",
    "FILTER 2",
    "FILTER 1+2",
    "EFFECTS",
    "DIRECT OUT",
    "CHORUS",
    "COMPRESSOR",
    "DELAY",
    "DISTORTION",
    "EQ",
    "FX FILTER",
    "FLANGER",
    "PHASER",
    "REVERB",
];
const DISTORTION_FILTER_ORDER_NAMES: &[&str] = &["None", "Pre", "Post"];
const DISTORTION_TYPE_NAMES: &[&str] = &[
    "Soft Clip",
    "Hard Clip",
    "Linear Fold",
    "Sine Fold",
    "Bit Crush",
    "Down Sample",
];
const EQ_BAND_MODE_NAMES: &[&str] = &["Shelf", "Notch"];
const EQ_HIGH_MODE_NAMES: &[&str] = &["Shelf", "Low Pass"];
const EQ_LOW_MODE_NAMES: &[&str] = &["Shelf", "High Pass"];
const FILTER_MODEL_NAMES: &[&str] = &[
    "Analog", "Dirty", "Ladder", "Digital", "Diode", "Formant", "Comb", "Phaser",
];
const FILTER_STYLE_NAMES: &[&str] = &["12dB", "24dB", "Notch Blend", "Notch Spread", "B/P/N"];
const FREQUENCY_SYNC_NAMES: &[&str] = &[
    "Seconds",
    "Tempo",
    "Tempo Dotted",
    "Tempo Triplets",
    "Keytrack",
];
const OFF_ON_NAMES: &[&str] = &["Off", "On"];
const OVERSAMPLING_NAMES: &[&str] = &["1x", "2x", "4x", "8x"];
const PHASE_DISTORTION_NAMES: &[&str] = &[
    "None",
    "Sync",
    "Formant",
    "Quantize",
    "Bend",
    "Squeeze",
    "Pulse",
    "FM <- Osc",
    "FM <- Osc",
    "FM <- Sample",
    "RM <- Osc",
    "RM <- Osc",
    "RM <- Sample",
];
const RANDOM_NAMES: &[&str] = &[
    "Perlin",
    "Sample & Hold",
    "Sine Interpolate",
    "Lorenz Attractor",
];
const SPECTRAL_MORPH_NAMES: &[&str] = &[
    "None",
    "Vocode",
    "Formant Scale",
    "Harmonic Stretch",
    "Inharmonic Stretch",
    "Smear",
    "Random Amplitudes",
    "Low Pass",
    "High Pass",
    "Phase Disperse",
    "Shepard Tone",
    "Spectral Time Skew",
];
const STEREO_MODE_NAMES: &[&str] = &["SPREAD", "ROTATE"];
const SYNC_NAMES: &[&str] = &[
    "Trigger",
    "Sync",
    "Envelope",
    "Sustain Envelope",
    "Loop Point",
    "Loop Hold",
];
const SYNCED_FREQUENCY_NAMES: &[&str] = &[
    "Freeze", "32/1", "16/1", "8/1", "4/1", "2/1", "1/1", "1/2", "1/4", "1/8", "1/16", "1/32",
    "1/64",
];
const UNISON_STACK_NAMES: &[&str] = &[
    "Unison",
    "Center Drop 12",
    "Center Drop 24",
    "Octave",
    "2x Octave",
    "Power Chord",
    "2x Power Chord",
    "Major Chord",
    "Minor Chord",
    "Harmonics",
    "Odd Harmonics",
];
const VOICE_OVERRIDE_NAMES: &[&str] = &["Kill", "Steal"];
const VOICE_PRIORITY_NAMES: &[&str] = &["Newest", "Oldest", "Highest", "Lowest", "Round Robin"];
