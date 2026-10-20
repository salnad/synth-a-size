// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `poly_float` helpers from `vital::utils`.
//!
//! Ports `vital/src/synthesis/framework/poly_utils.h`. Where upstream
//! overloads one name for `poly_float` and `poly_int`, the `poly_int`
//! version has an `_int` suffix (`clamp_int`, `mask_load_int`, ...).
//! `utils::mod` is [`modulo`] because `mod` is a Rust keyword.
//!
//! Note: [`to_int`] / [`trunc_to_int`] follow upstream's per-architecture
//! conversion: round to nearest even on SSE2 (`_mm_cvtps_epi32`), truncate on
//! NEON (`vcvtq_s32_f32`). [`trunc`], [`floor`] and [`ceil`] are built on
//! that exactly as upstream builds them, so `floor`/`ceil` agree on both for
//! in-range values while `trunc` does not.

use crate::backend::{Backend, Native};
use crate::common::{EPSILON, NOTES_PER_OCTAVE};
use crate::matrix::Matrix;
use crate::utils as mono;
use crate::{PolyFloat, PolyInt, PolyMask};

pub const PHASE_ENCODING_MULTIPLIER: f32 = 0.9;
pub const NOTE_PRESSED_MASK: u32 = 0xf;

/// `kStereoSplit`: `(1, -1, 1, -1)`.
#[inline(always)]
pub fn stereo_split() -> PolyFloat {
    PolyFloat::stereo(1.0, -1.0)
}

#[inline(always)]
pub fn mul_add(a: PolyFloat, b: PolyFloat, c: impl Into<PolyFloat>) -> PolyFloat {
    PolyFloat::mul_add(a, b, c.into())
}

#[inline(always)]
pub fn mul_sub(a: PolyFloat, b: PolyFloat, c: impl Into<PolyFloat>) -> PolyFloat {
    PolyFloat::mul_sub(a, b, c.into())
}

/// Applies a scalar function to each lane.
#[inline(always)]
pub fn map(value: PolyFloat, func: impl Fn(f32) -> f32) -> PolyFloat {
    let lanes = value.to_array();
    PolyFloat::from_array([
        func(lanes[0]),
        func(lanes[1]),
        func(lanes[2]),
        func(lanes[3]),
    ])
}

#[inline(always)]
pub fn cents_to_ratio(value: PolyFloat) -> PolyFloat {
    map(value, mono::cents_to_ratio)
}

#[inline(always)]
pub fn note_offset_to_ratio(value: PolyFloat) -> PolyFloat {
    map(value, mono::note_offset_to_ratio)
}

#[inline(always)]
pub fn ratio_to_midi_transpose(value: PolyFloat) -> PolyFloat {
    map(value, mono::ratio_to_midi_transpose)
}

#[inline(always)]
pub fn midi_cents_to_frequency(value: PolyFloat) -> PolyFloat {
    map(value, mono::midi_cents_to_frequency)
}

#[inline(always)]
pub fn midi_note_to_frequency(value: PolyFloat) -> PolyFloat {
    map(value, mono::midi_note_to_frequency)
}

#[inline(always)]
pub fn frequency_to_midi_note(value: PolyFloat) -> PolyFloat {
    map(value, mono::frequency_to_midi_note)
}

#[inline(always)]
pub fn frequency_to_midi_cents(value: PolyFloat) -> PolyFloat {
    map(value, mono::frequency_to_midi_cents)
}

#[inline(always)]
pub fn magnitude_to_db(value: PolyFloat) -> PolyFloat {
    map(value, mono::magnitude_to_db)
}

#[inline(always)]
pub fn db_to_magnitude(value: PolyFloat) -> PolyFloat {
    map(value, mono::db_to_magnitude)
}

/// `tanf` per lane.
#[inline(always)]
pub fn tan(value: PolyFloat) -> PolyFloat {
    map(value, f32::tan)
}

/// `sinf` per lane.
#[inline(always)]
pub fn sin(value: PolyFloat) -> PolyFloat {
    map(value, f32::sin)
}

