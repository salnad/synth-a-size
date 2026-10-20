// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lane-level SIMD backends behind [`PolyFloatOf`](crate::PolyFloatOf) and
//! [`PolyIntOf`](crate::PolyIntOf).
//!
//! Ports the intrinsic bodies of `vital/src/synthesis/framework/poly_values.h`
//! and the shuffles/conversions of `poly_utils.h`.
//!
//! Upstream's SSE2 and NEON paths are not the same arithmetic, and each
//! reference build uses the path of its own architecture (the x86_64 Linux
//! reference uses SSE2; the arm64 macOS reference, which matches the shipping
//! plugin, uses NEON). Each backend therefore follows its own upstream path.
//! These are the only per-ISA poly operations; the labels are the site IDs in
//! `migration/decisions/reference-arm.md` section A:
//!
//! | Operation | SSE2 (`Sse2`, [`Scalar`]) | NEON (`Neon`, [`ScalarNeon`]) |
//! |---|---|---|
//! | A5: float `max`/`min` | `MAXPS`/`MINPS`: `a > b ? a : b`, `a < b ? a : b`; NaN or `±0` tie returns `b` | `FMAX`/`FMIN`: NaN propagates (AArch64 `FPProcessNaNs`); `max(+0, -0) = +0`, `min(+0, -0) = -0` |
//! | A6: float `not_equal` | unordered (true on NaN) | `gt \| lt`: ordered (false on NaN) |
//! | A4: horizontal float sum | `(v0 + v2) + (v1 + v3)` | `(v0 + v1) + (v2 + v3)` |
//! | A1: float → int (`toInt`) | `CVTPS2DQ`: round to nearest even; NaN/out of range → `0x8000_0000` | `FCVTZS`: truncate toward zero, saturate, NaN → 0 |
//! | A7: int `min` | **signed** compare | unsigned |
//! | A8: `any_mask` | `PMOVMSKB` (16-bit byte-sign mask) | maximum lane value |
//!
//! Shared by both: exact IEEE `+ - * /` and `sqrt` (upstream's AArch64 NEON
//! path uses `vdivq_f32`; `NEON_ARM32` reciprocal estimates are not used),
//! `mul_add`/`mul_sub` as a separate multiply and add with two roundings
//! (no FMA contraction, no estimate instructions), ordered `==`, `>`, `>=`,
//! unsigned int `max` and `>`, wrapping int arithmetic, sign-bit negation
//! (upstream's NEON `vmulq_n_f32(v, -1)` is identical for every non-NaN input
//! and compilers emit it as `FNEG`), and the lane shuffles.
//!
//! [`ScalarOf`] is the plain-Rust model of both columns. [`Scalar`] (SSE2
//! semantics) is the fallback on other targets; [`Model`] is the scalar model
//! of the semantics this build uses, which the tests check [`Native`] against
//! bit for bit. With `--cfg sas_simd_force_scalar`, [`Native`] is [`Model`].
//!
//! The `aarch64-semantics` cargo feature selects the NEON column on x86_64
//! (as [`ScalarNeon`]), so x86 developers can reproduce the shipping arm64
//! behaviour against macOS reference renders. The scalar conversions that
//! also differ per ISA are in [`conversions`](crate::conversions).

pub mod scalar;

#[cfg(all(
    target_arch = "x86_64",
    not(sas_simd_force_scalar),
    not(feature = "aarch64-semantics")
))]
pub mod sse2;

#[cfg(all(target_arch = "aarch64", not(sas_simd_force_scalar)))]
pub mod neon;

pub use scalar::{Scalar, ScalarNeon, ScalarOf};

#[cfg(all(
    target_arch = "x86_64",
    not(sas_simd_force_scalar),
    not(feature = "aarch64-semantics")
))]
pub use sse2::Sse2;

#[cfg(all(target_arch = "aarch64", not(sas_simd_force_scalar)))]
pub use neon::Neon;

/// Scalar model of the semantics this build uses.
#[cfg(any(target_arch = "aarch64", feature = "aarch64-semantics"))]
pub type Model = ScalarNeon;

/// Scalar model of the semantics this build uses.
#[cfg(not(any(target_arch = "aarch64", feature = "aarch64-semantics")))]
pub type Model = Scalar;

/// The backend used by [`PolyFloat`](crate::PolyFloat) on this target.
#[cfg(all(
    target_arch = "x86_64",
    not(sas_simd_force_scalar),
    not(feature = "aarch64-semantics")
))]
pub type Native = Sse2;

/// The backend used by [`PolyFloat`](crate::PolyFloat) on this target.
#[cfg(all(target_arch = "aarch64", not(sas_simd_force_scalar)))]
pub type Native = Neon;

