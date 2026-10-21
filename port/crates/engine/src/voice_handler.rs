// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Voice` and `vital::VoiceHandler`: voice allocation and stealing,
//! legato, sustain and sostenuto, per-channel (MPE) expression, and the
//! per-voice graph copies.
//!
//! Ports `vital/src/synthesis/framework/voice_handler.h`,
//! `voice_handler.cpp` and `note_handler.h`.
//!
//! # Structure
//!
//! The handler is a `SynthModule` node whose kernel is [`VoiceHandler`]. It
//! owns two routers, as upstream: the **voice router** holds the polyphonic
//! graph and is copied once per pair of voices (an "aggregate voice", one
//! `poly_float` holding voice 0 in lanes 0–1 and voice 1 in lanes 2–3), and
//! the **global router** holds the monophonic processors. The upstream
//! router overrides (`isPolyphonic`, `getMonoRouter`, `getPolyRouter`,
//! `addProcessor`, `removeProcessor`) are installed as [`RouterHooks`] by
//! [`VoiceHandler::attach`].
//!
//! Subclasses (`SynthVoiceHandler`) embed a [`VoiceHandler`] in their kernel
//! and implement [`HasVoiceHandler`]; the free functions at the end of this
//! module (`add_processor`, `register_output`, ...) work on either.
//!
//! Voices live in `all_voices` for the handler's lifetime and are referred
//! to by index, as upstream refers to them by `Voice*`. The queues are
//! [`SlotQueue`]s because upstream walks `active_voices_` with range-for
//! loops whose bodies edit it.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use sas_dsp_core::constants;
use sas_dsp_core::{
    Output, Processor, ProcessorImpl, ProcessorRef, ProcessorRouter, RouterHooks, SynthModule,
};
use sas_simd::common::{VoiceEvent, MIDI_SIZE, NOTES_PER_OCTAVE, NUM_MIDI_CHANNELS};
use sas_simd::conversions;
use sas_simd::poly_utils as utils;
use sas_simd::{PolyFloat, PolyMask};

use crate::slot_queue::SlotQueue;
use crate::tuning::Tuning;

/// `kMaxPolyphony` (`common/synth_constants.h`).
pub const MAX_POLYPHONY: i32 = 33;
/// `kMaxActivePolyphony` (`common/synth_constants.h`).
pub const MAX_ACTIVE_POLYPHONY: i32 = 32;

const PARALLEL_VOICES: usize = PolyFloat::SIZE / 2;
const CHANNEL_SHIFT: i32 = 8;
const NOTE_MASK: i32 = (1 << CHANNEL_SHIFT) - 1;
const NUM_CHANNELS: usize = NUM_MIDI_CHANNELS as usize;

#[inline]
fn combine_note_channel(note: i32, channel: i32) -> i32 {
    (channel << CHANNEL_SHIFT) + note
}

#[inline]
fn get_channel(value: i32) -> i32 {
    value >> CHANNEL_SHIFT
}

#[inline]
fn get_note(value: i32) -> i32 {
    value & NOTE_MASK
}

/// `vital::NoteHandler`.
pub trait NoteHandler {
    fn all_sounds_off(&mut self);
    fn all_notes_off(&mut self, sample: i32);
    fn all_notes_off_channel(&mut self, sample: i32, channel: i32);
    fn note_on(&mut self, note: i32, velocity: f32, sample: i32, channel: i32);
    fn note_off(&mut self, note: i32, lift: f32, sample: i32, channel: i32);
}

/// `vital::VoiceState`.
#[derive(Clone, Copy, Debug)]
pub struct VoiceState {
    pub event: VoiceEvent,
    pub midi_note: i32,
    pub tuned_note: f32,
    pub last_note: PolyFloat,
    pub velocity: f32,
    pub lift: f32,
    pub local_pitch_bend: f32,
    pub note_pressed: i32,
    pub note_count: i32,
    pub channel: i32,
    pub sostenuto_pressed: bool,
}

/// `Voice::KeyState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyState {
    Triggering,
    Held,
    Sustained,
    Released,
    Dead,
}

/// Index of a voice in `all_voices` (`Voice*`); the default is null.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceId(usize);

impl Default for VoiceId {
    fn default() -> Self {
        VoiceId(usize::MAX)
    }
}

/// Index of an aggregate voice (`AggregateVoice*`); the default is null.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AggregateId(usize);

impl Default for AggregateId {
    fn default() -> Self {
        AggregateId(usize::MAX)
    }
}

/// `vital::Voice`.
#[derive(Clone, Debug)]
pub struct Voice {
    voice_index: usize,
    voice_mask: PolyMask,
    event_sample: i32,
    state: VoiceState,
    last_key_state: KeyState,
    key_state: KeyState,
    aftertouch_sample: i32,
    aftertouch: f32,
    slide_sample: i32,
    slide: f32,
    parent: AggregateId,
}

impl Voice {
    /// `kDefaultLiftVelocity`.
    pub const DEFAULT_LIFT_VELOCITY: f32 = 0.5;

    fn new(parent: AggregateId) -> Voice {
        Voice {
            voice_index: 0,
            voice_mask: PolyMask::zero(),
            event_sample: -1,
            state: VoiceState {
                event: VoiceEvent::VoiceOff,
                midi_note: 0,
                tuned_note: 0.0,
                last_note: PolyFloat::zero(),
                velocity: 0.0,
                lift: 0.0,
                local_pitch_bend: 0.0,
                note_pressed: 0,
                note_count: 0,
                channel: 0,
                sostenuto_pressed: false,
            },
            last_key_state: KeyState::Dead,
            key_state: KeyState::Dead,
            aftertouch_sample: -1,
            aftertouch: 0.0,
            slide_sample: -1,
            slide: 0.0,
            parent,
        }
    }

    pub fn state(&self) -> &VoiceState {
        &self.state
    }

    pub fn key_state(&self) -> KeyState {
        self.key_state
    }

    pub fn last_key_state(&self) -> KeyState {
        self.last_key_state
    }

    pub fn event_sample(&self) -> i32 {
        self.event_sample
    }

    pub fn voice_index(&self) -> usize {
        self.voice_index
    }

    pub fn voice_mask(&self) -> PolyMask {
        self.voice_mask
    }

    pub fn aftertouch(&self) -> f32 {
        self.aftertouch
    }

    pub fn slide(&self) -> f32 {
        self.slide
    }

    #[allow(clippy::too_many_arguments)]
    fn activate(
        &mut self,
        midi_note: i32,
        tuned_note: f32,
        velocity: f32,
        last_note: PolyFloat,
        note_pressed: i32,
        note_count: i32,
        sample: i32,
        channel: i32,
    ) {
        self.event_sample = sample;
        self.state.event = VoiceEvent::VoiceOn;
        self.state.midi_note = midi_note;
        self.state.tuned_note = tuned_note;
        self.state.velocity = velocity;
        self.state.lift = Self::DEFAULT_LIFT_VELOCITY;
        self.state.local_pitch_bend = 0.0;
        self.state.last_note = last_note;
        self.state.note_pressed = note_pressed;
        self.state.note_count = note_count;
        self.state.channel = channel;
        self.state.sostenuto_pressed = false;
        self.aftertouch = 0.0;
        self.aftertouch_sample = 0;
        self.slide = 0.0;
        self.slide_sample = 0;
        self.set_key_state(KeyState::Triggering);
    }