/// `cosf` per lane.
#[inline(always)]
pub fn cos(value: PolyFloat) -> PolyFloat {
    map(value, f32::cos)
}

/// Exact IEEE square root (`SQRTPS`).
#[inline(always)]
pub fn sqrt(value: PolyFloat) -> PolyFloat {
    value.map_raw(Native::f_sqrt)
}

/// `mulAdd(from, to - from, t)`. Covers upstream's three overloads (mono or
/// poly `t`, mono or poly endpoints), which all splat and compute the same.
#[inline(always)]
pub fn interpolate(
    from: impl Into<PolyFloat>,
    to: impl Into<PolyFloat>,
    t: impl Into<PolyFloat>,
) -> PolyFloat {
    let from = from.into();
    PolyFloat::mul_add(from, to.into() - from, t.into())
}

/// Lagrange cubic weights for `t` in `[0, 1]`.
#[inline(always)]
pub fn get_cubic_interpolation_values(mono_t: f32) -> PolyFloat {
    let lagrange_one = PolyFloat::new(0.0, 1.0, 0.0, 0.0);
    let lagrange_two = PolyFloat::new(-1.0, -1.0, 1.0, 1.0);
    let lagrange_three = PolyFloat::new(-2.0, -2.0, -2.0, -1.0);
    let lagrange_mult = PolyFloat::new(-1.0 / 6.0, 1.0 / 2.0, -1.0 / 2.0, 1.0 / 6.0);
    let t = PolyFloat::splat(mono_t);
    lagrange_mult * (t + lagrange_one) * (t + lagrange_two) * (t + lagrange_three)
}

/// "Optimal" 4-point interpolation weights.
#[inline(always)]
pub fn get_optimal_interpolation_values(mono_t: f32) -> PolyFloat {
    let optimal_one = PolyFloat::new(
        0.002_240_727_070_748_643_75,
        0.201_841_989_696_562_447_25,
        0.592_444_924_202_723_127_25,
        0.203_457_447_155_664_456_25,
    );
    let optimal_two = PolyFloat::new(
        -0.005_951_377_567_825_497_5,
        -0.456_633_315_206_820_491,
        -0.035_736_698_832_993_691,
        0.498_231_920_361_831_177_5,
    );
    let optimal_three = PolyFloat::new(
        0.093_515_484_757_265_265,
        0.294_278_871_937_834_749,
        -0.786_648_885_977_648_931,
        0.398_765_058_036_740_415,
    );
    let optimal_four = PolyFloat::new(
        -0.101_749_857_759_825_05,
        0.360_309_252_638_494_56,
        -0.360_309_252_638_494_56,
        0.101_749_857_759_825_05,
    );
    let t = PolyFloat::splat(mono_t);
    ((optimal_four * t + optimal_three) * t + optimal_two) * t + optimal_one
}

pub fn get_polynomial_interpolation_matrix(t_from: PolyFloat) -> Matrix {
    const MULT_PREV: f32 = -1.0 / 6.0;
    const MULT_FROM: f32 = 1.0 / 2.0;
    const MULT_TO: f32 = -1.0 / 2.0;
    const MULT_NEXT: f32 = 1.0 / 6.0;

    let t_prev = t_from + 1.0;
    let t_to = t_from - 1.0;
    let t_next = t_from - 2.0;

    let t_prev_from = t_prev * t_from;
    let t_to_next = t_to * t_next;

    Matrix::new(
        t_from * t_to_next * MULT_PREV,
        t_prev * t_to_next * MULT_FROM,
        t_prev_from * t_next * MULT_TO,
        t_prev_from * t_to * MULT_NEXT,
    )
}

pub fn get_catmull_interpolation_matrix(t: PolyFloat) -> Matrix {
    let half_t = t * 0.5;
    let half_t2 = t * half_t;
    let half_t3 = half_t2 * t;
    let half_three_t3 = half_t3 * 3.0;

    Matrix::new(
        half_t2 * 2.0 - half_t3 - half_t,
        mul_sub(half_three_t3, half_t2, 5.0) + 1.0,
        mul_add(half_t, half_t2, 4.0) - half_three_t3,
        half_t3 - half_t2,
    )
}

