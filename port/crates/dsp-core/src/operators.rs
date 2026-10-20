// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Operator` and the arithmetic processors built on it.
//!
//! Ports `vital/src/synthesis/framework/operators.h` and `operators.cpp`.
//!
//! An operator is disabled until at least one input is connected and the
//! owner has not disabled it; when it becomes disabled it clears its outputs
//! and processes one sample, exactly as upstream's `Operator::setEnabled`.
//! Operators are stateless (shared between voices) unless upstream overrides
//! `hasState` (`ModulationSum`, `SmoothMultiply`, `SmoothVolume`,
//! `StereoEncoder`).

use sas_simd::common::PI;
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyInt};

use crate::constants;
use crate::processor::{Processor, ProcessorImpl, ProcessorRef};

/// The per-type part of an operator.
pub trait OperatorKernel: Clone + 'static {
    /// `hasState()` (false for `Operator` unless overridden).
    const HAS_STATE: bool = false;

    fn process(&mut self, this: &Processor, num_samples: usize);
}

/// `vital::Operator`: wraps an [`OperatorKernel`] with upstream's
/// enable-on-connection logic.
#[derive(Clone)]
pub struct Operator<K: OperatorKernel> {
    externally_enabled: bool,
    pub op: K,
}

impl<K: OperatorKernel> Operator<K> {
    /// `Operator(num_inputs, num_outputs, control_rate)` around `op`.
    pub fn create(
        op: K,
        num_inputs: usize,
        num_outputs: usize,
        control_rate: bool,
    ) -> ProcessorRef {
        let processor = Processor::new(
            Operator {
                externally_enabled: true,
                op,
            },
            num_inputs,
            num_outputs,
            control_rate,
            1,
        );
        processor.processor_enable(false);
        processor
    }

    /// `hasEnoughInputs()`.
    pub fn has_enough_inputs(this: &Processor) -> bool {
        this.connected_inputs() > 0
    }

    /// `setEnabled()`.
    fn set_enabled(&mut self, this: &Processor) {
        let will_enable = Self::has_enough_inputs(this) && self.externally_enabled;
        this.processor_enable(will_enable);
        if !will_enable {
            for i in 0..this.num_outputs() {
                this.output(i).clear_buffer();
            }
            self.op.process(this, 1);
        }
    }
}

impl<K: OperatorKernel> ProcessorImpl for Operator<K> {
    crate::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        self.op.process(this, num_samples);
    }

    fn has_state(&self) -> bool {
        K::HAS_STATE
    }

    fn num_inputs_changed(&mut self, this: &Processor) {
        self.set_enabled(this);
    }

    fn enable(&mut self, this: &Processor, enable: bool) {
        self.externally_enabled = enable;
        self.set_enabled(this);
    }
}

#[inline]
fn for_each_sample(this: &Processor, num_samples: usize, f: impl Fn(PolyFloat) -> PolyFloat) {
    let source = this.input_buffer(0);
    let dest = this.output_buffer(0);
    for i in 0..num_samples {
        dest[i].set(f(source[i].get()));
    }
}

#[inline]
fn for_each_pair(
    this: &Processor,
    num_samples: usize,
    f: impl Fn(PolyFloat, PolyFloat) -> PolyFloat,
) {
    let dest = this.output_buffer(0);
    let source_left = this.input_buffer(0);
    let source_right = this.input_buffer(1);
    for i in 0..num_samples {
        dest[i].set(f(source_left[i].get(), source_right[i].get()));
    }
}

/// `vital::Clamp`.
#[derive(Clone)]
pub struct Clamp {
    min: f32,
    max: f32,
}

impl Clamp {
    pub fn create(min: f32, max: f32) -> ProcessorRef {
        Operator::create(Clamp { min, max }, 1, 1, false)
    }
}

impl OperatorKernel for Clamp {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        let (min, max) = (self.min, self.max);
        for_each_sample(this, num_samples, |s| utils::clamp(s, min, max));
    }
}

/// `vital::Negate`.
#[derive(Clone)]
pub struct Negate;

impl Negate {
    pub fn create() -> ProcessorRef {
        Operator::create(Negate, 1, 1, false)
    }
}

