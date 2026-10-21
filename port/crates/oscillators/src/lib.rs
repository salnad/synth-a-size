// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Wavetable oscillator, unison, spectral morph, distortion modes, sample player.
//!
//! - [`synth_oscillator`]: `SynthOscillator` and `RandomValues`
//!   (`vital/src/synthesis/producers/synth_oscillator.{h,cpp}`);
//! - [`spectral_morph`]: the spectral morph functions
//!   (`vital/src/synthesis/producers/spectral_morph.h`);
//! - [`sample_source`]: `Sample` and the `SampleSource` player
//!   (`vital/src/synthesis/producers/sample_source.{h,cpp}`);
//! - [`oscillator_module`], [`sample_module`], [`producers_module`]: the
//!   engine modules around them (`vital/src/synthesis/modules/`), whose
//!   controls come from the parameter registry
//!   ([`sas_dsp_core::synth_module::set_parameter_lookup`]).
//!
//! Numerics follow `migration/AGENTS.md`: the straightforward translation of
//! upstream's source formulas, with the per-target sites of
//! `migration/decisions/reference-arm.md` going through `sas-simd`
//! (see [`synth_oscillator`] for the list).

// Upstream literals are kept digit for digit so they can be checked against
// the C++ source.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

pub use sas_dsp_core;
pub use sas_wavetable;

pub mod oscillator_module;
pub mod producers_module;
pub mod sample_module;
pub mod sample_source;
pub mod spectral_morph;
pub mod synth_oscillator;

pub use sample_source::{Sample, SampleSource, SharedSample};
pub use synth_oscillator::{RandomValues, SharedWavetable, SynthOscillator};

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M4";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/producers/synth_oscillator.h",
    "vital/src/synthesis/producers/synth_oscillator.cpp",
    "vital/src/synthesis/producers/spectral_morph.h",
    "vital/src/synthesis/producers/sample_source.h",
    "vital/src/synthesis/producers/sample_source.cpp",
    "vital/src/synthesis/modules/oscillator_module.h",
    "vital/src/synthesis/modules/oscillator_module.cpp",
    "vital/src/synthesis/modules/sample_module.h",
    "vital/src/synthesis/modules/sample_module.cpp",
    "vital/src/synthesis/modules/producers_module.h",
    "vital/src/synthesis/modules/producers_module.cpp",
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
