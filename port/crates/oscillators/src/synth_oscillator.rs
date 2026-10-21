// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::SynthOscillator`: the wavetable oscillator with unison, spectral
//! morphs, phase distortion and FM/RM, plus `RandomValues`.
//!
//! Ports `vital/src/synthesis/producers/synth_oscillator.{h,cpp}`.
//!
//! # Structure
//!
//! The upstream function templates (`processOscillators<phaseDistort,
//! window>` and the `processDetuned`/`processCenter` families) are generic
//! functions over the same phase-distortion and window functions. Wave
//! buffers are upstream `const mono_float*` pointers into the oscillator's
//! two banks of Fourier buffers or the shared zero waveform; here they are
//! [`WaveBuffer`] handles into the same storage, compared and copied the way
//! upstream compares and copies the pointers.
//!
//! # Per-target sites
//!
//! - A1/A1b/A1c: every `utils::toInt` goes through [`poly_utils::to_int`]
//!   (wave-frame selection, phase accumulation, phase distortion).
//! - A2: the random start phase (`synth_oscillator.cpp:583`) converts a
//!   possibly negative float to `uint32_t` through [`float_to_u32`].
//! - A3: scalar `(int)` casts of floats use [`float_to_i32`], which matches
//!   each reference outside the `int` range too.
//! - A4–A8 are inside `sas-simd` (`sum`, `max`/`min`, `notEqual`,
//!   `anyMask`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use sas_common::random::RandomGenerator;
use sas_dsp_core::{constants, Output, Processor, ProcessorImpl, ProcessorRef};
use sas_simd::common::{MAX_BUFFER_SIZE, MAX_OVERSAMPLE, NOTES_PER_OCTAVE};
use sas_simd::conversions::{float_to_i32, float_to_u32};
use sas_simd::poly_utils::{self as utils, to_float, to_int};
use sas_simd::{futils, Matrix, PolyFloat, PolyInt, PolyMask};
use sas_wavetable::wave_frame::{WAVEFORM_BITS, WAVEFORM_SIZE};
use sas_wavetable::wavetable::{FREQUENCY_BINS, NUM_HARMONICS, ZERO_WAVEFORM};
use sas_wavetable::{FourierTransform, Wavetable, WavetableData, NUM_OSCILLATOR_WAVE_FRAMES};

use crate::spectral_morph::{self as morph, MorphFn, SPECTRAL_BUFFER_FLOATS};

/// The wavetable an oscillator reads, shared with whoever renders it
/// (`OscillatorModule` owns it upstream; the oscillator holds a pointer).
pub type SharedWavetable = Rc<RefCell<Wavetable>>;

// Input indices.
pub const WAVE_FRAME: usize = 0;
pub const MIDI_NOTE: usize = 1;
pub const MIDI_TRACK: usize = 2;
pub const SMOOTHLY_INTERPOLATE: usize = 3;
pub const TRANSPOSE: usize = 4;
pub const TRANSPOSE_QUANTIZE: usize = 5;
pub const TUNE: usize = 6;
pub const AMPLITUDE: usize = 7;
pub const PAN: usize = 8;
pub const UNISON_VOICES: usize = 9;
pub const UNISON_DETUNE: usize = 10;
pub const PHASE: usize = 11;
pub const DISTORTION_PHASE: usize = 12;
pub const RANDOM_PHASE: usize = 13;
pub const BLEND: usize = 14;
pub const STEREO_SPREAD: usize = 15;
pub const STACK_STYLE: usize = 16;
pub const DETUNE_POWER: usize = 17;
pub const DETUNE_RANGE: usize = 18;
pub const UNISON_FRAME_SPREAD: usize = 19;
pub const UNISON_DISTORTION_SPREAD: usize = 20;
pub const UNISON_SPECTRAL_MORPH_SPREAD: usize = 21;
pub const SPECTRAL_MORPH_TYPE: usize = 22;
pub const SPECTRAL_MORPH_AMOUNT: usize = 23;
pub const SPECTRAL_UNISON: usize = 24;
pub const DISTORTION_TYPE: usize = 25;
pub const DISTORTION_AMOUNT: usize = 26;
pub const ACTIVE_VOICES: usize = 27;
pub const RESET: usize = 28;
pub const RETRIGGER: usize = 29;
pub const NUM_INPUTS: usize = 30;

// Output indices.
pub const RAW: usize = 0;
pub const LEVELLED: usize = 1;
pub const NUM_OUTPUTS: usize = 2;

/// `SynthOscillator::SpectralMorph`.
pub mod spectral_morph_type {
    pub const NO_SPECTRAL_MORPH: i32 = 0;
    pub const VOCODE: i32 = 1;
    pub const FORM_SCALE: i32 = 2;
    pub const HARMONIC_SCALE: i32 = 3;
    pub const INHARMONIC_SCALE: i32 = 4;
    pub const SMEAR: i32 = 5;
    pub const RANDOM_AMPLITUDES: i32 = 6;
    pub const LOW_PASS: i32 = 7;
    pub const HIGH_PASS: i32 = 8;
    pub const PHASE_DISPERSE: i32 = 9;
    pub const SHEPARD_TONE: i32 = 10;
    pub const SKEW: i32 = 11;
    pub const NUM_SPECTRAL_MORPH_TYPES: i32 = 12;
}

/// `SynthOscillator::DistortionType`.
pub mod distortion_type {
    pub const NONE: i32 = 0;
    pub const SYNC: i32 = 1;
    pub const FORMANT: i32 = 2;
    pub const QUANTIZE: i32 = 3;
    pub const BEND: i32 = 4;
    pub const SQUEEZE: i32 = 5;
    pub const PULSE_WIDTH: i32 = 6;
    pub const FM_OSCILLATOR_A: i32 = 7;
    pub const FM_OSCILLATOR_B: i32 = 8;
    pub const FM_SAMPLE: i32 = 9;
    pub const RM_OSCILLATOR_A: i32 = 10;
    pub const RM_OSCILLATOR_B: i32 = 11;
    pub const RM_SAMPLE: i32 = 12;
    pub const NUM_DISTORTION_TYPES: i32 = 13;
}

/// `SynthOscillator::UnisonStackType`.
pub mod unison_stack_type {
    pub const NORMAL: i32 = 0;
    pub const CENTER_DROP_OCTAVE: i32 = 1;
    pub const CENTER_DROP_OCTAVE2: i32 = 2;
    pub const OCTAVE: i32 = 3;
    pub const OCTAVE2: i32 = 4;
    pub const POWER_CHORD: i32 = 5;
    pub const POWER_CHORD2: i32 = 6;
    pub const MAJOR_CHORD: i32 = 7;
    pub const MINOR_CHORD: i32 = 8;
    pub const HARMONIC_SERIES: i32 = 9;
    pub const ODD_HARMONIC_SERIES: i32 = 10;
    pub const NUM_UNISON_STACK_TYPES: i32 = 11;
}

use distortion_type as dt;
use spectral_morph_type as sm;

pub fn is_first_modulation(kind: i32) -> bool {
    kind == dt::FM_OSCILLATOR_A || kind == dt::RM_OSCILLATOR_A
}

pub fn is_second_modulation(kind: i32) -> bool {
    kind == dt::FM_OSCILLATOR_B || kind == dt::RM_OSCILLATOR_B
}

pub const MAX_UNISON: i32 = 16;
pub const POLY_PHASE_PER_VOICE: usize = MAX_UNISON as usize / 4;
pub const NUM_POLY_PHASE: usize = MAX_UNISON as usize / 2;
pub const NUM_BUFFERS: usize = NUM_POLY_PHASE * 4;

const NUM_VOICES_PER_PROCESS: usize = 4 / 2;
const INTERMEDIATE_BITS: i32 = 32 - WAVEFORM_BITS as i32;
const HALF_PHASE: u32 = 0x8000_0000;
const PHASE_MULT: f32 = 4_294_967_296.0;
const INV_PHASE_MULT: f32 = 1.0 / 4_294_967_296.0;
const INTERMEDIATE_MULT: f32 = (1 << INTERMEDIATE_BITS) as f32;
const INTERMEDIATE_MASK: u32 = (1 << INTERMEDIATE_BITS) - 1;
const DISTORT_BITS: f32 = 32.0;
const MAX_QUANTIZE: f32 = 0.85;
const MAX_SQUEEZE_PERCENT: f32 = 0.95;
const CENTER_LOW_AMPLITUDE: f32 = 0.4;
const DETUNED_HIGH_AMPLITUDE: f32 = 0.6;
const WAVETABLE_FADE_TIME: f32 = 0.007;
const MAX_SYNC_POWER: i32 = 4;
const MAX_SYNC: i32 = 1 << MAX_SYNC_POWER;
const FIFTH_MULT: f32 = 1.498_307_076_88;
const MAJOR_THIRD_MULT: f32 = 1.259_921_049_89;
const MINOR_THIRD_MULT: f32 = 1.189_207_115;
const NO_MIDI_TRACK_DEFAULT: f32 = 48.0;
const FM_PHASE_MULT: f32 = PHASE_MULT / 8.0;
const MAX_FM_MODULATION: u32 = 48;
/// `INT_MAX` converted to `float`.
const INT_MAX_F: f32 = i32::MAX as f32;

