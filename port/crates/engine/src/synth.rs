// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The synth around the [`SoundEngine`]: what `SynthBase` (and the
//! offline `ref-render` harness's `HeadlessSynth`) does on the audio path.
//!
//! Ports the engine-facing parts of `vital/src/common/synth_base.cpp`: the
//! constructor (engine, tuning, default wavetables), `loadFromFile` /
//! `LoadSave::jsonToState` through a [`PresetState`], `processAudio`,
//! `processMidi`, `checkOversampling`, and the render harness's block loop
//! (`migration/reference/src/render_main.cpp`, `RenderSynth::renderBlock`).
//!
//! Modulation routing (`connectModulation`, the modulation-change queue)
//! comes with the modulation slice of the engine port; a loaded preset's
//! modulations are not connected yet.

use std::rc::Rc;

use sas_dsp_core::ProcessorRef;
use sas_oscillators::sas_wavetable::creator::WavetableCreator;
use sas_preset::{lookup, PresetState};

use crate::midi_file::TimedMessage;
use crate::midi_manager::MidiManager;
use crate::sound_engine::SoundEngine;
use crate::synth_voice_handler::SynthVoiceHandler;
use crate::tuning::Tuning;

/// `kMaxBufferSize`: the longest engine sub-block.
pub const MAX_BUFFER_SIZE: usize = 128;
/// `kNumOscillators`.
const NUM_OSCILLATORS: usize = 3;
/// `kNumLfos`.
const NUM_LFOS: usize = 8;

/// The synth: a [`SoundEngine`] with its MIDI manager, tuning and wavetable
/// creators.
pub struct Synth {
    engine: ProcessorRef,
    midi_manager: MidiManager,
    wavetable_creators: Vec<WavetableCreator>,
}

impl Default for Synth {
    fn default() -> Self {
        Self::new()
    }
}

impl Synth {
    /// `SynthBase()`: the engine with the default tuning, and each
    /// oscillator's default wavetable rendered (`WavetableCreator::init`).
    pub fn new() -> Self {
        let engine = SoundEngine::create();
        let mut synth = Synth {
            engine,
            midi_manager: MidiManager::new(),
            wavetable_creators: Vec::new(),
        };
        synth.set_tuning(&Tuning::new());
        for i in 0..NUM_OSCILLATORS {
            let mut creator = WavetableCreator::new();
            creator.init();
            let wavetable = synth.with_svh(|svh| svh.wavetable(i));
            creator
                .render(&mut wavetable.borrow_mut())
                .expect("the default wavetable renders");
            synth.wavetable_creators.push(creator);
        }
        synth
    }

    /// The engine node (`SynthBase::getEngine`).
    pub fn engine(&self) -> &ProcessorRef {
        &self.engine
    }

    fn with_engine<R>(&self, f: impl FnOnce(&mut SoundEngine) -> R) -> R {
        self.engine.with::<SoundEngine, _>(|engine, _| f(engine))
    }

    fn with_svh<R>(&self, f: impl FnOnce(&mut SynthVoiceHandler) -> R) -> R {
        let voice_handler = self.with_engine(|engine| engine.voice_handler().clone());
        voice_handler.with::<SynthVoiceHandler, _>(|svh, _| f(svh))
    }

    /// `setTuning`: the engine converts notes through a copy of `tuning`.
    pub fn set_tuning(&mut self, tuning: &Tuning) {
        let tuning = Rc::new(tuning.clone());
        self.with_engine(|engine| engine.set_tuning(Some(tuning)));
    }

    /// `setMpeEnabled(enabled)`.
    pub fn set_mpe_enabled(&mut self, enabled: bool) {
        self.midi_manager.set_mpe_enabled(enabled);
    }

    /// `SoundEngine::setSampleRate` and `MidiManager::setSampleRate`.
    pub fn set_sample_rate(&mut self, sample_rate: i32) {
        self.engine.set_sample_rate(sample_rate);
    }

    /// `SoundEngine::setBpm`.
    pub fn set_bpm(&mut self, bpm: f32) {
        self.with_engine(|engine| engine.set_bpm(bpm));
    }

