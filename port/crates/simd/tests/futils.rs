// SPDX-License-Identifier: GPL-3.0-or-later

//! Every poly `futils` function against a lane-wise `f32` transcription of
//! its upstream formula (same operations, same order). IEEE `f32` arithmetic
//! is exact-rounded on every backend, so the results must be bit-identical.

// Upstream literals are kept digit for digit.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

mod common;

use common::{quads, special_floats, XorShift};
use sas_simd::{futils, PolyFloat};

fn same(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

fn inputs() -> Vec<f32> {
    let mut v: Vec<f32> = special_floats()
        .into_iter()
        .filter(|x| x.is_finite() && x.abs() < 1.0e6)
        .collect();
    let mut rng = XorShift(77);
    for _ in 0..2000 {
        v.push(rng.unit() * 40.0 - 20.0);
    }
    for _ in 0..500 {
        v.push(rng.unit() * 2.0 - 1.0);
    }
    v
}

fn check1(name: &str, poly: impl Fn(PolyFloat) -> PolyFloat, lane: impl Fn(f32) -> f32) {
    for q in quads(&inputs()) {
        let got = poly(PolyFloat::from_array(q)).to_array();
        for i in 0..4 {
            let want = lane(q[i]);
            assert!(
                same(got[i], want),
                "{name}({}) = {} want {}",
                q[i],
                got[i],
                want
            );
        }
    }
}

fn check2(
    name: &str,
    poly: impl Fn(PolyFloat, PolyFloat) -> PolyFloat,
    lane: impl Fn(f32, f32) -> f32,
) {
    let values = inputs();
    let q = quads(&values);
    for (n, a) in q.iter().enumerate() {
        let b = q[(n * 7 + 3) % q.len()];
        let got = poly(PolyFloat::from_array(*a), PolyFloat::from_array(b)).to_array();
        for i in 0..4 {
            let want = lane(a[i], b[i]);
            assert!(
                same(got[i], want),
                "{name}({}, {}) = {} want {}",
                a[i],
                b[i],
                got[i],
                want
            );
        }
    }
}

/// `utils::roundToInt` for values whose floor fits in an i32.
fn round_to_int(x: f32) -> i32 {
    (x + 0.5).floor() as i32
}

fn pow2(n: i32) -> f32 {
    f32::from_bits(((n as u32).wrapping_add(127)) << 23)
}

fn exp2_lane(x: f32) -> f32 {
    let c1 = (16970.0f64 / 24483.0) as f32;
    let c2 = (1960.0f64 / 8161.0) as f32;
    let c3 = (1360.0f64 / 24483.0) as f32;
    let c4 = (80.0f64 / 8161.0) as f32;
    let c5 = (32.0f64 / 24483.0) as f32;
    let n = round_to_int(x);
    let t = x - n as f32;
    let cubic = t * (t * (t * c5 + c4) + c3) + c2;
    let interpolate = t * (t * cubic + c1) + 1.0;
    pow2(n) * interpolate
}

fn log2_lane(x: f32) -> f32 {
    let c0 = (-1819.0f64 / 651.0) as f32;
    let c2 = (-10.0f64 / 3.0) as f32;
    let c3 = (10.0f64 / 7.0) as f32;
    let c4 = (-1.0f64 / 3.0) as f32;
    let c5 = (1.0f64 / 31.0) as f32;
    let floored = ((x.to_bits() >> 23).wrapping_sub(0x7f)) as i32 as f32;
    let t = f32::from_bits((x.to_bits() & 0x7f_ffff) | (0x7f << 23));
    let cubic = t * (t * (t * c5 + c4) + c3) + c2;
    let interpolate = t * (t * cubic + 5.0) + c0;
    floored + interpolate
}

fn tanh_lane(v: f32) -> f32 {
    let a = v.abs();
    let sq = v * v;
    let part_num1 = a * 0.821_226_666_969_744 + 0.893_229_853_513_558;
    let part_num2 = sq * part_num1 + 2.455_507_507_029_56;
    let num = v * (a * 2.455_507_507_029_56 + part_num2);
    let part_den = (a * 0.814_642_734_961_073 * v + v).abs();
    let den = part_den * (sq + 2.445_066_346_522_99) + 2.445_066_346_522_99;
    num / den
}

fn sin1_lane(p: f32) -> f32 {
    let adj = 0.5 - p;
    let approx = adj * (8.0 + adj.abs() * -16.0);
    approx * (0.776 + approx.abs() * 0.224)
}

#[test]
fn exp_log_family() {
    // Keep exp2 inputs where 2^n is a normal float, as upstream assumes.
    let in_range = |f: fn(f32) -> f32| move |x: f32| f(x.clamp(-100.0, 100.0));
    check1(
        "exp2",
        |v| futils::exp2(sas_simd::poly_utils::clamp(v, -100.0, 100.0)),
        in_range(exp2_lane),
    );
    check1("log2", futils::log2, log2_lane);
    check1(
        "cheap_exp2",
        |v| futils::cheap_exp2(sas_simd::poly_utils::clamp(v, -100.0, 100.0)),
        in_range(|x| {
            let n = round_to_int(x);
            let t = x - n as f32;
            pow2(n) * (t * (t * (4.0f64 / 17.0) as f32 + (12.0f64 / 17.0) as f32) + 1.0)
        }),
    );
    check1("cheap_log2", futils::cheap_log2, |x| {
        let floored = ((x.to_bits() >> 23).wrapping_sub(0x7f)) as i32 as f32;
        let t = f32::from_bits((x.to_bits() & 0x7f_ffff) | (0x7f << 23));
        floored + (t * (t * (-1.0f64 / 3.0) as f32 + 2.0) + (-5.0f64 / 3.0) as f32)
    });
    check1(
        "exp",
        |v| futils::exp(sas_simd::poly_utils::clamp(v, -60.0, 60.0)),
        |x| exp2_lane(x.clamp(-60.0, 60.0) * 1.442_695_040_89),
    );
    check1("log", futils::log, |x| log2_lane(x) * 0.693_147_180_56);
    check1(
        "exp_half",
        |v| futils::exp_half(sas_simd::poly_utils::clamp(v, -100.0, 100.0)),
        |x| exp2_lane(-x.clamp(-100.0, 100.0)),
    );
    check1(
        "db_to_magnitude",
        |v| futils::db_to_magnitude(sas_simd::poly_utils::clamp(v, -200.0, 200.0)),
        |x| exp2_lane(x.clamp(-200.0, 200.0) * (1.0 / 6.020_599_913_29)),
    );
    check1("magnitude_to_db", futils::magnitude_to_db, |x| {
        log2_lane(x) * 6.020_599_913_29
    });
    check1(
        "midi_note_to_frequency",
        |v| futils::midi_note_to_frequency(sas_simd::poly_utils::clamp(v, -500.0, 500.0)),
        |x| exp2_lane(x.clamp(-500.0, 500.0) * (1.0 / 12.0)) * 8.175_798_915_6,
    );
    check2(
        "pow",
        |b, e| futils::pow(PolyFloat::abs(b) + 0.01, e * 0.25),
        |b, e| exp2_lane(log2_lane(b.abs() + 0.01) * (e * 0.25)),
    );
}

#[test]
fn saturators() {
    check1("quick_tanh", futils::quick_tanh, |v| {
        let sq = v * v;
        v / (sq / (3.0 + sq * 0.2) + 1.0)
    });
    check1(
        "quick_tanh_derivative",
        futils::quick_tanh_derivative,
        |v| {
            let sq = v * v;
            let fourth = sq * sq;
            let part_den = sq + 2.5;
            let num = (6.25 + fourth * 0.166_667) + sq * -1.25;
            num / (part_den * part_den)
        },
    );
    check1("algebraic_sat", futils::algebraic_sat, |v| {
        let sq = v * v;
        v * sq * -0.9 / (sq + 3.0) + v
    });
    check1("quadratic_inv_sat", futils::quadratic_inv_sat, |v| {
        v / (v * v * 0.25 + 1.0)
    });
    check1("bump_sat", futils::bump_sat, |v| {
        let sq = v * v;
        v / (sq * sq * 0.1 + 1.0)
    });
    check1("bump_sat2", futils::bump_sat2, |v| {
        let sq = v * v;
        (v + sq * v * 3.0) / (sq * sq * 20.0 + 1.0)
    });
    check1(
        "algebraic_sat_derivative",
        futils::algebraic_sat_derivative,
        |v| {
            let sq = v * v;
            let num = sq * sq * 0.1 + (sq * -2.1 + 9.0);
            let part_den = sq + 3.0;
            num / (part_den * part_den)
        },
    );
    check1("tanh", futils::tanh, tanh_lane);
    check1("hard_tanh", futils::hard_tanh, |v| {
        let inv_rec = 1.0 / (1.0f32 - 0.66);
        let clamped = if v < 0.66 { v } else { 0.66 };
        let clamped = if clamped > -0.66 { clamped } else { -0.66 };
        clamped + tanh_lane((v - clamped) * inv_rec) * (1.0 - 0.66)
    });
    check1("tanh_derivative_fast", futils::tanh_derivative_fast, |v| {
        1.0 / (2.0 + v * v * 1.8)
    });
}

#[test]
fn sines_and_fades() {
    check1("quick_sin", futils::quick_sin, |p| {
        p * (8.0 + p.abs() * -16.0)
    });
    check1("sin", futils::sin, |p| {
        let approx = p * (8.0 + p.abs() * -16.0);
        approx * (0.776 + approx.abs() * 0.224)
    });
    check1("quick_sin1", futils::quick_sin1, |p| {
        let adj = 0.5 - p;
        adj * (8.0 + adj.abs() * -16.0)
    });
    check1("sin1", futils::sin1, sin1_lane);
    check1("equal_power_fade", futils::equal_power_fade, |t| {
        sin1_lane(t * 0.25)
    });
    check1(
        "equal_power_fade_inverse",
        futils::equal_power_fade_inverse,
        |t| sin1_lane((t + 1.0) * 0.25),
    );

    for q in quads(&inputs()) {
        let got = futils::pan_amplitude(PolyFloat::from_array(q)).to_array();
        for i in 0..4 {
            let split = if i % 2 == 0 { 1.0 } else { -1.0 };
            let want = sin1_lane(0.125 - split * q[i] * 0.125) * 1.414_213_562_37;
            assert!(same(got[i], want), "pan_amplitude({})", q[i]);
        }
    }

    check2(
        "sin_interpolate",
        |t, to| futils::sin_interpolate(PolyFloat::splat(0.25), to, t),
        |t, to| {
            let s = t * 0.5 - 0.25;
            let approx = s * (8.0 + s.abs() * -16.0);
            let sin_value = approx * (0.776 + approx.abs() * 0.224);
            let sin_t = sin_value * 0.5 + 0.5;
            0.25 + (to - 0.25) * sin_t
        },
    );
}

#[test]
fn power_scale_masks_small_powers() {
    check2(
        "power_scale",
        |v, p| futils::power_scale(v * 0.05, sas_simd::poly_utils::clamp(p, -20.0, 20.0)),
        |v, p| {
            let v = v * 0.05;
            let p = p.clamp(-20.0, 20.0);
            // `maskLoad` adds the two masked halves, so `-0` becomes `+0`.
            if p < 0.005 && -p < 0.005 {
                0.0 + v
            } else {
                let e = |x: f32| exp2_lane(x * 1.442_695_040_89);
                (e(p * v) - 1.0) / (e(p) - 1.0) + 0.0
            }
        },
    );
}

/// Sanity bounds on upstream's approximations. These pin the polynomials
/// (a wrong coefficient shows up as a large error); they are not tolerances
/// against the reference.
#[test]
fn approximation_accuracy() {
    let mut worst_exp2 = 0.0f64;
    let mut worst_log2 = 0.0f64;
    let mut worst_tanh = 0.0f64;
    let mut x = -10.0f32;
    while x < 10.0 {
        let e = futils::exp2_mono(x) as f64;
        worst_exp2 = worst_exp2.max((e / 2f64.powf(x as f64) - 1.0).abs());
        let y = 2f32.powf(x);
        worst_log2 = worst_log2.max((futils::log2_mono(y) as f64 - x as f64).abs());
        let t = futils::tanh(PolyFloat::splat(x)).get(0) as f64;
        worst_tanh = worst_tanh.max((t - (x as f64).tanh()).abs());
        x += 0.001_3;
    }
    assert!(worst_exp2 < 1.0e-5, "exp2 relative error {worst_exp2}");
    assert!(worst_log2 < 1.0e-3, "log2 absolute error {worst_log2}");
    assert!(worst_tanh < 1.0e-3, "tanh absolute error {worst_tanh}");
}
