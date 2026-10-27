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
//! - [`midi_learn`]: MIDI learn requests from the editor, and the learn map
//!   saved to the user [`config`] file.
//! - [`live`]: the LLM tools' session on the running plugin (host parameter
//!   gestures, the edit bus, undo, outside-change detection; D13), and
//!   [`chat_bridge`], how the chat worker reaches it on the GUI thread.
//! - [`chat`]: the chat behind the editor's panel (off until the user turns
//!   it on), with [`keys`] for its API key.
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

pub mod audio_file;
pub mod chat;
pub mod chat_bridge;
pub mod config;
pub mod editor;
pub mod engine;
pub mod keys;
pub mod live;
pub mod midi_learn;
pub mod params;
pub mod processor;
pub mod render;
pub mod resynthesis;
pub mod sound_engine;
pub mod state;

pub use engine::{Engine, SilentEngine};
pub use midi_learn::MidiLearn;
pub use params::SynthParams;
pub use processor::{BlockOutcome, BlockProcessor, Transport};
pub use sound_engine::SoundEngineHost;
pub use state::PresetDocument;

/// Milestone in `migration/PLAN.md` that makes this a working instrument.
pub const MILESTONE: &str = "M8";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/plugin/synth_plugin.cpp",
    "vital/src/plugin/synth_plugin.h",
    "vital/src/plugin/value_bridge.h",
    "vital/src/common/load_save.cpp",
];

/// Work the audio thread hands to nice-plug's background thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginTask {
    /// Save the MIDI learn map to the user config
    /// (`LoadSave::saveMidiMapConfig`).
    SaveMidiLearnConfig,
}

/// The plugin around engine `E`.
pub struct SynthPlugin<E: Engine> {
    params: Arc<SynthParams>,
    processor: BlockProcessor<E>,
    telemetry: Telemetry,
    keyboard: KeyboardHandle,
    /// The editor window's size and whether the chat's drawer is open, kept
    /// while the window is closed.
    editor_state: sas_gui::EditorState,
    /// The LLM chat (off until the user turns it on), kept while the window
    /// is closed.
    chat: Arc<chat::PluginChat>,
}

/// The plugin as shipped.
pub type SynthASize = SynthPlugin<SoundEngineHost>;

impl<E: Engine> SynthPlugin<E> {
    /// A plugin around `engine` as constructed. Upstream's `SynthPlugin`
    /// never calls `loadInitPreset`: the engine starts in its constructed
    /// state, which the init document describes, and nothing more is drawn
    /// from the engine's random seed counter (a draw here would shift every
    /// later one, and the sound with it). No MIDI learn map is loaded or
    /// saved, and the chat reads no settings file or Keychain; see
    /// [`with_midi_learn`](Self::with_midi_learn) and
    /// [`with_chat`](Self::with_chat).
    pub fn new(engine: E) -> SynthPlugin<E> {
        SynthPlugin::with_parts(
            engine,
            Arc::new(MidiLearn::in_memory()),
            chat::PluginChat::in_memory,
        )
    }

    /// [`new`](Self::new), with MIDI learn loaded from and saved to
    /// `midi_learn`'s config file.
    pub fn with_midi_learn(engine: E, midi_learn: Arc<MidiLearn>) -> SynthPlugin<E> {
        SynthPlugin::with_parts(engine, midi_learn, chat::PluginChat::in_memory)
    }

    /// [`new`](Self::new), with the chat `chat` builds for the parameters.
    pub fn with_chat(
        engine: E,
        chat: impl FnOnce(Arc<SynthParams>) -> chat::PluginChat,
    ) -> SynthPlugin<E> {
        SynthPlugin::with_parts(engine, Arc::new(MidiLearn::in_memory()), chat)
    }

    fn with_parts(
        engine: E,
        midi_learn: Arc<MidiLearn>,
        chat: impl FnOnce(Arc<SynthParams>) -> chat::PluginChat,
    ) -> SynthPlugin<E> {
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
        let equalizer_memory = engine.equalizer_memory().unwrap_or_default();
        let (writer, telemetry) = Telemetry::with_equalizer_memory(audio_memory, equalizer_memory);
        let (keyboard, keyboard_consumer) = keyboard();
        let params = Arc::new(SynthParams::new(edits));
        // SynthEditor's constructor: a new editor opens at the size last
        // saved, in any instance.
        let window_size = midi_learn.config_file().map_or(1.0, config::window_size);
        SynthPlugin {
            chat: Arc::new(chat(params.clone())),
            params,
            processor: BlockProcessor::with_midi_learn(
                engine,
                consumer,
                keyboard_consumer,
                writer,
                midi_learn,
            ),
            telemetry,
            keyboard,
            editor_state: sas_gui::state_for_window_size(window_size),
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

impl<E: Engine + Default> SynthPlugin<E> {
    /// The default engine with no user files: no saved MIDI learn map is
    /// loaded and nothing is saved, and the chat reads no settings file or
    /// Keychain. Tests use this; hosts get [`Default`], which reads them.
    pub fn in_memory() -> SynthPlugin<E> {
        SynthPlugin::new(E::default())
    }
}

impl<E: Engine + Default> Default for SynthPlugin<E> {
    /// The plugin as a host creates it: MIDI learn uses the user config file
    /// (`SynthBase`'s constructor loads it through `Startup`), and the chat
    /// the user's settings file and the Keychain (still off until the user
    /// turns it on).
    fn default() -> Self {
        let midi_learn = match config::config_file() {
            Some(path) => MidiLearn::with_config_file(path),
            None => MidiLearn::in_memory(),
        };
        SynthPlugin::with_parts(E::default(), Arc::new(midi_learn), chat::PluginChat::new)
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
    type BackgroundTask = PluginTask;

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let midi_learn = self.processor.midi_learn().clone();
        Box::new(move |task| match task {
            PluginTask::SaveMidiLearnConfig => midi_learn.save_config(),
        })
    }

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        // Called from the host's `createView`, which cannot unwind: a panic
        // here means no editor, not a crashed host.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let model = editor::PluginModel::new(
                self.params.clone(),
                self.telemetry(),
                self.keyboard.clone(),
                self.processor.midi_learn().clone(),
                Some(self.chat.clone()),
            );
            sas_gui::create_editor(self.editor_state.clone(), Arc::new(model))
        }))
        .unwrap_or_else(|_| {
            nice_plug::nice_error!("creating the editor panicked; the plugin has no editor");
            None
        })
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
            let outcome = self
                .processor
                .process(&self.params, left, right, &transport, || {
                    context.next_event()
                });
            if outcome.save_midi_learn {
                context.execute_background(PluginTask::SaveMidiLearnConfig);
            }
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

    /// `SynthEditor`'s constructor: a new instance's editor opens at the
    /// size saved in the config, and at upstream's default without one.
    #[test]
    fn a_new_instance_opens_at_the_saved_window_size() {
        let size = |plugin: &SynthASize| {
            let size = plugin.editor_state.window.size().to_logical::<f64>(1.0);
            (size.width, size.height)
        };
        let dir = std::env::temp_dir().join(format!("sas-plugin-window-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        config::save_window_size(&path, 1.5).unwrap();
        let midi_learn = Arc::new(MidiLearn::with_config_file(path));
        let plugin = SynthPlugin::with_midi_learn(SoundEngineHost::default(), midi_learn);
        assert_eq!(size(&plugin), (2100.0, 1230.0));
        assert_eq!(size(&SynthASize::in_memory()), (1400.0, 820.0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

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
