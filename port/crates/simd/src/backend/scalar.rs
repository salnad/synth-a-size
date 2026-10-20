// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust models of upstream's SSE2 and NEON `poly_float`/`poly_int`
//! semantics.

use super::Backend;

/// Scalar backend: four independent lanes. `NEON = false` models upstream's
/// SSE2 path, `NEON = true` its NEON path (see the [backend docs](super)).
#[derive(Clone, Copy, Debug, Default)]
pub struct ScalarOf<const NEON: bool>;

/// Scalar model of the SSE2 semantics.
pub type Scalar = ScalarOf<false>;

/// Scalar model of the NEON semantics.
pub type ScalarNeon = ScalarOf<true>;

const SIGN: u32 = 0x8000_0000;

#[inline(always)]
fn mask(b: bool) -> u32 {
    if b {
        u32::MAX
    } else {
        0
    }
}

#[inline(always)]
fn map_f(a: [f32; 4], f: impl Fn(f32) -> f32) -> [f32; 4] {
    [f(a[0]), f(a[1]), f(a[2]), f(a[3])]
}

#[inline(always)]
fn zip_f(a: [f32; 4], b: [f32; 4], f: impl Fn(f32, f32) -> f32) -> [f32; 4] {
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), f(a[3], b[3])]
}

#[inline(always)]
fn cmp_f(a: [f32; 4], b: [f32; 4], f: impl Fn(f32, f32) -> bool) -> [u32; 4] {
    [
        mask(f(a[0], b[0])),
        mask(f(a[1], b[1])),
        mask(f(a[2], b[2])),
        mask(f(a[3], b[3])),
    ]
}

#[inline(always)]
fn bits_f(a: [f32; 4], m: [u32; 4], f: impl Fn(u32, u32) -> u32) -> [f32; 4] {
    [
        f32::from_bits(f(a[0].to_bits(), m[0])),
        f32::from_bits(f(a[1].to_bits(), m[1])),
        f32::from_bits(f(a[2].to_bits(), m[2])),
        f32::from_bits(f(a[3].to_bits(), m[3])),
    ]
}

#[inline(always)]
fn map_i(a: [u32; 4], f: impl Fn(u32) -> u32) -> [u32; 4] {
    [f(a[0]), f(a[1]), f(a[2]), f(a[3])]
}

#[inline(always)]
fn zip_i(a: [u32; 4], b: [u32; 4], f: impl Fn(u32, u32) -> u32) -> [u32; 4] {
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), f(a[3], b[3])]
}

/// `CVTPS2DQ` under the default MXCSR rounding mode.
#[inline(always)]
pub(crate) fn cvtps2dq(x: f32) -> u32 {
    // 2^31 as f32; every finite f32 below it rounds to a value that fits.
    const LIMIT: f32 = 2_147_483_648.0;
    if x.is_nan() || !(-LIMIT..LIMIT).contains(&x) {
        SIGN
    } else {
        (x.round_ties_even() as i32) as u32
    }
}

/// `FCVTZS`: truncate toward zero, saturate, NaN → 0.
#[inline(always)]
pub(crate) fn fcvtzs(x: f32) -> u32 {
    (x as i32) as u32
}

#[inline(always)]
fn quiet(x: f32) -> f32 {
    f32::from_bits(x.to_bits() | 0x0040_0000)
}

#[inline(always)]
fn is_signaling(x: f32) -> bool {
    x.is_nan() && x.to_bits() & 0x0040_0000 == 0
}

/// AArch64 `FPProcessNaNs` for a two-operand instruction (FPCR.DN = 0).
#[inline(always)]
fn process_nans(a: f32, b: f32) -> Option<f32> {
    if is_signaling(a) {
        Some(quiet(a))
    } else if is_signaling(b) {
        Some(quiet(b))
    } else if a.is_nan() {
        Some(a)
    } else if b.is_nan() {
        Some(b)
    } else {
        None
    }
}

/// `FMAX`: NaN-propagating; `max(+0, -0) = +0`.
#[inline(always)]
pub(crate) fn fmax(a: f32, b: f32) -> f32 {
    if let Some(nan) = process_nans(a, b) {
        nan
    } else if a == b {
        f32::from_bits(a.to_bits() & b.to_bits())
    } else if a > b {
        a
    } else {
        b
    }
}

