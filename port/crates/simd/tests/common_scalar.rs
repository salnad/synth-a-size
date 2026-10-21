// SPDX-License-Identifier: GPL-3.0-or-later

//! `sas_common::scalar` against this crate, bit for bit, wherever upstream's
//! arithmetic is identical.
//!
//! Both crates port the same upstream helpers (`futils.h`, `utils.h`,
//! `poly_utils.h`): `sas-common` one lane for non-processor code, this crate
//! lane-wise for processors. Upstream has one definition per helper, so the
//! two copies must agree bit for bit:
//!
//! | `sas_common::scalar` | `sas_simd` |
//! |---|---|
//! | `exp2`, `log2`, `exp`, `log`, `pow` | `futils::{exp2,log2,exp,log,pow}_mono` and every lane of the poly versions |
//! | `power_scale` | `futils::power_scale_mono` |
//! | `tanh` | `futils::tanh_mono` |
//! | `poly_floor`, `round_to_int`, `pow2_to_float` | `poly_utils::{floor,round_to_int,pow2_to_float}`, every lane |
//! | `min`, `max`, `clamp`, `iclamp`, `ilog2`, `interpolate_f64` | `utils::*` |
//! | `interpolate` | `utils::interpolate` and every lane of `poly_utils::interpolate` |
//! | `catmull_weights`, `catmull_interpolate` | `get_catmull_interpolation_matrix` rows, and its `multiply_and_sum_rows` with the transposed `get_value_matrix` |
//! | `PI` | `common::PI` |
//!
//! Where upstream's arithmetic differs, the copies are *expected* to differ,
//! and `upstream_differs_*` pins one input that shows it:
//!
//! - `tanh`: upstream's `mono_float` overload and its `poly_float` overload
//!   evaluate the rational in a different order.
//! - `power_scale`: the `mono_float` overload returns `value` for
//!   `|power| < 0.01`, the `poly_float` one for `|power| < 0.005`.
//! - `min`/`max`: `utils.h` uses `fmin`/`fmax`, and `poly_float::min`/`max`
//!   use `MINPS`/`MAXPS` or `FMIN`/`FMAX` (reference-arm.md A5). They differ
//!   on NaN, and on SSE2 also on a `±0` tie. `poly_utils::clamp` builds on the
//!   poly versions.
//! - The rounding helpers (`poly_floor`, `round_to_int`, and through them
//!   `exp2`, `exp`, `pow`, `power_scale`) for NaN and `|x| >= 2^31`.
//!   `sas-common` implements only the SSE2 column of `toInt` (A1).
//!   Upstream's NEON column gives a different floor there (for example,
//!   `exp2(2^31)` is 0.5 instead of +inf), so under `aarch64-semantics` the
//!   comparison covers the range where the columns agree.
//!
//! Only in `sas-common`: `float_to_pcm`, `pcm_to_float`. Only here:
//! `quick_tanh`, the saturators, `sin`/`sin1`, the dB and pitch conversions,
//! and `exp_half`. Nothing outside processors calls these.

use sas_common::scalar as s;
use sas_simd::{common, futils as f, poly_utils as u, utils as mu, PolyFloat as P, PolyInt};

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

/// Whether the SSE2 and NEON columns of `toInt` give the same floor for
/// `round_to_int(x)`'s argument. On the SSE2 column every input compares.
fn rounds_alike(x: f32) -> bool {
    !sas_simd::AARCH64_SEMANTICS
        || (!x.is_nan() && (x + 0.5).abs() < TWO_POW_31 && x.abs() < TWO_POW_31)
}

#[track_caller]
fn assert_same(name: &str, input: impl std::fmt::Debug, common: f32, simd: f32) {
    assert_eq!(
        common.to_bits(),
        simd.to_bits(),
        "{name}({input:?}): sas-common {common:e} vs sas-simd {simd:e}"
    );
}

