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

use sas_dsp_core::constants;
use sas_simd::common::PI;
use sas_simd::conversions::float_to_i32;
use sas_simd::{futils, poly_utils, utils, PolyFloat};
use sas_wavetable::wave_frame::WAVEFORM_SIZE;
use sas_wavetable::wavetable::{FREQUENCY_BINS, NUM_HARMONICS as WAVETABLE_NUM_HARMONICS};
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

#[inline(always)]
fn mono_mod(value: f32) -> f32 {
    poly_utils::modulo(PolyFloat::splat(value)).get(0)
}

/// `std::min(a, b)`: `b < a ? b : a`.
#[inline(always)]
fn std_min(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}

/// `std::max(a, b)`: `a < b ? b : a`.
#[inline(always)]
fn std_max(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
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

pub fn shepard_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    shift: f32,
    last_harmonic: i32,
    _data_buffer: &[f32],
) {
    const MIN_AMPLITUDE_RATIO: f32 = 2.0;
    const MIN_AMPLITUDE_ADD: f32 = 0.001;
    let frequency_amplitudes = &data.frequency_amplitudes[wavetable_index];
    let normalized = &data.normalized_frequencies[wavetable_index];
    let phases = &data.phases[wavetable_index];
    let wave_start = &mut dest[WAVE_START..];
    let last_index = (2 * last_harmonic / 4) as usize;

    let regular_amount = 1.0 - shift;
    for i in 0..=last_index {
        let value = load(frequency_amplitudes, i) * load(normalized, i) * regular_amount;
        store(wave_start, i, value & constants::second_mask());
    }
    for i in last_index + 1..MAX_POLY_INDEX {
        store(wave_start, i, PolyFloat::zero());
    }

    for i in (0..=last_harmonic as usize).step_by(2) {
        let real_index = 2 * i;
        let imag_index = real_index + 1;

        let fundamental_amplitude = frequency_amplitudes[real_index];
        let shepard_amplitude = frequency_amplitudes[i];
        let amplitude = fundamental_amplitude + (shepard_amplitude - fundamental_amplitude) * shift;

        let ratio =
            (fundamental_amplitude + MIN_AMPLITUDE_ADD) / (shepard_amplitude + MIN_AMPLITUDE_ADD);
        let (real, imag);
        if ratio < MIN_AMPLITUDE_RATIO && ratio > (1.0 / MIN_AMPLITUDE_RATIO) {
            let fundamental_phase = phases[real_index] * (0.5 / PI);
            let shepard_phase = phases[i] * (0.5 / PI);
            let mut delta_phase = shepard_phase - fundamental_phase;
            let mut wraps = float_to_i32(delta_phase);
            wraps = (wraps + 1) / 2;
            delta_phase -= 2.0 * wraps as f32;

            let phase = fundamental_phase + delta_phase * shift;
            real = futils::sin_mono(mono_mod(phase + 0.75) - 0.5);
            imag = futils::sin_mono(mono_mod(phase + 0.5) - 0.5);
        } else {
            let fundamental_real = normalized[real_index];
            real = (normalized[i] - fundamental_real) * shift + fundamental_real;
            let fundamental_imag = normalized[real_index + 1];
            imag = (normalized[i + 1] - fundamental_imag) * shift + fundamental_imag;
        }

        wave_start[real_index] = amplitude * real;
        wave_start[imag_index] = amplitude * imag;
    }

    transform_and_wrap_buffer(transform, dest);
}

