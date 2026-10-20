//! Plugin editor (egui or vizia, decided in M10). The upstream OpenGL GUI is not ported line by line.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M10
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M10";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &["vital/src/interface/"];

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
