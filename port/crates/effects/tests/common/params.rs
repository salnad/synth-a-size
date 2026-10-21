// SPDX-License-Identifier: GPL-3.0-or-later

//! The `ValueDetails` of the effect parameters, copied from upstream's
//! `vital/src/common/synth_parameters.cpp` (min, max, default, post offset,
//! scale), installed as the parameter lookup that effect modules read when
//! they create their controls. The registry itself is `sas-preset` (M2); this
//! table covers only the names the effect captures use.

use std::sync::Once;

use sas_dsp_core::synth_module::{set_parameter_lookup, ParameterDetails, ValueScale};

use ValueScale::{Exponential, Indexed, Linear, Quadratic};

#[rustfmt::skip]
const PARAMETERS: &[(&str, f32, f32, f32, f32, ValueScale)] = &[
    // name, min, max, default, post_offset, scale
    ("delay_dry_wet", 0.0, 1.0, 0.3334, 0.0, Linear),
    ("delay_feedback", -1.0, 1.0, 0.5, 0.0, Linear),
    ("delay_frequency", -2.0, 9.0, 2.0, 0.0, Exponential),
    ("delay_aux_frequency", -2.0, 9.0, 2.0, 0.0, Exponential),
    ("delay_style", 0.0, 3.0, 0.0, 0.0, Indexed),
    ("delay_filter_cutoff", 8.0, 136.0, 60.0, 0.0, Linear),
    ("delay_filter_spread", 0.0, 1.0, 1.0, 0.0, Linear),
    ("delay_tempo", 4.0, 12.0, 9.0, 0.0, Indexed),
    ("delay_aux_tempo", 4.0, 12.0, 9.0, 0.0, Indexed),
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

/// Installs the lookup (once per test binary).
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        set_parameter_lookup(|name| {
            let &(_, min, max, default_value, post_offset, value_scale) = PARAMETERS
                .iter()
                .find(|entry| entry.0 == name)
                .unwrap_or_else(|| panic!("parameter {name} is not in tests/common/params.rs"));
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
