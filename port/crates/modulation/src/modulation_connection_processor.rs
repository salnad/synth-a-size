// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::ModulationConnectionProcessor`: one slot of the modulation
//! matrix. It scales a source by amount, applies bipolar offset, stereo
//! flip, power morphing and the remap line, then the destination scale.
//!
//! Ports `vital/src/synthesis/modules/modulation_connection_processor.h`
//! and `modulation_connection_processor.cpp`. The remap line and the
//! destination scale are shared with clones (`shared_ptr` upstream).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use sas_common::LineGenerator;
use sas_dsp_core::{constants, Output};
use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef, SynthModule};
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyInt};

#[inline(always)]
fn splat(value: f32) -> PolyFloat {
    PolyFloat::splat(value)
}

/// Catmull-Rom lookup of `value` (0..1) in the remap line, clamped to ±1.
#[inline(always)]
fn remap(buffer: &[f32], resolution: f32, value: PolyFloat) -> PolyFloat {
    let boost = utils::clamp(value * resolution, 0.0, resolution);
    let indices = utils::clamp_int(
        utils::to_int(boost),
        PolyInt::zero(),
        PolyInt::splat((resolution - 1.0) as i32 as u32),
    );
    let t = boost - utils::to_float(indices);

    let interpolation_matrix = utils::get_catmull_interpolation_matrix(t);
    let mut value_matrix = utils::get_value_matrix(buffer, indices);

    value_matrix.transpose();
    utils::clamp(
        interpolation_matrix.multiply_and_sum_rows(&value_matrix),
        -1.0,
        1.0,
    )
}

/// Kernel of `vital::ModulationConnectionProcessor`.
#[derive(Clone)]
pub struct ModulationConnectionProcessor {
    index: i32,
    polyphonic: bool,
    current_value: Option<ProcessorRef>,
    bipolar: Option<ProcessorRef>,
    stereo: Option<ProcessorRef>,
    bypass: Option<ProcessorRef>,

    power: PolyFloat,
    modulation_amount: PolyFloat,

    destination_scale: Rc<Cell<f32>>,
    last_destination_scale: f32,
    map_generator: Rc<RefCell<LineGenerator>>,
}

impl ModulationConnectionProcessor {
    pub const MODULATION_INPUT: usize = 0;
    pub const MODULATION_AMOUNT: usize = 1;
    pub const MODULATION_POWER: usize = 2;
    pub const RESET: usize = 3;
    pub const NUM_INPUTS: usize = 4;

    pub const MODULATION_OUTPUT: usize = 0;
    pub const MODULATION_PRE_SCALE: usize = 1;
    pub const MODULATION_SOURCE: usize = 2;
    pub const NUM_OUTPUTS: usize = 3;

    /// `new ModulationConnectionProcessor(index)`.
    pub fn create(index: i32) -> ProcessorRef {
        let mut map_generator = LineGenerator::default();
        map_generator.init_linear();
        let processor = SynthModule::create_with(
            ModulationConnectionProcessor {
                index,
                polyphonic: true,
                current_value: None,
                bipolar: None,
                stereo: None,
                bypass: None,
                power: PolyFloat::zero(),
                modulation_amount: PolyFloat::zero(),
                destination_scale: Rc::new(Cell::new(0.0)),
                last_destination_scale: 0.0,
                map_generator: Rc::new(RefCell::new(map_generator)),
            },
            Self::NUM_INPUTS,
            Self::NUM_OUTPUTS,
            false,
        );
        processor.set_control_rate(true);
        processor
    }

    /// `initializeBaseValue(base_value)`.
    pub fn initialize_base_value(&mut self, base_value: ProcessorRef) {
        self.current_value = Some(base_value);
    }

    /// `initializeMapping()`.
    pub fn initialize_mapping(&mut self) {
        self.map_generator.borrow_mut().init_linear();
    }

    /// `currentBaseValue()`.
    pub fn current_base_value(&self) -> f32 {
        self.current_value
            .as_ref()
            .expect("base value not initialized")
            .value()
    }

    /// `setBaseValue(value)`.
    pub fn set_base_value(&self, value: f32) {
        self.current_value
            .as_ref()
            .expect("base value not initialized")
            .set_value(value);
    }

    pub fn is_polyphonic_modulation(&self) -> bool {
        self.polyphonic
    }

