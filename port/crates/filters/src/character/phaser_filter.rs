// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::PhaserFilter`: twelve one-pole all-pass stages in three peak
//! groups, with band-limited resonance feedback.
//!
//! Ports `vital/src/synthesis/filters/phaser_filter.h` and
//! `phaser_filter.cpp`. The template parameters `saturateResonance` /
//! `saturateInput` are function pointers here: clean filters saturate the
//! resonance path with `futils::tanh`, dirty ones the all-pass input with
//! `futils::hardTanh`. The coefficient lookup's index goes through
//! `utils::toInt` (site A1c of `migration/decisions/reference-arm.md`).

use sas_dsp_core::output::Buffer;
use sas_dsp_core::sas_simd::{futils, poly_utils as utils, PolyFloat, PolyMask};
use sas_dsp_core::{processor_boilerplate, OnePoleFilter, Processor, ProcessorImpl, ProcessorRef};

use crate::synth_filter::{coefficient_lookup, input, FilterState, SynthFilter};

type Saturate = fn(PolyFloat) -> PolyFloat;

fn pass(value: PolyFloat) -> PolyFloat {
    value
}

/// `vital::PhaserFilter`.
#[derive(Clone)]
pub struct PhaserFilter {
    filter_state: FilterState,
    clean: bool,
    resonance: PolyFloat,
    drive: PolyFloat,
    peak1_amount: PolyFloat,
    peak3_amount: PolyFloat,
    peak5_amount: PolyFloat,
    invert_mult: PolyFloat,
    stages: [OnePoleFilter; Self::MAX_STAGES],
    remove_lows_stage: OnePoleFilter,
    remove_highs_stage: OnePoleFilter,
    allpass_output: PolyFloat,
}

impl PhaserFilter {
    pub const MIN_RESONANCE: f32 = 0.0;
    pub const MAX_RESONANCE: f32 = 1.0;
    pub const MIN_CUTOFF: f32 = 1.0;
    pub const CLEAR_RATIO: f32 = 20.0;

    pub const PEAK_STAGE: usize = 4;
    pub const MAX_STAGES: usize = 3 * Self::PEAK_STAGE;

    /// The kernel of `PhaserFilter(clean)`, for embedding in another
    /// processor (the phaser effect).
    pub fn new(clean: bool) -> PhaserFilter {
        let mut filter = PhaserFilter {
            filter_state: FilterState::default(),
            clean,
            resonance: PolyFloat::zero(),
            drive: PolyFloat::zero(),
            peak1_amount: PolyFloat::zero(),
            peak3_amount: PolyFloat::zero(),
            peak5_amount: PolyFloat::zero(),
            invert_mult: PolyFloat::zero(),
            stages: [OnePoleFilter::new(); Self::MAX_STAGES],
            remove_lows_stage: OnePoleFilter::new(),
            remove_highs_stage: OnePoleFilter::new(),
            allpass_output: PolyFloat::zero(),
        };
        filter.hard_reset_state();
        filter.invert_mult = PolyFloat::splat(1.0);
        filter
    }

    /// `new PhaserFilter(clean)`: `Processor(kNumInputs, 1)`.
    pub fn create(clean: bool) -> ProcessorRef {
        Processor::new(PhaserFilter::new(clean), input::NUM_INPUTS, 1, false, 1)
    }

    pub fn set_clean(&mut self, clean: bool) {
        self.clean = clean;
    }

    pub fn resonance(&self) -> PolyFloat {
        self.resonance
    }

    pub fn drive(&self) -> PolyFloat {
        self.drive
    }

    pub fn peak1_amount(&self) -> PolyFloat {
        self.peak1_amount
    }

    pub fn peak3_amount(&self) -> PolyFloat {
        self.peak3_amount
    }

    pub fn peak5_amount(&self) -> PolyFloat {
        self.peak5_amount
    }

    fn reset_state(&mut self, reset_mask: PolyMask) {
        self.allpass_output = utils::mask_load(self.allpass_output, 0.0, reset_mask);
        for stage in &mut self.stages {
            stage.reset(reset_mask);
        }
        self.remove_lows_stage.reset(reset_mask);
        self.remove_highs_stage.reset(reset_mask);
    }

    fn hard_reset_state(&mut self) {
        self.reset_state(PolyMask::full());
        self.resonance = PolyFloat::zero();
        self.drive = PolyFloat::zero();
        self.peak1_amount = PolyFloat::zero();
        self.peak3_amount = PolyFloat::zero();
        self.peak5_amount = PolyFloat::zero();
        self.allpass_output = PolyFloat::zero();
    }