    /// `SoundEngine::correctToTime`.
    pub fn correct_to_time(&mut self, seconds: f64) {
        self.with_engine(|engine| engine.correct_to_time(seconds));
    }

    /// `updateAllModulationSwitches()` on the engine.
    pub fn update_all_modulation_switches(&mut self) {
        self.engine.update_all_modulation_switches();
    }

    /// `checkOversampling()`.
    pub fn check_oversampling(&mut self) {
        self.engine
            .with::<SoundEngine, _>(|engine, this| engine.check_oversampling(this));
    }

    /// `SoundEngine::allSoundsOff`.
    pub fn all_sounds_off(&mut self) {
        self.with_engine(|engine| engine.all_sounds_off());
    }

    /// Sets control `name` (`SynthBase::valueChangedExternal`): the mod and
    /// pitch wheels also reach every channel's voices.
    pub fn set_control(&mut self, name: &str, value: f32) {
        let control = self.engine.get_controls().get(name).cloned();
        let Some(control) = control else { return };
        control.set_value(value);
        match name {
            "mod_wheel" => self.with_engine(|engine| engine.set_mod_wheel_all_channels(value)),
            "pitch_wheel" => self.with_engine(|engine| engine.set_zoned_pitch_wheel(value, 0, 15)),
            _ => {}
        }
    }

    /// `loadFromFile` / `LoadSave::jsonToState` for a parsed preset: every
    /// control (its default if the preset has none), the sample, the
    /// wavetables and the LFO shapes, then `checkOversampling`.
    pub fn load_preset(&mut self, preset: &PresetState) {
        self.all_sounds_off();

        let controls = self.engine.get_controls();
        for (name, control) in &controls {
            let value = preset
                .value(name)
                .or_else(|| lookup(name).map(|details| details.default_value))
                .unwrap_or(0.0);
            control.set_value(value);
        }
        let mod_wheel = controls["mod_wheel"].value();
        self.with_engine(|engine| engine.set_mod_wheel_all_channels(mod_wheel));

        self.with_svh(|svh| {
            for i in 0..svh.modulation_bank().num_connections() {
                let generator = svh
                    .modulation_bank()
                    .at_index(i)
                    .processor
                    .with::<sas_modulation::ModulationConnectionProcessor, _>(
                    |kernel, _| kernel.line_map_generator(),
                );
                generator.borrow_mut().init_linear();
            }
            svh.disable_unnecessary_mod_sources();
        });

        let sample = self.with_svh(|svh| svh.sample());
        {
            let state = &preset.sample;
            let mut sample = sample.borrow_mut();
            match &state.right {
                Some(right) => sample.load_stereo_sample(
                    &state.left,
                    right,
                    state.left.len(),
                    state.sample_rate,
                ),
                None => sample.load_sample(&state.left, state.left.len(), state.sample_rate),
            }
            sample.set_name(&state.name);
        }

        for (i, creator) in preset.wavetables.iter().enumerate().take(NUM_OSCILLATORS) {
            self.wavetable_creators[i] = creator.clone();
            let wavetable = self.with_svh(|svh| svh.wavetable(i));
            let _ = self.wavetable_creators[i].render(&mut wavetable.borrow_mut());
        }

        for (i, lfo) in preset.lfos.iter().enumerate().take(NUM_LFOS) {
            let source = self.with_svh(|svh| svh.lfo_source(i));
            let mut source = source.borrow_mut();
            if source.from_json(&lfo.to_json()).is_ok() {
                source.render();
            }
        }

        self.check_oversampling();
    }

    /// `MidiManager::processMidiMessage` for one short message at
    /// `sample_offset` into the next [`process`](Self::process).
    pub fn process_midi(&mut self, message: [u8; 3], sample_offset: i32) {
        let manager = &mut self.midi_manager;
        self.engine.with::<SoundEngine, _>(|engine, _| {
            manager.process_midi_message(engine, message, sample_offset)
        });
    }

