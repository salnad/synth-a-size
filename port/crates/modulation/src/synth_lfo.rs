// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::SynthLfo`: the shape LFO (a [`LineGenerator`] played back with
//! sync, envelope, loop, delay, fade, smoothing and stereo phase).
//!
//! Ports `vital/src/synthesis/modulators/synth_lfo.h` and `synth_lfo.cpp`.
//! The line shape is shared (upstream holds a `LineGenerator*` owned by the
//! synth), so the kernel holds it as `Rc<RefCell<LineGenerator>>`; edit it
//! between blocks, never while processing.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use sas_common::LineGenerator;
use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_simd::common::VoiceEvent;
use sas_simd::conversions::float_to_i32;
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyInt, PolyMask};

#[inline(always)]
fn splat(value: f32) -> PolyFloat {
    PolyFloat::splat(value)
}

/// `SynthLfo::LfoState`.
#[derive(Clone, Copy, Default)]
struct LfoState {
    delay_time_passed: PolyFloat,
    fade_amplitude: PolyFloat,
    smooth_value: PolyFloat,
    #[allow(dead_code)]
    fade_amount: PolyFloat,
    offset: PolyFloat,
    phase: PolyFloat,
}

/// `getValueAtPhase(buffer, resolution, max_index, phase)`: Catmull-Rom
/// lookup into a line generator's cubic interpolation buffer.
#[inline(always)]
pub(crate) fn value_at_phase(
    buffer: &[f32],
    resolution: PolyFloat,
    max_index: PolyInt,
    phase: PolyFloat,
) -> PolyFloat {
    let boost = utils::clamp(phase * resolution, 0.0, resolution);
    let indices = utils::clamp_int(utils::to_int(boost), PolyInt::zero(), max_index);
    let t = boost - utils::to_float(indices);

    let interpolation_matrix = utils::get_catmull_interpolation_matrix(t);
    let mut value_matrix = utils::get_value_matrix(buffer, indices);
    value_matrix.transpose();

    interpolation_matrix.multiply_and_sum_rows(&value_matrix)
}

/// Kernel of `vital::SynthLfo`.
#[derive(Clone)]
pub struct SynthLfo {
    was_control_rate: bool,
    control_rate_state: LfoState,
    audio_rate_state: LfoState,

    held_mask: PolyMask,
    trigger_sample: PolyInt,
    trigger_delay: PolyFloat,
    source: Rc<RefCell<LineGenerator>>,

    sync_seconds: Rc<Cell<f64>>,
}

/// Per-block constants shared by the audio-rate loops.
struct AudioRateSetup {
    resolution: PolyFloat,
    max_index: PolyInt,
    delta_phase: PolyFloat,
    delay_time: PolyFloat,
    delay_time_passed: PolyFloat,
    current_amplitude: PolyFloat,
    tick_time: PolyFloat,
    fade_increase: PolyFloat,
    smooth_mult: PolyFloat,
    current_value: PolyFloat,
}

impl SynthLfo {
    pub const FREQUENCY: usize = 0;
    pub const PHASE: usize = 1;
    pub const AMPLITUDE: usize = 2;
    pub const NOTE_TRIGGER: usize = 3;
    pub const SYNC_TYPE: usize = 4;
    pub const SMOOTH_MODE: usize = 5;
    pub const FADE: usize = 6;
    pub const SMOOTH_TIME: usize = 7;
    pub const STEREO_PHASE: usize = 8;
    pub const DELAY: usize = 9;
    pub const NOTE_COUNT: usize = 10;
    pub const NUM_INPUTS: usize = 11;

    pub const VALUE: usize = 0;
    pub const OSC_PHASE: usize = 1;
    pub const OSC_FREQUENCY: usize = 2;
    pub const NUM_OUTPUTS: usize = 3;

    // SyncType
    pub const TRIGGER: i32 = 0;
    pub const SYNC: i32 = 1;
    pub const ENVELOPE: i32 = 2;
    pub const SUSTAIN_ENVELOPE: i32 = 3;
    pub const LOOP_POINT: i32 = 4;
    pub const LOOP_HOLD: i32 = 5;
    pub const NUM_SYNC_TYPES: i32 = 6;

