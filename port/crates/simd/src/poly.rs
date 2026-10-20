// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `poly_float`, `poly_int` and `poly_mask`.
//!
//! Ports `vital/src/synthesis/framework/poly_values.h`. The types are generic
//! over a [`Backend`] so the tests can run the same expression on the native
//! backend and on the [`Scalar`](crate::backend::Scalar) model;
//! engine code uses the [`PolyFloat`](crate::PolyFloat),
//! [`PolyInt`](crate::PolyInt) and [`PolyMask`](crate::PolyMask) aliases.

use core::fmt;
use core::marker::PhantomData;
use core::ops::{
    Add, AddAssign, BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Div, DivAssign,
    Mul, MulAssign, Neg, Not, Sub, SubAssign,
};

use crate::backend::Backend;

/// Number of lanes (`poly_float::kSize`).
pub const SIZE: usize = 4;

/// Four `f32` lanes (`vital::poly_float`).
#[repr(transparent)]
pub struct PolyFloatOf<B: Backend> {
    value: B::F,
    _backend: PhantomData<B>,
}

/// Four `u32` lanes (`vital::poly_int`, also `vital::poly_mask`).
#[repr(transparent)]
pub struct PolyIntOf<B: Backend> {
    value: B::I,
    _backend: PhantomData<B>,
}

impl<B: Backend> Clone for PolyFloatOf<B> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}
impl<B: Backend> Copy for PolyFloatOf<B> {}

impl<B: Backend> Clone for PolyIntOf<B> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}
impl<B: Backend> Copy for PolyIntOf<B> {}

impl<B: Backend> fmt::Debug for PolyFloatOf<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PolyFloat").field(&self.to_array()).finish()
    }
}

impl<B: Backend> fmt::Debug for PolyIntOf<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lanes = self.to_array();
        write!(
            f,
            "PolyInt([{:#010x}, {:#010x}, {:#010x}, {:#010x}])",
            lanes[0], lanes[1], lanes[2], lanes[3]
        )
    }
}

impl<B: Backend> Default for PolyFloatOf<B> {
    #[inline(always)]
    fn default() -> Self {
        Self::zero()
    }
}

impl<B: Backend> Default for PolyIntOf<B> {
    #[inline(always)]
    fn default() -> Self {
        Self::zero()
    }
}

impl<B: Backend> PolyIntOf<B> {
    /// Number of lanes.
    pub const SIZE: usize = SIZE;
    /// All bits set (`kFullMask`).
    pub const FULL_MASK: u32 = u32::MAX;
    /// Sign bit (`kSignMask`).
    pub const SIGN_MASK: u32 = 0x8000_0000;
    /// Everything but the sign bit (`kNotSignMask`).
    pub const NOT_SIGN_MASK: u32 = Self::FULL_MASK ^ Self::SIGN_MASK;

    #[inline(always)]
    /// Wraps a backend vector.
    pub fn from_raw(value: B::I) -> Self {
        Self {
            value,
            _backend: PhantomData,
        }
    }

    /// The backend vector.
    #[inline(always)]
    pub fn raw(self) -> B::I {
        self.value
    }

    /// All lanes zero (`poly_int()`).
    #[inline(always)]
    pub fn zero() -> Self {
        Self::splat(0)
    }

    /// All lanes `value` (`poly_int(uint32_t)`).
    #[inline(always)]
    pub fn splat(value: u32) -> Self {
        Self::from_raw(B::i_splat(value))
    }

    /// Mask with every bit set in every lane (`poly_mask(-1)`).
    #[inline(always)]
    pub fn full() -> Self {
        Self::splat(Self::FULL_MASK)
    }

    /// Four lanes (`poly_int(first, second, third, fourth)`).
    #[inline(always)]
    pub fn new(first: u32, second: u32, third: u32, fourth: u32) -> Self {
        Self::from_array([first, second, third, fourth])
    }

