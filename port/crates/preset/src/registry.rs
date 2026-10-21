// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The parameter registry.
//!
//! Ports `vital/src/common/synth_parameters.{h,cpp}`: every parameter's name,
//! range, default, value scale, display details and the version it was added
//! in, expanded per envelope / LFO / random LFO / oscillator / filter /
//! modulation slot exactly like `ValueDetailsLookup`, and ordered like
//! `Parameters::getDetails(index)` (by version added, then name).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::constants::*;
use crate::strings::StringTable;

/// `ValueDetails::ValueScale`: how a parameter's stored value maps to its
/// displayed value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueScale {
    Indexed,
    Linear,
    Quadratic,
    Cubic,
    Quartic,
    SquareRoot,
    Exponential,
}

impl ValueScale {
    /// Lower-case name, as written by the reference harness.
    pub fn as_str(self) -> &'static str {
        match self {
            ValueScale::Indexed => "indexed",
            ValueScale::Linear => "linear",
            ValueScale::Quadratic => "quadratic",
            ValueScale::Cubic => "cubic",
            ValueScale::Quartic => "quartic",
            ValueScale::SquareRoot => "square_root",
            ValueScale::Exponential => "exponential",
        }
    }
}

/// One parameter (`ValueDetails`).
#[derive(Clone, Debug, PartialEq)]
pub struct ParamInfo {
    pub name: String,
    /// Upstream version (`0xMMmmpp`) that added the parameter.
    pub version_added: i32,
    pub min: f32,
    pub max: f32,
    /// The value a preset that omits this parameter loads.
    pub default_value: f32,
    /// Offset applied after quadratic / exponential display scaling.
    pub post_offset: f32,
    pub display_multiply: f32,
    pub value_scale: ValueScale,
    pub display_invert: bool,
    pub display_units: &'static str,
    pub display_name: String,
    /// Display options for indexed parameters.
    pub string_table: Option<StringTable>,
    /// Name inside its group ("Cutoff" for "Filter 1 Cutoff"); empty for
    /// ungrouped parameters.
    pub local_description: String,
}

impl ParamInfo {
    /// `max - min` (`Parameters::getParameterRange`), the scale applied to
    /// modulation amounts.
    pub fn range(&self) -> f32 {
        self.max - self.min
    }

    /// Display options of an indexed parameter, if it has a string table.
    pub fn options(&self) -> Option<&'static [&'static str]> {
        self.string_table.map(StringTable::values)
    }
}

/// A row of an upstream parameter list. Numbers are kept as written in the
/// C++ source (double literals) and narrowed to `f32` when the registry is
/// built, like the `mono_float` fields of `ValueDetails`.
#[derive(Clone, Copy)]
pub(crate) struct Details {
    name: &'static str,
    version_added: i32,
    min: f64,
    max: f64,
    default_value: f64,
    post_offset: f64,
    display_multiply: f64,
    value_scale: ValueScale,
    display_invert: bool,
    display_units: &'static str,
    display_name: &'static str,
    string_table: Option<StringTable>,
}

#[allow(clippy::too_many_arguments)]
const fn details(
    name: &'static str,
    version_added: i32,
    min: f64,
    max: f64,
    default_value: f64,
    post_offset: f64,
    display_multiply: f64,
    value_scale: ValueScale,
    display_invert: bool,
    display_units: &'static str,
    display_name: &'static str,
    string_table: Option<StringTable>,
) -> Details {
    Details {
        name,
        version_added,
        min,
        max,
        default_value,
        post_offset,
        display_multiply,
        value_scale,
        display_invert,
        display_units,
        display_name,
        string_table,
    }
}

use ValueScale::*;

