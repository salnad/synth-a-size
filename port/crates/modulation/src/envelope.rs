// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Envelope`: the DAHDSR envelope.
//!
//! Ports `vital/src/synthesis/modulators/envelope.h` and `envelope.cpp`.
//! Control-rate processing advances one step per block; audio-rate
//! processing splits the block into sections at stage transitions and
//! triggers.

use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_simd::common::{VoiceEvent, VOICE_KILL_TIME};
use sas_simd::conversions::float_to_i32;
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat, PolyInt, PolyMask};

const IDLE: f32 = VoiceEvent::VoiceIdle.as_f32();
const ON: f32 = VoiceEvent::VoiceOn.as_f32();
const HOLD: f32 = VoiceEvent::VoiceHold.as_f32();
const DECAY: f32 = VoiceEvent::VoiceDecay.as_f32();
const OFF: f32 = VoiceEvent::VoiceOff.as_f32();
const KILL: f32 = VoiceEvent::VoiceKill.as_f32();

#[inline(always)]
fn eq(value: PolyFloat, event: f32) -> PolyMask {
    PolyFloat::equal(value, PolyFloat::splat(event))
}

#[inline(always)]
fn splat(value: f32) -> PolyFloat {
    PolyFloat::splat(value)
}

/// Kernel of `vital::Envelope`.
#[derive(Clone, Default)]
pub struct Envelope {
    position: PolyFloat,
    value: PolyFloat,
    poly_state: PolyFloat,
    start_value: PolyFloat,

    attack_power: PolyFloat,
    decay_power: PolyFloat,
    release_power: PolyFloat,
    sustain: PolyFloat,
}

impl Envelope {
    pub const DELAY: usize = 0;
    pub const ATTACK: usize = 1;
    pub const ATTACK_POWER: usize = 2;
    pub const HOLD: usize = 3;
    pub const DECAY: usize = 4;
    pub const DECAY_POWER: usize = 5;
    pub const SUSTAIN: usize = 6;
    pub const RELEASE: usize = 7;
    pub const RELEASE_POWER: usize = 8;
    pub const TRIGGER: usize = 9;
    pub const NUM_INPUTS: usize = 10;

    pub const VALUE: usize = 0;
    pub const PHASE: usize = 1;
    pub const NUM_OUTPUTS: usize = 2;

    /// `new Envelope()`.
    pub fn create() -> ProcessorRef {
        Processor::new(
            Envelope::default(),
            Self::NUM_INPUTS,
            Self::NUM_OUTPUTS,
            false,
            1,
        )
    }

    fn process_control_rate(&mut self, this: &Processor, num_samples: usize) {
        let trigger = this.input(Self::TRIGGER);
        let trigger_mask = trigger.trigger_mask();
        let mut trigger_value = trigger.trigger_value();
        let delay_time = utils::max(this.input(Self::DELAY).at(0), splat(0.0));
        let has_delay_mask = PolyFloat::not_equal(delay_time, splat(0.0));
        let note_on_mask = eq(trigger_value, ON);
        trigger_value = utils::mask_load(trigger_value, IDLE, has_delay_mask & note_on_mask);

        self.poly_state = utils::mask_load(self.poly_state, trigger_value, trigger_mask);
        self.position = utils::mask_load(self.position, 0.0, trigger_mask);

        let triggered_remaining = PolyInt::splat(num_samples as u32) - trigger.trigger_offset();
        let remaining_samples = utils::mask_load_int(
            PolyInt::splat(num_samples as u32),
            triggered_remaining,
            trigger_mask,
        );
        self.start_value = utils::mask_load(self.start_value, self.value, trigger_mask);

        let delay_mask = eq(self.poly_state, IDLE);
        let attack_mask = eq(self.poly_state, ON);
        let hold_mask = eq(self.poly_state, HOLD);
        let decay_mask = eq(self.poly_state, DECAY);
        let release_mask = eq(self.poly_state, OFF);
        let kill_mask = eq(self.poly_state, KILL);

        let delta_time = utils::to_float(remaining_samples) * (1.0f32 / this.sample_rate() as f32);

        let delta_delay = delta_time / utils::max(delay_time, splat(0.0000001));
        self.position += delta_delay & delay_mask;

        let attack_time = utils::max(this.input(Self::ATTACK).at(0), splat(0.000000001));
        let delta_attack = delta_time / attack_time;
        self.position += delta_attack & attack_mask;

        let hold_time = utils::max(this.input(Self::HOLD).at(0), splat(0.0));
        let has_hold_mask = PolyFloat::not_equal(hold_time, splat(0.0));
        let delta_hold = delta_time / utils::max(hold_time, splat(0.0000001));
        self.position += delta_hold & hold_mask;

        let decay_time = utils::max(this.input(Self::DECAY).at(0), splat(0.000000001));
        let delta_decay = delta_time / decay_time;
        self.position += delta_decay & decay_mask;

        let release_time = utils::max(this.input(Self::RELEASE).at(0), splat(0.000000001));
        let delta_release = delta_time / release_time;
        self.position += delta_release & release_mask;

        let delta_kill = delta_time * (1.0f32 / VOICE_KILL_TIME);
        self.position += delta_kill & kill_mask;
        self.position = utils::clamp(self.position, 0.0, 1.0);

        let power = (-this.input(Self::ATTACK_POWER).at(0) & attack_mask)
            + (this.input(Self::DECAY_POWER).at(0) & decay_mask)
            + (this.input(Self::RELEASE_POWER).at(0) & release_mask);

        let attack_value = futils::power_scale(self.position, power);

        let sustain = this.input(Self::SUSTAIN).at(0);
        let decay_value = splat(1.0) - (splat(1.0) - sustain) * attack_value;
        let release_value = self.start_value * (splat(1.0) - attack_value);
        let kill_value = self.start_value * (splat(1.0) - attack_value);

        self.value = (attack_value & attack_mask)
            + (splat(1.0) & hold_mask)
            + (decay_value & decay_mask)
            + (release_value & release_mask)
            + (kill_value & kill_mask);

        self.value = utils::clamp(self.value, 0.0, 1.0);
        let value_output = this.output(Self::VALUE);
        value_output.set_trigger_value(self.value);
        value_output.set_at(0, self.value);
        this.output(Self::PHASE)
            .set_at(0, self.poly_state + self.position);

        let at_end = PolyFloat::equal(self.position, splat(1.0));
        let attack_transition_mask = delay_mask & at_end;
        let hold_transition_mask = attack_mask & at_end & has_hold_mask;
        let decay_turn_mask = (attack_mask & !has_hold_mask) | hold_mask;
        let decay_transition_mask = decay_turn_mask & at_end;
        self.poly_state = utils::mask_load(self.poly_state, ON, attack_transition_mask);
        self.poly_state = utils::mask_load(self.poly_state, HOLD, hold_transition_mask);
        self.poly_state = utils::mask_load(self.poly_state, DECAY, decay_transition_mask);

        let transition_mask = attack_transition_mask | hold_transition_mask | decay_transition_mask;
        self.position &= !transition_mask;

        let dead_transition_mask = release_mask & PolyFloat::equal(self.position, splat(1.0));
        self.poly_state = utils::mask_load(self.poly_state, KILL, dead_transition_mask);
    }

