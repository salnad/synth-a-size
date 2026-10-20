// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Scalar float → integer conversions whose upstream result depends on the
//! ISA (`migration/decisions/reference-arm.md`, section A).
//!
//! Upstream writes these as plain C++ casts. Out-of-range inputs are
//! undefined behaviour in C++, and the two reference builds do different
//! things:
//!
//! | Cast | x86_64 (GCC, SSE2) | aarch64 (Apple clang) |
//! |---|---|---|
//! | `(uint32_t)float` (A2, e.g. random phase in `synth_oscillator.cpp`) | 64-bit `CVTTSS2SI`, low 32 bits: negative values wrap modulo 2^32; NaN and \|x\| ≥ 2^63 give 0 | `FCVTZU`: saturate to `[0, u32::MAX]`, NaN → 0 |
//! | `(int32_t)float` (A3) | `CVTTSS2SI`: NaN and out of range give `0x8000_0000` | `FCVTZS`: saturate, NaN → 0 |
//!
//! Rust's `as` casts truncate and saturate, which is the aarch64 column; the
//! x86_64 column is written out in [`x86_64`]. Port code calls the
//! target-selected [`float_to_u32`] / [`float_to_i32`] at the enumerated
//! sites only. With the `aarch64-semantics` feature, x86_64 builds use the
//! aarch64 column too. The poly `toInt` (A1) is in
//! [`poly_utils::to_int`](crate::poly_utils::to_int).

/// The x86_64 (SSE2 reference) results.
pub mod x86_64 {
    /// `(uint32_t)value` as GCC compiles it on x86_64.
    #[inline]
    pub fn float_to_u32(value: f32) -> u32 {
        const LIMIT: f32 = 9_223_372_036_854_775_808.0; // 2^63
        if value.is_nan() || !(-LIMIT..LIMIT).contains(&value) {
            0
        } else {
            (value as i64) as u32
        }
    }

    /// `(int32_t)value` (`CVTTSS2SI`).
    #[inline]
    pub fn float_to_i32(value: f32) -> i32 {
        const LIMIT: f32 = 2_147_483_648.0; // 2^31
        if value.is_nan() || !(-LIMIT..LIMIT).contains(&value) {
            i32::MIN
        } else {
            value as i32
        }
    }
}

/// The aarch64 (NEON reference, shipping plugin) results.
pub mod aarch64 {
    /// `(uint32_t)value` (`FCVTZU`).
    #[inline]
    pub fn float_to_u32(value: f32) -> u32 {
        value as u32
    }

    /// `(int32_t)value` (`FCVTZS`).
    #[inline]
    pub fn float_to_i32(value: f32) -> i32 {
        value as i32
    }
}

#[cfg(any(target_arch = "aarch64", feature = "aarch64-semantics"))]
pub use self::aarch64::{float_to_i32, float_to_u32};

#[cfg(not(any(target_arch = "aarch64", feature = "aarch64-semantics")))]
pub use self::x86_64::{float_to_i32, float_to_u32};
