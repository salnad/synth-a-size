// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::ChorusModule`: up to four modulated [`MultiDelay`] pairs.
//!
//! Ports `vital/src/synthesis/modules/chorus_module.h` and
//! `chorus_module.cpp`. Like upstream, the delays are idle processors that
//! never receive `setSampleRate` (only oversampling reaches them), so they run
//! at the default rate times the oversampling amount.

use std::rc::Rc;

use sas_dsp_core::output::{Buffer, Output};
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::sas_simd::common::{MAX_SAMPLE_RATE, PI};
use sas_dsp_core::sas_simd::poly_utils as utils;
use sas_dsp_core::sas_simd::{futils, PolyFloat};
use sas_dsp_core::{constants, SynthModule, Value};

use crate::delay::{MultiDelay, Style};

/// `vital::ChorusModule`.
#[derive(Clone)]
pub struct ChorusModule {
    beats_per_second: Rc<Output>,
    voices: Option<ProcessorRef>,

    last_num_voices: i32,

    delay_status_outputs: Vec<Rc<Output>>,

    frequency: Option<Rc<Output>>,
    delay_time_1: Option<Rc<Output>>,
    delay_time_2: Option<Rc<Output>>,
    mod_depth: Option<Rc<Output>>,
    wet_output: Option<Rc<Output>>,
    phase: PolyFloat,
    wet: PolyFloat,
    dry: PolyFloat,

    delay_frequencies: Vec<ProcessorRef>,
    delays: Vec<ProcessorRef>,
}

fn control(output: &Option<Rc<Output>>) -> PolyFloat {
    output
        .as_ref()
        .expect("ChorusModule::init creates its controls")
        .at(0)
}

impl ChorusModule {
    pub const MAX_CHORUS_MODULATION: f32 = 0.03;
    pub const MAX_CHORUS_DELAY: f32 = 0.08;
    pub const MAX_DELAY_PAIRS: usize = 4;

    /// `new ChorusModule(beats_per_second)`.
    pub fn create(beats_per_second: &Rc<Output>) -> ProcessorRef {
        let max_samples = (Self::MAX_CHORUS_DELAY * MAX_SAMPLE_RATE as f32 + 1.0) as i32;
        let delay_status_outputs: Vec<Rc<Output>> = (0..Self::MAX_DELAY_PAIRS)
            .map(|_| Output::new_control_rate())
            .collect();
        let delays: Vec<ProcessorRef> = (0..Self::MAX_DELAY_PAIRS)
            .map(|_| MultiDelay::create(max_samples))
            .collect();
        let module = SynthModule::create_with(
            ChorusModule {
                beats_per_second: beats_per_second.clone(),
                voices: None,
                last_num_voices: 0,
                delay_status_outputs: delay_status_outputs.clone(),
                frequency: None,
                delay_time_1: None,
                delay_time_2: None,
                mod_depth: None,
                wet_output: None,
                phase: PolyFloat::zero(),
                wet: PolyFloat::zero(),
                dry: PolyFloat::zero(),
                delay_frequencies: (0..Self::MAX_DELAY_PAIRS)
                    .map(|_| Value::create_control_rate(0.0))
                    .collect(),
                delays: delays.clone(),
            },
            0,
            1,
            false,
        );
        for (status, delay) in delay_status_outputs.iter().zip(&delays) {
            module.register_output(status.clone());
            module.add_idle_processor(delay);
        }
        module
    }

    /// `getNextNumVoicePairs()`.
    pub fn get_next_num_voice_pairs(&mut self) -> i32 {
        let num_voice_pairs = self.voices.as_ref().expect("initialized").value() as i32;

        for i in self.last_num_voices..num_voice_pairs {
            self.delays[i as usize].reset(constants::full_mask());
        }

        self.last_num_voices = num_voice_pairs;
        num_voice_pairs
    }
}

