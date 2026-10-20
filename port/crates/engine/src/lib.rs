//! Sound engine: voice handling, MIDI/MPE, tuning, full patch graph, offline render.
//!
//! Stub crate: nothing is ported yet. Porting starts in milestone M7
//! (see `migration/PLAN.md`); follow `migration/AGENTS.md`.

pub use sas_dsp_core;
pub use sas_effects;
pub use sas_filters;
pub use sas_modulation;
pub use sas_oscillators;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M7";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/synth_engine/",
    "vital/src/synthesis/modules/synth_voice_handler.cpp",
    "vital/src/synthesis/framework/voice_handler.cpp",
    "vital/src/common/midi_manager.cpp",
    "vital/src/common/tuning.cpp",
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
