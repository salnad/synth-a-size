// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Effect chain: chorus, compressor, delay, distortion, EQ, filter FX, flanger, phaser, reverb.
//!
//! Ports upstream's `vital/src/synthesis/effects/` and the effect modules
//! in `vital/src/synthesis/modules/`. Each file names the upstream file it
//! ports. Processors are [`sas_dsp_core::ProcessorImpl`] kernels; effect
//! modules are `SynthModule` nodes whose controls come from the parameter
//! registry installed with
//! [`sas_dsp_core::synth_module::set_parameter_lookup`].
//!
//! # Per-ISA sites
//!
//! Delay-line reads go through [`sas_dsp_core::Memory`], whose read index
//! uses `utils::toInt` (site A1a of `migration/decisions/reference-arm.md`).
//! The `aarch64-semantics` feature selects upstream's arm64 behaviour on
//! every target.

pub use sas_dsp_core;
pub use sas_filters;

pub mod delay;
pub mod delay_module;
pub mod reverb;
pub mod reverb_module;

pub use delay::{Delay, MultiDelay, StereoDelay};
pub use delay_module::DelayModule;
pub use reverb::Reverb;
pub use reverb_module::ReverbModule;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M6";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/effects/",
    "vital/src/synthesis/effects/delay.h",
    "vital/src/synthesis/effects/delay.cpp",
    "vital/src/synthesis/modules/delay_module.h",
    "vital/src/synthesis/modules/delay_module.cpp",
    "vital/src/synthesis/effects/reverb.h",
    "vital/src/synthesis/effects/reverb.cpp",
    "vital/src/synthesis/modules/reverb_module.h",
    "vital/src/synthesis/modules/reorderable_effect_chain.cpp",
    "vital/src/synthesis/modules/chorus_module.cpp",
    "vital/src/synthesis/modules/reverb_module.cpp",
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
