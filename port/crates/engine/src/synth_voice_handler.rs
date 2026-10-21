// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::SynthVoiceHandler`: the synth's polyphonic voice graph on top of
//! [`VoiceHandler`].
//!
//! Ports `vital/src/synthesis/modules/synth_voice_handler.{h,cpp}`.
//!
//! # What is built so far
//!
//! `init` builds the parts of the upstream graph whose processors are
//! ported:
//!
//! - `createNoteArticulation`: portamento (`PortamentoSlope`), pitch bend
//!   with `pitch_bend_range`, local (MPE) pitch bend, `voice_tune` and
//!   `voice_transpose` into the bent MIDI note, plus the `note` percentage
//!   and the note relative to the reference (`midi_offset_output`);
//! - the macros, the `note`/`velocity`/expression modulation sources and
//!   status outputs, `stereo`, `num_voices`, and the poly modulation
//!   readouts;
//! - `noteOn`/`noteOff` with the `note_retrigger` output, and `process`
//!   (feedback reset, status outputs).
//!
//! The producers, the LFOs, envelopes and random sources, the filters, the
//! voice output stage (whose amplitude is the voice killer) and the
//! modulation bank are added as the oscillator, modulation, filter and
//! effect crates merge. Until then no voice killer is set, so released
//! voices stay active.

use std::collections::BTreeMap;
use std::rc::Rc;

use sas_dsp_core::operators::{self, cr};
use sas_dsp_core::utilities::portamento_slope::PortamentoSlope;
use sas_dsp_core::{constants, Output, Processor, ProcessorImpl, ProcessorRef, SynthModule, Value};
use sas_simd::common::{MIDI_SIZE, MIDI_TRACK_CENTER};
use sas_simd::PolyMask;

use crate::voice_handler::{HasVoiceHandler, NoteHandler, VoiceHandler, MAX_POLYPHONY};

/// `kNumMacros`.
pub const NUM_MACROS: usize = 4;

/// Kernel of `vital::SynthVoiceHandler`.
pub struct SynthVoiceHandler {
    base: VoiceHandler,
    beats_per_second: Rc<Output>,

    note_from_reference: ProcessorRef,
    midi_offset_output: Option<Rc<Output>>,
    bent_midi: Option<ProcessorRef>,
    current_midi_note: Option<ProcessorRef>,
    note_percentage: Option<ProcessorRef>,
    stereo: Option<ProcessorRef>,

    output: ProcessorRef,
    direct_output: ProcessorRef,
    accumulated_output: Option<Rc<Output>>,
    accumulated_direct_output: Option<Rc<Output>>,

    num_voices: Rc<Output>,
    note_retriggered: Rc<Output>,
    poly_readouts: BTreeMap<String, Rc<Output>>,
    last_active_voice_mask: PolyMask,

    /// Upstream's function-local `static const cr::Value`s.
    max_midi_invert: ProcessorRef,
    reference_adjust: ProcessorRef,
}

impl SynthVoiceHandler {
    /// `SynthVoiceHandler(beats_per_second)`: the handler node, with its
    /// output and `midi_offset_output` registered (as the upstream
    /// constructor does). Call `init` (through the node) to build the graph.
    pub fn create(beats_per_second: &Rc<Output>) -> ProcessorRef {
        let base = VoiceHandler::new(MAX_POLYPHONY);
        let (voice_router, global_router) = base.routers();
        let kernel = SynthVoiceHandler {
            base,
            beats_per_second: beats_per_second.clone(),
            note_from_reference: cr::Add::create(),
            midi_offset_output: None,
            bent_midi: None,
            current_midi_note: None,
            note_percentage: None,
            stereo: None,
            output: operators::Multiply::create(),
            direct_output: operators::Multiply::create(),
            accumulated_output: None,
            accumulated_direct_output: None,
            num_voices: Output::new_default(),
            note_retriggered: Output::new_default(),
            poly_readouts: BTreeMap::new(),
            last_active_voice_mask: PolyMask::zero(),
            max_midi_invert: Value::create_control_rate(1.0 / (MIDI_SIZE - 1) as f32),
            reference_adjust: Value::create_control_rate(-(MIDI_TRACK_CENTER as f32)),
        };
        let node = SynthModule::create_with(kernel, VoiceHandler::NUM_INPUTS, 0, false);
        VoiceHandler::attach(&node, &voice_router, &global_router);

        node.with::<SynthVoiceHandler, _>(|svh, this| {
            let output = svh.output.output(0);
            let accumulate = svh.should_accumulate(&output);
            svh.accumulated_output = Some(svh.base.register_output(this, &output, accumulate));

            let direct = svh.direct_output.output(0);
            let accumulate = svh.should_accumulate(&direct);
            svh.accumulated_direct_output =
                Some(svh.base.register_output(this, &direct, accumulate));

            let midi_offset = svh.note_from_reference.output(0);
            svh.midi_offset_output = Some(svh.base.register_control_rate_output(
                this,
                &midi_offset,
                true,
            ));
        });
        node
    }