    /// `(first, second, first, second)` (`poly_int(first, second)`).
    #[inline(always)]
    pub fn stereo(first: u32, second: u32) -> Self {
        Self::new(first, second, first, second)
    }

    #[inline(always)]
    pub fn from_array(lanes: [u32; 4]) -> Self {
        Self::from_raw(B::i_from_array(lanes))
    }

    #[inline(always)]
    pub fn to_array(self) -> [u32; 4] {
        B::i_to_array(self.value)
    }

    /// Lane `index` (`access` / `operator[]`).
    #[inline(always)]
    pub fn get(self, index: usize) -> u32 {
        self.to_array()[index]
    }

    /// Sets lane `index`.
    #[inline(always)]
    pub fn set(&mut self, index: usize, value: u32) {
        let mut lanes = self.to_array();
        lanes[index] = value;
        *self = Self::from_array(lanes);
    }

    /// Unsigned per-lane maximum.
    #[inline(always)]
    pub fn max(one: Self, two: Self) -> Self {
        Self::from_raw(B::i_max(one.value, two.value))
    }

    /// Per-lane minimum (site A7): compares **signed** on SSE2 (upstream's
    /// `_mm_cmpgt_epi32` without the sign flip), unsigned on NEON.
    #[inline(always)]
    pub fn min(one: Self, two: Self) -> Self {
        Self::from_raw(B::i_min(one.value, two.value))
    }

    #[inline(always)]
    pub fn equal(one: Self, two: Self) -> Self {
        Self::from_raw(B::i_eq(one.value, two.value))
    }

    /// Unsigned `one > two`.
    #[inline(always)]
    pub fn greater_than(one: Self, two: Self) -> Self {
        Self::from_raw(B::i_gt(one.value, two.value))
    }

    /// Unsigned `one < two`.
    #[inline(always)]
    pub fn less_than(one: Self, two: Self) -> Self {
        Self::from_raw(B::i_gt(two.value, one.value))
    }

    /// Wrapping sum of the lanes.
    #[inline(always)]
    pub fn sum(self) -> u32 {
        B::i_sum(self.value)
    }

    /// Non-zero when any lane of a comparison mask is set (site A8). The
    /// value is per target: SSE2 returns a 16-bit mask of each byte's top bit
    /// (`PMOVMSKB`), NEON the largest lane value.
    #[inline(always)]
    pub fn any_mask(self) -> u32 {
        B::i_any_mask(self.value)
    }
}

impl<B: Backend> PolyFloatOf<B> {
    /// Number of lanes.
    pub const SIZE: usize = SIZE;

    #[inline(always)]
    /// Wraps a backend vector.
    pub fn from_raw(value: B::F) -> Self {
        Self {
            value,
            _backend: PhantomData,
        }
    }

    /// The backend vector.
    #[inline(always)]
    pub fn raw(self) -> B::F {
        self.value
    }

    /// All lanes `0.0` (`poly_float()`).
    #[inline(always)]
    pub fn zero() -> Self {
        Self::splat(0.0)
    }

    /// All lanes `value` (`poly_float(float)`).
    #[inline(always)]
    pub fn splat(value: f32) -> Self {
        Self::from_raw(B::f_splat(value))
    }

    /// Four lanes (`poly_float(first, second, third, fourth)`).
    #[inline(always)]
    pub fn new(first: f32, second: f32, third: f32, fourth: f32) -> Self {
        Self::from_array([first, second, third, fourth])
    }

    /// `(left, right, left, right)` (`poly_float(first, second)`).
    #[inline(always)]
    pub fn stereo(left: f32, right: f32) -> Self {
        Self::new(left, right, left, right)
    }

    #[inline(always)]
    pub fn from_array(lanes: [f32; 4]) -> Self {
        Self::from_raw(B::f_from_array(lanes))
    }

    #[inline(always)]
    pub fn to_array(self) -> [f32; 4] {
        B::f_to_array(self.value)
    }

