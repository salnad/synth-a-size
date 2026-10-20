// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! NEON backend (aarch64).
//!
//! Uses the intrinsics of upstream's `VITAL_NEON` path (AArch64, so exact
//! `vdivq_f32` rather than the `NEON_ARM32` reciprocal estimate). The
//! differences from the SSE2 path are listed in the [backend docs](super).
//! This is the backend the shipping arm64 macOS plugin runs.

use core::arch::aarch64::*;

use super::Backend;

/// NEON backend.
#[derive(Clone, Copy, Debug, Default)]
pub struct Neon;

impl Backend for Neon {
    type F = float32x4_t;
    type I = uint32x4_t;

    #[inline(always)]
    fn f_splat(x: f32) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vdupq_n_f32(x) }
    }
    #[inline(always)]
    fn f_from_array(a: [f32; 4]) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend, and the
        // pointer covers the 16-byte array; the access has no alignment requirement.
        unsafe { vld1q_f32(a.as_ptr()) }
    }
    #[inline(always)]
    fn f_to_array(v: float32x4_t) -> [f32; 4] {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend, and the
        // pointer covers the 16-byte array; the access has no alignment requirement.
        unsafe {
            let mut out = [0.0f32; 4];
            vst1q_f32(out.as_mut_ptr(), v);
            out
        }
    }

    #[inline(always)]
    fn f_add(a: float32x4_t, b: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vaddq_f32(a, b) }
    }
    #[inline(always)]
    fn f_sub(a: float32x4_t, b: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vsubq_f32(a, b) }
    }
    #[inline(always)]
    fn f_mul(a: float32x4_t, b: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vmulq_f32(a, b) }
    }
    #[inline(always)]
    fn f_div(a: float32x4_t, b: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vdivq_f32(a, b) }
    }
    #[inline(always)]
    fn f_neg(a: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vnegq_f32(a) }
    }
    #[inline(always)]
    fn f_max(a: float32x4_t, b: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vmaxq_f32(a, b) }
    }
    #[inline(always)]
    fn f_min(a: float32x4_t, b: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vminq_f32(a, b) }
    }
    #[inline(always)]
    fn f_sqrt(a: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vsqrtq_f32(a) }
    }

    #[inline(always)]
    fn f_and(a: float32x4_t, m: uint32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vreinterpretq_f32_u32(vandq_u32(vreinterpretq_u32_f32(a), m)) }
    }
    #[inline(always)]
    fn f_or(a: float32x4_t, m: uint32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vreinterpretq_f32_u32(vorrq_u32(vreinterpretq_u32_f32(a), m)) }
    }
    #[inline(always)]
    fn f_xor(a: float32x4_t, m: uint32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vreinterpretq_f32_u32(veorq_u32(vreinterpretq_u32_f32(a), m)) }
    }

    #[inline(always)]
    fn f_eq(a: float32x4_t, b: float32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vceqq_f32(a, b) }
    }
    #[inline(always)]
    fn f_neq(a: float32x4_t, b: float32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vorrq_u32(vcgtq_f32(a, b), vcltq_f32(a, b)) }
    }
    #[inline(always)]
    fn f_gt(a: float32x4_t, b: float32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vcgtq_f32(a, b) }
    }
    #[inline(always)]
    fn f_ge(a: float32x4_t, b: float32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vcgeq_f32(a, b) }
    }

    #[inline(always)]
    fn f_sum(a: float32x4_t) -> f32 {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe {
            let sum = vpadd_f32(vget_low_f32(a), vget_high_f32(a));
            let sum = vpadd_f32(sum, sum);
            vget_lane_f32::<0>(sum)
        }
    }

    #[inline(always)]
    fn f_to_bits(a: float32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vreinterpretq_u32_f32(a) }
    }
    #[inline(always)]
    fn f_from_bits(a: uint32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vreinterpretq_f32_u32(a) }
    }
    #[inline(always)]
    fn i_to_f(a: uint32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vcvtq_f32_s32(vreinterpretq_s32_u32(a)) }
    }
    #[inline(always)]
    fn f_to_i(a: float32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vreinterpretq_u32_s32(vcvtq_s32_f32(a)) }
    }

    #[inline(always)]
    fn i_splat(x: u32) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vdupq_n_u32(x) }
    }
    #[inline(always)]
    fn i_from_array(a: [u32; 4]) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend, and the
        // pointer covers the 16-byte array; the access has no alignment requirement.
        unsafe { vld1q_u32(a.as_ptr()) }
    }
    #[inline(always)]
    fn i_to_array(v: uint32x4_t) -> [u32; 4] {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend, and the
        // pointer covers the 16-byte array; the access has no alignment requirement.
        unsafe {
            let mut out = [0u32; 4];
            vst1q_u32(out.as_mut_ptr(), v);
            out
        }
    }

    #[inline(always)]
    fn i_add(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vaddq_u32(a, b) }
    }
    #[inline(always)]
    fn i_sub(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vsubq_u32(a, b) }
    }
    #[inline(always)]
    fn i_mul(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vmulq_u32(a, b) }
    }
    #[inline(always)]
    fn i_neg(a: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vsubq_u32(vdupq_n_u32(0), a) }
    }
    #[inline(always)]
    fn i_and(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vandq_u32(a, b) }
    }
    #[inline(always)]
    fn i_or(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vorrq_u32(a, b) }
    }
    #[inline(always)]
    fn i_xor(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { veorq_u32(a, b) }
    }
    #[inline(always)]
    fn i_eq(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vceqq_u32(a, b) }
    }
    #[inline(always)]
    fn i_gt(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vcgtq_u32(a, b) }
    }
    #[inline(always)]
    fn i_max(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vmaxq_u32(a, b) }
    }
    #[inline(always)]
    fn i_min(a: uint32x4_t, b: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vminq_u32(a, b) }
    }
    #[inline(always)]
    fn i_sum(a: uint32x4_t) -> u32 {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vaddvq_u32(a) }
    }
    #[inline(always)]
    fn i_any_mask(a: uint32x4_t) -> u32 {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe {
            let max = vpmax_u32(vget_low_u32(a), vget_high_u32(a));
            let max = vpmax_u32(max, max);
            vget_lane_u32::<0>(max)
        }
    }
    #[inline(always)]
    fn i_shr<const N: i32>(a: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vshlq_u32(a, vdupq_n_s32(-N)) }
    }
    #[inline(always)]
    fn i_shl<const N: i32>(a: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vshlq_u32(a, vdupq_n_s32(N)) }
    }

    #[inline(always)]
    fn f_swap_stereo(a: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vrev64q_f32(a) }
    }
    #[inline(always)]
    fn f_swap_voices(a: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vextq_f32::<2>(a, a) }
    }
    #[inline(always)]
    fn f_swap_inner(a: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vzip1q_f32(a, vextq_f32::<2>(a, a)) }
    }
    #[inline(always)]
    fn f_reverse(a: float32x4_t) -> float32x4_t {
        Self::f_swap_voices(Self::f_swap_stereo(a))
    }
    #[inline(always)]
    fn f_unpack_lo(a: float32x4_t, b: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vzip1q_f32(a, b) }
    }
    #[inline(always)]
    fn f_compact_first_voices(a: float32x4_t, b: float32x4_t) -> float32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vcombine_f32(vget_low_f32(a), vget_low_f32(b)) }
    }
    #[inline(always)]
    fn i_swap_stereo(a: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vrev64q_u32(a) }
    }
    #[inline(always)]
    fn i_swap_voices(a: uint32x4_t) -> uint32x4_t {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe { vextq_u32::<2>(a, a) }
    }
    #[inline(always)]
    fn f_transpose(
        r0: &mut float32x4_t,
        r1: &mut float32x4_t,
        r2: &mut float32x4_t,
        r3: &mut float32x4_t,
    ) {
        // SAFETY: NEON (aarch64 baseline) is enabled for every build of this backend.
        unsafe {
            let swap_low = vtrnq_f32(*r0, *r1);
            let swap_high = vtrnq_f32(*r2, *r3);
            *r0 = vextq_f32::<2>(vextq_f32::<2>(swap_low.0, swap_low.0), swap_high.0);
            *r1 = vextq_f32::<2>(vextq_f32::<2>(swap_low.1, swap_low.1), swap_high.1);
            *r2 = vextq_f32::<2>(swap_low.0, vextq_f32::<2>(swap_high.0, swap_high.0));
            *r3 = vextq_f32::<2>(swap_low.1, vextq_f32::<2>(swap_high.1, swap_high.1));
        }
    }
}
