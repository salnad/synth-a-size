// SPDX-License-Identifier: GPL-3.0-or-later

//! `sas_common::scalar` (the single-lane helpers, which this crate
//! re-exports as its mono overloads) against the poly versions here, bit for
//! bit, wherever upstream's arithmetic is the same.
//!
//! Upstream evaluates its scalar helpers in lane 0 of a `poly_float`, so the
//! single-lane copy must agree with every lane of the poly version:
//!
//! | `sas_common::scalar` (= `sas_simd` mono) | `sas_simd` poly |
//! |---|---|
//! | `to_int`, `poly_trunc`, `poly_floor`, `round_to_int`, `pow2_to_float` | `poly_utils::{to_int, trunc, floor, round_to_int, pow2_to_float}` |
//! | `exp2`, `log2`, `exp`, `log`, `pow` | `futils::{exp2, log2, exp, log, pow}` |
//! | `interpolate` | `poly_utils::interpolate` |
//! | `catmull_weights`, `catmull_interpolate` | `get_catmull_interpolation_matrix` rows, and its `multiply_and_sum_rows` with the transposed `get_value_matrix` |
//!
//! Both columns of `toInt` (reference-arm.md A1) are ported on both sides,
//! so this holds for every input, NaN and out-of-range values included, with
//! and without `aarch64-semantics`.
//!
//! Where upstream's mono and poly arithmetic differs, the overloads are
//! *expected* to differ, and `upstream_differs_*` pins an input that shows
//! it:
//!
//! - `tanh`: the `mono_float` and `poly_float` overloads evaluate the rational
//!   in a different order.
//! - `power_scale`: the `mono_float` overload returns `value` for
//!   `|power| < 0.01`, the `poly_float` one for `|power| < 0.005`.
//! - `min`/`max`: `utils.h` uses `fmin`/`fmax`, and `poly_float::min`/`max`
//!   use `MINPS`/`MAXPS` or `FMIN`/`FMAX` (A5). They differ on NaN. On a
//!   `±0` tie SSE2 returns the second operand; neither C's `fmin`/`fmax` nor
//!   Rust's `f32::min`/`max` specifies the sign there, so only the poly side
//!   is pinned.
//! - `clamp`: `utils.h` computes `fmin(max, fmax(value, min))` and
//!   `poly_utils.h` `max(min(value, max), min)`. They differ for inverted
//!   bounds (`min > max`) and NaN on both columns, and agree otherwise up to
//!   the sign of zero.
//!
//! `tests/futils_mono_vs_poly.rs` covers the remaining `futils` pairs.

use sas_common::scalar as s;
use sas_simd::{futils as f, poly_utils as u, utils as mu, PolyFloat as P, PolyInt};

const TWO_POW_31: f32 = 2_147_483_648.0;

fn lcg(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *state
}

/// Special values, a dense grid over the working range, uniform values in
/// ±40 and random bit patterns (all classes, including NaN and infinities).
fn floats() -> Vec<f32> {
    let mut values = vec![
        0.0,
        -0.0,
        0.5,
        -0.5,
        1.5,
        -1.5,
        2.5,
        -2.5,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        1.0e-40,
        -1.0e-40,
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        -f32::NAN,
        8_388_607.5,
        -8_388_607.5,
        TWO_POW_31,
        -TWO_POW_31,
        2_147_483_520.0,
        -2_147_483_520.0,
        3.0e9,
        -3.0e9,
        126.99,
        127.5,
        128.0,
        -126.5,
        -149.0,
    ];
    values.extend((-300 * 128..=300 * 128).map(|i| i as f32 / 128.0));
    let mut state = 0x2545_f491;
    for _ in 0..100_000 {
        values.push((lcg(&mut state) >> 8) as f32 / (1 << 24) as f32 * 80.0 - 40.0);
    }
    for _ in 0..300_000 {
        values.push(f32::from_bits(lcg(&mut state)));
    }
    values
}

/// Pairs `(a, b)` from two differently ordered copies of [`floats`].
fn float_pairs() -> Vec<(f32, f32)> {
    let a = floats();
    let mut b = a.clone();
    let mut state = 0x9e37_79b9;
    for i in (1..b.len()).rev() {
        b.swap(i, lcg(&mut state) as usize % (i + 1));
    }
    a.into_iter().zip(b).collect()
}