// Rows as in upstream (one line each); literals kept exactly as written there.
#[rustfmt::skip]
#[allow(clippy::approx_constant)]
pub(crate) const PARAMETER_LIST: &[Details] = &[
    details("bypass", 0x000702, 0.0, 1.0, 0.0, 0.0, 60.0, Indexed, false, "", "Bypass", None),
    details("beats_per_minute", 0x000000, 0.333333333, 5.0, 2.0, 0.0, 60.0, Linear, false, "", "Beats Per Minute", None),
    details("delay_dry_wet", 0x000000, 0.0, 1.0, 0.3334, 0.0, 100.0, Linear, false, "%", "Delay Mix", None),
    details("delay_feedback", 0x000000, -1.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Delay Feedback", None),
    details("delay_frequency", 0x000000, -2.0, 9.0, 2.0, 0.0, 1.0, Exponential, true, " secs", "Delay Frequency", None),
    details("delay_aux_frequency", 0x000507, -2.0, 9.0, 2.0, 0.0, 1.0, Exponential, true, " secs", "Delay Frequency 2", None),
    details("delay_on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Delay Switch", Some(StringTable::OffOn)),
    details("delay_style", 0x000000, 0.0, 3.0, 0.0, 0.0, 1.0, Indexed, false, "", "Delay Style", Some(StringTable::DelayStyle)),
    details("delay_filter_cutoff", 0x000000, 8.0, 136.0, 60.0, 0.0, 1.0, Linear, false, "", "Delay Filter Cutoff", None),
    details("delay_filter_spread", 0x000000, 0.0, 1.0, 1.0, 0.0, 1.0, Linear, false, "", "Delay Filter Spread", None),
    details("delay_sync", 0x000000, 0.0, 3.0, 1.0, 0.0, 1.0, Indexed, false, "", "Delay Sync", Some(StringTable::FrequencySync)),
    details("delay_tempo", 0x000000, 4.0, 12.0, 9.0, 0.0, 1.0, Indexed, false, "", "Delay Tempo", Some(StringTable::SyncedFrequency)),
    details("delay_aux_sync", 0x000507, 0.0, 3.0, 1.0, 0.0, 1.0, Indexed, false, "", "Delay Sync 2", Some(StringTable::FrequencySync)),
    details("delay_aux_tempo", 0x000507, 4.0, 12.0, 9.0, 0.0, 1.0, Indexed, false, "", "Delay Tempo 2", Some(StringTable::SyncedFrequency)),
    details("distortion_on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Distortion Switch", Some(StringTable::OffOn)),
    details("distortion_type", 0x000000, 0.0, 5.0, 0.0, 0.0, 1.0, Indexed, false, "", "Distortion Type", Some(StringTable::DistortionType)),
    details("distortion_drive", 0x000000, DISTORTION_MIN_DRIVE, DISTORTION_MAX_DRIVE, 0.0, 0.0, 1.0, Linear, false, " dB", "Distortion Drive", None),
    details("distortion_mix", 0x000000, 0.0, 1.0, 1.0, 0.0, 1.0, Linear, false, "", "Distortion Mix", None),
    details("distortion_filter_order", 0x000000, 0.0, 2.0, 0.0, 0.0, 1.0, Indexed, false, "", "Distortion Filter Order", Some(StringTable::DistortionFilterOrder)),
    details("distortion_filter_cutoff", 0x000000, 8.0, 136.0, 80.0, 0.0, 1.0, Linear, false, " semitones", "Distortion Filter Cutoff", None),
    details("distortion_filter_resonance", 0x000000, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Distortion Filter Resonance", None),
    details("distortion_filter_blend", 0x000000, 0.0, 2.0, 0.0, 0.0, 1.0, Linear, false, "", "Distortion Filter Blend", None),
    details("legato", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Legato", Some(StringTable::OffOn)),
    details("macro_control_1", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Linear, false, "", "Macro 1", None),
    details("macro_control_2", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Linear, false, "", "Macro 2", None),
    details("macro_control_3", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Linear, false, "", "Macro 3", None),
    details("macro_control_4", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Linear, false, "", "Macro 4", None),
    details("pitch_bend_range", 0x000000, 0.0, 48.0, 2.0, 0.0, 1.0, Indexed, false, " semitones", "Pitch Bend Range", None),
    details("polyphony", 0x000000, 1.0, (MAX_POLYPHONY - 1) as f64, 8.0, 0.0, 1.0, Indexed, false, " voices", "Polyphony", None),
    details("voice_tune", 0x000000, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, " cents", "Voice Tune", None),
    details("voice_transpose", 0x000604, -48.0, 48.0, 0.0, 0.0, 1.0, Indexed, false, "", "Voice Transpose", None),
    details("voice_amplitude", 0x000000, 0.0, 1.0, 1.0, 0.0, 1.0, Linear, false, "", "Voice Amplitude", None),
    details("stereo_routing", 0x000000, 0.0, 1.0, 1.0, 0.0, 100.0, Linear, false, "%", "Stereo Routing", None),
    details("stereo_mode", 0x000605, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Stereo Mode", Some(StringTable::StereoMode)),
    details("portamento_time", 0x000000, -10.0, 4.0, -10.0, 0.0, 1.0, Exponential, false, " secs", "Portamento Time", None),
    details("portamento_slope", 0x000000, -8.0, 8.0, 0.0, 0.0, 1.0, Linear, false, "", "Portamento Slope", None),
    details("portamento_force", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Portamento Force", Some(StringTable::OffOn)),
    details("portamento_scale", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Portamento Scale", Some(StringTable::OffOn)),
    details("reverb_pre_low_cutoff", 0x000000, 0.0, 128.0, 0.0, 0.0, 1.0, Linear, false, " semitones", "Reverb Pre Low Cutoff", None),
    details("reverb_pre_high_cutoff", 0x000000, 0.0, 128.0, 110.0, 0.0, 1.0, Linear, false, " semitones", "Reverb Pre High Cutoff", None),
    details("reverb_low_shelf_cutoff", 0x000000, 0.0, 128.0, 0.0, 0.0, 1.0, Linear, false, " semitones", "Reverb Low Cutoff", None),
    details("reverb_low_shelf_gain", 0x000000, -6.0, 0.0, 0.0, 0.0, 1.0, Linear, false, " dB", "Reverb Low Gain", None),
    details("reverb_high_shelf_cutoff", 0x000000, 0.0, 128.0, 90.0, 0.0, 1.0, Linear, false, " semitones", "Reverb High Cutoff", None),
    details("reverb_high_shelf_gain", 0x000000, -6.0, 0.0, -1.0, 0.0, 1.0, Linear, false, " dB", "Reverb High Gain", None),
    details("reverb_dry_wet", 0x000000, 0.0, 1.0, 0.25, 0.0, 100.0, Linear, false, "%", "Reverb Mix", None),
    details("reverb_delay", 0x000609, 0.0, 0.3, 0.0, 0.0, 1.0, Linear, false, " secs", "Reverb Delay", None),
    details("reverb_decay_time", 0x000000, -6.0, 6.0, 0.0, 0.0, 1.0, Exponential, false, " secs", "Reverb Decay Time", None),
    details("reverb_size", 0x000506, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Reverb Size", None),
    details("reverb_chorus_amount", 0x000000, 0.0, 1.0, 0.223607, 0.0, 100.0, Quadratic, false, "%", "Reverb Chorus Amount", None),
    details("reverb_chorus_frequency", 0x000000, -8.0, 3.0, -2.0, 0.0, 1.0, Exponential, false, " Hz", "Reverb Chorus Frequency", None),
    details("reverb_on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Reverb Switch", Some(StringTable::OffOn)),
    details("sub_on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sub Switch", Some(StringTable::OffOn)),
    details("sub_direct_out", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sub Direct Out", None),
    details("sub_transpose", 0x000000, -48.0, 48.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sub Transpose", None),
    details("sub_transpose_quantize", 0x000000, 0.0, 8191.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sub Transpose Quantize", None),
    details("sub_tune", 0x000000, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "", "Sub Tune", None),
    details("sub_level", 0x000000, 0.0, 1.0, 0.70710678119, 0.0, 1.0, Quadratic, false, "", "Sub Level", None),
    details("sub_pan", 0x000000, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "%", "Sub Pan", None),
    details("sub_waveform", 0x000000, 0.0, (NUM_PREDEFINED_SHAPES - 1) as f64, 2.0, 0.0, 1.0, Indexed, false, "", "Sub Osc Waveform", None),
    details("sample_on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sample Switch", Some(StringTable::OffOn)),
    details("sample_random_phase", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sample Random Phase", Some(StringTable::OffOn)),
    details("sample_keytrack", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sample Keytrack", Some(StringTable::OffOn)),
    details("sample_loop", 0x000000, 0.0, 1.0, 1.0, 0.0, 1.0, Indexed, false, "", "Sample Loop", Some(StringTable::OffOn)),
    details("sample_bounce", 0x000603, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sample Bounce", Some(StringTable::OffOn)),
    details("sample_transpose", 0x000000, -48.0, 48.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sample Transpose", None),
    details("sample_transpose_quantize", 0x000000, 0.0, 8191.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sample Transpose Quantize", None),
    details("sample_tune", 0x000000, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "", "Sample Tune", None),
    details("sample_level", 0x000000, 0.0, 1.0, 0.70710678119, 0.0, 1.0, Quadratic, false, "", "Sample Level", None),
    details("sample_destination", 0x000500, 0.0, (NUM_SOURCE_DESTINATIONS as usize + NUM_EFFECTS) as f64, 3.0, 0.0, 1.0, Indexed, false, "", "Sample Destination", Some(StringTable::Destination)),
    details("sample_pan", 0x000000, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "%", "Sample Pan", None),
    details("velocity_track", 0x000000, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "%", "Velocity Track", None),
    details("volume", 0x000000, 0.0, 7399.4404, 5473.0404, -80.0, 1.0, SquareRoot, false, "dB", "Volume", None),
    details("phaser_on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Phaser Switch", Some(StringTable::OffOn)),
    details("phaser_dry_wet", 0x000000, 0.0, 1.0, 1.0, 0.0, 100.0, Linear, false, "%", "Phaser Mix", None),
    details("phaser_feedback", 0x000000, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Phaser Feedback", None),
    details("phaser_frequency", 0x000000, -5.0, 2.0, -3.0, 0.0, 1.0, Exponential, true, " secs", "Phaser Frequency", None),
    details("phaser_sync", 0x000000, 0.0, 3.0, 1.0, 0.0, 1.0, Indexed, false, "", "Phaser Sync", Some(StringTable::FrequencySync)),
    details("phaser_tempo", 0x000000, 0.0, 10.0, 3.0, 0.0, 1.0, Indexed, false, "", "Phaser Tempo", Some(StringTable::SyncedFrequency)),
    details("phaser_center", 0x000000, 8.0, 136.0, 80.0, 0.0, 1.0, Linear, false, " semitones", "Phaser Center", None),
    details("phaser_blend", 0x000509, 0.0, 2.0, 1.0, 0.0, 1.0, Linear, false, "", "Phaser Blend", None),
    details("phaser_mod_depth", 0x000000, 0.0, 48.0, 24.0, 0.0, 1.0, Linear, false, " semitones", "Phaser Mod Depth", None),
    details("phaser_phase_offset", 0x000000, 0.0, 1.0, 0.33333333, 0.0, DEGREES_PER_CYCLE as f64, Linear, false, "", "Phaser Phase Offset", None),
    details("flanger_on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Flanger Switch", Some(StringTable::OffOn)),
    details("flanger_dry_wet", 0x000000, 0.0, 0.5, 0.5, 0.0, 200.0, Linear, false, "%", "Flanger Mix", None),
    details("flanger_feedback", 0x000000, -1.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Flanger Feedback", None),
    details("flanger_frequency", 0x000000, -5.0, 2.0, 2.0, 0.0, 1.0, Exponential, true, " secs", "Flanger Frequency", None),
    details("flanger_sync", 0x000000, 0.0, 3.0, 1.0, 0.0, 1.0, Indexed, false, "", "Flanger Sync", Some(StringTable::FrequencySync)),
    details("flanger_tempo", 0x000000, 0.0, 10.0, 4.0, 0.0, 1.0, Indexed, false, "", "Flanger Tempo", Some(StringTable::SyncedFrequency)),
    details("flanger_center", 0x000505, 8.0, 136.0, 64.0, 0.0, 1.0, Linear, false, " semitones", "Flanger Center", None),
    details("flanger_mod_depth", 0x000000, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Flanger Mod Depth", None),
    details("flanger_phase_offset", 0x000000, 0.0, 1.0, 0.33333333, 0.0, DEGREES_PER_CYCLE as f64, Linear, false, "", "Flanger Phase Offset", None),
    details("chorus_on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Chorus Switch", Some(StringTable::OffOn)),
    details("chorus_dry_wet", 0x000000, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Chorus Mix", None),
    details("chorus_feedback", 0x000000, -0.95, 0.95, 0.4, 0.0, 100.0, Linear, false, "%", "Chorus Feedback", None),
    details("chorus_cutoff", 0x000000, 8.0, 136.0, 60.0, 0.0, 1.0, Linear, false, "", "Chorus Filter Cutoff", None),
    details("chorus_spread", 0x000607, 0.0, 1.0, 1.0, 0.0, 1.0, Linear, false, "", "Chorus Filter Spread", None),
    details("chorus_voices", 0x000508, 1.0, 4.0, 4.0, 0.0, 4.0, Indexed, false, "", "Chorus Voices", None),
    details("chorus_frequency", 0x000000, -6.0, 3.0, -3.0, 0.0, 1.0, Exponential, true, " secs", "Chorus Frequency", None),
    details("chorus_sync", 0x000000, 0.0, 3.0, 1.0, 0.0, 1.0, Indexed, false, "", "Chorus Sync", Some(StringTable::FrequencySync)),
    details("chorus_tempo", 0x000000, 0.0, 10.0, 4.0, 0.0, 1.0, Indexed, false, "", "Chorus Tempo", Some(StringTable::SyncedFrequency)),
    details("chorus_mod_depth", 0x000000, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Chorus Mod Depth", None),
    details("chorus_delay_1", 0x000000, -10.0, -5.64386, -9.0, 0.0, 1000.0, Exponential, false, "ms", "Chorus Delay 1", None),
    details("chorus_delay_2", 0x000000, -10.0, -5.64386, -7.0, 0.0, 1000.0, Exponential, false, " ms", "Chorus Delay 2", None),
    details("compressor_on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Compressor Switch", Some(StringTable::OffOn)),
    details("compressor_low_upper_threshold", 0x000000, -80.0, 0.0, -28.0, 0.0, 1.0, Linear, false, " dB", "Low Upper Threshold", None),
    details("compressor_band_upper_threshold", 0x000000, -80.0, 0.0, -25.0, 0.0, 1.0, Linear, false, " dB", "Band Upper Threshold", None),
    details("compressor_high_upper_threshold", 0x000000, -80.0, 0.0, -30.0, 0.0, 1.0, Linear, false, " dB", "High Upper Threshold", None),
    details("compressor_low_lower_threshold", 0x000000, -80.0, 0.0, -35.0, 0.0, 1.0, Linear, false, " dB", "Low Lower Threshold", None),
    details("compressor_band_lower_threshold", 0x000000, -80.0, 0.0, -36.0, 0.0, 1.0, Linear, false, " dB", "Band Lower Threshold", None),
    details("compressor_high_lower_threshold", 0x000000, -80.0, 0.0, -35.0, 0.0, 1.0, Linear, false, " dB", "High Lower Threshold", None),
    details("compressor_low_upper_ratio", 0x000000, 0.0, 1.0, 0.9, 0.0, 1.0, Linear, false, "", "Low Upper Ratio", None),
    details("compressor_band_upper_ratio", 0x000000, 0.0, 1.0, 0.857, 0.0, 1.0, Linear, false, "", "Band Upper Ratio", None),
    details("compressor_high_upper_ratio", 0x000000, 0.0, 1.0, 1.0, 0.0, 1.0, Linear, false, "", "High Upper Ratio", None),
    details("compressor_low_lower_ratio", 0x000000, -1.0, 1.0, 0.8, 0.0, 1.0, Linear, false, "", "Low Lower Ratio", None),
    details("compressor_band_lower_ratio", 0x000000, -1.0, 1.0, 0.8, 0.0, 1.0, Linear, false, "", "Band Lower Ratio", None),
    details("compressor_high_lower_ratio", 0x000000, -1.0, 1.0, 0.8, 0.0, 1.0, Linear, false, "", "High Lower Ratio", None),
    details("compressor_low_gain", 0x000000, -30.0, 30.0, 16.3, 0.0, 1.0, Linear, false, " dB", "Compressor Low Gain", None),
    details("compressor_band_gain", 0x000000, -30.0, 30.0, 11.7, 0.0, 1.0, Linear, false, " dB", "Compressor Band Gain", None),
    details("compressor_high_gain", 0x000000, -30.0, 30.0, 16.3, 0.0, 1.0, Linear, false, " dB", "Compressor High Gain", None),
    details("compressor_attack", 0x000000, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Compressor Attack", None),
    details("compressor_release", 0x000000, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Compressor Release", None),
    details("compressor_enabled_bands", 0x000000, 0.0, (NUM_COMPRESSOR_BAND_OPTIONS - 1) as f64, 0.0, 0.0, 1.0, Indexed, false, "", "Compressor Enabled Bands", Some(StringTable::CompressorBand)),
    details("compressor_mix", 0x000602, 0.0, 1.0, 1.0, 0.0, 1.0, Linear, false, "", "Compressor Mix", None),
    details("compressor_low_band_unused", 0x000000, 0.0, 1.0, 1.0, 0.0, 1.0, Indexed, false, "", "Compressor Unused", None),
    details("eq_on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "EQ Switch", Some(StringTable::OffOn)),
    details("eq_low_mode", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "EQ Low Mode", Some(StringTable::EqLowMode)),
    details("eq_low_cutoff", 0x000000, 8.0, 136.0, 40.0, 0.0, 1.0, Linear, false, " semitones", "EQ Low Cutoff", None),
    details("eq_low_gain", 0x000000, DIGITAL_SVF_MIN_GAIN, DIGITAL_SVF_MAX_GAIN, 0.0, 0.0, 1.0, Linear, false, " dB", "EQ Low Gain", None),
    details("eq_low_resonance", 0x000000, 0.0, 1.0, 0.3163, 0.0, 100.0, Quadratic, false, "%", "EQ Low Resonance", None),
    details("eq_band_mode", 0x000506, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "EQ Band Mode", Some(StringTable::EqBandMode)),
    details("eq_band_cutoff", 0x000000, 8.0, 136.0, 80.0, 0.0, 1.0, Linear, false, " semitones", "EQ Band Cutoff", None),
    details("eq_band_gain", 0x000000, DIGITAL_SVF_MIN_GAIN, DIGITAL_SVF_MAX_GAIN, 0.0, 0.0, 1.0, Linear, false, " dB", "EQ Band Gain", None),
    details("eq_band_resonance", 0x000000, 0.0, 1.0, 0.4473, 0.0, 100.0, Quadratic, false, "", "EQ Band Resonance", None),
    details("eq_high_mode", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "EQ High Mode", Some(StringTable::EqHighMode)),
    details("eq_high_cutoff", 0x000000, 8.0, 136.0, 100.0, 0.0, 1.0, Linear, false, " semitones", "EQ High Cutoff", None),
    details("eq_high_gain", 0x000000, DIGITAL_SVF_MIN_GAIN, DIGITAL_SVF_MAX_GAIN, 0.0, 0.0, 1.0, Linear, false, " dB", "EQ High Gain", None),
    details("eq_high_resonance", 0x000000, 0.0, 1.0, 0.3163, 0.0, 100.0, Quadratic, false, "", "EQ High Resonance", None),
    details("effect_chain_order", 0x000000, 0.0, (factorial(NUM_EFFECTS) - 1) as f64, 0.0, 0.0, 1.0, Indexed, false, "", "Effect Chain Order", None),
    details("voice_priority", 0x000000, 0.0, (NUM_VOICE_PRIORITIES - 1) as f64, VOICE_PRIORITY_ROUND_ROBIN as f64, 0.0, 1.0, Indexed, false, "", "Voice Priority", Some(StringTable::VoicePriority)),
    details("voice_override", 0x000700, 0.0, (NUM_VOICE_OVERRIDES - 1) as f64, VOICE_OVERRIDE_KILL as f64, 0.0, 1.0, Indexed, false, "", "Voice Override", Some(StringTable::VoiceOverride)),
    details("oversampling", 0x000000, 0.0, 3.0, 1.0, 0.0, 1.0, Indexed, false, "", "Oversampling", Some(StringTable::Oversampling)),
    details("pitch_wheel", 0x000400, -1.0, 1.0, 0.0, 0.0, 1.0, Linear, false, "", "Pitch Wheel", None),
    details("mod_wheel", 0x000400, 0.0, 1.0, 0.0, 0.0, 1.0, Linear, false, "", "Mod Wheel", None),
    details("mpe_enabled", 0x000501, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "MPE Enabled", Some(StringTable::OffOn)),
    details("view_spectrogram", 0x000803, 0.0, 2.0, 0.0, 0.0, 1.0, Indexed, false, "", "View Spectrogram", Some(StringTable::OffOn)),
];

// Rows as in upstream (one line each); literals kept exactly as written there.
#[rustfmt::skip]
#[allow(clippy::approx_constant)]
pub(crate) const ENV_PARAMETER_LIST: &[Details] = &[
    details("delay", 0x000503, 0.0, 1.4142135624, 0.0, 0.0, 1.0, Quartic, false, " secs", "Delay", None),
    details("attack", 0x000000, 0.0, 2.37842, 0.1495, 0.0, 1.0, Quartic, false, " secs", "Attack", None),
    details("hold", 0x000504, 0.0, 1.4142135624, 0.0, 0.0, 1.0, Quartic, false, " secs", "Hold", None),
    details("decay", 0x000000, 0.0, 2.37842, 1.0, 0.0, 1.0, Quartic, false, " secs", "Decay", None),
    details("release", 0x000000, 0.0, 2.37842, 0.5476, 0.0, 1.0, Quartic, false, " secs", "Release", None),
    details("attack_power", 0x000000, -20.0, 20.0, 0.0, 0.0, 1.0, Linear, false, "", "Attack Power", None),
    details("decay_power", 0x000000, -20.0, 20.0, -2.0, 0.0, 1.0, Linear, false, "", "Decay Power", None),
    details("release_power", 0x000000, -20.0, 20.0, -2.0, 0.0, 1.0, Linear, false, "", "Release Power", None),
    details("sustain", 0x000000, 0.0, 1.0, 1.0, 0.0, 1.0, Linear, false, "", "Sustain", None),
];

// Rows as in upstream (one line each); literals kept exactly as written there.
#[rustfmt::skip]
#[allow(clippy::approx_constant)]
pub(crate) const LFO_PARAMETER_LIST: &[Details] = &[
    details("phase", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Linear, false, "", "Phase", None),
    details("sync_type", 0x000000, 0.0, (NUM_LFO_SYNC_TYPES - 1) as f64, 0.0, 0.0, 1.0, Indexed, false, "", "Sync Type", Some(StringTable::Sync)),
    details("frequency", 0x000000, -7.0, 9.0, 1.0, 0.0, 1.0, Exponential, true, " secs", "Frequency", None),
    details("sync", 0x000000, 0.0, (NUM_LFO_SYNC_OPTIONS - 1) as f64, 1.0, 0.0, 1.0, Indexed, false, "", "Sync", Some(StringTable::FrequencySync)),
    details("tempo", 0x000000, 0.0, 12.0, 7.0, 0.0, 1.0, Indexed, false, "", "Tempo", Some(StringTable::SyncedFrequency)),
    details("fade_time", 0x000000, 0.0, 8.0, 0.0, 0.0, 1.0, Linear, false, " secs", "Fade In", None),
    details("smooth_mode", 0x000801, 0.0, 1.0, 1.0, 0.0, 1.0, Indexed, false, "", "Smooth Mode", Some(StringTable::OffOn)),
    details("smooth_time", 0x000801, -10.0, 4.0, -7.5, 0.0, 1.0, Exponential, false, " secs", "Smooth Time", None),
    details("delay_time", 0x000000, 0.0, 4.0, 0.0, 0.0, 1.0, Linear, false, " secs", "Delay", None),
    details("stereo", 0x000406, -0.5, 0.5, 0.0, 0.0, 1.0, Linear, false, "", "Stereo", None),
    details("keytrack_transpose", 0x000704, -60.0, 36.0, -12.0, 0.0, 1.0, Indexed, false, "", "Transpose", None),
    details("keytrack_tune", 0x000704, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "", "Tune", None),
];

// Rows as in upstream (one line each); literals kept exactly as written there.
#[rustfmt::skip]
#[allow(clippy::approx_constant)]
pub(crate) const RANDOM_LFO_PARAMETER_LIST: &[Details] = &[
    details("style", 0x000401, 0.0, (NUM_RANDOM_LFO_STYLES - 1) as f64, 0.0, 0.0, 1.0, Indexed, false, "", "Style", Some(StringTable::Random)),
    details("frequency", 0x000401, -7.0, 9.0, 1.0, 0.0, 1.0, Exponential, true, " secs", "Frequency", None),
    details("sync", 0x000401, 0.0, (NUM_LFO_SYNC_OPTIONS - 1) as f64, 1.0, 0.0, 1.0, Indexed, false, "", "Sync", Some(StringTable::FrequencySync)),
    details("tempo", 0x000401, 0.0, 12.0, 8.0, 0.0, 1.0, Indexed, false, "", "Tempo", Some(StringTable::SyncedFrequency)),
    details("stereo", 0x000401, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Stereo", Some(StringTable::OffOn)),
    details("sync_type", 0x000600, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Sync Type", Some(StringTable::OffOn)),
    details("keytrack_transpose", 0x000704, -60.0, 36.0, -12.0, 0.0, 1.0, Indexed, false, "", "Transpose", None),
    details("keytrack_tune", 0x000704, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "", "Tune", None),
];

// Rows as in upstream (one line each); literals kept exactly as written there.
#[rustfmt::skip]
#[allow(clippy::approx_constant)]
pub(crate) const FILTER_PARAMETER_LIST: &[Details] = &[
    details("mix", 0x000000, 0.0, 1.0, 1.0, 0.0, 100.0, Linear, false, "%", "Mix", None),
    details("cutoff", 0x000000, 8.0, 136.0, 60.0, -60.0, 1.0, Linear, false, " semitones", "Cutoff", None),
    details("resonance", 0x000000, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Resonance", None),
    details("drive", 0x000000, 0.0, 20.0, 0.0, 0.0, 1.0, Linear, false, "dB", "Drive", None),
    details("blend", 0x000000, 0.0, 2.0, 0.0, 0.0, 1.0, Linear, false, "", "Blend", None),
    details("style", 0x000000, 0.0, 9.0, 0.0, 0.0, 1.0, Indexed, false, "", "Style", Some(StringTable::FilterStyle)),
    details("model", 0x000000, 0.0, NUM_FILTER_MODELS as f64 - 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Model", Some(StringTable::FilterModel)),
    details("on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Switch", Some(StringTable::OffOn)),
    details("blend_transpose", 0x000000, 0.0, 84.0, 42.0, 0.0, 1.0, Linear, false, " semitones", "Comb Blend Offset", None),
    details("keytrack", 0x000000, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "%", "Key Track", None),
    details("formant_x", 0x000000, 0.0, 1.0, 0.5, 0.0, 1.0, Linear, false, "", "Formant X", None),
    details("formant_y", 0x000000, 0.0, 1.0, 0.5, 0.0, 1.0, Linear, false, "", "Formant Y", None),
    details("formant_transpose", 0x000000, -12.0, 12.0, 0.0, 0.0, 1.0, Linear, false, "", "Formant Transpose", None),
    details("formant_resonance", 0x000000, 0.3, 1.0, 0.85, 0.0, 100.0, Linear, false, "%", "Formant Resonance", None),
    details("formant_spread", 0x000707, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "%", "Formant Spread", None),
    details("osc1_input", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "OSC 1 Input", Some(StringTable::OffOn)),
    details("osc2_input", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "OSC 2 Input", Some(StringTable::OffOn)),
    details("osc3_input", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "OSC 3 Input", Some(StringTable::OffOn)),
    details("sample_input", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "SAMPLE Input", Some(StringTable::OffOn)),
    details("filter_input", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "FILTER Input", Some(StringTable::OffOn)),
];

// Rows as in upstream (one line each); literals kept exactly as written there.
#[rustfmt::skip]
#[allow(clippy::approx_constant)]
pub(crate) const OSC_PARAMETER_LIST: &[Details] = &[
    details("on", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Switch", Some(StringTable::OffOn)),
    details("transpose", 0x000000, -48.0, 48.0, 0.0, 0.0, 1.0, Indexed, false, "", "Transpose", None),
    details("transpose_quantize", 0x000000, 0.0, 8191.0, 0.0, 0.0, 1.0, Indexed, false, "", "Transpose Quantize", None),
    details("tune", 0x000000, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "", "Tune", None),
    details("pan", 0x000000, -1.0, 1.0, 0.0, 0.0, 100.0, Linear, false, "%", "Pan", None),
    details("stack_style", 0x000000, 0.0, (NUM_UNISON_STACK_TYPES - 1) as f64, 0.0, 0.0, 1.0, Indexed, false, "", "Stack Style", Some(StringTable::UnisonStack)),
    details("unison_detune", 0x000000, 0.0, 10.0, 4.472135955, 0.0, 1.0, Quadratic, false, "%", "Unison Detune", None),
    details("unison_voices", 0x000000, 1.0, 16.0, 1.0, 0.0, 1.0, Indexed, false, "v", "Unison Voices", None),
    details("unison_blend", 0x000000, 0.0, 1.0, 0.8, 0.0, 100.0, Linear, false, "%", "Blend", None),
    details("detune_power", 0x000000, -5.0, 5.0, 1.5, 0.0, 1.0, Linear, false, "", "Detune Power", None),
    details("detune_range", 0x000000, 0.0, 48.0, 2.0, 0.0, 1.0, Linear, false, "", "Detune Range", None),
    details("level", 0x000000, 0.0, 1.0, 0.70710678119, 0.0, 1.0, Quadratic, false, "", "Level", None),
    details("midi_track", 0x000000, 0.0, 1.0, 1.0, 0.0, 1.0, Indexed, false, "", "Midi Track", Some(StringTable::OffOn)),
    details("smooth_interpolation", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Smooth Interpolation", Some(StringTable::OffOn)),
    details("spectral_unison", 0x000500, 0.0, 1.0, 1.0, 0.0, 1.0, Indexed, false, "", "Spectral Unison", Some(StringTable::OffOn)),
    details("wave_frame", 0x000000, 0.0, (NUM_OSCILLATOR_WAVE_FRAMES - 1) as f64, 0.0, 0.0, 1.0, Linear, false, "", "Wave Frame", None),
    details("frame_spread", 0x000000, (-NUM_OSCILLATOR_WAVE_FRAMES / 2) as f64, (NUM_OSCILLATOR_WAVE_FRAMES / 2) as f64, 0.0, 0.0, 1.0, Linear, false, "", "Unison Frame Spread", None),
    details("stereo_spread", 0x000000, 0.0, 1.0, 1.0, 0.0, 100.0, Linear, false, "%", "Stereo Spread", None),
    details("phase", 0x000000, 0.0, 1.0, 0.5, 0.0, 360.0, Linear, false, "", "Phase", None),
    details("distortion_phase", 0x000000, 0.0, 1.0, 0.5, 0.0, 360.0, Linear, false, "", "Distortion Phase", None),
    details("random_phase", 0x000000, 0.0, 1.0, 1.0, 0.0, 100.0, Linear, false, "%", "Phase Randomization", None),
    details("distortion_type", 0x000000, 0.0, (NUM_DISTORTION_TYPES - 1) as f64, 0.0, 0.0, 1.0, Indexed, false, "", "Distortion Type", Some(StringTable::PhaseDistortion)),
    details("distortion_amount", 0x000000, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Distortion Amount", None),
    details("distortion_spread", 0x000000, -0.5, 0.5, 0.0, 0.0, 200.0, Linear, false, "%", "Distortion Spread", None),
    details("spectral_morph_type", 0x000407, 0.0, (NUM_SPECTRAL_MORPH_TYPES - 1) as f64, 0.0, 0.0, 1.0, Indexed, false, "", "Frequency Morph Type", Some(StringTable::SpectralMorph)),
    details("spectral_morph_amount", 0x000407, 0.0, 1.0, 0.5, 0.0, 100.0, Linear, false, "%", "Frequency Morph Amount", None),
    details("spectral_morph_spread", 0x000407, -0.5, 0.5, 0.0, 0.0, 200.0, Linear, false, "%", "Frequency Morph Spread", None),
    details("destination", 0x000500, 0.0, (NUM_SOURCE_DESTINATIONS as usize + NUM_EFFECTS) as f64, 0.0, 0.0, 1.0, Indexed, false, "", "Destination", Some(StringTable::Destination)),
    details("view_2d", 0x000402, 0.0, 2.0, 1.0, 0.0, 1.0, Indexed, false, "", "View 2D", Some(StringTable::OffOn)),
];

// Rows as in upstream (one line each); literals kept exactly as written there.
#[rustfmt::skip]
#[allow(clippy::approx_constant)]
pub(crate) const MOD_PARAMETER_LIST: &[Details] = &[
    details("amount", 0x000000, -1.0, 1.0, 0.0, 0.0, 1.0, Linear, false, "", "Amount", None),
    details("power", 0x000000, -10.0, 10.0, 0.0, 0.0, 1.0, Linear, false, "", "Power", None),
    details("bipolar", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Bipolar", Some(StringTable::OffOn)),
    details("stereo", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Stereo", Some(StringTable::OffOn)),
    details("bypass", 0x000000, 0.0, 1.0, 0.0, 0.0, 1.0, Indexed, false, "", "Bypass", Some(StringTable::OffOn)),
];

const ID_DELIMITER: &str = "_";
const NAME_DELIMITER: &str = " ";

/// The registry: all parameters in upstream index order plus a name index.
pub struct Registry {
    list: Vec<ParamInfo>,
    index: HashMap<String, usize>,
}

impl Registry {
    /// Builds the registry the way `ValueDetailsLookup::ValueDetailsLookup`
    /// does.
    fn build() -> Registry {
        const NUM_OSCILLATORS_OLD: usize = 2;
        const NEW_OSCILLATOR_VERSION: i32 = 0x000500;
        const OLD_MAX_MODULATIONS: usize = 32;
        const NEW_MODULATION_VERSION: i32 = 0x000601;

        let mut list: Vec<ParamInfo> = PARAMETER_LIST
            .iter()
            .map(|row| info(row, String::new()))
            .collect();

        for env in 0..NUM_ENVELOPES {
            add_group(
                &mut list,
                ENV_PARAMETER_LIST,
                &(env + 1).to_string(),
                "env",
                "Envelope",
                -1,
            );
        }
        for lfo in 0..NUM_LFOS {
            add_group(
                &mut list,
                LFO_PARAMETER_LIST,
                &(lfo + 1).to_string(),
                "lfo",
                "LFO",
                -1,
            );
        }
        for lfo in 0..NUM_RANDOM_LFOS {
            add_group(
                &mut list,
                RANDOM_LFO_PARAMETER_LIST,
                &(lfo + 1).to_string(),
                "random",
                "Random LFO",
                -1,
            );
        }
        for osc in 0..NUM_OSCILLATORS {
            let version = if osc < NUM_OSCILLATORS_OLD {
                -1
            } else {
                NEW_OSCILLATOR_VERSION
            };
            add_group(
                &mut list,
                OSC_PARAMETER_LIST,
                &(osc + 1).to_string(),
                "osc",
                "Oscillator",
                version,
            );
        }
        for filter in 0..NUM_FILTERS {
            add_group(
                &mut list,
                FILTER_PARAMETER_LIST,
                &(filter + 1).to_string(),
                "filter",
                "Filter",
                -1,
            );
        }
        add_group(
            &mut list,
            FILTER_PARAMETER_LIST,
            "fx",
            "filter",
            "Filter",
            -1,
        );
        for modulation in 0..MAX_MODULATION_CONNECTIONS {
            let version = if modulation < OLD_MAX_MODULATIONS {
                -1
            } else {
                NEW_MODULATION_VERSION
            };
            add_group(
                &mut list,
                MOD_PARAMETER_LIST,
                &(modulation + 1).to_string(),
                "modulation",
                "Modulation",
                version,
            );
        }

        for (name, default_value) in [
            ("osc_1_on", 1.0),
            ("osc_2_destination", 1.0),
            ("osc_3_destination", 3.0),
            ("filter_1_osc1_input", 1.0),
            ("filter_2_osc2_input", 1.0),
        ] {
            let entry = list
                .iter_mut()
                .find(|entry| entry.name == name)
                .expect("default override target");
            entry.default_value = default_value;
        }

        // compareValueDetails; names are unique, so a stable sort gives the
        // same order as upstream's std::sort.
        list.sort_by(|a, b| {
            a.version_added
                .cmp(&b.version_added)
                .then_with(|| a.name.as_bytes().cmp(b.name.as_bytes()))
        });

        let index = list
            .iter()
            .enumerate()
            .map(|(i, entry)| (entry.name.clone(), i))
            .collect();
        Registry { list, index }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Parameters in `Parameters::getDetails(index)` order.
    pub fn parameters(&self) -> &[ParamInfo] {
        &self.list
    }

    /// `Parameters::getDetails(index)`.
    pub fn get_index(&self, index: usize) -> Option<&ParamInfo> {
        self.list.get(index)
    }

    /// `Parameters::getDetails(name)`.
    pub fn get(&self, name: &str) -> Option<&ParamInfo> {
        self.index.get(name).map(|&i| &self.list[i])
    }

    /// Position of `name` in upstream index order.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    /// `Parameters::isParameter`.
    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }
}

fn info(row: &Details, local_description: String) -> ParamInfo {
    ParamInfo {
        name: row.name.to_string(),
        version_added: row.version_added,
        min: row.min as f32,
        max: row.max as f32,
        default_value: row.default_value as f32,
        post_offset: row.post_offset as f32,
        display_multiply: row.display_multiply as f32,
        value_scale: row.value_scale,
        display_invert: row.display_invert,
        display_units: row.display_units,
        display_name: row.display_name.to_string(),
        string_table: row.string_table,
        local_description,
    }
}

/// `ValueDetailsLookup::addParameterGroup`.
fn add_group(
    list: &mut Vec<ParamInfo>,
    rows: &[Details],
    id: &str,
    id_prefix: &str,
    name_prefix: &str,
    version: i32,
) {
    let id_start = format!("{id_prefix}{ID_DELIMITER}{id}{ID_DELIMITER}");
    let name_start = format!("{name_prefix}{NAME_DELIMITER}{id}{NAME_DELIMITER}");
    for row in rows {
        let mut entry = info(row, row.display_name.to_string());
        if version > entry.version_added {
            entry.version_added = version;
        }
        entry.name = format!("{id_start}{}", row.name);
        entry.display_name = format!("{name_start}{}", row.display_name);
        list.push(entry);
    }
}

/// The process-wide registry (`Parameters::lookup_`).
pub fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::build)
}

/// Shorthand for `registry().get(name)`.
pub fn lookup(name: &str) -> Option<&'static ParamInfo> {
    registry().get(name)
}
