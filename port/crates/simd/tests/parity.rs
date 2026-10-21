// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity against the C++ reference: every case of the `simd/poly_function`
//! ref-module (`migration/corpus/modules/simd/poly_function.json`), one per
//! upstream function, under the corpus tolerances (`module_defaults`).
//!
//! The table below mirrors `migration/reference/src/modules/simd/poly_function.cpp`
//! name for name. Run with `--nocapture` to print the per-function metrics.

// Upstream literals are kept digit for digit.
#![allow(clippy::excessive_precision)]

use std::collections::BTreeMap;

use sas_parity::ModuleCase;
use sas_simd::{futils as f, poly_utils as u, utils as mu, Matrix, PolyFloat as P, PolyMask};

const MODULE: &str = "simd/poly_function/";

type PolyFunction = fn(P, P) -> P;

fn mask_value(mask: PolyMask) -> P {
    P::splat(1.0) & mask
}

fn mixed(a: P, b: P) -> P {
    u::compact_first_voices(a, b)
}

fn lanes(value: P, func: fn(f32) -> f32) -> P {
    u::map(value, func)
}

fn transposed(a: P, b: P) -> Matrix {
    let x = mixed(a, b);
    let y = mixed(b, a);
    let mut m = Matrix::new(x, y, x * y, x - y);
    m.transpose();
    m
}