    // Sync options
    pub const TIME: i32 = 0;
    pub const TEMPO: i32 = 1;
    pub const DOTTED_TEMPO: i32 = 2;
    pub const TRIPLET_TEMPO: i32 = 3;
    pub const KEYTRACK: i32 = 4;
    pub const NUM_SYNC_OPTIONS: i32 = 5;

    pub const MAX_POWER: f32 = 20.0;
    pub const HALF_LIFE_RATIO: f32 = 0.2;
    pub const MIN_HALF_LIFE: f32 = 0.0002;

    /// `new SynthLfo(source)`.
    pub fn create(source: Rc<RefCell<LineGenerator>>) -> ProcessorRef {
        Processor::new(
            SynthLfo {
                was_control_rate: true,
                control_rate_state: LfoState::default(),
                audio_rate_state: LfoState::default(),
                held_mask: PolyMask::zero(),
                trigger_sample: PolyInt::zero(),
                trigger_delay: PolyFloat::zero(),
                source,
                sync_seconds: Rc::new(Cell::new(0.0)),
            },
            Self::NUM_INPUTS,
            Self::NUM_OUTPUTS,
            false,
            1,
        )
    }

    /// `correctToTime(seconds)`: the song time that `kSync` phase-locks to.
    pub fn correct_to_time(&mut self, seconds: f64) {
        self.sync_seconds.set(seconds);
    }

    /// `getValueAtPhase(phase)` on the current shape.
    pub fn value_at_phase(&self, phase: PolyFloat) -> PolyFloat {
        let source = self.source.borrow();
        let resolution = source.resolution() as i32;
        value_at_phase(
            source.cubic_interpolation_buffer(),
            splat(resolution as f32),
            PolyInt::splat((resolution - 1) as u32),
            phase,
        )
    }

    /// `getReleaseMask()`.
    fn release_mask(this: &Processor) -> PolyMask {
        let trigger = this.input(Self::NOTE_TRIGGER);
        trigger.trigger_mask()
            & PolyFloat::equal(
                trigger.trigger_value(),
                splat(VoiceEvent::VoiceOff.as_f32()),
            )
    }

    fn sync_type(this: &Processor) -> i32 {
        float_to_i32(this.input(Self::SYNC_TYPE).at(0).get(0))
    }

    fn smooth_mode(this: &Processor) -> bool {
        this.input(Self::SMOOTH_MODE).at(0).get(0) != 0.0
    }

    fn process_trigger(&mut self, this: &Processor) {
        let sync_type = Self::sync_type(this);
        let reset_mask = this.get_reset_mask(Self::NOTE_TRIGGER);
        let release_mask = Self::release_mask(this);
        self.held_mask = (self.held_mask | reset_mask) & !release_mask;
        let trigger_offset = this.input(Self::NOTE_TRIGGER).trigger_offset();
        self.trigger_sample = utils::mask_load_int(
            self.trigger_sample,
            trigger_offset,
            reset_mask | release_mask,
        );

        for state in [&mut self.control_rate_state, &mut self.audio_rate_state] {
            state.delay_time_passed = utils::mask_load(state.delay_time_passed, 0.0, reset_mask);
            state.fade_amplitude = utils::mask_load(state.fade_amplitude, 0.0, reset_mask);
            state.smooth_value = utils::mask_load(state.smooth_value, 0.0, reset_mask);
        }

        let sample_time = 1.0f32 / this.sample_rate() as f32;
        let trigger_delay = utils::to_float(trigger_offset) * sample_time;
        self.trigger_delay = utils::mask_load(self.trigger_delay, trigger_delay, reset_mask);

        if reset_mask.any_mask() != 0 {
            let frequency = this.input(Self::FREQUENCY).at(0);

            if sync_type == Self::SYNC {
                let sync_phase =
                    utils::get_cycle_offset_from_seconds(self.sync_seconds.get(), frequency);
                self.control_rate_state.offset =
                    utils::mask_load(self.control_rate_state.offset, sync_phase, reset_mask);
                self.audio_rate_state.offset =
                    utils::mask_load(self.audio_rate_state.offset, sync_phase, reset_mask);
            } else {
                self.control_rate_state.offset =
                    utils::mask_load(self.control_rate_state.offset, 0.0, reset_mask);

                let sample_offset = utils::to_float(trigger_offset) & reset_mask;
                let offset_start = frequency * sample_offset * sample_time;
                self.audio_rate_state.offset =
                    utils::mask_load(self.audio_rate_state.offset, -offset_start, reset_mask);
            }
        }
    }

