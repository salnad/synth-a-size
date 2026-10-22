// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::SoundEngine`: the synth's top-level module.
//!
//! Ports `vital/src/synthesis/synth_engine/sound_engine.{h,cpp}`: the engine
//! controls, the [`SynthVoiceHandler`], Effects' `ReorderableEffectChain`,
//! the output stage (sum with the direct output, decimate, decode the
//! stereo routing, smooth volume, peak meter, clamp), the status outputs,
//! oversampling and the voice-handler forwarding.

use std::rc::Rc;

use sas_dsp_core::operators::{self, cr};
use sas_dsp_core::utilities::peak_meter::PeakMeter;
use sas_dsp_core::{
    Output, Processor, ProcessorImpl, ProcessorRef, ScopedNoDenormals, SynthModule,
};
use sas_effects::{
    ChorusModule, CompressorModule, Effect, FlangerModule, PhaserModule, ReorderableEffectChain,
};
use sas_filters::Decimator;
use sas_modulation::ModulationConnectionProcessor;

use crate::synth_voice_handler::SynthVoiceHandler;
use crate::tuning::Tuning;
use crate::voice_handler::{with_voice_handler, NoteHandler, VoiceHandler, MAX_POLYPHONY};

/// `kDefaultOversamplingAmount`.
pub const DEFAULT_OVERSAMPLING_AMOUNT: i32 = 2;
/// `kDefaultSampleRate`.
pub const DEFAULT_SAMPLE_RATE: i32 = 44100;
/// `kNumMidiChannels`.
pub const NUM_MIDI_CHANNELS: i32 = 16;

/// Kernel of `vital::SoundEngine`.
pub struct SoundEngine {
    voice_handler: Option<ProcessorRef>,
    effect_chain: Option<ProcessorRef>,
    output_total: Option<ProcessorRef>,
    decimator: Option<ProcessorRef>,
    peak_meter: Option<ProcessorRef>,
    oversampling: Option<ProcessorRef>,
    legato: Option<ProcessorRef>,
    bps: Option<ProcessorRef>,
    last_oversampling_amount: i32,
    last_sample_rate: i32,
}

impl SoundEngine {
    /// `SoundEngine()`: builds the graph (`SoundEngine::init`), initialises
    /// it, disables the unused modulation sources and sets the default
    /// oversampling. Install the parameter lookup
    /// ([`crate::parameters::install`]) first.
    pub fn create() -> ProcessorRef {
        crate::parameters::install();
        let kernel = SoundEngine {
            voice_handler: None,
            effect_chain: None,
            output_total: None,
            decimator: None,
            peak_meter: None,
            oversampling: None,
            legato: None,
            bps: None,
            last_oversampling_amount: -1,
            last_sample_rate: -1,
        };
        let node = SynthModule::create_with(kernel, 0, 1, false);
        node.with::<SoundEngine, _>(|engine, this| engine.build(this));
        node.init();
        node.with::<SoundEngine, _>(|engine, _| {
            engine.disable_unnecessary_mod_sources();
            engine.set_oversampling_amount(DEFAULT_OVERSAMPLING_AMOUNT, DEFAULT_SAMPLE_RATE);
        });
        let bps = node.module_data().borrow().controls["beats_per_minute"].clone();
        node.with::<SoundEngine, _>(|engine, _| engine.bps = Some(bps));
        node
    }

