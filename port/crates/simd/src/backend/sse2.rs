// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! SSE2 backend: the same intrinsics upstream's `VITAL_SSE2` path uses.

use core::arch::x86_64::*;

use super::Backend;

/// SSE2 backend (x86_64 baseline).
#[derive(Clone, Copy, Debug, Default)]
pub struct Sse2;

const fn shuffle(z: i32, y: i32, x: i32, w: i32) -> i32 {
    (z << 6) | (y << 4) | (x << 2) | w
}

const SWAP_STEREO: i32 = shuffle(2, 3, 0, 1);
const SWAP_VOICES: i32 = shuffle(1, 0, 3, 2);
const SWAP_INNER: i32 = shuffle(3, 1, 2, 0);
const REVERSE: i32 = shuffle(0, 1, 2, 3);
const COMPACT_FIRST: i32 = shuffle(1, 0, 1, 0);
const SIGN: i32 = i32::MIN;

impl Backend for Sse2 {
    type F = __m128;
    type I = __m128i;

    #[inline(always)]
    fn f_splat(x: f32) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_set1_ps(x) }
    }
    #[inline(always)]
    fn f_from_array(a: [f32; 4]) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend, and the
        // pointer covers the 16-byte array; the access has no alignment requirement.
        unsafe { _mm_loadu_ps(a.as_ptr()) }
    }
    #[inline(always)]
    fn f_to_array(v: __m128) -> [f32; 4] {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend, and the
        // pointer covers the 16-byte array; the access has no alignment requirement.
        unsafe {
            let mut out = [0.0f32; 4];
            _mm_storeu_ps(out.as_mut_ptr(), v);
            out
        }
    }

    #[inline(always)]
    fn f_add(a: __m128, b: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_add_ps(a, b) }
    }
    #[inline(always)]
    fn f_sub(a: __m128, b: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_sub_ps(a, b) }
    }
    #[inline(always)]
    fn f_mul(a: __m128, b: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_mul_ps(a, b) }
    }
    #[inline(always)]
    fn f_div(a: __m128, b: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_div_ps(a, b) }
    }
    #[inline(always)]
    fn f_neg(a: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_xor_ps(a, _mm_set1_ps(-0.0)) }
    }
    #[inline(always)]
    fn f_max(a: __m128, b: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_max_ps(a, b) }
    }
    #[inline(always)]
    fn f_min(a: __m128, b: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_min_ps(a, b) }
    }
    #[inline(always)]
    fn f_sqrt(a: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_sqrt_ps(a) }
    }

    #[inline(always)]
    fn f_and(a: __m128, m: __m128i) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_and_ps(a, _mm_castsi128_ps(m)) }
    }
    #[inline(always)]
    fn f_or(a: __m128, m: __m128i) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_or_ps(a, _mm_castsi128_ps(m)) }
    }
    #[inline(always)]
    fn f_xor(a: __m128, m: __m128i) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_xor_ps(a, _mm_castsi128_ps(m)) }
    }

    #[inline(always)]
    fn f_eq(a: __m128, b: __m128) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_castps_si128(_mm_cmpeq_ps(a, b)) }
    }
    #[inline(always)]
    fn f_neq(a: __m128, b: __m128) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_castps_si128(_mm_cmpneq_ps(a, b)) }
    }
    #[inline(always)]
    fn f_gt(a: __m128, b: __m128) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_castps_si128(_mm_cmpgt_ps(a, b)) }
    }
    #[inline(always)]
    fn f_ge(a: __m128, b: __m128) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_castps_si128(_mm_cmpge_ps(a, b)) }
    }

    #[inline(always)]
    fn f_sum(a: __m128) -> f32 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe {
            let flip = _mm_shuffle_ps::<SWAP_VOICES>(a, a);
            let sum = _mm_add_ps(a, flip);
            let swap = _mm_shuffle_ps::<SWAP_STEREO>(sum, sum);
            _mm_cvtss_f32(_mm_add_ps(sum, swap))
        }
    }

    #[inline(always)]
    fn f_to_bits(a: __m128) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_castps_si128(a) }
    }
    #[inline(always)]
    fn f_from_bits(a: __m128i) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_castsi128_ps(a) }
    }
    #[inline(always)]
    fn i_to_f(a: __m128i) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_cvtepi32_ps(a) }
    }
    #[inline(always)]
    fn f_to_i(a: __m128) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_cvtps_epi32(a) }
    }

    #[inline(always)]
    fn i_splat(x: u32) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_set1_epi32(x as i32) }
    }
    #[inline(always)]
    fn i_from_array(a: [u32; 4]) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend, and the
        // pointer covers the 16-byte array; the access has no alignment requirement.
        unsafe { _mm_loadu_si128(a.as_ptr().cast()) }
    }
    #[inline(always)]
    fn i_to_array(v: __m128i) -> [u32; 4] {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend, and the
        // pointer covers the 16-byte array; the access has no alignment requirement.
        unsafe {
            let mut out = [0u32; 4];
            _mm_storeu_si128(out.as_mut_ptr().cast(), v);
            out
        }
    }

    #[inline(always)]
    fn i_add(a: __m128i, b: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_add_epi32(a, b) }
    }
    #[inline(always)]
    fn i_sub(a: __m128i, b: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_sub_epi32(a, b) }
    }
    #[inline(always)]
    fn i_mul(a: __m128i, b: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe {
            let mul0_2 = _mm_mul_epu32(a, b);
            let mul1_3 = _mm_mul_epu32(
                _mm_shuffle_epi32::<SWAP_STEREO>(a),
                _mm_shuffle_epi32::<SWAP_STEREO>(b),
            );
            _mm_unpacklo_epi32(
                _mm_shuffle_epi32::<{ shuffle(0, 0, 2, 0) }>(mul0_2),
                _mm_shuffle_epi32::<{ shuffle(0, 0, 2, 0) }>(mul1_3),
            )
        }
    }
    #[inline(always)]
    fn i_neg(a: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_sub_epi32(_mm_set1_epi32(0), a) }
    }
    #[inline(always)]
    fn i_and(a: __m128i, b: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_and_si128(a, b) }
    }
    #[inline(always)]
    fn i_or(a: __m128i, b: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_or_si128(a, b) }
    }
    #[inline(always)]
    fn i_xor(a: __m128i, b: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_xor_si128(a, b) }
    }
    #[inline(always)]
    fn i_eq(a: __m128i, b: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_cmpeq_epi32(a, b) }
    }
    #[inline(always)]
    fn i_gt(a: __m128i, b: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe {
            let sign = _mm_set1_epi32(SIGN);
            _mm_cmpgt_epi32(_mm_xor_si128(a, sign), _mm_xor_si128(b, sign))
        }
    }
    #[inline(always)]
    fn i_max(a: __m128i, b: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe {
            let greater_than_mask = Self::i_gt(a, b);
            _mm_or_si128(
                _mm_and_si128(greater_than_mask, a),
                _mm_andnot_si128(greater_than_mask, b),
            )
        }
    }
    #[inline(always)]
    fn i_min(a: __m128i, b: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe {
            let less_than_mask = _mm_cmpgt_epi32(b, a);
            _mm_or_si128(
                _mm_and_si128(less_than_mask, a),
                _mm_andnot_si128(less_than_mask, b),
            )
        }
    }
    #[inline(always)]
    fn i_sum(a: __m128i) -> u32 {
        let lanes = Self::i_to_array(a);
        let mut total = 0u32;
        for lane in lanes {
            total = total.wrapping_add(lane);
        }
        total
    }
    #[inline(always)]
    fn i_any_mask(a: __m128i) -> u32 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_movemask_epi8(a) as u32 }
    }
    #[inline(always)]
    fn i_shr<const N: i32>(a: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_srli_epi32::<N>(a) }
    }
    #[inline(always)]
    fn i_shl<const N: i32>(a: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_slli_epi32::<N>(a) }
    }

    #[inline(always)]
    fn f_swap_stereo(a: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_shuffle_ps::<SWAP_STEREO>(a, a) }
    }
    #[inline(always)]
    fn f_swap_voices(a: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_shuffle_ps::<SWAP_VOICES>(a, a) }
    }
    #[inline(always)]
    fn f_swap_inner(a: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_shuffle_ps::<SWAP_INNER>(a, a) }
    }
    #[inline(always)]
    fn f_reverse(a: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_shuffle_ps::<REVERSE>(a, a) }
    }
    #[inline(always)]
    fn f_unpack_lo(a: __m128, b: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_unpacklo_ps(a, b) }
    }
    #[inline(always)]
    fn f_compact_first_voices(a: __m128, b: __m128) -> __m128 {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_shuffle_ps::<COMPACT_FIRST>(a, b) }
    }
    #[inline(always)]
    fn i_swap_stereo(a: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_shuffle_epi32::<SWAP_STEREO>(a) }
    }
    #[inline(always)]
    fn i_swap_voices(a: __m128i) -> __m128i {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe { _mm_shuffle_epi32::<SWAP_VOICES>(a) }
    }
    #[inline(always)]
    fn f_transpose(r0: &mut __m128, r1: &mut __m128, r2: &mut __m128, r3: &mut __m128) {
        // SAFETY: SSE2 (x86_64 baseline) is enabled for every build of this backend.
        unsafe {
            let low0 = _mm_unpacklo_ps(*r0, *r1);
            let low1 = _mm_unpacklo_ps(*r2, *r3);
            let high0 = _mm_unpackhi_ps(*r0, *r1);
            let high1 = _mm_unpackhi_ps(*r2, *r3);
            *r0 = _mm_movelh_ps(low0, low1);
            *r1 = _mm_movehl_ps(low1, low0);
            *r2 = _mm_movelh_ps(high0, high1);
            *r3 = _mm_movehl_ps(high1, high0);
        }
    }
}
