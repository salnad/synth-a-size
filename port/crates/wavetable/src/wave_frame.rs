//! Ports `vital/src/synthesis/lookups/wave_frame.{h,cpp}` (Copyright
//! 2013-2019 Matt Tytel, GPL-3.0-or-later): one 2048-sample waveform cycle
//! kept in both the time and the frequency domain.

use std::cell::RefCell;
use std::sync::OnceLock;

use crate::complex::Complex;
use crate::fourier_transform::FourierTransform;

/// `WaveFrame::kWaveformBits`.
pub const WAVEFORM_BITS: u32 = 11;
/// `WaveFrame::kWaveformSize`.
pub const WAVEFORM_SIZE: usize = 1 << WAVEFORM_BITS;
/// `WaveFrame::kNumRealComplex`: bins 0 through Nyquist.
pub const NUM_REAL_COMPLEX: usize = WAVEFORM_SIZE / 2 + 1;
/// `WaveFrame::kNumExtraComplex`.
pub const NUM_EXTRA_COMPLEX: usize = WAVEFORM_SIZE - NUM_REAL_COMPLEX;
pub const DEFAULT_FREQUENCY_RATIO: f32 = 1.0;
pub const DEFAULT_SAMPLE_RATE: f32 = 44100.0;

thread_local! {
    static WAVEFORM_FFT: RefCell<FourierTransform> = RefCell::new(FourierTransform::new(WAVEFORM_BITS));
}

/// Runs `f` with this thread's shared `FFT<kWaveformBits>` instance.
pub fn with_waveform_fft<R>(f: impl FnOnce(&mut FourierTransform) -> R) -> R {
    WAVEFORM_FFT.with(|fft| f(&mut fft.borrow_mut()))
}

/// `vital::WaveFrame`.
#[derive(Clone, Debug)]
pub struct WaveFrame {
    pub index: i32,
    pub frequency_ratio: f32,
    pub sample_rate: f32,
    /// `2 * kWaveformSize` floats. Only the first half is the waveform; the
    /// inverse transform leaves its imaginary residue in the second half.
    pub time_domain: Box<[f32; 2 * WAVEFORM_SIZE]>,
    /// The full complex spectrum (`kWaveformSize` bins).
    pub frequency_domain: Box<[Complex; WAVEFORM_SIZE]>,
}

impl Default for WaveFrame {
    fn default() -> Self {
        Self {
            index: 0,
            frequency_ratio: DEFAULT_FREQUENCY_RATIO,
            sample_rate: DEFAULT_SAMPLE_RATE,
            time_domain: Box::new([0.0; 2 * WAVEFORM_SIZE]),
            frequency_domain: Box::new([Complex::ZERO; WAVEFORM_SIZE]),
        }
    }
}

impl WaveFrame {
    pub fn new() -> Self {
        Self::default()
    }

    /// The waveform: the first `kWaveformSize` time-domain samples.
    pub fn samples(&self) -> &[f32] {
        &self.time_domain[..WAVEFORM_SIZE]
    }

    /// `getMaxZeroOffset()`.
    pub fn max_zero_offset(&self) -> f32 {
        let mut max = 0.0f32;
        for &value in &self.time_domain[..WAVEFORM_SIZE] {
            max = if max < value.abs() { value.abs() } else { max };
        }
        max
    }

    /// `normalize(allow_positive_gain)`: scales the time domain so its peak
    /// is 1 (only downwards unless `allow_positive_gain`).
    pub fn normalize(&mut self, allow_positive_gain: bool) {
        const MAX_INVERSE_MULT: f32 = 0.000_000_1;
        let max = self.max_zero_offset();
        let min = if allow_positive_gain {
            MAX_INVERSE_MULT
        } else {
            1.0
        };
        let normalization = 1.0 / if min < max { max } else { min };
        for value in &mut self.time_domain[..WAVEFORM_SIZE] {
            *value *= normalization;
        }
    }

    /// `clear()`.
    pub fn clear(&mut self) {
        self.frequency_ratio = DEFAULT_FREQUENCY_RATIO;
        self.sample_rate = DEFAULT_SAMPLE_RATE;
        self.frequency_domain.fill(Complex::ZERO);
        self.time_domain[..WAVEFORM_SIZE].fill(0.0);
    }