impl OperatorKernel for Negate {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        for_each_sample(this, num_samples, |s| -s);
    }
}

/// `vital::Inverse`.
#[derive(Clone)]
pub struct Inverse;

impl Inverse {
    pub fn create() -> ProcessorRef {
        Operator::create(Inverse, 1, 1, false)
    }
}

impl OperatorKernel for Inverse {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        for_each_sample(this, num_samples, |s| PolyFloat::splat(1.0) / s);
    }
}

/// `vital::LinearScale`.
#[derive(Clone)]
pub struct LinearScale {
    scale: f32,
}

impl LinearScale {
    pub fn create(scale: f32) -> ProcessorRef {
        Operator::create(LinearScale { scale }, 1, 1, false)
    }
}

impl OperatorKernel for LinearScale {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        let scale = self.scale;
        for_each_sample(this, num_samples, |s| s * scale);
    }
}

/// `vital::Square`.
#[derive(Clone)]
pub struct Square;

impl Square {
    pub fn create() -> ProcessorRef {
        Operator::create(Square, 1, 1, false)
    }
}

impl OperatorKernel for Square {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        for_each_sample(this, num_samples, |s| s * s);
    }
}

/// `vital::Add`.
#[derive(Clone)]
pub struct Add;

impl Add {
    pub fn create() -> ProcessorRef {
        Operator::create(Add, 2, 1, false)
    }
}

impl OperatorKernel for Add {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        for_each_pair(this, num_samples, |a, b| a + b);
    }
}

/// `vital::VariableAdd`: sums every connected input. A control-rate node
/// sums lane values into one sample (base class behaviour upstream).
#[derive(Clone)]
pub struct VariableAdd;

impl VariableAdd {
    pub fn create(num_inputs: usize) -> ProcessorRef {
        Operator::create(VariableAdd, num_inputs, 1, false)
    }
}

impl OperatorKernel for VariableAdd {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        let dest = this.output_buffer(0);
        let num_inputs = this.num_inputs();

        if this.is_control_rate() {
            dest[0].set(PolyFloat::zero());
            for i in 0..num_inputs {
                dest[0].set(dest[0].get() + this.input(i).at(0));
            }
        } else {
            for sample in dest.iter().take(num_samples) {
                sample.set(PolyFloat::zero());
            }
            for i in 0..num_inputs {
                let input = this.input(i);
                if !input.is_null_source() {
                    let source = input.buffer();
                    for s in 0..num_samples {
                        dest[s].set(dest[s].get() + source[s].get());
                    }
                }
            }
        }
    }
}

/// `vital::ModulationSum`: control-rate inputs are summed and ramped across
/// the block, audio-rate inputs are added sample by sample.
#[derive(Clone)]
pub struct ModulationSum {
    control_value: PolyFloat,
}

impl ModulationSum {
    /// `kReset`.
    pub const RESET: usize = 0;
    /// `kNumStaticInputs`.
    pub const NUM_STATIC_INPUTS: usize = 1;

    pub fn create(num_inputs: usize) -> ProcessorRef {
        let processor = Operator::create(
            ModulationSum {
                control_value: PolyFloat::zero(),
            },
            num_inputs + Self::NUM_STATIC_INPUTS,
            1,
            false,
        );
        processor.set_plugging_start(Self::NUM_STATIC_INPUTS);
        processor
    }
}

fn owner_is_control_rate(this: &Processor, index: usize) -> bool {
    this.input(index)
        .source_owner()
        .is_some_and(|owner| owner.is_control_rate())
}

impl OperatorKernel for ModulationSum {
    const HAS_STATE: bool = true;

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let output = this.output(0);
        debug_assert!(output.buffer_size() >= num_samples);
        let dest = output.buffer();
        let num_inputs = this.num_inputs();

        let mut current_control_value = self.control_value;
        self.control_value = PolyFloat::zero();

        for i in Self::NUM_STATIC_INPUTS..num_inputs {
            let input = this.input(i);
            if !input.is_null_source() && owner_is_control_rate(this, i) {
                self.control_value += input.at(0);
            }
        }

