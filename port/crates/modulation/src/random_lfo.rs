// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::RandomLfo`: the random LFO (perlin, sample and hold, sine
//! interpolation, Lorenz attractor).
//!
//! Ports `vital/src/synthesis/modulators/random_lfo.h` and
//! `random_lfo.cpp`. Values come from the engine RNG replica
//! ([`sas_common::RandomGenerator`]), seeded from the global counter at
//! construction and again for every clone, as upstream's
//! `RandomGenerator` copy constructor does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use sas_common::RandomGenerator;
use sas_dsp_core::constants;
use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyInt, PolyMask};

const LORENZ_INITIAL_1: f32 = 0.0;
const LORENZ_INITIAL_2: f32 = 0.0;
const LORENZ_INITIAL_3: f32 = 37.6;
const LORENZ_A: f32 = 10.0;
const LORENZ_B: f32 = 28.0;
const LORENZ_C: f32 = 8.0 / 3.0;
const LORENZ_SIZE: f32 = 40.0;
const LORENZ_SCALE: f32 = 1.0 / LORENZ_SIZE;

#[inline(always)]
fn splat(value: f32) -> PolyFloat {
    PolyFloat::splat(value)
}

/// `RandomLfo::RandomState`.
#[derive(Clone, Copy)]
pub struct RandomState {
    offset: PolyFloat,
    last_random_value: PolyFloat,
    next_random_value: PolyFloat,
    state1: PolyFloat,
    state2: PolyFloat,
    state3: PolyFloat,
}

impl Default for RandomState {
    fn default() -> Self {
        RandomState {
            offset: PolyFloat::zero(),
            last_random_value: PolyFloat::zero(),
            next_random_value: PolyFloat::zero(),
            state1: splat(0.1),
            state2: PolyFloat::zero(),
            state3: PolyFloat::zero(),
        }
    }
}

/// Which state `process` advances: the voice's own, or the one shared by
/// all clones (tempo sync).
#[derive(Clone, Copy)]
enum StateSlot {
    Own,
    Shared,
}

/// Kernel of `vital::RandomLfo`.
pub struct RandomLfo {
    state: RandomState,
    shared_state: Rc<RefCell<RandomState>>,

    random_generator: RandomGenerator,
    last_value: PolyFloat,

    sync_seconds: Rc<Cell<f64>>,
    last_sync: Rc<Cell<f64>>,
}

impl Clone for RandomLfo {
    /// The implicit copy constructor: shared pointers are shared, and the
    /// `RandomGenerator` copy takes a new seed from the global counter.
    fn clone(&self) -> Self {
        RandomLfo {
            state: self.state,
            shared_state: self.shared_state.clone(),
            random_generator: RandomGenerator::from_global_counter(
                self.random_generator.min(),
                self.random_generator.max(),
            ),
            last_value: self.last_value,
            sync_seconds: self.sync_seconds.clone(),
            last_sync: self.last_sync.clone(),
        }
    }
}

impl Default for RandomLfo {
    /// `RandomLfo()`: takes one seed from the global counter.
    fn default() -> Self {
        RandomLfo {
            state: RandomState::default(),
            shared_state: Rc::new(RefCell::new(RandomState::default())),
            random_generator: RandomGenerator::from_global_counter(-1.0, 1.0),
            last_value: PolyFloat::zero(),
            sync_seconds: Rc::new(Cell::new(0.0)),
            last_sync: Rc::new(Cell::new(0.0)),
        }
    }
}

impl RandomLfo {
    pub const FREQUENCY: usize = 0;
    pub const AMPLITUDE: usize = 1;
    pub const RESET: usize = 2;
    pub const SYNC: usize = 3;
    pub const STYLE: usize = 4;
    pub const RANDOM_TYPE: usize = 5;
    pub const STEREO: usize = 6;
    pub const NUM_INPUTS: usize = 7;

    // RandomType
    pub const PERLIN: i32 = 0;
    pub const SAMPLE_AND_HOLD: i32 = 1;
    pub const SIN_INTERPOLATE: i32 = 2;
    pub const LORENZ_ATTRACTOR: i32 = 3;
    pub const NUM_STYLES: i32 = 4;

    /// `new RandomLfo()`.
    pub fn create() -> ProcessorRef {
        Processor::new(RandomLfo::default(), Self::NUM_INPUTS, 1, false, 1)
    }

    /// `correctToTime(seconds)`: the song time that tempo sync follows.
    pub fn correct_to_time(&mut self, seconds: f64) {
        self.sync_seconds.set(seconds);
    }

