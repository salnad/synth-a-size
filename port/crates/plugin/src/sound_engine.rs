// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The M7 sound engine behind the wrapper's [`Engine`] interface.

use sas_engine::synth::Synth;
use sas_engine::tuning::Tuning;
use sas_preset::PresetState;

use crate::engine::Engine;

/// [`Synth`] (the port of `SynthBase` and its `SoundEngine`) as the
/// plugin drives it.
pub struct SoundEngineHost {
    synth: Synth,
}

// SAFETY: `Synth` owns its whole processor graph. Its `Rc`/`RefCell`
// handles all point inside that graph and none is ever handed out of this
// wrapper, so moving the wrapper moves every handle together and no two
// threads can reach the same `Rc`. The wrapper is only used from one thread
// at a time (`Engine` methods take `&mut self`).
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

    /// `SynthBase::setTuning`: the engine converts notes through a copy of
    /// `tuning`. Upstream calls this after every preset load.
    pub fn set_tuning(&mut self, tuning: &Tuning) {
        self.synth.set_tuning(tuning);
    }

    /// The wrapped synth.
    pub fn synth(&mut self) -> &mut Synth {
        &mut self.synth
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
