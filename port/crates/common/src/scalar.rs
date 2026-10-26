// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Scalar (single-lane) versions of the upstream math helpers that
//! non-processor code calls: `futils.h` (its polynomial `exp2`/`log2`, not
//! libm), `utils.h` and the parts of `poly_utils.h` used on one value.
//!
//! Ports `vital/src/synthesis/framework/{futils.h,utils.h,utils.cpp,poly_utils.h}`
//! (Copyright 2013-2019 Matt Tytel, GPL-3.0-or-later).
//!
//! Upstream evaluates these in lane 0 of a `poly_float`, so the operation
//! order here follows the SIMD code (`mulAdd(a, b, c)` is `a + b * c`, no
//! FMA). This is the only copy: `sas-simd` re-exports these as its mono
//! overloads, and its `tests/common_scalar.rs` checks them bit for bit
//! against every lane of its poly versions.

/// `vital::kPi`.
pub const PI: f32 = std::f32::consts::PI;

// `utils::toInt`/`truncToInt` are ISA-dependent upstream (A1 in
// `migration/decisions/reference-arm.md`), and both columns are ported:
// `_mm_cvtps_epi32` rounds to nearest even and gives `i32::MIN` for NaN and
// out-of-range values; `vcvtq_s32_f32` truncates, saturates and gives 0 for
// NaN. [`to_int`] picks the column from [`crate::AARCH64_SEMANTICS`]. For
// non-NaN |value| < 2^31 the public callers (`poly_floor`, `round_to_int`,
// and through them `exp2`) give the same result on both columns:
// `poly_floor` compares the conversion with the input, which yields floor()
// whether the conversion rounded or truncated.

/// `_mm_cvtps_epi32` (SSE2 column of `utils::toInt`): round to nearest even;
/// NaN and out-of-range values give `i32::MIN` (the "integer indefinite"
/// value).
#[inline]
pub fn to_int_sse2(value: f32) -> i32 {
    if value.is_nan() || value >= 2_147_483_648.0 || value < -2_147_483_648.0 {
        i32::MIN
    } else {
        value.round_ties_even() as i32
    }
}

/// `vcvtq_s32_f32` (NEON column of `utils::toInt`): truncate toward zero,
/// saturate out-of-range values, NaN gives 0. This is Rust's `as i32`.
#[inline]
pub fn to_int_neon(value: f32) -> i32 {
    value as i32
}

/// `utils::toInt` for one lane, on this build's column (A1).
#[inline]
pub fn to_int(value: f32) -> i32 {
    if crate::AARCH64_SEMANTICS {
        to_int_neon(value)
    } else {
        to_int_sse2(value)
    }
}

/// `utils::trunc` for one lane. Upstream's `truncToInt` is `toInt`'s
/// conversion, so on the SSE2 column this rounds to nearest even despite the
/// name.
#[inline]
pub fn poly_trunc(value: f32) -> f32 {
    to_int(value) as f32
}

/// `utils::floor` for one lane. Both columns agree for non-NaN
/// |value| < 2^31.
#[inline]
pub fn poly_floor(value: f32) -> f32 {
    let truncated = poly_trunc(value);
    if truncated > value {
        truncated + -1.0
    } else {
        truncated + 0.0
    }
}

/// `utils::roundToInt` for one lane: `floor(value + 0.5)`. Both columns
/// agree for non-NaN |value| < 2^31.
#[inline]
pub fn round_to_int(value: f32) -> i32 {
    to_int(poly_floor(value + 0.5))
}

/// `utils::pow2ToFloat`.
#[inline]
pub fn pow2_to_float(value: i32) -> f32 {
    f32::from_bits((value.wrapping_add(127) as u32) << 23)
}

/// `futils::exp2` (polynomial approximation).
#[inline]
pub fn exp2(exponent: f32) -> f32 {
    const C0: f32 = 1.0;
    const C1: f32 = (16970.0f64 / 24483.0) as f32;
    const C2: f32 = (1960.0f64 / 8161.0) as f32;
    const C3: f32 = (1360.0f64 / 24483.0) as f32;
    const C4: f32 = (80.0f64 / 8161.0) as f32;
    const C5: f32 = (32.0f64 / 24483.0) as f32;

    let integer = round_to_int(exponent);
    let t = exponent - integer as f32;
    let int_pow = pow2_to_float(integer);

    let cubic = t * (t * (t * C5 + C4) + C3) + C2;
    let interpolate = t * (t * cubic + C1) + C0;
    int_pow * interpolate
}