/// `FMIN`: NaN-propagating; `min(+0, -0) = -0`.
#[inline(always)]
pub(crate) fn fmin(a: f32, b: f32) -> f32 {
    if let Some(nan) = process_nans(a, b) {
        nan
    } else if a == b {
        f32::from_bits(a.to_bits() | b.to_bits())
    } else if a < b {
        a
    } else {
        b
    }
}

impl<const NEON: bool> Backend for ScalarOf<NEON> {
    type F = [f32; 4];
    type I = [u32; 4];

    #[inline(always)]
    fn f_splat(x: f32) -> [f32; 4] {
        [x; 4]
    }
    #[inline(always)]
    fn f_from_array(a: [f32; 4]) -> [f32; 4] {
        a
    }
    #[inline(always)]
    fn f_to_array(v: [f32; 4]) -> [f32; 4] {
        v
    }

    #[inline(always)]
    fn f_add(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        zip_f(a, b, |x, y| x + y)
    }
    #[inline(always)]
    fn f_sub(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        zip_f(a, b, |x, y| x - y)
    }
    #[inline(always)]
    fn f_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        zip_f(a, b, |x, y| x * y)
    }
    #[inline(always)]
    fn f_div(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        zip_f(a, b, |x, y| x / y)
    }
    #[inline(always)]
    fn f_neg(a: [f32; 4]) -> [f32; 4] {
        map_f(a, |x| f32::from_bits(x.to_bits() ^ SIGN))
    }
    #[inline(always)]
    fn f_max(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        if NEON {
            zip_f(a, b, fmax)
        } else {
            zip_f(a, b, |x, y| if x > y { x } else { y })
        }
    }
    #[inline(always)]
    fn f_min(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        if NEON {
            zip_f(a, b, fmin)
        } else {
            zip_f(a, b, |x, y| if x < y { x } else { y })
        }
    }
    #[inline(always)]
    fn f_sqrt(a: [f32; 4]) -> [f32; 4] {
        map_f(a, f32::sqrt)
    }

    #[inline(always)]
    fn f_and(a: [f32; 4], m: [u32; 4]) -> [f32; 4] {
        bits_f(a, m, |x, y| x & y)
    }
    #[inline(always)]
    fn f_or(a: [f32; 4], m: [u32; 4]) -> [f32; 4] {
        bits_f(a, m, |x, y| x | y)
    }
    #[inline(always)]
    fn f_xor(a: [f32; 4], m: [u32; 4]) -> [f32; 4] {
        bits_f(a, m, |x, y| x ^ y)
    }

    #[inline(always)]
    fn f_eq(a: [f32; 4], b: [f32; 4]) -> [u32; 4] {
        cmp_f(a, b, |x, y| x == y)
    }
    #[inline(always)]
    fn f_neq(a: [f32; 4], b: [f32; 4]) -> [u32; 4] {
        if NEON {
            cmp_f(a, b, |x, y| x.partial_cmp(&y).is_some_and(|o| o.is_ne()))
        } else {
            cmp_f(a, b, |x, y| x != y)
        }
    }
    #[inline(always)]
    fn f_gt(a: [f32; 4], b: [f32; 4]) -> [u32; 4] {
        cmp_f(a, b, |x, y| x > y)
    }
    #[inline(always)]
    fn f_ge(a: [f32; 4], b: [f32; 4]) -> [u32; 4] {
        cmp_f(a, b, |x, y| x >= y)
    }

    #[inline(always)]
    fn f_sum(a: [f32; 4]) -> f32 {
        if NEON {
            (a[0] + a[1]) + (a[2] + a[3])
        } else {
            (a[0] + a[2]) + (a[1] + a[3])
        }
    }

    #[inline(always)]
    fn f_to_bits(a: [f32; 4]) -> [u32; 4] {
        a.map(f32::to_bits)
    }
    #[inline(always)]
    fn f_from_bits(a: [u32; 4]) -> [f32; 4] {
        a.map(f32::from_bits)
    }
    #[inline(always)]
    fn i_to_f(a: [u32; 4]) -> [f32; 4] {
        a.map(|x| x as i32 as f32)
    }
    #[inline(always)]
    fn f_to_i(a: [f32; 4]) -> [u32; 4] {
        if NEON {
            a.map(fcvtzs)
        } else {
            a.map(cvtps2dq)
        }
    }

    #[inline(always)]
    fn i_splat(x: u32) -> [u32; 4] {
        [x; 4]
    }
    #[inline(always)]
    fn i_from_array(a: [u32; 4]) -> [u32; 4] {
        a
    }
    #[inline(always)]
    fn i_to_array(v: [u32; 4]) -> [u32; 4] {
        v
    }

    #[inline(always)]
    fn i_add(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
        zip_i(a, b, u32::wrapping_add)
    }
    #[inline(always)]
    fn i_sub(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
        zip_i(a, b, u32::wrapping_sub)
    }
    #[inline(always)]
    fn i_mul(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
        zip_i(a, b, u32::wrapping_mul)
    }
    #[inline(always)]
    fn i_neg(a: [u32; 4]) -> [u32; 4] {
        map_i(a, u32::wrapping_neg)
    }
    #[inline(always)]
    fn i_and(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
        zip_i(a, b, |x, y| x & y)
    }
    #[inline(always)]
    fn i_or(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
        zip_i(a, b, |x, y| x | y)
    }
    #[inline(always)]
    fn i_xor(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
        zip_i(a, b, |x, y| x ^ y)
    }
    #[inline(always)]
    fn i_eq(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
        zip_i(a, b, |x, y| mask(x == y))
    }
    #[inline(always)]
    fn i_gt(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
        zip_i(a, b, |x, y| mask(x > y))
    }
    #[inline(always)]
    fn i_max(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
        zip_i(a, b, |x, y| if x > y { x } else { y })
    }
    #[inline(always)]
    fn i_min(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
        if NEON {
            zip_i(a, b, u32::min)
        } else {
            zip_i(a, b, |x, y| if (y as i32) > (x as i32) { x } else { y })
        }
    }
    #[inline(always)]
    fn i_sum(a: [u32; 4]) -> u32 {
        a[0].wrapping_add(a[1])
            .wrapping_add(a[2])
            .wrapping_add(a[3])
    }
    #[inline(always)]
    fn i_any_mask(a: [u32; 4]) -> u32 {
        if NEON {
            return a[0].max(a[1]).max(a[2]).max(a[3]);
        }
        let mut result = 0u32;
        for (lane, value) in a.iter().enumerate() {
            for byte in 0..4 {
                let bit = (value >> (byte * 8 + 7)) & 1;
                result |= bit << (lane * 4 + byte);
            }
        }
        result
    }
    #[inline(always)]
    fn i_shr<const N: i32>(a: [u32; 4]) -> [u32; 4] {
        map_i(a, |x| if (0..32).contains(&N) { x >> N } else { 0 })
    }
    #[inline(always)]
    fn i_shl<const N: i32>(a: [u32; 4]) -> [u32; 4] {
        map_i(a, |x| if (0..32).contains(&N) { x << N } else { 0 })
    }

    #[inline(always)]
    fn f_swap_stereo(a: [f32; 4]) -> [f32; 4] {
        [a[1], a[0], a[3], a[2]]
    }
    #[inline(always)]
    fn f_swap_voices(a: [f32; 4]) -> [f32; 4] {
        [a[2], a[3], a[0], a[1]]
    }
    #[inline(always)]
    fn f_swap_inner(a: [f32; 4]) -> [f32; 4] {
        [a[0], a[2], a[1], a[3]]
    }
    #[inline(always)]
    fn f_reverse(a: [f32; 4]) -> [f32; 4] {
        [a[3], a[2], a[1], a[0]]
    }
    #[inline(always)]
    fn f_unpack_lo(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        [a[0], b[0], a[1], b[1]]
    }
    #[inline(always)]
    fn f_compact_first_voices(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        [a[0], a[1], b[0], b[1]]
    }
    #[inline(always)]
    fn i_swap_stereo(a: [u32; 4]) -> [u32; 4] {
        [a[1], a[0], a[3], a[2]]
    }
    #[inline(always)]
    fn i_swap_voices(a: [u32; 4]) -> [u32; 4] {
        [a[2], a[3], a[0], a[1]]
    }
    #[inline(always)]
    fn f_transpose(r0: &mut [f32; 4], r1: &mut [f32; 4], r2: &mut [f32; 4], r3: &mut [f32; 4]) {
        let (a, b, c, d) = (*r0, *r1, *r2, *r3);
        *r0 = [a[0], b[0], c[0], d[0]];
        *r1 = [a[1], b[1], c[1], d[1]];
        *r2 = [a[2], b[2], c[2], d[2]];
        *r3 = [a[3], b[3], c[3], d[3]];
    }
}