    /// The graph part of `SoundEngine::init`.
    fn build(&mut self, this: &Processor) {
        this.create_base_control("bypass", false, false);
        this.create_base_control("mpe_enabled", false, false);
        this.create_base_control("view_spectrogram", false, false);
        let oversampling = this.create_base_control("oversampling", false, false);
        let legato = this.create_base_control("legato", false, false);

        let stereo_routing = this.create_mono_mod_control("stereo_routing", false, false, None);
        let stereo_mode = this.create_base_control("stereo_mode", false, false);
        let beats_per_second = this.create_mono_mod_control("beats_per_minute", false, false, None);
        let beats_per_second_clamped = cr::LowerBound::create(0.0);
        beats_per_second_clamped.plug(&beats_per_second, 0);
        this.add_processor(&beats_per_second_clamped);

        let polyphony = this.create_mono_mod_control("polyphony", false, false, None);
        let voice_priority = this.create_base_control("voice_priority", false, false);
        let voice_override = this.create_base_control("voice_override", false, false);

        let voice_handler = SynthVoiceHandler::create(&beats_per_second_clamped.output(0));
        this.add_submodule(&voice_handler);
        with_voice_handler::<SynthVoiceHandler, _>(&voice_handler, |vh, _| {
            vh.set_polyphony(MAX_POLYPHONY)
        });
        voice_handler.plug(&polyphony, VoiceHandler::POLYPHONY);
        voice_handler.plug(&voice_priority, VoiceHandler::VOICE_PRIORITY);
        voice_handler.plug(&voice_override, VoiceHandler::VOICE_OVERRIDE);
        this.add_processor(&voice_handler);

        this.create_base_control("pitch_wheel", false, false);
        this.create_base_control("mod_wheel", false, false);

        let effect_chain_order = this.create_base_control("effect_chain_order", false, false);
        let midi_offset =
            voice_handler.with::<SynthVoiceHandler, _>(|vh, _| vh.midi_offset_output());
        let effect_chain = ReorderableEffectChain::create(&beats_per_second, &midi_offset);
        this.add_submodule(&effect_chain);
        this.add_processor(&effect_chain);
        effect_chain.plug(&voice_handler, ReorderableEffectChain::AUDIO);
        effect_chain.plug(&effect_chain_order, ReorderableEffectChain::ORDER);

        let effect = |which: Effect| {
            effect_chain
                .with::<ReorderableEffectChain, _>(|chain, _| chain.get_effect(which).clone())
        };
        let compressor = effect(Effect::Compressor);
        for (name, index) in [
            (
                "compressor_low_input",
                CompressorModule::LOW_INPUT_MEAN_SQUARED,
            ),
            (
                "compressor_band_input",
                CompressorModule::BAND_INPUT_MEAN_SQUARED,
            ),
            (
                "compressor_high_input",
                CompressorModule::HIGH_INPUT_MEAN_SQUARED,
            ),
            (
                "compressor_low_output",
                CompressorModule::LOW_OUTPUT_MEAN_SQUARED,
            ),
            (
                "compressor_band_output",
                CompressorModule::BAND_OUTPUT_MEAN_SQUARED,
            ),
            (
                "compressor_high_output",
                CompressorModule::HIGH_OUTPUT_MEAN_SQUARED,
            ),
        ] {
            this.create_status_output(name, &compressor.output(index));
        }

        let chorus = effect(Effect::Chorus);
        for i in 0..ChorusModule::MAX_DELAY_PAIRS {
            this.create_status_output(&format!("chorus_delays{}", i + 1), &chorus.output(i + 1));
        }

        let phaser = effect(Effect::Phaser);
        this.create_status_output("phaser_cutoff", &phaser.output(PhaserModule::CUTOFF_OUTPUT));

        let flanger = effect(Effect::Flanger);
        this.create_status_output(
            "flanger_delay_frequency",
            &flanger.output(FlangerModule::FREQUENCY_OUTPUT),
        );

        let output_total = operators::Add::create();
        output_total.plug(&effect_chain, 0);
        let direct = voice_handler.with::<SynthVoiceHandler, _>(|vh, _| vh.direct_output());
        output_total.plug(&direct, 1);
        this.add_processor(&output_total);

        let decimator = Decimator::create(3);
        decimator.plug(&output_total, 0);
        this.add_processor(&decimator);

        let decoder = operators::StereoEncoder::create(true);
        decoder.plug(&decimator, operators::StereoEncoder::AUDIO);
        decoder.plug(&stereo_routing, operators::StereoEncoder::ENCODING_VALUE);
        decoder.plug(&stereo_mode, operators::StereoEncoder::MODE);
        this.add_processor(&decoder);

        let volume = this.create_mono_mod_control("volume", false, false, None);
        let scaled_audio = operators::SmoothVolume::create(operators::SmoothVolume::DEFAULT_MAX_DB);
        scaled_audio.plug(&decoder, operators::smooth_multiply::AUDIO_RATE);
        scaled_audio.plug(&volume, operators::SmoothVolume::DB);

        let peak_meter = PeakMeter::create();
        peak_meter.plug(&scaled_audio, 0);
        this.create_status_output("peak_meter", &peak_meter.output(PeakMeter::LEVEL));
        this.create_status_output(
            "peak_meter_memory",
            &peak_meter.output(PeakMeter::MEMORY_PEAK),
        );

        let clamp = operators::Clamp::create(-2.1, 2.1);
        clamp.plug(&scaled_audio, 0);

        this.add_processor(&peak_meter);
        this.add_processor(&scaled_audio);

        this.add_processor(&clamp);
        clamp.use_output(&this.output(0), 0);

        self.voice_handler = Some(voice_handler);
        self.effect_chain = Some(effect_chain);
        self.output_total = Some(output_total);
        self.decimator = Some(decimator);
        self.peak_meter = Some(peak_meter);
        self.oversampling = Some(oversampling);
        self.legato = Some(legato);
    }

    /// The [`SynthVoiceHandler`] node.
    pub fn voice_handler(&self) -> &ProcessorRef {
        self.voice_handler.as_ref().expect("built in create")
    }