        current_control_value = utils::mask_load(
            current_control_value,
            self.control_value,
            this.get_reset_mask(Self::RESET),
        );
        let delta_control_value =
            (self.control_value - current_control_value) * (1.0 / num_samples as f32);
        for sample in dest.iter().take(num_samples) {
            current_control_value += delta_control_value;
            sample.set(current_control_value);
        }

        for i in Self::NUM_STATIC_INPUTS..num_inputs {
            let input = this.input(i);
            if !input.is_null_source() && !owner_is_control_rate(this, i) {
                let source = input.buffer();
                for s in 0..num_samples {
                    let value = source[s].get();
                    dest[s].set(dest[s].get() + value);
                }
            }
        }

        output.set_trigger_value(dest[0].get());
    }
}

/// `vital::Subtract`.
#[derive(Clone)]
pub struct Subtract;

impl Subtract {
    pub fn create() -> ProcessorRef {
        Operator::create(Subtract, 2, 1, false)
    }
}

impl OperatorKernel for Subtract {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        for_each_pair(this, num_samples, |a, b| a - b);
    }
}

/// `vital::Multiply`.
#[derive(Clone)]
pub struct Multiply;

impl Multiply {
    pub fn create() -> ProcessorRef {
        Operator::create(Multiply, 2, 1, false)
    }
}

impl OperatorKernel for Multiply {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        for_each_pair(this, num_samples, |a, b| a * b);
    }
}

/// Inputs of `SmoothMultiply` and `SmoothVolume`.
pub mod smooth_multiply {
    pub const AUDIO_RATE: usize = 0;
    pub const CONTROL_RATE: usize = 1;
    pub const RESET: usize = 2;
    pub const NUM_INPUTS: usize = 3;
}

/// `SmoothMultiply::processMultiply`.
fn process_multiply(
    multiply_state: &mut PolyFloat,
    this: &Processor,
    num_samples: usize,
    multiply: PolyFloat,
) {
    debug_assert!(this.input_matches_buffer_size(smooth_multiply::AUDIO_RATE));

    let audio_out = this.output_buffer(0);
    let audio_in = this.input_buffer(smooth_multiply::AUDIO_RATE);

    let mut current_multiply = *multiply_state;
    *multiply_state = multiply;
    current_multiply = utils::mask_load(
        current_multiply,
        *multiply_state,
        this.get_reset_mask(smooth_multiply::RESET),
    );
    let delta_multiply = (*multiply_state - current_multiply) * (1.0 / num_samples as f32);

    for i in 0..num_samples {
        current_multiply += delta_multiply;
        audio_out[i].set(audio_in[i].get() * current_multiply);
    }
}

/// `vital::SmoothMultiply`: multiplies audio by a control value ramped over
/// the block.
#[derive(Clone)]
pub struct SmoothMultiply {
    multiply: PolyFloat,
}

impl SmoothMultiply {
    pub fn create() -> ProcessorRef {
        Operator::create(
            SmoothMultiply {
                multiply: PolyFloat::zero(),
            },
            smooth_multiply::NUM_INPUTS,
            1,
            false,
        )
    }
}

impl OperatorKernel for SmoothMultiply {
    const HAS_STATE: bool = true;

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let multiply = this.input(smooth_multiply::CONTROL_RATE).at(0);
        process_multiply(&mut self.multiply, this, num_samples, multiply);
    }
}

/// `vital::SmoothVolume`: `SmoothMultiply` with a dB control (`kDb` is the
/// control-rate input); at or below −80 dB the gain is exactly 0.
#[derive(Clone)]
pub struct SmoothVolume {
    multiply: PolyFloat,
    max_db: f32,
}

impl SmoothVolume {
    /// `kDb`.
    pub const DB: usize = smooth_multiply::CONTROL_RATE;
    pub const MIN_DB: f32 = -80.0;
    pub const DEFAULT_MAX_DB: f32 = 12.2;

    pub fn create(max_db: f32) -> ProcessorRef {
        Operator::create(
            SmoothVolume {
                multiply: PolyFloat::zero(),
                max_db,
            },
            smooth_multiply::NUM_INPUTS,
            1,
            false,
        )
    }
}