    pub fn set_polyphonic_modulation(&mut self, polyphonic: bool) {
        self.polyphonic = polyphonic;
    }

    fn control(control: &Option<ProcessorRef>) -> &ProcessorRef {
        control.as_ref().expect("init() creates the controls")
    }

    pub fn is_bipolar(&self) -> bool {
        Self::control(&self.bipolar).value() != 0.0
    }

    pub fn set_bipolar(&self, bipolar: bool) {
        Self::control(&self.bipolar).set_value(if bipolar { 1.0 } else { 0.0 });
    }

    pub fn is_stereo(&self) -> bool {
        Self::control(&self.stereo).value() != 0.0
    }

    pub fn set_stereo(&self, stereo: bool) {
        Self::control(&self.stereo).set_value(if stereo { 1.0 } else { 0.0 });
    }

    pub fn is_bypassed(&self) -> bool {
        Self::control(&self.bypass).value() != 0.0
    }

    /// `setDestinationScale(scale)`.
    pub fn set_destination_scale(&self, scale: f32) {
        self.destination_scale.set(scale);
    }

    pub fn index(&self) -> i32 {
        self.index
    }

    /// `lineMapGenerator()`: the remap line, shared with clones.
    pub fn line_map_generator(&self) -> Rc<RefCell<LineGenerator>> {
        self.map_generator.clone()
    }

    fn bipolar_value(&self) -> f32 {
        Self::control(&self.bipolar).value()
    }

    fn stereo_value(&self) -> f32 {
        Self::control(&self.stereo).value()
    }

    /// `poly_float(1.0f) - (constants::kRightOne * 2.0f * stereo_->value())`.
    fn stereo_scale(&self) -> PolyFloat {
        splat(1.0) - (constants::right_one() * 2.0 * self.stereo_value())
    }

    fn process_audio_rate(&mut self, this: &Processor, num_samples: usize, source: &Output) {
        if Self::control(&self.bypass).value() != 0.0 {
            let output = this.output(Self::MODULATION_OUTPUT);
            output.clear_buffer();
            output.set_trigger_value(PolyFloat::zero());
            return;
        }

        let power = -this.input(Self::MODULATION_POWER).at(0);
        let using_power = (PolyFloat::not_equal(splat(0.0), power)
            | PolyFloat::not_equal(splat(0.0), self.power))
        .any_mask()
            != 0;
        let using_map = !self.map_generator.borrow().is_linear();

        if using_power && using_map {
            self.process_audio_rate_remapped_and_morphed(this, num_samples, source, power);
        } else if using_power {
            self.process_audio_rate_morphed(this, num_samples, source, power);
        } else if using_map {
            self.process_audio_rate_remapped(this, num_samples, source);
        } else {
            self.process_audio_rate_linear(this, num_samples, source);
        }

        self.power = power;
    }

    fn process_audio_rate_linear(&mut self, this: &Processor, num_samples: usize, source: &Output) {
        let dest = this.output_buffer(Self::MODULATION_OUTPUT);
        let modulation_source = source.buffer();

        let bipolar_offset = splat(-self.bipolar_value() * 0.5);
        let mut current_amount = self.modulation_amount;
        let stereo_scale = self.stereo_scale();
        let modulation_amount =
            utils::clamp(this.input(Self::MODULATION_AMOUNT).at(0), -1.0, 1.0) * stereo_scale;
        self.modulation_amount = modulation_amount * self.destination_scale.get();
        current_amount = utils::mask_load(
            current_amount,
            self.modulation_amount,
            this.get_reset_mask(Self::RESET),
        );
        let delta_amount =
            (self.modulation_amount - current_amount) * (1.0f32 / num_samples as f32);

        for i in 0..num_samples {
            current_amount += delta_amount;
            let modulation_value = modulation_source[i].get();
            dest[i].set((modulation_value + bipolar_offset) * current_amount);
        }

        this.output(Self::MODULATION_PRE_SCALE).set_at(
            0,
            (modulation_source[0].get() + bipolar_offset) * modulation_amount,
        );
        this.output(Self::MODULATION_OUTPUT)
            .set_trigger_value(dest[0].get());
    }

