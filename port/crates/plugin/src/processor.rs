// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The audio callback: ports `SynthPlugin::processBlock`
//! (`vital/src/plugin/synth_plugin.cpp`).
//!
//! Per host block (with each parameter at the last value the host sent for
//! the block, as upstream's JUCE wrapper applies automation): hand any newly
//! restored preset, then the edits queued on the edit bus
//! (`sas_engine::edit_bus`), then changed parameters to the engine; output
//! silence and drop MIDI if bypassed; read
//! the tempo and song position; then render in chunks of at most
//! `MAX_BUFFER_SIZE` samples, each preceded by `correctToTime` and the MIDI
//! messages that fall inside it.

use nice_plug::midi::MidiResult;
use nice_plug::prelude::{NoteEvent, SysExMessage};
use sas_engine::edit_bus::EditConsumer;
use sas_engine::sas_dsp_core::sas_simd::common::MAX_BUFFER_SIZE;

use crate::engine::Engine;
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

/// Drives an [`Engine`] from host blocks.
pub struct BlockProcessor<E: Engine> {
    engine: E,
    sample_rate: f32,
    /// `SynthPlugin::last_seconds_time_`.
    last_seconds_time: f64,
    /// The audio side of the edit bus whose producer the parameters hold:
    /// restored documents and edits arrive through it.
    edits: EditConsumer,
    /// Set by [`activate`](Self::activate) until the next block prepares the
    /// engine: the document generation at activation.
    pending_prepare: Option<u64>,
}

impl<E: Engine> BlockProcessor<E> {
    pub fn new(engine: E, edits: EditConsumer) -> BlockProcessor<E> {
        BlockProcessor {
            engine,
            sample_rate: 44_100.0,
            last_seconds_time: 0.0,
            edits,
            pending_prepare: None,
        }
    }

    pub fn engine(&self) -> &E {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut E {
        &mut self.engine
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

        if params.bypassed() {
            // AudioProcessor::processBlockBypassed with no inputs.
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
        let sample_time = 1.0 / self.sample_rate as f64;
        let mut pending = next_event();
        let mut offset = 0;
        while offset < total_samples {
            let num_samples = (total_samples - offset).min(MAX_BUFFER_SIZE);
            let end = offset + num_samples;
            let last_chunk = end == total_samples;

            self.engine.correct_to_time(self.last_seconds_time);
            while let Some(event) = pending.take() {
                let timing = event.timing() as usize;
                if timing >= end && !last_chunk {
                    pending = Some(event);
                    break;
                }
                let sample_offset = timing.saturating_sub(offset).min(num_samples - 1);
                if let Some(message) = midi_message(&event) {
                    self.engine.process_midi(message, sample_offset);
                }
                pending = next_event();
            }
            self.engine
                .process(&mut left[offset..end], &mut right[offset..end]);

            self.last_seconds_time += num_samples as f64 * sample_time;
            offset = end;
        }
        while next_event().is_some() {}
    }

    /// Loads a newly restored preset, applies the queued edits, then pushes
    /// the parameters the host changed. Edits go first, so a host change in
    /// the same block (to a new modulation slot's amount, say) wins over the
    /// values an edit set.
    fn sync_parameters(&mut self, params: &SynthParams) {
        let Some(restore) = self.edits.take_restore() else {
            self.engine.apply_edits(&mut self.edits);
            params.changes.drain(|index| {
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