impl ProcessorImpl for ChorusModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let delay_style = Value::create_control_rate(Style::Mono as i32 as f32);
        let value_one = Value::create_control_rate(1.0);

        self.voices = Some(this.create_base_control("chorus_voices", false, false));

        let free_frequency = this.create_mono_mod_control("chorus_frequency", false, false, None);
        self.frequency = Some(this.create_tempo_sync_switch(
            "chorus",
            &free_frequency.owner().expect("control output has an owner"),
            &self.beats_per_second,
            false,
            None,
        ));
        let feedback = this.create_mono_mod_control("chorus_feedback", false, false, None);
        self.wet_output = Some(this.create_mono_mod_control("chorus_dry_wet", false, false, None));
        let cutoff = this.create_mono_mod_control("chorus_cutoff", false, false, None);
        let spread = this.create_mono_mod_control("chorus_spread", false, false, None);
        self.mod_depth = Some(this.create_mono_mod_control("chorus_mod_depth", false, false, None));

        self.delay_time_1 =
            Some(this.create_mono_mod_control("chorus_delay_1", false, false, None));
        self.delay_time_2 =
            Some(this.create_mono_mod_control("chorus_delay_2", false, false, None));

        for (delay, frequency) in self.delays.iter().zip(&self.delay_frequencies) {
            delay.plug(frequency, MultiDelay::FREQUENCY);
            delay.plug(&feedback, MultiDelay::FEEDBACK);
            delay.plug(&value_one, MultiDelay::WET);
            delay.plug(&cutoff, MultiDelay::FILTER_CUTOFF);
            delay.plug(&spread, MultiDelay::FILTER_SPREAD);
            delay.plug(&delay_style, MultiDelay::STYLE);
        }

        this.router_init();
    }

    fn enable(&mut self, this: &Processor, enable: bool) {
        this.synth_module_enable(enable);
        this.router_process(1);
        if enable {
            self.wet = PolyFloat::zero();
            self.dry = PolyFloat::zero();

            for delay in &self.delays {
                delay.hard_reset();
            }
        }
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        this.router_process(num_samples);
        let frequency = control(&self.frequency);
        let delta_phase = (frequency * num_samples as f32) * (1.0 / this.sample_rate() as f32);
        self.phase = utils::modulo(self.phase + delta_phase);

        let audio_out = this.output_buffer(0);
        for s in 0..num_samples {
            let sample = audio_in[s].get() & constants::first_mask();
            audio_out[s].set(sample + utils::swap_voices(sample));
        }

        let num_voices = self.get_next_num_voice_pairs();

        let delay1 = control(&self.delay_time_1);
        let delay2 = control(&self.delay_time_2);
        let delay_time = utils::mask_load(delay2, delay1, constants::first_mask());
        let average_delay = (delay_time + utils::swap_voices(delay_time)) * 0.5;
        for i in 0..num_voices.max(0) as usize {
            let pair_offset = i as f32 * 0.25 / num_voices as f32;
            let right_offset = PolyFloat::splat(0.25) & constants::right_mask();
            let phase = self.phase
                + right_offset
                + (PolyFloat::splat(0.5) & !constants::first_mask())
                + pair_offset;

            let mod_depth = control(&self.mod_depth) * Self::MAX_CHORUS_MODULATION;
            let modulation = utils::sin(phase * PI * 2.0) * 0.5 + 1.0;
            let mut delay_t = 0.0;
            if i > 0 {
                delay_t = i as f32 / (num_voices as f32 - 1.0);
            }
            let delay =
                modulation * mod_depth + utils::interpolate(delay_time, average_delay, delay_t);

            let delay_frequency =
                PolyFloat::splat(1.0) / utils::max(PolyFloat::splat(0.00001), delay);
            self.delay_frequencies[i].set_value(delay_frequency);
            self.delays[i].process_with_input(&audio_out, num_samples);

            self.delay_status_outputs[i].set_at(0, delay_frequency);
        }

        let current_wet_start = self.wet;
        let current_dry_start = self.dry;

        let wet_value = utils::clamp(control(&self.wet_output), 0.0, 1.0);
        self.wet = futils::equal_power_fade(wet_value);
        self.dry = futils::equal_power_fade_inverse(wet_value);

        let tick_increment = 1.0 / num_samples as f32;
        let delta_wet = (self.wet - current_wet_start) * tick_increment;
        let delta_dry = (self.dry - current_dry_start) * tick_increment;

        for s in 0..num_samples {
            audio_out[s].set(PolyFloat::zero());
        }

        for delay in self.delays.iter().take(num_voices.max(0) as usize) {
            let delay_out = delay.output(0).buffer();
            for s in 0..num_samples {
                let sample_out = delay_out[s].get() * 0.5;
                audio_out[s]
                    .set(audio_out[s].get() + (sample_out + utils::swap_voices(sample_out)));
            }
        }

        let mut current_wet = current_wet_start;
        let mut current_dry = current_dry_start;
        for s in 0..num_samples {
            current_dry += delta_dry;
            current_wet += delta_wet;
            audio_out[s].set(current_dry * audio_in[s].get() + current_wet * audio_out[s].get());
        }
    }

    fn correct_to_time(&mut self, _this: &Processor, seconds: f64) {
        self.phase = utils::get_cycle_offset_from_seconds(seconds, control(&self.frequency));
    }
}
