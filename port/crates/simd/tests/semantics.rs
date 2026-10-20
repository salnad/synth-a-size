// SPDX-License-Identifier: GPL-3.0-or-later

//! Pins the documented SSE2 and NEON lane semantics on the scalar models, so
//! both are checked on every host (the native backends are then checked
//! against their model in `backend_parity.rs`).

use sas_simd::backend::{Backend, Scalar, ScalarNeon};
use sas_simd::{PolyFloatOf, PolyIntOf};

type Sse = PolyFloatOf<Scalar>;
type Arm = PolyFloatOf<ScalarNeon>;
type SseI = PolyIntOf<Scalar>;
type ArmI = PolyIntOf<ScalarNeon>;

const NAN: f32 = f32::NAN;

fn bits(v: [f32; 4]) -> [u32; 4] {
    v.map(f32::to_bits)
}

#[test]
fn float_max_min_nan_and_zero_ties() {
    let a = [NAN, 1.0, 0.0, -0.0];
    let b = [1.0, NAN, -0.0, 0.0];

    // SSE2 MAXPS/MINPS return the second operand on NaN or equality.
    let sse_max = Sse::max(Sse::from_array(a), Sse::from_array(b)).to_array();
    assert_eq!(bits(sse_max), bits([1.0, NAN, -0.0, 0.0]));
    let sse_min = Sse::min(Sse::from_array(a), Sse::from_array(b)).to_array();
    assert_eq!(bits(sse_min), bits([1.0, NAN, -0.0, 0.0]));

    // NEON FMAX/FMIN propagate NaN and order the zeros.
    let arm_max = Arm::max(Arm::from_array(a), Arm::from_array(b)).to_array();
    assert!(arm_max[0].is_nan() && arm_max[1].is_nan());
    assert_eq!(arm_max[2].to_bits(), 0.0f32.to_bits());
    assert_eq!(arm_max[3].to_bits(), 0.0f32.to_bits());
    let arm_min = Arm::min(Arm::from_array(a), Arm::from_array(b)).to_array();
    assert!(arm_min[0].is_nan() && arm_min[1].is_nan());
    assert_eq!(arm_min[2].to_bits(), (-0.0f32).to_bits());
    assert_eq!(arm_min[3].to_bits(), (-0.0f32).to_bits());
}

#[test]
fn neon_max_prefers_signaling_nan() {
    let quiet = f32::from_bits(0x7fc0_0001);
    let signaling = f32::from_bits(0x7f80_0002);
    let got = Arm::max(Arm::splat(quiet), Arm::splat(signaling)).get(0);
    assert_eq!(got.to_bits(), 0x7fc0_0002);
}

#[test]
fn not_equal_on_nan() {
    let n = Sse::splat(NAN);
    assert_eq!(Sse::not_equal(n, n).get(0), u32::MAX);
    let n = Arm::splat(NAN);
    assert_eq!(Arm::not_equal(n, n).get(0), 0);
    // Ordered predicates agree.
    assert_eq!(Sse::equal(Sse::splat(NAN), Sse::splat(NAN)).get(0), 0);
    assert_eq!(
        Arm::greater_than(Arm::splat(NAN), Arm::splat(0.0)).get(0),
        0
    );
}

#[test]
fn horizontal_sum_order() {
    // Chosen so the two association orders round differently.
    let v = [1.0e8, 1.0, -1.0e8, 1.0];
    assert_eq!(Sse::from_array(v).sum(), (v[0] + v[2]) + (v[1] + v[3]));
    assert_eq!(Arm::from_array(v).sum(), (v[0] + v[1]) + (v[2] + v[3]));
    assert_ne!(Sse::from_array(v).sum(), Arm::from_array(v).sum());
}

#[test]
fn float_to_int_conversion() {
    let v = [2.5, -2.5, 3.7, -3.7];
    assert_eq!(Scalar::f_to_i(v), [2, -2i32 as u32, 4, -4i32 as u32]);
    assert_eq!(ScalarNeon::f_to_i(v), [2, -2i32 as u32, 3, -3i32 as u32]);

    let edge = [NAN, 3.0e9, -3.0e9, f32::INFINITY];
    assert_eq!(Scalar::f_to_i(edge), [0x8000_0000; 4]);
    assert_eq!(
        ScalarNeon::f_to_i(edge),
        [0, 0x7fff_ffff, 0x8000_0000, 0x7fff_ffff]
    );
}

#[test]
fn int_min_signedness() {
    let a = SseI::splat(0xffff_ffff);
    let b = SseI::splat(1);
    assert_eq!(SseI::min(a, b).get(0), 0xffff_ffff, "SSE2 min is signed");
    assert_eq!(SseI::max(a, b).get(0), 0xffff_ffff, "SSE2 max is unsigned");
    let a = ArmI::splat(0xffff_ffff);
    let b = ArmI::splat(1);
    assert_eq!(ArmI::min(a, b).get(0), 1, "NEON min is unsigned");
    assert_eq!(ArmI::max(a, b).get(0), 0xffff_ffff);
}

#[test]
fn any_mask_values() {
    let m = [0, 0xffff_ffff, 0, 0x8000_0000];
    assert_eq!(SseI::from_array(m).any_mask(), 0x80f0);
    assert_eq!(ArmI::from_array(m).any_mask(), 0xffff_ffff);
    assert_eq!(SseI::zero().any_mask(), 0);
    assert_eq!(ArmI::zero().any_mask(), 0);
}

#[test]
fn negation_flips_sign_bit() {
    for v in [0.0, -0.0, 1.5, f32::INFINITY] {
        assert_eq!((-Sse::splat(v)).get(0).to_bits(), v.to_bits() ^ 0x8000_0000);
        assert_eq!((-Arm::splat(v)).get(0).to_bits(), v.to_bits() ^ 0x8000_0000);
    }
}

#[test]
fn mul_add_rounds_twice() {
    // a + b * c where the fused result differs from the two-rounding one.
    let a = -1.0f32;
    let b = 1.0 + f32::EPSILON;
    let c = 1.0 - f32::EPSILON;
    let two_roundings = a + b * c;
    assert_ne!(two_roundings, b.mul_add(c, a));
    let got = Sse::mul_add(Sse::splat(a), Sse::splat(b), Sse::splat(c)).get(0);
    assert_eq!(got.to_bits(), two_roundings.to_bits());
    let got = Arm::mul_add(Arm::splat(a), Arm::splat(b), Arm::splat(c)).get(0);
    assert_eq!(got.to_bits(), two_roundings.to_bits());
}

#[test]
fn constructors_and_lanes() {
    let v = Sse::stereo(1.0, 2.0);
    assert_eq!(v.to_array(), [1.0, 2.0, 1.0, 2.0]);
    let mut w = Sse::zero();
    w.set(2, 5.0);
    assert_eq!(w.to_array(), [0.0, 0.0, 5.0, 0.0]);
    assert_eq!(SseI::stereo(7, 9).to_array(), [7, 9, 7, 9]);
    assert_eq!(Sse::load(&[1.0, 2.0, 3.0, 4.0, 5.0]).get(3), 4.0);
}