    /// The `ReorderableEffectChain` node.
    pub fn effect_chain(&self) -> &ProcessorRef {
        self.effect_chain.as_ref().expect("built in create")
    }

    fn with_svh<R>(&self, f: impl FnOnce(&mut SynthVoiceHandler) -> R) -> R {
        self.voice_handler()
            .with::<SynthVoiceHandler, _>(|svh, _| f(svh))
    }

    fn with_vh<R>(&self, f: impl FnOnce(&mut VoiceHandler) -> R) -> R {
        with_voice_handler::<SynthVoiceHandler, _>(self.voice_handler(), |vh, _| f(vh))
    }

    /// `checkOversampling()`.
    pub fn check_oversampling(&mut self, this: &Processor) {
        let oversampling = self.oversampling.as_ref().expect("built").value() as i32;
        let oversampling_amount = 1 << oversampling;
        let sample_rate = this.sample_rate();
        if self.last_oversampling_amount != oversampling_amount
            || self.last_sample_rate != sample_rate
        {
            self.set_oversampling_amount(oversampling_amount, sample_rate);
        }
    }

    /// `setOversamplingAmount(oversampling_amount, sample_rate)`.
    pub fn set_oversampling_amount(&mut self, oversampling_amount: i32, sample_rate: i32) {
        const BASE_SAMPLE_RATE: i32 = 44100;

        let mut oversample = oversampling_amount;
        let mut sample_rate_mult = sample_rate / BASE_SAMPLE_RATE;
        while sample_rate_mult > 1 && oversample > 1 {
            sample_rate_mult >>= 1;
            oversample >>= 1;
        }
        self.voice_handler().set_oversample_amount(oversample);
        self.effect_chain().set_oversample_amount(oversample);
        self.output_total
            .as_ref()
            .expect("built")
            .set_oversample_amount(oversample);
        self.last_oversampling_amount = oversampling_amount;
        self.last_sample_rate = sample_rate;
    }

    /// `getNumPressedNotes()`.
    pub fn num_pressed_notes(&self) -> usize {
        self.with_vh(|vh| vh.num_pressed_notes())
    }

    /// `getNumActiveVoices()`.
    pub fn num_active_voices(&self) -> usize {
        self.with_vh(|vh| vh.num_active_voices())
    }

    /// `getLastActiveNote()`.
    pub fn last_active_note(&self) -> f32 {
        self.with_vh(|vh| vh.last_active_note())
    }

    /// `setTuning(tuning)`.
    pub fn set_tuning(&self, tuning: Option<Rc<Tuning>>) {
        self.with_vh(|vh| vh.set_tuning(tuning));
    }

    /// `correctToTime(seconds)`.
    pub fn correct_to_time(&self, seconds: f64) {
        self.with_svh(|svh| svh.correct_to_time(seconds));
        self.effect_chain().correct_to_time(seconds);
    }

    /// `allSoundsOff()`.
    pub fn all_sounds_off(&self) {
        self.with_svh(|svh| svh.all_sounds_off());
        self.effect_chain().hard_reset();
        self.decimator.as_ref().expect("built").hard_reset();
    }

    /// `allNotesOff(sample)`.
    pub fn all_notes_off(&self, sample: i32) {
        self.with_svh(|svh| svh.all_notes_off(sample));
    }

    /// `allNotesOff(sample, channel)`: upstream passes the channel as the
    /// voice handler's sample argument.
    pub fn all_notes_off_channel(&self, _sample: i32, channel: i32) {
        self.with_svh(|svh| svh.all_notes_off(channel));
    }

    /// `allNotesOffRange(sample, from_channel, to_channel)`.
    pub fn all_notes_off_range(&self, sample: i32, from_channel: i32, to_channel: i32) {
        self.with_vh(|vh| vh.all_notes_off_range(sample, from_channel, to_channel));
    }

    /// `noteOn(note, velocity, sample, channel)`.
    pub fn note_on(&self, note: i32, velocity: f32, sample: i32, channel: i32) {
        self.with_svh(|svh| svh.note_on(note, velocity, sample, channel));
    }

    /// `noteOff(note, lift, sample, channel)`.
    pub fn note_off(&self, note: i32, lift: f32, sample: i32, channel: i32) {
        self.with_svh(|svh| svh.note_off(note, lift, sample, channel));
    }

    pub fn set_mod_wheel(&self, value: f32, channel: i32) {
        self.with_vh(|vh| vh.set_mod_wheel(value, channel));
    }

    pub fn set_mod_wheel_all_channels(&self, value: f32) {
        self.with_vh(|vh| vh.set_mod_wheel_all_channels(value));
    }

    pub fn set_pitch_wheel(&self, value: f32, channel: i32) {
        self.with_vh(|vh| vh.set_pitch_wheel(value, channel));
    }