    fn set_key_state(&mut self, key_state: KeyState) {
        self.last_key_state = self.key_state;
        self.key_state = key_state;
    }

    fn sustain(&mut self) {
        self.last_key_state = self.key_state;
        self.key_state = KeyState::Sustained;
    }

    pub fn sustained(&self) -> bool {
        self.key_state == KeyState::Sustained
    }

    pub fn held(&self) -> bool {
        self.key_state == KeyState::Held
    }

    pub fn released(&self) -> bool {
        self.key_state == KeyState::Released
    }

    pub fn sostenuto(&self) -> bool {
        self.state.sostenuto_pressed
    }

    fn set_sostenuto(&mut self, sostenuto: bool) {
        self.state.sostenuto_pressed = sostenuto;
    }

    fn set_local_pitch_bend(&mut self, bend: f32) {
        self.state.local_pitch_bend = bend;
    }

    fn set_lift_velocity(&mut self, lift: f32) {
        self.state.lift = lift;
    }

    fn deactivate(&mut self, sample: i32) {
        self.event_sample = sample;
        self.state.event = VoiceEvent::VoiceOff;
        self.set_key_state(KeyState::Released);
    }

    fn kill(&mut self, sample: i32) {
        self.event_sample = sample;
        self.state.event = VoiceEvent::VoiceKill;
    }

    fn mark_dead(&mut self) {
        self.set_key_state(KeyState::Dead);
    }

    fn has_new_event(&self) -> bool {
        self.event_sample >= 0
    }

    fn set_aftertouch(&mut self, aftertouch: f32, sample: i32) {
        self.aftertouch = aftertouch;
        self.aftertouch_sample = sample;
    }

    fn set_slide(&mut self, slide: f32, sample: i32) {
        self.slide = slide;
        self.slide_sample = sample;
    }

    fn has_new_aftertouch(&self) -> bool {
        self.aftertouch_sample >= 0
    }

    fn has_new_slide(&self) -> bool {
        self.slide_sample >= 0
    }

    fn complete_voice_event(&mut self) {
        self.event_sample = -1;
        if self.key_state == KeyState::Triggering {
            self.set_key_state(KeyState::Held);
        }
    }

    fn shift_voice_event(&mut self, num_samples: i32) {
        self.event_sample -= num_samples;
        debug_assert!(self.event_sample >= 0);
    }

    fn shift_aftertouch_event(&mut self, num_samples: i32) {
        self.aftertouch_sample -= num_samples;
        debug_assert!(self.aftertouch_sample >= 0);
    }

    fn shift_slide_event(&mut self, num_samples: i32) {
        self.slide_sample -= num_samples;
        debug_assert!(self.slide_sample >= 0);
    }

    fn clear_aftertouch_event(&mut self) {
        self.aftertouch_sample = -1;
    }

    /// Upstream clears the aftertouch sample here, not the slide sample.
    fn clear_slide_event(&mut self) {
        self.aftertouch_sample = -1;
    }

    fn set_voice_info(&mut self, voice_index: usize, voice_mask: PolyMask) {
        self.voice_index = voice_index;
        self.voice_mask = voice_mask;
    }
}

/// `vital::AggregateVoice`.
struct AggregateVoice {
    voices: Vec<VoiceId>,
    processor: ProcessorRef,
}

/// `VoiceHandler::VoiceOverride`.
pub mod voice_override {
    pub const KILL: i32 = 0;
    pub const STEAL: i32 = 1;
}

/// `VoiceHandler::VoicePriority`.
pub mod voice_priority {
    pub const NEWEST: i32 = 0;
    pub const OLDEST: i32 = 1;
    pub const HIGHEST: i32 = 2;
    pub const LOWEST: i32 = 3;
    pub const ROUND_ROBIN: i32 = 4;
}

/// Kernel of `vital::VoiceHandler`.
pub struct VoiceHandler {
    polyphony: i32,
    legato: bool,
    last_voice_outputs: Vec<(Rc<Output>, Rc<Output>)>,
    nonaccumulated_outputs: Vec<(Rc<Output>, Rc<Output>)>,
    accumulated_outputs: Vec<(Rc<Output>, Rc<Output>)>,
    voice_killer: Option<Rc<Output>>,
    voice_midi: Rc<Output>,
    last_num_voices: usize,
    last_played_note: PolyFloat,

    voice_event: Rc<Output>,
    retrigger: Rc<Output>,
    reset: Rc<Output>,
    note: Rc<Output>,
    last_note: Rc<Output>,
    note_pressed: Rc<Output>,
    note_count: Rc<Output>,
    note_in_octave: Rc<Output>,
    channel: Rc<Output>,
    velocity: Rc<Output>,
    lift: Rc<Output>,
    aftertouch: Rc<Output>,
    slide: Rc<Output>,
    active_mask: Rc<Output>,
    mod_wheel: Rc<Output>,
    pitch_wheel: Rc<Output>,
    pitch_wheel_percent: Rc<Output>,
    local_pitch_bend: Rc<Output>,

    sustain: [bool; NUM_CHANNELS],
    sostenuto: [bool; NUM_CHANNELS],
    mod_wheel_values: [f32; NUM_CHANNELS],
    pitch_wheel_values: [f32; NUM_CHANNELS],
    zoned_pitch_wheel_values: [f32; NUM_CHANNELS],
    pressure_values: [f32; NUM_CHANNELS],
    slide_values: [f32; NUM_CHANNELS],

    tuning: Option<Rc<Tuning>>,
    voice_priority: i32,
    voice_override: i32,

    total_notes: i32,
    pressed_notes: SlotQueue<i32>,
    all_voices: Vec<Voice>,

    free_voices: SlotQueue<VoiceId>,
    active_voices: SlotQueue<VoiceId>,
    all_aggregate_voices: Vec<AggregateVoice>,
    active_aggregate_voices: SlotQueue<AggregateId>,

    voice_router: ProcessorRef,
    global_router: ProcessorRef,
}

impl VoiceHandler {
    /// `kLocalPitchBendRange`.
    pub const LOCAL_PITCH_BEND_RANGE: f32 = 48.0;

    pub const POLYPHONY: usize = 0;
    pub const VOICE_PRIORITY: usize = 1;
    pub const VOICE_OVERRIDE: usize = 2;
    pub const NUM_INPUTS: usize = 3;