/// Runs `simd` on four consecutive inputs at once, so every lane is checked.
fn lanes(inputs: &[f32], simd: impl Fn(P) -> P) -> Vec<f32> {
    let mut out = Vec::with_capacity(inputs.len());
    for chunk in inputs.chunks(4) {
        let mut lanes = [0.0; 4];
        lanes[..chunk.len()].copy_from_slice(chunk);
        let result = simd(P::from(lanes)).to_array();
        out.extend_from_slice(&result[..chunk.len()]);
    }
    out
}

fn check_unary(
    name: &str,
    inputs: &[f32],
    common: impl Fn(f32) -> f32,
    mono: impl Fn(f32) -> f32,
    poly: Option<&dyn Fn(P) -> P>,
) {
    let poly_out = poly.map(|p| lanes(inputs, p));
    for (i, &x) in inputs.iter().enumerate() {
        let expected = common(x);
        assert_same(name, x, expected, mono(x));
        if let Some(out) = &poly_out {
            assert_same(&format!("{name} (poly lane)"), x, expected, out[i]);
        }
    }
}

#[test]
fn exp2_and_log2() {
    let all = floats();
    let rounding: Vec<f32> = all.iter().copied().filter(|&x| rounds_alike(x)).collect();
    check_unary("exp2", &rounding, s::exp2, f::exp2_mono, Some(&f::exp2));
    check_unary("log2", &all, s::log2, f::log2_mono, Some(&f::log2));
}

#[test]
fn exp_log_pow() {
    let all = floats();
    let exp_inputs: Vec<f32> = all
        .iter()
        .copied()
        .filter(|&x| rounds_alike(x * mu::INV_LOG_OF_2))
        .collect();
    check_unary("exp", &exp_inputs, s::exp, f::exp_mono, Some(&f::exp));
    check_unary("log", &all, s::log, f::log_mono, Some(&f::log));

    for (base, exponent) in float_pairs() {
        if !rounds_alike(s::log2(base) * exponent) {
            continue;
        }
        let expected = s::pow(base, exponent);
        assert_same(
            "pow",
            (base, exponent),
            expected,
            f::pow_mono(base, exponent),
        );
        let poly = f::pow(P::splat(base), P::splat(exponent)).get(0);
        assert_same("pow (poly)", (base, exponent), expected, poly);
    }
}

#[test]
fn power_scale_mono_overload() {
    let mut pairs = float_pairs();
    for value in [-1.0, 0.0, 0.25, 0.5, 1.0] {
        for power in [
            -20.0, -4.0, -0.01, -0.009_999, 0.0, 0.005, 0.009_999, 0.01, 4.0, 20.0,
        ] {
            pairs.push((value, power));
        }
    }
    for (value, power) in pairs {
        if !rounds_alike(power * value * mu::INV_LOG_OF_2)
            || !rounds_alike(power * mu::INV_LOG_OF_2)
        {
            continue;
        }
        assert_same(
            "power_scale",
            (value, power),
            s::power_scale(value, power),
            f::power_scale_mono(value, power),
        );
    }
}

#[test]
fn tanh_mono_overload() {
    check_unary("tanh", &floats(), s::tanh, f::tanh_mono, None);
}

#[test]
fn floor_round_and_pow2() {
    let rounding: Vec<f32> = floats().into_iter().filter(|&x| rounds_alike(x)).collect();
    check_unary(
        "poly_floor",
        &rounding,
        s::poly_floor,
        |x| u::floor(P::splat(x)).get(0),
        Some(&u::floor),
    );
    let rounded = lanes(&rounding, |p| u::to_float(u::round_to_int(p)));
    for (&x, &simd) in rounding.iter().zip(&rounded) {
        assert_same("round_to_int", x, s::round_to_int(x) as f32, simd);
    }

    let mut state = 0x0bad_5eed;
    let mut ints: Vec<i32> = (-300..=300).collect();
    ints.extend([i32::MIN, i32::MAX, -127, -126, 127, 128]);
    ints.extend((0..100_000).map(|_| lcg(&mut state) as i32));
    for value in ints {
        let simd = u::pow2_to_float(PolyInt::splat(value as u32)).get(0);
        assert_same("pow2_to_float", value, s::pow2_to_float(value), simd);
    }
}

