// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin editor: upstream's interface (`vital/src/interface/`) rebuilt
//! with egui, drawn with wgpu (Metal on macOS).
//!
//! Upstream's layout code, skin and fonts are ported directly, so sizes,
//! positions and colours match upstream's. Each module names the upstream
//! file it ports. The rendering itself is new: upstream draws with JUCE and
//! OpenGL. The plan is `gui-plan.md` (G1–G7) and the framework decision is
//! `migration/decisions/gui-framework.md`.

pub mod chat;
pub mod colour;
pub mod controls;
pub mod editor;
#[cfg(feature = "focus-spike")]
mod focus_spike;
pub mod fonts;
pub mod host;
pub mod interface;
pub mod layer;
pub mod layout;
pub mod paint;
pub mod popup;
pub mod primitives;
pub mod scope;
pub mod section;
pub mod sections;
pub mod skin;

pub use chat::{
    ChatHost, ChatPanel, ChatPlacement, ChatSettingsView, ChatStyle, ChatView, FocusMode,
};
pub use editor::{create as create_editor, default_state, Editor};
pub use host::{Host, HostParam, OpenGestures};
pub use interface::{EditorModel, Interface, Tab, PRODUCT_NAME};
pub use nice_plug_egui;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M10";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/interface/look_and_feel/skin.cpp",
    "vital/src/interface/look_and_feel/fonts.cpp",
    "vital/src/interface/editor_sections/synth_section.cpp",
    "vital/src/interface/editor_sections/full_interface.cpp",
    "vital/src/interface/editor_sections/header_section.cpp",
    "vital/src/interface/editor_components/tab_selector.cpp",
    "vital/src/interface/editor_components/preset_selector.cpp",
    "vital/src/interface/editor_components/synth_preset_selector.cpp",
    "vital/default.vitalskin",
    "vital/fonts/",
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