    /// The member initialisation and body of `VoiceHandler(num_outputs,
    /// polyphony, control_rate)` that do not need the node. Build the node
    /// around the kernel (a `SynthModule` with [`Self::NUM_INPUTS`] inputs),
    /// then call [`VoiceHandler::attach`].
    pub fn new(polyphony: i32) -> VoiceHandler {
        let voice_router = ProcessorRouter::create(0, 0, false);
        let global_router = ProcessorRouter::create(0, 0, false);

        let cr_output = || {
            let output = Output::new_control_rate();
            output.set_owner(Some(&voice_router));
            output
        };
        let note = cr_output();

        let reserve = (MAX_POLYPHONY as usize) + PARALLEL_VOICES;
        let mut pressed_notes = SlotQueue::default();
        pressed_notes.reserve(MIDI_SIZE as usize);
        let mut free_voices = SlotQueue::default();
        free_voices.reserve(reserve);
        let mut active_voices = SlotQueue::default();
        active_voices.reserve(reserve);
        let mut active_aggregate_voices = SlotQueue::default();
        active_aggregate_voices.reserve(MAX_POLYPHONY as usize / PARALLEL_VOICES + PARALLEL_VOICES);

        let mut handler = VoiceHandler {
            polyphony: 0,
            legato: false,
            last_voice_outputs: Vec::new(),
            nonaccumulated_outputs: Vec::new(),
            accumulated_outputs: Vec::new(),
            voice_killer: None,
            voice_midi: note.clone(),
            last_num_voices: 0,
            last_played_note: PolyFloat::splat(-1.0),
            voice_event: cr_output(),
            retrigger: cr_output(),
            reset: cr_output(),
            note,
            last_note: cr_output(),
            note_pressed: cr_output(),
            note_count: cr_output(),
            note_in_octave: cr_output(),
            channel: cr_output(),
            velocity: cr_output(),
            lift: cr_output(),
            aftertouch: cr_output(),
            slide: cr_output(),
            active_mask: cr_output(),
            mod_wheel: cr_output(),
            pitch_wheel: cr_output(),
            pitch_wheel_percent: cr_output(),
            local_pitch_bend: cr_output(),
            sustain: [false; NUM_CHANNELS],
            sostenuto: [false; NUM_CHANNELS],
            mod_wheel_values: [0.0; NUM_CHANNELS],
            pitch_wheel_values: [0.0; NUM_CHANNELS],
            zoned_pitch_wheel_values: [0.0; NUM_CHANNELS],
            pressure_values: [0.0; NUM_CHANNELS],
            slide_values: [0.0; NUM_CHANNELS],
            tuning: None,
            voice_priority: voice_priority::ROUND_ROBIN,
            voice_override: voice_override::KILL,
            total_notes: 0,
            pressed_notes,
            all_voices: Vec::with_capacity(MAX_POLYPHONY as usize + PARALLEL_VOICES),
            free_voices,
            active_voices,
            all_aggregate_voices: Vec::new(),
            active_aggregate_voices,
            voice_router,
            global_router,
        };
        handler.set_polyphony(polyphony);
        handler
    }

    /// `VoiceHandler(num_outputs, polyphony, control_rate)` for a plain
    /// voice handler.
    pub fn create(num_outputs: usize, polyphony: i32, control_rate: bool) -> ProcessorRef {
        let handler = VoiceHandler::new(polyphony);
        let (voice_router, global_router) = handler.routers();
        let node = SynthModule::create_with(handler, Self::NUM_INPUTS, num_outputs, control_rate);
        Self::attach(&node, &voice_router, &global_router);
        node
    }

    /// The end of the upstream constructor: the routers' parent is the
    /// handler, and the router overrides become node hooks.
    pub fn attach(node: &ProcessorRef, voice_router: &ProcessorRef, global_router: &ProcessorRef) {
        voice_router.set_router(Some(node));
        global_router.set_router(Some(node));
        node.set_router_hooks(RouterHooks {
            polyphonic_child: Some(Rc::downgrade(voice_router)),
            mono_router: Some(Rc::downgrade(global_router)),
            poly_router: Some(Rc::downgrade(voice_router)),
            forward_add_remove: Some(Rc::downgrade(voice_router)),
        });
    }

    /// (voice router, global router).
    pub fn routers(&self) -> (ProcessorRef, ProcessorRef) {
        (self.voice_router.clone(), self.global_router.clone())
    }

    // ----- Processor overrides -----

    /// `VoiceHandler::process`.
    pub fn process(&mut self, this: &Processor, num_samples: usize) {
        self.global_router.process(num_samples);

        let num_voices = self.active_voices.len();
        if num_voices == 0 {
            if self.last_num_voices != 0 {
                self.clear_accumulated_outputs();
            }
            self.last_num_voices = num_voices;
            return;
        }

        // `static_cast<int>(std::roundf(...))`: a per-ISA cast (A3).
        let polyphony = conversions::float_to_i32(this.input(Self::POLYPHONY).at(0).get(0).round());
        self.set_polyphony(polyphony.clamp(1, MAX_ACTIVE_POLYPHONY));

        self.voice_priority =
            utils::round_to_int(this.input(Self::VOICE_PRIORITY).at(0)).get(0) as i32;
        self.voice_override =
            utils::round_to_int(this.input(Self::VOICE_OVERRIDE).at(0)).get(0) as i32;

        self.clear_accumulated_outputs();

        self.active_aggregate_voices.clear();
        let mut last_aggregate_voice = None;
        let mut last_aggregate_index = 0;
        for active_voice in self.active_voices.iter() {
            let voice = &self.all_voices[active_voice.0];
            if self.active_aggregate_voices.count(voice.parent) == 0 {
                self.active_aggregate_voices.push_back(voice.parent);
            }
            last_aggregate_voice = Some(voice.parent);
            last_aggregate_index = voice.voice_index;
        }

        if let Some(last) = last_aggregate_voice {
            self.active_aggregate_voices.remove(last);
            self.active_aggregate_voices.push_back(last);
        }

        let aggregates: Vec<AggregateId> = self.active_aggregate_voices.iter().collect();
        let oversample = this.oversample_amount();
        for aggregate in aggregates {
            self.prepare_voice_triggers(aggregate, num_samples as i32, oversample);
            self.prepare_voice_values(aggregate);
            self.all_aggregate_voices[aggregate.0]
                .processor
                .process(num_samples);
            self.accumulate_outputs(num_samples);

            let mut alive_mask = constants::full_mask();
            if let Some(killer) = &self.voice_killer {
                let buffer: Vec<PolyFloat> = killer.buffer()[..num_samples]
                    .iter()
                    .map(Cell::get)
                    .collect();
                alive_mask = !utils::get_silent_mask(&buffer);
            }
            for i in 0..self.all_aggregate_voices[aggregate.0].voices.len() {
                let single_voice = self.all_aggregate_voices[aggregate.0].voices[i];
                let voice = &self.all_voices[single_voice.0];
                let released = voice.state.event == VoiceEvent::VoiceOff
                    || voice.state.event == VoiceEvent::VoiceKill;
                let alive = (voice.voice_mask & alive_mask).sum() != 0;
                let active = self.active_voices.count(single_voice) != 0;
                if released && !alive && active {
                    self.active_voices.remove(single_voice);
                    self.free_voices.push_back(single_voice);
                    self.all_voices[single_voice.0].mark_dead();
                }
            }
        }

        self.combine_accumulated_outputs(num_samples);

        if !self.active_voices.is_empty() {
            let mut voice_mask = constants::first_mask();
            if last_aggregate_index != 0 {
                voice_mask = !voice_mask;
            }

            self.write_nonaccumulated_outputs(voice_mask, num_samples);

            self.last_played_note = self.voice_midi.trigger_value() & voice_mask;
            self.last_played_note += utils::swap_voices(self.last_played_note);
        }

        self.last_num_voices = num_voices;
    }

