// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fast approximations (`vital::futils`).
//!
//! Ports `vital/src/synthesis/framework/futils.h`, including upstream's own
//! polynomial `exp2`/`log2`/`tanh`/`sin` approximations; nothing here calls
//! libm. Upstream overloads most names for `mono_float` and `poly_float`; the
//! mono versions here have a `_mono` suffix. Each overload follows its own
//! upstream formula, and several mono and poly overloads are **not** the same
//! arithmetic (see `tests/futils_mono_vs_poly.rs` for the list), including
//! `db_to_magnitude_mono`, which scales by `log2(e)` where the poly version
//! does not.

use crate::common::{MIDI_0_FREQUENCY, NOTES_PER_OCTAVE};
use crate::poly_utils as utils;
use crate::{PolyFloat, PolyInt};

const DB_GAIN_CONVERSION_MULT: f32 = 6.020_599_913_29;
const DB_MAGNITUDE_CONVERSION_MULT: f32 = 1.0 / DB_GAIN_CONVERSION_MULT;
const EXP_CONVERSION_MULT: f32 = 1.442_695_040_89;
const LOG_CONVERSION_MULT: f32 = 0.693_147_180_56;

#[inline(always)]
fn splat(x: f32) -> PolyFloat {
    PolyFloat::splat(x)
}

/// Polynomial `2^x`.
#[inline(always)]
pub fn exp2(exponent: PolyFloat) -> PolyFloat {
    const C0: f32 = 1.0;
    const C1: f32 = (16970.0f64 / 24483.0f64) as f32;
    const C2: f32 = (1960.0f64 / 8161.0f64) as f32;
    const C3: f32 = (1360.0f64 / 24483.0f64) as f32;
    const C4: f32 = (80.0f64 / 8161.0f64) as f32;
    const C5: f32 = (32.0f64 / 24483.0f64) as f32;

    let integer = utils::round_to_int(exponent);
    let t = exponent - utils::to_float(integer);
    let int_pow = utils::pow2_to_float(integer);

    let cubic = t * (t * (t * C5 + C4) + C3) + C2;
    let interpolate = t * (t * cubic + C1) + C0;
    int_pow * interpolate
}

/// Polynomial `log2(x)` from the exponent bits plus a mantissa polynomial.
#[inline(always)]
pub fn log2(value: PolyFloat) -> PolyFloat {
    const C0: f32 = (-1819.0f64 / 651.0f64) as f32;
    const C1: f32 = 5.0f64 as f32;
    const C2: f32 = (-10.0f64 / 3.0f64) as f32;
    const C3: f32 = (10.0f64 / 7.0f64) as f32;
    const C4: f32 = (-1.0f64 / 3.0f64) as f32;
    const C5: f32 = (1.0f64 / 31.0f64) as f32;

    let floored_log2 = utils::shift_right::<23>(utils::reinterpret_to_int(value)) - 0x7f;
    let t = (value & PolyInt::splat(0x7f_ffff)) | PolyInt::splat(0x7f << 23);

    let cubic = t * (t * (t * C5 + C4) + C3) + C2;
    let interpolate = t * (t * cubic + C1) + C0;
    utils::to_float(floored_log2) + interpolate
}

#[inline(always)]
pub fn cheap_exp2(exponent: PolyFloat) -> PolyFloat {
    const C0: f32 = 1.0;
    const C1: f32 = (12.0f64 / 17.0f64) as f32;
    const C2: f32 = (4.0f64 / 17.0f64) as f32;

    let integer = utils::round_to_int(exponent);
    let t = exponent - utils::to_float(integer);
    let int_pow = utils::pow2_to_float(integer);

    let interpolate = t * (t * C2 + C1) + C0;
    int_pow * interpolate
}

#[inline(always)]
pub fn cheap_log2(value: PolyFloat) -> PolyFloat {
    const C0: f32 = (-5.0f64 / 3.0f64) as f32;
    const C1: f32 = 2.0f64 as f32;
    const C2: f32 = (-1.0f64 / 3.0f64) as f32;

    let floored_log2 = utils::shift_right::<23>(utils::reinterpret_to_int(value)) - 0x7f;
    let t = (value & PolyInt::splat(0x7f_ffff)) | PolyInt::splat(0x7f << 23);

    let interpolate = t * (t * C2 + C1) + C0;
    utils::to_float(floored_log2) + interpolate
}