pub fn wavetable_skew_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    shift: f32,
    last_harmonic: i32,
    data_buffer: &[f32],
) {
    let num_frames = data.num_frames as i32;
    if num_frames <= 1 {
        passthrough_morph(
            data,
            wavetable_index,
            dest,
            transform,
            shift,
            last_harmonic,
            data_buffer,
        );
        return;
    }

    let wave_start = &mut dest[WAVE_START..];
    let dc_amplitude = data.frequency_amplitudes[wavetable_index][0];
    let dc_real = data.normalized_frequencies[wavetable_index][0];
    let dc_imag = data.normalized_frequencies[wavetable_index][1];
    wave_start[0] = dc_amplitude * dc_real;
    wave_start[1] = dc_amplitude * dc_imag;

    let max_frame = (sas_wavetable::NUM_OSCILLATOR_WAVE_FRAMES - 1) as f32;
    let base_wavetable_t = wavetable_index as f32 / max_frame;
    for i in 1..=last_harmonic as usize {
        let shift_scale = futils::log2_mono(i as f32) / FREQUENCY_BINS as f32;
        let base_value = PolyFloat::splat(1.0)
            - poly_utils::modulo(PolyFloat::splat(
                (base_wavetable_t + shift * shift_scale) * 0.5,
            )) * 2.0;
        let shifted_index = (1.0 - PolyFloat::abs(base_value).get(0)) * max_frame;
        let from_index = float_to_i32(shifted_index).min(num_frames - 2);
        let t = std_min(1.0, shifted_index - from_index as f32);
        let to_index = from_index + 1;

        let real_index = 2 * i;
        let imaginary_index = real_index + 1;
        let from_amplitudes = &data.frequency_amplitudes[from_index as usize];
        let to_amplitudes = &data.frequency_amplitudes[to_index as usize];
        let amplitude =
            utils::interpolate(from_amplitudes[real_index], to_amplitudes[real_index], t);

        let from_normalized = &data.normalized_frequencies[from_index as usize];
        let to_normalized = &data.normalized_frequencies[to_index as usize];
        let real = utils::interpolate(from_normalized[real_index], to_normalized[real_index], t);
        let imag = utils::interpolate(
            from_normalized[imaginary_index],
            to_normalized[imaginary_index],
            t,
        );

        wave_start[real_index] = amplitude * real;
        wave_start[imaginary_index] = amplitude * imag;
    }

    for value in &mut wave_start[2 * (last_harmonic as usize + 1)..2 * WAVEFORM_SIZE] {
        *value = 0.0;
    }

    transform_and_wrap_buffer(transform, dest);
}

pub fn phase_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    phase_shift: f32,
    last_harmonic: i32,
    _data_buffer: &[f32],
) {
    const CENTER_MORPH: f32 = 24.0;

    let frequency_amplitudes = &data.frequency_amplitudes[wavetable_index];
    let normalized_frequencies = &data.normalized_frequencies[wavetable_index];
    let wave_start = &mut dest[WAVE_START..];
    let last_index = (2 * last_harmonic / 4) as usize;

    let offset = -(CENTER_MORPH - 1.0) * (CENTER_MORPH - 1.0) * phase_shift;
    let value_offset = PolyFloat::new(0.0, 0.0, 1.0, 1.0);
    let phase_offset = PolyFloat::new(0.25, 0.0, 0.25, 0.0);
    let scale = PolyFloat::splat(0.5 / PI);
    for i in 0..=last_index {
        let amplitude = load(frequency_amplitudes, i);
        let normalized = load(normalized_frequencies, i);
        let index = value_offset + 2.0 * i as f32;

        let delta_center = (index - CENTER_MORPH) * (index - CENTER_MORPH) * phase_shift + offset;
        let phase = poly_utils::modulo(delta_center * scale + phase_offset);
        let shift = futils::sin1(phase);

        let match_mult = normalized * shift;
        let switch_mult = poly_utils::swap_stereo(normalized) * shift;
        let real = match_mult - poly_utils::swap_stereo(match_mult);
        let imag = switch_mult + poly_utils::swap_stereo(switch_mult);

        store(
            wave_start,
            i,
            amplitude * poly_utils::mask_load(imag, real, constants::left_mask()),
        );
    }
    for i in last_index + 1..MAX_POLY_INDEX {
        store(wave_start, i, PolyFloat::zero());
    }

    transform_and_wrap_buffer(transform, dest);
}

