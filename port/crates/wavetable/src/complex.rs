//! `std::complex<float>` with upstream source semantics: products use the
//! textbook formula (what libstdc++ computes for finite values), `abs` is
//! `cabsf` (`hypotf`) and `arg` is `atan2f`.
//!
//! The `-ffast-math` reference expands `std::abs` to `sqrtf(re² + im²)`
//! instead; the two differ by at most 1 ulp, which the parity tolerances
//! absorb, so the source semantics are kept.

use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

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

    /// `value / z` for a real `value`: `value * conj(z) / |z|^2`, the
    /// straightforward formula (libgcc's `__divsc3` adds scaling that only
    /// matters near overflow).
    #[inline]
    pub fn real_div(value: f32, z: Complex) -> Self {
        let denominator = z.re * z.re + z.im * z.im;
        Self {
            re: (value * z.re) / denominator,
            im: (-value * z.im) / denominator,
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
