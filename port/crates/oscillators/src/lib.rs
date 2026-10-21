// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Wavetable oscillator, unison, spectral morph, distortion modes, sample player.
//!
//! - [`synth_oscillator`]: `SynthOscillator` and `RandomValues`
//!   (`vital/src/synthesis/producers/synth_oscillator.{h,cpp}`);
//! - [`spectral_morph`]: the spectral morph functions
//!   (`vital/src/synthesis/producers/spectral_morph.h`).
//!
//! Numerics follow `migration/AGENTS.md`: the straightforward translation of
//! upstream's source formulas, with the per-target sites of
//! `migration/decisions/reference-arm.md` going through `sas-simd`
//! (see [`synth_oscillator`] for the list).

// Upstream literals are kept digit for digit so they can be checked against
// the C++ source.
#![allow(clippy::excessive_precision)]

pub use sas_dsp_core;
pub use sas_wavetable;

pub mod spectral_morph;
pub mod synth_oscillator;

pub use synth_oscillator::{RandomValues, SharedWavetable, SynthOscillator};

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M4";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/producers/synth_oscillator.h",
    "vital/src/synthesis/producers/synth_oscillator.cpp",
    "vital/src/synthesis/producers/spectral_morph.h",
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
