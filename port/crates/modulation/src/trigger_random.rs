// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::TriggerRandom`: a new random value per voice on each note-on
//! (the voice handler's `random` modulation source).
//!
//! Ports `vital/src/synthesis/modulators/trigger_random.h` and
//! `trigger_random.cpp`.

use sas_common::RandomGenerator;
use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_simd::PolyFloat;

/// Kernel of `vital::TriggerRandom`.
pub struct TriggerRandom {
    value: PolyFloat,
    random_generator: RandomGenerator,
}

impl Clone for TriggerRandom {
    /// The implicit copy constructor: the `RandomGenerator` copy takes a new
    /// seed from the global counter.
    fn clone(&self) -> Self {
        TriggerRandom {
            value: self.value,
            random_generator: RandomGenerator::from_global_counter(
                self.random_generator.min(),
                self.random_generator.max(),
            ),
        }
    }
}

impl TriggerRandom {
    pub const RESET: usize = 0;
    pub const NUM_INPUTS: usize = 1;

    /// `new TriggerRandom()`: takes one seed from the global counter.
    pub fn create() -> ProcessorRef {
        Processor::new(
            TriggerRandom {
                value: PolyFloat::zero(),
                random_generator: RandomGenerator::from_global_counter(0.0, 1.0),
            },
            Self::NUM_INPUTS,
            1,
            true,
            1,
        )
    }
}

impl ProcessorImpl for TriggerRandom {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, _num_samples: usize) {
        let trigger_mask = this.get_reset_mask(Self::RESET);
        if trigger_mask.any_mask() != 0 {
            let triggered = PolyFloat::splat(1.0) & trigger_mask;
            for i in (0..PolyFloat::SIZE).step_by(2) {
                if triggered.get(i) != 0.0 {
                    let rand_value = self.random_generator.next();
                    self.value.set(i, rand_value);
                    self.value.set(i + 1, rand_value);
                }
            }
        }

        this.output(0).set_at(0, self.value);
    }
}
