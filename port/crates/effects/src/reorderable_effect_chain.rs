// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::ReorderableEffectChain`: the nine effect modules in a
//! user-chosen order, each switched by its `<effect>_on` control.
//!
//! Ports `vital/src/synthesis/modules/reorderable_effect_chain.h` and
//! `reorderable_effect_chain.cpp`.

use std::cell::RefCell;
use std::rc::Rc;

use sas_dsp_core::output::{Buffer, Output};
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::{StereoMemory, SynthModule};
use sas_simd::utils::{decode_float_to_order, encode_order_to_float};

use crate::chorus_module::ChorusModule;
use crate::compressor_module::CompressorModule;

use crate::delay_module::DelayModule;
use crate::distortion_module::DistortionModule;
use crate::equalizer_module::EqualizerModule;
use crate::filter_fx_module::FilterFxModule;
use crate::flanger_module::FlangerModule;
use crate::phaser_module::PhaserModule;
use crate::reverb_module::ReverbModule;

/// `vital::constants::Effect`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Effect {
    Chorus,
    Compressor,
    Delay,
    Distortion,
    Eq,
    FilterFx,
    Flanger,
    Phaser,
    Reverb,
}

/// `vital::constants::kNumEffects`.
pub const NUM_EFFECTS: usize = 9;

/// `vital::strings::kEffectOrder`: the effect names, which prefix their
/// controls (`<name>_on`).
pub const EFFECT_ORDER: [&str; NUM_EFFECTS] = [
    "chorus",
    "compressor",
    "delay",
    "distortion",
    "eq",
    "filter_fx",
    "flanger",
    "phaser",
    "reverb",
];

/// `vital::ReorderableEffectChain`.
#[derive(Clone)]
pub struct ReorderableEffectChain {
    equalizer_memory: Option<Rc<RefCell<StereoMemory>>>,
    beats_per_second: Rc<Output>,
    keytrack: Rc<Output>,
    effects: Vec<ProcessorRef>,
    effects_on: Vec<ProcessorRef>,
    effect_order: [i32; NUM_EFFECTS],
    last_order: f32,
}

impl ReorderableEffectChain {
    pub const AUDIO: usize = 0;
    pub const ORDER: usize = 1;
    pub const NUM_INPUTS: usize = 2;

    /// `new ReorderableEffectChain(beats_per_second, keytrack)`.
    pub fn create(beats_per_second: &Rc<Output>, keytrack: &Rc<Output>) -> ProcessorRef {
        SynthModule::create_with(
            ReorderableEffectChain {
                equalizer_memory: None,
                beats_per_second: beats_per_second.clone(),
                keytrack: keytrack.clone(),
                effects: Vec::new(),
                effects_on: Vec::new(),
                effect_order: [0; NUM_EFFECTS],
                last_order: 0.0,
            },
            Self::NUM_INPUTS,
            1,
            false,
        )
    }

    /// `getEffect(effect)`.
    pub fn get_effect(&self, effect: Effect) -> &ProcessorRef {
        &self.effects[effect as usize]
    }

    /// `getEqualizerMemory()`.
    pub fn get_equalizer_memory(&self) -> Option<Rc<RefCell<StereoMemory>>> {
        self.equalizer_memory.clone()
    }

    /// `createEffectModule(index)`.
    fn create_effect_module(&mut self, index: usize) -> ProcessorRef {
        match index {
            0 => ChorusModule::create(&self.beats_per_second),
            1 => CompressorModule::create(),
            2 => DelayModule::create(&self.beats_per_second),
            3 => DistortionModule::create(),
            4 => {
                let eq = EqualizerModule::create();
                self.equalizer_memory =
                    Some(eq.with::<EqualizerModule, _>(|eq, _| eq.audio_memory()));
                eq
            }
            5 => FilterFxModule::create(&self.keytrack),
            6 => FlangerModule::create(&self.beats_per_second),
            7 => PhaserModule::create(&self.beats_per_second),
            8 => ReverbModule::create(),
            _ => unreachable!("no effect {index}"),
        }
    }
}

impl ProcessorImpl for ReorderableEffectChain {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        for (i, name) in EFFECT_ORDER.iter().enumerate() {
            let effect_module = self.create_effect_module(i);
            this.add_submodule(&effect_module);
            this.add_processor(&effect_module);
            self.effects_on
                .push(this.create_base_control(&format!("{name}_on"), false, false));
            self.effects.push(effect_module);
            self.effect_order[i] = i as i32;
        }
        self.last_order = encode_order_to_float(&self.effect_order);

        this.router_init();
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio_in = this.input_buffer(Self::AUDIO);
        self.process_with_input(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        let float_order = this.input(Self::ORDER).at(0).get(0).round();
        if float_order != self.last_order {
            decode_float_to_order(&mut self.effect_order, float_order);
        }
        self.last_order = float_order;

        let mut audio_in = audio_in.clone();
        for i in 0..NUM_EFFECTS {
            let index = self.effect_order[i] as usize;
            let on = self.effects_on[index].value() != 0.0;
            let effect = &self.effects[index];
            if on != effect.enabled() {
                effect.enable(on);
            }

            if on {
                effect.process_with_input(&audio_in, num_samples);
                audio_in = effect.output(0).buffer();
            }
        }

        let audio_out = this.output_buffer(0);
        for i in 0..num_samples {
            audio_out[i].set(audio_in[i].get());
        }
    }

    fn hard_reset(&mut self, _this: &Processor) {
        for effect in &self.effects {
            effect.hard_reset();
        }
    }

    fn correct_to_time(&mut self, _this: &Processor, seconds: f64) {
        for effect in &self.effects {
            effect.correct_to_time(seconds);
        }
    }
}
