//! Modulators (envelopes, LFOs, random LFOs, macros) and the modulation matrix.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M3
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

pub use sas_dsp_core;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M3";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/modulators/",
    "vital/src/synthesis/modules/envelope_module.cpp",
    "vital/src/synthesis/modules/lfo_module.cpp",
    "vital/src/synthesis/modules/modulation_connection_processor.cpp",
    "vital/src/common/line_generator.cpp",
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