pub fn smear_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    smear: f32,
    last_harmonic: i32,
    _data_buffer: &[f32],
) {
    let frequency_amplitudes = &data.frequency_amplitudes[wavetable_index];
    let normalized_frequencies = &data.normalized_frequencies[wavetable_index];
    let wave_start = &mut dest[WAVE_START..];
    let last_index = (2 * last_harmonic / 4) as usize;

    let mut amplitude = load(frequency_amplitudes, 0) * (1.0 - smear);
    store(wave_start, 0, amplitude * load(normalized_frequencies, 0));

    for i in 1..=last_index {
        let original_amplitude = load(frequency_amplitudes, i);
        amplitude = poly_utils::interpolate(original_amplitude, amplitude, smear);

        store(wave_start, i, amplitude * load(normalized_frequencies, i));
        amplitude *= (i as f32 + 0.25) / i as f32;
    }
    for i in last_index + 1..MAX_POLY_INDEX {
        store(wave_start, i, PolyFloat::zero());
    }

    transform_and_wrap_buffer(transform, dest);
}

pub fn low_pass_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    cutoff_t: f32,
    last_harmonic: i32,
    _data_buffer: &[f32],
) {
    let frequency_amplitudes = &data.frequency_amplitudes[wavetable_index];
    let normalized_frequencies = &data.normalized_frequencies[wavetable_index];
    let wave_start = &mut dest[WAVE_START..];
    let cutoff = futils::pow_mono(2.0, (FREQUENCY_BINS - 1) as f32 * cutoff_t) + 1.0;
    let mut last_index = 2 * last_harmonic / 4;
    let poly_cutoff = std_min(last_index as f32 + 1.0, 2.0 * cutoff / 4.0);
    last_index = last_index.min(float_to_i32(poly_cutoff));
    let t = 4.0 * (poly_cutoff - last_index as f32) / 2.0;
    let last_index = last_index as usize;

    for i in 0..=last_index {
        let value = load(frequency_amplitudes, i) * load(normalized_frequencies, i);
        store(wave_start, i, value);
    }
    for i in last_index + 1..=MAX_POLY_INDEX {
        store(wave_start, i, PolyFloat::zero());
    }

    let last_mult = if t >= 1.0 {
        PolyFloat::new(1.0, 1.0, t - 1.0, t - 1.0)
    } else {
        PolyFloat::new(t, t, 0.0, 0.0)
    };
    let last = load(wave_start, last_index) * last_mult;
    store(wave_start, last_index, last);

    transform_and_wrap_buffer(transform, dest);
}

pub fn high_pass_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    cutoff_t: f32,
    last_harmonic: i32,
    _data_buffer: &[f32],
) {
    let frequency_amplitudes = &data.frequency_amplitudes[wavetable_index];
    let normalized_frequencies = &data.normalized_frequencies[wavetable_index];
    let wave_start = &mut dest[WAVE_START..];
    let mut cutoff = futils::pow_mono(2.0, (FREQUENCY_BINS - 1) as f32 * cutoff_t);
    cutoff *= (NUM_HARMONICS as f32 + 1.0) / NUM_HARMONICS as f32;
    let last_index = 2 * last_harmonic / 4;
    let poly_cutoff = std_min(last_index as f32 + 1.0, 2.0 * cutoff / 4.0);
    let start_index = float_to_i32(poly_cutoff);
    let t = 4.0 * (poly_cutoff - start_index as f32) / 2.0;
    let (start_index, last_index) = (start_index as usize, last_index as usize);

    for i in 0..start_index {
        store(wave_start, i, PolyFloat::zero());
    }
    for i in start_index..=last_index {
        let value = load(frequency_amplitudes, i) * load(normalized_frequencies, i);
        store(wave_start, i, value);
    }
    for i in last_index + 1..=MAX_POLY_INDEX {
        store(wave_start, i, PolyFloat::zero());
    }

    let last_mult = if t >= 1.0 {
        PolyFloat::new(0.0, 0.0, 2.0 - t, 2.0 - t)
    } else {
        PolyFloat::new(1.0 - t, 1.0 - t, 1.0, 1.0)
    };
    let first = load(wave_start, start_index) * last_mult;
    store(wave_start, start_index, first);

    transform_and_wrap_buffer(transform, dest);
}