pub fn get_linear_interpolation_matrix(t: PolyFloat) -> Matrix {
    Matrix::new(0.0, PolyFloat::splat(1.0) - t, t, 0.0)
}

/// Loads four values starting at `unaligned[0]`.
#[inline(always)]
pub fn to_poly_float_from_unaligned(unaligned: &[f32]) -> PolyFloat {
    PolyFloat::load(unaligned)
}

/// Row `i` is the four values of `buffer` starting at `indices[i]`.
pub fn get_value_matrix(buffer: &[f32], indices: PolyInt) -> Matrix {
    let i = indices.to_array();
    Matrix::new(
        PolyFloat::load(&buffer[i[0] as usize..]),
        PolyFloat::load(&buffer[i[1] as usize..]),
        PolyFloat::load(&buffer[i[2] as usize..]),
        PolyFloat::load(&buffer[i[3] as usize..]),
    )
}

/// Row `i` is the four values of `buffers[i]` starting at `indices[i]`.
pub fn get_value_matrix_separate(buffers: [&[f32]; 4], indices: PolyInt) -> Matrix {
    let i = indices.to_array();
    Matrix::new(
        PolyFloat::load(&buffers[0][i[0] as usize..]),
        PolyFloat::load(&buffers[1][i[1] as usize..]),
        PolyFloat::load(&buffers[2][i[2] as usize..]),
        PolyFloat::load(&buffers[3][i[3] as usize..]),
    )
}

pub fn perlin_interpolate(from: PolyFloat, to: PolyFloat, t: PolyFloat) -> PolyFloat {
    let interpolate_from = from * t;
    let interpolate_to = to * (t - 1.0);
    let interpolate_t = t * t * (t * -2.0 + 3.0);
    interpolate(interpolate_from, interpolate_to, interpolate_t) * 2.0
}

/// `max(min(value, max), min)` with `MINPS`/`MAXPS` semantics.
#[inline(always)]
pub fn clamp(value: PolyFloat, min: impl Into<PolyFloat>, max: impl Into<PolyFloat>) -> PolyFloat {
    PolyFloat::max(PolyFloat::min(value, max.into()), min.into())
}

#[inline(always)]
pub fn clamp_int(value: PolyInt, min: PolyInt, max: PolyInt) -> PolyInt {
    PolyInt::max(PolyInt::min(value, max), min)
}

#[inline(always)]
pub fn max(left: PolyFloat, right: PolyFloat) -> PolyFloat {
    PolyFloat::max(left, right)
}

#[inline(always)]
pub fn min(left: PolyFloat, right: PolyFloat) -> PolyFloat {
    PolyFloat::min(left, right)
}

/// True when no lane differs (NaN lanes count as different).
#[inline(always)]
pub fn equal(left: PolyFloat, right: PolyFloat) -> bool {
    PolyFloat::not_equal(left, right).sum() == 0
}

/// `(zero_value & ~mask) + (one_value & mask)`.
#[inline(always)]
pub fn mask_load(
    zero_value: impl Into<PolyFloat>,
    one_value: impl Into<PolyFloat>,
    reset_mask: PolyMask,
) -> PolyFloat {
    let old_values = zero_value.into() & !reset_mask;
    let new_values = one_value.into() & reset_mask;
    old_values + new_values
}

/// `(zero_value & ~mask) | (one_value & mask)`.
#[inline(always)]
pub fn mask_load_int(zero_value: PolyInt, one_value: PolyInt, reset_mask: PolyMask) -> PolyInt {
    let old_values = zero_value & !reset_mask;
    let new_values = one_value & reset_mask;
    old_values | new_values
}

#[inline(always)]
pub fn mod_once(value: PolyFloat) -> PolyFloat {
    let less_mask = PolyFloat::less_than(value, 1.0.into());
    let lower = value - 1.0;
    mask_load(lower, value, less_mask)
}

