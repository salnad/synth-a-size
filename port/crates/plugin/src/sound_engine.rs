// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The M7 sound engine behind the wrapper's [`Engine`] interface.

use sas_engine::synth::Synth;
use sas_engine::Tuning;
use sas_preset::{PresetState, SampleState};

use crate::engine::Engine;

/// [`Synth`] (the port of `SynthBase` and its `SoundEngine`) as the
/// plugin drives it.
pub struct SoundEngineHost {
    synth: Synth,
}

// SAFETY: `Synth` owns its whole processor graph. Its `Rc`/`RefCell`
// handles all point inside that graph, and this wrapper's public surface
// takes and returns only plain values: no method exposes the `Synth` or any
// graph handle, and `sample_state` returns an owned copy. So moving the
// wrapper moves every handle together and no two threads can reach the
// same `Rc`. The graph keeps no thread-local
// buffers (such as dsp-core's per-thread null buffer), so nothing it holds
// belongs to the thread that built it. The wrapper is only used from one
// thread at a time (`Engine` methods take `&mut self`).
unsafe impl Send for SoundEngineHost {}

impl Default for SoundEngineHost {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundEngineHost {
    pub fn new() -> Self {
        Self {
            synth: Synth::new(),
        }
    }

    /// The sample the engine plays (see [`Synth::sample_state`]): the
    /// engine's own white noise until a preset loads a sample.
    pub fn sample_state(&self) -> SampleState {
        self.synth.sample_state()
    }
}

impl Engine for SoundEngineHost {
    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.synth.set_sample_rate(sample_rate as i32);
        self.synth.update_all_modulation_switches();
    }

    fn reset(&mut self) {
        self.synth.all_sounds_off();
    }

    fn load_preset(&mut self, preset: &PresetState) {
        self.synth.load_preset(preset);
    }

    fn engine_sample(&self) -> Option<SampleState> {
        Some(SoundEngineHost::sample_state(self))
    }

    fn set_tuning(&mut self, tuning: &Tuning) {
        self.synth.set_tuning(tuning);
    }

    fn set_control(&mut self, name: &'static str, value: f32) {
        self.synth.set_control(name, value);
    }

    fn set_bpm(&mut self, bpm: f64) {
        self.synth.set_bpm(bpm as f32);
    }

    fn correct_to_time(&mut self, seconds: f64) {
        self.synth.correct_to_time(seconds);
    }

    fn process_midi(&mut self, message: [u8; 3], sample_offset: usize) {
        self.synth.process_midi(message, sample_offset as i32);
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.synth.process(left, right);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sas_preset::InitContent;

    #[test]
    fn a_note_makes_sound() {
        let mut engine = SoundEngineHost::new();
        engine.set_sample_rate(44100.0);
        engine.load_preset(&PresetState::new(&InitContent { sample_seed: 0 }));
        engine.set_tuning(&Tuning::new());
        engine.process_midi([0x90, 60, 100], 0);
        let mut peak = 0.0f32;
        let (mut left, mut right) = (vec![0.0; 128], vec![0.0; 128]);
        for _ in 0..100 {
            engine.process(&mut left, &mut right);
            peak = left.iter().chain(&right).fold(peak, |m, v| m.max(v.abs()));
        }
        assert!(peak > 0.01, "peak {peak}");
    }
}