    /// `VoiceHandler::init`.
    pub fn init(&mut self, this: &Processor) {
        self.voice_router.init();
        self.global_router.init();
        this.router_init();
    }

    /// `VoiceHandler::setSampleRate`.
    pub fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.router_set_sample_rate(sample_rate);
        self.voice_router.set_sample_rate(sample_rate);
        self.global_router.set_sample_rate(sample_rate);
        for aggregate_voice in &self.all_aggregate_voices {
            aggregate_voice.processor.set_sample_rate(sample_rate);
        }
    }

    /// `VoiceHandler::setOversampleAmount`.
    pub fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.router_set_oversample_amount(oversample);
        self.voice_router.set_oversample_amount(oversample);
        self.global_router.set_oversample_amount(oversample);
    }

    /// `VoiceHandler::resetFeedbacks`.
    pub fn reset_feedbacks(&mut self, reset_mask: PolyMask) {
        self.voice_router.reset_feedbacks(reset_mask);
    }

    // ----- voice processing -----

    fn prepare_voice_triggers(
        &mut self,
        aggregate: AggregateId,
        num_samples: i32,
        oversample: i32,
    ) {
        self.note.clear_trigger();
        self.last_note.clear_trigger();
        self.channel.clear_trigger();
        self.velocity.clear_trigger();
        self.lift.clear_trigger();
        self.voice_event.clear_trigger();
        self.retrigger.clear_trigger();
        self.reset.clear_trigger();
        self.aftertouch.clear_trigger();
        self.slide.clear_trigger();

        for i in 0..self.all_aggregate_voices[aggregate.0].voices.len() {
            let id = self.all_aggregate_voices[aggregate.0].voices[i];
            let legato = self.legato;
            let voice = &mut self.all_voices[id.0];
            if !voice.has_new_event() {
                continue;
            }

            let offset = voice.event_sample * oversample;
            if num_samples <= offset {
                voice.shift_voice_event(num_samples / oversample);
            } else {
                let mask = voice.voice_mask;
                let offset_int = (offset as u32).into();
                let event = voice.state.event.as_f32();
                self.voice_event.trigger(mask, event.into(), offset_int);

                if voice.state.event == VoiceEvent::VoiceOn {
                    self.note
                        .trigger(mask, voice.state.tuned_note.into(), offset_int);
                    self.last_note
                        .trigger(mask, voice.state.last_note, offset_int);
                    self.velocity
                        .trigger(mask, voice.state.velocity.into(), offset_int);
                    self.channel
                        .trigger(mask, (voice.state.channel as f32).into(), offset_int);

                    if voice.last_key_state == KeyState::Dead {
                        self.reset
                            .trigger(mask, VoiceEvent::VoiceOn.as_f32().into(), offset_int);
                    }
                } else if voice.state.event == VoiceEvent::VoiceOff {
                    self.lift.trigger(mask, voice.state.lift.into(), offset_int);
                }

                if !legato
                    || voice.last_key_state != KeyState::Held
                    || voice.state.event != VoiceEvent::VoiceOn
                {
                    self.retrigger.trigger(mask, event.into(), offset_int);
                }

                voice.complete_voice_event();
            }

            if voice.has_new_aftertouch() {
                let aftertouch_sample = voice.aftertouch_sample * oversample;
                if num_samples <= aftertouch_sample {
                    voice.shift_aftertouch_event(num_samples / oversample);
                } else {
                    self.aftertouch.trigger(
                        voice.voice_mask,
                        voice.aftertouch.into(),
                        (aftertouch_sample as u32).into(),
                    );
                    voice.clear_aftertouch_event();
                }
            }

            if voice.has_new_slide() {
                let slide_sample = voice.slide_sample * oversample;
                if num_samples <= slide_sample {
                    voice.shift_slide_event(num_samples / oversample);
                } else {
                    self.slide.trigger(
                        voice.voice_mask,
                        voice.slide.into(),
                        (slide_sample as u32).into(),
                    );
                    voice.clear_slide_event();
                }
            }
        }
    }

    fn prepare_voice_values(&mut self, aggregate: AggregateId) {
        for &id in &self.all_aggregate_voices[aggregate.0].voices {
            let voice = &self.all_voices[id.0];
            let mask = voice.voice_mask;
            let channel = voice.state.channel as usize;
            let load = |output: &Output, value: PolyFloat| {
                output.set_trigger_value(utils::mask_load(output.trigger_value(), value, mask));
            };

            let note = utils::mask_load(self.note.trigger_value(), voice.state.tuned_note, mask);
            self.note.set_trigger_value(note);
            load(&self.last_note, voice.state.last_note);

            load(&self.note_pressed, (voice.state.note_pressed as f32).into());
            load(&self.note_count, (voice.state.note_count as f32).into());
            self.note_in_octave
                .set_trigger_value(utils::modulo(note * (1.0 / NOTES_PER_OCTAVE as f32)));
            load(&self.channel, (channel as f32).into());
            load(&self.velocity, voice.state.velocity.into());

            let mut lift = 0.0;
            if voice.released() {
                lift = voice.state.lift;
            }
            load(&self.lift, lift.into());

            load(&self.aftertouch, voice.aftertouch.into());
            load(&self.slide, voice.slide.into());

            let dead = voice.key_state == KeyState::Dead;
            let active_value = if dead { 0.0 } else { 1.0 };
            load(&self.active_mask, active_value.into());

            load(&self.mod_wheel, self.mod_wheel_values[channel].into());

            let pitch_wheel = self.zoned_pitch_wheel_values[channel];
            load(&self.pitch_wheel, pitch_wheel.into());

            let pitch_wheel_percent = pitch_wheel * 0.5 + 0.5;
            load(&self.pitch_wheel_percent, pitch_wheel_percent.into());

            let local_pitch_bend = voice.state.local_pitch_bend * Self::LOCAL_PITCH_BEND_RANGE;
            load(&self.local_pitch_bend, local_pitch_bend.into());
        }
    }

    fn clear_accumulated_outputs(&self) {
        for (_, output) in &self.accumulated_outputs {
            let buffer = output.buffer();
            for sample in buffer.iter().take(output.buffer_size()) {
                sample.set(PolyFloat::zero());
            }
        }
    }

    fn accumulate_outputs(&self, num_samples: usize) {
        for (source, output) in &self.accumulated_outputs {
            let buffer_size = num_samples.min(output.buffer_size());
            let dest = output.buffer();
            let source = source.buffer();
            for i in 0..buffer_size {
                dest[i].set(dest[i].get() + source[i].get());
            }
        }
    }

    fn combine_accumulated_outputs(&self, num_samples: usize) {
        for (_, output) in &self.accumulated_outputs {
            let buffer_size = num_samples.min(output.buffer_size());
            let dest = output.buffer();
            for sample in dest.iter().take(buffer_size) {
                sample.set(sample.get() + utils::swap_voices(sample.get()));
            }
        }
    }

    fn write_nonaccumulated_outputs(&self, voice_mask: PolyMask, num_samples: usize) {
        for (source, output) in &self.nonaccumulated_outputs {
            let buffer_size = num_samples.min(output.buffer_size());
            debug_assert!(buffer_size == 1);
            let dest = output.buffer();
            let source = source.buffer();
            for i in 0..buffer_size {
                let masked = source[i].get() & voice_mask;
                dest[i].set(masked + utils::swap_voices(masked));
            }
        }
    }

    // ----- queries -----

    pub fn set_tuning(&mut self, tuning: Option<Rc<Tuning>>) {
        self.tuning = tuning;
    }

    pub fn num_active_voices(&self) -> usize {
        self.active_voices.len()
    }

    pub fn num_pressed_notes(&self) -> usize {
        self.pressed_notes.len()
    }

    /// `isNotePlaying(note)`.
    pub fn is_note_playing(&self, note: i32) -> bool {
        self.active_voices.iter().any(|id| {
            let state = &self.all_voices[id.0].state;
            state.event != VoiceEvent::VoiceKill && state.midi_note == note
        })
    }

    /// `isNotePlaying(note, channel)`.
    pub fn is_note_playing_on(&self, note: i32, channel: i32) -> bool {
        self.active_voices.iter().any(|id| {
            let state = &self.all_voices[id.0].state;
            state.event != VoiceEvent::VoiceKill
                && state.midi_note == note
                && state.channel == channel
        })
    }

    /// `getCurrentVoiceMask()`.
    pub fn current_voice_mask(&self) -> PolyMask {
        if !self.active_voices.is_empty() {
            if self.all_voices[self.active_voices.back().0].voice_index != 0 {
                return !constants::first_mask();
            }
            return constants::first_mask();
        }
        PolyMask::zero()
    }

    /// `getLastActiveNote()`.
    pub fn last_active_note(&self) -> f32 {
        if !self.active_voices.is_empty() {
            return self.all_voices[self.active_voices.back().0]
                .state
                .tuned_note;
        }
        0.0
    }

    pub fn polyphony(&self) -> i32 {
        self.polyphony
    }

    pub fn legato(&self) -> bool {
        self.legato
    }

    pub fn set_legato(&mut self, legato: bool) {
        self.legato = legato;
    }

    pub fn set_voice_killer(&mut self, killer: Option<Rc<Output>>) {
        self.voice_killer = killer;
    }

    pub fn set_voice_midi(&mut self, midi: Rc<Output>) {
        self.voice_midi = midi;
    }

    pub fn voice(&self, id: VoiceId) -> &Voice {
        &self.all_voices[id.0]
    }

    // ----- pedals -----

    pub fn sustain_on(&mut self, channel: i32) {
        self.sustain[channel as usize] = true;
    }

    pub fn sustain_off(&mut self, sample: i32, channel: i32) {
        self.sustain[channel as usize] = false;
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            if voice.sustained() && !voice.sostenuto() && voice.state.channel == channel {
                voice.deactivate(sample);
            }
        }
    }

    pub fn sostenuto_on(&mut self, channel: i32) {
        self.sostenuto[channel as usize] = true;
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            if voice.state.channel == channel {
                voice.set_sostenuto(true);
            }
        }
    }

    pub fn sostenuto_off(&mut self, sample: i32, channel: i32) {
        self.sostenuto[channel as usize] = false;
        for id in self.active_ids() {
            let sustain = self.sustain[channel as usize];
            let voice = &mut self.all_voices[id.0];
            if voice.state.channel == channel {
                voice.set_sostenuto(false);
                if voice.sustained() && !sustain {
                    voice.deactivate(sample);
                }
            }
        }
    }

    pub fn sustain_on_range(&mut self, from_channel: i32, to_channel: i32) {
        for i in from_channel..=to_channel {
            self.sustain[i as usize] = true;
        }
    }

    pub fn sustain_off_range(&mut self, sample: i32, from_channel: i32, to_channel: i32) {
        for i in from_channel..=to_channel {
            self.sustain[i as usize] = false;
        }
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            let channel = voice.state.channel;
            if voice.sustained()
                && !voice.sostenuto()
                && channel >= from_channel
                && channel <= to_channel
            {
                voice.deactivate(sample);
            }
        }
    }

    pub fn sostenuto_on_range(&mut self, from_channel: i32, to_channel: i32) {
        for i in from_channel..=to_channel {
            self.sostenuto[i as usize] = true;
        }
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            let channel = voice.state.channel;
            if channel >= from_channel && channel <= to_channel {
                voice.set_sostenuto(true);
            }
        }
    }

    pub fn sostenuto_off_range(&mut self, sample: i32, from_channel: i32, to_channel: i32) {
        for i in from_channel..=to_channel {
            self.sostenuto[i as usize] = false;
        }
        for id in self.active_ids() {
            let voice = &self.all_voices[id.0];
            let channel = voice.state.channel;
            if channel >= from_channel && channel <= to_channel {
                let sustain = self.sustain[channel as usize];
                let voice = &mut self.all_voices[id.0];
                voice.set_sostenuto(false);
                if voice.sustained() && !sustain {
                    voice.deactivate(sample);
                }
            }
        }
    }

    /// `allNotesOffRange(sample, from_channel, to_channel)`.
    pub fn all_notes_off_range(&mut self, sample: i32, from_channel: i32, to_channel: i32) {
        self.pressed_notes.clear();
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            let channel = voice.state.channel;
            if channel >= from_channel && channel <= to_channel {
                voice.deactivate(sample);
            }
        }
    }

    // ----- expression -----

    pub fn set_aftertouch(&mut self, note: i32, aftertouch: f32, sample: i32, channel: i32) {
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            if voice.state.midi_note == note && voice.state.channel == channel {
                voice.set_aftertouch(aftertouch, sample);
            }
        }
    }

    pub fn set_channel_aftertouch(&mut self, channel: i32, aftertouch: f32, sample: i32) {
        self.pressure_values[channel as usize] = aftertouch;
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            if voice.state.channel == channel && voice.held() {
                voice.set_aftertouch(aftertouch, sample);
            }
        }
    }

    pub fn set_channel_range_aftertouch(
        &mut self,
        from_channel: i32,
        to_channel: i32,
        aftertouch: f32,
        sample: i32,
    ) {
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            let channel = voice.state.channel;
            if channel >= from_channel && channel <= to_channel {
                voice.set_aftertouch(aftertouch, sample);
            }
        }
    }

    pub fn set_channel_slide(&mut self, channel: i32, slide: f32, sample: i32) {
        self.slide_values[channel as usize] = slide;
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            if voice.state.channel == channel && voice.held() {
                voice.set_slide(slide, sample);
            }
        }
    }

    pub fn set_channel_range_slide(
        &mut self,
        from_channel: i32,
        to_channel: i32,
        slide: f32,
        sample: i32,
    ) {
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            let channel = voice.state.channel;
            if channel >= from_channel && channel <= to_channel {
                voice.set_slide(slide, sample);
            }
        }
    }

    pub fn set_mod_wheel(&mut self, value: f32, channel: i32) {
        debug_assert!((0..NUM_MIDI_CHANNELS).contains(&channel));
        self.mod_wheel_values[channel as usize] = value;
    }

    pub fn set_mod_wheel_all_channels(&mut self, value: f32) {
        self.mod_wheel_values = [value; NUM_CHANNELS];
    }

    pub fn set_pitch_wheel(&mut self, value: f32, channel: i32) {
        debug_assert!((0..NUM_MIDI_CHANNELS).contains(&channel));
        self.pitch_wheel_values[channel as usize] = value;
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            if voice.state.channel == channel && voice.held() {
                voice.set_local_pitch_bend(value);
            }
        }
    }

    pub fn set_zoned_pitch_wheel(&mut self, value: f32, from_channel: i32, to_channel: i32) {
        debug_assert!(to_channel >= from_channel);
        for i in from_channel..=to_channel {
            self.zoned_pitch_wheel_values[i as usize] = value;
        }
    }

    // ----- allocation -----

    /// A snapshot of `active_voices_` for loops whose bodies do not edit it.
    fn active_ids(&self) -> Vec<VoiceId> {
        self.active_voices.iter().collect()
    }

    /// `setPolyphony(polyphony)`.
    pub fn set_polyphony(&mut self, polyphony: i32) {
        while (self.all_voices.len() as i32) < polyphony {
            self.add_parallel_voices();
        }

        let num_voices_to_kill = self.active_voices.len() as i32 - polyphony;
        for _ in 0..num_voices_to_kill {
            if let Some(sacrifice) = self.get_voice_to_kill(polyphony) {
                self.all_voices[sacrifice.0].kill(0);
            }
        }

        self.polyphony = polyphony;
    }

    fn add_parallel_voices(&mut self) {
        let mut voice_value = PolyFloat::zero();
        for i in 0..PARALLEL_VOICES {
            voice_value.set(2 * i, i as f32);
            voice_value.set(2 * i + 1, i as f32);
        }

        let processor = self.voice_router.clone_processor();
        processor.process(1);

        let aggregate = AggregateId(self.all_aggregate_voices.len());
        let mut voices = Vec::with_capacity(PARALLEL_VOICES);
        for i in 0..PARALLEL_VOICES {
            let mut single_voice = Voice::new(aggregate);
            single_voice.set_voice_info(i, PolyFloat::equal(voice_value, (i as f32).into()));
            let id = VoiceId(self.all_voices.len());
            voices.push(id);
            self.free_voices.push_back(id);
            self.all_voices.push(single_voice);
        }

        self.all_aggregate_voices
            .push(AggregateVoice { voices, processor });
    }

    fn grab_voice(&mut self) -> Option<VoiceId> {
        let mut voice = None;
        if (self.active_voices.len() as i32) < self.polyphony
            || (self.voice_override == voice_override::KILL && !self.legato)
        {
            voice = self.grab_free_parallel_voice();
            if voice.is_none() {
                voice = self.grab_free_voice();
            }
        }

        for key_state in [
            KeyState::Released,
            KeyState::Sustained,
            KeyState::Held,
            KeyState::Triggering,
        ] {
            if voice.is_none() {
                voice = self.grab_voice_of_type(key_state);
            }
        }
        voice
    }

    fn grab_free_voice(&mut self) -> Option<VoiceId> {
        if !self.free_voices.is_empty() {
            return Some(self.free_voices.pop_front());
        }
        None
    }

    fn grab_free_parallel_voice(&mut self) -> Option<VoiceId> {
        for aggregate_voice in &self.all_aggregate_voices {
            let mut dead_voice = None;
            let mut has_active_voice = false;

            for &single_voice in &aggregate_voice.voices {
                if self.all_voices[single_voice.0].key_state == KeyState::Dead {
                    dead_voice = Some(single_voice);
                } else {
                    has_active_voice = true;
                }
            }

            if let (true, Some(dead_voice)) = (has_active_voice, dead_voice) {
                debug_assert!(self.free_voices.count(dead_voice) != 0);
                self.free_voices.remove(dead_voice);
                return Some(dead_voice);
            }
        }
        None
    }

    fn grab_voice_of_type(&mut self, key_state: KeyState) -> Option<VoiceId> {
        let end = self.active_voices.end_slot();
        let mut slot = self.active_voices.begin_slot();
        while slot != end {
            let voice = self.active_voices.slot(slot);
            if self.all_voices[voice.0].key_state == key_state {
                self.active_voices.erase_slot(slot);
                return Some(voice);
            }
            slot = self.active_voices.next_slot(slot);
        }
        None
    }

    fn get_voice_to_kill(&self, max_voices: i32) -> Option<VoiceId> {
        let mut excess_voices = self.active_voices.len() as i32 - max_voices;
        let mut released = None;
        let mut sustained = None;
        let mut held = None;

        for id in self.active_voices.iter() {
            let voice = &self.all_voices[id.0];
            if voice.state.event == VoiceEvent::VoiceKill {
                excess_voices -= 1;
            } else if released.is_none() && voice.key_state == KeyState::Released {
                released = Some(id);
            } else if sustained.is_none() && voice.key_state == KeyState::Sustained {
                sustained = Some(id);
            } else if held.is_none() {
                held = Some(id);
            }
        }

        if excess_voices <= 0 {
            return None;
        }
        released.or(sustained).or(held)
    }

    fn grab_next_unplayed_pressed_note(&mut self) -> i32 {
        let begin = self.pressed_notes.begin_slot();
        let end = self.pressed_notes.end_slot();
        let mut iter = begin;

        if self.voice_priority == voice_priority::NEWEST {
            iter = end;
            while iter != begin {
                iter = self.pressed_notes.prev_slot(iter);
                let value = self.pressed_notes.slot(iter);
                if !self.is_note_playing_on(get_note(value), get_channel(value)) {
                    break;
                }
            }
        } else {
            while iter != end {
                let value = self.pressed_notes.slot(iter);
                if !self.is_note_playing_on(get_note(value), get_channel(value)) {
                    break;
                }
                iter = self.pressed_notes.next_slot(iter);
            }
        }

        let old_note_value = self.pressed_notes.slot(iter);
        if self.voice_priority == voice_priority::ROUND_ROBIN {
            self.pressed_notes.erase_slot(iter);
            self.pressed_notes.push_back(old_note_value);
        }
        old_note_value
    }

    fn sort_voice_priority(&mut self) {
        let voices = &self.all_voices;
        match self.voice_priority {
            voice_priority::HIGHEST => {
                // voiceCompareLowestFirst / pressedCompareHighestFirst.
                self.active_voices
                    .sort(|l, r| voices[r.0].state.midi_note - voices[l.0].state.midi_note);
                self.pressed_notes.sort(|l, r| get_note(l) - get_note(r));
            }
            voice_priority::LOWEST => {
                // voiceCompareHighestFirst / pressedCompareLowestFirst.
                self.active_voices
                    .sort(|l, r| voices[l.0].state.midi_note - voices[r.0].state.midi_note);
                self.pressed_notes.sort(|l, r| get_note(r) - get_note(l));
            }
            voice_priority::OLDEST => {
                // voiceCompareNewestFirst.
                self.active_voices
                    .sort(|l, r| voices[l.0].state.note_count - voices[r.0].state.note_count);
            }
            _ => {}
        }
    }

    fn tune(&self, note: i32) -> f32 {
        match &self.tuning {
            Some(tuning) => tuning.convert_midi_note(note),
            None => note as f32,
        }
    }

    /// `setActiveNonaccumulatedOutput(output)`.
    pub fn set_active_nonaccumulated_output(&mut self, output: &Rc<Output>) {
        let Some((_, last)) = self
            .last_voice_outputs
            .iter()
            .find(|(s, _)| Rc::ptr_eq(s, output))
        else {
            return;
        };
        let pair = (output.clone(), last.clone());
        if !self
            .nonaccumulated_outputs
            .iter()
            .any(|(s, d)| Rc::ptr_eq(s, &pair.0) && Rc::ptr_eq(d, &pair.1))
        {
            self.nonaccumulated_outputs.push(pair);
        }
    }

    /// `setInactiveNonaccumulatedOutput(output)`.
    pub fn set_inactive_nonaccumulated_output(&mut self, output: &Rc<Output>) {
        let Some((_, last)) = self
            .last_voice_outputs
            .iter()
            .find(|(s, _)| Rc::ptr_eq(s, output))
        else {
            return;
        };
        let last = last.clone();
        let buffer = last.buffer();
        for sample in buffer.iter().take(last.buffer_size()) {
            sample.set(PolyFloat::zero());
        }
        if let Some(position) = self
            .nonaccumulated_outputs
            .iter()
            .position(|(s, d)| Rc::ptr_eq(s, output) && Rc::ptr_eq(d, &last))
        {
            self.nonaccumulated_outputs.remove(position);
        }
    }

    /// `getAccumulatedOutput(output)`.
    pub fn accumulated_output(&self, output: &Rc<Output>) -> Option<Rc<Output>> {
        self.accumulated_outputs
            .iter()
            .find(|(source, _)| Rc::ptr_eq(source, output))
            .map(|(_, dest)| dest.clone())
    }

    // ----- outputs -----

    pub fn voice_event(&self) -> Rc<Output> {
        self.voice_event.clone()
    }
    pub fn retrigger(&self) -> Rc<Output> {
        self.retrigger.clone()
    }
    /// `reset()` (renamed: `ProcessorImpl::reset` takes the name).
    pub fn reset_output(&self) -> Rc<Output> {
        self.reset.clone()
    }
    pub fn note(&self) -> Rc<Output> {
        self.note.clone()
    }
    pub fn last_note(&self) -> Rc<Output> {
        self.last_note.clone()
    }
    pub fn note_pressed(&self) -> Rc<Output> {
        self.note_pressed.clone()
    }
    pub fn note_count(&self) -> Rc<Output> {
        self.note_count.clone()
    }
    pub fn note_in_octave(&self) -> Rc<Output> {
        self.note_in_octave.clone()
    }
    pub fn channel(&self) -> Rc<Output> {
        self.channel.clone()
    }
    pub fn velocity(&self) -> Rc<Output> {
        self.velocity.clone()
    }
    pub fn lift(&self) -> Rc<Output> {
        self.lift.clone()
    }
    pub fn aftertouch(&self) -> Rc<Output> {
        self.aftertouch.clone()
    }
    pub fn slide(&self) -> Rc<Output> {
        self.slide.clone()
    }
    pub fn active_mask(&self) -> Rc<Output> {
        self.active_mask.clone()
    }
    pub fn pitch_wheel(&self) -> Rc<Output> {
        self.pitch_wheel.clone()
    }
    pub fn pitch_wheel_percent(&self) -> Rc<Output> {
        self.pitch_wheel_percent.clone()
    }
    pub fn local_pitch_bend(&self) -> Rc<Output> {
        self.local_pitch_bend.clone()
    }
    pub fn mod_wheel(&self) -> Rc<Output> {
        self.mod_wheel.clone()
    }
}