fn functions() -> BTreeMap<&'static str, PolyFunction> {
    let table: &[(&'static str, PolyFunction)] = &[
        // poly_values.h
        ("add", |a, b| a + b),
        ("sub", |a, b| a - b),
        ("mul", |a, b| a * b),
        ("div", |a, b| a / b),
        ("mul_scalar", |a, _| a * 0.3),
        ("neg", |a, _| -a),
        ("max", |a, b| P::max(a, b)),
        ("min", |a, b| P::min(a, b)),
        ("abs", |a, _| P::abs(a)),
        ("mul_add", |a, b| u::mul_add(a, b, u::swap_stereo(b))),
        ("mul_sub", |a, b| u::mul_sub(a, b, u::swap_stereo(b))),
        ("sum", |a, b| P::splat(mixed(a, b).sum())),
        ("equal", |a, b| {
            mask_value(P::equal(u::round(a), u::round(b)))
        }),
        ("not_equal", |a, b| {
            mask_value(P::not_equal(u::round(a), u::round(b)))
        }),
        ("greater_than", |a, b| mask_value(P::greater_than(a, b))),
        ("less_than_or_equal", |a, b| {
            mask_value(P::less_than_or_equal(a, b))
        }),
        ("sign_mask", |a, _| mask_value(P::sign_mask(a))),
        // poly_utils.h: conversions and rounding
        ("to_int", |a, _| u::to_float(u::to_int(a))),
        ("trunc", |a, _| u::trunc(a)),
        ("floor", |a, _| u::floor(a)),
        ("ceil", |a, _| u::ceil(a)),
        ("round", |a, _| u::round(a)),
        ("round_to_int", |a, _| u::to_float(u::round_to_int(a))),
        ("floor_to_int", |a, _| u::to_float(u::floor_to_int(a))),
        ("int_min", |a, b| {
            u::to_float(sas_simd::PolyInt::min(
                u::round_to_int(a),
                u::round_to_int(b),
            ))
        }),
        ("any_mask", |a, b| {
            P::splat(P::less_than(a, b).any_mask() as f32)
        }),
        ("mod", |a, _| u::modulo(a)),
        ("mod_once", |a, _| u::mod_once(a)),
        ("sqrt", |a, _| u::sqrt(a)),
        ("pow2_to_float", |a, _| u::pow2_to_float(u::round_to_int(a))),
        // poly_utils.h: shuffles and stereo
        ("swap_stereo", |a, b| u::swap_stereo(mixed(a, b))),
        ("swap_voices", |a, b| u::swap_voices(mixed(a, b))),
        ("swap_inner", |a, b| u::swap_inner(mixed(a, b))),
        ("reverse", |a, b| u::reverse(mixed(a, b))),
        ("consolidate_audio", |a, b| u::consolidate_audio(a, b)),
        ("compact_first_voices", |a, b| u::compact_first_voices(a, b)),
        ("sum_split_audio", |a, b| u::sum_split_audio(mixed(a, b))),
        ("max_float", |a, b| P::splat(u::max_float(mixed(a, b)))),
        ("min_float", |a, b| P::splat(u::min_float(mixed(a, b)))),
        ("encode_mid_side", |a, b| u::encode_mid_side(mixed(a, b))),
        ("decode_mid_side", |a, b| u::decode_mid_side(mixed(a, b))),
        ("transpose_row0", |a, b| transposed(a, b).row0),
        ("transpose_row3", |a, b| transposed(a, b).row3),
        // poly_utils.h: interpolation and shaping
        ("interpolate", |a, b| u::interpolate(a, b, u::modulo(a + b))),
        ("perlin_interpolate", |a, b| {
            u::perlin_interpolate(a, b, u::modulo(a + b))
        }),
        ("triangle_wave", |a, _| u::triangle_wave(a)),
        ("flt_scale", |a, b| u::flt_scale(a, b)),
        ("mask_load", |a, b| {
            u::mask_load(a, b, P::greater_than(a, b))
        }),
        ("clamp", |a, _| u::clamp(a, -1.5, 2.0)),
        ("snap_transpose", |a, _| u::snap_transpose(a, 0xab5)),
        ("encode_phase_and_voice", |a, b| {
            u::encode_phase_and_voice(a, b)
        }),
        ("cubic_values", |a, _| {
            u::get_cubic_interpolation_values(a.get(0))
        }),
        ("optimal_values", |a, _| {
            u::get_optimal_interpolation_values(a.get(1))
        }),
        ("catmull_row0", |a, _| {
            u::get_catmull_interpolation_matrix(a).row0
        }),
        ("catmull_row1", |a, _| {
            u::get_catmull_interpolation_matrix(a).row1
        }),
        ("catmull_row2", |a, _| {
            u::get_catmull_interpolation_matrix(a).row2
        }),
        ("catmull_row3", |a, _| {
            u::get_catmull_interpolation_matrix(a).row3
        }),
        ("polynomial_row0", |a, _| {
            u::get_polynomial_interpolation_matrix(a).row0
        }),
        ("polynomial_row1", |a, _| {
            u::get_polynomial_interpolation_matrix(a).row1
        }),
        ("polynomial_row2", |a, _| {
            u::get_polynomial_interpolation_matrix(a).row2
        }),
        ("polynomial_row3", |a, _| {
            u::get_polynomial_interpolation_matrix(a).row3
        }),
        ("polynomial_sum_rows", |a, _| {
            u::get_polynomial_interpolation_matrix(a).sum_rows()
        }),
        ("matrix_multiply_and_sum_rows", |a, b| {
            u::get_catmull_interpolation_matrix(a)
                .multiply_and_sum_rows(&u::get_polynomial_interpolation_matrix(b))
        }),
        // poly_utils.h: libm maps
        ("poly_cents_to_ratio", |a, _| u::cents_to_ratio(a)),
        ("poly_midi_note_to_frequency", |a, _| {
            u::midi_note_to_frequency(a)
        }),
        ("poly_midi_note_to_normalized_frequency", |a, b| {
            u::midi_note_to_frequency(a) * (1.0 / b.get(0))
        }),
        ("poly_frequency_to_midi_note", |a, _| {
            u::frequency_to_midi_note(a)
        }),
        ("poly_db_to_magnitude", |a, _| u::db_to_magnitude(a)),
        ("poly_magnitude_to_db", |a, _| u::magnitude_to_db(a)),
        ("poly_sin", |a, _| u::sin(a)),
        ("poly_cos", |a, _| u::cos(a)),
        ("poly_tan", |a, _| u::tan(a)),
        ("poly_pow", |a, b| u::pow(a, b)),
        // futils.h (poly)
        ("exp2", |a, _| f::exp2(a)),
        ("log2", |a, _| f::log2(a)),
        ("cheap_exp2", |a, _| f::cheap_exp2(a)),
        ("cheap_log2", |a, _| f::cheap_log2(a)),
        ("exp", |a, _| f::exp(a)),
        ("log", |a, _| f::log(a)),
        ("exp_half", |a, _| f::exp_half(a)),
        ("pow", |a, b| f::pow(a, b)),
        ("cheap_pow", |a, b| f::cheap_pow(a, b)),
        ("midi_offset_to_ratio", |a, _| f::midi_offset_to_ratio(a)),
        ("midi_note_to_frequency", |a, _| {
            f::midi_note_to_frequency(a)
        }),
        ("magnitude_to_db", |a, _| f::magnitude_to_db(a)),
        ("db_to_magnitude", |a, _| f::db_to_magnitude(a)),
        ("quick_tanh", |a, _| f::quick_tanh(a)),
        ("quick_tanh_derivative", |a, _| f::quick_tanh_derivative(a)),
        ("algebraic_sat", |a, _| f::algebraic_sat(a)),
        ("quadratic_inv_sat", |a, _| f::quadratic_inv_sat(a)),
        ("bump_sat", |a, _| f::bump_sat(a)),
        ("bump_sat2", |a, _| f::bump_sat2(a)),
        ("algebraic_sat_derivative", |a, _| {
            f::algebraic_sat_derivative(a)
        }),
        ("tanh", |a, _| f::tanh(a)),
        ("hard_tanh", |a, _| f::hard_tanh(a)),
        ("tanh_derivative_fast", |a, _| f::tanh_derivative_fast(a)),
        ("quick_sin", |a, _| f::quick_sin(a)),
        ("sin", |a, _| f::sin(a)),
        ("quick_sin1", |a, _| f::quick_sin1(a)),
        ("sin1", |a, _| f::sin1(a)),
        ("sin_interpolate", |a, b| {
            f::sin_interpolate(a, b, u::modulo(a + b))
        }),
        ("equal_power_fade", |a, _| f::equal_power_fade(a)),
        ("equal_power_fade_inverse", |a, _| {
            f::equal_power_fade_inverse(a)
        }),
        ("pan_amplitude", |a, _| f::pan_amplitude(a)),
        ("power_scale", |a, b| f::power_scale(a, b)),
        // futils.h (mono overloads, per lane)
        ("mono_exp2", |a, _| lanes(a, f::exp2_mono)),
        ("mono_log2", |a, _| lanes(a, f::log2_mono)),
        ("mono_exp", |a, _| lanes(a, f::exp_mono)),
        ("mono_log", |a, _| lanes(a, f::log_mono)),
        ("mono_magnitude_to_db", |a, _| {
            lanes(a, f::magnitude_to_db_mono)
        }),
        ("mono_db_to_magnitude", |a, _| {
            lanes(a, f::db_to_magnitude_mono)
        }),
        ("mono_quick_tanh", |a, _| lanes(a, f::quick_tanh_mono)),
        ("mono_quick_tanh_derivative", |a, _| {
            lanes(a, f::quick_tanh_derivative_mono)
        }),
        ("mono_reciprocal_sat", |a, _| {
            lanes(a, f::reciprocal_sat_mono)
        }),
        ("mono_algebraic_sat", |a, _| lanes(a, f::algebraic_sat_mono)),
        ("mono_algebraic_sat_derivative", |a, _| {
            lanes(a, f::algebraic_sat_derivative_mono)
        }),
        ("mono_tanh", |a, _| lanes(a, f::tanh_mono)),
        ("mono_quick_sin", |a, _| lanes(a, f::quick_sin_mono)),
        ("mono_sin", |a, _| lanes(a, f::sin_mono)),
        ("mono_quick_sin1", |a, _| lanes(a, f::quick_sin1_mono)),
        ("mono_sin1", |a, _| lanes(a, f::sin1_mono)),
        ("mono_power_scale", |a, b| {
            let (x, y) = (a.to_array(), b.to_array());
            P::from_array([0, 1, 2, 3].map(|i| f::power_scale_mono(x[i], y[i])))
        }),
        // utils.h (scalar, per lane)
        ("mono_cents_to_ratio", |a, _| lanes(a, mu::cents_to_ratio)),
        ("mono_note_offset_to_ratio", |a, _| {
            lanes(a, mu::note_offset_to_ratio)
        }),
        ("mono_ratio_to_midi_transpose", |a, _| {
            lanes(a, mu::ratio_to_midi_transpose)
        }),
        ("mono_midi_note_to_frequency", |a, _| {
            lanes(a, mu::midi_note_to_frequency)
        }),
        ("mono_midi_note_to_normalized_frequency", |a, b| {
            lanes(a, mu::midi_note_to_frequency) * (1.0 / b.get(0))
        }),
        ("mono_frequency_to_midi_note", |a, _| {
            lanes(a, mu::frequency_to_midi_note)
        }),
        ("mono_utils_db_to_magnitude", |a, _| {
            lanes(a, mu::db_to_magnitude)
        }),
        ("mono_utils_magnitude_to_db", |a, _| {
            lanes(a, mu::magnitude_to_db)
        }),
        ("mono_inverse_flt_scale", |a, _| {
            lanes(a, mu::inverse_flt_scale)
        }),
    ];
    let map: BTreeMap<_, _> = table.iter().copied().collect();
    assert_eq!(map.len(), table.len(), "duplicate function name");
    map
}

