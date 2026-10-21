// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks `tests/common/params.rs` against upstream's
//! `vital/src/common/synth_parameters.cpp`, so the effect modules are built
//! with the registry's real `ValueDetails`.

mod common;

use sas_dsp_core::synth_module::ValueScale;

/// The first `{ "name", version, min, max, default, post_offset, display_multiply,
/// ValueDetails::kScale` entry for `name`, as (min, max, default, post_offset, scale).
fn upstream(source: &str, name: &str) -> (f32, f32, f32, f32, String) {
    let start = source
        .find(&format!("{{ \"{name}\","))
        .unwrap_or_else(|| panic!("{name} is not in synth_parameters.cpp"));
    let entry = &source[start..];
    let entry = &entry[..entry.find("},").unwrap()];
    let fields: Vec<&str> = entry.split(',').map(str::trim).collect();
    let number = |field: &str| -> f32 {
        match field {
            "Distortion::kMinDrive" => -30.0,
            "Distortion::kMaxDrive" => 30.0,
            "DigitalSvf::kMinGain" => -15.0,
            "DigitalSvf::kMaxGain" => 15.0,
            "vital::MultibandCompressor::kNumBandOptions - 1" => 3.0,
            other => other
                .trim_end_matches('f')
                .parse()
                .unwrap_or_else(|_| panic!("{name}: cannot parse {other:?}")),
        }
    };
    let scale = fields[7]
        .strip_prefix("ValueDetails::k")
        .unwrap_or_else(|| panic!("{name}: no ValueDetails scale in {entry:?}"))
        .to_string();
    (
        number(fields[2]),
        number(fields[3]),
        number(fields[4]),
        number(fields[5]),
        scale,
    )
}

fn scale_name(scale: ValueScale) -> &'static str {
    match scale {
        ValueScale::Indexed => "Indexed",
        ValueScale::Linear => "Linear",
        ValueScale::Quadratic => "Quadratic",
        ValueScale::Cubic => "Cubic",
        ValueScale::Quartic => "Quartic",
        ValueScale::SquareRoot => "SquareRoot",
        ValueScale::Exponential => "Exponential",
    }
}

#[test]
fn parameter_table_matches_upstream() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../vital/src/common/synth_parameters.cpp");
    let source = std::fs::read_to_string(path).unwrap();
    for &(name, min, max, default_value, post_offset, scale) in common::params::rows() {
        let (u_min, u_max, u_default, u_post_offset, u_scale) = upstream(&source, name);
        assert_eq!(
            (min, max, default_value, post_offset, scale_name(scale)),
            (u_min, u_max, u_default, u_post_offset, u_scale.as_str()),
            "{name}"
        );
    }
}