    /// Loads the first four values of `memory` (`load`, unaligned).
    ///
    /// # Panics
    /// If `memory` has fewer than four elements.
    #[inline(always)]
    pub fn load(memory: &[f32]) -> Self {
        Self::from_array([memory[0], memory[1], memory[2], memory[3]])
    }

    /// Lane `index` (`access` / `operator[]`).
    #[inline(always)]
    pub fn get(self, index: usize) -> f32 {
        self.to_array()[index]
    }

    /// Sets lane `index`.
    #[inline(always)]
    pub fn set(&mut self, index: usize, value: f32) {
        let mut lanes = self.to_array();
        lanes[index] = value;
        *self = Self::from_array(lanes);
    }

    /// `one + two * three`, two roundings (no FMA) on every target.
    #[inline(always)]
    pub fn mul_add(one: Self, two: Self, three: Self) -> Self {
        Self::from_raw(B::f_add(one.value, B::f_mul(two.value, three.value)))
    }

    /// `one - two * three`, two roundings (no FMA).
    #[inline(always)]
    pub fn mul_sub(one: Self, two: Self, three: Self) -> Self {
        Self::from_raw(B::f_sub(one.value, B::f_mul(two.value, three.value)))
    }

    /// Per-lane maximum (site A5). SSE2 (`MAXPS`): `one > two ? one : two`,
    /// so a NaN or a `±0` tie returns `two`. NEON (`FMAX`): NaN propagates and
    /// `max(+0, -0) = +0`.
    #[inline(always)]
    pub fn max(one: Self, two: Self) -> Self {
        Self::from_raw(B::f_max(one.value, two.value))
    }

    /// Per-lane minimum (site A5). SSE2 (`MINPS`): `one < two ? one : two`,
    /// so a NaN or a `±0` tie returns `two`. NEON (`FMIN`): NaN propagates and
    /// `min(+0, -0) = -0`.
    #[inline(always)]
    pub fn min(one: Self, two: Self) -> Self {
        Self::from_raw(B::f_min(one.value, two.value))
    }

    /// Clears the sign bit.
    #[inline(always)]
    pub fn abs(value: Self) -> Self {
        value & PolyIntOf::splat(PolyIntOf::<B>::NOT_SIGN_MASK)
    }

    /// The sign bit of each lane, as a mask value.
    #[inline(always)]
    pub fn sign_mask(value: Self) -> PolyIntOf<B> {
        PolyIntOf::from_raw(B::f_to_bits(
            (value & PolyIntOf::splat(PolyIntOf::<B>::SIGN_MASK)).value,
        ))
    }

    /// Ordered `==` (false on NaN).
    #[inline(always)]
    pub fn equal(one: Self, two: Self) -> PolyIntOf<B> {
        PolyIntOf::from_raw(B::f_eq(one.value, two.value))
    }

    /// `!=` (site A6): unordered on SSE2 (true on NaN), ordered on NEON
    /// (false on NaN).
    #[inline(always)]
    pub fn not_equal(one: Self, two: Self) -> PolyIntOf<B> {
        PolyIntOf::from_raw(B::f_neq(one.value, two.value))
    }

    /// Ordered `>`.
    #[inline(always)]
    pub fn greater_than(one: Self, two: Self) -> PolyIntOf<B> {
        PolyIntOf::from_raw(B::f_gt(one.value, two.value))
    }

    /// Ordered `>=`.
    #[inline(always)]
    pub fn greater_than_or_equal(one: Self, two: Self) -> PolyIntOf<B> {
        PolyIntOf::from_raw(B::f_ge(one.value, two.value))
    }

    /// Ordered `<`.
    #[inline(always)]
    pub fn less_than(one: Self, two: Self) -> PolyIntOf<B> {
        Self::greater_than(two, one)
    }

