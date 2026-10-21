// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::VocalTract`: the formant module's third style. Upstream's
//! processing is empty, so its output buffer keeps whatever was last
//! written to it (the formant module shares that buffer with its formant
//! filters).
//!
//! Ports `vital/src/synthesis/filters/vocal_tract.h` and `vocal_tract.cpp`.

use sas_dsp_core::output::Buffer;
use sas_dsp_core::sas_simd::PolyMask;
use sas_dsp_core::{processor_boilerplate, Processor, ProcessorImpl, ProcessorRef};

/// `vital::VocalTract`.
#[derive(Clone, Default)]
pub struct VocalTract;

impl VocalTract {
    pub const AUDIO: usize = 0;
    pub const RESET: usize = 1;
    pub const BLEND: usize = 2;
    pub const TONGUE_POSITION: usize = 3;
    pub const TONGUE_HEIGHT: usize = 4;
    pub const NUM_INPUTS: usize = 5;

    /// `new VocalTract()`: `ProcessorRouter(kNumInputs, 1)`.
    pub fn create() -> ProcessorRef {
        Processor::new_router(VocalTract, Self::NUM_INPUTS, 1, false)
    }
}

impl ProcessorImpl for VocalTract {
    processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio_in = this.input_buffer(Self::AUDIO);
        self.process_with_input(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, _this: &Processor, _audio_in: &Buffer, _num_samples: usize) {}

    fn reset(&mut self, _this: &Processor, _reset_mask: PolyMask) {}

    fn hard_reset(&mut self, this: &Processor) {
        self.reset(this, PolyMask::full());
    }
}
