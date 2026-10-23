// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! VST3 instrument wrapper, built on nice-plug (the maintained community fork
//! of nih-plug; see `migration/decisions/plugin-framework.md`). Ports the
//! behaviour of `vital/src/plugin/synth_plugin.cpp`; the editor is `sas-gui`.
//!
//! - [`params`]: one host parameter per engine control of the `sas-preset`
//!   registry, with the registry name as stable ID, and `bypass` as the
//!   host bypass parameter.
//! - [`state`]: host state saved and restored as preset JSON (with the
//!   upgrade chain), and preset file load/save.
//! - [`engine`]: the engine interface the wrapper drives ([`SilentEngine`]
//!   for tests that need none).
//! - [`processor`]: the audio callback, MIDI/MPE in, and the edit bus
//!   (`sas_engine::edit_bus`) drained at the start of each block.
//! - [`sound_engine`]: the M7 sound engine ([`SoundEngineHost`]) behind
//!   [`Engine`]; the shipped plugin, [`SynthASize`], runs on it.
//! - [`render`]: offline rendering like the reference's `ref-render`, behind
//!   the `sas-render` binary.
//! - [`editor`]: what the `sas-gui` editor reads from the plugin.
//!
//! The shipping build is an arm64 macOS VST3 (`scripts/bundle.sh`).
//! CLAP export is kept because it costs nothing, but it is not tested or
//! shipped.

use std::num::NonZeroU32;
use std::sync::Arc;

use nice_plug::prelude::*;
use sas_engine::edit_bus::{EditBus, EditHandle};
use sas_engine::keyboard::{keyboard, KeyboardHandle};
use sas_engine::telemetry::{AudioMemory, Telemetry};
use sas_preset::InitContent;

pub mod editor;
pub mod engine;
pub mod params;
pub mod processor;
pub mod render;
pub mod sound_engine;
pub mod state;

pub use engine::{Engine, SilentEngine};
pub use params::SynthParams;
pub use processor::{BlockProcessor, Transport};
pub use sound_engine::SoundEngineHost;
pub use state::PresetDocument;

/// Milestone in `migration/PLAN.md` that makes this a working instrument.
pub const MILESTONE: &str = "M8";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/plugin/synth_plugin.cpp",
    "vital/src/plugin/synth_plugin.h",
    "vital/src/plugin/value_bridge.h",
];

/// The plugin around engine `E`.
pub struct SynthPlugin<E: Engine> {
    params: Arc<SynthParams>,
    processor: BlockProcessor<E>,
    telemetry: Telemetry,
    keyboard: KeyboardHandle,
    /// The editor window's size, kept while the window is closed.
    editor_state: Arc<sas_gui::nice_plug_egui::EguiEditorState>,
}

/// The plugin as shipped.
pub type SynthASize = SynthPlugin<SoundEngineHost>;

impl<E: Engine> SynthPlugin<E> {
    /// A plugin around `engine` as constructed. Upstream's `SynthPlugin`
    /// never calls `loadInitPreset`: the engine starts in its constructed
    /// state, which the init document describes, and nothing more is drawn
    /// from the engine's random seed counter (a draw here would shift every
    /// later one, and the sound with it).
    pub fn new(engine: E) -> SynthPlugin<E> {
        // The init state carries the engine's own sample; an engine that
        // can't report one gets white noise from a local seed, never from
        // the global counter.
        let mut document = PresetDocument::init(&InitContent { sample_seed: 0 });
        if let Some(sample) = engine.engine_sample() {
            document.state.sample = sample;
        }
        let (edits, consumer) = EditBus::new(document);
        let audio_memory = engine
            .audio_memory()
            .unwrap_or_else(|| Arc::new(AudioMemory::default()));
        let (writer, telemetry) = Telemetry::new(audio_memory);
        let (keyboard, keyboard_consumer) = keyboard();
        SynthPlugin {
            params: Arc::new(SynthParams::new(edits)),
            processor: BlockProcessor::new(engine, consumer, keyboard_consumer, writer),
            telemetry,
            keyboard,
            editor_state: sas_gui::default_state(),
        }
    }