    fn process_control_rate(&mut self, this: &Processor, num_samples: usize) {
        let delay_time = this.input(Self::DELAY).at(0);

        let tick_time = 1.0f32 / this.sample_rate() as f32;
        let mut time_passed = splat(tick_time * num_samples as f32);
        let state = &mut self.control_rate_state;
        state.delay_time_passed += time_passed;
        time_passed = utils::clamp(state.delay_time_passed - delay_time, 0.0, time_passed);

        let stereo_phase = this.input(Self::STEREO_PHASE).at(0);
        let phase = this.input(Self::PHASE).at(0) + stereo_phase * PolyFloat::stereo(0.5, -0.5);
        let frequency = this.input(Self::FREQUENCY).at(0);
        let current_offset = state.offset;
        state.offset += frequency * time_passed;

        let mut phased_offset = PolyFloat::zero();
        let sync_type = Self::sync_type(this);
        if sync_type == Self::ENVELOPE {
            state.offset = utils::min(state.offset, splat(1.0));
            phased_offset = utils::min(current_offset + phase, splat(1.0));
        } else if sync_type == Self::SUSTAIN_ENVELOPE {
            state.offset = utils::min(state.offset, utils::mask_load(1.0, phase, self.held_mask));
            phased_offset = current_offset;
        } else if sync_type == Self::TRIGGER || sync_type == Self::SYNC {
            state.offset = utils::modulo(state.offset);
            phased_offset = utils::modulo(current_offset + phase);
        } else if sync_type == Self::LOOP_POINT {
            let over = PolyFloat::greater_than_or_equal(state.offset, splat(1.0));
            state.offset = utils::mask_load(state.offset, state.offset - 1.0 + phase, over);
            phased_offset = utils::min(current_offset, splat(1.0));
        } else if sync_type == Self::LOOP_HOLD {
            let over = self.held_mask & PolyFloat::greater_than_or_equal(state.offset, phase);
            state.offset = utils::min(
                utils::mask_load(state.offset, state.offset - phase, over),
                splat(1.0),
            );
            phased_offset = utils::mask_load(
                current_offset,
                utils::min(current_offset, phase),
                self.held_mask,
            );
        }

        let fade_time = this.input(Self::FADE).at(0);
        let fade_increase =
            time_passed / utils::max(utils::max(splat(tick_time), time_passed), fade_time);
        state.fade_amplitude = utils::min(state.fade_amplitude + fade_increase, splat(1.0));
        state.fade_amplitude = utils::mask_load(
            state.fade_amplitude,
            1.0,
            PolyFloat::equal(fade_time, splat(0.0)),
        );

        let fade_amplitude = state.fade_amplitude;
        let smooth_value = state.smooth_value;
        let value = self.value_at_phase(phased_offset);
        let mut result;
        if Self::smooth_mode(this) {
            let half_life = this.input(Self::SMOOTH_TIME).at(0) * Self::HALF_LIFE_RATIO;
            let smooth_mask = PolyFloat::greater_than(half_life, splat(Self::MIN_HALF_LIFE));
            let exponent = -time_passed / utils::max(half_life, splat(Self::MIN_HALF_LIFE));
            let mut ratio = futils::exp2(exponent);
            ratio &= smooth_mask;
            result = utils::interpolate(value, smooth_value, ratio);
            self.control_rate_state.smooth_value = result;
        } else {
            let start_value = self.value_at_phase(phase);
            result = utils::interpolate(start_value, value, fade_amplitude);
        }

        result = utils::clamp(result, -1.0, 1.0);
        let value_output = this.output(Self::VALUE);
        value_output.set_trigger_value(result);
        if this.is_control_rate() {
            value_output.set_at(0, result);
        }
        this.output(Self::OSC_PHASE).set_at(
            0,
            utils::encode_phase_and_voice(phased_offset, this.input(Self::NOTE_COUNT).at(0)),
        );
        this.output(Self::OSC_FREQUENCY).set_at(0, frequency);
    }