impl NoteHandler for VoiceHandler {
    fn all_sounds_off(&mut self) {
        self.pressed_notes.clear();
        for id in self.active_ids() {
            self.all_voices[id.0].kill(0);
            self.all_voices[id.0].mark_dead();
            self.free_voices.push_back(id);
        }
        self.active_voices.clear();
    }

    fn all_notes_off(&mut self, sample: i32) {
        self.pressed_notes.clear();
        for id in self.active_ids() {
            self.all_voices[id.0].deactivate(sample);
        }
    }

    fn all_notes_off_channel(&mut self, sample: i32, channel: i32) {
        self.pressed_notes.clear();
        for id in self.active_ids() {
            let voice = &mut self.all_voices[id.0];
            if voice.state.channel == channel {
                voice.deactivate(sample);
            }
        }
    }

    fn note_on(&mut self, note: i32, velocity: f32, sample: i32, channel: i32) {
        debug_assert!((0..NUM_MIDI_CHANNELS).contains(&channel));

        let Some(voice) = self.grab_voice() else {
            return;
        };

        let tuned_note = self.tune(note);

        let mut last_note = PolyFloat::splat(tuned_note);
        if self.last_played_note.get(0) >= 0.0 {
            last_note = self.last_played_note;
        }
        self.last_played_note = PolyFloat::splat(tuned_note);

        let note_value = combine_note_channel(note, channel);
        self.pressed_notes.remove(note_value);
        self.pressed_notes.push_back(note_value);

        self.total_notes += 1;
        let pressed = self.pressed_notes.len() as i32;
        let total_notes = self.total_notes;
        let ch = channel as usize;
        let (bend, pressure, slide) = (
            self.pitch_wheel_values[ch],
            self.pressure_values[ch],
            self.slide_values[ch],
        );
        let v = &mut self.all_voices[voice.0];
        v.activate(
            note,
            tuned_note,
            velocity,
            last_note,
            pressed,
            total_notes,
            sample,
            channel,
        );
        v.set_local_pitch_bend(bend);
        v.set_aftertouch(pressure, 0);
        v.set_slide(slide, 0);
        self.active_voices.push_back(voice);

        self.sort_voice_priority();
    }