impl OperatorKernel for SmoothVolume {
    const HAS_STATE: bool = true;

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let db = utils::clamp(this.input(Self::DB).at(0), Self::MIN_DB, self.max_db);
        let amplitude = futils::db_to_magnitude(db);
        let zero_mask = PolyFloat::less_than_or_equal(db, PolyFloat::splat(Self::MIN_DB));
        let amplitude = utils::mask_load(amplitude, 0.0, zero_mask);
        process_multiply(&mut self.multiply, this, num_samples, amplitude);
    }
}

/// `vital::Interpolate`. Stateless upstream (shared between voices) even
/// though it keeps `fraction_`.
#[derive(Clone)]
pub struct Interpolate {
    fraction: PolyFloat,
}

impl Interpolate {
    pub const FROM: usize = 0;
    pub const TO: usize = 1;
    pub const FRACTIONAL: usize = 2;
    pub const RESET: usize = 3;
    pub const NUM_INPUTS: usize = 4;

    pub fn create() -> ProcessorRef {
        Operator::create(
            Interpolate {
                fraction: PolyFloat::zero(),
            },
            Self::NUM_INPUTS,
            1,
            false,
        )
    }
}

impl OperatorKernel for Interpolate {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        let dest = this.output_buffer(0);
        let from = this.input_buffer(Self::FROM);
        let to = this.input_buffer(Self::TO);
        let fractional_input = this.input(Self::FRACTIONAL);
        let fractional = fractional_input.buffer();

        if fractional_input.source_is_control_rate() {
            let mut current_fraction = self.fraction;
            self.fraction = fractional[0].get();
            current_fraction = utils::mask_load(
                current_fraction,
                self.fraction,
                this.get_reset_mask(Self::RESET),
            );
            let delta_fraction = (self.fraction - current_fraction) * (1.0 / num_samples as f32);

            for i in 0..num_samples {
                current_fraction += delta_fraction;
                dest[i].set(utils::interpolate(
                    from[i].get(),
                    to[i].get(),
                    current_fraction,
                ));
            }
        } else {
            for i in 0..num_samples {
                dest[i].set(utils::interpolate(
                    from[i].get(),
                    to[i].get(),
                    fractional[i].get(),
                ));
            }
        }
    }
}

/// `vital::BilinearInterpolate`.
#[derive(Clone)]
pub struct BilinearInterpolate;

impl BilinearInterpolate {
    pub const TOP_LEFT: usize = 0;
    pub const TOP_RIGHT: usize = 1;
    pub const BOTTOM_LEFT: usize = 2;
    pub const BOTTOM_RIGHT: usize = 3;
    pub const X_POSITION: usize = 4;
    pub const Y_POSITION: usize = 5;
    pub const NUM_INPUTS: usize = 6;
    pub const POSITION_START: usize = Self::TOP_LEFT;

    pub fn create() -> ProcessorRef {
        Operator::create(BilinearInterpolate, Self::NUM_INPUTS, 1, false)
    }
}

impl OperatorKernel for BilinearInterpolate {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        const MAX_OFFSET: f32 = 1.0;

        let top_left = this.input(Self::TOP_LEFT).at(0);
        let top_right = this.input(Self::TOP_RIGHT).at(0);
        let bottom_left = this.input(Self::BOTTOM_LEFT).at(0);
        let bottom_right = this.input(Self::BOTTOM_RIGHT).at(0);

        let x_position = this.input_buffer(Self::X_POSITION);
        let y_position = this.input_buffer(Self::Y_POSITION);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            let x = utils::clamp(x_position[i].get(), -MAX_OFFSET, 1.0 + MAX_OFFSET);
            let y = utils::clamp(y_position[i].get(), -MAX_OFFSET, 1.0 + MAX_OFFSET);
            let top = utils::interpolate(top_left, top_right, x);
            let bottom = utils::interpolate(bottom_left, bottom_right, x);
            dest[i].set(utils::interpolate(top, bottom, y));
        }
    }
}

