//! Processor graph core: processors, routers, inputs/outputs, values, lookup tables, shared utilities.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M1
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

pub use sas_simd;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M1";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/framework/",
    "vital/src/synthesis/lookups/lookup_table.h",
    "vital/src/synthesis/utilities/",
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