    /// The common head of the five `processAudioRate*` loops.
    fn audio_rate_setup(
        &self,
        this: &Processor,
        num_samples: usize,
        current_phase: PolyFloat,
    ) -> AudioRateSetup {
        let lfo_resolution = self.source.borrow().resolution() as i32;
        let resolution = splat(lfo_resolution as f32);
        let max_index = PolyInt::splat((lfo_resolution - 1) as u32);
        let delta_phase =
            (self.audio_rate_state.phase - current_phase) * (1.0f32 / num_samples as f32);

        let fade_time = this.input(Self::FADE).at(0);
        let delay_time = this.input(Self::DELAY).at(0) + self.trigger_delay;
        let delay_time_passed = self.audio_rate_state.delay_time_passed;
        let mut current_amplitude = self.audio_rate_state.fade_amplitude;
        let tick_time = splat(1.0f32 / this.sample_rate() as f32);
        let fade_increase = tick_time / utils::max(tick_time, fade_time);

        let mut smooth_mult = PolyFloat::zero();
        if Self::smooth_mode(this) {
            let half_life = this.input(Self::SMOOTH_TIME).at(0) * Self::HALF_LIFE_RATIO;
            let smooth_mask = PolyFloat::greater_than(half_life, splat(Self::MIN_HALF_LIFE));
            let exponent = -tick_time / utils::max(half_life, splat(Self::MIN_HALF_LIFE));
            smooth_mult = futils::exp2(exponent);
            smooth_mult &= smooth_mask;
            current_amplitude = splat(1.0);
        }

        AudioRateSetup {
            resolution,
            max_index,
            delta_phase,
            delay_time,
            delay_time_passed,
            current_amplitude,
            tick_time,
            fade_increase,
            smooth_mult,
            current_value: self.audio_rate_state.smooth_value,
        }
    }

    fn store_audio_rate_state(&mut self, setup: &AudioRateSetup) {
        self.audio_rate_state.smooth_value = setup.current_value;
        self.audio_rate_state.fade_amplitude = setup.current_amplitude;
        self.audio_rate_state.delay_time_passed = setup.delay_time_passed;
    }

    fn process_audio_rate_envelope(
        &mut self,
        this: &Processor,
        num_samples: usize,
        mut current_phase: PolyFloat,
        mut current_offset: PolyFloat,
        delta_offset: PolyFloat,
    ) -> PolyFloat {
        let mut s = self.audio_rate_setup(this, num_samples, current_phase);
        let source = self.source.clone();
        let source = source.borrow();
        let lfo_buffer = source.cubic_interpolation_buffer();
        let dest = this.output_buffer(Self::VALUE);
        let mut phased_offset = PolyFloat::zero();

        for sample in dest.iter().take(num_samples) {
            s.delay_time_passed += s.tick_time;
            let past_delay_mask =
                PolyFloat::greater_than_or_equal(s.delay_time_passed, s.delay_time);
            s.current_amplitude = utils::clamp(
                s.current_amplitude + (s.fade_increase & past_delay_mask),
                0.0,
                1.0,
            );
            phased_offset = utils::min(current_offset + current_phase, splat(1.0));
            let value = value_at_phase(lfo_buffer, s.resolution, s.max_index, phased_offset);
            s.current_value = utils::interpolate(value, s.current_value, s.smooth_mult);
            sample.set(s.current_amplitude * s.current_value);

            current_offset = utils::min(
                current_offset + (delta_offset & past_delay_mask),
                splat(1.0),
            );
            current_phase += s.delta_phase;
        }

        self.store_audio_rate_state(&s);
        self.audio_rate_state.offset = utils::min(current_offset, splat(1.0));
        phased_offset
    }