/// Lane 0 of the poly [`exp2`].
#[inline(always)]
pub fn exp2_mono(value: f32) -> f32 {
    exp2(splat(value)).get(0)
}

/// Lane 0 of the poly [`log2`].
#[inline(always)]
pub fn log2_mono(value: f32) -> f32 {
    log2(splat(value)).get(0)
}

#[inline(always)]
pub fn exp_mono(exponent: f32) -> f32 {
    exp2_mono(exponent * EXP_CONVERSION_MULT)
}

#[inline(always)]
pub fn log_mono(value: f32) -> f32 {
    log2_mono(value) * LOG_CONVERSION_MULT
}

#[inline(always)]
pub fn exp_half_mono(exponent: f32) -> f32 {
    exp2_mono(-exponent)
}

#[inline(always)]
pub fn pow_mono(base: f32, exponent: f32) -> f32 {
    exp2_mono(log2_mono(base) * exponent)
}

/// Applies a scalar function to each lane.
#[inline(always)]
pub fn map(value: PolyFloat, func: impl Fn(f32) -> f32) -> PolyFloat {
    utils::map(value, func)
}

#[inline(always)]
pub fn exp(exponent: PolyFloat) -> PolyFloat {
    exp2(exponent * EXP_CONVERSION_MULT)
}

#[inline(always)]
pub fn log(value: PolyFloat) -> PolyFloat {
    log2(value) * LOG_CONVERSION_MULT
}

#[inline(always)]
pub fn exp_half(exponent: PolyFloat) -> PolyFloat {
    exp2(-exponent)
}

#[inline(always)]
pub fn pow(base: PolyFloat, exponent: PolyFloat) -> PolyFloat {
    exp2(log2(base) * exponent)
}

#[inline(always)]
pub fn cheap_pow(base: PolyFloat, exponent: PolyFloat) -> PolyFloat {
    cheap_exp2(cheap_log2(base) * exponent)
}

#[inline(always)]
pub fn midi_offset_to_ratio(note_offset: PolyFloat) -> PolyFloat {
    exp2(note_offset * (1.0 / NOTES_PER_OCTAVE as f32))
}

#[inline(always)]
pub fn midi_note_to_frequency(note: PolyFloat) -> PolyFloat {
    midi_offset_to_ratio(note) * MIDI_0_FREQUENCY
}

#[inline(always)]
pub fn magnitude_to_db_mono(magnitude: f32) -> f32 {
    log2_mono(magnitude) * DB_GAIN_CONVERSION_MULT
}

#[inline(always)]
pub fn magnitude_to_db(magnitude: PolyFloat) -> PolyFloat {
    log2(magnitude) * DB_GAIN_CONVERSION_MULT
}

/// `exp(decibels / 6.02)`: upstream's mono version uses natural `exp`, so it
/// does not invert [`magnitude_to_db_mono`]. Kept for parity.
#[inline(always)]
pub fn db_to_magnitude_mono(decibels: f32) -> f32 {
    exp_mono(decibels * DB_MAGNITUDE_CONVERSION_MULT)
}

#[inline(always)]
pub fn db_to_magnitude(decibels: PolyFloat) -> PolyFloat {
    exp2(decibels * DB_MAGNITUDE_CONVERSION_MULT)
}

#[inline(always)]
pub fn mul_add(a: PolyFloat, b: PolyFloat, c: impl Into<PolyFloat>) -> PolyFloat {
    PolyFloat::mul_add(a, b, c.into())
}

#[inline(always)]
pub fn quick_tanh_mono(value: f32) -> f32 {
    let square = value * value;
    value / (square / (3.0 + square * 0.2) + 1.0)
}

#[inline(always)]
pub fn quick_tanh(value: PolyFloat) -> PolyFloat {
    let square = value * value;
    value / (square / mul_add(splat(3.0), square, 0.2) + 1.0)
}

