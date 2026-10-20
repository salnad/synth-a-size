// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Four `poly_float` rows.
//!
//! Ports `vital/src/synthesis/framework/matrix.h`.

use crate::PolyFloat;

/// `vital::matrix`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Matrix {
    pub row0: PolyFloat,
    pub row1: PolyFloat,
    pub row2: PolyFloat,
    pub row3: PolyFloat,
}

impl Matrix {
    #[inline(always)]
    pub fn new(
        row0: impl Into<PolyFloat>,
        row1: impl Into<PolyFloat>,
        row2: impl Into<PolyFloat>,
        row3: impl Into<PolyFloat>,
    ) -> Self {
        Self {
            row0: row0.into(),
            row1: row1.into(),
            row2: row2.into(),
            row3: row3.into(),
        }
    }

    #[inline(always)]
    pub fn transpose(&mut self) {
        PolyFloat::transpose(
            &mut self.row0,
            &mut self.row1,
            &mut self.row2,
            &mut self.row3,
        );
    }

    #[inline(always)]
    pub fn interpolate_columns(&mut self, other: &Matrix, t: PolyFloat) {
        self.row0 = PolyFloat::mul_add(self.row0, other.row0 - self.row0, t);
        self.row1 = PolyFloat::mul_add(self.row1, other.row1 - self.row1, t);
        self.row2 = PolyFloat::mul_add(self.row2, other.row2 - self.row2, t);
        self.row3 = PolyFloat::mul_add(self.row3, other.row3 - self.row3, t);
    }

    #[inline(always)]
    pub fn interpolate_rows(&mut self, other: &Matrix, t: PolyFloat) {
        let t = t.to_array();
        self.row0 = PolyFloat::mul_add(self.row0, other.row0 - self.row0, t[0].into());
        self.row1 = PolyFloat::mul_add(self.row1, other.row1 - self.row1, t[1].into());
        self.row2 = PolyFloat::mul_add(self.row2, other.row2 - self.row2, t[2].into());
        self.row3 = PolyFloat::mul_add(self.row3, other.row3 - self.row3, t[3].into());
    }

    /// `((row0 + row1) + row2) + row3`.
    #[inline(always)]
    pub fn sum_rows(&self) -> PolyFloat {
        self.row0 + self.row1 + self.row2 + self.row3
    }

    #[inline(always)]
    pub fn multiply_and_sum_rows(&self, other: &Matrix) -> PolyFloat {
        let row01 = PolyFloat::mul_add(self.row0 * other.row0, self.row1, other.row1);
        let row012 = PolyFloat::mul_add(row01, self.row2, other.row2);
        PolyFloat::mul_add(row012, self.row3, other.row3)
    }
}
