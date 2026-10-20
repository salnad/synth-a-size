// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Scalar (`mono_float`) helpers from `vital::utils`.
//!
//! Ports the scalar part of `vital/src/synthesis/framework/utils.h` and
//! `utils.cpp`. The poly overloads live in [`poly_utils`](crate::poly_utils).
//! `RandomGenerator`'s engine and the PCM conversions are in `sas-common`.
//!
//! libm calls (`powf`, `logf`, `log10f`, ...) go through Rust's `f32` methods,
//! which call the platform libm, as the reference does.

use crate::common::{
    CENTS_PER_NOTE, CENTS_PER_OCTAVE, EPSILON, MIDI_0_FREQUENCY, NOTES_PER_OCTAVE,
};

/// Scalar dB factor (`20 * log10`). `futils` uses a different one.
pub const DB_GAIN_CONVERSION_MULT: f32 = 20.0;
pub const MAX_ORDER_LENGTH: usize = 10;
pub const LOG_OF_2: f32 = 0.693_147_180_56;
pub const INV_LOG_OF_2: f32 = 1.442_695_040_89;

pub const fn factorial(value: i32) -> i32 {
    let mut result = 1;
    let mut i = 2;
    while i <= value {
        result *= i;
        i += 1;
    }
    result
}

#[inline(always)]
pub fn int_to_float_bits(i: i32) -> f32 {
    f32::from_bits(i as u32)
}

#[inline(always)]
pub fn float_to_int_bits(f: f32) -> i32 {
    f.to_bits() as i32
}

/// `fmin`.
#[inline(always)]
pub fn min(one: f32, two: f32) -> f32 {
    one.min(two)
}

/// `fmax`.
#[inline(always)]
pub fn max(one: f32, two: f32) -> f32 {
    one.max(two)
}

/// `fmin(max, fmax(value, min))`.
#[inline(always)]
pub fn clamp(value: f32, min: f32, max: f32) -> f32 {
    max.min(value.max(min))
}

#[inline(always)]
pub fn imax(one: i32, two: i32) -> i32 {
    if one > two {
        one
    } else {
        two
    }
}

#[inline(always)]
pub fn imin(one: i32, two: i32) -> i32 {
    if one > two {
        two
    } else {
        one
    }
}

/// `t * (to - from) + from` in `double`.
#[inline(always)]
pub fn interpolate_f64(from: f64, to: f64, t: f64) -> f64 {
    t * (to - from) + from
}

/// `from + t * (to - from)`.
#[inline(always)]
pub fn interpolate(from: f32, to: f32, t: f32) -> f32 {
    from + t * (to - from)
}

/// `modff`: returns the fractional part and stores the integral part.
#[inline(always)]
pub fn modf(value: f32, integral: &mut f32) -> f32 {
    let whole = value.trunc();
    *integral = whole;
    if value.is_infinite() {
        0.0f32.copysign(value)
    } else {
        value - whole
    }
}

#[inline(always)]
pub fn iclamp(value: i32, min: i32, max: i32) -> i32 {
    if value > max {
        max
    } else if value < min {
        min
    } else {
        value
    }
}

/// Floor of log2, with values below 1 treated as 1.
#[inline(always)]
pub fn ilog2(value: i32) -> i32 {
    31 - (value.max(1) as u32).leading_zeros() as i32
}

#[inline(always)]
pub fn close_to_zero(value: f32) -> bool {
    (-EPSILON..=EPSILON).contains(&value)
}

#[inline(always)]
pub fn magnitude_to_db(magnitude: f32) -> f32 {
    DB_GAIN_CONVERSION_MULT * magnitude.log10()
}

#[inline(always)]
pub fn db_to_magnitude(decibels: f32) -> f32 {
    10.0f32.powf(decibels / DB_GAIN_CONVERSION_MULT)
}

#[inline(always)]
pub fn cents_to_ratio(cents: f32) -> f32 {
    2.0f32.powf(cents / CENTS_PER_OCTAVE as f32)
}

