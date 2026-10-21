// SPDX-License-Identifier: GPL-3.0-or-later

//! Test-only parameter lookup for the module parity tests.
//!
//! The engine installs `sas-preset`'s registry with
//! `set_parameter_lookup`. These tests only need the controls the
//! modulation modules create, so the relevant rows are copied here from
//! `vital/src/common/synth_parameters.cpp` (`env_parameter_list`,
//! `lfo_parameter_list`, `random_lfo_parameter_list`,
//! `mod_parameter_list`: min, max, default, post offset, scale). The parity
//! captures check them: a wrong row changes the module output.

use sas_dsp_core::synth_module::{set_parameter_lookup, ParameterDetails};
use sas_dsp_core::ValueScale::{self, Exponential, Indexed, Linear, Quartic};

type Row = (&'static str, f32, f32, f32, f32, ValueScale);

// Upstream's literal (a double rounded to float), kept verbatim.
#[allow(clippy::excessive_precision, clippy::approx_constant)]
const ENV: &[Row] = &[
    ("delay", 0.0, 1.414_213_562_4, 0.0, 0.0, Quartic),
    ("attack", 0.0, 2.378_42, 0.1495, 0.0, Quartic),
    ("hold", 0.0, 1.414_213_562_4, 0.0, 0.0, Quartic),
    ("decay", 0.0, 2.378_42, 1.0, 0.0, Quartic),
    ("release", 0.0, 2.378_42, 0.5476, 0.0, Quartic),
    ("attack_power", -20.0, 20.0, 0.0, 0.0, Linear),
    ("decay_power", -20.0, 20.0, -2.0, 0.0, Linear),
    ("release_power", -20.0, 20.0, -2.0, 0.0, Linear),
    ("sustain", 0.0, 1.0, 1.0, 0.0, Linear),
];

const LFO: &[Row] = &[
    ("phase", 0.0, 1.0, 0.0, 0.0, Linear),
    ("sync_type", 0.0, 5.0, 0.0, 0.0, Indexed),
    ("frequency", -7.0, 9.0, 1.0, 0.0, Exponential),
    ("sync", 0.0, 4.0, 1.0, 0.0, Indexed),
    ("tempo", 0.0, 12.0, 7.0, 0.0, Indexed),
    ("fade_time", 0.0, 8.0, 0.0, 0.0, Linear),
    ("smooth_mode", 0.0, 1.0, 1.0, 0.0, Indexed),
    ("smooth_time", -10.0, 4.0, -7.5, 0.0, Exponential),
    ("delay_time", 0.0, 4.0, 0.0, 0.0, Linear),
    ("stereo", -0.5, 0.5, 0.0, 0.0, Linear),
    ("keytrack_transpose", -60.0, 36.0, -12.0, 0.0, Indexed),
    ("keytrack_tune", -1.0, 1.0, 0.0, 0.0, Linear),
];

const RANDOM_LFO: &[Row] = &[
    ("style", 0.0, 3.0, 0.0, 0.0, Indexed),
    ("frequency", -7.0, 9.0, 1.0, 0.0, Exponential),
    ("sync", 0.0, 4.0, 1.0, 0.0, Indexed),
    ("tempo", 0.0, 12.0, 8.0, 0.0, Indexed),
    ("stereo", 0.0, 1.0, 0.0, 0.0, Indexed),
    ("sync_type", 0.0, 1.0, 0.0, 0.0, Indexed),
    ("keytrack_transpose", -60.0, 36.0, -12.0, 0.0, Indexed),
    ("keytrack_tune", -1.0, 1.0, 0.0, 0.0, Linear),
];

const MODULATION: &[Row] = &[
    ("amount", -1.0, 1.0, 0.0, 0.0, Linear),
    ("power", -10.0, 10.0, 0.0, 0.0, Linear),
    ("bipolar", 0.0, 1.0, 0.0, 0.0, Indexed),
    ("stereo", 0.0, 1.0, 0.0, 0.0, Indexed),
    ("bypass", 0.0, 1.0, 0.0, 0.0, Indexed),
];

/// Splits `<group>_<n>_<parameter>` and returns `<parameter>`.
fn suffix<'a>(name: &'a str, group: &str) -> Option<&'a str> {
    let rest = name.strip_prefix(group)?.strip_prefix('_')?;
    let (index, parameter) = rest.split_once('_')?;
    index.parse::<u32>().ok()?;
    Some(parameter)
}

fn lookup(name: &str) -> ParameterDetails {
    let groups: [(&str, &[Row]); 4] = [
        ("env", ENV),
        ("lfo", LFO),
        ("random", RANDOM_LFO),
        ("modulation", MODULATION),
    ];
    for (group, rows) in groups {
        if let Some(parameter) = suffix(name, group) {
            if let Some(&(_, min, max, default_value, post_offset, value_scale)) =
                rows.iter().find(|row| row.0 == parameter)
            {
                return ParameterDetails {
                    min,
                    max,
                    default_value,
                    post_offset,
                    value_scale,
                };
            }
        }
    }
    panic!("test parameter table has no {name}");
}

/// Installs the lookup (idempotent).
pub fn install() {
    set_parameter_lookup(lookup);
}
