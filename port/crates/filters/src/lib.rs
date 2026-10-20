//! Filter models (analog, dirty, ladder, digital, diode, formant, comb, phaser) and building blocks.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M5
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

pub use sas_dsp_core;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M5";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/filters/",
    "vital/src/synthesis/modules/filter_module.cpp",
    "vital/src/synthesis/modules/filters_module.cpp",
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
