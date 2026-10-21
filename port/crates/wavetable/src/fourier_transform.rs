//! Ports `vital/src/common/fourier_transform.h` (Copyright 2013-2019 Matt
//! Tytel, GPL-3.0-or-later) with the backend the reference pins: JUCE's
//! built-in `FFTFallback` (`juce_dsp/frequency/juce_FFT.cpp`, JUCE 6,
//! used here under GPLv3). Upstream's other backends (IPP, Apple vDSP,
//! FFTW, kissfft) scale and order results differently; see
//! `migration/decisions/reference-determinism.md`.
//!
//! The transform is JUCE's recursive mixed-radix decimation (radix 4 and 2
//! for powers of two) with a twiddle table computed in double precision.
//! The twiddles use `libm`'s `cos`/`sin`: glibc's double versions return
//! different bits with and without FMA, and after rounding to `float` the
//! table matches the reference's for every size checked.
//! Real-only transforms work in place on `2 * size` floats:
//!
//! - forward: the input is the first `size` floats; the output is the full
//!   complex spectrum (`size` bins, interleaved re/im, unnormalised).
//! - inverse: the input is the first `size / 2 + 1` bins (the rest are
//!   rebuilt as conjugates, which also negates the Nyquist bin's imaginary
//!   part); the output is scaled by `1 / size`, real parts in the first
//!   `size` floats and imaginary parts in the second `size`.

use crate::complex::Complex;

#[derive(Clone, Copy, Debug, Default)]
struct Factor {
    radix: usize,
    length: usize,
}

/// `juce::dsp::FFTFallback::FFTConfig`.
#[derive(Clone, Debug)]
struct FftConfig {
    fft_size: usize,
    inverse: bool,
    factors: [Factor; 32],
    twiddles: Vec<Complex>,
}

/// One twiddle, `(float) cos(phase), (float) sin(phase)` in double. On Apple
/// targets Clang merges the pair into `__sincos_stret` (A11), so that is
/// called; elsewhere the pure-Rust `libm` crate stands in for glibc, whose
/// double `sin`/`cos` depend on the CPU's FMA support.
fn twiddle_at(phase: f64) -> Complex {
    #[cfg(target_vendor = "apple")]
    let (sin, cos) = {
        #[repr(C)]
        struct SinCos {
            sin: f64,
            cos: f64,
        }
        extern "C" {
            fn __sincos_stret(angle: f64) -> SinCos;
        }
        let result = unsafe { __sincos_stret(phase) };
        (result.sin, result.cos)
    };
    #[cfg(not(target_vendor = "apple"))]
    let (sin, cos) = (libm::sin(phase), libm::cos(phase));
    Complex::new(cos as f32, sin as f32)
}

impl FftConfig {
    fn new(fft_size: usize, inverse: bool) -> Self {
        let inverse_factor =
            (if inverse { 2.0 } else { -2.0 }) * std::f64::consts::PI / fft_size as f64;
        let mut twiddles = vec![Complex::ZERO; fft_size];

        if fft_size <= 4 {
            for (i, twiddle) in twiddles.iter_mut().enumerate() {
                let phase = i as f64 * inverse_factor;
                *twiddle = twiddle_at(phase);
            }
        } else {
            for (i, twiddle) in twiddles.iter_mut().enumerate().take(fft_size / 4) {
                let phase = i as f64 * inverse_factor;
                *twiddle = twiddle_at(phase);
            }
            for i in fft_size / 4..fft_size / 2 {
                let other = twiddles[i - fft_size / 4];
                twiddles[i] = if inverse {
                    Complex::new(-other.im, other.re)
                } else {
                    Complex::new(other.im, -other.re)
                };
            }
            twiddles[fft_size / 2] = Complex::new(-1.0, 0.0);
            for i in fft_size / 2..fft_size {
                let index = fft_size / 2 - (i - fft_size / 2);
                twiddles[i] = twiddles[index].conj();
            }
        }

        let root = (fft_size as f64).sqrt() as usize;
        let mut divisor = 4usize;
        let mut n = fft_size;
        let mut factors = [Factor::default(); 32];
        for factor in &mut factors {
            while !n.is_multiple_of(divisor) {
                if divisor == 2 {
                    divisor = 3;
                } else if divisor == 4 {
                    divisor = 2;
                } else {
                    divisor += 2;
                }
                if divisor > root {
                    divisor = n;
                }
            }
            n /= divisor;
            *factor = Factor {
                radix: divisor,
                length: n,
            };
        }

        Self {
            fft_size,
            inverse,
            factors,
            twiddles,
        }
    }

    fn perform(&self, input: &[Complex], output: &mut [Complex]) {
        self.perform_recursive(input, 0, output, 0, 1, 1, 0);
    }

