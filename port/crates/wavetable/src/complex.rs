//! `std::complex<float>` with the arithmetic upstream's release builds use.
//!
//! Under `-ffast-math` (which implies `-fcx-limited-range`) GCC evaluates
//! complex products with the textbook formula and expands `std::abs` to
//! `sqrtf(re * re + im * im)` instead of calling `cabsf`. The reference is
//! built that way, so this type does the same.

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

    /// `std::abs(z)` as the fast-math build computes it.
    #[inline]
    pub fn abs(self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
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

    /// `value / z` for a real `value`, in the limited-range form
    /// `value * conj(z) / |z|^2`.
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