/// The backend used by [`PolyFloat`](crate::PolyFloat) on this target.
#[cfg(any(
    sas_simd_force_scalar,
    all(target_arch = "x86_64", feature = "aarch64-semantics"),
    not(any(target_arch = "x86_64", target_arch = "aarch64"))
))]
pub type Native = Model;

/// Lane operations on 4 × `f32` (`F`) and 4 × `u32` (`I`) vectors.
///
/// Lane order is upstream's: voice 0 left, voice 0 right, voice 1 left,
/// voice 1 right. See the module docs for the semantics of each
/// implementation.
pub trait Backend: Copy + 'static {
    /// 4 × `f32` vector.
    type F: Copy;
    /// 4 × `u32` vector (also used as a lane mask).
    type I: Copy;

    fn f_splat(x: f32) -> Self::F;
    fn f_from_array(a: [f32; 4]) -> Self::F;
    fn f_to_array(v: Self::F) -> [f32; 4];

    fn f_add(a: Self::F, b: Self::F) -> Self::F;
    fn f_sub(a: Self::F, b: Self::F) -> Self::F;
    fn f_mul(a: Self::F, b: Self::F) -> Self::F;
    fn f_div(a: Self::F, b: Self::F) -> Self::F;
    fn f_neg(a: Self::F) -> Self::F;
    fn f_max(a: Self::F, b: Self::F) -> Self::F;
    fn f_min(a: Self::F, b: Self::F) -> Self::F;
    fn f_sqrt(a: Self::F) -> Self::F;

    fn f_and(a: Self::F, m: Self::I) -> Self::F;
    fn f_or(a: Self::F, m: Self::I) -> Self::F;
    fn f_xor(a: Self::F, m: Self::I) -> Self::F;

    fn f_eq(a: Self::F, b: Self::F) -> Self::I;
    fn f_neq(a: Self::F, b: Self::F) -> Self::I;
    fn f_gt(a: Self::F, b: Self::F) -> Self::I;
    fn f_ge(a: Self::F, b: Self::F) -> Self::I;

    fn f_sum(a: Self::F) -> f32;

    fn f_to_bits(a: Self::F) -> Self::I;
    fn f_from_bits(a: Self::I) -> Self::F;
    /// Signed int → float (round to nearest even).
    fn i_to_f(a: Self::I) -> Self::F;
    /// Float → signed int (`toInt`, see the module docs).
    fn f_to_i(a: Self::F) -> Self::I;

    fn i_splat(x: u32) -> Self::I;
    fn i_from_array(a: [u32; 4]) -> Self::I;
    fn i_to_array(v: Self::I) -> [u32; 4];

    fn i_add(a: Self::I, b: Self::I) -> Self::I;
    fn i_sub(a: Self::I, b: Self::I) -> Self::I;
    fn i_mul(a: Self::I, b: Self::I) -> Self::I;
    fn i_neg(a: Self::I) -> Self::I;
    fn i_and(a: Self::I, b: Self::I) -> Self::I;
    fn i_or(a: Self::I, b: Self::I) -> Self::I;
    fn i_xor(a: Self::I, b: Self::I) -> Self::I;
    fn i_eq(a: Self::I, b: Self::I) -> Self::I;
    /// Unsigned `a > b`.
    fn i_gt(a: Self::I, b: Self::I) -> Self::I;
    /// Unsigned max.
    fn i_max(a: Self::I, b: Self::I) -> Self::I;
    /// Min (signed on SSE2, unsigned on NEON; see the module docs).
    fn i_min(a: Self::I, b: Self::I) -> Self::I;
    fn i_sum(a: Self::I) -> u32;
    fn i_any_mask(a: Self::I) -> u32;
    fn i_shr<const N: i32>(a: Self::I) -> Self::I;
    fn i_shl<const N: i32>(a: Self::I) -> Self::I;

    /// `[v1, v0, v3, v2]`
    fn f_swap_stereo(a: Self::F) -> Self::F;
    /// `[v2, v3, v0, v1]`
    fn f_swap_voices(a: Self::F) -> Self::F;
    /// `[v0, v2, v1, v3]`
    fn f_swap_inner(a: Self::F) -> Self::F;
    /// `[v3, v2, v1, v0]`
    fn f_reverse(a: Self::F) -> Self::F;
    /// `[a0, b0, a1, b1]`
    fn f_unpack_lo(a: Self::F, b: Self::F) -> Self::F;
    /// `[a0, a1, b0, b1]`
    fn f_compact_first_voices(a: Self::F, b: Self::F) -> Self::F;
    /// `[v1, v0, v3, v2]`
    fn i_swap_stereo(a: Self::I) -> Self::I;
    /// `[v2, v3, v0, v1]`
    fn i_swap_voices(a: Self::I) -> Self::I;
    /// In-place 4 × 4 transpose of four rows.
    fn f_transpose(r0: &mut Self::F, r1: &mut Self::F, r2: &mut Self::F, r3: &mut Self::F);
}
