// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::OnePoleFilter`: a trapezoidal one-pole lowpass with an optional
//! saturator in the loop.
//!
//! Ports `vital/src/synthesis/filters/one_pole_filter.h`. The template
//! parameter `saturate` (default `utils::pass`) is a function pointer here.

use sas_simd::common::PI;
use sas_simd::poly_utils as utils;
use sas_simd::{PolyFloat, PolyMask};

/// A saturation function (`poly_float (*)(poly_float)`).
pub type Saturate = fn(PolyFloat) -> PolyFloat;

fn pass(value: PolyFloat) -> PolyFloat {
    value
}

/// `vital::OnePoleFilter<saturate>`.
#[derive(Clone, Copy, Debug)]
pub struct OnePoleFilter {
    saturate: Saturate,
    current_state: PolyFloat,
    filter_state: PolyFloat,
    sat_filter_state: PolyFloat,
}

impl Default for OnePoleFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl OnePoleFilter {
    /// `OnePoleFilter<>` (no saturation).
    pub fn new() -> Self {
        Self::with_saturate(pass)
    }

    /// `OnePoleFilter<saturate>`.
    pub fn with_saturate(saturate: Saturate) -> Self {
        let mut filter = OnePoleFilter {
            saturate,
            current_state: PolyFloat::zero(),
            filter_state: PolyFloat::zero(),
            sat_filter_state: PolyFloat::zero(),
        };
        filter.reset(PolyMask::full());
        filter
    }

    #[inline]
    pub fn reset(&mut self, reset_mask: PolyMask) {
        self.current_state = utils::mask_load(self.current_state, 0.0, reset_mask);
        self.filter_state = utils::mask_load(self.filter_state, 0.0, reset_mask);
        self.sat_filter_state = utils::mask_load(self.sat_filter_state, 0.0, reset_mask);
    }

    /// `tickBasic`: no saturation.
    #[inline]
    pub fn tick_basic(&mut self, audio_in: PolyFloat, coefficient: PolyFloat) -> PolyFloat {
        let delta = coefficient * (audio_in - self.filter_state);
        self.filter_state += delta;
        self.current_state = self.filter_state;
        self.filter_state += delta;
        self.current_state
    }

    /// `tick`: saturates the state.
    #[inline]
    pub fn tick(&mut self, audio_in: PolyFloat, coefficient: PolyFloat) -> PolyFloat {
        let delta = coefficient * (audio_in - self.sat_filter_state);
        self.filter_state += delta;
        self.current_state = (self.saturate)(self.filter_state);
        self.filter_state += delta;
        self.sat_filter_state = (self.saturate)(self.filter_state);
        self.current_state
    }

    /// `tickDerivative`: scales the update by the saturator's output.
    #[inline]
    pub fn tick_derivative(&mut self, audio_in: PolyFloat, coefficient: PolyFloat) -> PolyFloat {
        let delta = coefficient * (audio_in - self.filter_state);
        self.filter_state = utils::mul_add(
            self.filter_state,
            (self.saturate)(self.filter_state + delta),
            delta,
        );
        self.current_state = self.filter_state;
        self.filter_state = utils::mul_add(
            self.filter_state,
            (self.saturate)(self.filter_state + delta),
            delta,
        );
        self.sat_filter_state = self.filter_state;
        self.current_state
    }

    pub fn current_state(&self) -> PolyFloat {
        self.current_state
    }

    pub fn next_sat_state(&self) -> PolyFloat {
        self.sat_filter_state
    }

    pub fn next_state(&self) -> PolyFloat {
        self.filter_state
    }

    /// `computeCoefficient(cutoff_frequency, sample_rate)`.
    #[inline]
    pub fn compute_coefficient(cutoff_frequency: PolyFloat, sample_rate: i32) -> PolyFloat {
        let delta_phase = cutoff_frequency * (PI / sample_rate as f32);
        utils::tan(delta_phase / (delta_phase + 1.0))
    }
}