#[inline(always)]
pub fn close_to_zero_mask(value: PolyFloat) -> PolyMask {
    PolyFloat::less_than(PolyFloat::abs(value), EPSILON.into())
}

/// `powf` per lane.
#[inline(always)]
pub fn pow(base: PolyFloat, exponent: PolyFloat) -> PolyFloat {
    let b = base.to_array();
    let e = exponent.to_array();
    PolyFloat::from_array([
        b[0].powf(e[0]),
        b[1].powf(e[1]),
        b[2].powf(e[2]),
        b[3].powf(e[3]),
    ])
}

/// Lanes where every sample of `buffer` is within `±EPSILON`.
pub fn get_silent_mask(buffer: &[PolyFloat]) -> PolyMask {
    let mut silent_mask = PolyFloat::equal(0.0.into(), 0.0.into());
    for &value in buffer {
        silent_mask &= close_to_zero_mask(value);
    }
    silent_mask
}

/// `[v1, v0, v3, v2]`
#[inline(always)]
pub fn swap_stereo(value: PolyFloat) -> PolyFloat {
    value.map_raw(Native::f_swap_stereo)
}

/// `[v1, v0, v3, v2]`
#[inline(always)]
pub fn swap_stereo_int(value: PolyInt) -> PolyInt {
    value.map_raw(Native::i_swap_stereo)
}

/// `[v2, v3, v0, v1]`
#[inline(always)]
pub fn swap_voices(value: PolyFloat) -> PolyFloat {
    value.map_raw(Native::f_swap_voices)
}

/// `[v2, v3, v0, v1]`
#[inline(always)]
pub fn swap_voices_int(value: PolyInt) -> PolyInt {
    value.map_raw(Native::i_swap_voices)
}

/// `[v0, v2, v1, v3]`
#[inline(always)]
pub fn swap_inner(value: PolyFloat) -> PolyFloat {
    value.map_raw(Native::f_swap_inner)
}

/// `[v3, v2, v1, v0]`
#[inline(always)]
pub fn reverse(value: PolyFloat) -> PolyFloat {
    value.map_raw(Native::f_reverse)
}

/// `[one0, two0, one1, two1]`
#[inline(always)]
pub fn consolidate_audio(one: PolyFloat, two: PolyFloat) -> PolyFloat {
    PolyFloat::from_raw(Native::f_unpack_lo(one.raw(), two.raw()))
}

/// `[one0, one1, two0, two1]`
#[inline(always)]
pub fn compact_first_voices(one: PolyFloat, two: PolyFloat) -> PolyFloat {
    PolyFloat::from_raw(Native::f_compact_first_voices(one.raw(), two.raw()))
}

#[inline(always)]
pub fn sum_split_audio(sum: PolyFloat) -> PolyFloat {
    let totals = sum + swap_stereo(sum);
    swap_inner(totals)
}

#[inline(always)]
pub fn max_float(values: PolyFloat) -> f32 {
    let swap = swap_voices(values);
    let max_voice = max(values, swap);
    max(max_voice, swap_stereo(max_voice)).get(0)
}

#[inline(always)]
pub fn min_float(values: PolyFloat) -> f32 {
    let swap = swap_voices(values);
    let min_voice = min(values, swap);
    min(min_voice, swap_stereo(min_voice)).get(0)
}

#[inline(always)]
pub fn encode_mid_side(value: PolyFloat) -> PolyFloat {
    (value + stereo_split() * swap_stereo(value)) * 0.5
}

#[inline(always)]
pub fn decode_mid_side(value: PolyFloat) -> PolyFloat {
    value + swap_stereo(stereo_split() * value)
}

/// Peak absolute value over `buffer[0..num]`, stepping by `skip`.
pub fn peak(buffer: &[PolyFloat], num: usize, skip: usize) -> PolyFloat {
    let mut peak = PolyFloat::zero();
    let mut i = 0;
    while i < num {
        peak = PolyFloat::max(peak, buffer[i]);
        peak = PolyFloat::max(peak, -buffer[i]);
        i += skip;
    }
    peak
}