/// `futils::log2` (polynomial approximation).
#[inline]
pub fn log2(value: f32) -> f32 {
    const C0: f32 = (-1819.0f64 / 651.0) as f32;
    const C1: f32 = 5.0;
    const C2: f32 = (-10.0f64 / 3.0) as f32;
    const C3: f32 = (10.0f64 / 7.0) as f32;
    const C4: f32 = (-1.0f64 / 3.0) as f32;
    const C5: f32 = (1.0f64 / 31.0) as f32;

    let bits = value.to_bits();
    let floored_log2 = (bits >> 23).wrapping_sub(0x7f) as i32;
    let t = f32::from_bits((bits & 0x7f_ffff) | (0x7f << 23));

    let cubic = t * (t * (t * C5 + C4) + C3) + C2;
    let interpolate = t * (t * cubic + C1) + C0;
    floored_log2 as f32 + interpolate
}

// Upstream's literals, not the std constants (they may round differently).
#[allow(clippy::excessive_precision, clippy::approx_constant)]
const EXP_CONVERSION_MULT: f32 = 1.44269504089;
#[allow(clippy::excessive_precision, clippy::approx_constant)]
const LOG_CONVERSION_MULT: f32 = 0.69314718056;

/// `futils::exp`.
#[inline]
pub fn exp(exponent: f32) -> f32 {
    exp2(exponent * EXP_CONVERSION_MULT)
}

/// `futils::log`.
#[inline]
pub fn log(value: f32) -> f32 {
    log2(value) * LOG_CONVERSION_MULT
}

/// `futils::pow`.
#[inline]
pub fn pow(base: f32, exponent: f32) -> f32 {
    exp2(log2(base) * exponent)
}

/// `futils::powerScale` (the `mono_float` overload).
#[inline]
pub fn power_scale(value: f32, power: f32) -> f32 {
    const MIN_POWER: f32 = 0.01;
    if power.abs() < MIN_POWER {
        return value;
    }
    let numerator = exp(power * value) - 1.0;
    let denominator = exp(power) - 1.0;
    numerator / denominator
}

/// `(sinf(angle), cosf(angle))` where upstream computes both for one angle
/// (A11 in `migration/decisions/reference-arm.md`). On Apple targets, Clang
/// merges such a pair into `__sincosf_stret` even without fast-math. That
/// can differ from separate `sinf`/`cosf` by 1 ulp, so this calls it
/// directly. Elsewhere it calls `sinf` and `cosf`; glibc's `sincosf`, which
/// GCC may merge a pair into, returns the same bits.
///
/// Calling it explicitly also keeps the port independent of whether its own
/// compiler happens to merge two separate calls, which depends on the
/// optimisation level.
#[inline]
pub fn sin_cos(angle: f32) -> (f32, f32) {
    #[cfg(target_vendor = "apple")]
    {
        let result = unsafe { apple::__sincosf_stret(angle) };
        (result.sin, result.cos)
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        (angle.sin(), angle.cos())
    }
}

/// `(sinf(angle), cosf(angle))` as two separate library calls that no
/// compiler can merge into a `sincos` (each lives in its own non-inlined
/// function). This is for code whose reference evaluates the pair
/// separately; on Apple targets the merged form can differ by 1 ulp.
#[inline]
pub fn sin_cos_unmerged(angle: f32) -> (f32, f32) {
    (sin_unmerged(angle), cos_unmerged(angle))
}

#[inline(never)]
fn sin_unmerged(angle: f32) -> f32 {
    angle.sin()
}

#[inline(never)]
fn cos_unmerged(angle: f32) -> f32 {
    angle.cos()
}

#[cfg(target_vendor = "apple")]
mod apple {
    #[repr(C)]
    pub struct SinCos {
        pub sin: f32,
        pub cos: f32,
    }

    extern "C" {
        pub fn __sincosf_stret(angle: f32) -> SinCos;
    }
}

/// `futils::tanh` (the `mono_float` overload, a rational approximation).
#[inline]
#[allow(clippy::excessive_precision)]
pub fn tanh(value: f32) -> f32 {
    let abs_value = value.abs();
    let square = value * value;
    let num = value
        * (2.45550750702956_f32
            + 2.45550750702956_f32 * abs_value
            + square * (0.893229853513558_f32 + 0.821226666969744_f32 * abs_value));
    let den = 2.44506634652299_f32
        + (2.44506634652299_f32 + square)
            * (value + 0.814642734961073_f32 * value * abs_value).abs();
    num / den
}

/// `utils::min` (`fmin`).
#[inline]
pub fn min(one: f32, two: f32) -> f32 {
    one.min(two)
}

/// `utils::max` (`fmax`).
#[inline]
pub fn max(one: f32, two: f32) -> f32 {
    one.max(two)
}

/// `utils::clamp` (`fmin(max, fmax(value, min))`).
#[inline]
pub fn clamp(value: f32, min: f32, max: f32) -> f32 {
    max.min(value.max(min))
}