#[inline(always)]
pub fn note_offset_to_ratio(cents: f32) -> f32 {
    2.0f32.powf(cents / NOTES_PER_OCTAVE as f32)
}

#[inline(always)]
pub fn ratio_to_midi_transpose(ratio: f32) -> f32 {
    ratio.ln() * (INV_LOG_OF_2 * NOTES_PER_OCTAVE as f32)
}

#[inline(always)]
pub fn midi_cents_to_frequency(cents: f32) -> f32 {
    MIDI_0_FREQUENCY * cents_to_ratio(cents)
}

#[inline(always)]
pub fn midi_note_to_frequency(note: f32) -> f32 {
    midi_cents_to_frequency(note * CENTS_PER_NOTE as f32)
}

#[inline(always)]
pub fn frequency_to_midi_note(frequency: f32) -> f32 {
    NOTES_PER_OCTAVE as f32 * (frequency / MIDI_0_FREQUENCY).ln() * INV_LOG_OF_2
}

#[inline(always)]
pub fn frequency_to_midi_cents(frequency: f32) -> f32 {
    CENTS_PER_NOTE as f32 * frequency_to_midi_note(frequency)
}

#[inline(always)]
pub fn next_power_of_two(value: f32) -> i32 {
    2.0f32.powf((value.ln() * INV_LOG_OF_2).ceil()).round() as i32
}

#[inline(always)]
pub fn is_silent(buffer: &[f32]) -> bool {
    buffer.iter().all(|&value| close_to_zero(value))
}

/// Root mean square of `buffer` (sequential sum of squares).
#[inline(always)]
pub fn rms(buffer: &[f32]) -> f32 {
    let mut square_total = 0.0f32;
    for &value in buffer {
        square_total += value * value;
    }
    (square_total / buffer.len() as f32).sqrt()
}

#[inline(always)]
pub fn inverse_power_scale(t: f32) -> f32 {
    2.0 * ((-t + 1.0) / t).ln()
}

#[inline(always)]
pub fn inverse_flt_scale(t: f32) -> f32 {
    (t - 1.0) / t
}

/// Encodes a permutation of `0..order.len()` (at most 10 long) as a float.
pub fn encode_order_to_float(order: &[i32]) -> f32 {
    debug_assert!(order.len() <= MAX_ORDER_LENGTH);
    let mut code: u32 = 0;
    for i in 1..order.len() {
        let mut index: u32 = 0;
        for j in 0..i {
            index += (order[i] < order[j]) as u32;
        }
        code = code.wrapping_mul(i as u32 + 1);
        code = code.wrapping_add(index);
    }
    code as f32
}

/// Inverse of [`encode_order_to_float`]; fills `order`.
pub fn decode_float_to_order(order: &mut [i32], float_code: f32) {
    debug_assert!(order.len() <= MAX_ORDER_LENGTH);
    let size = order.len();
    let mut code = float_code as i32;
    for (i, value) in order.iter_mut().enumerate() {
        *value = i as i32;
    }

    for i in 0..size {
        let remaining = (size - i) as i32;
        let index = remaining - 1;
        let inversions = code % remaining;
        code /= remaining;

        let start = (index - inversions) as usize;
        let placement = order[start];
        for j in start..index as usize {
            order[j] = order[j + 1];
        }
        order[index as usize] = placement;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_round_trip() {
        let order = [3, 0, 2, 1, 4];
        let code = encode_order_to_float(&order);
        let mut decoded = [0; 5];
        decode_float_to_order(&mut decoded, code);
        assert_eq!(decoded, order);
    }

    #[test]
    fn ilog2_matches_clz() {
        assert_eq!(ilog2(0), 0);
        assert_eq!(ilog2(1), 0);
        assert_eq!(ilog2(2), 1);
        assert_eq!(ilog2(1023), 9);
        assert_eq!(ilog2(1024), 10);
        assert_eq!(ilog2(i32::MAX), 30);
    }

    #[test]
    fn modf_splits() {
        let mut whole = 0.0;
        assert_eq!(modf(-2.75, &mut whole), -0.75);
        assert_eq!(whole, -2.0);
    }
}