#[inline(always)]
pub fn zero_buffer(buffer: &mut [PolyFloat]) {
    buffer.fill(PolyFloat::zero());
}

#[inline(always)]
pub fn zero_mono_buffer(buffer: &mut [f32]) {
    buffer.fill(0.0);
}

#[inline(always)]
pub fn copy_buffer<T: Copy>(dest: &mut [T], source: &[T]) {
    dest.copy_from_slice(source);
}

pub fn add_buffers(dest: &mut [PolyFloat], b1: &[PolyFloat], b2: &[PolyFloat]) {
    for ((d, &x), &y) in dest.iter_mut().zip(b1).zip(b2) {
        *d = x + y;
    }
}

/// Every lane is finite.
///
/// The fast-math reference may fold `std::isfinite` to `true`; this is the
/// source-level semantics. Callers in upstream are assertion-only.
#[inline(always)]
pub fn is_finite(value: PolyFloat) -> bool {
    value.to_array().iter().all(|v| v.is_finite())
}

#[inline(always)]
pub fn is_in_range(value: PolyFloat, min: f32, max: f32) -> bool {
    let greater_mask = PolyFloat::greater_than(value, max.into());
    let less_than_mask = PolyFloat::greater_than(min.into(), value);
    greater_mask.sum().wrapping_add(less_than_mask.sum()) == 0
}

#[inline(always)]
pub fn is_contained(value: PolyFloat) -> bool {
    const RANGE: f32 = 8000.0;
    is_in_range(value, -RANGE, RANGE)
}

pub fn is_finite_buffer(buffer: &[PolyFloat]) -> bool {
    buffer.iter().all(|&v| is_finite(v))
}

pub fn is_in_range_buffer(buffer: &[PolyFloat], min: f32, max: f32) -> bool {
    buffer.iter().all(|&v| is_in_range(v, min, max))
}

pub fn is_contained_buffer(buffer: &[PolyFloat]) -> bool {
    const RANGE: f32 = 8000.0;
    is_in_range_buffer(buffer, -RANGE, RANGE)
}

pub fn is_silent(buffer: &[PolyFloat]) -> bool {
    buffer
        .iter()
        .all(|v| v.to_array().iter().all(|&x| mono::close_to_zero(x)))
}

#[inline(always)]
pub fn gather(buffer: &[f32], indices: PolyInt) -> PolyFloat {
    let i = indices.to_array();
    PolyFloat::from_array([
        buffer[i[0] as i32 as usize],
        buffer[i[1] as i32 as usize],
        buffer[i[2] as i32 as usize],
        buffer[i[3] as i32 as usize],
    ])
}

/// Returns `(buffer[i], buffer[i + 1])` per lane.
#[inline(always)]
pub fn adjacent_gather(buffer: &[f32], indices: PolyInt) -> (PolyFloat, PolyFloat) {
    let i = indices.to_array().map(|x| x as i32 as usize);
    (
        PolyFloat::from_array([buffer[i[0]], buffer[i[1]], buffer[i[2]], buffer[i[3]]]),
        PolyFloat::from_array([
            buffer[i[0] + 1],
            buffer[i[1] + 1],
            buffer[i[2] + 1],
            buffer[i[3] + 1],
        ]),
    )
}

#[inline(always)]
pub fn gather_separate(buffers: [&[f32]; 4], indices: PolyInt) -> PolyFloat {
    let i = indices.to_array().map(|x| x as i32 as usize);
    PolyFloat::from_array([
        buffers[0][i[0]],
        buffers[1][i[1]],
        buffers[2][i[2]],
        buffers[3][i[3]],
    ])
}

