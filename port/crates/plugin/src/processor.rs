// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_audio_basics, ISC).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The audio callback: ports `SynthPlugin::processBlock`
//! (`vital/src/plugin/synth_plugin.cpp`).
//!
//! Per host block (with each parameter at the last value the host sent for
//! the block, as upstream's JUCE wrapper applies automation): hand any newly
//! restored preset, then the edits queued on the edit bus
//! (`sas_engine::edit_bus`), then changed parameters to the engine; output
//! silence and drop MIDI if bypassed; read
//! the tempo and song position; merge the on-screen keyboard's notes into
//! the block's MIDI (`processKeyboardEvents`, `sas_engine::keyboard`); then
//! render in chunks of at most `MAX_BUFFER_SIZE` samples, each preceded by
//! `correctToTime` and the MIDI messages that fall inside it. MIDI learn
//! requests from the editor reach the engine first, bypassed or not;
//! parameters a learned controller sets follow each message.

use std::sync::Arc;

use nice_plug::midi::MidiResult;
use nice_plug::prelude::{NoteEvent, SysExMessage};
use sas_engine::edit_bus::EditConsumer;
use sas_engine::keyboard::KeyboardConsumer;
use sas_engine::sas_dsp_core::sas_simd::common::MAX_BUFFER_SIZE;
use sas_engine::telemetry::TelemetryWriter;

use crate::engine::Engine;
use crate::midi_learn::MidiLearn;
use crate::params::SynthParams;

/// The parts of the host transport the engine uses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transport {
    pub tempo: Option<f64>,
    pub playing: bool,
    /// Song position at the start of the block, in quarter notes.
    pub pos_beats: Option<f64>,
}

/// The short MIDI message of `event`, as upstream's `MidiManager` receives it
/// from JUCE (velocity, pressure and CC values at 7 bits, pitch bend at 14).
/// Note expressions with no MIDI equivalent are dropped, and so are note
/// events addressed to every channel or key (CLAP only; VST3 always names
/// both).
pub fn midi_message<S: SysExMessage>(event: &NoteEvent<S>) -> Option<[u8; 3]> {
    let addressed = match event {
        NoteEvent::NoteOn { channel, key, .. }
        | NoteEvent::NoteOff { channel, key, .. }
        | NoteEvent::PolyPressure { channel, key, .. } => {
            channel.number().is_some() && key.number().is_some()
        }
        _ => true,
    };
    match event.as_midi() {
        Some(MidiResult::Basic(message)) if addressed => Some(message),
        _ => None,
    }
}

/// What a block asks of the wrapper after it ran.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockOutcome {
    /// The MIDI learn map changed: save the user config off the audio
    /// thread ([`MidiLearn::save_config`]).
    pub save_midi_learn: bool,
}

/// Drives an [`Engine`] from host blocks.
pub struct BlockProcessor<E: Engine> {
    engine: E,
    midi_learn: Arc<MidiLearn>,
    /// The engine's learn map changed but could not be published yet.
    midi_learn_unpublished: bool,
    sample_rate: f32,
    /// `SynthPlugin::last_seconds_time_`.
    last_seconds_time: f64,
    /// Samples processed so far, bypassed or not: the clock the echo guard
    /// times MIDI moves with.
    samples: u64,
    /// The audio side of the edit bus whose producer the parameters hold:
    /// restored documents and edits arrive through it.
    edits: EditConsumer,
    /// The on-screen keyboard's notes, merged into each block's MIDI.
    keyboard: KeyboardConsumer,
    /// The editor's telemetry, published at the end of each block that runs
    /// the engine.
    telemetry: TelemetryWriter,
    /// Set by [`activate`](Self::activate) until the next block prepares the
    /// engine: the document generation at activation.
    pending_prepare: Option<u64>,
}

impl<E: Engine> BlockProcessor<E> {
    /// `engine` with no saved MIDI learn map, saving none.
    pub fn new(
        engine: E,
        edits: EditConsumer,
        keyboard: KeyboardConsumer,
        telemetry: TelemetryWriter,
    ) -> BlockProcessor<E> {
        BlockProcessor::with_midi_learn(
            engine,
            edits,
            keyboard,
            telemetry,
            Arc::new(MidiLearn::in_memory()),
        )
    }

