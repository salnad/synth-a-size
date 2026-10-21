//! `std::complex<float>` with upstream source semantics: products use the
//! textbook formula (what libstdc++ computes for finite values), division
//! follows the target's C++ library (libgcc's `__divsc3` or libc++, A10),
//! `abs` is `cabsf` (`hypotf`) and `arg` is `atan2f`.
//!
//! The `-ffast-math` reference expands `std::abs` to `sqrtf(re² + im²)`
//! and divides in float without scaling; both differ by rounding only, which
//! the parity tolerances absorb, so the source semantics are kept.

use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

/// One `std::complex<float>` (real part first, as in memory).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };

    #[inline]
    pub const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    /// `std::complex<float>(value)`.
    #[inline]
    pub const fn real(value: f32) -> Self {
        Self { re: value, im: 0.0 }
    }

    /// `std::abs(z)`: `cabsf`, i.e. `hypotf(re, im)`.
    #[inline]
    pub fn abs(self) -> f32 {
        self.re.hypot(self.im)
    }

    /// `std::arg(z)`: `atan2f(im, re)`.
    #[inline]
    pub fn arg(self) -> f32 {
        self.im.atan2(self.re)
    }

    /// `std::conj(z)`.
    #[inline]
    pub fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    /// `std::polar(rho, theta)`: `(rho * cos(theta), rho * sin(theta))`.
    #[inline]
    pub fn polar(rho: f32, theta: f32) -> Self {
        Self {
            re: rho * theta.cos(),
            im: rho * theta.sin(),
        }
    }

    /// `z * scalar`.
    #[inline]
    pub fn scale(self, value: f32) -> Self {
        Self {
            re: self.re * value,
            im: self.im * value,
        }
    }

    /// `value / z` for a real `value`: libstdc++ divides `complex(value, 0)`
    /// by `z`.
    #[inline]
    pub fn real_div(value: f32, z: Complex) -> Self {
        Complex { re: value, im: 0.0 } / z
    }
}

/// `self / z` in this build's column (A10 in
/// `migration/decisions/reference-arm.md`): [`div_libgcc`] by default, and
/// [`div_libcxx`] on aarch64 or with the `aarch64-semantics` feature (the
/// macOS arm64 reference uses libc++).
impl Div for Complex {
    type Output = Complex;

    #[inline]
    fn div(self, z: Complex) -> Complex {
        if sas_common::AARCH64_SEMANTICS {
            div_libcxx(self, z)
        } else {
            div_libgcc(self, z)
        }
    }
}

fn unit(v: f32, on: bool) -> f32 {
    (if on { 1.0f32 } else { 0.0 }).copysign(v)
}

/// `std::complex<float>` division as GCC compiles it without fast-math:
/// libgcc's `__divsc3`, which (GCC 12 and later, with double hardware)
/// evaluates the textbook formula in double and rounds each part to float,
/// then recovers infinities from NaN results.
pub fn div_libgcc(z: Complex, w: Complex) -> Complex {
    let (mut a, mut b, mut c, mut d) = (z.re, z.im, w.re, w.im);
    let (ad, bd, cd, dd) = (f64::from(a), f64::from(b), f64::from(c), f64::from(d));
    let denominator = cd * cd + dd * dd;
    let mut x = ((ad * cd + bd * dd) / denominator) as f32;
    let mut y = ((bd * cd - ad * dd) / denominator) as f32;

    if x.is_nan() && y.is_nan() {
        if denominator == 0.0 && (!a.is_nan() || !b.is_nan()) {
            x = f32::INFINITY.copysign(c) * a;
            y = f32::INFINITY.copysign(c) * b;
        } else if (a.is_infinite() || b.is_infinite()) && c.is_finite() && d.is_finite() {
            a = unit(a, a.is_infinite());
            b = unit(b, b.is_infinite());
            x = f32::INFINITY * (a * c + b * d);
            y = f32::INFINITY * (b * c - a * d);
        } else if (c.is_infinite() || d.is_infinite()) && a.is_finite() && b.is_finite() {
            c = unit(c, c.is_infinite());
            d = unit(d, d.is_infinite());
            x = 0.0 * (a * c + b * d);
            y = 0.0 * (b * c - a * d);
        }
    }
    Complex { re: x, im: y }
}

/// `logbf`: the unbiased exponent as a float, -inf for 0, +inf for
/// infinities.
fn logb(value: f32) -> f32 {
    if value.is_nan() {
        value
    } else if value == 0.0 {
        f32::NEG_INFINITY
    } else if value.is_infinite() {
        f32::INFINITY
    } else {
        libm::ilogbf(value) as f32
    }
}

