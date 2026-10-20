// SPDX-License-Identifier: GPL-3.0-or-later

//! Flush-to-zero for the duration of engine processing.
//!
//! Upstream processes every block inside JUCE's `ScopedNoDenormals`
//! (`SynthPlugin::processBlock`), which sets MXCSR FTZ and DAZ on x86 and
//! FPCR.FZ on arm64, and the reference renders do the same. Denormal results
//! are then flushed to zero, which changes the output of anything that
//! decays into the denormal range (filters, envelopes, feedback delays).
//! The engine's block entry point must hold a [`ScopedNoDenormals`] around
//! processing, exactly where upstream does, for parity on those paths.
//!
//! Rust code compiled without the guard assumes IEEE gradual underflow;
//! only hold the guard around engine processing, never across code that
//! relies on denormals.

/// `juce::ScopedNoDenormals`: sets flush-to-zero (and denormals-are-zero on
/// x86) and restores the previous mode on drop.
#[must_use = "the mode is restored when the guard is dropped"]
pub struct ScopedNoDenormals {
    previous: usize,
}

impl ScopedNoDenormals {
    #[cfg(target_arch = "x86_64")]
    pub fn new() -> Self {
        // MXCSR bit 15 = FTZ, bit 6 = DAZ (JUCE's 0x8040).
        let previous = read_mode();
        write_mode(previous | 0x8040);
        ScopedNoDenormals { previous }
    }

    #[cfg(target_arch = "aarch64")]
    pub fn new() -> Self {
        // FPCR bit 24 = FZ.
        let previous = read_mode();
        write_mode(previous | (1 << 24));
        ScopedNoDenormals { previous }
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    pub fn new() -> Self {
        ScopedNoDenormals { previous: 0 }
    }
}

impl Default for ScopedNoDenormals {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ScopedNoDenormals {
    fn drop(&mut self) {
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        write_mode(self.previous);
    }
}

#[cfg(target_arch = "x86_64")]
fn read_mode() -> usize {
    let mut mxcsr: u32 = 0;
    // SAFETY: STMXCSR stores the 32-bit MXCSR to the given address.
    unsafe {
        std::arch::asm!("stmxcsr [{}]", in(reg) &mut mxcsr, options(nostack, preserves_flags));
    }
    mxcsr as usize
}

#[cfg(target_arch = "x86_64")]
fn write_mode(mode: usize) {
    let mxcsr = mode as u32;
    // SAFETY: LDMXCSR loads MXCSR from the given address; only the FTZ/DAZ
    // bits differ from the current value.
    unsafe {
        std::arch::asm!("ldmxcsr [{}]", in(reg) &mxcsr, options(nostack, preserves_flags, readonly));
    }
}

#[cfg(target_arch = "aarch64")]
fn read_mode() -> usize {
    let fpcr: u64;
    // SAFETY: reading FPCR has no side effects.
    unsafe {
        std::arch::asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack, preserves_flags));
    }
    fpcr as usize
}

#[cfg(target_arch = "aarch64")]
fn write_mode(mode: usize) {
    // SAFETY: only the FZ bit differs from the current FPCR.
    unsafe {
        std::arch::asm!("msr fpcr, {}", in(reg) mode as u64, options(nomem, nostack, preserves_flags));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flushes_and_restores() {
        let tiny = std::hint::black_box(f32::MIN_POSITIVE);
        let half = std::hint::black_box(0.5f32);
        assert!(tiny * half > 0.0, "gradual underflow outside the guard");
        {
            let _guard = ScopedNoDenormals::new();
            assert_eq!(std::hint::black_box(tiny) * std::hint::black_box(half), 0.0);
        }
        assert!(tiny * half > 0.0, "mode restored");
    }
}
