// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::PortamentoSlope`: glides from a source to a target pitch.
//!
//! Ports `vital/src/synthesis/utilities/portamento_slope.h` and
//! `portamento_slope.cpp`.

use sas_simd::common::NOTES_PER_OCTAVE;
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat};

use crate::processor::{Processor, ProcessorImpl, ProcessorRef};

/// `vital::PortamentoSlope` (control rate).
#[derive(Clone)]
pub struct PortamentoSlope {
    position: PolyFloat,
}

impl PortamentoSlope {
    pub const MIN_PORTAMENTO_TIME: f32 = 0.001;

    pub const TARGET: usize = 0;
    pub const SOURCE: usize = 1;
    pub const PORTAMENTO_FORCE: usize = 2;
    pub const PORTAMENTO_SCALE: usize = 3;
    pub const RUN_SECONDS: usize = 4;
    pub const SLOPE_POWER: usize = 5;
    pub const RESET: usize = 6;
    pub const NUM_NOTES_PRESSED: usize = 7;
    pub const NUM_INPUTS: usize = 8;

    pub fn create() -> ProcessorRef {
        Processor::new(
            PortamentoSlope {
                position: PolyFloat::zero(),
            },
            Self::NUM_INPUTS,
            1,
            true,
            1,
        )
    }

    /// `processBypass(start)`.
    pub fn process_bypass(&mut self, this: &Processor, _start: usize) {
        self.position = PolyFloat::splat(1.0);
        this.output(0).set_at(0, this.input(Self::TARGET).at(0));
    }
}

impl ProcessorImpl for PortamentoSlope {
    crate::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let force = this.input(Self::PORTAMENTO_FORCE).at(0).get(0) != 0.0;
        let mut run_seconds = this.input(Self::RUN_SECONDS).at(0);
        let active_mask =
            PolyFloat::greater_than(run_seconds, PolyFloat::splat(Self::MIN_PORTAMENTO_TIME));
        if active_mask.any_mask() == 0 {
            self.process_bypass(this, 0);
            return;
        }

        let mut reset_mask = this.get_reset_mask(Self::RESET);
        self.position = utils::mask_load(self.position, 0.0, reset_mask);
        if !force {
            let num_voices = this.input(Self::NUM_NOTES_PRESSED).at(0);
            reset_mask &= PolyFloat::equal(num_voices, PolyFloat::splat(1.0));
            self.position = utils::mask_load(self.position, 1.0, reset_mask);
        }

        let target = this.input(Self::TARGET).at(0);
        let source = this.input(Self::SOURCE).at(0);
        if this.input(Self::PORTAMENTO_SCALE).at(0).get(0) != 0.0 {
            let midi_delta = PolyFloat::abs(target - source);
            run_seconds *= midi_delta * (1.0 / NOTES_PER_OCTAVE as f32);
        }
        let position_delta =
            PolyFloat::splat(1.0 * num_samples as f32) / (run_seconds * this.sample_rate() as f32);
        self.position = utils::clamp(self.position + position_delta, 0.0, 1.0);
        let power = -this.input(Self::SLOPE_POWER).at(0);
        let adjusted_position = futils::power_scale(self.position, power);
        this.output(0)
            .set_at(0, utils::interpolate(source, target, adjusted_position));
    }
}