    pub fn set_zoned_pitch_wheel(&self, value: f32, from_channel: i32, to_channel: i32) {
        self.with_vh(|vh| vh.set_zoned_pitch_wheel(value, from_channel, to_channel));
    }

    pub fn set_aftertouch(&self, note: i32, value: f32, sample: i32, channel: i32) {
        self.with_vh(|vh| vh.set_aftertouch(note, value, sample, channel));
    }

    pub fn set_channel_aftertouch(&self, channel: i32, value: f32, sample: i32) {
        self.with_vh(|vh| vh.set_channel_aftertouch(channel, value, sample));
    }

    pub fn set_channel_range_aftertouch(
        &self,
        from_channel: i32,
        to_channel: i32,
        value: f32,
        sample: i32,
    ) {
        self.with_vh(|vh| vh.set_channel_range_aftertouch(from_channel, to_channel, value, sample));
    }

    pub fn set_channel_slide(&self, channel: i32, value: f32, sample: i32) {
        self.with_vh(|vh| vh.set_channel_slide(channel, value, sample));
    }

    pub fn set_channel_range_slide(
        &self,
        from_channel: i32,
        to_channel: i32,
        value: f32,
        sample: i32,
    ) {
        self.with_vh(|vh| vh.set_channel_range_slide(from_channel, to_channel, value, sample));
    }

    pub fn sustain_on(&self, channel: i32) {
        self.with_vh(|vh| vh.sustain_on(channel));
    }

    pub fn sustain_off(&self, sample: i32, channel: i32) {
        self.with_vh(|vh| vh.sustain_off(sample, channel));
    }

    pub fn sostenuto_on(&self, channel: i32) {
        self.with_vh(|vh| vh.sostenuto_on(channel));
    }

    pub fn sostenuto_off(&self, sample: i32, channel: i32) {
        self.with_vh(|vh| vh.sostenuto_off(sample, channel));
    }

    pub fn sustain_on_range(&self, from_channel: i32, to_channel: i32) {
        self.with_vh(|vh| vh.sustain_on_range(from_channel, to_channel));
    }

    pub fn sustain_off_range(&self, sample: i32, from_channel: i32, to_channel: i32) {
        self.with_vh(|vh| vh.sustain_off_range(sample, from_channel, to_channel));
    }

    pub fn sostenuto_on_range(&self, from_channel: i32, to_channel: i32) {
        self.with_vh(|vh| vh.sostenuto_on_range(from_channel, to_channel));
    }

    pub fn sostenuto_off_range(&self, sample: i32, from_channel: i32, to_channel: i32) {
        self.with_vh(|vh| vh.sostenuto_off_range(sample, from_channel, to_channel));
    }

    /// `disableUnnecessaryModSources()`.
    pub fn disable_unnecessary_mod_sources(&self) {
        self.with_svh(|svh| svh.disable_unnecessary_mod_sources());
    }

    /// `setBpm(bpm)`: the `beats_per_minute` control holds beats per second.
    pub fn set_bpm(&self, bpm: f32) {
        let bps = bpm / 60.0;
        let control = self.bps.as_ref().expect("set in create");
        if control.value() != bps {
            control.set_value(bps);
        }
    }

    /// `SoundEngine::process`.
    pub fn process(&mut self, this: &Processor, num_samples: usize) {
        debug_assert!(num_samples <= this.output(0).buffer_size());
        let _no_denormals = ScopedNoDenormals::new();
        let legato = self.legato.as_ref().expect("built").value() != 0.0;
        self.with_vh(|vh| vh.set_legato(legato));
        this.router_process(num_samples);

        if self.num_active_voices() == 0 {
            let connections = self.with_svh(|svh| svh.enabled_modulation_connections().to_vec());
            for modulation in connections {
                let polyphonic_source = modulation
                    .input(ModulationConnectionProcessor::MODULATION_INPUT)
                    .source()
                    .and_then(|source| source.owner())
                    .is_some_and(|owner| owner.is_polyphonic());
                if !polyphonic_source {
                    modulation.process(num_samples);
                }
            }
        }

        let status_outputs = this.module_data().borrow().status_outputs.clone();
        for status in status_outputs.values() {
            status.update();
        }
    }
}

impl ProcessorImpl for SoundEngine {
    fn clone_box(&self) -> Box<dyn ProcessorImpl> {
        panic!("SoundEngine::clone is not supported (upstream's inherited SynthModule::clone slices the engine)")
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        SoundEngine::process(self, this, num_samples);
    }

    fn correct_to_time(&mut self, _this: &Processor, seconds: f64) {
        SoundEngine::correct_to_time(self, seconds);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// The engine's stereo output buffer (`output(0)` of the engine node).
pub fn engine_output(node: &ProcessorRef) -> Rc<Output> {
    node.output(0)
}
