//! Effect chain: chorus, compressor, delay, distortion, EQ, filter FX, flanger, phaser, reverb.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M6
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

pub use sas_dsp_core;
pub use sas_filters;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M6";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/effects/",
    "vital/src/synthesis/modules/reorderable_effect_chain.cpp",
    "vital/src/synthesis/modules/chorus_module.cpp",
    "vital/src/synthesis/modules/delay_module.cpp",
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