    fn process_audio_rate_sustain_envelope(
        &mut self,
        this: &Processor,
        num_samples: usize,
        mut current_phase: PolyFloat,
        mut current_offset: PolyFloat,
        delta_offset: PolyFloat,
    ) -> PolyFloat {
        let mut s = self.audio_rate_setup(this, num_samples, current_phase);
        let source = self.source.clone();
        let source = source.borrow();
        let lfo_buffer = source.cubic_interpolation_buffer();
        let dest = this.output_buffer(Self::VALUE);

        let mut current_hold_mask = PolyMask::zero();
        let held_mask = self.held_mask;
        let trigger_sample = self.trigger_sample;

        for (i, sample) in dest.iter().take(num_samples).enumerate() {
            s.delay_time_passed += s.tick_time;
            let past_delay_mask =
                PolyFloat::greater_than_or_equal(s.delay_time_passed, s.delay_time);
            s.current_amplitude = utils::clamp(
                s.current_amplitude + (s.fade_increase & past_delay_mask),
                0.0,
                1.0,
            );

            current_hold_mask = utils::mask_load_int(
                current_hold_mask,
                held_mask,
                PolyInt::equal(PolyInt::splat(i as u32), trigger_sample),
            );
            let max = utils::mask_load(1.0, current_phase, current_hold_mask);
            let value = value_at_phase(lfo_buffer, s.resolution, s.max_index, current_offset);
            s.current_value = utils::interpolate(value, s.current_value, s.smooth_mult);
            sample.set(s.current_amplitude * s.current_value);

            current_offset = utils::min(current_offset + (delta_offset & past_delay_mask), max);
            current_phase += s.delta_phase;
        }

        self.store_audio_rate_state(&s);
        let last_max = utils::mask_load(1.0, current_phase, current_hold_mask);
        self.audio_rate_state.offset = utils::min(current_offset, last_max);
        current_offset
    }

    fn process_audio_rate_lfo(
        &mut self,
        this: &Processor,
        num_samples: usize,
        mut current_phase: PolyFloat,
        mut current_offset: PolyFloat,
        delta_offset: PolyFloat,
    ) -> PolyFloat {
        let mut s = self.audio_rate_setup(this, num_samples, current_phase);
        let source = self.source.clone();
        let source = source.borrow();
        let lfo_buffer = source.cubic_interpolation_buffer();

        let delaying_mask = PolyFloat::greater_than(s.delay_time, s.delay_time_passed);
        let dest = this.output_buffer(Self::VALUE);
        let mut phased_offset = PolyFloat::zero();

        for sample in dest.iter().take(num_samples) {
            s.delay_time_passed += s.tick_time;
            let past_delay_mask =
                PolyFloat::greater_than_or_equal(s.delay_time_passed, s.delay_time);
            s.current_amplitude = utils::clamp(
                s.current_amplitude + (s.fade_increase & past_delay_mask),
                0.0,
                1.0,
            );

            phased_offset = utils::modulo(current_offset + current_phase);
            let value = value_at_phase(lfo_buffer, s.resolution, s.max_index, phased_offset);
            s.current_value = utils::interpolate(value, s.current_value, s.smooth_mult);
            sample.set(s.current_amplitude * s.current_value);

            current_offset = utils::modulo(current_offset + (delta_offset & past_delay_mask));
            current_phase += s.delta_phase;
        }

        self.store_audio_rate_state(&s);
        let undelayed_offset =
            utils::modulo(self.audio_rate_state.offset + delta_offset * num_samples as f32);
        self.audio_rate_state.offset =
            utils::mask_load(undelayed_offset, current_offset, delaying_mask);
        phased_offset
    }

    fn process_audio_rate_loop_point(
        &mut self,
        this: &Processor,
        num_samples: usize,
        mut current_phase: PolyFloat,
        mut current_offset: PolyFloat,
        delta_offset: PolyFloat,
    ) -> PolyFloat {
        let mut s = self.audio_rate_setup(this, num_samples, current_phase);
        let source = self.source.clone();
        let source = source.borrow();
        let lfo_buffer = source.cubic_interpolation_buffer();
        let dest = this.output_buffer(Self::VALUE);

        for sample in dest.iter().take(num_samples) {
            s.delay_time_passed += s.tick_time;
            let past_delay_mask =
                PolyFloat::greater_than_or_equal(s.delay_time_passed, s.delay_time);
            s.current_amplitude = utils::clamp(
                s.current_amplitude + (s.fade_increase & past_delay_mask),
                0.0,
                1.0,
            );

            current_offset += delta_offset & past_delay_mask;
            let over = PolyFloat::greater_than_or_equal(current_offset, splat(1.0));
            current_offset = utils::min(
                utils::mask_load(current_offset, current_offset - 1.0 + current_phase, over),
                splat(1.0),
            );
            let value = value_at_phase(lfo_buffer, s.resolution, s.max_index, current_offset);
            s.current_value = utils::interpolate(value, s.current_value, s.smooth_mult);
            sample.set(s.current_amplitude * s.current_value);
            current_phase += s.delta_phase;
        }

        self.store_audio_rate_state(&s);
        self.audio_rate_state.offset = current_offset;
        current_offset
    }

