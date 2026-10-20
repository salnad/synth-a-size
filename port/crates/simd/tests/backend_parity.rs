// SPDX-License-Identifier: GPL-3.0-or-later

//! The native backend (SSE2 on x86_64, NEON on aarch64) must produce the same
//! bits as the scalar model of its upstream path (`backend::Model`) for every
//! operation. On aarch64 this is what exercises the NEON backend.

mod common;

use common::{quad_pairs, quads, special_floats, special_ints};
use sas_simd::backend::{Backend, Model, Native};
use sas_simd::{PolyFloatOf, PolyIntOf};

/// Every float operation of `PolyFloatOf`, flattened to bit patterns.
fn float_ops<B: Backend>(a: [f32; 4], b: [f32; 4], c: [f32; 4]) -> Vec<(&'static str, Vec<u32>)> {
    let x = PolyFloatOf::<B>::from_array(a);
    let y = PolyFloatOf::<B>::from_array(b);
    let z = PolyFloatOf::<B>::from_array(c);
    let f = |v: PolyFloatOf<B>| v.to_array().map(f32::to_bits).to_vec();
    // Which payload survives when both operands of `+ - * /` are NaN depends on
    // operand order, and compilers may commute these operations (in the port
    // and in the reference alike). Compare NaN-ness there, not payload.
    let arith = |v: PolyFloatOf<B>| {
        v.to_array()
            .map(|x| if x.is_nan() { 0x7fc0_0000 } else { x.to_bits() })
            .to_vec()
    };
    let m = |v: PolyIntOf<B>| v.to_array().to_vec();
    let mut transposed = (x, y, z, -z);
    PolyFloatOf::<B>::transpose(
        &mut transposed.0,
        &mut transposed.1,
        &mut transposed.2,
        &mut transposed.3,
    );

    vec![
        ("add", arith(x + y)),
        ("sub", arith(x - y)),
        ("mul", arith(x * y)),
        ("div", arith(x / y)),
        ("add_scalar", arith(x + b[1])),
        ("scalar_sub", arith(a[2] - y)),
        ("mul_scalar", arith(x * b[0])),
        ("div_scalar", arith(x / b[3])),
        ("neg", f(-x)),
        ("not", f(!x)),
        ("mul_add", arith(PolyFloatOf::mul_add(x, y, z))),
        ("mul_sub", arith(PolyFloatOf::mul_sub(x, y, z))),
        ("max", f(PolyFloatOf::max(x, y))),
        ("min", f(PolyFloatOf::min(x, y))),
        ("abs", f(PolyFloatOf::abs(x))),
        ("sign_mask", m(PolyFloatOf::sign_mask(x))),
        ("equal", m(PolyFloatOf::equal(x, y))),
        ("equal_self", m(PolyFloatOf::equal(x, x))),
        ("not_equal", m(PolyFloatOf::not_equal(x, y))),
        ("not_equal_self", m(PolyFloatOf::not_equal(x, x))),
        ("greater_than", m(PolyFloatOf::greater_than(x, y))),
        (
            "greater_than_or_equal",
            m(PolyFloatOf::greater_than_or_equal(x, y)),
        ),
        ("less_than", m(PolyFloatOf::less_than(x, y))),
        (
            "less_than_or_equal",
            m(PolyFloatOf::less_than_or_equal(x, y)),
        ),
        ("sum", arith(PolyFloatOf::splat(x.sum()))),
        ("and", f(x & PolyFloatOf::greater_than(y, z))),
        ("or", f(x | PolyIntOf::<B>::from_array(c.map(f32::to_bits)))),
        (
            "xor",
            f(x ^ PolyIntOf::<B>::from_array(b.map(f32::to_bits))),
        ),
        ("sqrt", f(PolyFloatOf::from_raw(B::f_sqrt(x.raw())))),
        ("to_int", m(PolyIntOf::from_raw(B::f_to_i(x.raw())))),
        ("to_bits", m(PolyIntOf::from_raw(B::f_to_bits(x.raw())))),
        (
            "swap_stereo",
            f(PolyFloatOf::from_raw(B::f_swap_stereo(x.raw()))),
        ),
        (
            "swap_voices",
            f(PolyFloatOf::from_raw(B::f_swap_voices(x.raw()))),
        ),
        (
            "swap_inner",
            f(PolyFloatOf::from_raw(B::f_swap_inner(x.raw()))),
        ),
        ("reverse", f(PolyFloatOf::from_raw(B::f_reverse(x.raw())))),
        (
            "unpack_lo",
            f(PolyFloatOf::from_raw(B::f_unpack_lo(x.raw(), y.raw()))),
        ),
        (
            "compact_first_voices",
            f(PolyFloatOf::from_raw(B::f_compact_first_voices(
                x.raw(),
                y.raw(),
            ))),
        ),
        ("transpose0", f(transposed.0)),
        ("transpose1", f(transposed.1)),
        ("transpose2", f(transposed.2)),
        ("transpose3", f(transposed.3)),
    ]
}