    /// `shouldAccumulate(output)`. The amplitude envelope's outputs are
    /// excluded upstream; the envelope is not built yet.
    pub fn should_accumulate(&self, output: &Output) -> bool {
        VoiceHandler::should_accumulate(output)
    }

    /// `SynthVoiceHandler::init`.
    pub fn init(&mut self, this: &Processor) {
        self.create_note_articulation(this);
        let macros: Vec<Rc<Output>> = (0..NUM_MACROS)
            .map(|i| {
                this.create_mono_mod_control(
                    &format!("macro_control_{}", i + 1),
                    false,
                    false,
                    None,
                )
            })
            .collect();
        self.create_expression_sources(this);

        VoiceHandler::init(&mut self.base, this);
        self.setup_poly_modulation_readouts(this);

        for (i, source) in macros.iter().enumerate() {
            let name = format!("macro_control_{}", i + 1);
            this.add_modulation_source(&name, source);
            this.create_status_output(&name, source);
        }

        let stereo = self.stereo.as_ref().expect("stereo source").output(0);
        this.add_modulation_source("stereo", &stereo);
        this.create_status_output("stereo", &stereo);
        this.create_status_output("num_voices", &self.num_voices);
    }

    fn create_note_articulation(&mut self, this: &Processor) {
        let portamento = this.create_poly_mod_control("portamento_time", false, false, None, None);
        let portamento_slope =
            this.create_poly_mod_control("portamento_slope", false, false, None, None);
        let portamento_force = this.create_base_control("portamento_force", false, false);
        let portamento_scale = this.create_base_control("portamento_scale", false, false);

        let current_midi_note = PortamentoSlope::create();
        current_midi_note.plug(self.base.last_note(), PortamentoSlope::SOURCE);
        current_midi_note.plug(self.base.note(), PortamentoSlope::TARGET);
        current_midi_note.plug(&portamento_force, PortamentoSlope::PORTAMENTO_FORCE);
        current_midi_note.plug(&portamento_scale, PortamentoSlope::PORTAMENTO_SCALE);
        current_midi_note.plug(&portamento, PortamentoSlope::RUN_SECONDS);
        current_midi_note.plug(&portamento_slope, PortamentoSlope::SLOPE_POWER);
        current_midi_note.plug(self.base.voice_event(), PortamentoSlope::RESET);
        current_midi_note.plug(self.base.note_pressed(), PortamentoSlope::NUM_NOTES_PRESSED);
        self.base.set_voice_midi(current_midi_note.output(0));
        this.add_processor(&current_midi_note);

        let pitch_bend_range =
            this.create_poly_mod_control("pitch_bend_range", false, false, None, None);
        let voice_tune = this.create_poly_mod_control("voice_tune", false, false, None, None);
        let voice_transpose =
            this.create_poly_mod_control("voice_transpose", false, false, None, None);
        let pitch_bend = cr::Multiply::create();
        pitch_bend.plug(self.base.pitch_wheel(), 0);
        pitch_bend.plug(&pitch_bend_range, 1);
        let bent_midi = cr::VariableAdd::create(0);
        bent_midi.plug_next(&current_midi_note);
        bent_midi.plug_next(&pitch_bend);
        bent_midi.plug_next(self.base.local_pitch_bend());
        bent_midi.plug_next(&voice_tune);
        bent_midi.plug_next(&voice_transpose);

        let note_percentage = cr::Multiply::create();
        note_percentage.plug(&self.max_midi_invert, 0);
        note_percentage.plug(&bent_midi, 1);
        this.add_processor(&note_percentage);

        self.note_from_reference.plug(&self.reference_adjust, 0);
        self.note_from_reference.plug(&bent_midi, 1);
        this.add_processor(&self.note_from_reference);

        this.add_processor(&pitch_bend);
        this.add_processor(&bent_midi);

        self.current_midi_note = Some(current_midi_note);
        self.bent_midi = Some(bent_midi);
        self.note_percentage = Some(note_percentage);
    }

    /// The tail of `createModulators` (after the LFOs, envelopes and random
    /// sources): `stereo` and the note and expression sources.
    fn create_expression_sources(&mut self, this: &Processor) {
        let stereo = Value::create_control_rate(constants::left_one());
        this.add_idle_mono_processor(&stereo);
        self.stereo = Some(stereo);

        let note = self
            .note_percentage
            .as_ref()
            .expect("note articulation")
            .output(0);
        let sources = [
            ("note", note),
            ("note_in_octave", self.base.note_in_octave()),
            ("aftertouch", self.base.aftertouch()),
            ("velocity", self.base.velocity()),
            ("slide", self.base.slide()),
            ("lift", self.base.lift()),
            ("mod_wheel", self.base.mod_wheel()),
            ("pitch_wheel", self.base.pitch_wheel_percent()),
        ];
        for (name, source) in &sources {
            this.add_modulation_source(name, source);
        }
        for (name, source) in &sources {
            this.create_status_output(name, source);
        }
    }