/// `vital::SampleAndHoldBuffer`: fills the block with the input's first
/// sample (skipped when the output already starts with it).
#[derive(Clone)]
pub struct SampleAndHoldBuffer;

impl SampleAndHoldBuffer {
    pub fn create() -> ProcessorRef {
        Operator::create(SampleAndHoldBuffer, 1, 1, false)
    }
}

impl OperatorKernel for SampleAndHoldBuffer {
    fn process(&mut self, this: &Processor, num_samples: usize) {
        let value = this.input(0).at(0);
        let dest = this.output_buffer(0);
        if utils::equal(value, dest[0].get()) {
            return;
        }
        for sample in dest.iter().take(num_samples) {
            sample.set(value);
        }
    }
}

/// `vital::StereoEncoder`: rotate or center-spread a stereo signal, with the
/// coefficients ramped over the block.
#[derive(Clone)]
pub struct StereoEncoder {
    cos_mult: PolyFloat,
    sin_mult: PolyFloat,
    decoding_mult: f32,
}

impl StereoEncoder {
    pub const AUDIO: usize = 0;
    pub const ENCODING_VALUE: usize = 1;
    pub const MODE: usize = 2;
    pub const NUM_INPUTS: usize = 3;

    pub const SPREAD: i32 = 0;
    pub const ROTATE: i32 = 1;
    pub const NUM_STEREO_MODES: i32 = 2;

    pub fn create(decoding: bool) -> ProcessorRef {
        Operator::create(
            StereoEncoder {
                cos_mult: PolyFloat::zero(),
                sin_mult: PolyFloat::zero(),
                decoding_mult: if decoding { -1.0 } else { 1.0 },
            },
            Self::NUM_INPUTS,
            1,
            false,
        )
    }

    /// `processRotate`.
    pub fn process_rotate(&mut self, this: &Processor, num_samples: usize) {
        let sign = PolyFloat::stereo(1.0, -1.0);
        debug_assert!(this.input_matches_buffer_size(0));

        let mut current_cos_mult = self.cos_mult;
        let mut current_sin_mult = self.sin_mult;
        let encoding = utils::clamp(this.input(Self::ENCODING_VALUE).at(0), 0.0, 1.0)
            * self.decoding_mult
            * (2.0 * PI);
        self.cos_mult = utils::cos(encoding);
        self.sin_mult = utils::sin(encoding);
        let delta_tick = 1.0 / num_samples as f32;
        let delta_cos = (self.cos_mult - current_cos_mult) * delta_tick;
        let delta_sin = (self.sin_mult - current_sin_mult) * delta_tick;

        let source = this.input_buffer(Self::AUDIO);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            current_cos_mult += delta_cos;
            current_sin_mult += delta_sin;
            let s = source[i].get();
            let swap = sign * utils::swap_stereo(s);
            dest[i].set(s * current_cos_mult + swap * current_sin_mult);
        }
    }

    /// `processCenter`.
    pub fn process_center(&mut self, this: &Processor, num_samples: usize) {
        let mut current_cos_mult = self.cos_mult;
        let mut current_sin_mult = self.sin_mult;
        let encoding = utils::clamp(this.input(Self::ENCODING_VALUE).at(0), 0.0, 1.0);
        let phase = (PolyFloat::splat(1.0) - encoding) * (0.25 * PI);
        self.cos_mult = utils::cos(phase);
        self.sin_mult = utils::sin(phase);
        let delta_tick = 1.0 / num_samples as f32;
        let delta_cos = (self.cos_mult - current_cos_mult) * delta_tick;
        let delta_sin = (self.sin_mult - current_sin_mult) * delta_tick;

        let source = this.input_buffer(Self::AUDIO);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            current_cos_mult += delta_cos;
            current_sin_mult += delta_sin;
            let s = source[i].get();
            let swap = utils::swap_stereo(s);
            dest[i].set(s * current_cos_mult + swap * current_sin_mult);
        }
    }
}

impl OperatorKernel for StereoEncoder {
    const HAS_STATE: bool = true;

    fn process(&mut self, this: &Processor, num_samples: usize) {
        if this.input(Self::MODE).at(0).get(0) != 0.0 {
            self.process_rotate(this, num_samples);
        } else {
            self.process_center(this, num_samples);
        }
    }
}