/// Every int operation of `PolyIntOf`.
fn int_ops<B: Backend>(a: [u32; 4], b: [u32; 4]) -> Vec<(&'static str, Vec<u32>)> {
    let x = PolyIntOf::<B>::from_array(a);
    let y = PolyIntOf::<B>::from_array(b);
    let m = |v: PolyIntOf<B>| v.to_array().to_vec();
    let f = |v: PolyFloatOf<B>| v.to_array().map(f32::to_bits).to_vec();
    vec![
        ("add", m(x + y)),
        ("sub", m(x - y)),
        ("mul", m(x * y)),
        ("mul_scalar", m(x * b[2])),
        ("neg", m(-x)),
        ("not", m(!x)),
        ("and", m(x & y)),
        ("or", m(x | y)),
        ("xor", m(x ^ y)),
        ("max", m(PolyIntOf::max(x, y))),
        ("min", m(PolyIntOf::min(x, y))),
        ("equal", m(PolyIntOf::equal(x, y))),
        ("greater_than", m(PolyIntOf::greater_than(x, y))),
        ("less_than", m(PolyIntOf::less_than(x, y))),
        ("sum", vec![x.sum()]),
        ("any_mask", vec![x.any_mask()]),
        ("to_float", f(PolyFloatOf::from_raw(B::i_to_f(x.raw())))),
        (
            "from_bits",
            f(PolyFloatOf::from_raw(B::f_from_bits(x.raw()))),
        ),
        ("shr0", m(PolyIntOf::from_raw(B::i_shr::<0>(x.raw())))),
        ("shr1", m(PolyIntOf::from_raw(B::i_shr::<1>(x.raw())))),
        ("shr23", m(PolyIntOf::from_raw(B::i_shr::<23>(x.raw())))),
        ("shr31", m(PolyIntOf::from_raw(B::i_shr::<31>(x.raw())))),
        ("shl0", m(PolyIntOf::from_raw(B::i_shl::<0>(x.raw())))),
        ("shl23", m(PolyIntOf::from_raw(B::i_shl::<23>(x.raw())))),
        ("shl31", m(PolyIntOf::from_raw(B::i_shl::<31>(x.raw())))),
        (
            "swap_stereo",
            m(PolyIntOf::from_raw(B::i_swap_stereo(x.raw()))),
        ),
        (
            "swap_voices",
            m(PolyIntOf::from_raw(B::i_swap_voices(x.raw()))),
        ),
    ]
}

#[test]
fn float_ops_match_scalar_model() {
    let floats = special_floats();
    let pairs = quad_pairs(&floats);
    let thirds = quads(&floats);
    assert!(pairs.len() > 10_000, "too few cases: {}", pairs.len());
    for (n, (a, b)) in pairs.iter().enumerate() {
        let c = thirds[n % thirds.len()];
        let native = float_ops::<Native>(*a, *b, c);
        let scalar = float_ops::<Model>(*a, *b, c);
        for ((name, got), (_, want)) in native.iter().zip(&scalar) {
            assert_eq!(got, want, "{name}({a:?}, {b:?}, {c:?})");
        }
    }
}

#[test]
fn int_ops_match_scalar_model() {
    let ints = special_ints();
    for (a, b) in quad_pairs(&ints) {
        let native = int_ops::<Native>(a, b);
        let scalar = int_ops::<Model>(a, b);
        for ((name, got), (_, want)) in native.iter().zip(&scalar) {
            assert_eq!(got, want, "{name}({a:x?}, {b:x?})");
        }
    }
}

#[test]
fn float_conversion_matches_on_float_bit_patterns_as_ints() {
    // `to_float` on integers that come from float bit patterns (log2's path).
    let floats = special_floats();
    for a in quads(&floats) {
        let ints = a.map(f32::to_bits);
        let native = int_ops::<Native>(ints, ints);
        let scalar = int_ops::<Model>(ints, ints);
        assert_eq!(native, scalar, "{a:?}");
    }
}
