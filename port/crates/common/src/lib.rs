// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Dependency-free building blocks shared by several crates, each ported to
//! match the reference exactly:
//!
//! - [`json`]: JSON with the upstream serializer's semantics (sorted keys,
//!   typed numbers, its double formatting), so saved presets and wavetables
//!   match byte for byte;
//! - [`base64`]: JUCE's base64, used for buffers inside that JSON;
//! - [`scalar`]: single-lane versions of the `futils`/`utils` math helpers;
//! - [`random`]: the engine RNG (`std::mt19937` through
//!   `uniform_real_distribution<float>`, per target C++ library);
//! - [`line_generator`]: `LineGenerator` (LFO shapes, remap curves, the
//!   wavetable Line Source);
//! - [`load_save`]: version comparison and buffer re-encoding used by JSON
//!   upgrades.
//!
//! This crate depends on nothing, so the preset, wavetable, modulation and
//! SIMD crates can all use it.

pub mod base64;
pub mod json;
pub mod line_generator;
pub mod load_save;
pub mod random;
pub mod scalar;

pub use line_generator::LineGenerator;
pub use random::RandomGenerator;

/// Whether this build follows the aarch64 reference (macOS arm64, libc++)
/// where it differs from the x86_64 one: true on aarch64, and on other
/// targets with the `aarch64-semantics` feature (the same switch as
/// `sas-simd`, whose feature forwards to this one). The sites here are
/// [`random::canonical_from_draw`] and [`scalar::to_int`].
pub const AARCH64_SEMANTICS: bool =
    cfg!(any(target_arch = "aarch64", feature = "aarch64-semantics"));

/// Milestone in `migration/PLAN.md` that introduced this crate.
pub const MILESTONE: &str = "M4";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/common/line_generator.cpp",
    "vital/src/common/line_generator.h",
    "vital/src/common/load_save.cpp",
    "vital/src/synthesis/framework/futils.h",
    "vital/src/synthesis/framework/utils.h",
    "vital/src/synthesis/framework/utils.cpp",
    "vital/src/synthesis/framework/poly_utils.h",
    "vital/third_party/json/json.h",
    "vital/third_party/JUCE/modules/juce_core/text/juce_Base64.cpp",
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