    /// What the editor shows of the playing engine (`sas_engine::telemetry`).
    pub fn telemetry(&self) -> Telemetry {
        self.telemetry.clone()
    }

    /// The on-screen keyboard (`sas_engine::keyboard`).
    pub fn keyboard_handle(&self) -> KeyboardHandle {
        self.keyboard.clone()
    }

    /// The edit bus, for the editor and the chat worker.
    pub fn edit_handle(&self) -> EditHandle {
        self.params.edit_handle().clone()
    }

    pub fn synth_params(&self) -> &Arc<SynthParams> {
        &self.params
    }

    pub fn processor(&self) -> &BlockProcessor<E> {
        &self.processor
    }

    pub fn processor_mut(&mut self) -> &mut BlockProcessor<E> {
        &mut self.processor
    }
}

impl<E: Engine + Default> Default for SynthPlugin<E> {
    fn default() -> Self {
        SynthPlugin::new(E::default())
    }
}

impl<E: Engine + Default> Plugin for SynthPlugin<E> {
    const NAME: &'static str = "synth-a-size";
    const VENDOR: &'static str = "synth-a-size contributors";
    const URL: &'static str = "https://github.com/salnad/synth-a-size";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    // Notes, poly and channel pressure, pitch bend and CCs on all 16
    // channels: what upstream's MidiManager reads, MPE included.
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    // Upstream's JUCE VST3 wrapper applies only the last point of each
    // parameter queue, once, before processBlock
    // (juce_VST3_Wrapper.cpp, processParameterChanges). Without
    // sample-accurate automation nice-plug applies every point before
    // `process`, leaving the last, and never splits the block.
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type Editor = sas_gui::Editor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        let model = editor::PluginModel::new(self.params.clone(), self.keyboard.clone());
        sas_gui::create_editor(self.editor_state.clone(), Arc::new(model))
    }

    fn filter_state(state: &mut PluginState) {
        state::filter_state(state);
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.processor
            .activate(&self.params, buffer_config.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.processor.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let host = context.transport();
        let transport = Transport {
            tempo: host.tempo,
            playing: host.playing,
            pos_beats: host.pos_beats(),
        };
        if let [left, right, ..] = buffer.as_slice() {
            self.processor
                .process(&self.params, left, right, &transport, || {
                    context.next_event()
                });
        }
        ProcessStatus::KeepAlive
    }
}

impl<E: Engine + Default> ClapPlugin for SynthPlugin<E> {
    const CLAP_ID: &'static str = "io.github.salnad.synth-a-size";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Wavetable synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl<E: Engine + Default> Vst3Plugin for SynthPlugin<E> {
    const VST3_CLASS_ID: [u8; 16] = *b"SynthASizePort01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nice_export_clap!(SynthASize);
nice_export_vst3!(SynthASize);

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

    #[test]
    fn plugin_metadata_avoids_upstream_names() {
        let class_id = String::from_utf8_lossy(&SynthASize::VST3_CLASS_ID).into_owned();
        for text in [
            SynthASize::NAME,
            SynthASize::VENDOR,
            SynthASize::URL,
            SynthASize::CLAP_ID,
            class_id.as_str(),
        ] {
            let lower = text.to_lowercase();
            for banned in ["vital", "tytel"] {
                assert!(!lower.contains(banned), "{text} contains {banned}");
            }
        }
    }

    #[test]
    fn automation_applies_once_per_block_like_upstream() {
        // Sample-accurate automation would split host blocks at every
        // automation point and render different audio from upstream.
        const { assert!(!SynthASize::SAMPLE_ACCURATE_AUTOMATION) };
    }

    #[test]
    fn instrument_layout_is_stereo_out_only() {
        let layout = &SynthASize::AUDIO_IO_LAYOUTS[0];
        assert!(layout.main_input_channels.is_none());
        assert_eq!(layout.main_output_channels, NonZeroU32::new(2));
    }
}
