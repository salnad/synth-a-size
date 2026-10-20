//! Wavetable oscillator, unison, spectral morph, distortion modes, sample player.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M4
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

pub use sas_dsp_core;
pub use sas_wavetable;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M4";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/producers/",
    "vital/src/synthesis/modules/oscillator_module.cpp",
    "vital/src/synthesis/modules/sample_module.cpp",
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
