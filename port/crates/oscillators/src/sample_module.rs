// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::SampleModule`: the sample player with its `sample_*` controls.
//!
//! Ports `vital/src/synthesis/modules/sample_module.{h,cpp}`.

use std::cell::Cell;
use std::rc::Rc;

use sas_dsp_core::{Output, Processor, ProcessorImpl, ProcessorRef, SynthModule};
use sas_simd::PolyFloat;

use crate::sample_source::{self as src, SampleSource, SharedSample};

// Input indices.
pub const RESET: usize = 0;
pub const MIDI: usize = 1;
pub const NOTE_COUNT: usize = 2;
pub const NUM_INPUTS: usize = 3;

// Output indices.
pub const RAW: usize = 0;
pub const LEVELLED: usize = 1;
pub const NUM_OUTPUTS: usize = 2;

/// `vital::SampleModule`'s members. A clone shares the player, the sample
/// and the on-state, as upstream's copy does.
#[derive(Clone)]
pub struct SampleModule {
    was_on: Rc<Cell<bool>>,
    sampler: ProcessorRef,
    sample: SharedSample,
    phase_output: Rc<Output>,
    on: Option<ProcessorRef>,
}

impl SampleModule {
    /// The kernel of `SampleModule()`; constructs the player, which takes
    /// two seeds from the global counter.
    pub fn new() -> Self {
        let source = SampleSource::new();
        let sample = source.sample().clone();
        let phase_output = source.phase_output().clone();
        let sampler = Processor::new(source, src::NUM_INPUTS, src::NUM_OUTPUTS, false, 1);
        Self {
            was_on: Rc::new(Cell::new(true)),
            sampler,
            sample,
            phase_output,
            on: None,
        }
    }

    /// `new SampleModule()`.
    pub fn create() -> ProcessorRef {
        SynthModule::create_with(Self::new(), NUM_INPUTS, NUM_OUTPUTS, false)
    }

    /// `getSample()`.
    pub fn sample(&self) -> &SharedSample {
        &self.sample
    }

    /// `getPhaseOutput()`.
    pub fn phase_output(&self) -> &Rc<Output> {
        &self.phase_output
    }
}

impl Default for SampleModule {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessorImpl for SampleModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let on = this.create_base_control("sample_on", false, false);
        let random_phase = this.create_base_control("sample_random_phase", false, false);
        let looping = this.create_base_control("sample_loop", false, false);
        let bounce = this.create_base_control("sample_bounce", false, false);
        let keytrack = this.create_base_control("sample_keytrack", false, false);
        let transpose_quantize =
            this.create_base_control("sample_transpose_quantize", false, false);
        let transpose = this.create_poly_mod_control("sample_transpose", false, false, None, None);
        let tune = this.create_poly_mod_control("sample_tune", false, false, None, None);
        let level = this.create_poly_mod_control("sample_level", true, true, None, None);
        let pan = this.create_poly_mod_control("sample_pan", false, false, None, None);

        let sampler = &self.sampler;
        sampler.use_input(&this.input(RESET), src::RESET);
        sampler.use_input(&this.input(MIDI), src::MIDI);
        sampler.use_input(&this.input(NOTE_COUNT), src::NOTE_COUNT);
        sampler.plug(&random_phase, src::RANDOM_PHASE);
        sampler.plug(&keytrack, src::KEYTRACK);
        sampler.plug(&looping, src::LOOP);
        sampler.plug(&bounce, src::BOUNCE);
        sampler.plug(&transpose, src::TRANSPOSE);
        sampler.plug(&transpose_quantize, src::TRANSPOSE_QUANTIZE);
        sampler.plug(&tune, src::TUNE);
        sampler.plug(&level, src::LEVEL);
        sampler.plug(&pan, src::PAN);
        sampler.use_output(&this.output(RAW), src::RAW);
        sampler.use_output(&this.output(LEVELLED), src::LEVELLED);

        this.add_processor(sampler);
        self.on = Some(on);
        this.router_init();
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let on = self.on.as_ref().is_some_and(|on| on.value() != 0.0);

        if on {
            this.router_process(num_samples);
        } else if self.was_on.get() {
            this.output(RAW).clear_buffer();
            this.output(LEVELLED).clear_buffer();
            self.phase_output.set_at(0, PolyFloat::zero());
        }

        self.was_on.set(on);
    }
}
