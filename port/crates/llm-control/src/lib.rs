//! LLM control layer: set named parameters, connect modulations by name, generate preset and wavetable JSON, render offline.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M9
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

pub use sas_preset;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M9";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/common/synth_parameters.cpp",
    "vital/src/common/wavetable/wavetable_creator.cpp",
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
