//! Parameter registry (names, ranges, scales, defaults) and preset JSON load/save (upstream preset format) with version upgrades.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M2
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M2";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/common/synth_parameters.cpp",
    "vital/src/common/load_save.cpp",
    "vital/src/common/synth_base.cpp",
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