    /// The shared head of the two morphed paths: polarity scales and the
    /// smoothed amount and power.
    fn morph_setup(
        &mut self,
        this: &Processor,
        num_samples: usize,
        power: PolyFloat,
    ) -> MorphSetup {
        let bipolar = splat(self.bipolar_value());
        let polarity_pre_scale = bipolar + 1.0;
        let mut polarity_post_scale = -bipolar * 0.5 + 1.0;
        polarity_post_scale *= self.stereo_scale();

        let mut current_amount = self.modulation_amount;
        let mut current_power = self.power;

        let modulation_amount = utils::clamp(this.input(Self::MODULATION_AMOUNT).at(0), -1.0, 1.0);
        self.modulation_amount = modulation_amount * self.destination_scale.get();

        let reset_mask = this.get_reset_mask(Self::RESET);
        current_amount = utils::mask_load(current_amount, self.modulation_amount, reset_mask);
        current_power = utils::mask_load(current_power, power, reset_mask);

        let sample_inc = 1.0f32 / num_samples as f32;
        let delta_amount = (self.modulation_amount - current_amount) * sample_inc;
        let delta_power = (power - current_power) * sample_inc;

        MorphSetup {
            bipolar,
            polarity_pre_scale,
            polarity_post_scale,
            current_amount,
            current_power,
            delta_amount,
            delta_power,
        }
    }

    fn morph(
        s: &MorphSetup,
        modulation_value: PolyFloat,
        current_amount: PolyFloat,
        current_power: PolyFloat,
    ) -> PolyFloat {
        let modulation_shift = modulation_value * s.polarity_pre_scale - s.bipolar;
        let modulation_abs = PolyFloat::abs(modulation_shift);
        let sign_mask = PolyFloat::sign_mask(modulation_shift);

        let shifted_modulation = futils::power_scale(modulation_abs, current_power);
        let pre_modulation = current_amount * shifted_modulation;
        (pre_modulation ^ sign_mask) * s.polarity_post_scale
    }

    fn finish_morphed(&self, this: &Processor) {
        let dest = this.output_buffer(Self::MODULATION_OUTPUT);
        this.output(Self::MODULATION_PRE_SCALE)
            .set_at(0, dest[0].get() * (1.0f32 / self.destination_scale.get()));
        this.output(Self::MODULATION_OUTPUT)
            .set_trigger_value(dest[0].get());
    }

    fn process_audio_rate_morphed(
        &mut self,
        this: &Processor,
        num_samples: usize,
        source: &Output,
        power: PolyFloat,
    ) {
        let dest = this.output_buffer(Self::MODULATION_OUTPUT);
        let modulation_source = source.buffer();
        let s = self.morph_setup(this, num_samples, power);

        let mut current_amount = s.current_amount;
        let mut current_power = s.current_power;
        for i in 0..num_samples {
            current_amount += s.delta_amount;
            current_power += s.delta_power;
            let modulation_value = modulation_source[i].get();
            dest[i].set(Self::morph(
                &s,
                modulation_value,
                current_amount,
                current_power,
            ));
        }

        self.finish_morphed(this);
    }

    fn process_audio_rate_remapped_and_morphed(
        &mut self,
        this: &Processor,
        num_samples: usize,
        source: &Output,
        power: PolyFloat,
    ) {
        let dest = this.output_buffer(Self::MODULATION_OUTPUT);
        let modulation_source = source.buffer();
        let s = self.morph_setup(this, num_samples, power);

        let map_generator = self.map_generator.clone();
        let map_generator = map_generator.borrow();
        let buffer = map_generator.cubic_interpolation_buffer();
        let resolution = map_generator.resolution() as f32;
        let mut current_amount = s.current_amount;
        let mut current_power = s.current_power;
        for i in 0..num_samples {
            current_amount += s.delta_amount;
            current_power += s.delta_power;
            let modulation_value = remap(buffer, resolution, modulation_source[i].get());
            dest[i].set(Self::morph(
                &s,
                modulation_value,
                current_amount,
                current_power,
            ));
        }

        self.finish_morphed(this);
    }

