// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Decimator`: a chain of up to `max_stages` IIR halfband
//! decimators that brings an oversampled input back to its own rate.
//!
//! Ports `vital/src/synthesis/filters/decimator.h` and `decimator.cpp`. The
//! stage count follows the input owner's sample rate at each block.

use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_simd::PolyMask;

use crate::iir_halfband_decimator::{self, IirHalfbandDecimator};

/// Input index `kAudio`.
pub const AUDIO: usize = 0;
/// `kNumInputs`.
pub const NUM_INPUTS: usize = 1;

/// `vital::Decimator` (a `ProcessorRouter`).
pub struct Decimator {
    num_stages: i32,
    max_stages: i32,
    stages: Vec<ProcessorRef>,
}

impl Decimator {
    /// `new Decimator(max_stages)`: `ProcessorRouter(kNumInputs, 1)` with
    /// `max_stages` halfband stages.
    pub fn create(max_stages: i32) -> ProcessorRef {
        let router = Processor::new_router(
            Decimator {
                num_stages: -1,
                max_stages,
                stages: Vec::new(),
            },
            NUM_INPUTS,
            1,
            false,
        );
        let mut stages = Vec::new();
        for i in 0..max_stages {
            let stage = IirHalfbandDecimator::create();
            stage.set_oversample_amount(1 << (max_stages - i - 1));
            router.add_processor(&stage);
            stages.push(stage);
        }
        router.with::<Decimator, _>(|decimator, _| decimator.stages = stages);
        router
    }
}

impl ProcessorImpl for Decimator {
    /// Upstream's `clone()` is `VITAL_ASSERT(false); return nullptr;`: this
    /// processor only lives in the mono engine path and is never cloned.
    fn clone_box(&self) -> Box<dyn ProcessorImpl> {
        panic!("Decimator cannot be cloned (upstream asserts in clone())")
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    /// Upstream's `init` wires the stages and does not call
    /// `ProcessorRouter::init`.
    fn init(&mut self, this: &Processor) {
        self.stages[0].use_input(&this.input(AUDIO), iir_halfband_decimator::AUDIO);
        self.stages[0].use_output(&this.output(0), 0);
        for i in 1..self.max_stages as usize {
            self.stages[i].plug(&self.stages[i - 1], iir_halfband_decimator::AUDIO);
            self.stages[i].use_output(&this.output(0), 0);
        }
    }

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        for stage in &self.stages {
            stage.reset(reset_mask);
        }
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let mut num_stages = 0;
        let input = this.input(AUDIO);
        if let Some(owner) = input.source_owner() {
            let mut input_sample_rate = owner.sample_rate();
            let output_sample_rate = this.sample_rate();
            while input_sample_rate > output_sample_rate {
                num_stages += 1;
                input_sample_rate /= 2;
            }
        }

        if num_stages == 0 {
            let source = input.buffer();
            let dest = this.output_buffer(0);
            for i in 0..num_samples {
                dest[i].set(source[i].get());
            }
            return;
        }

        if num_stages != self.num_stages {
            for stage in &self.stages[..num_stages as usize] {
                stage.reset(PolyMask::full());
            }
            self.num_stages = num_stages;
            for (i, stage) in self.stages.iter().enumerate() {
                let i = i as i32;
                let should_enable = i < num_stages;
                stage.enable(should_enable);
                stage.with::<IirHalfbandDecimator, _>(|stage, _| {
                    stage.set_sharp_cutoff(i == num_stages - 1)
                });
                if should_enable {
                    stage.set_oversample_amount(1 << (num_stages - i - 1));
                }
            }
        }

        this.router_process(num_samples);
    }

    /// Upstream overrides `setOversampleAmount` with a no-op: the decimator
    /// stays at the base rate.
    fn set_oversample_amount(&mut self, _this: &Processor, _oversample: i32) {}
}
