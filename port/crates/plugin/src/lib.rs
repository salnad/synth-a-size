// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! VST3 instrument wrapper, built on nice-plug (the maintained community fork
//! of nih-plug; see `migration/decisions/plugin-framework.md`). Ports the
//! behaviour of `vital/src/plugin/synth_plugin.cpp` (no editor yet).
//!
//! - [`params`]: one host parameter per engine control of the `sas-preset`
//!   registry, with the registry name as stable ID, and `bypass` as the
//!   host bypass parameter.
//! - [`state`]: host state saved and restored as preset JSON (with the
//!   upgrade chain), and preset file load/save.
//! - [`engine`]: the engine interface the wrapper drives; [`SilentEngine`]
//!   stands in until the M7 sound engine lands.
//! - [`processor`]: the audio callback, MIDI/MPE in.
//!
//! The shipping build is an arm64 macOS VST3 (`scripts/bundle-macos.sh`).
//! CLAP export is kept because it costs nothing, but it is not tested or
//! shipped.

use std::num::NonZeroU32;
use std::sync::Arc;

use nice_plug::prelude::*;
use sas_preset::InitContent;

pub mod engine;
pub mod params;
pub mod processor;
pub mod state;

pub use engine::{Engine, SilentEngine};
pub use params::SynthParams;
pub use processor::{BlockProcessor, Transport};
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
}

/// The plugin as shipped.
pub type SynthASize = SynthPlugin<SilentEngine>;

impl<E: Engine> SynthPlugin<E> {
    /// A plugin on the init preset (`SynthBase::loadInitPreset`, whose
    /// white-noise sample takes the next engine seed).
    pub fn new(engine: E) -> SynthPlugin<E> {
        let init = InitContent {
            sample_seed: sas_common::random::take_next_seed(),
        };
        SynthPlugin {
            params: Arc::new(SynthParams::new(PresetDocument::init(&init))),
            processor: BlockProcessor::new(engine),
        }
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
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
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
        self.processor.set_sample_rate(buffer_config.sample_rate);
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
    fn instrument_layout_is_stereo_out_only() {
        let layout = &SynthASize::AUDIO_IO_LAYOUTS[0];
        assert!(layout.main_input_channels.is_none());
        assert_eq!(layout.main_output_channels, NonZeroU32::new(2));
    }
}
