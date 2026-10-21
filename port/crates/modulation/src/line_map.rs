// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::LineMap`: maps a control-rate phase through a line shape (the
//! voice handler's note, velocity and aftertouch mappings).
//!
//! Ports `vital/src/synthesis/modulators/line_map.h` and `line_map.cpp`.

use std::cell::RefCell;
use std::rc::Rc;

use sas_common::LineGenerator;
use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_simd::poly_utils as utils;
use sas_simd::{PolyFloat, PolyInt};

/// Kernel of `vital::LineMap`.
#[derive(Clone)]
pub struct LineMap {
    source: Rc<RefCell<LineGenerator>>,
}

impl LineMap {
    pub const MAX_POWER: f32 = 20.0;

    pub const VALUE: usize = 0;
    pub const PHASE: usize = 1;
    pub const NUM_OUTPUTS: usize = 2;

    /// `new LineMap(source)`.
    pub fn create(source: Rc<RefCell<LineGenerator>>) -> ProcessorRef {
        Processor::new(LineMap { source }, 1, Self::NUM_OUTPUTS, true, 1)
    }

    /// `process(phase)`.
    pub fn process_phase(&mut self, this: &Processor, phase: PolyFloat) {
        let source = self.source.borrow();
        let buffer = source.cubic_interpolation_buffer();
        let resolution = source.resolution() as i32;
        let boost = utils::clamp(phase * resolution as f32, 0.0, resolution as f32);
        let indices = utils::clamp_int(
            utils::to_int(boost),
            PolyInt::zero(),
            PolyInt::splat((resolution - 1) as u32),
        );
        let t = boost - utils::to_float(indices);

        let interpolation_matrix = utils::get_polynomial_interpolation_matrix(t);
        let mut value_matrix = utils::get_value_matrix(buffer, indices);

        value_matrix.transpose();

        let result = utils::clamp(
            interpolation_matrix.multiply_and_sum_rows(&value_matrix),
            -1.0,
            1.0,
        );
        this.output(Self::VALUE).set_at(0, result);
        this.output(Self::PHASE).set_at(0, phase);
    }
}

impl ProcessorImpl for LineMap {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, _num_samples: usize) {
        self.process_phase(this, this.input(0).at(0));
    }
}