    fn note_off(&mut self, note: i32, lift: f32, sample: i32, channel: i32) {
        self.pressed_notes
            .remove_all(combine_note_channel(note, channel));

        let end = self.active_voices.end_slot();
        let mut slot = self.active_voices.begin_slot();
        while slot != end {
            let voice = self.active_voices.slot(slot);
            self.note_off_voice(voice, note, lift, sample, channel);
            slot = self.active_voices.next_slot(slot);
        }

        self.sort_voice_priority();
    }
}

impl VoiceHandler {
    /// The body of `noteOff`'s range-for over `active_voices_`.
    fn note_off_voice(&mut self, voice: VoiceId, note: i32, lift: f32, sample: i32, channel: i32) {
        let state = self.all_voices[voice.0].state;
        if state.midi_note != note || state.channel != channel {
            return;
        }

        if self.sustain[channel as usize] {
            self.all_voices[voice.0].sustain();
            self.all_voices[voice.0].set_lift_velocity(lift);
        } else if self.polyphony <= self.pressed_notes.len() as i32
            && state.event != VoiceEvent::VoiceKill
        {
            let mut new_voice = Some(voice);
            if self.voice_override == voice_override::KILL {
                self.all_voices[voice.0].kill(0);
                new_voice = self.grab_voice();
            } else {
                self.active_voices.remove(voice);
            }
            let new_voice = new_voice.expect("VoiceHandler::noteOff found no voice to reuse");

            if self.voice_priority == voice_priority::NEWEST {
                self.active_voices.push_front(new_voice);
            } else {
                self.active_voices.push_back(new_voice);
            }

            let old_note_value = self.grab_next_unplayed_pressed_note();

            let old_note = get_note(old_note_value) as f32;
            let old_channel = get_channel(old_note_value);
            let tuned_note = self.tune(old_note as i32);

            self.total_notes += 1;
            let velocity = self.all_voices[voice.0].state.velocity;
            let last_played_note = self.last_played_note;
            let pressed = self.pressed_notes.len() as i32 + 1;
            let total_notes = self.total_notes;
            self.all_voices[new_voice.0].activate(
                old_note as i32,
                tuned_note,
                velocity,
                last_played_note,
                pressed,
                total_notes,
                sample,
                old_channel,
            );
            let ch = channel as usize;
            let (bend, pressure, slide) = (
                self.pitch_wheel_values[ch],
                self.pressure_values[ch],
                self.slide_values[ch],
            );
            let v = &mut self.all_voices[voice.0];
            v.set_local_pitch_bend(bend);
            v.set_aftertouch(pressure, 0);
            v.set_slide(slide, 0);
        } else {
            self.all_voices[voice.0].deactivate(sample);
            self.all_voices[voice.0].set_lift_velocity(lift);
        }
    }

