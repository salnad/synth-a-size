//! 4-lane SIMD value types (poly_float / poly_int / poly_mask) and helpers.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M1
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M1";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/framework/poly_values.h",
    "vital/src/synthesis/framework/poly_utils.h",
    "vital/src/synthesis/framework/futils.h",
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