/// The captures must come from a reference built for the semantics this
/// build uses (x86_64 captures for x86 semantics, arm64 captures for
/// aarch64 semantics, including the `aarch64-semantics` feature on x86_64).
fn assert_capture_matches_semantics(id: &str) {
    let path = sas_parity::captures::captures_dir()
        .join("modules")
        .join(id)
        .join("capture.json");
    let capture: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let arch = capture["build"]["arch"].as_str().unwrap_or_default();
    let capture_is_arm = matches!(arch, "arm64" | "aarch64");
    assert_eq!(
        capture_is_arm,
        sas_simd::AARCH64_SEMANTICS,
        "captures in {} were rendered on {arch:?}, but this build uses {} semantics; \
         compare against a reference of the same ISA (migration/decisions/reference-arm.md)",
        path.display(),
        if sas_simd::AARCH64_SEMANTICS {
            "aarch64"
        } else {
            "x86_64"
        }
    );
}

fn case_ids() -> Vec<String> {
    sas_parity::all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with(MODULE))
        .collect()
}

#[test]
fn function_table_matches_case_file() {
    let table = functions();
    let mut from_cases: Vec<String> = Vec::new();
    for id in case_ids() {
        let case = ModuleCase::load(&id);
        let name = case.params()["function"].as_str().unwrap().to_string();
        assert_eq!(id, format!("{MODULE}{name}"), "case id and function differ");
        from_cases.push(name);
    }
    from_cases.sort();
    let from_table: Vec<String> = table.keys().map(|k| k.to_string()).collect();
    assert_eq!(
        from_cases, from_table,
        "tests/parity.rs and poly_function.json disagree"
    );
}