    fn setup_poly_modulation_readouts(&mut self, this: &Processor) {
        for (name, output) in this.get_poly_modulations() {
            let active = output.owner().is_some_and(|owner| owner.enabled());
            let readout = self
                .base
                .register_control_rate_output(this, &output, active);
            self.poly_readouts.insert(name, readout);
        }
    }

    /// `SynthVoiceHandler::process`.
    pub fn process(&mut self, this: &Processor, num_samples: usize) {
        let reset_mask = self.base.reset_output().trigger_mask();
        if reset_mask.any_mask() != 0 {
            self.base.reset_feedbacks(reset_mask);
        }

        self.base.process(this, num_samples);
        let num_voices = self.base.num_active_voices();
        self.num_voices.set_at(0, (num_voices as f32).into());
        self.note_retriggered.clear_trigger();

        let status_outputs = this.module_data().borrow().status_outputs.clone();
        if num_voices == 0 {
            for status in status_outputs.values() {
                status.clear();
            }
        } else {
            self.last_active_voice_mask = self.base.current_voice_mask();
            for status in status_outputs.values() {
                status.update_masked(self.last_active_voice_mask);
            }
        }
    }

    /// `getPolyModulations()` (the readouts).
    pub fn poly_modulations(&self) -> &BTreeMap<String, Rc<Output>> {
        &self.poly_readouts
    }

    /// `midi_offset_output()`.
    pub fn midi_offset_output(&self) -> Rc<Output> {
        self.midi_offset_output
            .clone()
            .expect("registered in create")
    }

    /// `note_retrigger()`.
    pub fn note_retrigger(&self) -> Rc<Output> {
        self.note_retriggered.clone()
    }

    /// The accumulated voice output (`output()` of the handler node).
    pub fn accumulated_output(&self) -> Rc<Output> {
        self.accumulated_output
            .clone()
            .expect("registered in create")
    }

    /// `getDirectOutput()`.
    pub fn direct_output(&self) -> Rc<Output> {
        self.accumulated_direct_output
            .clone()
            .expect("registered in create")
    }

    pub fn beats_per_second(&self) -> Rc<Output> {
        self.beats_per_second.clone()
    }

    /// `correctToTime(seconds)`: realigns the LFOs and random LFOs, which are
    /// not built yet.
    pub fn correct_to_time(&mut self, _seconds: f64) {}

    /// `disableUnnecessaryModSources()`: disables the LFOs, envelopes 2–6
    /// and random sources, which are not built yet.
    pub fn disable_unnecessary_mod_sources(&mut self) {}
}

impl NoteHandler for SynthVoiceHandler {
    fn all_sounds_off(&mut self) {
        self.base.all_sounds_off();
    }

    fn all_notes_off(&mut self, sample: i32) {
        self.base.all_notes_off(sample);
    }

    fn all_notes_off_channel(&mut self, sample: i32, channel: i32) {
        self.base.all_notes_off_channel(sample, channel);
    }

    fn note_on(&mut self, note: i32, velocity: f32, sample: i32, channel: i32) {
        if (self.base.num_pressed_notes() as i32) < self.base.polyphony() || !self.base.legato() {
            self.note_retriggered.trigger(
                constants::full_mask(),
                (note as f32).into(),
                (sample as u32).into(),
            );
        }
        self.base.note_on(note, velocity, sample, channel);
    }

    fn note_off(&mut self, note: i32, lift: f32, sample: i32, channel: i32) {
        if self.base.num_pressed_notes() as i32 > self.base.polyphony()
            && self.base.is_note_playing(note)
            && !self.base.legato()
        {
            self.note_retriggered.trigger(
                constants::full_mask(),
                (note as f32).into(),
                (sample as u32).into(),
            );
        }
        self.base.note_off(note, lift, sample, channel);
    }
}

impl HasVoiceHandler for SynthVoiceHandler {
    fn voice_handler(&mut self) -> &mut VoiceHandler {
        &mut self.base
    }
}

impl ProcessorImpl for SynthVoiceHandler {
    fn clone_box(&self) -> Box<dyn ProcessorImpl> {
        panic!("SynthVoiceHandler::clone is not supported (upstream asserts)")
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        SynthVoiceHandler::process(self, this, num_samples);
    }

    fn init(&mut self, this: &Processor) {
        SynthVoiceHandler::init(self, this);
    }

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        self.base.set_sample_rate(this, sample_rate);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        self.base.set_oversample_amount(this, oversample);
    }

    fn reset_feedbacks(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.base.reset_feedbacks(reset_mask);
    }

    fn correct_to_time(&mut self, _this: &Processor, seconds: f64) {
        SynthVoiceHandler::correct_to_time(self, seconds);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