#[track_caller]
fn assert_same(name: &str, input: impl std::fmt::Debug, scalar: f32, poly: f32) {
    assert_eq!(
        scalar.to_bits(),
        poly.to_bits(),
        "{name}({input:?}): sas-common {scalar:e} vs poly lane {poly:e}"
    );
}

/// Runs `poly` on four consecutive inputs at once, so every lane is checked.
fn lanes(inputs: &[f32], poly: impl Fn(P) -> P) -> Vec<f32> {
    let mut out = Vec::with_capacity(inputs.len());
    for chunk in inputs.chunks(4) {
        let mut lanes = [0.0; 4];
        lanes[..chunk.len()].copy_from_slice(chunk);
        let result = poly(P::from(lanes)).to_array();
        out.extend_from_slice(&result[..chunk.len()]);
    }
    out
}

fn check_unary(name: &str, inputs: &[f32], scalar: impl Fn(f32) -> f32, poly: impl Fn(P) -> P) {
    for (&x, &lane) in inputs.iter().zip(&lanes(inputs, poly)) {
        assert_same(name, x, scalar(x), lane);
    }
}

#[test]
fn rounding_on_this_column() {
    let all = floats();
    check_unary(
        "to_int",
        &all,
        |x| s::to_int(x) as f32,
        |p| u::to_float(u::to_int(p)),
    );
    check_unary("poly_trunc", &all, s::poly_trunc, u::trunc);
    check_unary("poly_floor", &all, s::poly_floor, u::floor);
    check_unary(
        "round_to_int",
        &all,
        |x| s::round_to_int(x) as f32,
        |p| u::to_float(u::round_to_int(p)),
    );

    let mut state = 0x0bad_5eed;
    let mut ints: Vec<i32> = (-300..=300).collect();
    ints.extend([i32::MIN, i32::MAX, -127, -126, 127, 128]);
    ints.extend((0..100_000).map(|_| lcg(&mut state) as i32));
    for value in ints {
        let poly = u::pow2_to_float(PolyInt::splat(value as u32)).get(0);
        assert_same("pow2_to_float", value, s::pow2_to_float(value), poly);
    }
}

#[test]
fn exp2_log2_exp_log() {
    let all = floats();
    check_unary("exp2", &all, s::exp2, f::exp2);
    check_unary("log2", &all, s::log2, f::log2);
    check_unary("exp", &all, s::exp, f::exp);
    check_unary("log", &all, s::log, f::log);
}

#[test]
fn pow() {
    for (base, exponent) in float_pairs() {
        let poly = f::pow(P::splat(base), P::splat(exponent)).get(0);
        assert_same("pow", (base, exponent), s::pow(base, exponent), poly);
    }
}

#[test]
fn interpolate() {
    let ts = floats();
    for ((from, to), &t) in float_pairs().into_iter().zip(ts.iter().rev()) {
        let poly = u::interpolate(from, to, t).get(0);
        assert_same(
            "interpolate",
            (from, to, t),
            s::interpolate(from, to, t),
            poly,
        );
    }
}

#[test]
fn catmull() {
    let ts = floats();
    let mut state = 0xc0ff_ee11;
    let buffer: Vec<f32> = (0..4096)
        .map(|_| (lcg(&mut state) >> 8) as f32 / (1 << 24) as f32 * 2.0 - 1.0)
        .collect();
    for chunk in ts.chunks(4) {
        let mut t = [0.0; 4];
        t[..chunk.len()].copy_from_slice(chunk);
        let matrix = u::get_catmull_interpolation_matrix(P::from(t));
        let rows = [matrix.row0, matrix.row1, matrix.row2, matrix.row3];

        let indices: [u32; 4] =
            std::array::from_fn(|_| lcg(&mut state) % (buffer.len() as u32 - 4));
        let mut values = u::get_value_matrix(&buffer, PolyInt::from(indices));
        values.transpose();
        let interpolated = matrix.multiply_and_sum_rows(&values);

        for lane in 0..chunk.len() {
            let weights = s::catmull_weights(t[lane]);
            for (row, weight) in rows.iter().zip(weights) {
                assert_same("catmull_weights", t[lane], weight, row.get(lane));
            }
            assert_same(
                "catmull_interpolate",
                (indices[lane], t[lane]),
                s::catmull_interpolate(&buffer, indices[lane] as usize, t[lane]),
                interpolated.get(lane),
            );
        }
    }
}