/// libc++'s `std::complex<float>` division (`<complex>` in LLVM 17; later
/// versions route through the compiler's `__divsc3`, which compiler-rt
/// implements with the same algorithm): scale the divisor by
/// `2^-logb(max(|c|, |d|))`, apply the textbook formula in float, scale the
/// result back, then recover infinities from NaN results.
pub fn div_libcxx(z: Complex, w: Complex) -> Complex {
    let (mut a, mut b, mut c, mut d) = (z.re, z.im, w.re, w.im);
    let logbw = logb(c.abs().max(d.abs()));
    let mut ilogbw = 0;
    if logbw.is_finite() {
        ilogbw = logbw as i32;
        c = libm::scalbnf(c, -ilogbw);
        d = libm::scalbnf(d, -ilogbw);
    }
    let denominator = c * c + d * d;
    let mut x = libm::scalbnf((a * c + b * d) / denominator, -ilogbw);
    let mut y = libm::scalbnf((b * c - a * d) / denominator, -ilogbw);

    if x.is_nan() && y.is_nan() {
        if denominator == 0.0 && (!a.is_nan() || !b.is_nan()) {
            x = f32::INFINITY.copysign(c) * a;
            y = f32::INFINITY.copysign(c) * b;
        } else if (a.is_infinite() || b.is_infinite()) && c.is_finite() && d.is_finite() {
            a = unit(a, a.is_infinite());
            b = unit(b, b.is_infinite());
            x = f32::INFINITY * (a * c + b * d);
            y = f32::INFINITY * (b * c - a * d);
        } else if logbw.is_infinite() && logbw > 0.0 && a.is_finite() && b.is_finite() {
            c = unit(c, c.is_infinite());
            d = unit(d, d.is_infinite());
            x = 0.0 * (a * c + b * d);
            y = 0.0 * (b * c - a * d);
        }
    }
    Complex { re: x, im: y }
}

impl Add for Complex {
    type Output = Complex;
    #[inline]
    fn add(self, other: Complex) -> Complex {
        Complex {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }
}

impl AddAssign for Complex {
    #[inline]
    fn add_assign(&mut self, other: Complex) {
        *self = *self + other;
    }
}

impl Sub for Complex {
    type Output = Complex;
    #[inline]
    fn sub(self, other: Complex) -> Complex {
        Complex {
            re: self.re - other.re,
            im: self.im - other.im,
        }
    }
}

impl SubAssign for Complex {
    #[inline]
    fn sub_assign(&mut self, other: Complex) {
        *self = *self - other;
    }
}

impl Mul for Complex {
    type Output = Complex;
    #[inline]
    fn mul(self, other: Complex) -> Complex {
        Complex {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }
}

impl MulAssign for Complex {
    #[inline]
    fn mul_assign(&mut self, other: Complex) {
        *self = *self * other;
    }
}

impl Mul<f32> for Complex {
    type Output = Complex;
    #[inline]
    fn mul(self, value: f32) -> Complex {
        self.scale(value)
    }
}

impl MulAssign<f32> for Complex {
    #[inline]
    fn mul_assign(&mut self, value: f32) {
        *self = self.scale(value);
    }
}

impl Neg for Complex {
    type Output = Complex;
    #[inline]
    fn neg(self) -> Complex {
        Complex {
            re: -self.re,
            im: -self.im,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Case = ((f32, f32), (f32, f32), [u32; 2], [u32; 2]);

    /// `(a + bi) / (c + di)` with the expected bit patterns of each column:
    /// libgcc's `__divsc3` from GCC 13 without fast-math, and libc++'s
    /// algorithm (LLVM 17 `<complex>`) transcribed into C and compiled
    /// without fast-math or FMA contraction.
    const CASES: [Case; 7] = [
        (
            (1.0, 0.0),
            (0.3, 0.7),
            [0x3f04_69ef, 0xbf9a_7b96],
            [0x3f04_69ef, 0xbf9a_7b96],
        ),
        (
            (1.0, 0.0),
            (-0.123456, 0.987654),
            [0xbdff_35fe, 0xbf7f_3663],
            [0xbdff_35fe, 0xbf7f_3663],
        ),
        (
            (0.5, -0.25),
            (1e-3, 3.0),
            [0xbdaa_8d89, 0xbe2a_b1f2],
            [0xbdaa_8d89, 0xbe2a_b1f1],
        ),
        (
            (1.0, 0.0),
            (0.70710677, 0.70710677),
            [0x3f35_04f3, 0xbf35_04f3],
            [0x3f35_04f4, 0xbf35_04f4],
        ),
        (
            (1.0, 0.0),
            (1e-30, 1e-30),
            [0x70c9_f2ca, 0xf0c9_f2ca],
            [0x70c9_f2ca, 0xf0c9_f2ca],
        ),
        (
            (3.0, -2.0),
            (3e37, -1e38),
            [0x0110_daaa, 0x00ef_c233],
            [0x0110_daaa, 0x00ef_c235],
        ),
        ((1.0, 0.0), (f32::INFINITY, 1.0), [0, 0], [0, 0]),
    ];

    fn bits(q: Complex) -> [u32; 2] {
        [q.re.to_bits(), q.im.to_bits()]
    }

    #[test]
    fn division_columns_match_their_libraries() {
        for ((a, b), (c, d), libgcc, libcxx) in CASES {
            let (z, w) = (Complex { re: a, im: b }, Complex { re: c, im: d });
            assert_eq!(
                bits(div_libgcc(z, w)),
                libgcc,
                "libgcc ({a}, {b}) / ({c}, {d})"
            );
            assert_eq!(
                bits(div_libcxx(z, w)),
                libcxx,
                "libc++ ({a}, {b}) / ({c}, {d})"
            );
            let expected = if sas_common::AARCH64_SEMANTICS {
                libcxx
            } else {
                libgcc
            };
            assert_eq!(bits(z / w), expected);
        }
    }

    #[test]
    fn division_by_zero_recovers_infinity() {
        for divide in [div_libgcc, div_libcxx] {
            let q = divide(Complex { re: 1.0, im: 0.0 }, Complex::ZERO);
            assert_eq!(q.re, f32::INFINITY);
            assert!(q.im.is_nan());
        }
    }
}