pub fn even_odd_vocode_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    shift: f32,
    last_harmonic: i32,
    _data_buffer: &[f32],
) {
    let wave_start = &mut dest[WAVE_START..];
    let last_index = last_harmonic.min(float_to_i32(WAVEFORM_SIZE as f32 / (2.0 * shift)));

    let amplitudes = &data.frequency_amplitudes[wavetable_index];
    let normalized = &data.normalized_frequencies[wavetable_index];

    let dc_amplitude = amplitudes[0];
    wave_start[0] = dc_amplitude * normalized[0];
    wave_start[1] = dc_amplitude * normalized[1];

    for i in 1..=last_index {
        let shifted_index = std_max(1.0, i as f32 * shift);
        let mut index_start = float_to_i32(shifted_index);
        index_start -= (i + index_start) % 2;

        let t = (shifted_index - index_start as f32) * 0.5;
        let real_index1 = (2 * index_start) as usize;
        let real_index2 = real_index1 + 4;
        let amplitude_from = amplitudes[real_index1];
        let amplitude_to = amplitudes[real_index2];
        let real_from = amplitude_from * normalized[real_index1];
        let real_to = amplitude_to * normalized[real_index2];
        let imag_from = amplitude_from * normalized[real_index1 + 1];
        let imag_to = amplitude_to * normalized[real_index2 + 1];

        let real_index = 2 * i as usize;
        wave_start[real_index] = shift * utils::interpolate(real_from, real_to, t);
        wave_start[real_index + 1] = shift * utils::interpolate(imag_from, imag_to, t);
    }
    let first_zero = (2 * (last_index + 1)).max(0) as usize;
    for value in wave_start.iter_mut().take(WAVEFORM_SIZE).skip(first_zero) {
        *value = 0.0;
    }

    transform_and_wrap_buffer(transform, dest);
}

pub fn harmonic_scale_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    shift: f32,
    last_harmonic: i32,
    _data_buffer: &[f32],
) {
    let wave_start = &mut dest[WAVE_START..];
    for value in &mut wave_start[..2 * WAVEFORM_SIZE] {
        *value = 0.0;
    }
    let harmonics =
        (NUM_HARMONICS as i32).min(float_to_i32((last_harmonic - 1) as f32 / shift + 1.0));

    let amplitudes = &data.frequency_amplitudes[wavetable_index];
    let normalized = &data.normalized_frequencies[wavetable_index];

    let dc_amplitude = amplitudes[0];
    wave_start[0] = dc_amplitude * normalized[0];
    wave_start[1] = dc_amplitude * normalized[1];

    for i in 1..=harmonics.max(0) as usize {
        let shifted_index = std_max(1.0, (i - 1) as f32 * shift + 1.0);
        let dest_index = float_to_i32(shifted_index);

        let t = shifted_index - dest_index as f32;
        let real_amount = normalized[2 * i];
        let imag_amount = normalized[2 * i + 1];
        let amplitude = amplitudes[2 * i];
        let amplitude1 = (1.0 - t) * amplitude;
        let amplitude2 = t * amplitude;

        let real_index1 = 2 * dest_index as usize;
        let imaginary_index1 = real_index1 + 1;
        wave_start[real_index1] += amplitude1 * real_amount;
        wave_start[imaginary_index1] += amplitude1 * imag_amount;

        let real_index2 = imaginary_index1 + 1;
        let imaginary_index2 = real_index2 + 1;
        wave_start[real_index2] += amplitude2 * real_amount;
        wave_start[imaginary_index2] += amplitude2 * imag_amount;
    }

    transform_and_wrap_buffer(transform, dest);
}

