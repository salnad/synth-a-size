//! `std::complex<float>` as upstream uses it: products use the textbook
//! formula (what libstdc++ computes for finite values), `abs` is `cabsf`
//! (`hypotf`) and `arg` is `atan2f`. Division is the official build's
//! plain float formula on every target (A10, owner decision).
//!
//! The `-ffast-math` reference expands `std::abs` to `sqrtf(re² + im²)`;
//! that differs by rounding only, which the parity tolerances absorb, so
//! `abs` keeps the source semantics.

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

    /// `std::polar(rho, theta)`: `(rho * cos(theta), rho * sin(theta))`, with
    /// the pair evaluated as upstream's compiler does
    /// ([`sas_common::scalar::sin_cos`], A11).
    #[inline]
    pub fn polar(rho: f32, theta: f32) -> Self {
        let (sin, cos) = sas_common::scalar::sin_cos(theta);
        Self {
            re: rho * cos,
            im: rho * sin,
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

/// `self / z` as both official reference builds compute it (A10): the
/// `-ffast-math` builds use limited-range complex arithmetic, so GCC (x86)
/// and Apple clang (arm64) both inline the textbook formula in float,
/// `((ac + bd) / (c² + d²), (bc − ad) / (c² + d²))`, with no scaling, no
/// fused multiply-adds and no `__divsc3` call. The owner chose this over the
/// no-fast-math builds' `__divsc3`, which differs between the targets.
impl Div for Complex {
    type Output = Complex;

    #[inline]
    fn div(self, z: Complex) -> Complex {
        let denominator = z.re * z.re + z.im * z.im;
        Complex {
            re: (self.re * z.re + self.im * z.im) / denominator,
            im: (self.im * z.re - self.re * z.im) / denominator,
        }
    }
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

    /// Dividend and divisor as f32 bit patterns, then the quotient that
    /// GCC 13 computes with the reference's flags (`-ffast-math
    /// -ffp-contract=off`, limited-range division), which equals the plain
    /// float formula on 2M random inputs. The last four are inputs where the
    /// no-fast-math `__divsc3` forms give different bits.
    const CASES: [([u32; 4], [u32; 2]); 8] = [
        (
            [0x3f80_0000, 0, 0x3e99_999a, 0x3f33_3333],
            [0x3f04_69ef, 0xbf9a_7b96],
        ),
        (
            [0x3f80_0000, 0, 0xbdfc_d680, 0x3f7c_d6e4],
            [0xbdff_35fe, 0xbf7f_3663],
        ),
        (
            [0x3f00_0000, 0xbe80_0000, 0x3a83_126f, 0x4040_0000],
            [0xbdaa_8d89, 0xbe2a_b1f1],
        ),
        (
            [0x3f80_0000, 0, 0x3f35_04f3, 0x3f35_04f3],
            [0x3f35_04f4, 0xbf35_04f4],
        ),
        (
            [0x3f80_0000, 0, 0xbf19_188c, 0x3f4d_2d57],
            [0xbf19_188d, 0xbf4d_2d59],
        ),
        (
            [0x3f80_0000, 0, 0x3f78_5029, 0xbe79_0c8e],
            [0x3f78_5029, 0x3e79_0c8e],
        ),
        (
            [0xbf9e_c796, 0x3fa5_bf6a, 0xbffa_4fc4, 0xc03b_0afa],
            [0xbde1_042b, 0xbefe_f549],
        ),
        (
            [0xc014_b4dd, 0x3fb1_8e0e, 0x3eff_d650, 0xc01c_9b88],
            [0xbf3a_f698, 0xbf4c_e7f9],
        ),
    ];

    #[test]
    fn division_matches_the_official_build() {
        for ([a, b, c, d], expected) in CASES {
            let z = Complex {
                re: f32::from_bits(a),
                im: f32::from_bits(b),
            };
            let w = Complex {
                re: f32::from_bits(c),
                im: f32::from_bits(d),
            };
            let q = z / w;
            assert_eq!([q.re.to_bits(), q.im.to_bits()], expected, "{z:?} / {w:?}");
        }
    }
}