/// `SynthOscillator::kStackMultipliers`.
pub const STACK_MULTIPLIERS: [[f32; NUM_POLY_PHASE];
    unison_stack_type::NUM_UNISON_STACK_TYPES as usize] = [
    [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    [0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    [0.25, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    [1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0],
    [1.0, 2.0, 3.0, 1.0, 2.0, 3.0, 1.0, 2.0],
    [1.0, FIFTH_MULT, 2.0, 1.0, FIFTH_MULT, 2.0, 1.0, FIFTH_MULT],
    [
        1.0,
        FIFTH_MULT,
        2.0,
        2.0 * FIFTH_MULT,
        4.0,
        1.0,
        FIFTH_MULT,
        2.0,
    ],
    [
        1.0,
        MAJOR_THIRD_MULT,
        FIFTH_MULT,
        2.0,
        1.0,
        MAJOR_THIRD_MULT,
        FIFTH_MULT,
        2.0,
    ],
    [
        1.0,
        MINOR_THIRD_MULT,
        FIFTH_MULT,
        2.0,
        1.0,
        MINOR_THIRD_MULT,
        FIFTH_MULT,
        2.0,
    ],
    [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
    [1.0, 3.0, 5.0, 7.0, 9.0, 11.0, 13.0, 15.0],
];

// ----- RandomValues -----

/// `vital::RandomValues`: a process-wide table of random amplitudes for the
/// random-amplitude morph, generated once with seed `0x4`.
pub struct RandomValues;

static RANDOM_VALUES: OnceLock<Vec<f32>> = OnceLock::new();
static RANDOM_VALUES_SEEDED: AtomicBool = AtomicBool::new(false);

impl RandomValues {
    pub const SEED: i32 = 0x4;

    /// `RandomValues::instance()`. Upstream's function-local static is
    /// constructed by the first `SynthOscillator` of the process, and its
    /// `RandomGenerator` takes a value from the global seed counter before
    /// reseeding; that happens here once per process too (see
    /// [`RandomValues::reset_process_state`]).
    pub fn instance() -> &'static [f32] {
        if !RANDOM_VALUES_SEEDED.swap(true, Ordering::Relaxed) {
            sas_common::random::take_next_seed();
        }
        RANDOM_VALUES.get_or_init(|| {
            let size = (morph::RANDOM_AMPLITUDE_STAGES + 1) * (NUM_HARMONICS + 1) / 4;
            let mut generator = RandomGenerator::new(-1.0, 1.0, Self::SEED);
            let mut data = Vec::with_capacity(4 * size);
            for _ in 0..size {
                data.extend_from_slice(&generator.poly_next());
            }
            data
        })
    }

    /// Makes the next [`RandomValues::instance`] call behave like the first
    /// one in a fresh process (it takes a seed again). The reference renders
    /// every capture in its own process; a test that builds several engines
    /// in one process calls this before each, after setting the seed counter.
    pub fn reset_process_state() {
        RANDOM_VALUES_SEEDED.store(false, Ordering::Relaxed);
    }
}

// ----- wave buffers -----

/// A wave buffer pointer (`const mono_float*`): the zero waveform, or a
/// Fourier buffer of bank 1 or 2 (`fourier_frames1_`/`fourier_frames2_`)
/// read from float 3, so index 0 reads the sample before the start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveBuffer {
    Null,
    Frames1(usize),
    Frames2(usize),
}

/// `SynthOscillator::VoiceBlock` (the buffer pointers that do not change
/// between chunks are passed to the process functions separately).
#[derive(Clone, Debug)]
pub struct VoiceBlock {
    pub start_sample: i32,
    pub end_sample: i32,
    pub total_samples: i32,
    pub phase: PolyInt,
    pub phase_inc_mult: PolyFloat,
    pub from_phase_inc_mult: PolyFloat,
    pub shepard_double_mask: PolyMask,
    pub shepard_half_mask: PolyMask,
    pub distortion_phase: PolyInt,
    pub last_distortion_phase: PolyInt,
    pub distortion: PolyFloat,
    pub last_distortion: PolyFloat,
    pub num_buffer_samples: i32,
    pub current_buffer_sample: PolyInt,
    pub smoothing_enabled: bool,
    pub spectral_morph: i32,
    pub from_buffers: [WaveBuffer; 4],
    pub to_buffers: [WaveBuffer; 4],
}

impl Default for VoiceBlock {
    fn default() -> Self {
        Self {
            start_sample: 0,
            end_sample: 0,
            total_samples: 0,
            phase: PolyInt::zero(),
            phase_inc_mult: PolyFloat::zero(),
            from_phase_inc_mult: PolyFloat::zero(),
            shepard_double_mask: PolyMask::zero(),
            shepard_half_mask: PolyMask::zero(),
            distortion_phase: PolyInt::zero(),
            last_distortion_phase: PolyInt::zero(),
            distortion: PolyFloat::zero(),
            last_distortion: PolyFloat::zero(),
            num_buffer_samples: 0,
            current_buffer_sample: PolyInt::zero(),
            smoothing_enabled: false,
            spectral_morph: sm::NO_SPECTRAL_MORPH,
            from_buffers: [WaveBuffer::Null; 4],
            to_buffers: [WaveBuffer::Null; 4],
        }
    }
}

impl VoiceBlock {
    pub fn is_static(&self) -> bool {
        self.from_buffers == self.to_buffers
    }
}

/// What the process functions read besides the voice block.
struct Sources<'a> {
    from: [&'a [f32]; 4],
    to: [&'a [f32]; 4],
    phase_inc: &'a [PolyFloat],
    phase: &'a [PolyInt],
    modulation: &'a [Cell<PolyFloat>],
}

// ----- phase distortion and window functions -----

type PhaseDistortFn = fn(PolyInt, PolyFloat, PolyInt, &[Cell<PolyFloat>], usize) -> PolyInt;
type WindowFn = fn(PolyInt, PolyInt, PolyFloat, &[Cell<PolyFloat>], usize) -> PolyFloat;

#[inline(always)]
fn pass_through_phase(
    phase: PolyInt,
    _: PolyFloat,
    _: PolyInt,
    _: &[Cell<PolyFloat>],
    _: usize,
) -> PolyInt {
    phase
}

#[inline(always)]
fn quantize_phase(
    phase: PolyInt,
    distortion: PolyFloat,
    distortion_phase: PolyInt,
    _: &[Cell<PolyFloat>],
    _: usize,
) -> PolyInt {
    let normal_phase = to_float(phase) * distortion * INV_PHASE_MULT;
    let adjustment = to_float(distortion_phase) * INV_PHASE_MULT;
    let floored_phase = utils::trunc(normal_phase + adjustment) - adjustment;
    to_int((floored_phase / distortion) * PHASE_MULT) - distortion_phase
}

#[inline(always)]
fn bend_phase(
    phase: PolyInt,
    distortion: PolyFloat,
    distortion_phase: PolyInt,
    _: &[Cell<PolyFloat>],
    _: usize,
) -> PolyInt {
    let float_phase = to_float(phase - distortion_phase) * (1.0 / PHASE_MULT) + 0.5;

    let distortion_offset = (distortion - distortion * distortion) * 2.0;
    let float_phase2 = float_phase * float_phase;
    let float_phase3 = float_phase * float_phase2;

    let distortion_scale = distortion * 3.0;
    let middle_mult1 = distortion_scale + distortion_offset;
    let middle_mult2 = distortion_scale - distortion_offset;
    let middle1 = middle_mult1 * (float_phase2 - float_phase3);
    let middle2 = middle_mult2 * (float_phase - float_phase2 * 2.0 + float_phase3);
    let new_phase = float_phase3 + middle1 + middle2;
    to_int((new_phase - 0.5) * PHASE_MULT)
}

#[inline(always)]
fn squeeze_phase(
    phase: PolyInt,
    distortion: PolyFloat,
    distortion_phase: PolyInt,
    _: &[Cell<PolyFloat>],
    _: usize,
) -> PolyInt {
    let center_phase = PolyFloat::splat(PHASE_MULT / 4.0);
    let max_phase = PolyFloat::splat(PHASE_MULT / 2.0);
    let mut float_phase = to_float(phase - distortion_phase);
    let positive_mask = PolyFloat::greater_than(float_phase, PolyFloat::zero());
    float_phase = PolyFloat::abs(float_phase);

    let pivot = distortion * center_phase;
    let right_half_mask = PolyFloat::greater_than(float_phase, pivot);

    let left_phase = float_phase / distortion;
    let right_phase = max_phase - (max_phase - float_phase) / (PolyFloat::splat(2.0) - distortion);
    let mut new_phase = utils::mask_load(left_phase, right_phase, right_half_mask);
    new_phase = utils::mask_load(-new_phase, new_phase, positive_mask);
    to_int(new_phase)
}

#[inline(always)]
fn sync_phase(
    phase: PolyInt,
    distortion: PolyFloat,
    _: PolyInt,
    _: &[Cell<PolyFloat>],
    _: usize,
) -> PolyInt {
    let float_val = to_float(phase + HALF_PHASE) * distortion;
    to_int(float_val) * MAX_SYNC as u32 + HALF_PHASE
}

#[inline(always)]
fn pulse_width_phase(
    phase: PolyInt,
    distortion: PolyFloat,
    _: PolyInt,
    _: &[Cell<PolyFloat>],
    _: usize,
) -> PolyInt {
    let distorted_phase = to_float(phase) * distortion;
    let clamped_phase = utils::clamp(distorted_phase, i32::MIN as f32, i32::MAX as f32);
    to_int(clamped_phase)
}

#[inline(always)]
fn fm_phase(
    phase: PolyInt,
    distortion: PolyFloat,
    _: PolyInt,
    modulation: &[Cell<PolyFloat>],
    i: usize,
) -> PolyInt {
    let phase_offset = modulation[i].get() * distortion;
    phase + to_int(phase_offset * FM_PHASE_MULT) * MAX_FM_MODULATION
}

#[inline(always)]
fn fm_phase_left(
    phase: PolyInt,
    distortion: PolyFloat,
    _: PolyInt,
    modulation: &[Cell<PolyFloat>],
    i: usize,
) -> PolyInt {
    let mut modulation_value = modulation[i].get() & constants::first_mask();
    modulation_value += utils::swap_voices(modulation_value);
    let phase_offset = modulation_value * distortion;
    phase + to_int(phase_offset * FM_PHASE_MULT) * MAX_FM_MODULATION
}

#[inline(always)]
fn fm_phase_right(
    phase: PolyInt,
    distortion: PolyFloat,
    _: PolyInt,
    modulation: &[Cell<PolyFloat>],
    i: usize,
) -> PolyInt {
    let mut modulation_value = modulation[i].get() & constants::second_mask();
    modulation_value += utils::swap_voices(modulation_value);
    let phase_offset = modulation_value * distortion;
    phase + to_int(phase_offset * FM_PHASE_MULT) * MAX_FM_MODULATION
}

#[inline(always)]
fn pass_through_window(
    _: PolyInt,
    _: PolyInt,
    _: PolyFloat,
    _: &[Cell<PolyFloat>],
    _: usize,
) -> PolyFloat {
    PolyFloat::splat(1.0)
}

#[inline(always)]
fn pulse_width_window(
    _: PolyInt,
    distorted_phase: PolyInt,
    _: PolyFloat,
    _: &[Cell<PolyFloat>],
    _: usize,
) -> PolyFloat {
    PolyFloat::splat(1.0) & !PolyInt::equal(distorted_phase, PolyInt::splat(HALF_PHASE))
}

#[inline(always)]
fn half_sin_window(
    phase: PolyInt,
    _: PolyInt,
    _: PolyFloat,
    _: &[Cell<PolyFloat>],
    _: usize,
) -> PolyFloat {
    let normal_phase = to_float(phase + i32::MAX as u32) * (INV_PHASE_MULT / 2.0);
    futils::sin(normal_phase + 0.25)
}

#[inline(always)]
fn rm_window(
    _: PolyInt,
    _: PolyInt,
    distortion: PolyFloat,
    modulation: &[Cell<PolyFloat>],
    i: usize,
) -> PolyFloat {
    utils::interpolate(1.0, modulation[i].get(), distortion)
}

#[inline(always)]
fn rm_window_left(
    _: PolyInt,
    _: PolyInt,
    distortion: PolyFloat,
    modulation: &[Cell<PolyFloat>],
    i: usize,
) -> PolyFloat {
    let mut modulation_value = modulation[i].get() & constants::first_mask();
    modulation_value += utils::swap_voices(modulation_value);
    utils::interpolate(1.0, modulation_value, distortion)
}

#[inline(always)]
fn rm_window_right(
    _: PolyInt,
    _: PolyInt,
    distortion: PolyFloat,
    modulation: &[Cell<PolyFloat>],
    i: usize,
) -> PolyFloat {
    let mut modulation_value = modulation[i].get() & constants::second_mask();
    modulation_value += utils::swap_voices(modulation_value);
    utils::interpolate(1.0, modulation_value, distortion)
}

// ----- transpose snapping -----

type SnapFn = fn(PolyFloat, PolyFloat, &[f32]) -> PolyFloat;

#[inline(always)]
fn no_transpose_snap(midi: PolyFloat, transpose: PolyFloat, _: &[f32]) -> PolyFloat {
    midi + transpose
}

#[inline(always)]
fn snap_lookup(int_snap: PolyInt, snap_buffer: &[f32]) -> PolyFloat {
    let mut result = PolyFloat::zero();
    for i in 0..4 {
        result.set(i, snap_buffer[int_snap.get(i) as i32 as usize]);
    }
    result
}

#[inline(always)]
fn local_transpose_snap(midi: PolyFloat, transpose: PolyFloat, snap_buffer: &[f32]) -> PolyFloat {
    const SCALE_DOWN: f32 = 1.0 / NOTES_PER_OCTAVE as f32;
    const SCALE_UP: f32 = NOTES_PER_OCTAVE as f32;

    let note_offset = utils::modulo(transpose * SCALE_DOWN) * SCALE_UP;
    let octave_snap = transpose - note_offset;
    let int_snap = utils::round_to_int(note_offset);
    let result = snap_lookup(int_snap, snap_buffer);
    midi + octave_snap + result
}

#[inline(always)]
fn global_transpose_snap(midi: PolyFloat, transpose: PolyFloat, snap_buffer: &[f32]) -> PolyFloat {
    const SCALE_DOWN: f32 = 1.0 / NOTES_PER_OCTAVE as f32;
    const SCALE_UP: f32 = NOTES_PER_OCTAVE as f32;

    let total = midi + transpose;
    let note_offset = utils::modulo(total * SCALE_DOWN) * SCALE_UP;
    let octave_snap = total - note_offset;
    let int_snap = utils::round_to_int(note_offset);
    let result = snap_lookup(int_snap, snap_buffer);
    octave_snap + result
}

// ----- buffer interpolation -----

#[inline(always)]
fn get_interpolation_values(indices: PolyInt) -> PolyFloat {
    to_float(indices & INTERMEDIATE_MASK) * (1.0 / INTERMEDIATE_MULT)
}

#[inline(always)]
fn linearly_interpolate_buffer(buffer: &[f32], indices: PolyInt) -> PolyFloat {
    let start_indices = utils::shift_right::<INTERMEDIATE_BITS>(indices);
    let t = get_interpolation_values(indices);
    let interpolation_matrix = utils::get_linear_interpolation_matrix(t);
    let mut value_matrix = utils::get_value_matrix(buffer, start_indices);
    value_matrix.transpose();
    interpolation_matrix.multiply_and_sum_rows(&value_matrix)
}

#[inline(always)]
fn interpolate_buffers(buffers: &[&[f32]; 4], indices: PolyInt) -> PolyFloat {
    let start_indices = utils::shift_right::<INTERMEDIATE_BITS>(indices);
    let t = get_interpolation_values(indices);
    let interpolation_matrix = utils::get_catmull_interpolation_matrix(t);
    let mut value_matrix = utils::get_value_matrix_separate(*buffers, start_indices);
    value_matrix.transpose();
    interpolation_matrix.multiply_and_sum_rows(&value_matrix)
}

#[inline(always)]
fn interpolate_multiple_buffers(
    buffers_from: &[&[f32]; 4],
    buffers_to: &[&[f32]; 4],
    indices: PolyInt,
    buffer_t: PolyFloat,
) -> PolyFloat {
    let start_indices = utils::shift_right::<INTERMEDIATE_BITS>(indices);
    let t = get_interpolation_values(indices);
    let interpolation_matrix = utils::get_catmull_interpolation_matrix(t);
    let mut value_matrix: Matrix = utils::get_value_matrix_separate(*buffers_from, start_indices);
    value_matrix.interpolate_rows(
        &utils::get_value_matrix_separate(*buffers_to, start_indices),
        buffer_t,
    );
    value_matrix.transpose();
    interpolation_matrix.multiply_and_sum_rows(&value_matrix)
}

/// `interpolateBuffers` (static buffers) or `interpolateMultipleBuffers`.
#[inline(always)]
fn interpolate_pair<const MULTIPLE: bool>(
    from: &[&[f32]; 4],
    to: &[&[f32]; 4],
    indices: PolyInt,
    buffer_t: PolyFloat,
) -> PolyFloat {
    if MULTIPLE {
        interpolate_multiple_buffers(from, to, indices, buffer_t)
    } else {
        interpolate_buffers(from, indices)
    }
}

#[inline(always)]
fn interpolate_shepard_buffers(
    buffers_from: &[&[f32]; 4],
    buffers_to: &[&[f32]; 4],
    indices: PolyInt,
    buffer_t: PolyFloat,
    double_mask: PolyMask,
    half_mask: PolyMask,
) -> PolyFloat {
    let mut adjusted_indices = utils::mask_load_int(indices, indices * 2, double_mask);
    adjusted_indices = utils::mask_load_int(
        adjusted_indices,
        utils::shift_right::<1>(adjusted_indices),
        half_mask,
    );
    let from = interpolate_buffers(buffers_from, adjusted_indices);
    let to = interpolate_buffers(buffers_to, indices);
    utils::interpolate(from, to, buffer_t)
}

// ----- per-chunk processing -----

/// The shared prologue of the process functions: buffer fade position,
/// phase-increment multiplier ramp, and the per-sample loop bounds.
struct ChunkRamp {
    start: usize,
    num_samples: usize,
    t_inc: PolyFloat,
    t: PolyFloat,
    sample_inc: f32,
    current_phase_inc_mult: PolyFloat,
    delta_phase_inc_mult: PolyFloat,
}

impl ChunkRamp {
    fn new(voice_block: &VoiceBlock) -> Self {
        let start = voice_block.start_sample;
        let t_inc = PolyFloat::splat(1.0 / voice_block.num_buffer_samples as f32);
        let t = to_float(voice_block.current_buffer_sample + 1) * t_inc;
        let sample_inc = 1.0 / voice_block.total_samples as f32;

        let mut current_phase_inc_mult = voice_block.from_phase_inc_mult;
        let end_phase_inc_mult = voice_block.phase_inc_mult;
        let delta_phase_inc_mult = (end_phase_inc_mult - current_phase_inc_mult) * sample_inc;
        current_phase_inc_mult += delta_phase_inc_mult * start as f32;
        Self {
            start: start as usize,
            num_samples: (voice_block.end_sample - start) as usize,
            t_inc,
            t,
            sample_inc,
            current_phase_inc_mult,
            delta_phase_inc_mult,
        }
    }
}

/// The distortion-phase and distortion-amount ramps.
struct DistortionRamp {
    current_dist_phase: PolyInt,
    delta_dist_phase: PolyInt,
    current_distortion: PolyFloat,
    distortion_inc: PolyFloat,
}

impl DistortionRamp {
    fn new(voice_block: &VoiceBlock, ramp: &ChunkRamp) -> Self {
        let start = ramp.start as u32;
        let mut current_dist_phase = voice_block.last_distortion_phase;
        let end_dist_phase = voice_block.distortion_phase;
        let delta_dist_phase =
            to_int(to_float(end_dist_phase - current_dist_phase) * ramp.sample_inc);
        current_dist_phase += delta_dist_phase * start;

        let mut current_distortion = voice_block.last_distortion;
        let distortion_inc = (voice_block.distortion - current_distortion) * ramp.sample_inc;
        current_distortion += distortion_inc * ramp.start as f32;
        Self {
            current_dist_phase,
            delta_dist_phase,
            current_distortion,
            distortion_inc,
        }
    }
}

fn process_detuned_shepard(
    voice_block: &VoiceBlock,
    sources: &Sources,
    audio_out: &[Cell<PolyFloat>],
) -> PolyInt {
    let mut ramp = ChunkRamp::new(voice_block);
    let mut phase = voice_block.phase;
    let double_mask = voice_block.shepard_double_mask;
    let half_mask = voice_block.shepard_half_mask;

    let phase_inc_buffer = &sources.phase_inc[ramp.start..];
    let phase_buffer = &sources.phase[ramp.start..];
    for i in 0..ramp.num_samples {
        ramp.current_phase_inc_mult += ramp.delta_phase_inc_mult;
        phase += to_int(phase_inc_buffer[i] * ramp.current_phase_inc_mult);
        let adjusted_phase = phase + phase_buffer[i];
        let value = interpolate_shepard_buffers(
            &sources.from,
            &sources.to,
            adjusted_phase,
            ramp.t,
            double_mask,
            half_mask,
        );
        audio_out[i].set(audio_out[i].get() + value);
        ramp.t += ramp.t_inc;
    }
    phase
}

fn process_detuned_with<const MULTIPLE: bool>(
    voice_block: &VoiceBlock,
    sources: &Sources,
    audio_out: &[Cell<PolyFloat>],
    phase_distort: PhaseDistortFn,
    window: WindowFn,
) -> PolyInt {
    let mut ramp = ChunkRamp::new(voice_block);
    let mut phase = voice_block.phase;
    let mut dist = DistortionRamp::new(voice_block, &ramp);

    let modulation_buffer = sources.modulation.get(ramp.start..).unwrap_or(&[]);
    let phase_inc_buffer = &sources.phase_inc[ramp.start..];
    let phase_buffer = &sources.phase[ramp.start..];
    for i in 0..ramp.num_samples {
        ramp.current_phase_inc_mult += ramp.delta_phase_inc_mult;
        phase += to_int(phase_inc_buffer[i] * ramp.current_phase_inc_mult);
        let adjusted_phase = phase + phase_buffer[i];
        dist.current_distortion += dist.distortion_inc;
        dist.current_dist_phase += dist.delta_dist_phase;
        let distorted_phase = phase_distort(
            adjusted_phase,
            dist.current_distortion,
            dist.current_dist_phase,
            modulation_buffer,
            i,
        );
        let result = interpolate_pair::<MULTIPLE>(
            &sources.from,
            &sources.to,
            distorted_phase + dist.current_dist_phase,
            ramp.t,
        );
        let mult = window(
            adjusted_phase,
            distorted_phase,
            dist.current_distortion,
            modulation_buffer,
            i,
        );
        audio_out[i].set(audio_out[i].get() + mult * result);
        ramp.t += ramp.t_inc;
    }
    phase
}

fn process_detuned(
    voice_block: &VoiceBlock,
    sources: &Sources,
    audio_out: &[Cell<PolyFloat>],
    phase_distort: PhaseDistortFn,
    window: WindowFn,
) -> PolyInt {
    if voice_block.is_static() {
        return process_detuned_with::<false>(
            voice_block,
            sources,
            audio_out,
            phase_distort,
            window,
        );
    }
    if voice_block.shepard_double_mask.any_mask() != 0
        || voice_block.shepard_half_mask.any_mask() != 0
    {
        return process_detuned_shepard(voice_block, sources, audio_out);
    }
    process_detuned_with::<true>(voice_block, sources, audio_out, phase_distort, window)
}

#[derive(Clone, Copy)]
struct AmplitudeRamp {
    current_center: PolyFloat,
    delta_center: PolyFloat,
    current_detuned: PolyFloat,
    delta_detuned: PolyFloat,
}

fn process_center_shepard(
    voice_block: &VoiceBlock,
    sources: &Sources,
    audio_out: &[Cell<PolyFloat>],
    mut amplitude: AmplitudeRamp,
) -> PolyInt {
    let mut ramp = ChunkRamp::new(voice_block);
    let mut phase = voice_block.phase;
    let double_mask = voice_block.shepard_double_mask;
    let half_mask = voice_block.shepard_half_mask;

    let phase_inc_buffer = &sources.phase_inc[ramp.start..];
    let phase_buffer = &sources.phase[ramp.start..];
    for i in 0..ramp.num_samples {
        ramp.current_phase_inc_mult += ramp.delta_phase_inc_mult;
        phase += to_int(phase_inc_buffer[i] * ramp.current_phase_inc_mult);
        let adjusted_phase = phase + phase_buffer[i];

        amplitude.current_center += amplitude.delta_center;
        amplitude.current_detuned += amplitude.delta_detuned;
        let read = interpolate_shepard_buffers(
            &sources.from,
            &sources.to,
            adjusted_phase,
            ramp.t,
            double_mask,
            half_mask,
        );
        audio_out[i]
            .set(amplitude.current_center * read + amplitude.current_detuned * audio_out[i].get());
        ramp.t += ramp.t_inc;
    }
    phase
}

fn process_center_with<const MULTIPLE: bool>(
    voice_block: &VoiceBlock,
    sources: &Sources,
    audio_out: &[Cell<PolyFloat>],
    mut amplitude: AmplitudeRamp,
    phase_distort: PhaseDistortFn,
    window: WindowFn,
) -> PolyInt {
    let mut ramp = ChunkRamp::new(voice_block);
    let mut phase = voice_block.phase;
    let mut dist = DistortionRamp::new(voice_block, &ramp);

    let modulation_buffer = sources.modulation.get(ramp.start..).unwrap_or(&[]);
    let phase_inc_buffer = &sources.phase_inc[ramp.start..];
    let phase_buffer = &sources.phase[ramp.start..];
    for i in 0..ramp.num_samples {
        ramp.current_phase_inc_mult += ramp.delta_phase_inc_mult;
        phase += to_int(phase_inc_buffer[i] * ramp.current_phase_inc_mult);
        let adjusted_phase = phase + phase_buffer[i];
        dist.current_distortion += dist.distortion_inc;
        dist.current_dist_phase += dist.delta_dist_phase;
        amplitude.current_center += amplitude.delta_center;
        amplitude.current_detuned += amplitude.delta_detuned;
        let distorted_phase = phase_distort(
            adjusted_phase,
            dist.current_distortion,
            dist.current_dist_phase,
            modulation_buffer,
            i,
        );
        let mult = window(
            adjusted_phase,
            distorted_phase,
            dist.current_distortion,
            modulation_buffer,
            i,
        );
        let read = mult
            * interpolate_pair::<MULTIPLE>(
                &sources.from,
                &sources.to,
                distorted_phase + dist.current_dist_phase,
                ramp.t,
            );
        let center_value = amplitude.current_center * read;
        audio_out[i].set(center_value + amplitude.current_detuned * audio_out[i].get());
        ramp.t += ramp.t_inc;
    }
    phase
}

fn process_center(
    voice_block: &VoiceBlock,
    sources: &Sources,
    audio_out: &[Cell<PolyFloat>],
    amplitude: AmplitudeRamp,
    phase_distort: PhaseDistortFn,
    window: WindowFn,
) -> PolyInt {
    if voice_block.is_static() {
        return process_center_with::<false>(
            voice_block,
            sources,
            audio_out,
            amplitude,
            phase_distort,
            window,
        );
    }
    if voice_block.shepard_double_mask.any_mask() != 0
        || voice_block.shepard_half_mask.any_mask() != 0
    {
        return process_center_shepard(voice_block, sources, audio_out, amplitude);
    }
    process_center_with::<true>(
        voice_block,
        sources,
        audio_out,
        amplitude,
        phase_distort,
        window,
    )
}

// ----- voice compaction -----

#[inline(always)]
fn compact_and_load_voice_float(values: &[PolyFloat], active_mask: PolyMask) -> PolyFloat {
    let one = values[0];
    let two = utils::swap_voices(values[1]);
    utils::mask_load(two, one, active_mask)
}

#[inline(always)]
fn compact_and_load_voice_int(values: &[PolyInt], active_mask: PolyMask) -> PolyInt {
    let one = values[0];
    let two = utils::swap_voices_int(values[1]);
    utils::mask_load_int(two, one, active_mask)
}

#[inline(always)]
fn expand_and_write_voice_int(values: &mut [PolyInt], value: PolyInt, active_mask: PolyMask) {
    let two = utils::swap_voices_int(value);
    values[0] = utils::mask_load_int(values[0], value, active_mask);
    values[1] = utils::mask_load_int(values[1], two, active_mask);
}

#[inline(always)]
fn compact_and_load_voice_buffers(
    dest: &mut [WaveBuffer; 4],
    values: &[WaveBuffer],
    active_mask: PolyMask,
) {
    let position1 = values;
    let position2 = &values[4..];
    let index = if active_mask.get(0) != 0 { 0 } else { 2 };
    dest[index] = position1[index];
    dest[index + 1] = position1[index + 1];
    dest[(index + 2) % 4] = position2[index];
    dest[(index + 3) % 4] = position2[index + 1];
}

fn set_power_distortion_values(values: &mut [PolyFloat], exponent: f32, spread: bool) {
    if spread {
        for value in values.iter_mut() {
            *value = futils::pow(PolyFloat::splat(2.0), (*value - 0.5) * 2.0 * exponent);
        }
    } else {
        let value = futils::pow(PolyFloat::splat(2.0), (values[0] - 0.5) * 2.0 * exponent);
        for v in values.iter_mut() {
            *v = value;
        }
    }
}

/// `SynthOscillator::setDistortionValues(type, values, num_values, spread)`.
pub fn set_distortion_values_for(kind: i32, values: &mut [PolyFloat], spread: bool) {
    match kind {
        dt::FM_OSCILLATOR_A | dt::FM_OSCILLATOR_B | dt::FM_SAMPLE => {
            for value in values.iter_mut() {
                *value *= *value;
            }
        }
        dt::SYNC | dt::FORMANT => {
            set_power_distortion_values(values, MAX_SYNC_POWER as f32, spread);
            for value in values.iter_mut() {
                *value *= 1.0 / MAX_SYNC as f32;
            }
        }
        dt::QUANTIZE => {
            let quantize = |value: PolyFloat| {
                let mut distortion = PolyFloat::splat(1.0) - value;
                distortion = distortion * (distortion * distortion);
                distortion *= MAX_QUANTIZE;
                futils::pow(PolyFloat::splat(2.0), distortion * DISTORT_BITS + 1.0)
            };
            if spread {
                for value in values.iter_mut() {
                    *value = quantize(*value);
                }
            } else {
                let distortion = quantize(values[0]);
                for value in values.iter_mut() {
                    *value = distortion;
                }
            }
        }
        dt::SQUEEZE => {
            for value in values.iter_mut() {
                *value = *value * 2.0 * MAX_SQUEEZE_PERCENT + (1.0 - MAX_SQUEEZE_PERCENT);
            }
        }
        dt::PULSE_WIDTH => {
            let pulse = |value: PolyFloat| {
                let distortion = utils::max(
                    PolyFloat::splat(1.0) - value,
                    PolyFloat::splat(1.0 / u32::MAX as f32),
                );
                PolyFloat::splat(1.0) / distortion
            };
            if spread {
                for value in values.iter_mut() {
                    *value = pulse(*value);
                }
            } else {
                let distortion = pulse(values[0]);
                for value in values.iter_mut() {
                    *value = distortion;
                }
            }
        }
        _ => {}
    }
}

/// `SynthOscillator::setSpectralMorphValues(type, values, num_values, spread)`.
pub fn set_spectral_morph_values_for(kind: i32, values: &mut [PolyFloat], spread: bool) {
    match kind {
        sm::VOCODE => set_power_distortion_values(values, -morph::MAX_FORMANT_SHIFT, spread),
        sm::FORM_SCALE => {
            set_power_distortion_values(values, -morph::MAX_EVEN_ODD_FORMANT_SHIFT, spread)
        }
        sm::HARMONIC_SCALE => {
            set_power_distortion_values(values, morph::MAX_HARMONIC_SCALE, spread)
        }
        sm::INHARMONIC_SCALE => {
            set_power_distortion_values(values, morph::MAX_INHARMONIC_SCALE, spread)
        }
        sm::SMEAR => {
            for value in values.iter_mut() {
                let invert = PolyFloat::splat(1.0) - *value;
                *value = PolyFloat::splat(1.0) - invert * invert * invert;
            }
        }
        sm::RANDOM_AMPLITUDES => {
            for value in values.iter_mut() {
                *value *= (morph::RANDOM_AMPLITUDE_STAGES - 1) as f32;
            }
        }
        sm::PHASE_DISPERSE => {
            for value in values.iter_mut() {
                *value = -(*value * 2.0 - 1.0) * morph::PHASE_DISPERSE_SCALE;
            }
        }
        sm::SKEW => {
            for value in values.iter_mut() {
                *value = *value * *value * morph::SKEW_SCALE;
            }
        }
        sm::SHEPARD_TONE => {
            for value in values.iter_mut() {
                *value = PolyFloat::splat(1.0) - *value;
            }
        }
        _ => {}
    }
}

/// The morph function `setWaveBuffers` uses for `kind`.
pub fn morph_function(kind: i32) -> MorphFn {
    match kind {
        sm::SHEPARD_TONE => morph::shepard_morph,
        sm::VOCODE | sm::FORM_SCALE => morph::even_odd_vocode_morph,
        sm::HARMONIC_SCALE => morph::harmonic_scale_morph,
        sm::INHARMONIC_SCALE => morph::inharmonic_scale_morph,
        sm::SMEAR => morph::smear_morph,
        sm::RANDOM_AMPLITUDES => morph::random_amplitude_morph,
        sm::LOW_PASS => morph::low_pass_morph,
        sm::HIGH_PASS => morph::high_pass_morph,
        sm::PHASE_DISPERSE => morph::phase_morph,
        sm::SKEW => morph::wavetable_skew_morph,
        _ => morph::passthrough_morph,
    }
}

/// `SynthOscillator::runSpectralMorph` (used by displays).
pub fn run_spectral_morph(
    kind: i32,
    morph_amount: f32,
    wavetable_data: &WavetableData,
    wavetable_index: usize,
    dest: &mut [f32],
    transform: &mut FourierTransform,
) {
    let morph_fn = morph_function(kind);
    morph_fn(
        wavetable_data,
        wavetable_index,
        dest,
        transform,
        morph_amount,
        NUM_HARMONICS as i32,
        RandomValues::instance(),
    );
}

/// `SynthOscillator::adjustPhase` (used by displays).
pub fn adjust_phase(
    kind: i32,
    phase: PolyInt,
    distortion_amount: PolyFloat,
    distortion_phase: PolyInt,
) -> PolyInt {
    let distort: PhaseDistortFn = match kind {
        dt::SYNC | dt::FORMANT => sync_phase,
        dt::QUANTIZE => quantize_phase,
        dt::BEND => bend_phase,
        dt::SQUEEZE => squeeze_phase,
        dt::PULSE_WIDTH => pulse_width_phase,
        _ => return phase,
    };
    distort(phase, distortion_amount, distortion_phase, &[], 0)
}

/// `SynthOscillator::getPhaseWindow` (used by displays).
pub fn get_phase_window(kind: i32, phase: PolyInt, distorted_phase: PolyInt) -> PolyFloat {
    match kind {
        dt::FORMANT => half_sin_window(phase, distorted_phase, PolyFloat::zero(), &[], 0),
        dt::PULSE_WIDTH => pulse_width_window(phase, distorted_phase, PolyFloat::zero(), &[], 0),
        _ => PolyFloat::splat(1.0),
    }
}

/// `SynthOscillator::interpolate` (used by displays).
pub fn interpolate(buffer: &[f32], indices: PolyInt) -> PolyFloat {
    linearly_interpolate_buffer(buffer, indices)
}

/// `SynthOscillator::usesDistortionPhase`.
pub fn uses_distortion_phase(kind: i32) -> bool {
    kind == dt::SYNC
        || kind == dt::FORMANT
        || kind == dt::QUANTIZE
        || kind == dt::BEND
        || kind == dt::SQUEEZE
        || kind == dt::PULSE_WIDTH
}

// ----- the processor -----

type FourierBank = Vec<Box<[f32]>>;

fn new_bank() -> FourierBank {
    (0..=NUM_BUFFERS)
        .map(|_| vec![0.0f32; SPECTRAL_BUFFER_FLOATS].into_boxed_slice())
        .collect()
}

/// `vital::SynthOscillator`'s members.
pub struct SynthOscillator {
    phases: [PolyInt; NUM_POLY_PHASE],
    detunings: [PolyFloat; NUM_POLY_PHASE],
    phase_inc_mults: [PolyFloat; NUM_POLY_PHASE],
    from_phase_inc_mults: [PolyFloat; NUM_POLY_PHASE],
    shepard_double_masks: [PolyInt; NUM_POLY_PHASE],
    shepard_half_masks: [PolyInt; NUM_POLY_PHASE],
    waiting_shepard_double_masks: [PolyInt; NUM_POLY_PHASE],
    waiting_shepard_half_masks: [PolyInt; NUM_POLY_PHASE],
    pan_amplitude: PolyFloat,
    center_amplitude: PolyFloat,
    detuned_amplitude: PolyFloat,
    midi_note: PolyFloat,
    distortion_phase: PolyFloat,
    blend_stereo_multiply: PolyFloat,
    blend_center_multiply: PolyFloat,
    wave_buffers: [WaveBuffer; NUM_BUFFERS],
    last_buffers: [WaveBuffer; NUM_BUFFERS],
    spectral_morph_values: [PolyFloat; NUM_POLY_PHASE],
    last_spectral_morph_values: [PolyFloat; NUM_POLY_PHASE],
    distortion_values: [PolyFloat; NUM_POLY_PHASE],
    last_distortion_values: [PolyFloat; NUM_POLY_PHASE],
    voice_block: VoiceBlock,
    /// `voice_block_.modulation_buffer` (the output it points into).
    voice_block_modulation: Option<Rc<Output>>,
    random_generator: RandomGenerator,

    transpose_quantize: i32,
    last_quantized_transpose: PolyFloat,
    last_quantize_ratio: PolyFloat,
    unison: i32,
    active_oscillators: i32,
    wavetable: SharedWavetable,
    wavetable_version: i32,
    first_mod_oscillator: Option<Rc<Output>>,
    second_mod_oscillator: Option<Rc<Output>>,
    sample: Option<Rc<Output>>,

    fourier_frames1: FourierBank,
    fourier_frames2: FourierBank,
    fourier_transform: FourierTransform,
    phase_inc_buffer: Vec<PolyFloat>,
    phase_buffer: Vec<PolyInt>,
}

impl Clone for SynthOscillator {
    /// The copy constructor. Like every upstream `RandomGenerator` copy, the
    /// copy's generator takes a fresh seed from the global counter.
    fn clone(&self) -> Self {
        Self {
            phases: self.phases,
            detunings: self.detunings,
            phase_inc_mults: self.phase_inc_mults,
            from_phase_inc_mults: self.from_phase_inc_mults,
            shepard_double_masks: self.shepard_double_masks,
            shepard_half_masks: self.shepard_half_masks,
            waiting_shepard_double_masks: self.waiting_shepard_double_masks,
            waiting_shepard_half_masks: self.waiting_shepard_half_masks,
            pan_amplitude: self.pan_amplitude,
            center_amplitude: self.center_amplitude,
            detuned_amplitude: self.detuned_amplitude,
            midi_note: self.midi_note,
            distortion_phase: self.distortion_phase,
            blend_stereo_multiply: self.blend_stereo_multiply,
            blend_center_multiply: self.blend_center_multiply,
            wave_buffers: self.wave_buffers,
            last_buffers: self.last_buffers,
            spectral_morph_values: self.spectral_morph_values,
            last_spectral_morph_values: self.last_spectral_morph_values,
            distortion_values: self.distortion_values,
            last_distortion_values: self.last_distortion_values,
            voice_block: self.voice_block.clone(),
            voice_block_modulation: self.voice_block_modulation.clone(),
            random_generator: RandomGenerator::from_global_counter(
                self.random_generator.min(),
                self.random_generator.max(),
            ),
            transpose_quantize: self.transpose_quantize,
            last_quantized_transpose: self.last_quantized_transpose,
            last_quantize_ratio: self.last_quantize_ratio,
            unison: self.unison,
            active_oscillators: self.active_oscillators,
            wavetable: self.wavetable.clone(),
            wavetable_version: self.wavetable_version,
            first_mod_oscillator: self.first_mod_oscillator.clone(),
            second_mod_oscillator: self.second_mod_oscillator.clone(),
            sample: self.sample.clone(),
            fourier_frames1: self.fourier_frames1.clone(),
            fourier_frames2: self.fourier_frames2.clone(),
            fourier_transform: self.fourier_transform.clone(),
            phase_inc_buffer: self.phase_inc_buffer.clone(),
            phase_buffer: self.phase_buffer.clone(),
        }
    }
}

impl SynthOscillator {
    /// `SynthOscillator(wavetable)` (the kernel; see [`SynthOscillator::create`]).
    pub fn new(wavetable: SharedWavetable) -> Self {
        let random_generator = RandomGenerator::from_global_counter(-1.0, 1.0);
        let wavetable_version = wavetable.borrow().version();
        let mut oscillator = Self {
            phases: [PolyInt::zero(); NUM_POLY_PHASE],
            detunings: [PolyFloat::splat(1.0); NUM_POLY_PHASE],
            phase_inc_mults: [PolyFloat::splat(1.0); NUM_POLY_PHASE],
            from_phase_inc_mults: [PolyFloat::splat(1.0); NUM_POLY_PHASE],
            shepard_double_masks: [PolyInt::zero(); NUM_POLY_PHASE],
            shepard_half_masks: [PolyInt::zero(); NUM_POLY_PHASE],
            waiting_shepard_double_masks: [PolyInt::zero(); NUM_POLY_PHASE],
            waiting_shepard_half_masks: [PolyInt::zero(); NUM_POLY_PHASE],
            pan_amplitude: PolyFloat::zero(),
            center_amplitude: PolyFloat::zero(),
            detuned_amplitude: PolyFloat::zero(),
            midi_note: PolyFloat::zero(),
            distortion_phase: PolyFloat::zero(),
            blend_stereo_multiply: PolyFloat::zero(),
            blend_center_multiply: PolyFloat::zero(),
            wave_buffers: [WaveBuffer::Null; NUM_BUFFERS],
            last_buffers: [WaveBuffer::Null; NUM_BUFFERS],
            spectral_morph_values: [PolyFloat::zero(); NUM_POLY_PHASE],
            last_spectral_morph_values: [PolyFloat::splat(1.0); NUM_POLY_PHASE],
            distortion_values: [PolyFloat::zero(); NUM_POLY_PHASE],
            last_distortion_values: [PolyFloat::zero(); NUM_POLY_PHASE],
            voice_block: VoiceBlock::default(),
            voice_block_modulation: None,
            random_generator,
            transpose_quantize: 0,
            last_quantized_transpose: PolyFloat::zero(),
            last_quantize_ratio: PolyFloat::splat(1.0),
            unison: 1,
            active_oscillators: 2,
            wavetable,
            wavetable_version,
            first_mod_oscillator: None,
            second_mod_oscillator: None,
            sample: None,
            fourier_frames1: new_bank(),
            fourier_frames2: new_bank(),
            fourier_transform: FourierTransform::new(WAVEFORM_BITS),
            phase_inc_buffer: vec![PolyFloat::zero(); MAX_BUFFER_SIZE],
            phase_buffer: vec![PolyInt::zero(); MAX_BUFFER_SIZE * MAX_OVERSAMPLE],
        };
        oscillator.reset_wavetable_buffers();
        RandomValues::instance();
        oscillator
    }

    /// `new SynthOscillator(wavetable)`.
    pub fn create(wavetable: SharedWavetable) -> ProcessorRef {
        Processor::new(Self::new(wavetable), NUM_INPUTS, NUM_OUTPUTS, false, 1)
    }

    pub fn set_first_oscillator_output(&mut self, oscillator: Option<Rc<Output>>) {
        self.first_mod_oscillator = oscillator;
    }

    pub fn set_second_oscillator_output(&mut self, oscillator: Option<Rc<Output>>) {
        self.second_mod_oscillator = oscillator;
    }

    pub fn set_sample_output(&mut self, sample: Option<Rc<Output>>) {
        self.sample = sample;
    }

    pub fn wavetable(&self) -> &SharedWavetable {
        &self.wavetable
    }

    /// `phase_inc_buffer_`: the per-sample phase increments of the last
    /// processed block (`frequency * 2^32 / sampleRate`, voice-swapped for a
    /// single active voice).
    pub fn phase_increments(&self) -> &[PolyFloat] {
        &self.phase_inc_buffer
    }

    fn buffer_slice(&self, buffer: WaveBuffer) -> &[f32] {
        match buffer {
            WaveBuffer::Null => &ZERO_WAVEFORM,
            WaveBuffer::Frames1(index) => &self.fourier_frames1[index][3..],
            WaveBuffer::Frames2(index) => &self.fourier_frames2[index][3..],
        }
    }

    fn phase_inc_adjustment(this: &Processor) -> f32 {
        const BASE_SAMPLE_RATE: i32 = 44100;
        let mut adjustment = 1.0f32;
        let mut sample_rate_mult = this.sample_rate() / BASE_SAMPLE_RATE;
        while sample_rate_mult > 1 {
            sample_rate_mult >>= 1;
            adjustment *= 2.0;
        }
        adjustment
    }

    /// `reset(reset_mask, sample)`.
    fn reset_at(&mut self, this: &Processor, reset_mask: PolyMask, sample: PolyInt) {
        self.reset_mask(this, reset_mask);
        self.voice_block.current_buffer_sample =
            utils::mask_load_int(self.voice_block.current_buffer_sample, -sample, reset_mask);
    }

    /// `reset(reset_mask)`.
    fn reset_mask(&mut self, this: &Processor, reset_mask: PolyMask) {
        let random_amount = this.input(RANDOM_PHASE).at(0);
        self.last_quantize_ratio = utils::mask_load(self.last_quantize_ratio, 1.0, reset_mask);

        for v in 0..NUM_VOICES_PER_PROCESS {
            if reset_mask.get(v * 2) != 0 {
                for i in 0..NUM_POLY_PHASE {
                    let random_phase_left = float_to_u32(
                        self.random_generator.next() * random_amount.get(2 * v) * INT_MAX_F,
                    );
                    let random_phase_right = float_to_u32(
                        self.random_generator.next() * random_amount.get(2 * v + 1) * INT_MAX_F,
                    );
                    self.phases[i].set(2 * v, random_phase_left);
                    self.phases[i].set(2 * v + 1, random_phase_right);

                    let buffer_index = i * 4 + 2 * v;
                    self.last_buffers[buffer_index] = self.wave_buffers[buffer_index];
                    self.last_buffers[buffer_index + 1] = self.wave_buffers[buffer_index + 1];
                }

                if self.unison < self.active_oscillators {
                    let right = self.phases[0].get(v * 2 + 1);
                    self.phases[0].set(v * 2, right);
                }
            }
        }

        for i in 0..NUM_POLY_PHASE {
            self.last_distortion_values[i] = utils::mask_load(
                self.last_distortion_values[i],
                self.distortion_values[i],
                reset_mask,
            );
            self.last_spectral_morph_values[i] = utils::mask_load(
                self.last_spectral_morph_values[i],
                self.spectral_morph_values[i],
                reset_mask,
            );
            self.from_phase_inc_mults[i] = utils::mask_load(
                self.from_phase_inc_mults[i],
                self.phase_inc_mults[i],
                reset_mask,
            );
            self.shepard_double_masks[i] &= !reset_mask;
            self.shepard_half_masks[i] &= !reset_mask;
            self.waiting_shepard_double_masks[i] &= !reset_mask;
            self.waiting_shepard_half_masks[i] &= !reset_mask;
        }
    }

    #[allow(clippy::needless_range_loop)]
    fn set_phase_inc_mults(&mut self, this: &Processor) {
        let range = this.input(DETUNE_RANGE).at(0);
        let cents = range * this.input(UNISON_DETUNE).at(0);
        let power = this.input(DETUNE_POWER).at(0);
        let stack_style = float_to_i32(this.input(STACK_STYLE).at(0).get(0).round());
        let stack_settings = &STACK_MULTIPLIERS[stack_style as usize];

        let divisor = sas_simd::utils::max(1.0, self.unison as f32 - 1.0);
        let bump = if self.unison % 2 == 0 { 1 } else { 0 };

        let mut sharp_flat_mask = constants::left_mask();
        let num_updates = (self.active_oscillators / 2) as usize;
        for i in 0..num_updates {
            let t = (2 * i as i32 + bump) as f32 / divisor;
            let adjusted_t = futils::power_scale(PolyFloat::splat(t), power);
            let oscillator_cents = adjusted_t * cents;

            let up_ratio = utils::cents_to_ratio(oscillator_cents);
            let down_ratio = PolyFloat::splat(1.0) / up_ratio;
            self.detunings[i] =
                utils::mask_load(up_ratio, down_ratio, sharp_flat_mask) * stack_settings[i];
            self.from_phase_inc_mults[i] = self.phase_inc_mults[i];
            self.phase_inc_mults[i] = self.detunings[i];

            sharp_flat_mask = !sharp_flat_mask;
        }
    }

    fn setup_shepard_wrap(&mut self) {
        let num_phase_updates = (self.active_oscillators / 2) as usize;

        let ratio_div = PolyFloat::splat(1.0) / self.last_quantize_ratio;
        for i in 0..num_phase_updates {
            let spectral_diff = self.last_spectral_morph_values[i] - self.spectral_morph_values[i];
            let mult = futils::exp2(-self.spectral_morph_values[i]);
            self.phase_inc_mults[i] *= mult;
            self.detunings[i] *= mult;

            let double_mask = self.waiting_shepard_double_masks[i]
                | PolyFloat::less_than(spectral_diff, PolyFloat::splat(-0.6));
            let half_mask = self.waiting_shepard_half_masks[i]
                | PolyFloat::greater_than(spectral_diff, PolyFloat::splat(0.6));

            self.phase_inc_mults[i] = utils::mask_load(
                self.phase_inc_mults[i],
                self.phase_inc_mults[i] * 2.0,
                double_mask,
            );
            self.phase_inc_mults[i] = utils::mask_load(
                self.phase_inc_mults[i],
                self.phase_inc_mults[i] * 0.5,
                half_mask,
            );
            let reset_phase_inc_mult = self.from_phase_inc_mults[i] * ratio_div;
            self.from_phase_inc_mults[i] = utils::mask_load(
                self.from_phase_inc_mults[i],
                reset_phase_inc_mult,
                half_mask | double_mask,
            );

            self.waiting_shepard_double_masks[i] = double_mask;
            self.waiting_shepard_half_masks[i] = half_mask;
        }
    }

    fn clear_shepard_wrap(&mut self) {
        let num_phase_updates = (self.active_oscillators / 2) as usize;
        for i in 0..num_phase_updates {
            self.shepard_double_masks[i] = PolyInt::zero();
            self.shepard_half_masks[i] = PolyInt::zero();
            self.waiting_shepard_double_masks[i] = PolyInt::zero();
            self.waiting_shepard_half_masks[i] = PolyInt::zero();
        }
    }

    fn do_shepard_wrap(&mut self, new_buffer_mask: PolyMask, quantize: i32) {
        let num_phase_updates = (self.active_oscillators / 2) as usize;

        for i in 0..num_phase_updates {
            let double_mask = self.waiting_shepard_double_masks[i] & new_buffer_mask;
            let half_mask = self.waiting_shepard_half_masks[i] & new_buffer_mask;
            self.waiting_shepard_double_masks[i] &= !new_buffer_mask;
            self.waiting_shepard_half_masks[i] &= !new_buffer_mask;

            if quantize != 0 {
                self.phase_inc_mults[i] = utils::mask_load(
                    self.phase_inc_mults[i],
                    self.phase_inc_mults[i] * 0.5,
                    double_mask,
                );
                self.phases[i] = utils::mask_load_int(
                    self.phases[i],
                    utils::shift_right::<1>(self.phases[i]),
                    double_mask,
                );

                self.phase_inc_mults[i] = utils::mask_load(
                    self.phase_inc_mults[i],
                    self.phase_inc_mults[i] * 2.0,
                    half_mask,
                );
                self.phases[i] =
                    utils::mask_load_int(self.phases[i], self.phases[i] * 2, half_mask);
                self.from_phase_inc_mults[i] = utils::mask_load(
                    self.from_phase_inc_mults[i],
                    self.phase_inc_mults[i],
                    double_mask | half_mask,
                );
            } else {
                self.phase_inc_mults[i] = utils::mask_load(
                    self.phase_inc_mults[i],
                    self.phase_inc_mults[i] * 0.5,
                    double_mask,
                );
                self.from_phase_inc_mults[i] = utils::mask_load(
                    self.from_phase_inc_mults[i],
                    self.from_phase_inc_mults[i] * 0.5,
                    double_mask,
                );
                self.phases[i] = utils::mask_load_int(
                    self.phases[i],
                    utils::shift_right::<1>(self.phases[i]),
                    double_mask,
                );

                self.phase_inc_mults[i] = utils::mask_load(
                    self.phase_inc_mults[i],
                    self.phase_inc_mults[i] * 2.0,
                    half_mask,
                );
                self.from_phase_inc_mults[i] = utils::mask_load(
                    self.from_phase_inc_mults[i],
                    self.from_phase_inc_mults[i] * 2.0,
                    half_mask,
                );
                // `phases_[i] * 2.0f`: the float converts to a `poly_int`.
                self.phases[i] =
                    utils::mask_load_int(self.phases[i], self.phases[i] * 2, half_mask);
            }

            self.shepard_double_masks[i] =
                utils::mask_load_int(self.shepard_double_masks[i], double_mask, new_buffer_mask);
            self.shepard_half_masks[i] =
                utils::mask_load_int(self.shepard_half_masks[i], half_mask, new_buffer_mask);
        }
    }

    fn set_amplitude(&mut self, this: &Processor) {
        if self.unison <= 2 {
            self.center_amplitude = PolyFloat::splat(1.0);
            self.detuned_amplitude = PolyFloat::zero();
            return;
        }

        let blend = this.input(BLEND).at(0);
        let center = utils::interpolate(1.0, CENTER_LOW_AMPLITUDE, blend);
        let mut detuned_blend = -blend + 1.0;
        detuned_blend *= detuned_blend;
        let detuned = utils::interpolate(DETUNED_HIGH_AMPLITUDE, 0.0, detuned_blend);

        let half_oscillators = self.active_oscillators / 2;
        let square_sums = center * center + detuned * detuned * (half_oscillators - 1) as f32;
        let adjustment = PolyFloat::splat(1.0) / utils::sqrt(square_sums);
        self.center_amplitude = adjustment * center;
        self.detuned_amplitude = adjustment * detuned;
    }

    fn set_wave_buffers(&mut self, this: &Processor, phase_inc: PolyFloat, index: usize) {
        let kind = self.voice_block.spectral_morph;
        let formant_shift = kind == sm::VOCODE;
        self.set_fourier_wave_buffers(this, morph_function(kind), phase_inc, index, formant_shift);

        self.voice_block.current_buffer_sample.set(index, 0);
        self.voice_block.current_buffer_sample.set(index + 1, 0);
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_spectral_wave_buffer_pair(
        &mut self,
        morph_fn: MorphFn,
        phase_update: usize,
        index: usize,
        formant_shift: bool,
        phase_adjustment: f32,
        wave_index: PolyInt,
        voice_increment: PolyFloat,
        morph_amount: PolyFloat,
    ) {
        let random_values = RandomValues::instance();
        for i in index..index + 2 {
            let adjust_phase_inc = voice_increment.get(i) * phase_adjustment;
            let formant_adjustment = voice_increment.get(i) * WAVEFORM_SIZE as f32;
            let bin = Wavetable::frequency_float_bin(adjust_phase_inc);
            let buffer_index = phase_update * 4 + i;
            self.last_buffers[buffer_index] = self.wave_buffers[buffer_index];

            let use_second = self.wave_buffers[buffer_index] == WaveBuffer::Frames1(buffer_index);

            let mut shift = morph_amount.get(i);
            if formant_shift {
                shift *= formant_adjustment;
            }
            let wavetable = self.wavetable.borrow();
            let wavetable_data = wavetable.data();
            let table_index =
                (wave_index.get(i) as i32).min(wavetable_data.num_frames as i32 - 1) as usize;

            let bin_shift = FREQUENCY_BINS as f32 + 1.0 - bin;
            let mut last_harmonic = 0.max(float_to_i32(
                WAVEFORM_SIZE as f32 * futils::exp2_mono(-bin_shift),
            ));
            last_harmonic = last_harmonic.min(WAVEFORM_SIZE as i32 / 2);

            let fourier_buffer = if use_second {
                &mut self.fourier_frames2[buffer_index]
            } else {
                &mut self.fourier_frames1[buffer_index]
            };
            morph_fn(
                wavetable_data,
                table_index,
                fourier_buffer,
                &mut self.fourier_transform,
                shift,
                last_harmonic,
                random_values,
            );
            self.wave_buffers[buffer_index] = if use_second {
                WaveBuffer::Frames2(buffer_index)
            } else {
                WaveBuffer::Frames1(buffer_index)
            };

            if i == index
                && morph_amount.get(i) == morph_amount.get(i + 1)
                && wave_index.get(i) == wave_index.get(i + 1)
            {
                self.last_buffers[buffer_index + 1] = self.wave_buffers[buffer_index + 1];
                self.wave_buffers[buffer_index + 1] = self.wave_buffers[buffer_index];
                return;
            }
        }
    }

    fn set_fourier_wave_buffers(
        &mut self,
        this: &Processor,
        morph_fn: MorphFn,
        phase_inc: PolyFloat,
        index: usize,
        formant_shift: bool,
    ) {
        let wave_frame = this.input(WAVE_FRAME).at(0);
        let frame_spread = this.input(UNISON_FRAME_SPREAD).at(0);
        let phase_inc = utils::max(PolyFloat::zero(), phase_inc);
        let phase_inc_adjustment = Self::phase_inc_adjustment(this);

        let distortion = float_to_i32(this.input(DISTORTION_TYPE).at(0).get(0));
        let mut distortion_frequency_mask = PolyMask::zero();
        let mut distortion_mult = 1.0f32;
        if distortion == dt::FORMANT || distortion == dt::SYNC {
            distortion_frequency_mask = constants::full_mask();
            distortion_mult = MAX_SYNC as f32;
        }

        let spectral_morph = float_to_i32(this.input(SPECTRAL_MORPH_TYPE).at(0).get(0));
        let spectral_unison_mask =
            PolyFloat::not_equal(this.input(SPECTRAL_UNISON).at(0), PolyFloat::zero());
        let spectral_morph_mask =
            PolyFloat::not_equal(self.spectral_morph_values[0], self.spectral_morph_values[1]);
        let frame_spread_mask = PolyFloat::not_equal(frame_spread, PolyFloat::zero());
        let spectral_unison = spectral_unison_mask.any_mask() != 0
            && (spectral_morph_mask.any_mask() != 0
                || frame_spread_mask.any_mask() != 0
                || spectral_morph == sm::VOCODE);

        let max_frame = (NUM_OSCILLATOR_WAVE_FRAMES - 1) as f32;
        let num_phase_updates = (self.active_oscillators / 2) as usize;
        if spectral_unison {
            let t_inc = 1.0 / (sas_simd::utils::imax(2, num_phase_updates as i32) as f32 - 1.0);
            for v in 0..num_phase_updates {
                let frequency_mult = self.distortion_values[v] * distortion_mult;
                let frequency_mult =
                    utils::mask_load(1.0, frequency_mult, distortion_frequency_mask);

                let morph_amount = self.spectral_morph_values[v];
                let voice_increment = phase_inc * self.detunings[v] * frequency_mult;
                let t = PolyFloat::splat(v as f32 * t_inc);
                let frame = wave_frame + t * frame_spread;
                let wave_index = to_int(utils::clamp(frame, 0.0, max_frame));

                self.compute_spectral_wave_buffer_pair(
                    morph_fn,
                    v,
                    index,
                    formant_shift,
                    phase_inc_adjustment,
                    wave_index,
                    voice_increment,
                    morph_amount,
                );
            }
        } else {
            let frequency_mult = self.distortion_values[0] * distortion_mult;
            let frequency_mult = utils::mask_load(1.0, frequency_mult, distortion_frequency_mask);

            let morph_amount = self.spectral_morph_values[0];
            let voice_increment = phase_inc * self.detunings[0] * frequency_mult;
            let wave_index = to_int(utils::clamp(wave_frame, 0.0, max_frame));

            self.compute_spectral_wave_buffer_pair(
                morph_fn,
                0,
                index,
                formant_shift,
                phase_inc_adjustment,
                wave_index,
                voice_increment,
                morph_amount,
            );

            for v in 1..num_phase_updates {
                for i in index..index + 2 {
                    let buffer_index = v * 4 + i;
                    self.last_buffers[buffer_index] = self.wave_buffers[buffer_index];
                    self.wave_buffers[buffer_index] = self.wave_buffers[i];
                }
            }
        }
    }

    fn stereo_blend(&mut self, this: &Processor, num_samples: usize, reset_mask: PolyMask) {
        let audio_out = this.output_buffer(RAW);
        let stereo_spread = utils::clamp(this.input(STEREO_SPREAD).at(0), 0.0, 1.0);

        let mut current_stereo_mult = self.blend_stereo_multiply;
        let mut current_center_mult = self.blend_center_multiply;
        self.blend_stereo_multiply = futils::equal_power_fade(stereo_spread * 0.5 + 0.5);
        self.blend_center_multiply = futils::equal_power_fade_inverse(stereo_spread * 0.5 + 0.5);

        current_stereo_mult =
            utils::mask_load(current_stereo_mult, self.blend_stereo_multiply, reset_mask);
        current_center_mult =
            utils::mask_load(current_center_mult, self.blend_center_multiply, reset_mask);
        let delta_stereo_mult =
            (self.blend_stereo_multiply - current_stereo_mult) * (1.0 / num_samples as f32);
        let delta_center_mult =
            (self.blend_center_multiply - current_center_mult) * (1.0 / num_samples as f32);

        if delta_stereo_mult.sum() + delta_center_mult.sum() == 0.0
            && utils::equal(stereo_spread, PolyFloat::splat(1.0))
        {
            return;
        }

        for sample in audio_out.iter().take(num_samples) {
            current_stereo_mult += delta_stereo_mult;
            current_center_mult += delta_center_mult;
            let val = sample.get();
            let swap = utils::swap_stereo(val);
            sample.set(val * current_stereo_mult + swap * current_center_mult);
        }
    }

    fn level_output(&mut self, this: &Processor, num_samples: usize, reset_mask: PolyMask) {
        let audio_out = this.output_buffer(LEVELLED);
        let raw_out = this.output_buffer(RAW);

        let mut current_pan_amplitude = self.pan_amplitude;
        self.pan_amplitude = futils::pan_amplitude(utils::clamp(this.input(PAN).at(0), -1.0, 1.0));

        current_pan_amplitude =
            utils::mask_load(current_pan_amplitude, self.pan_amplitude, reset_mask);
        let delta_pan_amplitude =
            (self.pan_amplitude - current_pan_amplitude) * (1.0 / num_samples as f32);

        let amplitude = this.input_buffer(AMPLITUDE);
        let zero = PolyFloat::zero();
        for i in 0..num_samples {
            let amp = utils::max(amplitude[i].get(), zero);
            current_pan_amplitude += delta_pan_amplitude;
            audio_out[i].set(current_pan_amplitude * raw_out[i].get() * amp * amp);
        }
    }

    fn convert_voice_channels(audio_out: &[Cell<PolyFloat>]) {
        for sample in audio_out {
            sample.set(sample.get() + utils::swap_voices(sample.get()));
        }
    }

    fn reset_wavetable_buffers(&mut self) {
        self.last_buffers = [WaveBuffer::Null; NUM_BUFFERS];
        self.wave_buffers = [WaveBuffer::Null; NUM_BUFFERS];
    }

    fn load_voice_block(&mut self, index: usize, active_mask: PolyMask) {
        let voice_block = &mut self.voice_block;
        let single_voice = (!active_mask).any_mask() != 0;
        if single_voice {
            voice_block.phase = compact_and_load_voice_int(&self.phases[2 * index..], active_mask);
            voice_block.phase_inc_mult =
                compact_and_load_voice_float(&self.phase_inc_mults[2 * index..], active_mask);
            voice_block.from_phase_inc_mult =
                compact_and_load_voice_float(&self.from_phase_inc_mults[2 * index..], active_mask);
            voice_block.shepard_double_mask =
                compact_and_load_voice_int(&self.shepard_double_masks[2 * index..], active_mask);
            voice_block.shepard_half_mask =
                compact_and_load_voice_int(&self.shepard_half_masks[2 * index..], active_mask);
            voice_block.distortion =
                compact_and_load_voice_float(&self.distortion_values[2 * index..], active_mask);
            voice_block.last_distortion = compact_and_load_voice_float(
                &self.last_distortion_values[2 * index..],
                active_mask,
            );
            let distortion_phase_swap = utils::swap_voices_int(voice_block.distortion_phase);
            voice_block.distortion_phase = utils::mask_load_int(
                distortion_phase_swap,
                voice_block.distortion_phase,
                active_mask,
            );
            let last_distortion_phase_swap =
                utils::swap_voices_int(voice_block.last_distortion_phase);
            voice_block.last_distortion_phase = utils::mask_load_int(
                last_distortion_phase_swap,
                voice_block.last_distortion_phase,
                active_mask,
            );

            let buffer_index = 2 * index * 4;
            compact_and_load_voice_buffers(
                &mut voice_block.from_buffers,
                &self.last_buffers[buffer_index..],
                active_mask,
            );
            compact_and_load_voice_buffers(
                &mut voice_block.to_buffers,
                &self.wave_buffers[buffer_index..],
                active_mask,
            );

            if (index as i32 + 1) * 4 > self.active_oscillators {
                let zero_index = if active_mask.get(0) != 0 { 2 } else { 0 };
                voice_block.from_buffers[zero_index] = WaveBuffer::Null;
                voice_block.from_buffers[zero_index + 1] = WaveBuffer::Null;
                voice_block.to_buffers[zero_index] = WaveBuffer::Null;
                voice_block.to_buffers[zero_index + 1] = WaveBuffer::Null;
            }
        } else {
            voice_block.phase = self.phases[index];
            voice_block.phase_inc_mult = self.phase_inc_mults[index];
            voice_block.from_phase_inc_mult = self.from_phase_inc_mults[index];
            voice_block.shepard_double_mask = self.shepard_double_masks[index];
            voice_block.shepard_half_mask = self.shepard_half_masks[index];
            voice_block.distortion = self.distortion_values[index];
            voice_block.last_distortion = self.last_distortion_values[index];

            let buffer_index = index * 4;
            voice_block
                .from_buffers
                .copy_from_slice(&self.last_buffers[buffer_index..buffer_index + 4]);
            voice_block
                .to_buffers
                .copy_from_slice(&self.wave_buffers[buffer_index..buffer_index + 4]);
        }
    }

    fn set_spectral_morph_values(&mut self, this: &Processor, spectral_morph: i32) {
        const MOD_MULT: f32 = 0.99;
        let spectral_morph_amount = this.input(SPECTRAL_MORPH_AMOUNT).at(0);
        let morph_spread = this.input(UNISON_SPECTRAL_MORPH_SPREAD).at(0);
        let num_phase_updates = self.active_oscillators / 2;

        for v in 0..NUM_POLY_PHASE {
            let t = (v as f32 / (sas_simd::utils::imax(2, num_phase_updates) as f32 - 1.0)) * 2.0;
            self.last_spectral_morph_values[v] = self.spectral_morph_values[v];
            self.spectral_morph_values[v] = spectral_morph_amount + morph_spread * t;
        }

        if spectral_morph == sm::SHEPARD_TONE {
            for value in self.spectral_morph_values.iter_mut() {
                *value = utils::modulo(*value * MOD_MULT) * (1.0 / MOD_MULT);
            }
        } else {
            for value in self.spectral_morph_values.iter_mut() {
                *value = utils::clamp(*value, 0.0, 1.0);
            }
        }

        let is_spread = PolyFloat::not_equal(morph_spread, PolyFloat::zero()).any_mask() != 0;
        set_spectral_morph_values_for(spectral_morph, &mut self.spectral_morph_values, is_spread);
        if spectral_morph == sm::VOCODE {
            const DEFAULT_SAMPLE_RATE: f32 = 88200.0;
            let wavetable = self.wavetable.borrow();
            let mut sample_rate = wavetable.data().sample_rate;
            if sample_rate <= 0.0 {
                sample_rate = DEFAULT_SAMPLE_RATE;
            }
            let sample_rate_ratio = this.sample_rate() as f32 / sample_rate;
            let frequency_ratio = sample_rate_ratio * wavetable.data().frequency_ratio;

            for value in self.spectral_morph_values.iter_mut() {
                *value *= frequency_ratio;
            }
        }
    }

    fn set_distortion_values(&mut self, this: &Processor, distortion: i32) {
        let distortion_amount = this.input(DISTORTION_AMOUNT).at(0);
        let distortion_spread = this.input(UNISON_DISTORTION_SPREAD).at(0);
        let num_phase_updates = self.active_oscillators / 2;
        for v in 0..NUM_POLY_PHASE {
            let t = v as f32 / (sas_simd::utils::imax(2, num_phase_updates) as f32 - 1.0) * 2.0;
            self.last_distortion_values[v] = self.distortion_values[v];
            self.distortion_values[v] =
                utils::clamp(distortion_amount + distortion_spread * t, 0.0, 1.0);
        }

        if distortion == dt::QUANTIZE {
            for value in self.last_distortion_values.iter_mut() {
                *value = utils::max(PolyFloat::splat(1.5), *value);
            }
        }

        let is_spread = PolyFloat::not_equal(distortion_spread, PolyFloat::zero()).any_mask() != 0;
        set_distortion_values_for(distortion, &mut self.distortion_values, is_spread);
    }

    fn set_active_oscillators(&mut self, new_active_oscillators: i32) {
        for i in self.active_oscillators..new_active_oscillators {
            self.wave_buffers[2 * i as usize] = WaveBuffer::Null;
            self.wave_buffers[2 * i as usize + 1] = WaveBuffer::Null;
        }
        self.active_oscillators = new_active_oscillators;
    }

    #[allow(clippy::too_many_arguments)]
    fn set_phase_inc_buffer_snap(
        &mut self,
        this: &Processor,
        num_samples: usize,
        reset_mask: PolyMask,
        trigger_sample: PolyInt,
        active_mask: PolyMask,
        snap_transpose: SnapFn,
        snap_buffer: &[f32],
    ) {
        let midi_track =
            PolyFloat::not_equal(this.input(MIDI_TRACK).at(0), PolyFloat::zero()).any_mask() != 0;
        let mut current_midi = self.midi_note;
        self.midi_note = PolyFloat::splat(NO_MIDI_TRACK_DEFAULT);
        if midi_track {
            self.midi_note = this.input(MIDI_NOTE).at(0);
        }

        let sample_inc = 1.0 / num_samples as f32;
        current_midi = utils::mask_load(current_midi, self.midi_note, reset_mask);
        let mut delta_midi = (self.midi_note - current_midi) * sample_inc;
        current_midi =
            utils::mask_load(utils::swap_voices(current_midi), current_midi, active_mask);
        delta_midi = utils::mask_load(utils::swap_voices(delta_midi), delta_midi, active_mask);

        let transpose_buffer = this.input_buffer(TRANSPOSE);
        let tune_buffer = this.input_buffer(TUNE);
        let phase_input = this.input_buffer(PHASE);

        let base_midi = current_midi + transpose_buffer[0].get() + tune_buffer[0].get();
        let base_frequency = utils::midi_note_to_frequency(base_midi);

        let sample_rate_scale = PolyFloat::splat(PHASE_MULT / this.sample_rate() as f32);
        let phase_scale = PolyFloat::splat(PHASE_MULT);

        for i in 0..num_samples {
            let shift_phase = utils::modulo(phase_input[i].get()) - 0.5;
            let phase = to_int(shift_phase * phase_scale);
            self.phase_buffer[i] =
                utils::mask_load_int(utils::swap_voices_int(phase), phase, active_mask);

            current_midi += delta_midi;

            let midi = snap_transpose(current_midi, transpose_buffer[i].get(), snap_buffer)
                + tune_buffer[i].get();
            let frequency = base_frequency * futils::midi_offset_to_ratio(midi - base_midi);
            let zero_mask =
                PolyInt::less_than(PolyInt::splat(i as u32), trigger_sample) & reset_mask;
            let result = (frequency * sample_rate_scale) & !zero_mask;
            self.phase_inc_buffer[i] =
                utils::mask_load(utils::swap_voices(result), result, active_mask);
        }
    }

    fn set_phase_inc_buffer(
        &mut self,
        this: &Processor,
        num_samples: usize,
        reset_mask: PolyMask,
        trigger_sample: PolyInt,
        active_mask: PolyMask,
    ) {
        let transpose_quantize = float_to_i32(this.input(TRANSPOSE_QUANTIZE).at(0).get(0));
        if !utils::is_transpose_snapping(transpose_quantize) {
            self.set_phase_inc_buffer_snap(
                this,
                num_samples,
                reset_mask,
                trigger_sample,
                active_mask,
                no_transpose_snap,
                &[],
            );
            return;
        }

        let mut snap_buffer = [0.0f32; NOTES_PER_OCTAVE as usize + 1];
        utils::fill_snap_buffer(transpose_quantize, &mut snap_buffer);
        let snap: SnapFn = if utils::is_transpose_quantize_global(transpose_quantize) {
            global_transpose_snap
        } else {
            local_transpose_snap
        };
        self.set_phase_inc_buffer_snap(
            this,
            num_samples,
            reset_mask,
            trigger_sample,
            active_mask,
            snap,
            &snap_buffer,
        );
    }

    fn process_oscillators(
        &mut self,
        this: &Processor,
        num_samples: usize,
        distortion: i32,
        phase_distort: PhaseDistortFn,
        window: WindowFn,
    ) {
        let active_voice_mask =
            PolyFloat::equal(this.input(ACTIVE_VOICES).at(0), PolyFloat::splat(1.0));
        let mut current_center_amplitude = self.center_amplitude;
        let mut current_detuned_amplitude = self.detuned_amplitude;
        self.set_amplitude(this);

        let reset_mask = this.get_reset_mask(RESET);
        let trigger_offset = this.input(RESET).trigger_offset();
        let retrigger_mask = this.get_reset_mask(RETRIGGER) & !reset_mask;

        current_center_amplitude =
            utils::mask_load(current_center_amplitude, self.center_amplitude, reset_mask);
        current_detuned_amplitude = utils::mask_load(
            current_detuned_amplitude,
            self.detuned_amplitude,
            reset_mask,
        );

        self.set_phase_inc_mults(this);
        self.set_phase_inc_buffer(
            this,
            num_samples,
            reset_mask,
            trigger_offset,
            active_voice_mask,
        );

        let mut current_distortion_phase = self.distortion_phase;
        self.distortion_phase = PolyFloat::zero();
        if uses_distortion_phase(distortion) {
            self.distortion_phase = this.input(DISTORTION_PHASE).at(0) - 0.5;
        }
        current_distortion_phase =
            utils::mask_load(current_distortion_phase, self.distortion_phase, reset_mask);

        self.voice_block.last_distortion_phase = to_int(current_distortion_phase * PHASE_MULT);
        self.voice_block.distortion_phase = to_int(self.distortion_phase * PHASE_MULT);

        let wave_buffer_mask = reset_mask | retrigger_mask;
        let buffer_phase_inc = self.phase_inc_buffer[num_samples - 1] * (1.0 / PHASE_MULT);
        if wave_buffer_mask.get(0) != 0 {
            self.set_wave_buffers(this, buffer_phase_inc, 0);
        }
        if wave_buffer_mask.get(2) != 0 {
            self.set_wave_buffers(this, buffer_phase_inc, 2);
        }

        if reset_mask.any_mask() != 0 {
            self.reset_at(this, reset_mask, trigger_offset);
        }

        if retrigger_mask.any_mask() != 0 {
            for i in 0..NUM_POLY_PHASE {
                self.from_phase_inc_mults[i] = utils::mask_load(
                    self.from_phase_inc_mults[i],
                    self.phase_inc_mults[i],
                    retrigger_mask,
                );
            }
        }

        self.voice_block.start_sample = 0;
        self.voice_block.total_samples = num_samples as i32;
        let num_buffer_samples = float_to_i32(WAVETABLE_FADE_TIME * this.sample_rate() as f32);
        if self.voice_block.num_buffer_samples != num_buffer_samples {
            self.voice_block.num_buffer_samples = num_buffer_samples;
            self.voice_block.current_buffer_sample = PolyInt::zero();
        }

        let shepard = self.voice_block.spectral_morph == sm::SHEPARD_TONE;
        if shepard {
            self.setup_shepard_wrap();
        } else {
            self.clear_shepard_wrap();
        }

        self.voice_block.current_buffer_sample &= active_voice_mask;
        while (self.voice_block.start_sample as usize) < num_samples {
            let remaining_fade_samples = PolyInt::splat(self.voice_block.num_buffer_samples as u32)
                - self.voice_block.current_buffer_sample;
            let min_remaining_fade_samples =
                (remaining_fade_samples.get(0) as i32).min(remaining_fade_samples.get(2) as i32);
            let samples =
                min_remaining_fade_samples.min(num_samples as i32 - self.voice_block.start_sample);
            self.voice_block.end_sample = self.voice_block.start_sample + samples;
            self.process_chunk(
                this,
                current_center_amplitude,
                current_detuned_amplitude,
                phase_distort,
                window,
            );

            self.voice_block.current_buffer_sample += samples as u32;
            self.voice_block.start_sample = self.voice_block.end_sample;

            let new_buffer_mask = PolyInt::equal(
                self.voice_block.current_buffer_sample,
                PolyInt::splat(self.voice_block.num_buffer_samples as u32),
            );
            if shepard && new_buffer_mask.any_mask() != 0 {
                self.do_shepard_wrap(new_buffer_mask, self.transpose_quantize);
            }

            if new_buffer_mask.get(0) != 0 {
                self.set_wave_buffers(this, buffer_phase_inc, 0);
            }
            if new_buffer_mask.get(2) != 0 {
                self.set_wave_buffers(this, buffer_phase_inc, 2);
            }
        }

        if reset_mask.any_mask() != 0 {
            this.clear_output_buffer_for_reset(reset_mask, RESET, RAW);
        }

        self.process_blend(this, num_samples, reset_mask);
    }

    fn process_chunk(
        &mut self,
        this: &Processor,
        mut current_center_amplitude: PolyFloat,
        mut current_detuned_amplitude: PolyFloat,
        phase_distort: PhaseDistortFn,
        window: WindowFn,
    ) {
        let active_channels = float_to_i32(this.input(ACTIVE_VOICES).at(0).sum());
        if active_channels < 2 {
            return;
        }

        let num_active_voices = active_channels / 2;
        let active_voice_mask =
            PolyFloat::equal(this.input(ACTIVE_VOICES).at(0), PolyFloat::splat(1.0));
        let start = self.voice_block.start_sample as usize;
        let num_samples = (self.voice_block.end_sample - self.voice_block.start_sample) as usize;

        let raw = this.output_buffer(RAW);
        let audio_out = &raw[start..start + num_samples];
        for sample in audio_out {
            sample.set(PolyFloat::zero());
        }

        let mut center_amplitude = self.center_amplitude;
        let mut detuned_amplitude = self.detuned_amplitude;

        if num_active_voices < 2 {
            let current_detuned_swap = utils::swap_voices(current_detuned_amplitude);
            current_detuned_amplitude = utils::mask_load(
                current_detuned_swap,
                current_detuned_amplitude,
                active_voice_mask,
            );
            current_center_amplitude = utils::mask_load(
                current_detuned_amplitude,
                current_center_amplitude,
                active_voice_mask,
            );

            let detuned_swap = utils::swap_voices(detuned_amplitude);
            detuned_amplitude =
                utils::mask_load(detuned_swap, detuned_amplitude, active_voice_mask);
            center_amplitude =
                utils::mask_load(detuned_amplitude, center_amplitude, active_voice_mask);

            let voice_block = &mut self.voice_block;
            let distortion_swap = utils::swap_voices(voice_block.distortion);
            voice_block.distortion =
                utils::mask_load(distortion_swap, voice_block.distortion, active_voice_mask);

            let last_distortion_swap = utils::swap_voices(voice_block.last_distortion);
            voice_block.last_distortion = utils::mask_load(
                last_distortion_swap,
                voice_block.last_distortion,
                active_voice_mask,
            );
            let swap_buffer_sample = utils::swap_voices_int(voice_block.current_buffer_sample);
            voice_block.current_buffer_sample = utils::mask_load_int(
                swap_buffer_sample,
                voice_block.current_buffer_sample,
                active_voice_mask,
            );
        }

        let modulation_output = self.voice_block_modulation.clone();
        let modulation_buffer = modulation_output.as_ref().map(|output| output.buffer());
        let modulation: &[Cell<PolyFloat>] = modulation_buffer.as_deref().unwrap_or(&[]);

        let num_phase_updates = (3 + num_active_voices * self.active_oscillators) / 4;
        for p in 1..num_phase_updates.max(1) as usize {
            self.load_voice_block(p, active_voice_mask);
            let phase = {
                let sources = self.sources(modulation);
                process_detuned(
                    &self.voice_block,
                    &sources,
                    audio_out,
                    phase_distort,
                    window,
                )
            };
            if num_active_voices < 2 {
                expand_and_write_voice_int(&mut self.phases[2 * p..], phase, active_voice_mask);
            } else {
                self.phases[p] = phase;
            }
        }

        self.load_voice_block(0, active_voice_mask);

        let sample_inc = 1.0 / self.voice_block.total_samples as f32;
        let delta_center_amplitude = (center_amplitude - current_center_amplitude) * sample_inc;
        let delta_detuned_amplitude = (detuned_amplitude - current_detuned_amplitude) * sample_inc;
        current_center_amplitude += delta_center_amplitude * self.voice_block.start_sample as f32;
        current_detuned_amplitude += delta_detuned_amplitude * self.voice_block.start_sample as f32;
        let amplitude = AmplitudeRamp {
            current_center: current_center_amplitude,
            delta_center: delta_center_amplitude,
            current_detuned: current_detuned_amplitude,
            delta_detuned: delta_detuned_amplitude,
        };
        let center_phase = {
            let sources = self.sources(modulation);
            process_center(
                &self.voice_block,
                &sources,
                audio_out,
                amplitude,
                phase_distort,
                window,
            )
        };

        if num_active_voices < 2 {
            expand_and_write_voice_int(&mut self.phases[..], center_phase, active_voice_mask);
            Self::convert_voice_channels(audio_out);
        } else {
            self.phases[0] = center_phase;
        }
    }

    fn sources<'a>(&'a self, modulation: &'a [Cell<PolyFloat>]) -> Sources<'a> {
        Sources {
            from: self
                .voice_block
                .from_buffers
                .map(|buffer| self.buffer_slice(buffer)),
            to: self
                .voice_block
                .to_buffers
                .map(|buffer| self.buffer_slice(buffer)),
            phase_inc: &self.phase_inc_buffer,
            phase: &self.phase_buffer,
            modulation,
        }
    }

    fn process_blend(&mut self, this: &Processor, num_samples: usize, reset_mask: PolyMask) {
        self.stereo_blend(this, num_samples, reset_mask);
        self.level_output(this, num_samples, reset_mask);
    }
}

impl ProcessorImpl for SynthOscillator {
    sas_dsp_core::processor_boilerplate!();

    fn reset(&mut self, this: &Processor, reset_mask: PolyMask) {
        self.reset_mask(this, reset_mask);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.processor_set_oversample_amount(oversample);
        let size = oversample as usize * MAX_BUFFER_SIZE;
        if self.phase_inc_buffer.len() < size {
            self.phase_inc_buffer = vec![PolyFloat::zero(); size];
        }
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let wavetable_version = self.wavetable.borrow().version();
        if wavetable_version != self.wavetable_version {
            self.wavetable_version = wavetable_version;
            self.reset_wavetable_buffers();
        }

        let active_voice = this.input(ACTIVE_VOICES).at(0);
        let left_active = active_voice.get(0) == 1.0;
        let right_active = active_voice.get(2) == 1.0;

        self.unison = float_to_i32(sas_simd::utils::clamp(
            this.input(UNISON_VOICES).at(0).get(0).round(),
            1.0,
            MAX_UNISON as f32,
        ));
        self.set_active_oscillators(self.unison + (self.unison % 2));

        let spectral_morph = float_to_i32(this.input(SPECTRAL_MORPH_TYPE).at(0).get(0));
        let distortion = float_to_i32(this.input(DISTORTION_TYPE).at(0).get(0));
        self.set_spectral_morph_values(this, spectral_morph);
        self.set_distortion_values(this, distortion);
        self.voice_block.spectral_morph = spectral_morph;

        let modulation_source = || match distortion {
            dt::FM_OSCILLATOR_B | dt::RM_OSCILLATOR_B => self.second_mod_oscillator.clone(),
            dt::FM_SAMPLE | dt::RM_SAMPLE => self.sample.clone(),
            _ => self.first_mod_oscillator.clone(),
        };

        match distortion {
            dt::SYNC => self.process_oscillators(
                this,
                num_samples,
                distortion,
                sync_phase,
                pass_through_window,
            ),
            dt::FORMANT => {
                self.process_oscillators(this, num_samples, distortion, sync_phase, half_sin_window)
            }
            dt::QUANTIZE => self.process_oscillators(
                this,
                num_samples,
                distortion,
                quantize_phase,
                pass_through_window,
            ),
            dt::BEND => self.process_oscillators(
                this,
                num_samples,
                distortion,
                bend_phase,
                pass_through_window,
            ),
            dt::SQUEEZE => self.process_oscillators(
                this,
                num_samples,
                distortion,
                squeeze_phase,
                pass_through_window,
            ),
            dt::PULSE_WIDTH => self.process_oscillators(
                this,
                num_samples,
                distortion,
                pulse_width_phase,
                pulse_width_window,
            ),
            dt::FM_OSCILLATOR_A | dt::FM_OSCILLATOR_B | dt::FM_SAMPLE => {
                self.voice_block_modulation = modulation_source();
                let distort: PhaseDistortFn = if left_active && right_active {
                    fm_phase
                } else if left_active {
                    fm_phase_left
                } else {
                    fm_phase_right
                };
                self.process_oscillators(
                    this,
                    num_samples,
                    distortion,
                    distort,
                    pass_through_window,
                );
            }
            dt::RM_OSCILLATOR_A | dt::RM_OSCILLATOR_B | dt::RM_SAMPLE => {
                self.voice_block_modulation = modulation_source();
                let window: WindowFn = if left_active && right_active {
                    rm_window
                } else if left_active {
                    rm_window_left
                } else {
                    rm_window_right
                };
                self.process_oscillators(this, num_samples, distortion, pass_through_phase, window);
            }
            _ => self.process_oscillators(
                this,
                num_samples,
                distortion,
                pass_through_phase,
                pass_through_window,
            ),
        }
    }
}