    pub fn set_frequency_ratio(&mut self, ratio: f32) {
        self.frequency_ratio = ratio;
    }

    pub fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate;
    }

    /// `multiply(value)`.
    pub fn multiply(&mut self, value: f32) {
        for i in 0..WAVEFORM_SIZE {
            self.time_domain[i] *= value;
            self.frequency_domain[i] *= value;
        }
    }

    /// `loadTimeDomain(buffer)`.
    pub fn load_time_domain(&mut self, buffer: &[f32]) {
        self.time_domain[..WAVEFORM_SIZE].copy_from_slice(&buffer[..WAVEFORM_SIZE]);
        self.to_frequency_domain();
    }

    /// `addFrom(source)`.
    pub fn add_from(&mut self, source: &WaveFrame) {
        for i in 0..WAVEFORM_SIZE {
            self.time_domain[i] += source.time_domain[i];
            self.frequency_domain[i] += source.frequency_domain[i];
        }
    }

    /// `copy(other)`: both domains (the first `kWaveformSize` time samples),
    /// not the index, ratio or sample rate.
    pub fn copy_from(&mut self, other: &WaveFrame) {
        self.frequency_domain
            .copy_from_slice(&other.frequency_domain[..]);
        self.time_domain[..WAVEFORM_SIZE].copy_from_slice(&other.time_domain[..WAVEFORM_SIZE]);
    }

    /// `toFrequencyDomain()`.
    pub fn to_frequency_domain(&mut self) {
        let mut data = [0.0f32; 2 * WAVEFORM_SIZE];
        data[..WAVEFORM_SIZE].copy_from_slice(&self.time_domain[..WAVEFORM_SIZE]);
        with_waveform_fft(|fft| fft.transform_real_forward(&mut data));
        for (i, bin) in self.frequency_domain.iter_mut().enumerate() {
            *bin = Complex::new(data[2 * i], data[2 * i + 1]);
        }
    }

    /// `toTimeDomain()`: zeroes the bins above Nyquist, then inverse
    /// transforms bins 0 through Nyquist.
    pub fn to_time_domain(&mut self) {
        for i in 0..NUM_REAL_COMPLEX {
            self.time_domain[2 * i] = self.frequency_domain[i].re;
            self.time_domain[2 * i + 1] = self.frequency_domain[i].im;
        }
        for bin in &mut self.frequency_domain[NUM_REAL_COMPLEX..] {
            *bin = Complex::ZERO;
        }
        let time_domain = &mut self.time_domain[..];
        with_waveform_fft(|fft| fft.transform_real_inverse(time_domain));
    }

    /// `removedDc()`: subtracts the imaginary part of bin 0 (which the JUCE
    /// backend leaves at 0) and clears bin 0.
    pub fn removed_dc(&mut self) {
        let offset = self.frequency_domain[0].im;
        self.frequency_domain[0] = Complex::ZERO;
        for value in &mut self.time_domain[..WAVEFORM_SIZE] {
            *value -= offset;
        }
    }
}

/// `PredefinedWaveFrames::Shape`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredefinedShape {
    Sin,
    SaturatedSin,
    Triangle,
    Square,
    Pulse,
    Saw,
}

impl PredefinedShape {
    pub const ALL: [PredefinedShape; 6] = [
        PredefinedShape::Sin,
        PredefinedShape::SaturatedSin,
        PredefinedShape::Triangle,
        PredefinedShape::Square,
        PredefinedShape::Pulse,
        PredefinedShape::Saw,
    ];
}

/// `PredefinedWaveFrames`: the six basic shapes, built once.
pub struct PredefinedWaveFrames {
    frames: [WaveFrame; 6],
}

impl PredefinedWaveFrames {
    fn build() -> Self {
        let mut frames: [WaveFrame; 6] = Default::default();
        create_sin(&mut frames[0]);
        create_saturated_sin(&mut frames[1]);
        create_triangle(&mut frames[2]);
        create_square(&mut frames[3]);
        create_saw(&mut frames[5]);
        create_pulse(&mut frames[4]);
        Self { frames }
    }

