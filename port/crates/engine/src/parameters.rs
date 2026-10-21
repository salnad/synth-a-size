// SPDX-License-Identifier: GPL-3.0-or-later

//! Connects the engine graph to the parameter registry.
//!
//! `SynthModule` controls read their default, range and value scale through
//! `sas_dsp_core::synth_module::parameter_details` (upstream
//! `Parameters::getDetails`). [`install`] backs it with `sas-preset`'s
//! registry.

use sas_dsp_core::synth_module::{set_parameter_lookup, ParameterDetails, ValueScale};
use sas_preset::ValueScale as PresetScale;

/// Installs the `sas-preset` registry as the graph's parameter lookup.
/// Idempotent. Unknown names panic, as upstream asserts.
pub fn install() {
    set_parameter_lookup(details);
}

/// `Parameters::getDetails(name)` in the graph's form.
pub fn details(name: &str) -> ParameterDetails {
    let info = sas_preset::lookup(name).unwrap_or_else(|| panic!("unknown parameter {name:?}"));
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
