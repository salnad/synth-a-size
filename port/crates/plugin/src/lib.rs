//! VST3 / CLAP instrument wrapper, built on nice-plug (the maintained
//! community fork of nih-plug; see `migration/decisions/plugin-framework.md`).
//!
//! Stub: the plugin loads in a host, accepts MIDI and outputs silence. The
//! engine is wired in at milestone M8 (see `migration/PLAN.md`).

use std::num::NonZeroU32;
use std::sync::Arc;

use nice_plug::prelude::*;

/// Milestone in `migration/PLAN.md` that makes this a working instrument.
pub const MILESTONE: &str = "M8";

pub struct SynthASize {
    params: Arc<SynthASizeParams>,
}

#[derive(Params)]
pub struct SynthASizeParams {
    #[id = "volume"]
    pub volume: FloatParam,
}

impl Default for SynthASizeParams {
    fn default() -> Self {
        Self {
            volume: FloatParam::new("Volume", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
        }
    }
}

impl Default for SynthASize {
    fn default() -> Self {
        Self {
            params: Arc::new(SynthASizeParams::default()),
        }
    }
}

impl Plugin for SynthASize {
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

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type Editor = ();
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        while context.next_event().is_some() {}
        for channel in buffer.as_slice() {
            channel.fill(0.0);
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for SynthASize {
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

impl Vst3Plugin for SynthASize {
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
    fn plugin_metadata_avoids_upstream_names() {
        for text in [SynthASize::NAME, SynthASize::VENDOR, SynthASize::CLAP_ID] {
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