/// `vital::TempoChooser`: picks a free frequency, a tempo-synced frequency
/// (straight, dotted, triplet) or a keytracked frequency.
#[derive(Clone)]
pub struct TempoChooser;

impl TempoChooser {
    pub const FREQUENCY_MODE: i32 = 0;
    pub const TEMPO_MODE: i32 = 1;
    pub const DOTTED_MODE: i32 = 2;
    pub const TRIPLET_MODE: i32 = 3;
    pub const KEYTRACK: i32 = 4;
    pub const NUM_SYNC_MODES: i32 = 5;

    pub const FREQUENCY: usize = 0;
    pub const TEMPO_INDEX: usize = 1;
    pub const BEATS_PER_SECOND: usize = 2;
    pub const SYNC: usize = 3;
    pub const MIDI: usize = 4;
    pub const KEYTRACK_TRANSPOSE: usize = 5;
    pub const KEYTRACK_TUNE: usize = 6;
    pub const NUM_INPUTS: usize = 7;

    pub fn create() -> ProcessorRef {
        Operator::create(TempoChooser, Self::NUM_INPUTS, 1, true)
    }
}

impl OperatorKernel for TempoChooser {
    fn process(&mut self, this: &Processor, _num_samples: usize) {
        let dotted_ratio = PolyFloat::splat(2.0 / 3.0);
        let triplet_ratio = PolyFloat::splat(3.0 / 2.0);

        let tempo = utils::clamp(
            this.input(Self::TEMPO_INDEX).at(0),
            0.0,
            (constants::NUM_SYNCED_FREQUENCY_RATIOS - 1) as f32,
        );
        let tempo_index: PolyInt = utils::to_int(tempo + 0.3);
        let mut tempo_value = PolyFloat::zero();
        for i in 0..PolyFloat::SIZE {
            tempo_value.set(
                i,
                constants::SYNCED_FREQUENCY_RATIOS[tempo_index.get(i) as i32 as usize],
            );
        }

        let beats_per_second = this.input(Self::BEATS_PER_SECOND).at(0);
        tempo_value *= beats_per_second;

        let sync = this.input(Self::SYNC).at(0);
        let triplet_mask = PolyFloat::equal(sync, PolyFloat::splat(Self::TRIPLET_MODE as f32));
        let dotted_mask =
            PolyFloat::equal(sync, PolyFloat::splat(Self::DOTTED_MODE as f32)) & !triplet_mask;

        let triplet_mult = utils::mask_load(1.0, triplet_ratio, triplet_mask);
        let dotted_mult = utils::mask_load(1.0, dotted_ratio, dotted_mask);
        let tempo_adjusted = triplet_mult * dotted_mult * tempo_value;

        let frequency_mask = PolyFloat::equal(sync, PolyFloat::splat(Self::FREQUENCY_MODE as f32));
        let keytrack_mask = PolyFloat::equal(sync, PolyFloat::splat(Self::KEYTRACK as f32));
        let midi = this.input(Self::KEYTRACK_TRANSPOSE).at(0)
            + this.input(Self::KEYTRACK_TUNE).at(0)
            + this.input(Self::MIDI).at(0);
        let keytrack_frequency = utils::midi_note_to_frequency(midi);
        let result = utils::mask_load(
            tempo_adjusted,
            this.input(Self::FREQUENCY).at(0),
            frequency_mask,
        );
        this.output(0).set_at(
            0,
            utils::mask_load(result, keytrack_frequency, keytrack_mask),
        );
    }
}

/// Control-rate operators (`vital::cr::*`): one sample per block.
pub mod cr {
    use super::*;

