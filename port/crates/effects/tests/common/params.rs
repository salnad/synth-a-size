// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `ValueDetails` of the effect parameters, copied from upstream's
//! `vital/src/common/synth_parameters.cpp` (min, max, default, post offset,
//! scale), installed as the parameter lookup that effect modules read when
//! they create their controls. The registry itself is `sas-preset` (M2); this
//! table covers the names the single-effect captures use, and other names fall
//! back to the registry.

use std::sync::Once;

use sas_dsp_core::synth_module::{set_parameter_lookup, ParameterDetails, ValueScale};

use ValueScale::{Exponential, Indexed, Linear, Quadratic};

#[rustfmt::skip]
const PARAMETERS: &[(&str, f32, f32, f32, f32, ValueScale)] = &[
    // name, min, max, default, post_offset, scale
    ("compressor_low_upper_threshold", -80.0, 0.0, -28.0, 0.0, Linear),
    ("compressor_band_upper_threshold", -80.0, 0.0, -25.0, 0.0, Linear),
    ("compressor_high_upper_threshold", -80.0, 0.0, -30.0, 0.0, Linear),
    ("compressor_low_lower_threshold", -80.0, 0.0, -35.0, 0.0, Linear),
    ("compressor_band_lower_threshold", -80.0, 0.0, -36.0, 0.0, Linear),
    ("compressor_high_lower_threshold", -80.0, 0.0, -35.0, 0.0, Linear),
    ("compressor_low_upper_ratio", 0.0, 1.0, 0.9, 0.0, Linear),
    ("compressor_band_upper_ratio", 0.0, 1.0, 0.857, 0.0, Linear),
    ("compressor_high_upper_ratio", 0.0, 1.0, 1.0, 0.0, Linear),
    ("compressor_low_lower_ratio", -1.0, 1.0, 0.8, 0.0, Linear),
    ("compressor_band_lower_ratio", -1.0, 1.0, 0.8, 0.0, Linear),
    ("compressor_high_lower_ratio", -1.0, 1.0, 0.8, 0.0, Linear),
    ("compressor_low_gain", -30.0, 30.0, 16.3, 0.0, Linear),
    ("compressor_band_gain", -30.0, 30.0, 11.7, 0.0, Linear),
    ("compressor_high_gain", -30.0, 30.0, 16.3, 0.0, Linear),
    ("compressor_attack", 0.0, 1.0, 0.5, 0.0, Linear),
    ("compressor_release", 0.0, 1.0, 0.5, 0.0, Linear),
    ("compressor_enabled_bands", 0.0, 3.0, 0.0, 0.0, Indexed),
    ("compressor_mix", 0.0, 1.0, 1.0, 0.0, Linear),
    ("delay_dry_wet", 0.0, 1.0, 0.3334, 0.0, Linear),
    ("delay_feedback", -1.0, 1.0, 0.5, 0.0, Linear),
    ("delay_frequency", -2.0, 9.0, 2.0, 0.0, Exponential),
    ("delay_aux_frequency", -2.0, 9.0, 2.0, 0.0, Exponential),
    ("delay_style", 0.0, 3.0, 0.0, 0.0, Indexed),
    ("delay_filter_cutoff", 8.0, 136.0, 60.0, 0.0, Linear),
    ("delay_filter_spread", 0.0, 1.0, 1.0, 0.0, Linear),
    ("delay_tempo", 4.0, 12.0, 9.0, 0.0, Indexed),
    ("delay_aux_tempo", 4.0, 12.0, 9.0, 0.0, Indexed),
    ("distortion_type", 0.0, 5.0, 0.0, 0.0, Indexed),
    ("distortion_drive", -30.0, 30.0, 0.0, 0.0, Linear),
    ("distortion_mix", 0.0, 1.0, 1.0, 0.0, Linear),
    ("distortion_filter_order", 0.0, 2.0, 0.0, 0.0, Indexed),
    ("distortion_filter_cutoff", 8.0, 136.0, 80.0, 0.0, Linear),
    ("distortion_filter_resonance", 0.0, 1.0, 0.5, 0.0, Linear),
    ("distortion_filter_blend", 0.0, 2.0, 0.0, 0.0, Linear),
    ("eq_low_mode", 0.0, 1.0, 0.0, 0.0, Indexed),
    ("eq_low_cutoff", 8.0, 136.0, 40.0, 0.0, Linear),
    ("eq_low_gain", -15.0, 15.0, 0.0, 0.0, Linear),
    ("eq_low_resonance", 0.0, 1.0, 0.3163, 0.0, Quadratic),
    ("eq_band_mode", 0.0, 1.0, 0.0, 0.0, Indexed),
    ("eq_band_cutoff", 8.0, 136.0, 80.0, 0.0, Linear),
    ("eq_band_gain", -15.0, 15.0, 0.0, 0.0, Linear),
    ("eq_band_resonance", 0.0, 1.0, 0.4473, 0.0, Quadratic),
    ("eq_high_mode", 0.0, 1.0, 0.0, 0.0, Indexed),
    ("eq_high_cutoff", 8.0, 136.0, 100.0, 0.0, Linear),
    ("eq_high_gain", -15.0, 15.0, 0.0, 0.0, Linear),
    ("eq_high_resonance", 0.0, 1.0, 0.3163, 0.0, Quadratic),
    ("reverb_pre_low_cutoff", 0.0, 128.0, 0.0, 0.0, Linear),
    ("reverb_pre_high_cutoff", 0.0, 128.0, 110.0, 0.0, Linear),
    ("reverb_low_shelf_cutoff", 0.0, 128.0, 0.0, 0.0, Linear),
    ("reverb_low_shelf_gain", -6.0, 0.0, 0.0, 0.0, Linear),
    ("reverb_high_shelf_cutoff", 0.0, 128.0, 90.0, 0.0, Linear),
    ("reverb_high_shelf_gain", -6.0, 0.0, -1.0, 0.0, Linear),
    ("reverb_dry_wet", 0.0, 1.0, 0.25, 0.0, Linear),
    ("reverb_delay", 0.0, 0.3, 0.0, 0.0, Linear),
    ("reverb_decay_time", -6.0, 6.0, 0.0, 0.0, Exponential),
    ("reverb_size", 0.0, 1.0, 0.5, 0.0, Linear),
    ("reverb_chorus_amount", 0.0, 1.0, 0.223607, 0.0, Quadratic),
    ("reverb_chorus_frequency", -8.0, 3.0, -2.0, 0.0, Exponential),
    ("phaser_dry_wet", 0.0, 1.0, 1.0, 0.0, Linear),
    ("phaser_feedback", 0.0, 1.0, 0.5, 0.0, Linear),
    ("phaser_frequency", -5.0, 2.0, -3.0, 0.0, Exponential),
    ("phaser_tempo", 0.0, 10.0, 3.0, 0.0, Indexed),
    ("phaser_center", 8.0, 136.0, 80.0, 0.0, Linear),
    ("phaser_blend", 0.0, 2.0, 1.0, 0.0, Linear),
    ("phaser_mod_depth", 0.0, 48.0, 24.0, 0.0, Linear),
    ("phaser_phase_offset", 0.0, 1.0, 0.33333333, 0.0, Linear),
    ("flanger_dry_wet", 0.0, 0.5, 0.5, 0.0, Linear),
    ("flanger_feedback", -1.0, 1.0, 0.5, 0.0, Linear),
    ("flanger_frequency", -5.0, 2.0, 2.0, 0.0, Exponential),
    ("flanger_tempo", 0.0, 10.0, 4.0, 0.0, Indexed),
    ("flanger_center", 8.0, 136.0, 64.0, 0.0, Linear),
    ("flanger_mod_depth", 0.0, 1.0, 0.5, 0.0, Linear),
    ("flanger_phase_offset", 0.0, 1.0, 0.33333333, 0.0, Linear),
    ("chorus_dry_wet", 0.0, 1.0, 0.5, 0.0, Linear),
    ("chorus_feedback", -0.95, 0.95, 0.4, 0.0, Linear),
    ("chorus_cutoff", 8.0, 136.0, 60.0, 0.0, Linear),
    ("chorus_spread", 0.0, 1.0, 1.0, 0.0, Linear),
    ("chorus_voices", 1.0, 4.0, 4.0, 0.0, Indexed),
    ("chorus_frequency", -6.0, 3.0, -3.0, 0.0, Exponential),
    ("chorus_tempo", 0.0, 10.0, 4.0, 0.0, Indexed),
    ("chorus_mod_depth", 0.0, 1.0, 0.5, 0.0, Linear),
    ("chorus_delay_1", -10.0, -5.64386, -9.0, 0.0, Exponential),
    ("chorus_delay_2", -10.0, -5.64386, -7.0, 0.0, Exponential),
];