/// Returns `(buffers[lane][i], buffers[lane][i + 1])` per lane.
#[inline(always)]
pub fn adjacent_gather_separate(buffers: [&[f32]; 4], indices: PolyInt) -> (PolyFloat, PolyFloat) {
    let i = indices.to_array().map(|x| x as i32 as usize);
    (
        PolyFloat::from_array([
            buffers[0][i[0]],
            buffers[1][i[1]],
            buffers[2][i[2]],
            buffers[3][i[3]],
        ]),
        PolyFloat::from_array([
            buffers[0][i[0] + 1],
            buffers[1][i[1] + 1],
            buffers[2][i[2] + 1],
            buffers[3][i[3] + 1],
        ]),
    )
}

#[inline(always)]
pub fn flt_scale(value: PolyFloat, power: PolyFloat) -> PolyFloat {
    power * value / ((power - 1.0) * value + 1.0)
}

/// Signed int → float.
#[inline(always)]
pub fn to_float(integers: PolyInt) -> PolyFloat {
    PolyFloat::from_raw(Native::i_to_f(integers.raw()))
}

/// Float → signed int: nearest-even on SSE2, truncation on NEON.
#[inline(always)]
pub fn to_int(floats: PolyFloat) -> PolyInt {
    PolyInt::from_raw(Native::f_to_i(floats.raw()))
}

/// Same as [`to_int`] (upstream's name; it only truncates on NEON).
#[inline(always)]
pub fn trunc_to_int(value: PolyFloat) -> PolyInt {
    to_int(value)
}

/// `to_float(to_int(value))` (see [`to_int`]).
#[inline(always)]
pub fn trunc(value: PolyFloat) -> PolyFloat {
    to_float(trunc_to_int(value))
}

#[inline(always)]
pub fn floor(value: PolyFloat) -> PolyFloat {
    let truncated = trunc(value);
    truncated + (PolyFloat::splat(-1.0) & PolyFloat::greater_than(truncated, value))
}

#[inline(always)]
pub fn floor_to_int(value: PolyFloat) -> PolyInt {
    to_int(floor(value))
}

#[inline(always)]
pub fn round_to_int(value: PolyFloat) -> PolyInt {
    floor_to_int(value + 0.5)
}

#[inline(always)]
pub fn ceil(value: PolyFloat) -> PolyFloat {
    let truncated = trunc(value);
    truncated + (PolyFloat::splat(1.0) & PolyFloat::less_than(truncated, value))
}

#[inline(always)]
pub fn round(value: PolyFloat) -> PolyFloat {
    floor(value + 0.5)
}

/// `value - floor(value)` (upstream `utils::mod`).
#[inline(always)]
pub fn modulo(value: PolyFloat) -> PolyFloat {
    value - floor(value)
}

#[inline(always)]
pub fn reinterpret_to_float(value: PolyInt) -> PolyFloat {
    PolyFloat::from_raw(Native::f_from_bits(value.raw()))
}

#[inline(always)]
pub fn reinterpret_to_int(value: PolyFloat) -> PolyInt {
    PolyInt::from_raw(Native::f_to_bits(value.raw()))
}

/// Logical shift right of each lane.
#[inline(always)]
pub fn shift_right<const SHIFT: i32>(integer: PolyInt) -> PolyInt {
    integer.map_raw(Native::i_shr::<SHIFT>)
}

/// Shift left of each lane.
#[inline(always)]
pub fn shift_left<const SHIFT: i32>(integer: PolyInt) -> PolyInt {
    integer.map_raw(Native::i_shl::<SHIFT>)
}

#[inline(always)]
pub fn pow2_to_float(value: PolyInt) -> PolyFloat {
    reinterpret_to_float(shift_left::<23>(value + 127))
}

#[inline(always)]
pub fn triangle_wave(t: PolyFloat) -> PolyFloat {
    let adjust = t + 0.75;
    let range = modulo(adjust);
    PolyFloat::abs(mul_add((-1.0).into(), range, 2.0))
}

/// Fractional cycle position after `seconds`, computed in `double`.
pub fn get_cycle_offset_from_seconds(seconds: f64, frequency: PolyFloat) -> PolyFloat {
    let f = frequency.to_array();
    let offset = |freq: f32| {
        let cycles = freq as f64 * seconds;
        (cycles - cycles.floor()) as f32
    };
    PolyFloat::from_array([offset(f[0]), offset(f[1]), offset(f[2]), offset(f[3])])
}