/// `utils::iclamp`.
#[inline]
pub fn iclamp(value: i32, min: i32, max: i32) -> i32 {
    if value > max {
        max
    } else if value < min {
        min
    } else {
        value
    }
}

/// `utils::ilog2`.
#[inline]
pub fn ilog2(value: i32) -> i32 {
    31 - value.max(1).leading_zeros() as i32
}

/// `utils::interpolate` for `mono_float`: `from + t * (to - from)`.
#[inline]
pub fn interpolate(from: f32, to: f32, t: f32) -> f32 {
    from + t * (to - from)
}

/// `utils::interpolate` for `double`: `t * (to - from) + from`.
#[inline]
pub fn interpolate_f64(from: f64, to: f64, t: f64) -> f64 {
    t * (to - from) + from
}

/// The four weights of `utils::getCatmullInterpolationMatrix(t)`.
#[inline]
pub fn catmull_weights(t: f32) -> [f32; 4] {
    let half_t = t * 0.5;
    let half_t2 = t * half_t;
    let half_t3 = half_t2 * t;
    let half_three_t3 = half_t3 * 3.0;
    [
        half_t2 * 2.0 - half_t3 - half_t,
        (half_three_t3 - half_t2 * 5.0) + 1.0,
        (half_t + half_t2 * 4.0) - half_three_t3,
        half_t3 - half_t2,
    ]
}

/// One lane of
/// `getCatmullInterpolationMatrix(t).multiplyAndSumRows(transposed getValueMatrix(buffer, index))`:
/// Catmull-Rom interpolation of `buffer[index..index + 4]`.
#[inline]
pub fn catmull_interpolate(buffer: &[f32], index: usize, t: f32) -> f32 {
    let w = catmull_weights(t);
    let v = &buffer[index..index + 4];
    let row01 = w[0] * v[0] + w[1] * v[1];
    let row012 = row01 + w[2] * v[2];
    row012 + w[3] * v[3]
}

const PCM_SCALE: f32 = 32767.0;

/// `utils::floatToPcmData`.
pub fn float_to_pcm(float_data: &[f32]) -> Vec<i16> {
    float_data
        .iter()
        .map(|&v| clamp(v * PCM_SCALE, -PCM_SCALE, PCM_SCALE) as i32 as i16)
        .collect()
}