    /// `engine` with the map `midi_learn` holds (the saved one, as
    /// `SynthBase`'s constructor loads it through `Startup`).
    pub fn with_midi_learn(
        mut engine: E,
        edits: EditConsumer,
        keyboard: KeyboardConsumer,
        telemetry: TelemetryWriter,
        midi_learn: Arc<MidiLearn>,
    ) -> BlockProcessor<E> {
        engine.set_midi_learn_map(midi_learn.map());
        BlockProcessor {
            engine,
            midi_learn,
            midi_learn_unpublished: false,
            sample_rate: 44_100.0,
            last_seconds_time: 0.0,
            samples: 0,
            edits,
            keyboard,
            telemetry,
            pending_prepare: None,
        }
    }

    pub fn engine(&self) -> &E {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut E {
        &mut self.engine
    }

    pub fn midi_learn(&self) -> &Arc<MidiLearn> {
        &self.midi_learn
    }

    /// `SynthPlugin::prepareToPlay`, now.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.engine.set_sample_rate(sample_rate);
    }

    /// The host activated the plugin at `sample_rate`. The engine is
    /// prepared (`prepareToPlay`) at the next block, in upstream's order
    /// relative to restoring state, which loads synchronously there: a state
    /// restored before activation loads first and is then prepared (as when
    /// a host opens a project, and as the reference's offline render does);
    /// one restored after activation is prepared first and then loads.
    pub fn activate(&mut self, _params: &SynthParams, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.pending_prepare = Some(self.edits.published_generation());
    }

    pub fn reset(&mut self) {
        self.engine.reset();
    }

    /// Renders one host block into `left` / `right` (same length), taking
    /// MIDI from `next_event` (in time order, timings relative to the block).
    pub fn process<S: SysExMessage>(
        &mut self,
        params: &SynthParams,
        left: &mut [f32],
        right: &mut [f32],
        transport: &Transport,
        next_event: impl FnMut() -> Option<NoteEvent<S>>,
    ) -> BlockOutcome {
        if self.midi_learn.apply_commands(&mut self.engine) {
            self.midi_learn_unpublished = true;
        }
        self.render(params, left, right, transport, next_event);
        self.samples += left.len().min(right.len()) as u64;
        if self.engine.take_midi_learn_changed() {
            self.midi_learn_unpublished = true;
        }
        let mut outcome = BlockOutcome::default();
        if self.midi_learn_unpublished {
            if let Some(changed) = self.midi_learn.publish(&self.engine) {
                self.midi_learn_unpublished = false;
                outcome.save_midi_learn = changed;
            }
        }
        outcome
    }