    /// `processAudio`: runs the engine for `left.len()` samples (at most
    /// [`MAX_BUFFER_SIZE`]) and writes its first two lanes.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let num_samples = left.len();
        debug_assert!(num_samples <= MAX_BUFFER_SIZE && right.len() == num_samples);
        self.engine.process(num_samples);
        let output = self.engine.output(0);
        for i in 0..num_samples {
            let frame = output.at(i);
            left[i] = frame.get(0);
            right[i] = frame.get(1);
        }
    }

    /// `RenderSynth::prepare`: sample rate, bpm, modulation switches.
    pub fn prepare(&mut self, sample_rate: i32, bpm: f64) {
        self.set_sample_rate(sample_rate);
        self.set_bpm(bpm as f32);
        self.update_all_modulation_switches();
    }

    /// `RenderSynth::renderBlock`: one host block, in engine sub-blocks of at
    /// most [`MAX_BUFFER_SIZE`], each preceded by `correctToTime` and the
    /// block's MIDI that falls inside it. The whole block runs with denormals
    /// flushed to zero, as upstream's sticky `disableDenormalisedNumberSupport`
    /// and the plugin's `ScopedNoDenormals` leave it. `midi` holds `(offset, message)`
    /// pairs relative to the block start, in order.
    pub fn render_block(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        midi: &[(usize, [u8; 3])],
        seconds_time: &mut f64,
        sample_rate: i32,
    ) {
        let _no_denormals = sas_dsp_core::ScopedNoDenormals::new();
        let total_samples = left.len();
        let sample_time = 1.0 / sample_rate as f64;
        let mut sample_offset = 0;
        while sample_offset < total_samples {
            let num_samples = (total_samples - sample_offset).min(MAX_BUFFER_SIZE);
            self.correct_to_time(*seconds_time);
            for &(position, message) in midi {
                if position >= sample_offset && position < sample_offset + num_samples {
                    self.process_midi(message, (position - sample_offset) as i32);
                }
            }
            let end = sample_offset + num_samples;
            self.process(
                &mut left[sample_offset..end],
                &mut right[sample_offset..end],
            );
            *seconds_time += num_samples as f64 * sample_time;
            sample_offset = end;
        }
    }
}

/// Settings for [`render`], as `ref-render` takes them.
#[derive(Clone, Debug)]
pub struct RenderSettings {
    pub sample_rate: i32,
    pub block_size: usize,
    pub bpm: f64,
    /// Frames to render.
    pub frames: usize,
}

/// `ref-render`'s main loop: a loaded synth, MIDI events at
/// `floor(seconds * rate + 0.5)`, rendered block by block from t = 0.
/// Returns interleaved stereo.
pub fn render(synth: &mut Synth, events: &[TimedMessage], settings: &RenderSettings) -> Vec<f32> {
    let sample_rate = settings.sample_rate;
    synth.prepare(sample_rate, settings.bpm);

    let timed: Vec<(i64, [u8; 3])> = events
        .iter()
        .map(|event| {
            let sample = (event.seconds * sample_rate as f64 + 0.5).floor() as i64;
            (sample, event.message)
        })
        .collect();

    let mut seconds_time = 0.0;
    let mut interleaved = vec![0.0f32; settings.frames * 2];
    let mut left = vec![0.0f32; settings.block_size];
    let mut right = vec![0.0f32; settings.block_size];
    let mut event_index = 0;
    let mut block_start = 0usize;
    while block_start < settings.frames {
        let n = settings.block_size.min(settings.frames - block_start);
        left[..n].fill(0.0);
        right[..n].fill(0.0);

        let mut midi = Vec::new();
        while event_index < timed.len() && timed[event_index].0 < (block_start + n) as i64 {
            let offset = (timed[event_index].0 - block_start as i64).max(0) as usize;
            midi.push((offset, timed[event_index].1));
            event_index += 1;
        }

        synth.render_block(
            &mut left[..n],
            &mut right[..n],
            &midi,
            &mut seconds_time,
            sample_rate,
        );
        for i in 0..n {
            interleaved[2 * (block_start + i)] = left[i];
            interleaved[2 * (block_start + i) + 1] = right[i];
        }
        block_start += n;
    }
    interleaved
}
