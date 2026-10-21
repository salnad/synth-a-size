// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::OscillatorModule`: one oscillator with its wavetable and its
//! `<prefix>_*` controls.
//!
//! Ports `vital/src/synthesis/modules/oscillator_module.{h,cpp}`. The
//! controls come from the parameter registry through
//! [`sas_dsp_core::synth_module::set_parameter_lookup`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef, SynthModule};
use sas_wavetable::{Wavetable, NUM_OSCILLATOR_WAVE_FRAMES};

use crate::synth_oscillator::{self as osc, SharedWavetable, SynthOscillator};

// Input indices.
pub const RESET: usize = 0;
pub const RETRIGGER: usize = 1;
pub const MIDI: usize = 2;
pub const ACTIVE_VOICES: usize = 3;
pub const NUM_INPUTS: usize = 4;

// Output indices.
pub const RAW: usize = 0;
pub const LEVELLED: usize = 1;
pub const NUM_OUTPUTS: usize = 2;

/// `vital::OscillatorModule`'s members. Like upstream's copy, a clone
/// shares the wavetable and the on-state, and keeps pointing at the
/// original's controls and oscillator.
#[derive(Clone)]
pub struct OscillatorModule {
    prefix: String,
    wavetable: SharedWavetable,
    was_on: Rc<Cell<bool>>,
    on: Option<ProcessorRef>,
    oscillator: Option<ProcessorRef>,
    distortion_type: Option<ProcessorRef>,
}

impl OscillatorModule {
    /// The kernel of `OscillatorModule(prefix)` (see [`OscillatorModule::create`]).
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            wavetable: Rc::new(RefCell::new(Wavetable::new(
                NUM_OSCILLATOR_WAVE_FRAMES as usize,
            ))),
            was_on: Rc::new(Cell::new(true)),
            on: None,
            oscillator: None,
            distortion_type: None,
        }
    }

    /// `new OscillatorModule(prefix)`.
    pub fn create(prefix: &str) -> ProcessorRef {
        SynthModule::create_with(Self::new(prefix), NUM_INPUTS, NUM_OUTPUTS, false)
    }

    /// `getWavetable()`.
    pub fn wavetable(&self) -> &SharedWavetable {
        &self.wavetable
    }

    /// `oscillator()` (set by `init`).
    pub fn oscillator(&self) -> Option<&ProcessorRef> {
        self.oscillator.as_ref()
    }

    /// `getDistortionType()`.
    pub fn distortion_type(&self) -> i32 {
        self.distortion_type.as_ref().map_or(0, |value| {
            sas_simd::conversions::float_to_i32(value.value())
        })
    }
}

