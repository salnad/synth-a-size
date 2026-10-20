// SPDX-License-Identifier: GPL-3.0-or-later

//! Mono (`_mono`) vs poly overloads of `futils`.
//!
//! Where upstream's two overloads are the same arithmetic, the mono result must
//! equal every lane of the poly result bit for bit. Where upstream's formulas
//! differ, each overload follows its own upstream lines and the test only
//! documents (and checks) that they differ. Line numbers refer to
//! `vital/src/synthesis/framework/futils.h`.
//!
//! Upstream differs:
//!
//! | Function | Mono lines | Poly lines | Difference |
//! |---|---|---|---|
//! | `db_to_magnitude` | 168–170 | 172–174 | mono is `exp(x / 6.02)` (natural exp, extra `log2(e)` factor); poly is `exp2(x / 6.02)` |
//! | `quick_tanh_derivative` | 199–206 | 190–197 | numerator summed in a different order |
//! | `algebraic_sat_derivative` | 243–250 | 252–260 | numerator summed in a different order |
//! | `tanh` | 275–284 | 262–273 | numerator and denominator associated differently |
//! | `power_scale` | 360–369 | 371–378 | bypass threshold `0.01` vs `0.005`; poly bypass goes through `maskLoad` (`-0` → `+0`) |
//!
//! Identical arithmetic (checked bit for bit): `exp2`, `log2`, `exp`, `log`,
//! `exp_half`, `pow`, `magnitude_to_db`, `quick_tanh`, `algebraic_sat`,
//! `quick_sin`, `sin`, `quick_sin1`, `sin1`.

// Upstream literals are kept digit for digit.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

mod common;

use common::XorShift;
use sas_simd::{futils, PolyFloat};

fn inputs() -> Vec<f32> {
    let mut v = vec![
        0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 2.0, 1e-3, -1e-3, 0.007, -0.007,
    ];
    let mut rng = XorShift(4242);
    for _ in 0..4000 {
        v.push(rng.unit() * 16.0 - 8.0);
    }
    v
}

fn assert_lane_equal(name: &str, mono: impl Fn(f32) -> f32, poly: impl Fn(PolyFloat) -> PolyFloat) {
    for x in inputs() {
        let m = mono(x);
        for lane in poly(PolyFloat::splat(x)).to_array() {
            assert!(
                m.to_bits() == lane.to_bits() || (m.is_nan() && lane.is_nan()),
                "{name}({x}): mono {m} poly {lane}"
            );
        }
    }
}

fn count_differences(mono: impl Fn(f32) -> f32, poly: impl Fn(PolyFloat) -> PolyFloat) -> usize {
    inputs()
        .into_iter()
        .filter(|&x| mono(x).to_bits() != poly(PolyFloat::splat(x)).get(0).to_bits())
        .count()
}

#[test]
fn identical_overloads_match_bit_for_bit() {
    assert_lane_equal("exp2", futils::exp2_mono, futils::exp2);
    assert_lane_equal(
        "log2",
        |x| futils::log2_mono(x.abs() + 0.1),
        |v| futils::log2(PolyFloat::abs(v) + 0.1),
    );
    assert_lane_equal("exp", futils::exp_mono, futils::exp);
    assert_lane_equal(
        "log",
        |x| futils::log_mono(x.abs() + 0.1),
        |v| futils::log(PolyFloat::abs(v) + 0.1),
    );
    assert_lane_equal("exp_half", futils::exp_half_mono, futils::exp_half);
    assert_lane_equal(
        "pow",
        |x| futils::pow_mono(x.abs() + 0.1, x * 0.5),
        |v| futils::pow(PolyFloat::abs(v) + 0.1, v * 0.5),
    );
    assert_lane_equal(
        "magnitude_to_db",
        |x| futils::magnitude_to_db_mono(x.abs() + 0.1),
        |v| futils::magnitude_to_db(PolyFloat::abs(v) + 0.1),
    );
    assert_lane_equal("quick_tanh", futils::quick_tanh_mono, futils::quick_tanh);
    assert_lane_equal(
        "algebraic_sat",
        futils::algebraic_sat_mono,
        futils::algebraic_sat,
    );
    assert_lane_equal("quick_sin", futils::quick_sin_mono, futils::quick_sin);
    assert_lane_equal("sin", futils::sin_mono, futils::sin);
    assert_lane_equal("quick_sin1", futils::quick_sin1_mono, futils::quick_sin1);
    assert_lane_equal("sin1", futils::sin1_mono, futils::sin1);
}

#[test]
fn upstream_different_overloads_stay_different() {
    let differs = [
        (
            "db_to_magnitude",
            count_differences(futils::db_to_magnitude_mono, futils::db_to_magnitude),
        ),
        (
            "quick_tanh_derivative",
            count_differences(
                futils::quick_tanh_derivative_mono,
                futils::quick_tanh_derivative,
            ),
        ),
        (
            "algebraic_sat_derivative",
            count_differences(
                futils::algebraic_sat_derivative_mono,
                futils::algebraic_sat_derivative,
            ),
        ),
        ("tanh", count_differences(futils::tanh_mono, futils::tanh)),
        (
            "power_scale",
            count_differences(
                |x| futils::power_scale_mono(0.3, x * 0.002),
                |v| futils::power_scale(PolyFloat::splat(0.3), v * 0.002),
            ),
        ),
    ];
    for (name, count) in differs {
        assert!(
            count > 0,
            "{name}: upstream's overloads differ, but the port's agree"
        );
    }
}

#[test]
fn db_to_magnitude_scale_bug_is_kept() {
    // Poly: 2^(6.02 / 6.02) = 2. Mono: e^(6.02 / 6.02 * log2(e))... = 2^(log2 e).
    let poly = futils::db_to_magnitude(PolyFloat::splat(6.020_599_913_29)).get(0);
    let mono = futils::db_to_magnitude_mono(6.020_599_913_29);
    assert!((poly - 2.0).abs() < 1e-4, "{poly}");
    assert!((mono - std::f32::consts::E).abs() < 1e-3, "{mono}");
}