/// `utils::pcmToFloatData`.
pub fn pcm_to_float(pcm_data: &[i16]) -> Vec<f32> {
    const MULT: f32 = 1.0 / PCM_SCALE;
    pcm_data.iter().map(|&v| f32::from(v) * MULT).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_POW_31: f32 = 2_147_483_648.0;

    #[test]
    fn to_int_sse2_column() {
        assert_eq!(to_int_sse2(2.5), 2);
        assert_eq!(to_int_sse2(3.5), 4);
        assert_eq!(to_int_sse2(-2.5), -2);
        assert_eq!(to_int_sse2(-2.7), -3);
        assert_eq!(to_int_sse2(2_147_483_520.0), 2_147_483_520);
        assert_eq!(to_int_sse2(-TWO_POW_31), i32::MIN);
        for indefinite in [
            f32::NAN,
            -f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            TWO_POW_31,
            3.0e9,
            -3.0e9,
        ] {
            assert_eq!(to_int_sse2(indefinite), i32::MIN, "{indefinite}");
        }
    }

    #[test]
    fn to_int_neon_column() {
        assert_eq!(to_int_neon(2.5), 2);
        assert_eq!(to_int_neon(3.5), 3);
        assert_eq!(to_int_neon(-2.5), -2);
        assert_eq!(to_int_neon(-2.7), -2);
        assert_eq!(to_int_neon(2_147_483_520.0), 2_147_483_520);
        assert_eq!(to_int_neon(-TWO_POW_31), i32::MIN);
        assert_eq!(to_int_neon(f32::NAN), 0);
        assert_eq!(to_int_neon(-f32::NAN), 0);
        for high in [f32::INFINITY, TWO_POW_31, 3.0e9] {
            assert_eq!(to_int_neon(high), i32::MAX, "{high}");
        }
        for low in [f32::NEG_INFINITY, -3.0e9] {
            assert_eq!(to_int_neon(low), i32::MIN, "{low}");
        }
    }

    /// NaN and out-of-range inputs through the public helpers, on this
    /// build's column.
    #[test]
    fn selected_column_out_of_range() {
        let neon = crate::AARCH64_SEMANTICS;
        for value in [
            2.5,
            -2.5,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            TWO_POW_31,
            -3.0e9,
        ] {
            let expected = if neon {
                to_int_neon(value)
            } else {
                to_int_sse2(value)
            };
            assert_eq!(to_int(value), expected, "{value}");
        }
        if neon {
            assert_eq!(poly_floor(f32::NAN).to_bits(), 0.0f32.to_bits());
            assert_eq!(round_to_int(f32::NAN), 0);
            assert_eq!(poly_floor(3.0e9), TWO_POW_31);
            assert_eq!(poly_floor(-3.0e9), -TWO_POW_31);
            assert_eq!(round_to_int(3.0e9), i32::MAX);
            assert_eq!(exp2(TWO_POW_31), 0.5);
        } else {
            assert_eq!(poly_floor(f32::NAN), -TWO_POW_31);
            assert_eq!(round_to_int(f32::NAN), i32::MIN);
            assert_eq!(poly_floor(3.0e9), -TWO_POW_31);
            assert_eq!(poly_floor(-3.0e9), -TWO_POW_31);
            assert_eq!(round_to_int(3.0e9), i32::MIN);
            assert_eq!(exp2(TWO_POW_31), f32::INFINITY);
        }
        assert!(exp2(f32::NAN).is_nan());
    }

    #[test]
    fn rounding_examples() {
        assert_eq!(round_to_int(2.5), 3);
        assert_eq!(round_to_int(-2.5), -2);
        assert_eq!(round_to_int(-2.6), -3);
        assert_eq!(poly_floor(-0.5), -1.0);
        assert_eq!(poly_floor(2.0), 2.0);
    }

    /// The two columns give the same floor and `roundToInt` for in-range
    /// values.
    #[test]
    fn columns_agree_in_range() {
        let floor_with = |convert: fn(f32) -> i32, value: f32| {
            let truncated = convert(value) as f32;
            if truncated > value {
                truncated + -1.0
            } else {
                truncated + 0.0
            }
        };
        let round_with =
            |convert: fn(f32) -> i32, value: f32| convert(floor_with(convert, value + 0.5));
        let mut values: Vec<f32> = (-40_000..=40_000).map(|i| i as f32 * 0.025).collect();
        values.extend([
            -0.5,
            0.5,
            -1.5,
            1.5,
            -0.0,
            8_388_607.5,
            -8_388_607.5,
            2_147_483_520.0,
            -TWO_POW_31,
        ]);
        let mut state = 0x1234_5678u32;
        for _ in 0..100_000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let value = f32::from_bits(state);
            if value.is_finite() && value.abs() < 2_147_483_520.0 {
                values.push(value);
            }
        }
        for value in values {
            let floor = floor_with(to_int_sse2, value);
            assert_eq!(
                floor.to_bits(),
                floor_with(to_int_neon, value).to_bits(),
                "{value}"
            );
            assert_eq!(floor.to_bits(), poly_floor(value).to_bits(), "{value}");
            let round = round_with(to_int_sse2, value);
            assert_eq!(round, round_with(to_int_neon, value), "{value}");
            assert_eq!(round, round_to_int(value), "{value}");
        }
    }

    #[test]
    fn approximations_are_close() {
        for i in -200..200 {
            let x = i as f32 * 0.05;
            assert!((exp2(x) / x.exp2() - 1.0).abs() < 1e-4, "exp2({x})");
            assert!((tanh(x) - x.tanh()).abs() < 2e-3, "tanh({x})");
        }
        for i in 1..400 {
            let x = i as f32 * 0.37;
            assert!((log2(x) - x.log2()).abs() < 1e-3, "log2({x})");
        }
        assert_eq!(exp2(3.0), 8.0);
        assert_eq!(exp2(0.0), 1.0);
    }

    #[test]
    fn power_scale_endpoints() {
        assert_eq!(power_scale(0.3, 0.0), 0.3);
        assert!((power_scale(1.0, 4.0) - 1.0).abs() < 1e-6);
        assert!(power_scale(0.5, 4.0) < 0.5);
    }

    #[test]
    fn helpers() {
        assert_eq!(ilog2(1), 0);
        assert_eq!(ilog2(0), 0);
        assert_eq!(ilog2(1024), 10);
        assert_eq!(ilog2(1023), 9);
        assert_eq!(iclamp(5, 0, 3), 3);
        assert_eq!(clamp(f32::NAN, -1.0, 1.0), -1.0);
        assert_eq!(
            float_to_pcm(&[1.0, -1.0, 0.5, 2.0, 0.0]),
            vec![32767, -32767, 16383, 32767, 0]
        );
        assert_eq!(pcm_to_float(&[32767])[0], 1.0);
        let buffer = [0.0, 1.0, 2.0, 3.0];
        assert_eq!(catmull_interpolate(&buffer, 0, 0.0), 1.0);
        assert_eq!(catmull_interpolate(&buffer, 0, 1.0), 2.0);
        assert_eq!(catmull_interpolate(&buffer, 0, 0.5), 1.5);
    }
}
