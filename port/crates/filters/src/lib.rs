// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Filter models (analog, dirty, ladder, digital, diode, formant, comb,
//! phaser) and building blocks.
//!
//! Ports `vital/src/synthesis/filters/` and the filter modules
//! (`modules/filter_module.cpp`, `modules/filters_module.cpp`). Every model
//! is a `sas_dsp_core` processor kernel implementing
//! [`synth_filter::SynthFilter`]; [`synth_filter::create_filter`] is
//! `SynthFilter::createFilter`.
//!
//! The core models and building blocks are top-level modules (M5a). The
//! character models (comb, formant, phaser) live under [`character`] (M5b).
//!
//! # Numerics
//!
//! Straightforward translations of upstream: no FMA contraction, no
//! estimate instructions, upstream's operation order. Per-ISA behaviour
//! exists only at the sites of `migration/decisions/reference-arm.md`,
//! inside `sas-simd` (for example `toInt` in the coefficient lookups); the
//! `aarch64-semantics` feature forwards to `sas-dsp-core` and `sas-simd`.

// Indexed loops mirror upstream's.
#![allow(clippy::needless_range_loop)]

pub use sas_dsp_core;

pub mod character;
pub mod dc_filter;
pub mod decimator;
pub mod digital_svf;
pub mod diode_filter;
pub mod dirty_filter;
pub mod fir_halfband_decimator;
pub mod iir_halfband_decimator;
pub mod ladder_filter;
pub mod linkwitz_riley_filter;
pub mod sallen_key_filter;
pub mod synth_filter;
pub mod upsampler;

pub use dc_filter::DcFilter;
pub use decimator::Decimator;
pub use digital_svf::DigitalSvf;
pub use diode_filter::DiodeFilter;
pub use dirty_filter::DirtyFilter;
pub use fir_halfband_decimator::FirHalfbandDecimator;
pub use iir_halfband_decimator::IirHalfbandDecimator;
pub use ladder_filter::LadderFilter;
pub use linkwitz_riley_filter::LinkwitzRileyFilter;
pub use sallen_key_filter::SallenKeyFilter;
pub use synth_filter::{
    coefficient_lookup, create_filter, CoefficientLookup, FilterModel, FilterState, SynthFilter,
};
pub use upsampler::Upsampler;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M5";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/filters/",
    "vital/src/synthesis/filters/synth_filter.h",
    "vital/src/synthesis/filters/synth_filter.cpp",
    "vital/src/synthesis/filters/digital_svf.h",
    "vital/src/synthesis/filters/digital_svf.cpp",
    "vital/src/synthesis/filters/linkwitz_riley_filter.h",
    "vital/src/synthesis/filters/linkwitz_riley_filter.cpp",
    "vital/src/synthesis/filters/dc_filter.h",
    "vital/src/synthesis/filters/dc_filter.cpp",
    "vital/src/synthesis/filters/diode_filter.h",
    "vital/src/synthesis/filters/diode_filter.cpp",
    "vital/src/synthesis/filters/dirty_filter.h",
    "vital/src/synthesis/filters/dirty_filter.cpp",
    "vital/src/synthesis/filters/ladder_filter.h",
    "vital/src/synthesis/filters/ladder_filter.cpp",
    "vital/src/synthesis/filters/sallen_key_filter.h",
    "vital/src/synthesis/filters/sallen_key_filter.cpp",
    "vital/src/synthesis/filters/upsampler.h",
    "vital/src/synthesis/filters/upsampler.cpp",
    "vital/src/synthesis/filters/decimator.h",
    "vital/src/synthesis/filters/decimator.cpp",
    "vital/src/synthesis/filters/iir_halfband_decimator.h",
    "vital/src/synthesis/filters/iir_halfband_decimator.cpp",
    "vital/src/synthesis/filters/fir_halfband_decimator.h",
    "vital/src/synthesis/filters/fir_halfband_decimator.cpp",
    "vital/src/synthesis/modules/filter_module.cpp",
    "vital/src/synthesis/modules/filters_module.cpp",
    "vital/src/synthesis/filters/phaser_filter.h",
    "vital/src/synthesis/filters/phaser_filter.cpp",
    "vital/src/synthesis/filters/comb_filter.h",
    "vital/src/synthesis/filters/comb_filter.cpp",
    "vital/src/synthesis/modules/comb_module.h",
    "vital/src/synthesis/modules/comb_module.cpp",
    "vital/src/synthesis/filters/formant_filter.h",
    "vital/src/synthesis/filters/formant_filter.cpp",
    "vital/src/synthesis/filters/formant_manager.h",
    "vital/src/synthesis/filters/formant_manager.cpp",
    "vital/src/synthesis/filters/vocal_tract.h",
    "vital/src/synthesis/filters/vocal_tract.cpp",
    "vital/src/synthesis/modules/formant_module.h",
    "vital/src/synthesis/modules/formant_module.cpp",
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