    /// `shouldAccumulate(output)` (the `VoiceHandler` version).
    pub fn should_accumulate(output: &Output) -> bool {
        output.buffer_size() > 1 || output.owner().is_some_and(|owner| !owner.is_control_rate())
    }

    /// `registerOutput(output)` from inside the kernel (`this` is the
    /// handler node); see the free function [`register_output`].
    pub fn register_output(
        &mut self,
        this: &Processor,
        output: &Rc<Output>,
        accumulate: bool,
    ) -> Rc<Output> {
        let new_output = Output::new(output.buffer_size(), 1);
        new_output.set_owner(Some(&this.this_ref()));
        this.register_output(new_output.clone());
        if accumulate {
            self.accumulated_outputs
                .push((output.clone(), new_output.clone()));
        } else {
            self.last_voice_outputs
                .push((output.clone(), new_output.clone()));
        }
        new_output
    }

    /// `registerControlRateOutput(output, active)` from inside the kernel.
    pub fn register_control_rate_output(
        &mut self,
        this: &Processor,
        output: &Rc<Output>,
        active: bool,
    ) -> Rc<Output> {
        let new_output = Output::new_control_rate();
        new_output.set_owner(Some(&this.this_ref()));
        this.register_output(new_output.clone());
        self.last_voice_outputs
            .push((output.clone(), new_output.clone()));
        if active {
            self.nonaccumulated_outputs
                .push((output.clone(), new_output.clone()));
        }
        new_output
    }
}