pub fn get_cycle_offset_from_samples(
    samples: i64,
    frequency: PolyFloat,
    sample_rate: i32,
    oversample_amount: i32,
) -> PolyFloat {
    let tick_time = (1.0 * oversample_amount as f64) / sample_rate as f64;
    let seconds_passed = tick_time * samples as f64;
    get_cycle_offset_from_seconds(seconds_passed, frequency)
}

/// Snaps `transpose` (semitones) to the nearest enabled note of `quantize`
/// (bit `i` = semitone `i` within the octave).
pub fn snap_transpose(transpose: PolyFloat, quantize: i32) -> PolyFloat {
    let notes = NOTES_PER_OCTAVE as f32;
    let octave_floored = floor(transpose * (1.0 / notes)) * notes;
    let transpose_from_octave = transpose - octave_floored;
    let mut min_distance = PolyFloat::splat(notes);
    let mut transpose_in_octave = transpose_from_octave;
    for i in 0..=NOTES_PER_OCTAVE {
        if (quantize >> (i % NOTES_PER_OCTAVE)) & 1 != 0 {
            let distance = PolyFloat::abs(transpose_from_octave - i as f32);
            let best_mask = PolyFloat::less_than(distance, min_distance);
            min_distance = mask_load(min_distance, distance, best_mask);
            transpose_in_octave = mask_load(transpose_in_octave, i as f32, best_mask);
        }
    }

    octave_floored + transpose_in_octave
}

/// Fills `snap_buffer[0..=12]` with the snapped semitone for each input
/// semitone.
pub fn fill_snap_buffer(transpose_quantize: i32, snap_buffer: &mut [f32]) {
    let notes = NOTES_PER_OCTAVE as usize;
    let mut min_snap = 0.0f32;
    let mut max_snap = 0.0f32;
    for i in 0..notes {
        if (transpose_quantize >> i) & 1 != 0 {
            max_snap = i as f32;
            if min_snap == 0.0 {
                min_snap = i as f32;
            }
        }
    }

    let mut offset = notes as f32 - max_snap;
    for (i, snap) in snap_buffer.iter_mut().enumerate().take(notes + 1) {
        if (transpose_quantize >> (i % notes)) & 1 != 0 {
            offset = 0.0;
        }
        *snap = offset;
        offset += 1.0;
    }
    offset = min_snap;
    for i in (0..=notes).rev() {
        if offset < snap_buffer[i] {
            snap_buffer[i] = i as f32 + offset;
        } else if snap_buffer[i] != 0.0 {
            snap_buffer[i] = i as f32 - snap_buffer[i];
        } else {
            snap_buffer[i] = i as f32;
            offset = 0.0;
        }
        offset += 1.0;
    }
}

#[inline(always)]
pub fn is_transpose_quantize_global(quantize: i32) -> bool {
    (quantize >> NOTES_PER_OCTAVE) != 0
}

#[inline(always)]
pub fn is_transpose_snapping(quantize: i32) -> bool {
    const TRANSPOSE_MASK: i32 = (1 << NOTES_PER_OCTAVE) - 1;
    (quantize & TRANSPOSE_MASK) != 0
}

#[inline(always)]
pub fn encode_phase_and_voice(phase: PolyFloat, voice: PolyFloat) -> PolyFloat {
    let voice_float = to_float((to_int(voice) & PolyInt::splat(NOTE_PRESSED_MASK)) + 1);
    voice_float + phase * PHASE_ENCODING_MULTIPLIER
}

/// Returns `(phase, voice)`.
#[inline(always)]
pub fn decode_phase_and_voice(encoded: PolyFloat) -> (PolyFloat, PolyFloat) {
    let modulo = modulo(encoded);
    let voice = encoded - modulo;
    let phase = modulo * (1.0 / PHASE_ENCODING_MULTIPLIER);
    (phase, voice)
}