    fn process_audio_rate_loop_hold(
        &mut self,
        this: &Processor,
        num_samples: usize,
        mut current_phase: PolyFloat,
        mut current_offset: PolyFloat,
        delta_offset: PolyFloat,
    ) -> PolyFloat {
        let mut s = self.audio_rate_setup(this, num_samples, current_phase);
        let source = self.source.clone();
        let source = source.borrow();
        let lfo_buffer = source.cubic_interpolation_buffer();
        let dest = this.output_buffer(Self::VALUE);
        let held_mask = self.held_mask;

        for sample in dest.iter().take(num_samples) {
            s.delay_time_passed += s.tick_time;
            let past_delay_mask =
                PolyFloat::greater_than_or_equal(s.delay_time_passed, s.delay_time);
            s.current_amplitude = utils::clamp(
                s.current_amplitude + (s.fade_increase & past_delay_mask),
                0.0,
                1.0,
            );

            current_offset += delta_offset & past_delay_mask;
            let over = held_mask & PolyFloat::greater_than_or_equal(current_offset, current_phase);
            current_offset = utils::min(
                utils::mask_load(current_offset, current_offset - current_phase, over),
                splat(1.0),
            );
            let value = value_at_phase(lfo_buffer, s.resolution, s.max_index, current_offset);
            s.current_value = utils::interpolate(value, s.current_value, s.smooth_mult);
            sample.set(s.current_amplitude * s.current_value);
            current_phase += s.delta_phase;
        }

        self.store_audio_rate_state(&s);
        self.audio_rate_state.offset = current_offset;
        current_offset
    }

    fn process_audio_rate(&mut self, this: &Processor, num_samples: usize) {
        let stereo_phase = this.input(Self::STEREO_PHASE).at(0);
        let mut current_phase = self.audio_rate_state.phase;
        self.audio_rate_state.phase =
            this.input(Self::PHASE).at(0) + stereo_phase * PolyFloat::stereo(0.5, -0.5);

        let sync_type = Self::sync_type(this);
        let reset_mask = this.get_reset_mask(Self::NOTE_TRIGGER);
        if sync_type == Self::SUSTAIN_ENVELOPE {
            current_phase = utils::mask_load(current_phase, 0.0, reset_mask);
        } else {
            current_phase =
                utils::mask_load(current_phase, self.audio_rate_state.phase, reset_mask);
        }

        let frequency = this.input(Self::FREQUENCY).at(0);
        let tick_time = 1.0f32 / this.sample_rate() as f32;
        let delta_offset = frequency * tick_time;

        let mut output_phase = PolyFloat::zero();
        let offset = utils::max(splat(0.0), self.audio_rate_state.offset);
        if sync_type == Self::ENVELOPE {
            output_phase = self.process_audio_rate_envelope(
                this,
                num_samples,
                current_phase,
                offset,
                delta_offset,
            );
        } else if sync_type == Self::SUSTAIN_ENVELOPE {
            output_phase = self.process_audio_rate_sustain_envelope(
                this,
                num_samples,
                current_phase,
                offset,
                delta_offset,
            );
        } else if sync_type == Self::TRIGGER || sync_type == Self::SYNC {
            output_phase =
                self.process_audio_rate_lfo(this, num_samples, current_phase, offset, delta_offset);
        } else if sync_type == Self::LOOP_POINT {
            output_phase = self.process_audio_rate_loop_point(
                this,
                num_samples,
                current_phase,
                offset,
                delta_offset,
            );
        } else if sync_type == Self::LOOP_HOLD {
            output_phase = self.process_audio_rate_loop_hold(
                this,
                num_samples,
                current_phase,
                offset,
                delta_offset,
            );
        }

        this.output(Self::OSC_PHASE).set_at(
            0,
            utils::encode_phase_and_voice(output_phase, this.input(Self::NOTE_COUNT).at(0)),
        );
        this.output(Self::OSC_FREQUENCY).set_at(0, frequency);
    }
}

impl ProcessorImpl for SynthLfo {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let control_rate = this.is_control_rate();
        if self.was_control_rate && !control_rate {
            self.audio_rate_state = self.control_rate_state;
        }
        self.was_control_rate = control_rate;

        self.process_trigger(this);

        if !control_rate {
            self.process_audio_rate(this, num_samples);
        }
        self.process_control_rate(this, num_samples);
    }
}