pub fn inharmonic_scale_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    mult: f32,
    last_harmonic: i32,
    _data_buffer: &[f32],
) {
    // `poly_float* poly_data_start = dest + 2 + kMaxPolyIndex`: an index table
    // inside the same buffer, past the spectrum.
    let data_start = (2 + MAX_POLY_INDEX) * 4;

    let offset = PolyFloat::new(0.0, 2.0, 1.0, 3.0);
    for i in 0..=MAX_POLY_INDEX / 2 {
        let index = offset + (i * 4) as f32;
        let octave = futils::log2(index);
        let power = octave * (1.0 / (FREQUENCY_BINS as f32 - 1.0));
        let shift = futils::pow(PolyFloat::splat(mult), power);
        let shifted_index = poly_utils::max(PolyFloat::splat(1.0), shift * (index - 1.0) + 1.0);
        store(&mut dest[data_start..], 2 * i, shifted_index);
        store(
            &mut dest[data_start..],
            2 * i + 1,
            poly_utils::swap_stereo(shifted_index),
        );
    }

    let amplitudes = &data.frequency_amplitudes[wavetable_index];
    let normalized = &data.normalized_frequencies[wavetable_index];
    for value in &mut dest[WAVE_START..WAVE_START + WAVEFORM_SIZE] {
        *value = 0.0;
    }

    let dc_amplitude = amplitudes[0];
    dest[WAVE_START] = dc_amplitude * normalized[0];
    dest[WAVE_START + 1] = dc_amplitude * normalized[1];

    for processed_index in 1..=NUM_HARMONICS {
        let index = 2 * processed_index;
        let shifted_index = dest[data_start + index];
        let dest_index = float_to_i32(shifted_index);
        if dest_index > 2 * last_harmonic {
            break;
        }

        let t = shifted_index - dest_index as f32;
        let amplitude = amplitudes[index];
        let real = normalized[index];
        let imag = normalized[index + 1];

        let real_index = WAVE_START + 2 * dest_index as usize;
        let value1 = (1.0 - t) * amplitude;
        dest[real_index] += value1 * real;
        dest[real_index + 1] += value1 * imag;
        let value2 = t * amplitude;
        dest[real_index + 2] += value2 * real;
        dest[real_index + 3] += value2 * imag;
    }

    transform_and_wrap_buffer(transform, dest);
}

pub fn random_amplitude_morph(
    data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
    shift: f32,
    last_harmonic: i32,
    data_buffer: &[f32],
) {
    let frequency_amplitudes = &data.frequency_amplitudes[wavetable_index];
    let normalized_frequencies = &data.normalized_frequencies[wavetable_index];
    let wave_start = &mut dest[WAVE_START..];
    let last_index = (2 * last_harmonic / 4) as usize;
    let index = float_to_i32(shift).min(RANDOM_AMPLITUDE_STAGES as i32 - 2);
    let t = shift - index as f32;
    let scale = PolyFloat::splat(shift);
    let center = PolyFloat::splat(1.0) - scale;
    let mult = PolyFloat::splat(1.0 + shift);

    let buffer1 = &data_buffer[4 * (index as usize * NUM_HARMONICS / 4)..];
    let buffer2 = &data_buffer[4 * ((index as usize + 1) * NUM_HARMONICS / 4)..];

    for i in 0..=last_index {
        let mut random_value1 = load(buffer1, i) & constants::left_mask();
        random_value1 = random_value1 + poly_utils::swap_stereo(random_value1);
        let mut random_value2 = load(buffer2, i) & constants::left_mask();
        random_value2 = random_value2 + poly_utils::swap_stereo(random_value2);
        let random1 = mult * poly_utils::max(center - scale * random_value1, PolyFloat::zero());
        let random2 = mult * poly_utils::max(center - scale * random_value2, PolyFloat::zero());
        let amplitude = poly_utils::min(
            poly_utils::interpolate(random1, random2, t) * load(frequency_amplitudes, i),
            PolyFloat::splat(1024.0),
        );

        store(wave_start, i, amplitude * load(normalized_frequencies, i));
    }
    for i in last_index + 1..=MAX_POLY_INDEX {
        store(wave_start, i, PolyFloat::zero());
    }

    transform_and_wrap_buffer(transform, dest);
}

const _: () = assert!(NUM_HARMONICS == WAVETABLE_NUM_HARMONICS);