    /// `PredefinedWaveFrames::getWaveFrame(shape)`.
    pub fn get(shape: PredefinedShape) -> &'static WaveFrame {
        static INSTANCE: OnceLock<PredefinedWaveFrames> = OnceLock::new();
        &INSTANCE.get_or_init(Self::build).frames[shape as usize]
    }
}

fn create_sin(frame: &mut WaveFrame) {
    let half_waveform = (WAVEFORM_SIZE / 2) as f32;
    frame.frequency_domain[1] = Complex::real(half_waveform);
    frame.to_time_domain();
}

fn create_saturated_sin(frame: &mut WaveFrame) {
    frame.frequency_domain[1] = Complex::real(WAVEFORM_SIZE as f32);
    frame.to_time_domain();
    for value in &mut frame.time_domain[..WAVEFORM_SIZE] {
        *value = sas_common::scalar::tanh(*value);
    }
    frame.to_frequency_domain();
}

fn create_triangle(frame: &mut WaveFrame) {
    let section_size = WAVEFORM_SIZE / 4;
    for i in 0..section_size {
        let t = i as f32 / (section_size as f32 * 1.0);
        frame.time_domain[i] = 1.0 - t;
        frame.time_domain[i + section_size] = -t;
        frame.time_domain[i + 2 * section_size] = t - 1.0;
        frame.time_domain[i + 3 * section_size] = t;
    }
    frame.to_frequency_domain();
}

fn create_square(frame: &mut WaveFrame) {
    let section_size = WAVEFORM_SIZE / 4;
    for i in 0..section_size {
        frame.time_domain[i] = 1.0;
        frame.time_domain[i + section_size] = -1.0;
        frame.time_domain[i + 2 * section_size] = -1.0;
        frame.time_domain[i + 3 * section_size] = 1.0;
    }
    frame.to_frequency_domain();
}

fn create_pulse(frame: &mut WaveFrame) {
    let sections = 4;
    let pulse_size = WAVEFORM_SIZE / sections;
    for i in 0..pulse_size {
        frame.time_domain[i + (sections - 1) * pulse_size] = 1.0;
        for s in 0..sections - 1 {
            frame.time_domain[i + s * pulse_size] = -1.0;
        }
    }
    frame.to_frequency_domain();
}

fn create_saw(frame: &mut WaveFrame) {
    let section_size = WAVEFORM_SIZE / 2;
    for i in 0..section_size {
        let t = i as f32 / (section_size as f32 * 1.0);
        frame.time_domain[(i + WAVEFORM_SIZE / 4) % WAVEFORM_SIZE] = t - 1.0;
        frame.time_domain[(i + section_size + WAVEFORM_SIZE / 4) % WAVEFORM_SIZE] = t;
    }
    frame.to_frequency_domain();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predefined_sin_is_unit_cosine() {
        let frame = PredefinedWaveFrames::get(PredefinedShape::Sin);
        assert!((frame.time_domain[0] - 1.0).abs() < 1e-6);
        assert!(frame.time_domain[WAVEFORM_SIZE / 2] + 1.0 < 1e-6);
        assert!(frame.time_domain[WAVEFORM_SIZE / 4].abs() < 1e-6);
    }

    #[test]
    fn time_frequency_round_trip() {
        let mut frame = WaveFrame::new();
        for i in 0..WAVEFORM_SIZE {
            frame.time_domain[i] = ((i * 37) % 101) as f32 / 50.0 - 1.0;
        }
        let original = frame.time_domain.clone();
        frame.to_frequency_domain();
        frame.to_time_domain();
        for i in 0..WAVEFORM_SIZE {
            assert!((frame.time_domain[i] - original[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn normalize_and_dc() {
        let mut frame = WaveFrame::new();
        frame.time_domain[3] = -4.0;
        frame.time_domain[5] = 2.0;
        frame.normalize(false);
        assert_eq!(frame.time_domain[3], -1.0);
        let mut small = WaveFrame::new();
        small.time_domain[0] = 0.5;
        small.normalize(false);
        assert_eq!(small.time_domain[0], 0.5);
        small.normalize(true);
        assert_eq!(small.time_domain[0], 1.0);
    }
}