    fn render<S: SysExMessage>(
        &mut self,
        params: &SynthParams,
        left: &mut [f32],
        right: &mut [f32],
        transport: &Transport,
        mut next_event: impl FnMut() -> Option<NoteEvent<S>>,
    ) {
        const SECONDS_PER_MINUTE: f64 = 60.0;

        // Upstream applies state and parameter changes as they arrive, bypassed
        // or not; here they reach the engine at the start of the next block.
        match self.pending_prepare.take() {
            Some(generation_at_activation)
                if self.edits.published_generation() != generation_at_activation =>
            {
                self.engine.set_sample_rate(self.sample_rate);
                self.sync_parameters(params);
            }
            Some(_) => {
                self.sync_parameters(params);
                self.engine.set_sample_rate(self.sample_rate);
            }
            None => self.sync_parameters(params),
        }

        self.keyboard.collect();
        if params.bypassed() {
            // AudioProcessor::processBlockBypassed with no inputs; the
            // keyboard's notes wait.
            left.fill(0.0);
            right.fill(0.0);
            while next_event().is_some() {}
            return;
        }

        if let Some(bpm) = transport.tempo.filter(|bpm| *bpm > 0.0) {
            self.engine.set_bpm(bpm);
            if transport.playing {
                if let Some(beats) = transport.pos_beats {
                    let bps = bpm / SECONDS_PER_MINUTE;
                    self.last_seconds_time = beats / bps;
                }
            }
        }

        let total_samples = left.len().min(right.len());
        self.keyboard.schedule(total_samples);
        let sample_time = 1.0 / self.sample_rate as f64;
        let mut pending = next_event();
        let mut offset = 0;
        while offset < total_samples {
            let num_samples = (total_samples - offset).min(MAX_BUFFER_SIZE);
            let end = offset + num_samples;
            let last_chunk = end == total_samples;

            self.engine.correct_to_time(self.last_seconds_time);
            loop {
                let host = pending
                    .as_ref()
                    .map(|event| event.timing() as usize)
                    .filter(|&timing| timing < end || last_chunk);
                let keyboard = self
                    .keyboard
                    .next_position()
                    .filter(|&position| position < end);
                match (host, keyboard) {
                    // MidiBuffer::addEvent puts a keyboard message after the
                    // host's at the same sample.
                    (Some(timing), Some(position)) if position < timing => {
                        self.play_keyboard(offset)
                    }
                    (Some(timing), _) => {
                        let sample_offset = timing.saturating_sub(offset).min(num_samples - 1);
                        if let Some(message) = pending.take().as_ref().and_then(midi_message) {
                            self.keyboard.host_message(message);
                            self.engine.process_midi(message, sample_offset);
                            let now = self.samples + (offset + sample_offset) as u64;
                            self.engine.drain_midi_value_changes(&mut |name, value| {
                                params.set_from_midi(name, value, now)
                            });
                        }
                        pending = next_event();
                    }
                    (None, Some(_)) => self.play_keyboard(offset),
                    (None, None) => break,
                }
            }
            self.engine
                .process(&mut left[offset..end], &mut right[offset..end]);

            self.last_seconds_time += num_samples as f64 * sample_time;
            offset = end;
        }
        self.telemetry
            .snapshot_mut()
            .set_held_notes(self.keyboard.held_notes());
        self.engine.publish_telemetry(&mut self.telemetry);
        while next_event().is_some() {}
    }

    /// How long after a learned MIDI move a host change can still be its
    /// stale echo, in samples: a few editor frames and a long host block.
    fn stale_echo_window(&self) -> u64 {
        const STALE_ECHO_SECONDS: f32 = 0.5;
        (STALE_ECHO_SECONDS * self.sample_rate) as u64
    }

    /// The keyboard's next message, in the chunk starting at `offset`.
    fn play_keyboard(&mut self, offset: usize) {
        if let Some((position, message)) = self.keyboard.pop() {
            self.engine.process_midi(message, position - offset);
        }
    }

    /// Loads a newly restored preset, applies the queued edits, then pushes
    /// the parameters the host changed. Edits go first, so a host change in
    /// the same block (to a new modulation slot's amount, say) wins over the
    /// values an edit set.
    fn sync_parameters(&mut self, params: &SynthParams) {
        let Some(restore) = self.edits.take_restore() else {
            self.engine.apply_edits(&mut self.edits);
            let (now, window) = (self.samples, self.stale_echo_window());
            params.changes.drain(|index| {
                // The host echoing a forwarded MIDI move that newer MIDI
                // overtook is not a host change.
                if params.take_stale_echo(index, now, window) {
                    return;
                }
                let control = &params.controls()[index];
                self.engine
                    .set_control(control.info.name.as_str(), control.param.value());
            });
            return;
        };
        let document = &restore.document;
        self.engine.load_preset(&document.state);
        self.engine.set_tuning(&document.tuning);
        self.engine.apply_edits(&mut self.edits);
        params.changes.drain(|_| {});
        // Parameters the host changed after the restore.
        for (index, control) in params.controls().iter().enumerate() {
            let name = control.info.name.as_str();
            let value = control.param.value();
            let represented = document
                .state
                .value(name)
                .is_some_and(|preset| params.preset_value_stands(index, preset, value));
            if !represented {
                self.engine.set_control(name, value);
            }
        }
        self.edits.retire_restore(restore);
    }
}