#[test]
fn poly_functions_match_reference() {
    let table = functions();
    let mut failures = Vec::new();
    let mut exact_cases = 0;
    let mut worst_abs = (0.0f64, String::new());
    let mut worst_residual = (f64::NEG_INFINITY, String::new());
    println!(
        "{:<32} {:>12} {:>12} {:>10}",
        "function", "max_abs", "residual_db", "exact"
    );
    let ids = case_ids();
    for id in &ids {
        let case = ModuleCase::load(id);
        assert_capture_matches_semantics(id);
        let name = case.params()["function"].as_str().unwrap();
        let function = table[name];
        let a = case.input("a");
        let b = case.input("b");
        let output: Vec<[f32; 4]> = a
            .iter()
            .zip(&b)
            .map(|(a, b)| function(P::from_array(*a), P::from_array(*b)).to_array())
            .collect();

        let reference = case.reference("out");
        let lanes = reference.len() * 4;
        let exact = reference
            .iter()
            .flatten()
            .zip(output.iter().flatten())
            .filter(|(r, t)| r.to_bits() == t.to_bits())
            .count();
        if exact == lanes {
            exact_cases += 1;
        }

        let report = case.report("out", &output);
        println!(
            "{:<32} {:>12.3e} {:>12.1} {:>9.1}%",
            name,
            report.max_abs_error,
            report.residual_db,
            100.0 * exact as f64 / lanes as f64
        );
        if report.max_abs_error > worst_abs.0 {
            worst_abs = (report.max_abs_error, name.to_string());
        }
        if report.residual_db > worst_residual.0 {
            worst_residual = (report.residual_db, name.to_string());
        }
        if !report.pass {
            failures.push(id.clone());
        }
    }
    println!(
        "{} cases, {} bit-identical; worst max_abs_error {:.3e} ({}), worst residual {:.1} dB ({})",
        ids.len(),
        exact_cases,
        worst_abs.0,
        worst_abs.1,
        worst_residual.0,
        worst_residual.1
    );
    assert!(!ids.is_empty(), "no {MODULE} cases found");
    if let Some(first) = failures.first() {
        eprintln!("failing cases: {failures:?}");
        // Re-run the first failure through the standard assertion for its
        // report and failure dump.
        let case = ModuleCase::load(first);
        let function = table[case.params()["function"].as_str().unwrap()];
        let output: Vec<[f32; 4]> = case
            .input("a")
            .iter()
            .zip(&case.input("b"))
            .map(|(a, b)| function(P::from_array(*a), P::from_array(*b)).to_array())
            .collect();
        case.assert_parity("out", &output);
    }
}
