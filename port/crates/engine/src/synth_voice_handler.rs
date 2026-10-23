// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::SynthVoiceHandler`: the synth's polyphonic voice graph on top of
//! [`VoiceHandler`].
//!
//! Ports `vital/src/synthesis/modules/synth_voice_handler.{h,cpp}`.
//!
//! `init` builds the whole upstream voice graph in upstream order:
//! note articulation, producers, modulators (LFOs, envelopes, random
//! sources, expression sources), filters, the voice output stage (whose
//! amplitude is the voice killer) and the modulation bank's processors.
//! The order matters beyond the graph itself: every oscillator, sample
//! source and random modulator takes the next engine RNG seed when it is
//! constructed or cloned, so each voice's seeds depend on everything built
//! before it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use sas_common::line_generator::LineGenerator;
use sas_dsp_core::operators::{self, cr};
use sas_dsp_core::utilities::portamento_slope::PortamentoSlope;
use sas_dsp_core::{constants, Output, Processor, ProcessorImpl, ProcessorRef, SynthModule, Value};
use sas_filters::modules::filters_module::FiltersModule;
use sas_modulation::{
    EnvelopeModule, LfoModule, ModulationConnectionProcessor, RandomLfoModule, TriggerRandom,
};
use sas_oscillators::producers_module::{self, ProducersModule};
use sas_oscillators::{SharedSample, SharedWavetable};
use sas_preset::constants::{NUM_ENVELOPES, NUM_LFOS, NUM_RANDOM_LFOS};
use sas_simd::common::{MIDI_SIZE, MIDI_TRACK_CENTER};
use sas_simd::{poly_utils, PolyMask};