    macro_rules! cr_unary {
        ($(#[$doc:meta])* $name:ident, |$this:ident, $value:ident, $self_:ident| $body:expr) => {
            $(#[$doc])*
            #[derive(Clone)]
            pub struct $name;

            impl $name {
                pub fn create() -> ProcessorRef {
                    Operator::create($name, 1, 1, true)
                }
            }

            impl OperatorKernel for $name {
                fn process(&mut self, $this: &Processor, _num_samples: usize) {
                    let $self_ = self;
                    let _ = &$self_;
                    let $value = $this.input(0).at(0);
                    $this.output(0).set_at(0, $body);
                }
            }
        };
    }

    macro_rules! cr_unary_param {
        ($(#[$doc:meta])* $name:ident { $($field:ident),* }, |$this:ident, $value:ident, $self_:ident| $body:expr) => {
            $(#[$doc])*
            #[derive(Clone)]
            pub struct $name {
                $($field: f32),*
            }

            impl $name {
                pub fn create($($field: f32),*) -> ProcessorRef {
                    Operator::create($name { $($field),* }, 1, 1, true)
                }
            }

            impl OperatorKernel for $name {
                fn process(&mut self, $this: &Processor, _num_samples: usize) {
                    let $self_ = &*self;
                    let $value = $this.input(0).at(0);
                    $this.output(0).set_at(0, $body);
                }
            }
        };
    }

    cr_unary_param!(
        /// `cr::Clamp`.
        Clamp { min, max },
        |this, value, s| utils::clamp(value, s.min, s.max)
    );
    cr_unary_param!(
        /// `cr::LowerBound`.
        LowerBound { min },
        |this, value, s| utils::max(value, PolyFloat::splat(s.min))
    );
    cr_unary_param!(
        /// `cr::UpperBound`.
        UpperBound { max },
        |this, value, s| utils::min(value, PolyFloat::splat(s.max))
    );
    cr_unary!(
        /// `cr::Square`: `max(x, 0)²`.
        Square,
        |this, value, s| {
            let v = utils::max(value, PolyFloat::zero());
            v * v
        }
    );
    cr_unary!(
        /// `cr::Cube`: `max(x, 0)³`.
        Cube,
        |this, value, s| {
            let v = utils::max(value, PolyFloat::zero());
            v * v * v
        }
    );
    cr_unary!(
        /// `cr::Quart`: `max(x, 0)⁴`.
        Quart,
        |this, value, s| {
            let mut v = utils::max(value, PolyFloat::zero());
            v *= v;
            v * v
        }
    );
    cr_unary_param!(
        /// `cr::Quadratic`: `max(x, 0)² + offset`.
        Quadratic { offset },
        |this, value, s| {
            let v = utils::max(value, PolyFloat::zero());
            v * v + s.offset
        }
    );
    cr_unary_param!(
        /// `cr::Cubic`: `max(x, 0)³ + offset`.
        Cubic { offset },
        |this, value, s| {
            let v = utils::max(value, PolyFloat::zero());
            v * v * v + s.offset
        }
    );
    cr_unary_param!(
        /// `cr::Quartic`: `max(x, 0)⁴ + offset`.
        Quartic { offset },
        |this, value, s| {
            let mut v = utils::max(value, PolyFloat::zero());
            v *= v;
            v * v + s.offset
        }
    );
    cr_unary_param!(
        /// `cr::Root`: `sqrt(max(x, 0)) + offset`.
        Root { offset },
        |this, value, s| utils::sqrt(utils::max(value, PolyFloat::zero())) + s.offset
    );
    cr_unary_param!(
        /// `cr::ExponentialScale`: `scale^clamp(x, min, max)` (futils pow).
        /// `offset` is stored but unused, as upstream.
        ExponentialScale { min, max, scale, offset },
        |this, value, s| {
            let _ = s.offset;
            futils::pow(PolyFloat::splat(s.scale), utils::clamp(value, s.min, s.max))
        }
    );
    cr_unary!(
        /// `cr::FrequencyToPhase`: `x / sample_rate`.
        FrequencyToPhase,
        |this, value, s| value * (1.0 / this.sample_rate() as f32)
    );
    cr_unary!(
        /// `cr::FrequencyToSamples`: `sample_rate / x`.
        FrequencyToSamples,
        |this, value, s| PolyFloat::splat(this.sample_rate() as f32) / value
    );
    cr_unary!(
        /// `cr::TimeToSamples`: `x * sample_rate`.
        TimeToSamples,
        |this, value, s| value * this.sample_rate() as f32
    );
    cr_unary!(
        /// `cr::MagnitudeScale`: futils `dbToMagnitude`.
        MagnitudeScale,
        |this, value, s| futils::db_to_magnitude(value)
    );
    cr_unary!(
        /// `cr::MidiScale`: utils `midiCentsToFrequency` (libm `powf`).
        MidiScale,
        |this, value, s| utils::midi_cents_to_frequency(value)
    );

    /// `cr::Add`.
    #[derive(Clone)]
    pub struct Add;

    impl Add {
        pub fn create() -> ProcessorRef {
            Operator::create(Add, 2, 1, true)
        }
    }

    impl OperatorKernel for Add {
        fn process(&mut self, this: &Processor, _num_samples: usize) {
            this.output(0)
                .set_at(0, this.input(0).at(0) + this.input(1).at(0));
        }
    }

    /// `cr::Multiply`.
    #[derive(Clone)]
    pub struct Multiply;

    impl Multiply {
        pub fn create() -> ProcessorRef {
            Operator::create(Multiply, 2, 1, true)
        }
    }

    impl OperatorKernel for Multiply {
        fn process(&mut self, this: &Processor, _num_samples: usize) {
            this.output(0)
                .set_at(0, this.input(0).at(0) * this.input(1).at(0));
        }
    }

    /// `cr::Interpolate`.
    #[derive(Clone)]
    pub struct Interpolate;

    impl Interpolate {
        pub const FROM: usize = 0;
        pub const TO: usize = 1;
        pub const FRACTIONAL: usize = 2;
        pub const NUM_INPUTS: usize = 3;

        pub fn create() -> ProcessorRef {
            Operator::create(Interpolate, Self::NUM_INPUTS, 1, true)
        }
    }

    impl OperatorKernel for Interpolate {
        fn process(&mut self, this: &Processor, _num_samples: usize) {
            let from = this.input(Self::FROM).at(0);
            let to = this.input(Self::TO).at(0);
            let fraction = this.input(Self::FRACTIONAL).at(0);
            this.output(0)
                .set_at(0, utils::interpolate(from, to, fraction));
        }
    }

    /// `cr::VariableAdd`.
    #[derive(Clone)]
    pub struct VariableAdd;

    impl VariableAdd {
        pub fn create(num_inputs: usize) -> ProcessorRef {
            Operator::create(VariableAdd, num_inputs, 1, true)
        }
    }

    impl OperatorKernel for VariableAdd {
        fn process(&mut self, this: &Processor, _num_samples: usize) {
            let mut value = PolyFloat::zero();
            for i in 0..this.num_inputs() {
                value += this.input(i).at(0);
            }
            this.output(0).set_at(0, value);
        }
    }

    /// `cr::BilinearInterpolate`.
    #[derive(Clone)]
    pub struct BilinearInterpolate;

    impl BilinearInterpolate {
        pub const TOP_LEFT: usize = 0;
        pub const TOP_RIGHT: usize = 1;
        pub const BOTTOM_LEFT: usize = 2;
        pub const BOTTOM_RIGHT: usize = 3;
        pub const X_POSITION: usize = 4;
        pub const Y_POSITION: usize = 5;
        pub const NUM_INPUTS: usize = 6;
        pub const POSITION_START: usize = Self::TOP_LEFT;

        pub fn create() -> ProcessorRef {
            Operator::create(BilinearInterpolate, Self::NUM_INPUTS, 1, true)
        }
    }

    impl OperatorKernel for BilinearInterpolate {
        fn process(&mut self, this: &Processor, _num_samples: usize) {
            let x = this.input(Self::X_POSITION).at(0);
            let top = utils::interpolate(
                this.input(Self::TOP_LEFT).at(0),
                this.input(Self::TOP_RIGHT).at(0),
                x,
            );
            let bottom = utils::interpolate(
                this.input(Self::BOTTOM_LEFT).at(0),
                this.input(Self::BOTTOM_RIGHT).at(0),
                x,
            );
            this.output(0).set_at(
                0,
                utils::interpolate(top, bottom, this.input(Self::Y_POSITION).at(0)),
            );
        }
    }
}
