// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Phaser`: a [`PhaserFilter`] whose cutoff a triangle LFO sweeps
//! around a center note, with a dry/wet mix.
//!
//! Ports `vital/src/synthesis/effects/phaser.h` and `phaser.cpp`. The LFO
//! phase is a wrapping `poly_int`; float → int conversions go through
//! `utils::toInt` (site A1 of `migration/decisions/reference-arm.md`).

use std::rc::Rc;

use sas_dsp_core::constants;
use sas_dsp_core::output::{Buffer, Output};
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::sas_simd::common::MAX_BUFFER_SIZE;
use sas_dsp_core::sas_simd::poly_utils as utils;
use sas_dsp_core::sas_simd::{PolyFloat, PolyInt};
use sas_filters::character::PhaserFilter;
use sas_filters::synth_filter::input as filter_input;

const INT_MAX: f32 = i32::MAX as f32;
const UINT_MAX: f32 = u32::MAX as f32;

/// `constants::kStereoSplit` (`kLeftOne - kRightOne`).
fn stereo_split() -> PolyFloat {
    constants::left_one() - constants::right_one()
}

/// `vital::Phaser`.
#[derive(Clone)]
pub struct Phaser {
    cutoff: Rc<Output>,
    phaser_filter: ProcessorRef,
    mix: PolyFloat,
    mod_depth: PolyFloat,
    phase_offset: PolyFloat,
    phase: PolyInt,
}

impl Phaser {
    pub const AUDIO: usize = 0;
    pub const MIX: usize = 1;
    pub const RATE: usize = 2;
    pub const FEEDBACK_GAIN: usize = 3;
    pub const CENTER: usize = 4;
    pub const MOD_DEPTH: usize = 5;
    pub const PHASE_OFFSET: usize = 6;
    pub const BLEND: usize = 7;
    pub const NUM_INPUTS: usize = 8;

    pub const AUDIO_OUTPUT: usize = 0;
    pub const CUTOFF_OUTPUT: usize = 1;
    pub const NUM_OUTPUTS: usize = 2;

    /// `new Phaser()`.
    pub fn create() -> ProcessorRef {
        let phaser_filter = PhaserFilter::create(true);
        let phaser = Processor::new_router(
            Phaser {
                cutoff: Output::new(MAX_BUFFER_SIZE, 1),
                phaser_filter: phaser_filter.clone(),
                mix: PolyFloat::zero(),
                mod_depth: PolyFloat::zero(),
                phase_offset: PolyFloat::zero(),
                phase: PolyInt::zero(),
            },
            Self::NUM_INPUTS,
            Self::NUM_OUTPUTS,
            false,
        );
        phaser.add_idle_processor(&phaser_filter);
        phaser
    }

    /// `processWithInput(audio_in, num_samples)`.
    pub fn process_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        let tick_delta = this.input(Self::RATE).at(0) * (1.0 / this.sample_rate() as f32);

        let tick_inc = 1.0 / num_samples as f32;
        let phase_spread = self.phase_offset * stereo_split();
        let mut phase_offset = utils::to_int(phase_spread * INT_MAX);
        self.phase_offset = this.input(Self::PHASE_OFFSET).at(0);
        let end_spread = self.phase_offset * stereo_split();
        let delta_spread = (end_spread - phase_spread) * tick_inc;
        let delta_phase_offset = utils::to_int(delta_spread * INT_MAX);

        let mut current_mod_depth = self.mod_depth;
        self.mod_depth = this.input(Self::MOD_DEPTH).at(0);
        let delta_depth = (self.mod_depth - current_mod_depth) * tick_inc;

        let center_buffer = this.input_buffer(Self::CENTER);
        let cutoff = self.cutoff.buffer();
        let current_phase = self.phase;
        for i in 0..num_samples {
            phase_offset += delta_phase_offset;
            current_mod_depth += delta_depth;
            let shifted_phase = current_phase + phase_offset;
            let fold_mask = PolyInt::greater_than(shifted_phase, PolyInt::splat(i32::MAX as u32));
            let folded_phase = utils::mask_load_int(shifted_phase, -shifted_phase, fold_mask);
            let modulation = utils::to_float(folded_phase) * (2.0 / INT_MAX) - 1.0;
            cutoff[i].set(center_buffer[i].get() + modulation * current_mod_depth);
        }

        self.phaser_filter.process_with_input(audio_in, num_samples);

        self.phase += utils::to_int((tick_delta * num_samples as f32) * UINT_MAX);
        let mut current_mix = self.mix;
        self.mix = utils::clamp(this.input(Self::MIX).at(0), 0.0, 1.0);
        let delta_mix = (self.mix - current_mix) * (1.0 / num_samples as f32);

        let phaser_out = self.phaser_filter.output(0).buffer();
        let audio_out = this.output_buffer(Self::AUDIO_OUTPUT);
        for i in 0..num_samples {
            current_mix += delta_mix;
            audio_out[i].set(utils::interpolate(
                audio_in[i].get(),
                phaser_out[i].get(),
                current_mix,
            ));
        }

        this.output(Self::CUTOFF_OUTPUT)
            .set_at(0, cutoff[num_samples - 1].get());
    }

    /// `correctToTime(seconds)`.
    pub fn correct_to_time_phase(&mut self, this: &Processor, seconds: f64) {
        let rate = this.input(Self::RATE).at(0);
        let offset = utils::get_cycle_offset_from_seconds(seconds, rate);
        self.phase = utils::to_int((offset - 0.5) * UINT_MAX) + (i32::MAX / 2) as u32;
    }
}

impl ProcessorImpl for Phaser {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        self.phaser_filter
            .use_input(&this.input(Self::FEEDBACK_GAIN), filter_input::RESONANCE);
        self.phaser_filter
            .use_input(&this.input(Self::BLEND), filter_input::PASS_BLEND);
        self.phaser_filter
            .plug(&self.cutoff, filter_input::MIDI_CUTOFF);

        self.phaser_filter.init();
        this.router_init();
    }

    fn hard_reset(&mut self, this: &Processor) {
        self.phaser_filter.reset(constants::full_mask());
        self.mod_depth = this.input(Self::MOD_DEPTH).at(0);
        self.phase_offset = this.input(Self::PHASE_OFFSET).at(0);
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio_in = this.input_buffer(Self::AUDIO);
        self.process_input(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        self.process_input(this, audio_in, num_samples);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.router_set_oversample_amount(oversample);
        self.cutoff
            .ensure_buffer_size(oversample as usize * MAX_BUFFER_SIZE);
    }

    fn correct_to_time(&mut self, this: &Processor, seconds: f64) {
        self.correct_to_time_phase(this, seconds);
    }
}