    /// Ordered `<=`.
    #[inline(always)]
    pub fn less_than_or_equal(one: Self, two: Self) -> PolyIntOf<B> {
        Self::greater_than_or_equal(two, one)
    }

    /// Horizontal sum (site A4): `(v0 + v2) + (v1 + v3)` on SSE2,
    /// `(v0 + v1) + (v2 + v3)` on NEON.
    #[inline(always)]
    pub fn sum(self) -> f32 {
        B::f_sum(self.value)
    }

    /// In-place 4 × 4 transpose of four rows.
    #[inline(always)]
    pub fn transpose(row0: &mut Self, row1: &mut Self, row2: &mut Self, row3: &mut Self) {
        B::f_transpose(
            &mut row0.value,
            &mut row1.value,
            &mut row2.value,
            &mut row3.value,
        );
    }
}

impl<B: Backend> From<f32> for PolyFloatOf<B> {
    #[inline(always)]
    fn from(value: f32) -> Self {
        Self::splat(value)
    }
}

impl<B: Backend> From<[f32; 4]> for PolyFloatOf<B> {
    #[inline(always)]
    fn from(lanes: [f32; 4]) -> Self {
        Self::from_array(lanes)
    }
}

impl<B: Backend> From<u32> for PolyIntOf<B> {
    #[inline(always)]
    fn from(value: u32) -> Self {
        Self::splat(value)
    }
}

impl<B: Backend> From<[u32; 4]> for PolyIntOf<B> {
    #[inline(always)]
    fn from(lanes: [u32; 4]) -> Self {
        Self::from_array(lanes)
    }
}

macro_rules! float_binop {
    ($trait:ident, $method:ident, $assign_trait:ident, $assign_method:ident, $op:ident) => {
        impl<B: Backend> $trait for PolyFloatOf<B> {
            type Output = Self;
            #[inline(always)]
            fn $method(self, rhs: Self) -> Self {
                Self::from_raw(B::$op(self.value, rhs.value))
            }
        }
        impl<B: Backend> $trait<f32> for PolyFloatOf<B> {
            type Output = Self;
            #[inline(always)]
            fn $method(self, rhs: f32) -> Self {
                Self::from_raw(B::$op(self.value, B::f_splat(rhs)))
            }
        }
        impl<B: Backend> $trait<PolyFloatOf<B>> for f32 {
            type Output = PolyFloatOf<B>;
            #[inline(always)]
            fn $method(self, rhs: PolyFloatOf<B>) -> PolyFloatOf<B> {
                PolyFloatOf::from_raw(B::$op(B::f_splat(self), rhs.value))
            }
        }
        impl<B: Backend> $assign_trait for PolyFloatOf<B> {
            #[inline(always)]
            fn $assign_method(&mut self, rhs: Self) {
                self.value = B::$op(self.value, rhs.value);
            }
        }
        impl<B: Backend> $assign_trait<f32> for PolyFloatOf<B> {
            #[inline(always)]
            fn $assign_method(&mut self, rhs: f32) {
                self.value = B::$op(self.value, B::f_splat(rhs));
            }
        }
    };
}

float_binop!(Add, add, AddAssign, add_assign, f_add);
float_binop!(Sub, sub, SubAssign, sub_assign, f_sub);
float_binop!(Mul, mul, MulAssign, mul_assign, f_mul);
float_binop!(Div, div, DivAssign, div_assign, f_div);

macro_rules! float_bitop {
    ($trait:ident, $method:ident, $assign_trait:ident, $assign_method:ident, $op:ident) => {
        impl<B: Backend> $trait<PolyIntOf<B>> for PolyFloatOf<B> {
            type Output = Self;
            #[inline(always)]
            fn $method(self, rhs: PolyIntOf<B>) -> Self {
                Self::from_raw(B::$op(self.value, rhs.value))
            }
        }
        impl<B: Backend> $trait<u32> for PolyFloatOf<B> {
            type Output = Self;
            #[inline(always)]
            fn $method(self, rhs: u32) -> Self {
                Self::from_raw(B::$op(self.value, B::i_splat(rhs)))
            }
        }
        impl<B: Backend> $assign_trait<PolyIntOf<B>> for PolyFloatOf<B> {
            #[inline(always)]
            fn $assign_method(&mut self, rhs: PolyIntOf<B>) {
                self.value = B::$op(self.value, rhs.value);
            }
        }
    };
}