impl ProcessorImpl for OscillatorModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let oscillator = SynthOscillator::create(self.wavetable.clone());
        let name = |suffix: &str| format!("{}_{suffix}", self.prefix);

        this.create_base_control(&name("view_2d"), false, false);
        let on = this.create_base_control(&name("on"), false, false);
        let midi_track = this.create_base_control(&name("midi_track"), false, false);
        let smooth_interpolation =
            this.create_base_control(&name("smooth_interpolation"), false, false);
        let spectral_unison = this.create_base_control(&name("spectral_unison"), false, false);
        let stack_style = this.create_base_control(&name("stack_style"), false, false);
        let transpose_quantize =
            this.create_base_control(&name("transpose_quantize"), false, false);

        let reset = this.input(RESET);

        let wave_frame =
            this.create_poly_mod_control(&name("wave_frame"), false, false, None, None);
        let transpose =
            this.create_poly_mod_control(&name("transpose"), true, false, None, Some(&reset));
        let tune = this.create_poly_mod_control(&name("tune"), true, false, None, Some(&reset));
        let unison_voices =
            this.create_poly_mod_control(&name("unison_voices"), false, false, None, None);
        let unison_detune =
            this.create_poly_mod_control(&name("unison_detune"), false, false, None, None);
        let detune_power =
            this.create_poly_mod_control(&name("detune_power"), false, false, None, None);
        let detune_range =
            this.create_poly_mod_control(&name("detune_range"), false, false, None, None);
        let amplitude =
            this.create_poly_mod_control(&name("level"), true, true, None, Some(&reset));
        let pan = this.create_poly_mod_control(&name("pan"), false, false, None, None);
        let phase = this.create_poly_mod_control(&name("phase"), true, true, None, Some(&reset));
        let distortion_phase =
            this.create_poly_mod_control(&name("distortion_phase"), false, false, None, None);
        let rand_phase =
            this.create_poly_mod_control(&name("random_phase"), false, false, None, None);
        let blend = this.create_poly_mod_control(&name("unison_blend"), false, false, None, None);
        let stereo_spread =
            this.create_poly_mod_control(&name("stereo_spread"), false, false, None, None);
        let frame_spread =
            this.create_poly_mod_control(&name("frame_spread"), false, false, None, None);
        let distortion_spread =
            this.create_poly_mod_control(&name("distortion_spread"), false, false, None, None);
        let distortion_type = this.create_base_control(&name("distortion_type"), false, false);
        let distortion_amount =
            this.create_poly_mod_control(&name("distortion_amount"), false, false, None, None);
        let spectral_morph_spread =
            this.create_poly_mod_control(&name("spectral_morph_spread"), false, false, None, None);
        let spectral_morph_type =
            this.create_base_control(&name("spectral_morph_type"), false, false);
        let spectral_morph_amount =
            this.create_poly_mod_control(&name("spectral_morph_amount"), false, false, None, None);

        oscillator.use_input(&this.input(RESET), osc::RESET);
        oscillator.use_input(&this.input(RETRIGGER), osc::RETRIGGER);
        oscillator.use_input(&this.input(ACTIVE_VOICES), osc::ACTIVE_VOICES);
        oscillator.use_input(&this.input(MIDI), osc::MIDI_NOTE);
        oscillator.plug(&wave_frame, osc::WAVE_FRAME);
        oscillator.plug(&midi_track, osc::MIDI_TRACK);
        oscillator.plug(&smooth_interpolation, osc::SMOOTHLY_INTERPOLATE);
        oscillator.plug(&spectral_unison, osc::SPECTRAL_UNISON);
        oscillator.plug(&transpose_quantize, osc::TRANSPOSE_QUANTIZE);
        oscillator.plug(&transpose, osc::TRANSPOSE);
        oscillator.plug(&tune, osc::TUNE);
        oscillator.plug(&stack_style, osc::STACK_STYLE);
        oscillator.plug(&unison_detune, osc::UNISON_DETUNE);
        oscillator.plug(&unison_voices, osc::UNISON_VOICES);
        oscillator.plug(&phase, osc::PHASE);
        oscillator.plug(&distortion_phase, osc::DISTORTION_PHASE);
        oscillator.plug(&rand_phase, osc::RANDOM_PHASE);
        oscillator.plug(&blend, osc::BLEND);
        oscillator.plug(&amplitude, osc::AMPLITUDE);
        oscillator.plug(&pan, osc::PAN);
        oscillator.plug(&detune_power, osc::DETUNE_POWER);
        oscillator.plug(&detune_range, osc::DETUNE_RANGE);
        oscillator.plug(&stereo_spread, osc::STEREO_SPREAD);
        oscillator.plug(&frame_spread, osc::UNISON_FRAME_SPREAD);
        oscillator.plug(&distortion_spread, osc::UNISON_DISTORTION_SPREAD);
        oscillator.plug(&distortion_type, osc::DISTORTION_TYPE);
        oscillator.plug(&distortion_amount, osc::DISTORTION_AMOUNT);
        oscillator.plug(&spectral_morph_spread, osc::UNISON_SPECTRAL_MORPH_SPREAD);
        oscillator.plug(&spectral_morph_type, osc::SPECTRAL_MORPH_TYPE);
        oscillator.plug(&spectral_morph_amount, osc::SPECTRAL_MORPH_AMOUNT);
        oscillator.use_output(&this.output(RAW), osc::RAW);
        oscillator.use_output(&this.output(LEVELLED), osc::LEVELLED);

        this.add_processor(&oscillator);
        self.on = Some(on);
        self.oscillator = Some(oscillator);
        self.distortion_type = Some(distortion_type);
        this.router_init();
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let on = self.on.as_ref().is_some_and(|on| on.value() != 0.0);

        if on {
            this.router_process(num_samples);
        } else if self.was_on.get() {
            this.output(RAW).clear_buffer();
            this.output(LEVELLED).clear_buffer();
        }

        self.was_on.set(on);
    }
}
