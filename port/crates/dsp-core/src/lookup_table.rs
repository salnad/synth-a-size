// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::OneDimLookup`: a tabulated function read with Catmull-Rom
//! interpolation.
//!
//! Ports `vital/src/synthesis/lookups/lookup_table.h`. The index uses
//! `utils::toInt` (site A1 of `migration/decisions/reference-arm.md`:
//! rounding on x86_64, truncation on aarch64) and the poly_int clamp
//! (site A7), exactly as upstream.

use sas_simd::poly_utils as utils;
use sas_simd::{PolyFloat, PolyInt};

/// `vital::OneDimLookup<function, resolution>`.
#[derive(Clone, Debug)]
pub struct OneDimLookup {
    lookup: Vec<f32>,
    scale: f32,
    resolution: usize,
}

impl OneDimLookup {
    /// `kExtraValues`.
    pub const EXTRA_VALUES: usize = 4;

    /// `OneDimLookup(scale)` tabulating `function` over `[0, scale]` with
    /// `resolution` points (plus padding).
    pub fn new(function: impl Fn(f32) -> f32, resolution: usize, scale: f32) -> OneDimLookup {
        let lookup = (0..resolution + Self::EXTRA_VALUES)
            .map(|i| {
                let t = (i as f32 - 1.0) / (resolution as f32 - 1.0);
                function(t * scale)
            })
            .collect();
        OneDimLookup {
            lookup,
            scale: resolution as f32 / scale,
            resolution,
        }
    }

    /// `cubicLookup(value)`.
    pub fn cubic_lookup(&self, value: PolyFloat) -> PolyFloat {
        let boost = value * self.scale;
        let indices = utils::clamp_int(
            utils::to_int(boost),
            PolyInt::splat(0),
            PolyInt::splat(self.resolution as u32),
        );
        let t = boost - utils::to_float(indices);
        let interpolation_matrix = utils::get_catmull_interpolation_matrix(t);
        let mut value_matrix = utils::get_value_matrix(&self.lookup, indices);
        value_matrix.transpose();
        interpolation_matrix.multiply_and_sum_rows(&value_matrix)
    }

    /// The table, including padding.
    pub fn table(&self) -> &[f32] {
        &self.lookup
    }
}
