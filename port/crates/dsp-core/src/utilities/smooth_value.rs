// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::SmoothValue` and `cr::SmoothValue`: values that glide to a new
//! setting with a one-pole smoother.
//!
//! Ports `vital/src/synthesis/utilities/smooth_value.h` and
//! `smooth_value.cpp`.

use sas_simd::common::PI;
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyMask};

use crate::processor::{Processor, ProcessorImpl, ProcessorRef};
use crate::value::{ValueCore, ValueKernel, NUM_INPUTS};

/// `vital::SmoothValue` (audio rate). Disables itself once settled; `set`
/// re-enables it.
#[derive(Clone, Debug)]
pub struct SmoothValue {
    pub core: ValueCore,
    current_value: PolyFloat,
}

impl SmoothValue {
    /// `kSmoothCutoff` (Hz).
    pub const SMOOTH_CUTOFF: f32 = 5.0;

    /// `new SmoothValue(value)`.
    pub fn create(value: f32) -> ProcessorRef {
        let core = ValueCore::new(PolyFloat::splat(value));
        let processor = Processor::new(
            SmoothValue {
                core,
                current_value: PolyFloat::splat(value),
            },
            NUM_INPUTS,
            1,
            false,
            1,
        );
        core.fill_output(&processor);
        processor
    }

    /// `setHard(value)`: jumps without smoothing.
    pub fn set_hard(&mut self, this: &Processor, value: PolyFloat) {
        this.processor_enable(true);
        self.core.set(this, value);
        self.current_value = value;
    }

    /// `linearInterpolate(num_samples, linear_mask)`.
    pub fn linear_interpolate(
        &mut self,
        this: &Processor,
        num_samples: usize,
        linear_mask: PolyMask,
    ) {
        let mut current_value = self.current_value;
        self.current_value = utils::mask_load(self.current_value, self.core.value, linear_mask);
        let delta_value = (self.core.value - current_value) * (1.0 / num_samples as f32);
        let output = this.output(0);
        let dest = output.buffer();
        for sample in dest.iter().take(num_samples) {
            current_value += delta_value;
            sample.set(utils::mask_load(sample.get(), current_value, linear_mask));
        }

        let max_samples = output.buffer_size();
        for sample in dest.iter().take(max_samples).skip(num_samples) {
            sample.set(self.current_value);
        }
    }
}

impl ValueKernel for SmoothValue {
    fn set(&mut self, this: &Processor, value: PolyFloat) {
        this.processor_enable(true);
        self.core.value = value;
    }

    fn value(&self) -> f32 {
        self.core.value.get(0)
    }
}

impl ProcessorImpl for SmoothValue {
    crate::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let output = this.output(0);
        if utils::equal(self.current_value, self.core.value)
            && utils::equal(self.current_value, output.at(0))
            && utils::equal(self.current_value, output.at(num_samples - 1))
        {
            this.processor_enable(false);
            return;
        }

        let decay = futils::exp_mono(-2.0 * PI * Self::SMOOTH_CUTOFF / this.sample_rate() as f32);
        let mut current_value = self.current_value;
        let target_value = self.core.value;
        let dest = output.buffer();
        for sample in dest.iter().take(num_samples) {
            current_value = utils::interpolate(target_value, current_value, decay);
            sample.set(current_value);
        }

        let equal_mask = PolyFloat::equal(current_value, self.current_value)
            | PolyFloat::equal(self.core.value, self.current_value);
        if equal_mask.any_mask() != 0 {
            self.linear_interpolate(this, num_samples, equal_mask);
        }
        self.current_value = utils::mask_load(current_value, self.current_value, equal_mask);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        self.core.set_oversample_amount(this, oversample);
    }

    fn as_value(&mut self) -> Option<&mut dyn ValueKernel> {
        Some(self)
    }
}

/// `vital::cr::SmoothValue` (control rate; smooths once per block).
#[derive(Clone, Debug)]
pub struct CrSmoothValue {
    pub core: ValueCore,
    current_value: PolyFloat,
}

impl CrSmoothValue {
    /// `kSmoothCutoff` (Hz).
    pub const SMOOTH_CUTOFF: f32 = 20.0;

    /// `new cr::SmoothValue(value)`.
    pub fn create(value: f32) -> ProcessorRef {
        let core = ValueCore::new(PolyFloat::splat(value));
        let processor = Processor::new(
            CrSmoothValue {
                core,
                current_value: PolyFloat::splat(value),
            },
            NUM_INPUTS,
            1,
            true,
            1,
        );
        core.fill_output(&processor);
        processor
    }

    /// `setHard(value)`.
    pub fn set_hard(&mut self, this: &Processor, value: f32) {
        self.core.set(this, PolyFloat::splat(value));
        self.current_value = PolyFloat::splat(value);
    }
}

impl ValueKernel for CrSmoothValue {
    fn set(&mut self, this: &Processor, value: PolyFloat) {
        self.core.set(this, value);
    }

    fn value(&self) -> f32 {
        self.core.value.get(0)
    }
}

impl ProcessorImpl for CrSmoothValue {
    crate::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let decay = futils::exp_mono(
            -2.0 * PI * Self::SMOOTH_CUTOFF * num_samples as f32 / this.sample_rate() as f32,
        );
        self.current_value = utils::interpolate(self.core.value, self.current_value, decay);
        this.output(0).set_at(0, self.current_value);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        self.core.set_oversample_amount(this, oversample);
    }

    fn as_value(&mut self) -> Option<&mut dyn ValueKernel> {
        Some(self)
    }
}
