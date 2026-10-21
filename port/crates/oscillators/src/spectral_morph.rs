// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The spectral morph functions: each builds one wave buffer from a
//! wavetable frame's harmonics and inverse-transforms it.
//!
//! Ports `vital/src/synthesis/producers/spectral_morph.h`.
//!
//! A destination is a whole Fourier buffer of the oscillator
//! ([`SPECTRAL_BUFFER_FLOATS`] floats, upstream `poly_float
//! [kSpectralBufferSize]`), addressed in floats. The spectrum is written
//! from float 4 (`dest + 1` in `poly_float`s), the inverse transform runs in
//! place there, and [`transform_and_wrap_buffer`] then copies four samples
//! of wrap-around to each side. Like upstream, the functions read and write
//! only the regions they name: what is left in the rest of the buffer (the
//! Nyquist bin, the inharmonic morph's index table) carries over to the next
//! transform of the same buffer, so the oscillator keeps every buffer for its
//! lifetime.

use sas_simd::PolyFloat;
use sas_wavetable::wave_frame::WAVEFORM_SIZE;
use sas_wavetable::wavetable::NUM_HARMONICS as WAVETABLE_NUM_HARMONICS;
use sas_wavetable::{FourierTransform, WavetableData};

/// `vital::kNumHarmonics` (`spectral_morph.h`).
pub const NUM_HARMONICS: usize = WAVEFORM_SIZE / 2 + 1;
pub const MAX_FORMANT_SHIFT: f32 = 1.0;
pub const MAX_EVEN_ODD_FORMANT_SHIFT: f32 = 2.0;
pub const MAX_HARMONIC_SCALE: f32 = 4.0;
pub const MAX_INHARMONIC_SCALE: f32 = 12.0;
pub const RANDOM_AMPLITUDE_STAGES: usize = 16;
pub const PHASE_DISPERSE_SCALE: f32 = 0.05;
pub const SKEW_SCALE: f32 = 16.0;
/// `kMaxPolyIndex`, in `poly_float`s.
pub const MAX_POLY_INDEX: usize = WAVEFORM_SIZE / 4;

/// `SynthOscillator::kSpectralBufferSize` in floats.
pub const SPECTRAL_BUFFER_FLOATS: usize = (WAVEFORM_SIZE * 2 / 4 + 4) * 4;
/// Float offset of the spectrum (`dest + 1`).
const WAVE_START: usize = 4;

/// The signature shared by the morphs: `(wavetable_data, wavetable_index,
/// dest, transform, shift, last_harmonic, data_buffer)`. `data_buffer` is
/// [`RandomValues`](crate::synth_oscillator::RandomValues) in floats.
pub type MorphFn = fn(&WavetableData, usize, &mut [f32], &mut FourierTransform, f32, i32, &[f32]);

#[inline(always)]
fn load(buffer: &[f32], poly_index: usize) -> PolyFloat {
    PolyFloat::load(&buffer[4 * poly_index..])
}

#[inline(always)]
fn store(buffer: &mut [f32], poly_index: usize, value: PolyFloat) {
    buffer[4 * poly_index..4 * poly_index + 4].copy_from_slice(&value.to_array());
}

/// `transformAndWrapBuffer`.
pub fn transform_and_wrap_buffer(transform: &mut FourierTransform, buffer: &mut [f32]) {
    transform.transform_real_inverse(&mut buffer[4..]);
    for i in 0..4 {
        buffer[i] = buffer[i + WAVEFORM_SIZE];
        buffer[i + WAVEFORM_SIZE + 4] = buffer[i + 4];
    }
}

pub fn passthrough_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    _shift: f32,
    last_harmonic: i32,
    _data_buffer: &[f32],
) {
    let frequency_amplitudes = &data.frequency_amplitudes[wavetable_index];
    let normalized_frequencies = &data.normalized_frequencies[wavetable_index];
    let wave_start = &mut dest[WAVE_START..];
    let last_index = (2 * last_harmonic / 4) as usize;

    for i in 0..=last_index {
        let value = load(frequency_amplitudes, i) * load(normalized_frequencies, i);
        store(wave_start, i, value);
    }
    for i in last_index + 1..MAX_POLY_INDEX {
        store(wave_start, i, PolyFloat::zero());
    }
    transform_and_wrap_buffer(transform, dest);
}

const _: () = assert!(NUM_HARMONICS == WAVETABLE_NUM_HARMONICS);
