// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sound engine: voice handling, MIDI/MPE, tuning, full patch graph, offline render.
//!
//! Milestone M7 (see `migration/PLAN.md`). Ported so far:
//!
//! - [`voice_handler`]: `vital::VoiceHandler`, `Voice` and `NoteHandler`
//!   (`framework/voice_handler.*`, `note_handler.h`): allocation, voice
//!   priority and override (stealing), legato, sustain and sostenuto, MPE
//!   per-channel pitch bend, pressure and slide, and the per-voice-pair graph
//!   copies;
//! - [`tuning`]: `Tuning` (`common/tuning.*`): Scala `.scl`, keyboard maps
//!   `.kbm` and AnaMark `.tun`, and the JSON state.
//!
//! - [`synth_voice_handler`](mod@synth_voice_handler): `SynthVoiceHandler`
//!   (`modules/synth_voice_handler.*`), built so far from note articulation,
//!   macros and expression sources; see its docs for what follows;
//! - [`parameters`]: the `sas-preset` registry as the graph's
//!   `Parameters::getDetails`;
//! - [`edit_bus`]: live edits from the editor and the chat layer (modulation
//!   routing, shapes, wavetables, the sample) and the producers' mirror of
//!   the [`document::PresetDocument`] (milestone G0 of the GUI plan).
//!
//! The upstream per-ISA sites (`migration/decisions/reference-arm.md`) are
//! reached only through `sas-simd`; the `aarch64-semantics` feature forwards
//! to every direct dependency that has it.

pub use sas_dsp_core;
pub use sas_effects;
pub use sas_filters;
pub use sas_modulation;
pub use sas_oscillators;

pub mod document;
pub mod edit_bus;
pub mod midi_file;
pub mod midi_manager;
pub mod modulation_bank;
pub mod parameters;
mod slot_queue;
pub mod sound_engine;
pub mod synth;
pub mod synth_voice_handler;
pub mod tuning;
pub mod voice_handler;

pub use synth_voice_handler::SynthVoiceHandler;
pub use tuning::Tuning;
pub use voice_handler::{NoteHandler, VoiceHandler};

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M7";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/synthesis/framework/voice_handler.h",
    "vital/src/synthesis/framework/voice_handler.cpp",
    "vital/src/synthesis/framework/note_handler.h",
    "vital/src/synthesis/framework/circular_queue.h",
    "vital/src/common/tuning.h",
    "vital/src/common/tuning.cpp",
    "vital/src/synthesis/synth_engine/",
    "vital/src/synthesis/modules/synth_voice_handler.cpp",
    "vital/src/common/midi_manager.cpp",
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