    /// `processSection`: writes `audio_out[from..to]` and returns the
    /// position at `to`.
    #[allow(clippy::too_many_arguments)]
    fn process_section(
        audio_out: &sas_dsp_core::Buffer,
        from: usize,
        to: usize,
        power: PolyFloat,
        delta_power: PolyFloat,
        position: PolyFloat,
        delta_position: PolyFloat,
        start: PolyFloat,
        end: PolyFloat,
        delta_end: PolyFloat,
    ) -> PolyFloat {
        let num_samples = (to - from) as i32;

        let mut current_power = power;
        let mut current_position = position;
        let mut current_end = end;
        for sample in &audio_out[from..to] {
            let t = futils::power_scale(current_position, current_power);
            sample.set(utils::interpolate(start, current_end, t));

            current_power += delta_power;
            current_position = utils::clamp(current_position + delta_position, 0.0, 1.0);
            current_end += delta_end;
        }

        utils::clamp(position + delta_position * num_samples as f32, 0.0, 1.0)
    }

    fn process_audio_rate(&mut self, this: &Processor, num_samples: usize) {
        let delta_time = splat(1.0f32 / this.sample_rate() as f32);
        let delta_sample = 1.0f32 / num_samples as f32;

        let sustain_end = utils::clamp(this.input(Self::SUSTAIN).at(0), 0.0, 1.0);

        let delay_time = utils::max(this.input(Self::DELAY).at(0), splat(0.0));
        let delta_delay = delta_time / utils::max(delay_time, splat(0.0000001));

        let attack_time = utils::max(this.input(Self::ATTACK).at(0), splat(0.000000001));
        let delta_attack = delta_time / attack_time;
        let attack_power_end = -this.input(Self::ATTACK_POWER).at(0);

        let hold_time = utils::max(this.input(Self::HOLD).at(0), splat(0.0));
        let has_hold_mask = PolyFloat::not_equal(hold_time, splat(0.0));
        let delta_hold = delta_time / utils::max(hold_time, splat(0.0000001));

        let decay_time = utils::max(this.input(Self::DECAY).at(0), splat(0.000000001));
        let delta_decay = delta_time / decay_time;
        let decay_power_end = this.input(Self::DECAY_POWER).at(0);

        let release_time = utils::max(this.input(Self::RELEASE).at(0), splat(0.000000001));
        let delta_release = delta_time / release_time;
        let release_power_end = this.input(Self::RELEASE_POWER).at(0);

        let delta_kill = delta_time * (1.0f32 / VOICE_KILL_TIME);

        let trigger = this.input(Self::TRIGGER);
        let trigger_mask = trigger.trigger_mask();
        let mut trigger_value = trigger.trigger_value();
        let has_delay_mask = PolyFloat::not_equal(delay_time, splat(0.0));
        let note_on_mask = eq(trigger_value, ON);
        trigger_value = utils::mask_load(trigger_value, IDLE, has_delay_mask & note_on_mask);

        let samples = PolyInt::splat(num_samples as u32);
        let mut triggered_position = trigger.trigger_offset();
        triggered_position = utils::mask_load_int(samples, triggered_position, trigger_mask);

        let mut current_position = self.position;
        let audio_out = this.output_buffer(Self::VALUE);

        let mut i = 0usize;
        while i < num_samples {
            let triggering =
                trigger_mask & PolyInt::equal(PolyInt::splat(i as u32), triggered_position);
            triggered_position = utils::mask_load_int(triggered_position, samples, triggering);
            self.poly_state = utils::mask_load(self.poly_state, trigger_value, triggering);
            current_position = utils::mask_load(current_position, 0.0, triggering);

            self.start_value = utils::mask_load(self.start_value, self.value, triggering);
            self.attack_power = utils::mask_load(self.attack_power, attack_power_end, triggering);
            self.decay_power = utils::mask_load(self.decay_power, decay_power_end, triggering);
            self.release_power =
                utils::mask_load(self.release_power, release_power_end, triggering);
            self.sustain = utils::mask_load(self.sustain, sustain_end, triggering);

            let delay_mask = eq(self.poly_state, IDLE);
            let attack_mask = eq(self.poly_state, ON);
            let hold_mask = eq(self.poly_state, HOLD);
            let decay_mask = eq(self.poly_state, DECAY);
            let release_mask = eq(self.poly_state, OFF);
            let kill_mask = eq(self.poly_state, KILL);

            let delta_position = (delta_delay & delay_mask)
                + (delta_attack & attack_mask)
                + (delta_hold & hold_mask)
                + (delta_decay & decay_mask)
                + (delta_release & release_mask)
                + (delta_kill & kill_mask);

            let from_power = (self.attack_power & attack_mask)
                + (self.decay_power & decay_mask)
                + (self.release_power & release_mask);
            let to_power = (attack_power_end & attack_mask)
                + (decay_power_end & decay_mask)
                + (release_power_end & release_mask);

            let block_t = i as f32 / (1.0f32 * num_samples as f32);
            let power = utils::interpolate(from_power, to_power, block_t);
            let delta_power = (to_power - from_power) * delta_sample;

            let cycles_remaining =
                (utils::ceil(current_position) - current_position) / delta_position;
            let mut end_cycle =
                utils::mask_load(num_samples as f32, cycles_remaining + i as f32, attack_mask);
            end_cycle = utils::min(end_cycle, utils::to_float(triggered_position));
            let last_cycle = float_to_i32(utils::min_float(end_cycle)).max(i as i32 + 1) as usize;

            let current_sustain = utils::interpolate(self.sustain, sustain_end, block_t);
            let start = utils::mask_load(self.start_value, 1.0, decay_mask | hold_mask);
            let end = (splat(1.0) & (attack_mask | hold_mask)) + (current_sustain & decay_mask);
            let delta_end = ((sustain_end - self.sustain) * delta_sample) & decay_mask;

            current_position = Self::process_section(
                &audio_out,
                i,
                last_cycle,
                power,
                delta_power,
                current_position,
                delta_position,
                start,
                end,
                delta_end,
            );
            i = last_cycle;

            self.value = audio_out[i - 1].get();

            let at_end = PolyFloat::equal(current_position, splat(1.0));
            let attack_transition_mask = delay_mask & at_end;
            let hold_transition_mask = attack_mask & at_end & has_hold_mask;
            let decay_turn_mask = (attack_mask & !has_hold_mask) | hold_mask;
            let decay_transition_mask = decay_turn_mask & at_end;

            self.poly_state = utils::mask_load(self.poly_state, ON, attack_transition_mask);
            self.poly_state = utils::mask_load(self.poly_state, HOLD, hold_transition_mask);
            self.poly_state = utils::mask_load(self.poly_state, DECAY, decay_transition_mask);

            let transition_mask =
                attack_transition_mask | hold_transition_mask | decay_transition_mask;
            current_position &= !transition_mask;

            let dead_transition_mask =
                release_mask & PolyFloat::equal(current_position, splat(1.0));
            self.poly_state = utils::mask_load(self.poly_state, KILL, dead_transition_mask);
        }

        this.output(Self::VALUE)
            .set_trigger_value(audio_out[0].get());

        self.position = current_position;
        self.attack_power = attack_power_end;
        self.decay_power = decay_power_end;
        self.release_power = release_power_end;
        self.sustain = sustain_end;
        this.output(Self::PHASE)
            .set_at(0, self.poly_state + self.position);
    }
}

impl ProcessorImpl for Envelope {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        if this.is_control_rate() {
            self.process_control_rate(this, num_samples);
        } else {
            self.process_audio_rate(this, num_samples);
        }
    }
}