    fn poly_next(&mut self, mono: bool) -> PolyFloat {
        PolyFloat::from_array(if mono {
            self.random_generator.poly_voice_next()
        } else {
            self.random_generator.poly_next()
        })
    }

    fn with_state<R>(
        &mut self,
        slot: StateSlot,
        f: impl FnOnce(&mut Self, &mut RandomState) -> R,
    ) -> R {
        match slot {
            StateSlot::Own => {
                let mut state = self.state;
                let result = f(self, &mut state);
                self.state = state;
                result
            }
            StateSlot::Shared => {
                let shared = self.shared_state.clone();
                let mut state = *shared.borrow();
                let result = f(self, &mut state);
                *shared.borrow_mut() = state;
                result
            }
        }
    }

    fn do_reset(
        &mut self,
        this: &Processor,
        state: &mut RandomState,
        mono: bool,
        frequency: PolyFloat,
    ) {
        let reset_mask = this.get_reset_mask(Self::RESET);
        if reset_mask.any_mask() == 0 || this.input(Self::SYNC).at(0).get(0) != 0.0 {
            return;
        }

        let sample_offset = utils::to_float(this.input(Self::RESET).trigger_offset());
        let start_offset = frequency * (1.0f32 / this.sample_rate() as f32) * sample_offset;
        state.offset = utils::mask_load(state.offset, -start_offset, reset_mask);
        let from_random = self.poly_next(mono);
        let to_random = self.poly_next(mono);

        state.last_random_value =
            utils::mask_load(state.last_random_value, from_random, reset_mask);
        state.next_random_value = utils::mask_load(state.next_random_value, to_random, reset_mask);
        self.last_value = utils::mask_load(
            self.last_value,
            state.last_random_value * 0.5 + 0.5,
            reset_mask,
        );
    }

    fn update_phase(
        &mut self,
        this: &Processor,
        state: &mut RandomState,
        num_samples: usize,
    ) -> PolyInt {
        let frequency = this.input(Self::FREQUENCY).at(0);
        let phase_delta = frequency * (1.0f32 / this.sample_rate() as f32) * num_samples as f32;
        let mono = this.input(Self::STEREO).at(0).get(0) == 0.0;
        let mut new_random_mask = PolyMask::zero();

        if this.input(Self::SYNC).at(0).get(0) != 0.0 {
            if self.last_sync.get() != self.sync_seconds.get() {
                let new_offset =
                    utils::get_cycle_offset_from_seconds(self.sync_seconds.get(), frequency);
                new_random_mask = PolyFloat::less_than(new_offset, splat(0.5))
                    & PolyFloat::greater_than_or_equal(state.offset, splat(0.5));
                state.offset = new_offset;
            }
        } else {
            let frequency = this.input(Self::FREQUENCY).at(0);
            self.do_reset(this, state, mono, frequency);

            state.offset += phase_delta;
            new_random_mask = PolyFloat::greater_than_or_equal(state.offset, splat(1.0));
            state.offset = utils::modulo(state.offset);
        }

        if new_random_mask.any_mask() != 0 {
            state.last_random_value = utils::mask_load(
                state.last_random_value,
                state.next_random_value,
                new_random_mask,
            );
            let next_random = self.poly_next(mono);

            state.next_random_value =
                utils::mask_load(state.next_random_value, next_random, new_random_mask);

            let delta = utils::mask_load(
                phase_delta,
                1.0,
                PolyFloat::less_than_or_equal(phase_delta, splat(0.0)),
            );
            let samples_to_wrap = state.offset / delta;
            return utils::round_to_int(samples_to_wrap);
        }

        PolyInt::zero()
    }

    fn process_state(&mut self, this: &Processor, slot: StateSlot, num_samples: usize) {
        let style = this.input(Self::STYLE).at(0).get(0);
        let random_type = style.max(0.0).min(Self::NUM_STYLES as f32 - 1.0).round() as i32;

        if random_type == Self::LORENZ_ATTRACTOR {
            self.with_state(slot, |lfo, state| {
                lfo.process_lorenz_attractor(this, state, num_samples)
            });
            return;
        }
        if random_type == Self::SAMPLE_AND_HOLD {
            self.with_state(slot, |lfo, state| {
                lfo.process_sample_and_hold(this, state, num_samples)
            });
            return;
        }

        let state = self.with_state(slot, |lfo, state| {
            lfo.update_phase(this, state, num_samples);
            *state
        });

        let mut result = match random_type {
            Self::PERLIN => utils::perlin_interpolate(
                state.last_random_value,
                state.next_random_value,
                state.offset,
            ),
            Self::SIN_INTERPOLATE => futils::sin_interpolate(
                state.last_random_value,
                state.next_random_value,
                state.offset,
            ),
            _ => PolyFloat::zero(),
        };

        result = result * 0.5 + 0.5;
        let output = this.output(0);
        output.set_trigger_value(result);

        let dest = output.buffer();
        if !this.is_control_rate() {
            let mut current_value = self.last_value;
            let delta_value = (result - current_value) * (1.0f32 / num_samples as f32);
            for sample in dest.iter().take(num_samples) {
                current_value += delta_value;
                sample.set(current_value);
            }
        } else {
            dest[0].set(result);
        }

        self.last_value = result;
    }

