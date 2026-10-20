// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Facts about the engine the preset loader consults.
//!
//! Upstream asks the live `SoundEngine` which parameters exist as controls
//! (`getControls`), which names are modulation sources (`getModulationSource`)
//! and which controls accept modulation (`getMonoModulationDestination`).
//! Those sets follow from how the modules under `vital/src/synthesis/modules`
//! create their controls (`createBaseControl` vs `createMonoModControl` /
//! `createPolyModControl`). They are recorded here as data so this crate stays
//! independent of the DSP crates; `tests/reference.rs` checks them against the
//! reference's `ref-preset registry` dump.

use crate::constants::*;
use crate::registry::registry;

/// Registry parameters no engine module instantiates. They only exist so old
/// presets can be upgraded (`LoadSave::updateFromOldVersion`); they are never
/// loaded or saved.
pub const NON_CONTROL_PARAMETERS: &[&str] = &[
    "sub_on",
    "sub_direct_out",
    "sub_transpose",
    "sub_transpose_quantize",
    "sub_tune",
    "sub_level",
    "sub_pan",
    "sub_waveform",
    "compressor_low_band_unused",
    "filter_1_osc1_input",
    "filter_1_osc2_input",
    "filter_1_osc3_input",
    "filter_1_sample_input",
    "filter_2_osc1_input",
    "filter_2_osc2_input",
    "filter_2_osc3_input",
    "filter_2_sample_input",
    "filter_fx_osc1_input",
    "filter_fx_osc2_input",
    "filter_fx_osc3_input",
    "filter_fx_sample_input",
    "filter_fx_filter_input",
];

/// Ungrouped controls created with `createBaseControl` (not modulatable).
const UNMODULATED_CONTROLS: &[&str] = &[
    "bypass",
    "chorus_on",
    "chorus_sync",
    "chorus_voices",
    "compressor_band_lower_ratio",
    "compressor_band_lower_threshold",
    "compressor_band_upper_ratio",
    "compressor_band_upper_threshold",
    "compressor_enabled_bands",
    "compressor_high_lower_ratio",
    "compressor_high_lower_threshold",
    "compressor_high_upper_ratio",
    "compressor_high_upper_threshold",
    "compressor_low_lower_ratio",
    "compressor_low_lower_threshold",
    "compressor_low_upper_ratio",
    "compressor_low_upper_threshold",
    "compressor_on",
    "delay_aux_sync",
    "delay_on",
    "delay_style",
    "delay_sync",
    "distortion_filter_order",
    "distortion_on",
    "distortion_type",
    "effect_chain_order",
    "eq_band_mode",
    "eq_high_mode",
    "eq_low_mode",
    "eq_on",
    "flanger_on",
    "flanger_sync",
    "legato",
    "mod_wheel",
    "mpe_enabled",
    "oversampling",
    "phaser_on",
    "phaser_sync",
    "pitch_wheel",
    "portamento_force",
    "portamento_scale",
    "reverb_on",
    "sample_bounce",
    "sample_destination",
    "sample_keytrack",
    "sample_loop",
    "sample_on",
    "sample_random_phase",
    "sample_transpose_quantize",
    "stereo_mode",
    "view_spectrogram",
    "voice_override",
    "voice_priority",
];

/// Per-group controls created with `createBaseControl`, as
/// (group prefix, name inside the group).
const UNMODULATED_GROUP_CONTROLS: &[(&str, &[&str])] = &[
    ("env", &["attack_power", "decay_power", "release_power"]),
    ("lfo", &["smooth_mode", "sync", "sync_type"]),
    ("random", &["stereo", "style", "sync", "sync_type"]),
    (
        "osc",
        &[
            "destination",
            "distortion_type",
            "midi_track",
            "on",
            "smooth_interpolation",
            "spectral_morph_type",
            "spectral_unison",
            "stack_style",
            "transpose_quantize",
            "view_2d",
        ],
    ),
    ("filter", &["filter_input", "model", "on", "style"]),
    ("modulation", &["bipolar", "bypass", "stereo"]),
];

/// Whether the engine has a control (and so a saved value) for `name`.
pub fn is_control(name: &str) -> bool {
    registry().contains(name) && !NON_CONTROL_PARAMETERS.contains(&name)
}

/// Every engine control, sorted by name (the order of the saved `settings`
/// object).
pub fn controls() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = registry()
        .parameters()
        .iter()
        .map(|info| info.name.as_str())
        .filter(|name| is_control(name))
        .collect();
    names.sort_unstable();
    names
}

/// Whether `name` is a modulation destination (`getMonoModulationDestination`
/// returns non-null).
pub fn is_modulation_destination(name: &str) -> bool {
    if !is_control(name) || UNMODULATED_CONTROLS.contains(&name) {
        return false;
    }
    for (prefix, locals) in UNMODULATED_GROUP_CONTROLS {
        let Some(rest) = name
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('_'))
        else {
            continue;
        };
        let Some((_, local)) = rest.split_once('_') else {
            continue;
        };
        if locals.contains(&local) {
            return false;
        }
    }
    true
}

/// Every modulation destination, sorted by name.
pub fn modulation_destinations() -> Vec<&'static str> {
    controls()
        .into_iter()
        .filter(|name| is_modulation_destination(name))
        .collect()
}

/// Every modulation source, sorted by name.
pub fn modulation_sources() -> Vec<String> {
    let mut names: Vec<String> = [
        "aftertouch",
        "lift",
        "mod_wheel",
        "note",
        "note_in_octave",
        "pitch_wheel",
        "random",
        "slide",
        "stereo",
        "velocity",
    ]
    .iter()
    .map(|name| name.to_string())
    .collect();
    names.extend((1..=NUM_ENVELOPES).map(|i| format!("env_{i}")));
    names.extend((1..=NUM_LFOS).map(|i| format!("lfo_{i}")));
    names.extend((1..=NUM_RANDOM_LFOS).map(|i| format!("random_{i}")));
    names.extend((1..=NUM_MACROS).map(|i| format!("macro_control_{i}")));
    names.sort_unstable();
    names
}

/// Whether `name` is a modulation source (`getModulationSource` returns
/// non-null).
pub fn is_modulation_source(name: &str) -> bool {
    modulation_sources().iter().any(|source| source == name)
}

/// `ModulationConnection::isModulationSourceDefaultBipolar`.
pub fn is_source_default_bipolar(source: &str) -> bool {
    let prefix = source.split('_').next().unwrap_or("");
    matches!(prefix, "lfo" | "stereo" | "random" | "pitch")
}
