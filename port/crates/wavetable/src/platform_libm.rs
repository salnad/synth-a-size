//! Double-precision transcendentals as upstream calls them: the platform
//! libm (consequence 4 in `migration/decisions/reference-arm.md`), so on
//! macOS the port gets Apple libm's bits like the macOS reference.
//!
//! Linux is the exception. The x86 reference pins glibc to its baseline
//! (non-FMA) variants, because glibc's dispatch otherwise depends on the
//! CPU. A port calling glibc would get whichever variant the CPU selects, so
//! on Linux the pure-Rust `libm` crate stands in for the pinned variants.
//! It matches them on the whole corpus.

#[cfg(target_os = "linux")]
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

#[cfg(not(target_os = "linux"))]
pub fn exp(x: f64) -> f64 {
    x.exp()
}

#[cfg(target_os = "linux")]
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

#[cfg(not(target_os = "linux"))]
pub fn cos(x: f64) -> f64 {
    x.cos()
}

/// `(sin(x), cos(x))` where upstream computes both for one angle. Apple
/// clang merges such a pair into `__sincos_stret` (A11), so that is called
/// on Apple targets.
pub fn sin_cos(x: f64) -> (f64, f64) {
    #[cfg(target_vendor = "apple")]
    {
        #[repr(C)]
        struct SinCos {
            sin: f64,
            cos: f64,
        }
        extern "C" {
            fn __sincos_stret(angle: f64) -> SinCos;
        }
        let result = unsafe { __sincos_stret(x) };
        (result.sin, result.cos)
    }
    #[cfg(target_os = "linux")]
    {
        (libm::sin(x), libm::cos(x))
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
    {
        (x.sin(), x.cos())
    }
}