/// The table rows (name, min, max, default, post offset, scale).
pub fn rows() -> &'static [(&'static str, f32, f32, f32, f32, ValueScale)] {
    PARAMETERS
}

/// `sas-preset`'s registry entry for `name`, for the parameters this table
/// does not list (the effect chain's `<effect>_on` switches and the
/// `filter_fx_*` group).
fn registry_details(name: &str) -> ParameterDetails {
    use sas_preset::registry::{self, ValueScale as PresetScale};
    let info = registry::lookup(name).unwrap_or_else(|| {
        panic!("parameter {name} is not in tests/common/params.rs or the registry")
    });
    ParameterDetails {
        min: info.min,
        max: info.max,
        default_value: info.default_value,
        post_offset: info.post_offset,
        value_scale: match info.value_scale {
            PresetScale::Indexed => ValueScale::Indexed,
            PresetScale::Linear => ValueScale::Linear,
            PresetScale::Quadratic => ValueScale::Quadratic,
            PresetScale::Cubic => ValueScale::Cubic,
            PresetScale::Quartic => ValueScale::Quartic,
            PresetScale::SquareRoot => ValueScale::SquareRoot,
            PresetScale::Exponential => ValueScale::Exponential,
        },
    }
}

/// Installs the lookup (once per test binary).
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        set_parameter_lookup(|name| {
            let Some(&(_, min, max, default_value, post_offset, value_scale)) =
                PARAMETERS.iter().find(|entry| entry.0 == name)
            else {
                return registry_details(name);
            };
            ParameterDetails {
                min,
                max,
                default_value,
                post_offset,
                value_scale,
            }
        })
    });
}