    #[allow(clippy::too_many_arguments)]
    fn perform_recursive(
        &self,
        input: &[Complex],
        input_pos: usize,
        output: &mut [Complex],
        output_pos: usize,
        stride: usize,
        stride_in: usize,
        factor_index: usize,
    ) {
        let factor = self.factors[factor_index];
        let output_end = output_pos + factor.radix * factor.length;

        if stride == 1 && factor.radix <= 5 {
            for i in 0..factor.radix {
                self.perform_recursive(
                    input,
                    input_pos + stride * stride_in * i,
                    output,
                    output_pos + i * factor.length,
                    stride * factor.radix,
                    stride_in,
                    factor_index + 1,
                );
            }
            self.butterfly(factor, output, output_pos, stride);
            return;
        }

        let mut in_pos = input_pos;
        let mut out_pos = output_pos;
        if factor.length == 1 {
            while out_pos < output_end {
                output[out_pos] = input[in_pos];
                out_pos += 1;
                in_pos += stride * stride_in;
            }
        } else {
            while out_pos < output_end {
                self.perform_recursive(
                    input,
                    in_pos,
                    output,
                    out_pos,
                    stride * factor.radix,
                    stride_in,
                    factor_index + 1,
                );
                in_pos += stride * stride_in;
                out_pos += factor.length;
            }
        }

        self.butterfly(factor, output, output_pos, stride);
    }

    fn butterfly(&self, factor: Factor, data: &mut [Complex], pos: usize, stride: usize) {
        match factor.radix {
            1 => {}
            2 => return self.butterfly2(data, pos, stride, factor.length),
            4 => return self.butterfly4(data, pos, stride, factor.length),
            _ => debug_assert!(false, "unsupported radix {}", factor.radix),
        }

        let mut scratch = vec![Complex::ZERO; factor.radix];
        for i in 0..factor.length {
            let mut k = i;
            for value in scratch.iter_mut() {
                *value = data[pos + k];
                k += factor.length;
            }

            let mut k = i;
            for _ in 0..factor.radix {
                let mut twiddle_index = 0usize;
                data[pos + k] = scratch[0];
                for value in scratch.iter().skip(1) {
                    twiddle_index += stride * k;
                    if twiddle_index >= self.fft_size {
                        twiddle_index -= self.fft_size;
                    }
                    let product = *value * self.twiddles[twiddle_index];
                    data[pos + k] += product;
                }
                k += factor.length;
            }
        }
    }

    fn butterfly2(&self, data: &mut [Complex], pos: usize, stride: usize, length: usize) {
        let mut tw = 0usize;
        for i in 0..length {
            let mut s = data[pos + length + i];
            s *= self.twiddles[tw];
            tw += stride;
            data[pos + length + i] = data[pos + i] - s;
            data[pos + i] += s;
        }
    }

    fn butterfly4(&self, data: &mut [Complex], pos: usize, stride: usize, length: usize) {
        let length_x2 = length * 2;
        let length_x3 = length * 3;
        let stride_x2 = stride * 2;
        let stride_x3 = stride * 3;

        let mut twiddle1 = 0usize;
        let mut twiddle2 = 0usize;
        let mut twiddle3 = 0usize;

        for i in 0..length {
            let d = pos + i;
            let s0 = data[d + length] * self.twiddles[twiddle1];
            let s1 = data[d + length_x2] * self.twiddles[twiddle2];
            let s2 = data[d + length_x3] * self.twiddles[twiddle3];
            let s3 = s0 + s2;
            let s4 = s0 - s2;
            let s5 = data[d] - s1;

            data[d] += s1;
            data[d + length_x2] = data[d];
            data[d + length_x2] -= s3;
            twiddle1 += stride;
            twiddle2 += stride_x2;
            twiddle3 += stride_x3;
            data[d] += s3;

            if self.inverse {
                data[d + length] = Complex::new(s5.re - s4.im, s5.im + s4.re);
                data[d + length_x3] = Complex::new(s5.re + s4.im, s5.im - s4.re);
            } else {
                data[d + length] = Complex::new(s5.re + s4.im, s5.im - s4.re);
                data[d + length_x3] = Complex::new(s5.re - s4.im, s5.im + s4.re);
            }
        }
    }
}

/// `vital::FourierTransform` on the JUCE backend.
#[derive(Clone, Debug)]
pub struct FourierTransform {
    size: usize,
    forward: FftConfig,
    inverse: FftConfig,
    scratch: Vec<Complex>,
    buffer: Vec<Complex>,
}

