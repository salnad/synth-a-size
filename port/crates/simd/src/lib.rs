// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! 4-lane SIMD value types (`poly_float` / `poly_int` / `poly_mask`) and the
//! math built on them.
//!
//! Ports upstream's `vital/src/synthesis/framework/` headers
//! `poly_values.h`, `poly_utils.h`, `futils.h`, `matrix.h`, `common.h`, and
//! the scalar part of `utils.h`/`utils.cpp`.
//!
//! # Lane layout
//!
//! Every [`PolyFloat`] holds four `f32` lanes: voice 0 left, voice 0 right,
//! voice 1 left, voice 1 right. Masks ([`PolyMask`], same type as
//! [`PolyInt`]) are all-ones or all-zeros per lane.
//!
//! # Numerics
//!
//! The C++ reference is built from upstream's SSE2 path with exact division
//! and square root, no FMA contraction and no approximate reciprocals
//! (`migration/decisions/reference-determinism.md`). This crate matches that:
//!
//! - every backend implements the SSE2 lane semantics listed in [`backend`]
//!   (NaN behaviour of `max`/`min`, ordered/unordered compares,
//!   round-to-nearest-even float → int with `0x8000_0000` for out-of-range,
//!   signed int `min` vs unsigned int `max`, ...);
//! - `mul_add`/`mul_sub` round twice (`a + b * c`), never fused;
//! - the [`futils`] approximations are ported as written, operation for
//!   operation, and never replaced by libm;
//! - the libm calls upstream makes (`powf`, `logf`, `sinf`, ...) go through
//!   Rust's `f32` methods, which call the same platform libm.
//!
//! The native backend is SSE2 on x86_64 and NEON on aarch64; elsewhere, or
//! with `RUSTFLAGS="--cfg sas_simd_force_scalar"`, it is [`backend::Scalar`].
//! The test suite checks the native backend against `Scalar` bit for bit.
//!
//! # Naming
//!
//! `camelCase` functions become `snake_case`, `kFooBar` constants become
//! `FOO_BAR`. Upstream overloads get suffixes: poly_int versions of
//! [`poly_utils`] functions end in `_int`, and mono versions of [`futils`]
//! functions end in `_mono`. Upstream `utils::mod` is
//! [`poly_utils::modulo`].
//!
//! # Example
//!
//! ```
//! use sas_simd::{futils, poly_utils, PolyFloat};
//!
//! let phase = PolyFloat::new(0.0, 0.25, 0.5, 0.75);
//! let wrapped = poly_utils::modulo(phase + 1.5);
//! let gain = futils::db_to_magnitude(PolyFloat::splat(-6.0));
//! assert_eq!(wrapped.get(1), 0.75);
//! assert!(gain.get(0) > 0.49 && gain.get(0) < 0.51);
//! ```

// Upstream literals are kept digit for digit so they can be checked against
// the C++ source; Rust rounds them to f32 exactly as the C++ compiler does.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

pub mod backend;
pub mod common;
pub mod futils;
pub mod matrix;
mod poly;
pub mod poly_utils;
pub mod utils;

pub use matrix::Matrix;
pub use poly::{PolyFloatOf, PolyIntOf, SIZE};

/// `vital::poly_float` on the native backend.
pub type PolyFloat = PolyFloatOf<backend::Native>;
/// `vital::poly_int` on the native backend.
pub type PolyInt = PolyIntOf<backend::Native>;
/// `vital::poly_mask` (the same type as [`PolyInt`]).
pub type PolyMask = PolyInt;

/// The [`Scalar`](backend::Scalar)-backend types, for cross-checking.
pub mod scalar {
    /// `poly_float` on the scalar backend.
    pub type PolyFloat = crate::PolyFloatOf<crate::backend::Scalar>;
    /// `poly_int` on the scalar backend.
    pub type PolyInt = crate::PolyIntOf<crate::backend::Scalar>;
    /// `poly_mask` on the scalar backend.
    pub type PolyMask = PolyInt;
}

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M1";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/framework/poly_values.h",
    "vital/src/synthesis/framework/poly_utils.h",
    "vital/src/synthesis/framework/futils.h",
    "vital/src/synthesis/framework/matrix.h",
    "vital/src/synthesis/framework/common.h",
    "vital/src/synthesis/framework/utils.h",
    "vital/src/synthesis/framework/utils.cpp",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_sources_exist() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        for source in UPSTREAM_SOURCES {
            assert!(repo.join(source).exists(), "missing upstream path {source}");
        }
        assert!(MILESTONE.starts_with('M'));
    }
}