#[inline(always)]
pub fn quick_tanh_derivative(value: PolyFloat) -> PolyFloat {
    let square = value * value;
    let fourth = square * square;
    let part_den = square + 2.5;
    let num = mul_add(mul_add(splat(6.25), fourth, 0.166_667), square, -1.25);
    let den = part_den * part_den;
    num / den
}

#[inline(always)]
pub fn quick_tanh_derivative_mono(value: f32) -> f32 {
    let square = value * value;
    let fourth = square * square;
    let part_den = square + 2.5;
    let num = square * -1.25 + fourth * 0.166_667 + 6.25;
    let den = part_den * part_den;
    num / den
}

/// `1/x`-based saturation.
#[inline(always)]
pub fn reciprocal_sat_mono(value: f32) -> f32 {
    let sign = 1.0f32.copysign(value);
    -1.42 * (1.0 / (value + sign) - sign)
}

#[inline(always)]
pub fn algebraic_sat_mono(value: f32) -> f32 {
    let square = value * value;
    value - value * square * 0.9 / (square + 3.0)
}

#[inline(always)]
pub fn algebraic_sat(value: PolyFloat) -> PolyFloat {
    let square = value * value;
    value * square * -0.9 / (square + 3.0) + value
}

#[inline(always)]
pub fn quadratic_inv_sat(value: PolyFloat) -> PolyFloat {
    value / (value * value * 0.25 + 1.0)
}

#[inline(always)]
pub fn bump_sat(value: PolyFloat) -> PolyFloat {
    let square = value * value;
    let pow_four = square * square;
    value / (pow_four * 0.1 + 1.0)
}

#[inline(always)]
pub fn bump_sat2(value: PolyFloat) -> PolyFloat {
    let square = value * value;
    let pow_four = square * square;
    (value + square * value * 3.0) / (pow_four * 20.0 + 1.0)
}

#[inline(always)]
pub fn algebraic_sat_derivative_mono(value: f32) -> f32 {
    let square = value * value;
    let fourth = square * square;
    let num = fourth * 0.1 + square * -2.1 + 9.0;
    let part_den = square + 3.0;
    let den = part_den * part_den;
    num / den
}

#[inline(always)]
pub fn algebraic_sat_derivative(value: PolyFloat) -> PolyFloat {
    let square = value * value;
    let fourth = square * square;
    let part_num = square * -2.1 + 9.0;
    let num = fourth * 0.1 + part_num;
    let part_den = square + 3.0;
    let den = part_den * part_den;
    num / den
}

/// Rational `tanh` approximation.
#[inline(always)]
pub fn tanh(value: PolyFloat) -> PolyFloat {
    let abs_value = PolyFloat::abs(value);
    let square = value * value;

    let part_num1 = abs_value * 0.821_226_666_969_744 + 0.893_229_853_513_558;
    let part_num2 = square * part_num1 + 2.455_507_507_029_56;
    let num = value * (abs_value * 2.455_507_507_029_56 + part_num2);

    let part_den = PolyFloat::abs(abs_value * 0.814_642_734_961_073 * value + value);
    let den = part_den * (square + 2.445_066_346_522_99) + 2.445_066_346_522_99;
    num / den
}

#[inline(always)]
pub fn tanh_mono(value: f32) -> f32 {
    let abs_value = value.abs();
    let square = value * value;

    let num = value
        * (2.455_507_507_029_56
            + 2.455_507_507_029_56 * abs_value
            + square * (0.893_229_853_513_558 + 0.821_226_666_969_744 * abs_value));
    let den = 2.445_066_346_522_99
        + (2.445_066_346_522_99 + square)
            * (value + 0.814_642_734_961_073 * value * abs_value).abs();
    num / den
}

#[inline(always)]
pub fn hard_tanh(value: PolyFloat) -> PolyFloat {
    const HARDNESS_CONSTANT: f32 = 0.66;
    const HARDNESS_CONSTANT_INV: f32 = 1.0 - HARDNESS_CONSTANT;
    const HARDNESS_CONSTANT_INV_REC: f32 = 1.0 / HARDNESS_CONSTANT_INV;

    let clamped = PolyFloat::max(
        PolyFloat::min(value, splat(HARDNESS_CONSTANT)),
        splat(-HARDNESS_CONSTANT),
    );
    clamped + tanh((value - clamped) * HARDNESS_CONSTANT_INV_REC) * (1.0 - HARDNESS_CONSTANT)
}