    fn process_sample_and_hold(
        &mut self,
        this: &Processor,
        state: &mut RandomState,
        num_samples: usize,
    ) {
        let last_random_value = state.last_random_value * 0.5 + 0.5;
        let sample_change = self.update_phase(this, state, num_samples);
        let current_random_value = state.last_random_value * 0.5 + 0.5;

        let output = this.output(0);
        let dest = output.buffer();
        if !this.is_control_rate() {
            for (i, sample) in dest.iter().take(num_samples).enumerate() {
                let over = PolyInt::greater_than(PolyInt::splat(i as u32), sample_change);
                sample.set(utils::mask_load(
                    last_random_value,
                    current_random_value,
                    over,
                ));
            }
        } else {
            dest[0].set(current_random_value);
        }

        output.set_trigger_value(current_random_value);
    }

    fn process_lorenz_attractor(
        &mut self,
        this: &Processor,
        state: &mut RandomState,
        num_samples: usize,
    ) {
        const MAX_FREQUENCY: f32 = 0.01;

        let mono = this.input(Self::STEREO).at(0).get(0) == 0.0;
        let stereo_equal_mask = PolyFloat::equal(state.state1, utils::swap_stereo(state.state1));

        let mut state1 = state.state1;
        let mut state2 = state.state2;
        let mut state3 = state.state3;

        let reset_mask = this.get_reset_mask(Self::RESET);
        if reset_mask.any_mask() != 0 && this.input(Self::SYNC).at(0).get(0) == 0.0 {
            let value1 = self.poly_next(mono) + LORENZ_INITIAL_1;
            let value2 = self.poly_next(mono) + LORENZ_INITIAL_2;
            let value3 = self.poly_next(mono) + LORENZ_INITIAL_3;
            state1 = utils::mask_load(state1, value1, reset_mask);
            state2 = utils::mask_load(state2, value2, reset_mask);
            state3 = utils::mask_load(state3, value3, reset_mask);
        }

        if mono {
            state1 &= constants::left_mask();
            state1 += utils::swap_stereo(state1);
            state2 &= constants::left_mask();
            state2 += utils::swap_stereo(state2);
            state3 &= constants::left_mask();
            state3 += utils::swap_stereo(state3);
        } else {
            state1 -= (state1 * 0.5) & stereo_equal_mask & constants::left_mask();
        }

        let frequency = this.input(Self::FREQUENCY).at(0);
        let t = utils::min(
            splat(MAX_FREQUENCY),
            frequency * (0.5f32 / this.sample_rate() as f32),
        );

        let dest = this.output_buffer(0);
        for sample in dest.iter().take(num_samples) {
            let delta1 = (state2 - state1) * LORENZ_A;
            let delta2 = (-state3 + LORENZ_B) * state1 - state2;
            let delta3 = state1 * state2 - state3 * LORENZ_C;
            state1 += delta1 * t;
            state2 += delta2 * t;
            state3 += delta3 * t;

            sample.set(state1 * LORENZ_SCALE + 0.5);
        }

        state.state1 = state1;
        state.state2 = state2;
        state.state3 = state3;

        this.output(0)
            .set_trigger_value(state.state1 * LORENZ_SCALE + 0.5);
    }
}

impl ProcessorImpl for RandomLfo {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        if this.input(Self::SYNC).at(0).get(0) != 0.0 {
            if self.last_sync.get() != self.sync_seconds.get() {
                self.process_state(this, StateSlot::Shared, num_samples);

                let output = this.output(0);
                let dest = output.buffer();
                let update_samples = if this.is_control_rate() {
                    1
                } else {
                    num_samples
                };
                for sample in dest.iter().take(update_samples) {
                    let value = sample.get() & constants::first_mask();
                    sample.set(value + utils::swap_voices(value));
                }

                let trigger_value = output.trigger_value() & constants::first_mask();
                output.set_trigger_value(trigger_value + utils::swap_voices(trigger_value));
                self.last_sync.set(self.sync_seconds.get());
            }
        } else {
            self.process_state(this, StateSlot::Own, num_samples);
        }
    }
}