    fn process_filter(
        &mut self,
        this: &Processor,
        audio_in: &Buffer,
        num_samples: usize,
        saturate_resonance: Saturate,
        saturate_input: Saturate,
    ) {
        let mut current_resonance = self.resonance;
        let mut current_drive = self.drive;
        let mut current_peak1 = self.peak1_amount;
        let mut current_peak3 = self.peak3_amount;
        let mut current_peak5 = self.peak5_amount;

        self.filter_state.load_settings(this);
        let filter_state = self.filter_state.clone();
        self.setup_filter(this, &filter_state);

        let reset_mask = this.get_reset_mask(input::RESET);
        if reset_mask.any_mask() != 0 {
            self.reset_state(reset_mask);

            current_resonance = utils::mask_load(current_resonance, self.resonance, reset_mask);
            current_drive = utils::mask_load(current_drive, self.drive, reset_mask);
            current_peak1 = utils::mask_load(current_peak1, self.peak1_amount, reset_mask);
            current_peak3 = utils::mask_load(current_peak3, self.peak3_amount, reset_mask);
            current_peak5 = utils::mask_load(current_peak5, self.peak5_amount, reset_mask);
        }

        let tick_increment = 1.0f32 / num_samples as f32;
        let delta_resonance = (self.resonance - current_resonance) * tick_increment;
        let delta_drive = (self.drive - current_drive) * tick_increment;
        let delta_peak1 = (self.peak1_amount - current_peak1) * tick_increment;
        let delta_peak3 = (self.peak3_amount - current_peak3) * tick_increment;
        let delta_peak5 = (self.peak5_amount - current_peak5) * tick_increment;

        let audio_out = this.output_buffer(0);
        let coefficient_lookup = coefficient_lookup();
        let midi_cutoff_buffer = filter_state.midi_cutoff_buffer();
        let base_midi = midi_cutoff_buffer[num_samples - 1].get();
        let base_frequency =
            utils::midi_note_to_frequency(base_midi) * (1.0 / this.sample_rate() as f32);

        for i in 0..num_samples {
            let midi_delta = midi_cutoff_buffer[i].get() - base_midi;
            let frequency = utils::min(
                base_frequency * futils::midi_offset_to_ratio(midi_delta),
                PolyFloat::splat(1.0),
            );
            let coefficient = coefficient_lookup.cubic_lookup(frequency);

            current_resonance += delta_resonance;
            current_drive += delta_drive;
            current_peak1 += delta_peak1;
            current_peak3 += delta_peak3;
            current_peak5 += delta_peak5;

            let audio = audio_in[i].get();
            self.tick(
                saturate_resonance,
                saturate_input,
                audio,
                coefficient,
                current_resonance,
                current_drive,
                current_peak1,
                current_peak3,
                current_peak5,
            );

            audio_out[i].set((audio + self.invert_mult * self.allpass_output) * 0.5);
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn tick(
        &mut self,
        saturate_resonance: Saturate,
        saturate_input: Saturate,
        audio_in: PolyFloat,
        coefficient: PolyFloat,
        resonance: PolyFloat,
        drive: PolyFloat,
        peak1: PolyFloat,
        peak3: PolyFloat,
        peak5: PolyFloat,
    ) {
        let filter_state_lows = self.remove_lows_stage.tick_basic(
            self.allpass_output,
            utils::min(coefficient * Self::CLEAR_RATIO, PolyFloat::splat(0.9)),
        );
        let filter_state_highs = self
            .remove_highs_stage
            .tick_basic(filter_state_lows, coefficient * (1.0 / Self::CLEAR_RATIO));
        let filter_state = saturate_resonance(resonance * (filter_state_lows - filter_state_highs));

        let filter_input = utils::mul_add(drive * audio_in, self.invert_mult, filter_state);
        let mut all_pass_input = saturate_input(filter_input);

        for stage in &mut self.stages[..Self::PEAK_STAGE] {
            let stage_out = stage.tick_basic(all_pass_input, coefficient);
            all_pass_input = utils::mul_add(all_pass_input, stage_out, -2.0);
        }

        let peak1_out = all_pass_input;

        for stage in &mut self.stages[Self::PEAK_STAGE..2 * Self::PEAK_STAGE] {
            let stage_out = stage.tick_basic(all_pass_input, coefficient);
            all_pass_input = utils::mul_add(all_pass_input, stage_out, -2.0);
        }

        let peak3_out = all_pass_input;

        for stage in &mut self.stages[2 * Self::PEAK_STAGE..3 * Self::PEAK_STAGE] {
            let stage_out = stage.tick_basic(all_pass_input, coefficient);
            all_pass_input = utils::mul_add(all_pass_input, stage_out, -2.0);
        }

        let peak5_out = all_pass_input;
        let all_pass_output_1_3 = utils::mul_add(peak1 * peak1_out, peak3, peak3_out);
        self.allpass_output = utils::mul_add(all_pass_output_1_3, peak5, peak5_out);
    }
}

impl SynthFilter for PhaserFilter {
    fn setup_filter(&mut self, _this: &Processor, filter_state: &FilterState) {
        let resonance_percent = utils::clamp(filter_state.resonance_percent, 0.0, 1.0);
        self.resonance =
            utils::interpolate(Self::MIN_RESONANCE, Self::MAX_RESONANCE, resonance_percent);
        self.drive = (self.resonance * 0.5 + 1.0) * filter_state.drive;

        let blend = filter_state.pass_blend;
        self.peak1_amount = utils::clamp(-blend + 1.0, 0.0, 1.0);
        self.peak5_amount = utils::clamp(blend - 1.0, 0.0, 1.0);
        self.peak3_amount = -self.peak1_amount - self.peak5_amount + 1.0;

        self.invert_mult = PolyFloat::splat(if filter_state.style != 0 { -1.0 } else { 1.0 });
    }
}

impl ProcessorImpl for PhaserFilter {
    processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let audio_in = this.input_buffer(input::AUDIO);
        self.process_with_input(this, &audio_in, num_samples);
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        if self.clean {
            self.process_filter(this, audio_in, num_samples, futils::tanh, pass);
        } else {
            self.process_filter(this, audio_in, num_samples, pass, futils::hard_tanh);
        }
    }

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.reset_state(reset_mask);
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.hard_reset_state();
    }
}