#[test]
fn scalar_utils() {
    for (a, b) in float_pairs() {
        assert_same("min", (a, b), s::min(a, b), mu::min(a, b));
        assert_same("max", (a, b), s::max(a, b), mu::max(a, b));
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        for value in [a, b, 0.0, -0.0, 1.0, f32::NAN] {
            assert_same(
                "clamp",
                (value, low, high),
                s::clamp(value, low, high),
                mu::clamp(value, low, high),
            );
        }
    }

    let mut state = 0x5eed_1234;
    let mut ints: Vec<i32> = (-1100..=1100).collect();
    ints.extend([i32::MIN, i32::MAX, 1 << 30, (1 << 30) - 1]);
    ints.extend((0..50_000).map(|_| lcg(&mut state) as i32));
    for &value in &ints {
        assert_eq!(s::ilog2(value), mu::ilog2(value), "ilog2({value})");
        for (low, high) in [(0, 0), (-7, 7), (0, 1023), (i32::MIN, i32::MAX), (5, 100)] {
            assert_eq!(
                s::iclamp(value, low, high),
                mu::iclamp(value, low, high),
                "iclamp({value}, {low}, {high})"
            );
        }
    }
}

#[test]
fn interpolate_all_overloads() {
    let ts = floats();
    for ((from, to), &t) in float_pairs().into_iter().zip(ts.iter().rev()) {
        let expected = s::interpolate(from, to, t);
        assert_same(
            "interpolate",
            (from, to, t),
            expected,
            mu::interpolate(from, to, t),
        );
        let poly = u::interpolate(from, to, t).get(0);
        assert_same("interpolate (poly)", (from, to, t), expected, poly);

        let (from, to, t) = (f64::from(from) * 1.1, f64::from(to) / 3.0, f64::from(t));
        let expected = s::interpolate_f64(from, to, t);
        let simd = mu::interpolate_f64(from, to, t);
        assert!(
            expected.to_bits() == simd.to_bits(),
            "interpolate_f64({from}, {to}, {t}): {expected:e} vs {simd:e}"
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

#[test]
fn pi() {
    assert_eq!(s::PI.to_bits(), common::PI.to_bits());
}

#[test]
fn upstream_differs_tanh_overloads() {
    let differs = floats()
        .into_iter()
        .filter(|x| x.is_finite())
        .any(|x| s::tanh(x).to_bits() != f::tanh(P::splat(x)).get(0).to_bits());
    assert!(
        differs,
        "the mono and poly tanh overloads no longer differ anywhere"
    );
}

#[test]
fn upstream_differs_power_scale_overloads() {
    let (value, power) = (0.25, 0.007);
    assert_eq!(s::power_scale(value, power), value);
    let poly = f::power_scale(P::splat(value), P::splat(power)).get(0);
    assert_ne!(poly.to_bits(), value.to_bits());
}

#[test]
fn upstream_differs_min_max() {
    let nan = f32::NAN;
    assert_eq!(s::min(1.0, nan), 1.0);
    assert_eq!(s::max(1.0, nan), 1.0);
    assert!(P::min(P::splat(1.0), P::splat(nan)).get(0).is_nan());
    assert!(P::max(P::splat(1.0), P::splat(nan)).get(0).is_nan());
}

#[test]
fn upstream_differs_rounding_out_of_range() {
    let nan_floor = u::floor(P::splat(f32::NAN)).get(0);
    let huge_exp2 = f::exp2_mono(TWO_POW_31);
    if sas_simd::AARCH64_SEMANTICS {
        assert_eq!(nan_floor, 0.0);
        assert_ne!(s::poly_floor(f32::NAN), nan_floor);
        assert_eq!(huge_exp2, 0.5);
        assert_eq!(s::exp2(TWO_POW_31), f32::INFINITY);
    } else {
        assert_eq!(s::poly_floor(f32::NAN).to_bits(), nan_floor.to_bits());
        assert_eq!(s::exp2(TWO_POW_31).to_bits(), huge_exp2.to_bits());
    }
}
