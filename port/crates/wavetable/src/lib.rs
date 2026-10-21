//! Wavetable data and the wavetable creator (sources, modifiers, keyframes,
//! groups) with JSON state.
//!
//! - [`fourier_transform`]: the FFT the reference pins (JUCE's fallback
//!   engine), with its scaling and bin ordering;
//! - [`wave_frame`]: `WaveFrame` and `PredefinedWaveFrames`;
//! - [`creator`]: the `WavetableCreator` state model (groups, components,
//!   keyframes) with upstream's exact JSON load/save and version upgrades.
//!
//! The crate depends only on `sas-common`, so `sas-preset` can use it for
//! exact wavetable JSON without pulling in the processing crates.

pub mod complex;
pub mod creator;
pub mod fourier_transform;
mod platform_libm;
pub mod wave_frame;
pub mod wavetable;

pub use complex::Complex;
pub use fourier_transform::FourierTransform;
pub use wave_frame::{PredefinedShape, PredefinedWaveFrames, WaveFrame};
pub use wavetable::{Wavetable, WavetableData};

/// `vital::kNumOscillatorWaveFrames` (`synth_constants.h`).
pub const NUM_OSCILLATOR_WAVE_FRAMES: i32 = 257;

/// Milestone in `migration/PLAN.md` that ports this crate.
pub const MILESTONE: &str = "M4";

/// Upstream files (relative to the repository root) this crate ports.
pub const UPSTREAM_SOURCES: &[&str] = &[
    "vital/src/common/fourier_transform.h",
    "vital/third_party/JUCE/modules/juce_dsp/frequency/juce_FFT.cpp",
    "vital/src/synthesis/lookups/wave_frame.cpp",
    "vital/src/synthesis/lookups/wave_frame.h",
    "vital/src/synthesis/lookups/wavetable.cpp",
    "vital/src/synthesis/lookups/wavetable.h",
    "vital/src/common/wavetable/",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_sources_exist() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        for source in UPSTREAM_SOURCES {
            assert!(repo.join(source).exists(), "missing upstream path {source}");
        }
        assert!(MILESTONE.starts_with('M'));
    }
}