    fn process_audio_rate_remapped(
        &mut self,
        this: &Processor,
        num_samples: usize,
        source: &Output,
    ) {
        let dest = this.output_buffer(Self::MODULATION_OUTPUT);
        let modulation_source = source.buffer();

        let bipolar_offset = splat(-self.bipolar_value() * 0.5);
        let mut current_amount = self.modulation_amount;
        let stereo_scale = self.stereo_scale();
        let modulation_amount =
            utils::clamp(this.input(Self::MODULATION_AMOUNT).at(0), -1.0, 1.0) * stereo_scale;
        self.modulation_amount = modulation_amount * self.destination_scale.get();
        current_amount = utils::mask_load(
            current_amount,
            self.modulation_amount,
            this.get_reset_mask(Self::RESET),
        );
        let delta_amount =
            (self.modulation_amount - current_amount) * (1.0f32 / num_samples as f32);

        let map_generator = self.map_generator.borrow();
        let buffer = map_generator.cubic_interpolation_buffer();
        let resolution = map_generator.resolution() as f32;
        for i in 0..num_samples {
            current_amount += delta_amount;
            let modulation_value = remap(buffer, resolution, modulation_source[i].get());
            dest[i].set((modulation_value + bipolar_offset) * current_amount);
        }

        this.output(Self::MODULATION_PRE_SCALE).set_at(
            0,
            (modulation_source[0].get() + bipolar_offset) * modulation_amount,
        );
        this.output(Self::MODULATION_OUTPUT)
            .set_trigger_value(dest[0].get());
    }

    fn process_control_rate(&mut self, this: &Processor, source: &Output) {
        let output = this.output(Self::MODULATION_OUTPUT);
        if Self::control(&self.bypass).value() != 0.0 {
            output.set_at(0, PolyFloat::zero());
            output.set_trigger_value(PolyFloat::zero());
            return;
        }

        let mut modulation_input = utils::clamp(source.at(0), 0.0, 1.0);

        {
            let map_generator = self.map_generator.borrow();
            if !map_generator.is_linear() {
                modulation_input = remap(
                    map_generator.cubic_interpolation_buffer(),
                    map_generator.resolution() as f32,
                    modulation_input,
                );
            }
        }

        let bipolar = splat(self.bipolar_value());
        let polarity_pre_scale = bipolar + 1.0;
        let mut polarity_post_scale = -bipolar * 0.5 + 1.0;
        polarity_post_scale *= self.stereo_scale();

        let modulation_shift = modulation_input * polarity_pre_scale - bipolar;
        let modulation_abs = PolyFloat::abs(modulation_shift);
        let sign_mask = PolyFloat::sign_mask(modulation_shift);

        let power = -this.input(Self::MODULATION_POWER).at(0);
        let shifted_modulation = futils::power_scale(modulation_abs, power);
        let modulation_amount = utils::clamp(this.input(Self::MODULATION_AMOUNT).at(0), -1.0, 1.0);
        let pre_modulation = modulation_amount * shifted_modulation;
        let raw_modulation = (pre_modulation ^ sign_mask) * polarity_post_scale;
        this.output(Self::MODULATION_PRE_SCALE)
            .set_at(0, raw_modulation);
        output.set_at(0, raw_modulation * self.destination_scale.get());
    }
}

struct MorphSetup {
    bipolar: PolyFloat,
    polarity_pre_scale: PolyFloat,
    polarity_post_scale: PolyFloat,
    current_amount: PolyFloat,
    current_power: PolyFloat,
    delta_amount: PolyFloat,
    delta_power: PolyFloat,
}

impl ProcessorImpl for ModulationConnectionProcessor {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let number = self.index + 1;
        self.bipolar =
            Some(this.create_base_control(&format!("modulation_{number}_bipolar"), false, false));
        self.stereo =
            Some(this.create_base_control(&format!("modulation_{number}_stereo"), false, false));
        self.bypass =
            Some(this.create_base_control(&format!("modulation_{number}_bypass"), false, false));

        this.router_init();
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let input = this.input(Self::MODULATION_INPUT);
        let source = input.source().expect("modulation input is plugged");
        let modulation_input = source.trigger_value();
        this.output(Self::MODULATION_SOURCE)
            .set_at(0, modulation_input);

        if self.last_destination_scale != self.destination_scale.get() {
            self.modulation_amount = PolyFloat::zero();
        }
        self.last_destination_scale = self.destination_scale.get();

        if this.is_control_rate() || source.is_control_rate() {
            self.process_control_rate(this, &source);
        } else {
            self.process_audio_rate(this, num_samples, &source);
        }
    }
}