#[inline(always)]
pub fn tanh_derivative_fast(value: PolyFloat) -> PolyFloat {
    let square = value * value;
    splat(1.0) / mul_add(splat(2.0), square, 1.8)
}

/// Parabolic sine for phase in `[-0.5, 0.5]`.
#[inline(always)]
pub fn quick_sin_mono(phase: f32) -> f32 {
    phase * (8.0 - 16.0 * phase.abs())
}

#[inline(always)]
pub fn quick_sin(phase: PolyFloat) -> PolyFloat {
    phase * mul_add(splat(8.0), PolyFloat::abs(phase), -16.0)
}

#[inline(always)]
pub fn sin_mono(phase: f32) -> f32 {
    let approx = quick_sin_mono(phase);
    approx * (0.776 + 0.224 * approx.abs())
}

#[inline(always)]
pub fn sin(phase: PolyFloat) -> PolyFloat {
    let approx = quick_sin(phase);
    approx * mul_add(splat(0.776), PolyFloat::abs(approx), 0.224)
}

#[inline(always)]
pub fn sin_interpolate(from: PolyFloat, to: PolyFloat, t: PolyFloat) -> PolyFloat {
    let sin_value = sin(t * 0.5 - 0.25);
    let sin_t = sin_value * 0.5 + 0.5;
    from + (to - from) * sin_t
}

/// Parabolic sine for phase in `[0, 1]`.
#[inline(always)]
pub fn quick_sin1_mono(phase: f32) -> f32 {
    let phase = 0.5 - phase;
    phase * (8.0 - 16.0 * phase.abs())
}

#[inline(always)]
pub fn quick_sin1(phase: PolyFloat) -> PolyFloat {
    let adjusted_phase = splat(0.5) - phase;
    adjusted_phase * mul_add(splat(8.0), PolyFloat::abs(adjusted_phase), -16.0)
}

#[inline(always)]
pub fn sin1_mono(phase: f32) -> f32 {
    let approx = quick_sin1_mono(phase);
    approx * (0.776 + 0.224 * approx.abs())
}

#[inline(always)]
pub fn sin1(phase: PolyFloat) -> PolyFloat {
    let approx = quick_sin1(phase);
    approx * mul_add(splat(0.776), PolyFloat::abs(approx), 0.224)
}

#[inline(always)]
pub fn equal_power_fade(t: PolyFloat) -> PolyFloat {
    sin1(t * 0.25)
}

#[inline(always)]
pub fn pan_amplitude(pan: PolyFloat) -> PolyFloat {
    const SCALE: f32 = 1.414_213_562_37;
    let eighth_phase = splat(0.125);
    sin1(eighth_phase - utils::stereo_split() * pan * eighth_phase) * SCALE
}

#[inline(always)]
pub fn equal_power_fade_inverse(t: PolyFloat) -> PolyFloat {
    sin1((t + 1.0) * 0.25)
}

#[inline(always)]
pub fn power_scale_mono(value: f32, power: f32) -> f32 {
    const MIN_POWER: f32 = 0.01;

    if power.abs() < MIN_POWER {
        return value;
    }

    let numerator = exp_mono(power * value) - 1.0;
    let denominator = exp_mono(power) - 1.0;
    numerator / denominator
}

#[inline(always)]
pub fn power_scale(value: PolyFloat, power: PolyFloat) -> PolyFloat {
    const MIN_POWER_MAG: f32 = 0.005;
    let zero_mask = PolyFloat::less_than(power, splat(MIN_POWER_MAG))
        & PolyFloat::less_than(-power, splat(MIN_POWER_MAG));
    let numerator = exp(power * value) - 1.0;
    let denominator = exp(power) - 1.0;
    let result = numerator / denominator;
    utils::mask_load(result, value, zero_mask)
}