/// The out-of-range results differ between the columns, and both crates
/// pick the same column (`sas-simd`'s `aarch64-semantics` forwards to
/// `sas-common`'s).
#[test]
fn both_crates_use_the_same_column() {
    assert_eq!(sas_simd::AARCH64_SEMANTICS, sas_common::AARCH64_SEMANTICS);
    let (floor_nan, exp2_huge) = if sas_simd::AARCH64_SEMANTICS {
        (0.0, 0.5)
    } else {
        (-TWO_POW_31, f32::INFINITY)
    };
    assert_eq!(s::poly_floor(f32::NAN), floor_nan);
    assert_eq!(u::floor(P::splat(f32::NAN)).get(0), floor_nan);
    assert_eq!(s::exp2(TWO_POW_31), exp2_huge);
    assert_eq!(f::exp2(P::splat(TWO_POW_31)).get(0), exp2_huge);
}

#[test]
fn upstream_differs_tanh_overloads() {
    let differs = floats()
        .into_iter()
        .filter(|x| x.is_finite())
        .any(|x| f::tanh_mono(x).to_bits() != f::tanh(P::splat(x)).get(0).to_bits());
    assert!(
        differs,
        "the mono and poly tanh overloads no longer differ anywhere"
    );
}

#[test]
fn upstream_differs_power_scale_overloads() {
    let (value, power) = (0.25, 0.007);
    assert_eq!(f::power_scale_mono(value, power), value);
    let poly = f::power_scale(P::splat(value), P::splat(power)).get(0);
    assert_ne!(poly.to_bits(), value.to_bits());
}

#[test]
fn upstream_differs_min_max() {
    let nan = f32::NAN;
    assert_eq!(mu::min(1.0, nan), 1.0);
    assert_eq!(mu::max(1.0, nan), 1.0);
    assert!(P::min(P::splat(1.0), P::splat(nan)).get(0).is_nan());
    assert!(P::max(P::splat(1.0), P::splat(nan)).get(0).is_nan());

    let (zero, negative_zero) = (P::splat(0.0), P::splat(-0.0));
    let max_ties = [
        P::max(zero, negative_zero).get(0).to_bits(),
        P::max(negative_zero, zero).get(0).to_bits(),
    ];
    let min_ties = [
        P::min(zero, negative_zero).get(0).to_bits(),
        P::min(negative_zero, zero).get(0).to_bits(),
    ];
    let (zero, negative_zero) = (0.0f32.to_bits(), (-0.0f32).to_bits());
    if sas_simd::AARCH64_SEMANTICS {
        assert_eq!(max_ties, [zero, zero]);
        assert_eq!(min_ties, [negative_zero, negative_zero]);
    } else {
        assert_eq!(max_ties, [negative_zero, zero]);
        assert_eq!(min_ties, [negative_zero, zero]);
    }
}

#[test]
fn upstream_differs_clamp() {
    assert_eq!(mu::clamp(0.0, 1.0, -1.0), -1.0);
    assert_eq!(u::clamp(P::splat(0.0), 1.0, -1.0).get(0), 1.0);
    assert_eq!(mu::clamp(f32::NAN, -1.0, 1.0), -1.0);
    let poly_nan = u::clamp(P::splat(f32::NAN), -1.0, 1.0).get(0);
    assert_ne!(poly_nan.to_bits(), (-1.0f32).to_bits());

    for (a, b) in float_pairs() {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        for value in [a, b, 0.5, -3.0] {
            if value.is_nan() || low.is_nan() || high.is_nan() {
                continue;
            }
            let mono = mu::clamp(value, low, high);
            let poly = u::clamp(P::splat(value), low, high).get(0);
            assert!(
                mono == poly,
                "clamp({value}, {low}, {high}): mono {mono} vs poly {poly}"
            );
        }
    }
}
