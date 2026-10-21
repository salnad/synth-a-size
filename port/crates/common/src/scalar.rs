//! Scalar (single-lane) versions of the upstream math helpers that
//! non-processor code calls: `futils.h` (its polynomial `exp2`/`log2`, not
//! libm), `utils.h` and the parts of `poly_utils.h` used on one value.
//!
//! Ports `vital/src/synthesis/framework/{futils.h,utils.h,utils.cpp,poly_utils.h}`
//! (Copyright 2013-2019 Matt Tytel, GPL-3.0-or-later).
//!
//! Upstream evaluates these in lane 0 of a `poly_float`, so the operation
//! order here follows the SSE2 code (`mulAdd(a, b, c)` is `a + b * c`, no
//! FMA). The SIMD crate's lane-wise versions must agree with these bit for
//! bit.

/// `vital::kPi`.
pub const PI: f32 = std::f32::consts::PI;

// `utils::toInt`/`truncToInt` are ISA-dependent upstream (A1 in
// `migration/decisions/reference-arm.md`): `_mm_cvtps_epi32` rounds to
// nearest even and gives `i32::MIN` for NaN, `vcvtq_s32_f32` truncates and
// gives 0. These two helpers implement only the SSE2 column, so they stay
// crate-private. Their callers (`poly_floor`, `round_to_int`, and through
// them `exp2`) give the same result on both ISAs for non-NaN |value| < 2^31:
// `poly_floor` corrects the conversion by comparing it with the input, which
// yields floor() whether the conversion rounded or truncated, and
// `round_to_int` converts that floor, which is already an integer.

/// `_mm_cvtps_epi32`: round to nearest even; NaN and out-of-range values give
/// `i32::MIN` (the "integer indefinite" value).
#[inline]
pub(crate) fn round_to_i32_sse(value: f32) -> i32 {
    if value.is_nan() || value >= 2_147_483_648.0 || value < -2_147_483_648.0 {
        i32::MIN
    } else {
        value.round_ties_even() as i32
    }
}

/// `utils::trunc` for one lane, SSE2 column: upstream's `truncToInt` is
/// `_mm_cvtps_epi32` there, so this rounds to nearest even despite the name.
#[inline]
pub(crate) fn poly_trunc(value: f32) -> f32 {
    round_to_i32_sse(value) as f32
}

/// `utils::floor` for one lane. ISA-independent for non-NaN |value| < 2^31.
#[inline]
pub fn poly_floor(value: f32) -> f32 {
    let truncated = poly_trunc(value);
    if truncated > value {
        truncated + -1.0
    } else {
        truncated + 0.0
    }
}

/// `utils::roundToInt` for one lane: `floor(value + 0.5)`. ISA-independent
/// for non-NaN |value| < 2^31.
#[inline]
pub fn round_to_int(value: f32) -> i32 {
    round_to_i32_sse(poly_floor(value + 0.5))
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

    #[test]
    fn rounding_matches_sse() {
        assert_eq!(round_to_i32_sse(2.5), 2);
        assert_eq!(round_to_i32_sse(3.5), 4);
        assert_eq!(round_to_i32_sse(-2.5), -2);
        assert_eq!(round_to_i32_sse(f32::NAN), i32::MIN);
        assert_eq!(round_to_int(2.5), 3);
        assert_eq!(round_to_int(-2.5), -2);
        assert_eq!(round_to_int(-2.6), -3);
        assert_eq!(poly_floor(-0.5), -1.0);
        assert_eq!(poly_floor(2.0), 2.0);
    }

    /// The public callers of the SSE2-only conversion give the NEON column's
    /// results (truncating `vcvtq_s32_f32`) for in-range values.
    #[test]
    fn public_rounding_is_isa_independent_in_range() {
        let neon_floor = |value: f32| {
            let truncated = value as i32 as f32;
            if truncated > value {
                truncated + -1.0
            } else {
                truncated + 0.0
            }
        };
        let neon_round_to_int = |value: f32| neon_floor(value + 0.5) as i32;
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
            -2_147_483_648.0,
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
            assert_eq!(
                poly_floor(value).to_bits(),
                neon_floor(value).to_bits(),
                "{value}"
            );
            assert_eq!(round_to_int(value), neon_round_to_int(value), "{value}");
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
