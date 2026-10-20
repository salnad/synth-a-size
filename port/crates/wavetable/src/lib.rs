//! Wavetable data and the wavetable creator (sources, modifiers, keyframes, groups) with JSON state.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M4
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

pub use sas_dsp_core;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M4";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/lookups/wavetable.cpp",
    "vital/src/synthesis/lookups/wave_frame.cpp",
    "vital/src/common/wavetable/",
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