impl FourierTransform {
    /// A transform of `1 << bits` points.
    pub fn new(bits: u32) -> Self {
        let size = 1usize << bits;
        Self {
            size,
            forward: FftConfig::new(size, false),
            inverse: FftConfig::new(size, true),
            scratch: vec![Complex::ZERO; size],
            buffer: vec![Complex::ZERO; size],
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// The complex transform (`juce::dsp::FFT::perform`); the inverse is
    /// scaled by `1 / size`.
    pub fn perform(&self, input: &[Complex], output: &mut [Complex], inverse: bool) {
        if self.size == 1 {
            output[0] = input[0];
            return;
        }
        if inverse {
            self.inverse.perform(input, output);
            let scale = 1.0 / self.size as f32;
            for value in &mut output[..self.size] {
                *value *= scale;
            }
        } else {
            self.forward.perform(input, output);
        }
    }

    /// `transformRealForward(data)`: `data` holds at least `2 * size`
    /// floats; the first `size` are the input.
    pub fn transform_real_forward(&mut self, data: &mut [f32]) {
        let size = self.size;
        if size == 1 {
            return;
        }
        for (value, &sample) in self.scratch.iter_mut().zip(&data[..size]) {
            *value = Complex::real(sample);
        }
        let mut output = std::mem::take(&mut self.buffer);
        self.forward.perform(&self.scratch, &mut output);
        for (i, value) in output.iter().enumerate() {
            data[2 * i] = value.re;
            data[2 * i + 1] = value.im;
        }
        self.buffer = output;
    }

    /// `transformRealInverse(data)`: `data` holds at least `2 * size`
    /// floats, read as `size` interleaved complex bins.
    pub fn transform_real_inverse(&mut self, data: &mut [f32]) {
        let size = self.size;
        if size == 1 {
            return;
        }
        let mut input = std::mem::take(&mut self.buffer);
        for (i, value) in input.iter_mut().enumerate() {
            *value = Complex::new(data[2 * i], data[2 * i + 1]);
        }
        for i in size / 2..size {
            input[i] = input[size - i].conj();
        }

        let mut output = std::mem::take(&mut self.scratch);
        self.perform(&input, &mut output, true);
        for (i, value) in output.iter().enumerate() {
            data[i] = value.re;
            data[i + size] = value.im;
        }
        self.scratch = output;
        self.buffer = input;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive_dft(input: &[Complex], inverse: bool) -> Vec<Complex> {
        let n = input.len();
        let sign = if inverse { 1.0 } else { -1.0 };
        (0..n)
            .map(|k| {
                let mut re = 0.0f64;
                let mut im = 0.0f64;
                for (j, x) in input.iter().enumerate() {
                    let angle = sign * 2.0 * std::f64::consts::PI * (j * k % n) as f64 / n as f64;
                    re += x.re as f64 * angle.cos() - x.im as f64 * angle.sin();
                    im += x.re as f64 * angle.sin() + x.im as f64 * angle.cos();
                }
                let scale = if inverse { 1.0 / n as f64 } else { 1.0 };
                Complex::new((re * scale) as f32, (im * scale) as f32)
            })
            .collect()
    }

    #[test]
    fn factors_for_2048() {
        let config = FftConfig::new(2048, false);
        let radices: Vec<usize> = config.factors.iter().take(7).map(|f| f.radix).collect();
        assert_eq!(radices, vec![4, 4, 4, 4, 4, 2, 1]);
    }

    #[test]
    fn matches_naive_dft() {
        for bits in 1..=11 {
            let transform = FourierTransform::new(bits);
            let n = transform.size();
            let input: Vec<Complex> = (0..n)
                .map(|i| {
                    Complex::new(
                        ((i * 7919) % 13) as f32 - 6.0,
                        ((i * 104729) % 7) as f32 - 3.0,
                    )
                })
                .collect();
            for inverse in [false, true] {
                let mut output = vec![Complex::ZERO; n];
                transform.perform(&input, &mut output, inverse);
                let expected = naive_dft(&input, inverse);
                for (a, b) in output.iter().zip(&expected) {
                    assert!(
                        (a.re - b.re).abs() < 1e-2 && (a.im - b.im).abs() < 1e-2,
                        "bits {bits}"
                    );
                }
            }
        }
    }

    #[test]
    fn real_round_trip() {
        let mut transform = FourierTransform::new(11);
        let original: Vec<f32> = (0..2048)
            .map(|i| ((i as f32) * 0.37).sin() + 0.25)
            .collect();
        let mut data = vec![0.0f32; 4096];
        data[..2048].copy_from_slice(&original);
        transform.transform_real_forward(&mut data);
        assert!((data[0] - original.iter().sum::<f32>()).abs() < 1e-2);
        transform.transform_real_inverse(&mut data);
        for (a, b) in data.iter().zip(&original) {
            assert!((a - b).abs() < 1e-5);
        }
    }
}