float_bitop!(BitAnd, bitand, BitAndAssign, bitand_assign, f_and);
float_bitop!(BitOr, bitor, BitOrAssign, bitor_assign, f_or);
float_bitop!(BitXor, bitxor, BitXorAssign, bitxor_assign, f_xor);

impl<B: Backend> Neg for PolyFloatOf<B> {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self {
        Self::from_raw(B::f_neg(self.value))
    }
}

impl<B: Backend> Not for PolyFloatOf<B> {
    type Output = Self;
    /// Bitwise not (`operator~`).
    #[inline(always)]
    fn not(self) -> Self {
        Self::from_raw(B::f_xor(self.value, B::i_splat(u32::MAX)))
    }
}

macro_rules! int_binop {
    ($trait:ident, $method:ident, $assign_trait:ident, $assign_method:ident, $op:ident) => {
        impl<B: Backend> $trait for PolyIntOf<B> {
            type Output = Self;
            #[inline(always)]
            fn $method(self, rhs: Self) -> Self {
                Self::from_raw(B::$op(self.value, rhs.value))
            }
        }
        impl<B: Backend> $trait<u32> for PolyIntOf<B> {
            type Output = Self;
            #[inline(always)]
            fn $method(self, rhs: u32) -> Self {
                Self::from_raw(B::$op(self.value, B::i_splat(rhs)))
            }
        }
        impl<B: Backend> $assign_trait for PolyIntOf<B> {
            #[inline(always)]
            fn $assign_method(&mut self, rhs: Self) {
                self.value = B::$op(self.value, rhs.value);
            }
        }
        impl<B: Backend> $assign_trait<u32> for PolyIntOf<B> {
            #[inline(always)]
            fn $assign_method(&mut self, rhs: u32) {
                self.value = B::$op(self.value, B::i_splat(rhs));
            }
        }
    };
}

int_binop!(Add, add, AddAssign, add_assign, i_add);
int_binop!(Sub, sub, SubAssign, sub_assign, i_sub);
int_binop!(Mul, mul, MulAssign, mul_assign, i_mul);
int_binop!(BitAnd, bitand, BitAndAssign, bitand_assign, i_and);
int_binop!(BitOr, bitor, BitOrAssign, bitor_assign, i_or);
int_binop!(BitXor, bitxor, BitXorAssign, bitxor_assign, i_xor);

impl<B: Backend> Neg for PolyIntOf<B> {
    type Output = Self;
    /// Wrapping negation.
    #[inline(always)]
    fn neg(self) -> Self {
        Self::from_raw(B::i_neg(self.value))
    }
}

impl<B: Backend> Not for PolyIntOf<B> {
    type Output = Self;
    #[inline(always)]
    fn not(self) -> Self {
        Self::from_raw(B::i_xor(self.value, B::i_splat(u32::MAX)))
    }
}

/// Raw backend access for the shuffle and conversion helpers in
/// [`poly_utils`](crate::poly_utils).
impl<B: Backend> PolyFloatOf<B> {
    #[inline(always)]
    pub(crate) fn map_raw(self, f: impl FnOnce(B::F) -> B::F) -> Self {
        Self::from_raw(f(self.value))
    }
}

impl<B: Backend> PolyIntOf<B> {
    #[inline(always)]
    pub(crate) fn map_raw(self, f: impl FnOnce(B::I) -> B::I) -> Self {
        Self::from_raw(f(self.value))
    }
}