impl ProcessorImpl for VoiceHandler {
    fn clone_box(&self) -> Box<dyn ProcessorImpl> {
        panic!("VoiceHandler::clone is not supported (upstream asserts)")
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        VoiceHandler::process(self, this, num_samples);
    }

    fn init(&mut self, this: &Processor) {
        VoiceHandler::init(self, this);
    }

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        VoiceHandler::set_sample_rate(self, this, sample_rate);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        VoiceHandler::set_oversample_amount(self, this, oversample);
    }

    fn reset_feedbacks(&mut self, _this: &Processor, reset_mask: PolyMask) {
        VoiceHandler::reset_feedbacks(self, reset_mask);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// A kernel that is (or embeds) a `VoiceHandler`.
pub trait HasVoiceHandler: ProcessorImpl {
    fn voice_handler(&mut self) -> &mut VoiceHandler;
}

impl HasVoiceHandler for VoiceHandler {
    fn voice_handler(&mut self) -> &mut VoiceHandler {
        self
    }
}

/// Runs `f` on the `VoiceHandler` inside `node`'s kernel `K`.
pub fn with_voice_handler<K: HasVoiceHandler, R>(
    node: &Processor,
    f: impl FnOnce(&mut VoiceHandler, &Processor) -> R,
) -> R {
    node.with::<K, R>(|kernel, this| f(kernel.voice_handler(), this))
}

/// `VoiceHandler::addProcessor(processor)`: into the voice router, at the
/// handler's sample rate.
pub fn add_processor<K: HasVoiceHandler>(node: &Processor, processor: &ProcessorRef) {
    let voice_router = with_voice_handler::<K, _>(node, |vh, _| vh.voice_router.clone());
    processor.set_sample_rate(node.sample_rate());
    voice_router.add_processor(processor);
}

/// `VoiceHandler::addIdleProcessor(processor)`.
pub fn add_idle_processor<K: HasVoiceHandler>(node: &Processor, processor: &ProcessorRef) {
    let voice_router = with_voice_handler::<K, _>(node, |vh, _| vh.voice_router.clone());
    processor.set_sample_rate(node.sample_rate());
    voice_router.add_idle_processor(processor);
}

/// `VoiceHandler::removeProcessor(processor)`.
pub fn remove_processor<K: HasVoiceHandler>(node: &Processor, processor: &ProcessorRef) {
    let voice_router = with_voice_handler::<K, _>(node, |vh, _| vh.voice_router.clone());
    voice_router.remove_processor(processor);
}

/// `VoiceHandler::addGlobalProcessor(processor)`.
pub fn add_global_processor<K: HasVoiceHandler>(node: &Processor, processor: &ProcessorRef) {
    let global_router = with_voice_handler::<K, _>(node, |vh, _| vh.global_router.clone());
    global_router.add_processor(processor);
}

/// `VoiceHandler::removeGlobalProcessor(processor)`.
pub fn remove_global_processor<K: HasVoiceHandler>(node: &Processor, processor: &ProcessorRef) {
    let global_router = with_voice_handler::<K, _>(node, |vh, _| vh.global_router.clone());
    global_router.remove_processor(processor);
}

/// `VoiceHandler::registerOutput(output)`: a new output of the handler that
/// collects `output` from every voice. `accumulate` is the kernel's
/// `shouldAccumulate(output)` ([`VoiceHandler::should_accumulate`] unless a
/// subclass overrides it): summed over voices, or the last voice's value.
pub fn register_output<K: HasVoiceHandler>(
    node: &Processor,
    output: &Rc<Output>,
    accumulate: bool,
) -> Rc<Output> {
    with_voice_handler::<K, _>(node, |vh, this| {
        vh.register_output(this, output, accumulate)
    })
}

/// `VoiceHandler::registerControlRateOutput(output, active)`.
pub fn register_control_rate_output<K: HasVoiceHandler>(
    node: &Processor,
    output: &Rc<Output>,
    active: bool,
) -> Rc<Output> {
    with_voice_handler::<K, _>(node, |vh, this| {
        vh.register_control_rate_output(this, output, active)
    })
}

/// The handler's voice router, as a weak reference (for hooks).
pub fn voice_router_of<K: HasVoiceHandler>(node: &Processor) -> Weak<Processor> {
    with_voice_handler::<K, _>(node, |vh, _| Rc::downgrade(&vh.voice_router))
}
