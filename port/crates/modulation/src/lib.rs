// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Modulators (envelopes, LFOs, random LFOs) and the modulation matrix.
//!
//! Ports upstream's `vital/src/synthesis/modulators/*` and the
//! envelope, LFO, random LFO and modulation-connection modules. The line
//! shapes (`LineGenerator`) and the engine RNG are [`sas_common`]'s.
//!
//! Each processor is a [`sas_dsp_core::ProcessorImpl`] kernel with a
//! `create` constructor, as described in [`sas_dsp_core`]. Modules
//! (`*Module`) are `SynthModule` nodes; their controls come from the
//! parameter registry lookup
//! ([`sas_dsp_core::synth_module::set_parameter_lookup`]), so build them
//! after the engine has installed it, and call `init` on them as the parent
//! router would.
//!
//! The macro, mod wheel, pitch wheel, velocity, aftertouch and note sources
//! are created by the voice handler (`synth_voice_handler.cpp`), not by
//! these modules, so they are ported with it in `sas-engine` (M7).
//!
//! # Numerics
//!
//! Straight translations of upstream's arithmetic, through [`sas_simd`] for
//! every per-ISA site (`toInt`, the lane min/max, `anyMask`) and
//! [`sas_common::random`] for the RNG column (A9). Enable the
//! `aarch64-semantics` feature to use the aarch64 column on x86_64.

pub use sas_dsp_core;

pub mod envelope;
pub mod envelope_module;
pub mod lfo_module;
pub mod synth_lfo;

pub use envelope::Envelope;
pub use envelope_module::EnvelopeModule;
pub use lfo_module::LfoModule;
pub use synth_lfo::SynthLfo;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M3";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/modulators/envelope.h",
    "vital/src/synthesis/modulators/envelope.cpp",
    "vital/src/synthesis/modules/envelope_module.h",
    "vital/src/synthesis/modules/envelope_module.cpp",
    "vital/src/synthesis/modulators/synth_lfo.h",
    "vital/src/synthesis/modulators/synth_lfo.cpp",
    "vital/src/synthesis/modules/lfo_module.h",
    "vital/src/synthesis/modules/lfo_module.cpp",
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