use crate::modulation_bank::ModulationConnectionBank;
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

    producers: Option<ProcessorRef>,
    lfos: Vec<ProcessorRef>,
    lfo_sources: Vec<Rc<RefCell<LineGenerator>>>,
    envelopes: Vec<ProcessorRef>,
    random: Option<ProcessorRef>,
    random_lfos: Vec<ProcessorRef>,
    filters_module: Option<ProcessorRef>,
    amplitude_envelope: Option<ProcessorRef>,
    amplitude: Option<ProcessorRef>,
    modulation_bank: ModulationConnectionBank,
    enabled_modulation_processors: Vec<ProcessorRef>,

    num_voices: Rc<Output>,
    note_retriggered: Rc<Output>,
    poly_readouts: BTreeMap<String, Rc<Output>>,
    last_active_voice_mask: PolyMask,

    /// Upstream's function-local `static const cr::Value`s and
    /// `constants::kValueOne`.
    max_midi_invert: ProcessorRef,
    reference_adjust: ProcessorRef,
    value_one: ProcessorRef,
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
            producers: None,
            lfos: Vec::new(),
            lfo_sources: Vec::new(),
            envelopes: Vec::new(),
            random: None,
            random_lfos: Vec::new(),
            filters_module: None,
            amplitude_envelope: None,
            amplitude: None,
            modulation_bank: ModulationConnectionBank::new(),
            enabled_modulation_processors: Vec::new(),
            num_voices: Output::new_default(),
            note_retriggered: Output::new_default(),
            poly_readouts: BTreeMap::new(),
            last_active_voice_mask: PolyMask::zero(),
            max_midi_invert: Value::create_control_rate(1.0 / (MIDI_SIZE - 1) as f32),
            reference_adjust: Value::create_control_rate(-(MIDI_TRACK_CENTER as f32)),
            value_one: Value::create_control_rate(1.0),
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

    /// `shouldAccumulate(output)`: the amplitude envelope's outputs are
    /// never accumulated.
    pub fn should_accumulate(&self, output: &Output) -> bool {
        let from_amplitude_envelope = match (&self.amplitude_envelope, output.owner()) {
            (Some(envelope), Some(owner)) => Rc::ptr_eq(envelope, &owner),
            _ => false,
        };
        !from_amplitude_envelope && VoiceHandler::should_accumulate(output)
    }

    /// `SynthVoiceHandler::init`.
    pub fn init(&mut self, this: &Processor) {
        self.create_note_articulation(this);
        self.create_producers(this);
        self.create_modulators(this);
        let keytrack = self.note_from_reference.output(0);
        self.create_filters(this, &keytrack);
        self.create_voice_output(this);

        let producers = self.producers.clone().expect("producers");
        let amplitude = self.amplitude.clone().expect("amplitude");
        let voice_sum = operators::Add::create();
        voice_sum.plug(self.filters_module.as_ref().expect("filters"), 0);
        voice_sum.plug(producers.output(producers_module::RAW_OUT), 1);
        self.output.plug(&voice_sum, 0);
        self.output.plug(&amplitude, 1);
        self.direct_output
            .plug(producers.output(producers_module::DIRECT_OUT), 0);
        self.direct_output.plug(&amplitude, 1);

        this.add_processor(&voice_sum);
        this.add_processor(&self.output);
        this.add_processor(&self.direct_output);

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

        self.base.set_voice_killer(Some(amplitude.output(0)));

        for i in 0..self.modulation_bank.num_connections() {
            let processor = self.modulation_bank.at_index(i).processor.clone();
            processor.plug(
                self.base.reset_output(),
                ModulationConnectionProcessor::RESET,
            );

            let number = i + 1;
            let amount_name = format!("modulation_{number}_amount");
            let amount = this.create_poly_mod_control(&amount_name, false, false, None, None);
            processor.plug(&amount, ModulationConnectionProcessor::MODULATION_AMOUNT);
            let base_value = this.module_data().borrow().controls[&amount_name].clone();
            processor.with::<ModulationConnectionProcessor, _>(|kernel, _| {
                kernel.initialize_base_value(base_value)
            });

            let power = this.create_poly_mod_control(
                &format!("modulation_{number}_power"),
                false,
                false,
                None,
                None,
            );
            processor.plug(&power, ModulationConnectionProcessor::MODULATION_POWER);

            this.add_processor(&processor);
            this.add_submodule(&processor);
            processor.enable(false);
        }

        VoiceHandler::init(&mut self.base, this);
        let filters = self.filters_module.clone().expect("filters");
        let (filter1_on, filter2_on) = filters.with::<FiltersModule, _>(|filters, _| {
            (
                filters.get_filter_1_on_value(),
                filters.get_filter_2_on_value(),
            )
        });
        producers.with::<ProducersModule, _>(|producers, _| {
            producers.set_filter1_on(filter1_on);
            producers.set_filter2_on(filter2_on);
        });
        self.setup_poly_modulation_readouts(this);

        for (i, source) in macros.iter().enumerate() {
            let name = format!("macro_control_{}", i + 1);
            this.add_modulation_source(&name, source);
            this.create_status_output(&name, source);
        }

        for (i, random_lfo) in self.random_lfos.iter().enumerate() {
            let name = format!("random_{}", i + 1);
            this.add_modulation_source(&name, &random_lfo.output(0));
            this.create_status_output(&name, &random_lfo.output(0));
        }

        let random = self.random.as_ref().expect("random").output(0);
        let stereo = self.stereo.as_ref().expect("stereo source").output(0);
        this.add_modulation_source("random", &random);
        this.add_modulation_source("stereo", &stereo);

        this.create_status_output("random", &random);
        this.create_status_output("stereo", &stereo);
        let sample_phase =
            producers.with::<ProducersModule, _>(|producers, _| producers.sample_phase_output());
        this.create_status_output("sample_phase", &sample_phase);
        this.create_status_output("num_voices", &self.num_voices);

        for i in 0..self.modulation_bank.num_connections() {
            let processor = &self.modulation_bank.at_index(i).processor;
            let number = i + 1;
            this.create_status_output(
                &format!("modulation_source_{number}"),
                &processor.output(ModulationConnectionProcessor::MODULATION_SOURCE),
            );
            this.create_status_output(
                &format!("modulation_amount_{number}"),
                &processor.output(ModulationConnectionProcessor::MODULATION_PRE_SCALE),
            );
        }
    }

    /// `prepareDestroy()`: takes the bank's processors out of the router.
    pub fn prepare_destroy(&mut self, this: &Processor) {
        for i in 0..self.modulation_bank.num_connections() {
            this.remove_processor(&self.modulation_bank.at_index(i).processor);
        }
    }

    fn create_producers(&mut self, this: &Processor) {
        let producers = ProducersModule::create();
        producers.plug(self.base.reset_output(), producers_module::RESET);
        producers.plug(self.base.retrigger(), producers_module::RETRIGGER);
        producers.plug(
            self.bent_midi.as_ref().expect("bent midi"),
            producers_module::MIDI,
        );
        producers.plug(self.base.note_count(), producers_module::NOTE_COUNT);
        producers.plug(self.base.active_mask(), producers_module::ACTIVE_VOICES);
        this.add_submodule(&producers);
        this.add_processor(&producers);
        self.producers = Some(producers);
    }

    fn create_modulators(&mut self, this: &Processor) {
        let bent_midi = self.bent_midi.clone().expect("bent midi");
        for i in 0..NUM_LFOS {
            let source = Rc::new(RefCell::new(LineGenerator::default()));
            {
                let mut source = source.borrow_mut();
                source.set_loop(false);
                source.init_triangle();
            }
            let prefix = format!("lfo_{}", i + 1);
            let lfo = LfoModule::create(&prefix, source.clone(), &self.beats_per_second);
            this.add_submodule(&lfo);
            this.add_processor(&lfo);
            lfo.plug(self.base.retrigger(), LfoModule::NOTE_TRIGGER);
            lfo.plug(self.base.note_count(), LfoModule::NOTE_COUNT);
            lfo.plug(&bent_midi, LfoModule::MIDI);

            let value = lfo.output(LfoModule::VALUE);
            this.add_modulation_source(&prefix, &value);
            this.create_status_output(&prefix, &value);
            this.create_status_output(
                &format!("{prefix}_phase"),
                &lfo.output(LfoModule::OSC_PHASE),
            );
            this.create_status_output(
                &format!("{prefix}_frequency"),
                &lfo.output(LfoModule::OSC_FREQUENCY),
            );
            self.lfos.push(lfo);
            self.lfo_sources.push(source);
        }

        for i in 0..NUM_ENVELOPES {
            let prefix = format!("env_{}", i + 1);
            let envelope = EnvelopeModule::create(&prefix, i == 0);
            envelope.plug(self.base.retrigger(), EnvelopeModule::TRIGGER);
            this.add_submodule(&envelope);
            this.add_processor(&envelope);

            this.add_modulation_source(&prefix, &envelope.output(0));
            this.create_status_output(&prefix, &envelope.output(EnvelopeModule::VALUE));
            this.create_status_output(
                &format!("{prefix}_phase"),
                &envelope.output(EnvelopeModule::PHASE),
            );
            self.envelopes.push(envelope);
        }

        let random = TriggerRandom::create();
        random.plug(self.base.retrigger(), TriggerRandom::RESET);
        this.add_processor(&random);
        self.random = Some(random);

        for i in 0..NUM_RANDOM_LFOS {
            let name = format!("random_{}", i + 1);
            let random_lfo = RandomLfoModule::create(&name, &self.beats_per_second);
            random_lfo.plug(self.base.retrigger(), RandomLfoModule::NOTE_TRIGGER);
            random_lfo.plug(&bent_midi, RandomLfoModule::MIDI);
            this.add_submodule(&random_lfo);
            this.add_processor(&random_lfo);
            self.random_lfos.push(random_lfo);
        }

        self.create_expression_sources(this);
    }

    fn create_filters(&mut self, this: &Processor, keytrack: &Rc<Output>) {
        let filters = FiltersModule::create();
        this.add_submodule(&filters);
        this.add_processor(&filters);

        let producers = self.producers.clone().expect("producers");
        filters.plug(
            producers.output(producers_module::TO_FILTER1),
            FiltersModule::FILTER_1_INPUT,
        );
        filters.plug(
            producers.output(producers_module::TO_FILTER2),
            FiltersModule::FILTER_2_INPUT,
        );
        filters.plug(self.base.reset_output(), FiltersModule::RESET);
        filters.plug(keytrack, FiltersModule::KEYTRACK);
        filters.plug(
            self.bent_midi.as_ref().expect("bent midi"),
            FiltersModule::MIDI,
        );
        self.filters_module = Some(filters);
    }

    fn create_voice_output(&mut self, this: &Processor) {
        let velocity_track_amount =
            this.create_poly_mod_control("velocity_track", false, false, None, None);
        let velocity_track_mult = cr::Interpolate::create();
        velocity_track_mult.plug(&self.value_one, cr::Interpolate::FROM);
        velocity_track_mult.plug(self.base.velocity(), cr::Interpolate::TO);
        velocity_track_mult.plug(&velocity_track_amount, cr::Interpolate::FRACTIONAL);
        this.add_processor(&velocity_track_mult);

        let voice_amplitude =
            this.create_poly_mod_control("voice_amplitude", false, false, None, None);
        let amplitude = cr::Multiply::create();
        amplitude.plug(&velocity_track_mult, 0);
        amplitude.plug(&voice_amplitude, 1);
        this.add_processor(&amplitude);

        let amplitude_envelope = self.envelopes[0].clone();
        amplitude_envelope.set_control_rate(false);

        let control_amplitude = operators::SmoothMultiply::create();
        control_amplitude.plug(
            amplitude_envelope.output(EnvelopeModule::VALUE),
            operators::smooth_multiply::AUDIO_RATE,
        );
        control_amplitude.plug(&amplitude, operators::smooth_multiply::CONTROL_RATE);
        control_amplitude.plug(self.base.reset_output(), operators::smooth_multiply::RESET);

        let squared = operators::Square::create();
        squared.plug(&control_amplitude, 0);

        this.add_processor(&control_amplitude);
        this.add_processor(&squared);
        self.amplitude_envelope = Some(amplitude_envelope);
        self.amplitude = Some(squared);
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

        if num_voices == 0 {
            for status in this.module_data().borrow().status_outputs.values() {
                status.clear();
            }
        } else {
            self.last_active_voice_mask = self.base.current_voice_mask();
            for status in this.module_data().borrow().status_outputs.values() {
                status.update_masked(self.last_active_voice_mask);
            }

            let mask = self.last_active_voice_mask;
            for processor in &self.enabled_modulation_processors {
                let output = processor.output(0);
                let single = processor.is_control_rate()
                    || processor.with::<ModulationConnectionProcessor, _>(|kernel, _| {
                        kernel.is_polyphonic_modulation()
                    });
                let count = if single { 1 } else { num_samples };
                for i in 0..count {
                    let masked = output.at(i) & mask;
                    output.set_at(i, masked + poly_utils::swap_voices(masked));
                }
            }
        }
    }

    /// `enableModulationConnection(processor)`.
    pub fn enable_modulation_connection(&mut self, processor: &ProcessorRef) {
        self.enabled_modulation_processors.push(processor.clone());
    }

    /// `disableModulationConnection(processor)`.
    pub fn disable_modulation_connection(&mut self, processor: &ProcessorRef) {
        self.enabled_modulation_processors
            .retain(|enabled| !Rc::ptr_eq(enabled, processor));
    }

    /// `enabledModulationConnection()`.
    pub fn enabled_modulation_connections(&self) -> &[ProcessorRef] {
        &self.enabled_modulation_processors
    }

    /// `getModulationBank()`.
    pub fn modulation_bank(&mut self) -> &mut ModulationConnectionBank {
        &mut self.modulation_bank
    }

    /// `getWavetable(index)`.
    pub fn wavetable(&self, index: usize) -> SharedWavetable {
        self.producers
            .as_ref()
            .expect("producers")
            .with::<ProducersModule, _>(|producers, _| producers.wavetable(index))
    }

    /// `getSample()`.
    pub fn sample(&self) -> SharedSample {
        self.producers
            .as_ref()
            .expect("producers")
            .with::<ProducersModule, _>(|producers, _| producers.sample())
    }

    /// `getLfoSource(index)`.
    pub fn lfo_source(&self, index: usize) -> Rc<RefCell<LineGenerator>> {
        self.lfo_sources[index].clone()
    }

    /// `disableModSource(source)`: every source but `env_1` (the amplitude
    /// envelope).
    pub fn disable_mod_source(&self, this: &Processor, source: &str) {
        if source != "env_1" {
            if let Some(owner) = this
                .get_modulation_source(source)
                .and_then(|output| output.owner())
            {
                owner.enable(false);
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

    /// `correctToTime(seconds)`: realigns the LFOs and random LFOs.
    pub fn correct_to_time(&mut self, seconds: f64) {
        for lfo in &self.lfos {
            lfo.correct_to_time(seconds);
        }
        for random_lfo in &self.random_lfos {
            random_lfo.correct_to_time(seconds);
        }
    }

    /// `disableUnnecessaryModSources()`: disables the LFOs, envelopes 2–6
    /// and random sources.
    pub fn disable_unnecessary_mod_sources(&mut self) {
        for lfo in &self.lfos {
            lfo.enable(false);
        }
        for envelope in self.envelopes.iter().skip(1) {
            envelope.enable(false);
        }
        for random_lfo in &self.random_lfos {
            random_lfo.enable(false);
        }
        if let Some(random) = &self.random {
            random.enable(false);
        }
    }
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
