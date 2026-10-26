// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The synth around the [`SoundEngine`]: what `SynthBase` (and the
//! offline `ref-render` harness's `HeadlessSynth`) does on the audio path.
//!
//! Ports the engine-facing parts of `vital/src/common/synth_base.cpp`: the
//! constructor (engine, tuning, default wavetables), `loadFromFile` /
//! `LoadSave::jsonToState` through a [`PresetState`], `processAudio`,
//! `processMidi`, `checkOversampling`, and the render harness's block loop
//! (`migration/reference/src/render_main.cpp`, `RenderSynth::renderBlock`).
//!
//! Modulation routing follows `SynthBase`: connections are resolved into
//! [`ModulationChange`]s and queued, and the queue is applied before the
//! next audio (`processModulationChanges`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use sas_common::LineGenerator;
use sas_dsp_core::constants::MAX_MODULATION_CONNECTIONS;
use sas_dsp_core::memory::StereoMemory;
use sas_dsp_core::sas_simd::{utils, PolyFloat};
use sas_dsp_core::{Output, ProcessorRef, StatusOutput};
use sas_effects::reorderable_effect_chain::ReorderableEffectChain;
use sas_modulation::ModulationConnectionProcessor;
use sas_oscillators::sas_wavetable::creator::WavetableCreator;
use sas_oscillators::sas_wavetable::Wavetable;
use sas_oscillators::Sample;
use sas_preset::{lookup, registry, PresetState, SampleState, SlotValues};

use crate::midi_file::TimedMessage;
use crate::midi_manager::{MidiLearnMap, MidiManager};
use crate::sound_engine::{ModulationChange, SoundEngine};
use crate::synth_voice_handler::SynthVoiceHandler;
use crate::telemetry::{
    destination_names, status_names, AudioMemory, TelemetryWriter, OSCILLOSCOPE_MEMORY_RESOLUTION,
    OSCILLOSCOPE_MEMORY_SAMPLE_RATE, OSCILLOSCOPE_SAMPLES,
};
use crate::tuning::Tuning;

/// `kMaxBufferSize`: the longest engine sub-block.
pub const MAX_BUFFER_SIZE: usize = 128;
/// `kNumOscillators`.
pub const NUM_OSCILLATORS: usize = 3;
/// `kNumLfos`.
pub const NUM_LFOS: usize = 8;
/// Modulation changes [`Synth`] can queue between two blocks without
/// growing its queue: a full bank's worth plus a full edit queue.
const MODULATION_CHANGE_CAPACITY: usize =
    MAX_MODULATION_CONNECTIONS + crate::edit_bus::EDIT_QUEUE_CAPACITY;

/// `kOutputWindowMinNote` / `kOutputWindowMaxNote`.
const OUTPUT_WINDOW_MIN_NOTE: f32 = 16.0;
const OUTPUT_WINDOW_MAX_NOTE: f32 = 128.0;

/// `SynthBase`'s oscilloscope state (`updateMemoryOutput`).
struct OscilloscopeMemory {
    last_played_note: f32,
    last_num_pressed: i32,
    reset_period: f32,
    input_offset: i32,
    index: i32,
    /// `oscilloscope_memory_`: the last complete window.
    memory: Box<[PolyFloat]>,
    /// `oscilloscope_memory_write_`: the window being filled.
    write: Box<[PolyFloat]>,
}

impl Default for OscilloscopeMemory {
    fn default() -> Self {
        OscilloscopeMemory {
            last_played_note: 0.0,
            last_num_pressed: 0,
            reset_period: OSCILLOSCOPE_MEMORY_RESOLUTION as f32,
            input_offset: 0,
            index: 0,
            memory: vec![PolyFloat::zero(); OSCILLOSCOPE_SAMPLES].into_boxed_slice(),
            write: vec![PolyFloat::zero(); OSCILLOSCOPE_SAMPLES].into_boxed_slice(),
        }
    }
}

/// The synth: a [`SoundEngine`] with its MIDI manager, tuning and wavetable
/// creators.
pub struct Synth {
    engine: ProcessorRef,
    /// `SynthBase::controls_`: every control of the engine, by name.
    controls: BTreeMap<String, ProcessorRef>,
    /// Each modulation slot's `power`, `amount`, `bipolar`, `stereo` and
    /// `bypass` controls, so an edit sets them without building names.
    slot_controls: Vec<[ProcessorRef; 5]>,
    midi_manager: MidiManager,
    wavetable_creators: Vec<WavetableCreator>,
    /// `mod_connections_`: the bank indices of the connected slots, in
    /// connection order.
    mod_connections: Vec<usize>,
    /// `modulation_change_queue_`.
    modulation_change_queue: Vec<ModulationChange>,
    /// Every status output, in [`status_names`] order.
    status_outputs: Vec<Rc<StatusOutput>>,
    /// Each destination's mono and poly modulation totals, in
    /// [`destination_names`] order.
    modulation_totals: Vec<(Rc<Output>, Option<Rc<Output>>)>,
    oscilloscope: OscilloscopeMemory,
    /// `audio_memory_`.
    audio_memory: Arc<AudioMemory>,
    /// `getEqualizerMemory()`: the equalizer module's output memory, which
    /// the audio thread copies into `equalizer_memory` after each block.
    equalizer_source: Option<Rc<RefCell<StereoMemory>>>,
    /// The source's write offset at the last copy.
    equalizer_offset: u32,
    equalizer_memory: Arc<AudioMemory>,
    /// Controls a learned MIDI controller set, in order, for the host
    /// (`SynthBase::valueChangedThroughMidi`'s `setValueNotifyHost`). Callers
    /// drain it after every message, and one message sets each parameter at
    /// most once, so the capacity reserved for every parameter is never
    /// outgrown on the audio thread.
    midi_value_changes: Vec<(&'static str, f32)>,
}

impl Default for Synth {
    fn default() -> Self {
        Self::new()
    }
}

impl Synth {
    /// `SynthBase()`: the engine with the default tuning, and each
    /// oscillator's default wavetable rendered (`WavetableCreator::init`).
    pub fn new() -> Self {
        let engine = SoundEngine::create();
        let controls = engine.get_controls();
        let slot_controls = (1..=MAX_MODULATION_CONNECTIONS)
            .map(|number| {
                ["power", "amount", "bipolar", "stereo", "bypass"]
                    .map(|suffix| controls[&format!("modulation_{number}_{suffix}")].clone())
            })
            .collect();
        let mut synth = Synth {
            engine,
            controls,
            slot_controls,
            midi_manager: MidiManager::new(),
            wavetable_creators: Vec::new(),
            mod_connections: Vec::with_capacity(MAX_MODULATION_CONNECTIONS),
            modulation_change_queue: Vec::with_capacity(MODULATION_CHANGE_CAPACITY),
            status_outputs: Vec::new(),
            modulation_totals: Vec::new(),
            oscilloscope: OscilloscopeMemory::default(),
            audio_memory: Arc::new(AudioMemory::default()),
            equalizer_source: None,
            equalizer_offset: 0,
            equalizer_memory: Arc::new(AudioMemory::default()),
            midi_value_changes: Vec::with_capacity(registry().parameters().len()),
        };
        synth.status_outputs = status_names()
            .iter()
            .map(|name| {
                synth
                    .engine
                    .get_status_output(name)
                    .unwrap_or_else(|| panic!("the engine registers status output {name}"))
            })
            .collect();
        let mono = synth.engine.get_mono_modulations();
        let poly = synth.engine.get_poly_modulations();
        synth.modulation_totals = destination_names()
            .iter()
            .map(|name| {
                let total = mono
                    .get(*name)
                    .unwrap_or_else(|| panic!("destination {name} has a mono modulation total"));
                (total.clone(), poly.get(*name).cloned())
            })
            .collect();
        synth.set_tuning(&Tuning::new());
        for i in 0..NUM_OSCILLATORS {
            let mut creator = WavetableCreator::new();
            creator.init();
            let wavetable = synth.with_svh(|svh| svh.wavetable(i));
            creator
                .render(&mut wavetable.borrow_mut())
                .expect("the default wavetable renders");
            synth.wavetable_creators.push(creator);
        }
        // getEqualizerMemory(): taken before the first block, so the editor's
        // copy starts with it.
        synth.equalizer_source = synth.with_engine(|engine| {
            engine
                .effect_chain()
                .with::<ReorderableEffectChain, _>(|chain, _| chain.get_equalizer_memory())
        });
        if let Some(source) = &synth.equalizer_source {
            synth.equalizer_offset = source.borrow().offset();
        }
        synth
    }

    /// The engine node (`SynthBase::getEngine`).
    pub fn engine(&self) -> &ProcessorRef {
        &self.engine
    }

    fn with_engine<R>(&self, f: impl FnOnce(&mut SoundEngine) -> R) -> R {
        self.engine.with::<SoundEngine, _>(|engine, _| f(engine))
    }

    fn with_svh<R>(&self, f: impl FnOnce(&mut SynthVoiceHandler) -> R) -> R {
        let voice_handler = self.with_engine(|engine| engine.voice_handler().clone());
        voice_handler.with::<SynthVoiceHandler, _>(|svh, _| f(svh))
    }

    /// `setTuning`: the engine converts notes through a copy of `tuning`.
    pub fn set_tuning(&mut self, tuning: &Tuning) {
        let tuning = Rc::new(tuning.clone());
        self.with_engine(|engine| engine.set_tuning(Some(tuning)));
    }

    /// [`set_tuning`](Self::set_tuning) without allocating: `tuning` takes
    /// the engine's tuning's place and comes back holding the old one.
    pub fn swap_tuning(&mut self, tuning: &mut Tuning) {
        self.with_engine(|engine| engine.swap_tuning(tuning));
    }

    /// `setMpeEnabled(enabled)`.
    pub fn set_mpe_enabled(&mut self, enabled: bool) {
        self.midi_manager.set_mpe_enabled(enabled);
    }

    /// `SoundEngine::setSampleRate` and `MidiManager::setSampleRate`.
    pub fn set_sample_rate(&mut self, sample_rate: i32) {
        self.engine.set_sample_rate(sample_rate);
    }

    /// `SoundEngine::setBpm`.
    pub fn set_bpm(&mut self, bpm: f32) {
        self.with_engine(|engine| engine.set_bpm(bpm));
    }

    /// `SoundEngine::correctToTime`. Queued modulation changes are applied
    /// first: the plugin calls this at the start of each engine sub-block,
    /// after upstream's per-block `processModulationChanges`.
    pub fn correct_to_time(&mut self, seconds: f64) {
        self.process_modulation_changes();
        self.with_engine(|engine| engine.correct_to_time(seconds));
    }

    /// `updateAllModulationSwitches()` on the engine.
    pub fn update_all_modulation_switches(&mut self) {
        self.engine.update_all_modulation_switches();
    }

    /// `checkOversampling()`.
    pub fn check_oversampling(&mut self) {
        self.engine
            .with::<SoundEngine, _>(|engine, this| engine.check_oversampling(this));
    }

    /// `SoundEngine::noteOn`, as `SynthBase::renderAudioForResynthesis`
    /// plays its note: a velocity from 0 to 1, not a MIDI byte.
    pub fn note_on(&mut self, note: i32, velocity: f32, sample: i32, channel: i32) {
        self.with_engine(|engine| engine.note_on(note, velocity, sample, channel));
    }

    /// `SoundEngine::allSoundsOff`.
    pub fn all_sounds_off(&mut self) {
        self.with_engine(|engine| engine.all_sounds_off());
    }

    /// Sets control `name` (`SynthBase::valueChangedExternal`): the mod and
    /// pitch wheels also reach every channel's voices.
    pub fn set_control(&mut self, name: &str, value: f32) {
        let Some(control) = self.controls.get(name) else {
            return;
        };
        control.set_value(value);
        match name {
            "mod_wheel" => self.with_engine(|engine| engine.set_mod_wheel_all_channels(value)),
            "pitch_wheel" => self.with_engine(|engine| engine.set_zoned_pitch_wheel(value, 0, 15)),
            _ => {}
        }
    }

    /// `loadFromFile` / `LoadSave::jsonToState` for a parsed preset: every
    /// control (its default if the preset has none), the sample, the
    /// wavetables and the LFO shapes, then `checkOversampling`.
    pub fn load_preset(&mut self, preset: &PresetState) {
        self.all_sounds_off();

        let controls = &self.controls;
        for (name, control) in controls {
            let value = preset
                .value(name)
                .or_else(|| lookup(name).map(|details| details.default_value))
                .unwrap_or(0.0);
            control.set_value(value);
        }
        let mod_wheel = controls["mod_wheel"].value();
        self.with_engine(|engine| engine.set_mod_wheel_all_channels(mod_wheel));

        self.load_modulations(preset);

        let sample = self.with_svh(|svh| svh.sample());
        {
            let state = &preset.sample;
            let mut sample = sample.borrow_mut();
            match &state.right {
                Some(right) => sample.load_stereo_sample(
                    &state.left,
                    right,
                    state.left.len(),
                    state.sample_rate,
                ),
                None => sample.load_sample(&state.left, state.left.len(), state.sample_rate),
            }
            sample.set_name(&state.name);
        }

        for (i, creator) in preset.wavetables.iter().enumerate().take(NUM_OSCILLATORS) {
            self.wavetable_creators[i] = creator.clone();
            let wavetable = self.with_svh(|svh| svh.wavetable(i));
            let _ = self.wavetable_creators[i].render(&mut wavetable.borrow_mut());
        }

        for (i, lfo) in preset.lfos.iter().enumerate().take(NUM_LFOS) {
            let source = self.with_svh(|svh| svh.lfo_source(i));
            let mut source = source.borrow_mut();
            if source.from_json(&lfo.to_json()).is_ok() {
                source.render();
            }
        }

        self.check_oversampling();
    }

    /// The sample the engine plays, as a [`SampleState`]: the white noise
    /// the engine built (and took a seed for) until a preset loads a sample,
    /// then that sample. Saving an untouched init state needs this, since
    /// the engine's noise cannot be regenerated without its seed.
    pub fn sample_state(&self) -> SampleState {
        let sample = self.with_svh(|svh| svh.sample());
        let sample = sample.borrow();
        let data = sample.data();
        let original = |buffers: &[Vec<f32>]| -> Vec<f32> {
            let start = Sample::BUFFER_SAMPLES;
            buffers[Sample::UPSAMPLE_TIMES][start..start + data.length].to_vec()
        };
        SampleState {
            name: sample.name().to_string(),
            sample_rate: data.sample_rate,
            left: original(&data.left_buffers),
            right: data.stereo.then(|| original(&data.right_buffers)),
        }
    }

    fn bank_processor(&self, index: usize) -> ProcessorRef {
        self.with_svh(|svh| svh.modulation_bank().at_index(index).processor.clone())
    }

    fn bank_names(&self, index: usize) -> (String, String) {
        self.with_svh(|svh| {
            let connection = svh.modulation_bank().at_index(index);
            (
                connection.source_name.clone(),
                connection.destination_name.clone(),
            )
        })
    }

    /// Whether bank slot `index` connects `source` to `destination`.
    fn bank_slot_connects(&self, index: usize, source: &str, destination: &str) -> bool {
        self.with_svh(|svh| {
            svh.modulation_bank()
                .at_index(index)
                .connects(source, destination)
        })
    }

    fn set_bank_names(&self, index: usize, source: &str, destination: &str) {
        self.with_svh(|svh| {
            svh.modulation_bank()
                .at_index_mut(index)
                .reset_connection(source, destination)
        });
    }

    /// `createModulationChange(connection)` for bank slot `index`, which
    /// connects `source_name` to `destination_name`.
    fn create_modulation_change(
        &self,
        index: usize,
        source_name: &str,
        destination_name: &str,
    ) -> ModulationChange {
        let engine = &self.engine;
        let num_audio_rate = self.with_svh(|svh| {
            let bank = svh.modulation_bank();
            (0..bank.num_connections())
                .filter(|&i| {
                    let other = bank.at_index(i);
                    other.source_name == source_name
                        && other.destination_name != destination_name
                        && !other.processor.is_control_rate()
                })
                .count()
        });
        ModulationChange {
            source: engine
                .get_modulation_source(source_name)
                .expect("a connected modulation source exists"),
            mono_destination: engine
                .get_mono_modulation_destination(destination_name)
                .expect("a connected modulation destination exists"),
            poly_destination: engine.get_poly_modulation_destination(destination_name),
            destination_scale: lookup(destination_name).map_or(0.0, |details| details.range()),
            mono_modulation_switch: engine
                .get_mono_modulation_switch(destination_name)
                .expect("a modulation destination has a switch"),
            poly_modulation_switch: engine.get_poly_modulation_switch(destination_name),
            modulation_processor: self.bank_processor(index),
            disconnecting: false,
            num_audio_rate,
        }
    }

    /// `connectModulation(connection)` for bank slot `index`, whose names
    /// are already `source` and `destination`: queues the connection, or
    /// clears the slot if it would modulate its own amount
    /// (`isInvalidConnection`).
    fn connect_slot(&mut self, index: usize, source: &str, destination: &str) {
        let change = self.create_modulation_change(index, source, destination);
        let invalid = change
            .poly_destination
            .as_ref()
            .is_some_and(|poly_destination| {
                poly_destination.router().is_some_and(|router| {
                    std::rc::Rc::ptr_eq(&router, &change.modulation_processor)
                })
            });
        if invalid {
            self.set_bank_names(index, "", "");
        } else if !self.mod_connections.contains(&index) {
            self.mod_connections.push(index);
            self.modulation_change_queue.push(change);
        }
    }

    /// `connectModulation(source, destination)`: reuses a connection
    /// between the two or takes a new bank slot. Returns whether a slot was
    /// created.
    pub fn connect_modulation(&mut self, source: &str, destination: &str) -> bool {
        let existing = self
            .mod_connections
            .iter()
            .copied()
            .find(|&i| self.bank_slot_connects(i, source, destination));
        let create = existing.is_none();
        let index = match existing {
            Some(index) => Some(index),
            None => {
                self.with_svh(|svh| svh.modulation_bank().create_connection(source, destination))
            }
        };
        if let Some(index) = index {
            self.connect_slot(index, source, destination);
        }
        create
    }

    /// The engine side of a source dropped on a control
    /// (`ModulationManager::modulationDraggedToComponent`, see
    /// `PresetState::connect_modulation_from_gui`): if this creates the
    /// connection, its slot's power goes to 0 (`initModulationValues`) and
    /// its amount, bipolar, stereo and bypass to `values`
    /// (`setModulationValues`), right after `createConnection` set the
    /// source's default polarity, as upstream writes them before the next
    /// block. The remap curve's reset to linear arrives as its own edit. A
    /// plugin applies these edits before the block's host parameter changes,
    /// so a value the host changed in the same block still wins. Nothing is
    /// allocated.
    pub fn gui_connect_modulation(&mut self, source: &str, destination: &str, values: &SlotValues) {
        if !self.connect_modulation(source, destination) {
            return;
        }
        let Some(index) = self
            .mod_connections
            .iter()
            .copied()
            .find(|&i| self.bank_slot_connects(i, source, destination))
        else {
            return;
        };
        let flag = |on: bool| if on { 1.0 } else { 0.0 };
        let [power, amount, bipolar, stereo, bypass] = &self.slot_controls[index];
        power.set_value(0.0);
        amount.set_value(values.amount);
        bipolar.set_value(flag(values.bipolar));
        stereo.set_value(flag(values.stereo));
        bypass.set_value(flag(values.bypass));
    }

    /// The amount, bipolar, stereo and bypass controls of bank slot
    /// `index` (`SynthGuiInterface::setModulationValues`'s `valueChanged`
    /// calls); the connection keeps running.
    pub fn set_connection_values(&mut self, index: usize, values: &SlotValues) {
        let Some([_, amount, bipolar, stereo, bypass]) = self.slot_controls.get(index) else {
            return;
        };
        let flag = |on: bool| if on { 1.0 } else { 0.0 };
        amount.set_value(values.amount);
        bipolar.set_value(flag(values.bipolar));
        stereo.set_value(flag(values.stereo));
        bypass.set_value(flag(values.bypass));
    }

    /// `disconnectModulation(source, destination)`.
    pub fn disconnect_modulation(&mut self, source: &str, destination: &str) {
        let Some(position) = self
            .mod_connections
            .iter()
            .position(|&i| self.bank_slot_connects(i, source, destination))
        else {
            return;
        };
        let index = self.mod_connections[position];
        let mut change = self.create_modulation_change(index, source, destination);
        self.set_bank_names(index, "", "");
        self.mod_connections.remove(position);
        change.disconnecting = true;
        self.modulation_change_queue.push(change);
    }

    /// `clearModulations()`: disconnects everything now, resets every
    /// slot's line mapping to linear and disables the unused sources.
    pub fn clear_modulations(&mut self) {
        self.modulation_change_queue.clear();
        while let Some(index) = self.mod_connections.first().copied() {
            self.mod_connections.remove(0);
            let (source, destination) = self.bank_names(index);
            let mut change = self.create_modulation_change(index, &source, &destination);
            change.disconnecting = true;
            self.with_engine(|engine| engine.disconnect_modulation(&change));
            self.set_bank_names(index, "", "");
        }

        let num_connections = self.with_svh(|svh| svh.modulation_bank().num_connections());
        for i in 0..num_connections {
            let generator = self
                .bank_processor(i)
                .with::<ModulationConnectionProcessor, _>(|kernel, _| kernel.line_map_generator());
            generator.borrow_mut().init_linear();
        }
        self.with_engine(|engine| engine.disable_unnecessary_mod_sources());
    }

    /// `LoadSave::loadModulations`: slot `i` of the preset goes to bank slot
    /// `i`; a slot whose source or destination the engine lacks is skipped.
    fn load_modulations(&mut self, preset: &PresetState) {
        self.clear_modulations();
        for (index, slot) in preset.modulations().iter().enumerate() {
            let known = self.engine.get_modulation_source(&slot.source).is_some()
                && self
                    .engine
                    .get_mono_modulation_destination(&slot.destination)
                    .is_some();
            if !known {
                continue;
            }
            if !slot.source.is_empty() && !slot.destination.is_empty() {
                self.set_bank_names(index, &slot.source, &slot.destination);
                self.connect_slot(index, &slot.source, &slot.destination);
            }
            let generator = self
                .bank_processor(index)
                .with::<ModulationConnectionProcessor, _>(|kernel, _| kernel.line_map_generator());
            let _ = generator
                .borrow_mut()
                .from_json(&slot.line_mapping.to_json());
        }
    }

    /// `processModulationChanges()`: applies the queued connections and
    /// disconnections, in order.
    pub fn process_modulation_changes(&mut self) {
        let mut changes = std::mem::take(&mut self.modulation_change_queue);
        for change in changes.drain(..) {
            let change = &change;
            self.with_engine(|engine| {
                if change.disconnecting {
                    engine.disconnect_modulation(change);
                } else {
                    engine.connect_modulation(change);
                }
            });
        }
        self.modulation_change_queue = changes;
    }

    /// Sets the remap curve of modulation slot `slot` (0-based) to `line`'s
    /// shape, as upstream's curve editor edits the slot's line map in place.
    /// `line` gets the slot's old name; nothing is allocated or freed.
    pub fn set_line_mapping(&mut self, slot: usize, line: &mut LineGenerator) {
        let generator = self
            .bank_processor(slot)
            .with::<ModulationConnectionProcessor, _>(|kernel, _| kernel.line_map_generator());
        generator.borrow_mut().take_shape(line);
    }

    /// Sets LFO `lfo`'s shape (`getLfoSource(lfo)` edited in place, as the
    /// LFO editor does). `line` gets the old name; nothing is allocated or
    /// freed.
    pub fn set_lfo_shape(&mut self, lfo: usize, line: &mut LineGenerator) {
        let source = self.with_svh(|svh| svh.lfo_source(lfo));
        source.borrow_mut().take_shape(line);
    }

    /// Swaps in oscillator `oscillator`'s newly rendered wavetable, as if its
    /// creator had rendered into the engine's table
    /// ([`Wavetable::take_rendered`]). `rendered` gets the old table.
    pub fn swap_wavetable(&mut self, oscillator: usize, rendered: &mut Wavetable) {
        let wavetable = self.with_svh(|svh| svh.wavetable(oscillator));
        wavetable.borrow_mut().take_rendered(rendered);
    }

    /// Swaps in a loaded sample ([`Sample::swap_loaded`]). `loaded` gets the
    /// old audio and name.
    pub fn swap_sample(&mut self, loaded: &mut Sample) {
        let sample = self.with_svh(|svh| svh.sample());
        sample.borrow_mut().swap_loaded(loaded);
    }

    /// `MidiManager::processMidiMessage` for one short message at
    /// `sample_offset` into the next [`process`](Self::process). Controls a
    /// learned controller sets change before the next message, as
    /// upstream's listener sets them inside `processMidiMessage`.
    pub fn process_midi(&mut self, message: [u8; 3], sample_offset: i32) {
        let manager = &mut self.midi_manager;
        let changes = &mut self.midi_value_changes;
        let first = changes.len();
        self.engine.with::<SoundEngine, _>(|engine, _| {
            manager.process_midi_message(engine, message, sample_offset, &mut |name, value| {
                changes.push((name, value))
            })
        });
        for index in first..self.midi_value_changes.len() {
            let (name, value) = self.midi_value_changes[index];
            self.value_changed_through_midi(name, value);
        }
    }

    /// `valueChangedThroughMidi(name, value)`: sets the control alone. Unlike
    /// [`set_control`](Self::set_control) (`valueChangedExternal`), the mod
    /// and pitch wheel controls do not reach the voices: upstream's
    /// `ValueBridge` keeps the host notification from coming back as an
    /// external change.
    fn value_changed_through_midi(&mut self, name: &str, value: f32) {
        if let Some(control) = self.controls.get(name) {
            control.set_value(value);
        }
    }

    /// The controls learned MIDI controllers set since the last call, in
    /// order, for the host to be told (`setValueNotifyHost`).
    pub fn drain_midi_value_changes(&mut self, mut f: impl FnMut(&'static str, f32)) {
        for (name, value) in self.midi_value_changes.drain(..) {
            f(name, value);
        }
    }

    /// `armMidiLearn(name)`.
    pub fn arm_midi_learn(&mut self, name: &str) {
        self.midi_manager.arm_midi_learn(name);
    }

    /// `cancelMidiLearn()`.
    pub fn cancel_midi_learn(&mut self) {
        self.midi_manager.cancel_midi_learn();
    }

    /// `clearMidiLearn(name)`; returns whether anything was cleared.
    pub fn clear_midi_learn(&mut self, name: &str) -> bool {
        self.midi_manager.clear_midi_learn(name)
    }

    /// `isMidiMapped(name)`.
    pub fn is_midi_mapped(&self, name: &str) -> bool {
        self.midi_manager.is_midi_mapped(name)
    }

    /// The MIDI learn map (`MidiManager::getMidiLearnMap`).
    pub fn midi_learn_map(&self) -> &MidiLearnMap {
        self.midi_manager.midi_learn_map()
    }

    /// `MidiManager::setMidiLearnMap`, as `LoadSave::loadConfig` restores it.
    pub fn set_midi_learn_map(&mut self, midi_learn_map: MidiLearnMap) {
        self.midi_manager.set_midi_learn_map(midi_learn_map);
    }

    /// Whether a MIDI controller learned a parameter since the last call
    /// (upstream saves the config there).
    pub fn take_midi_learn_changed(&mut self) -> bool {
        self.midi_manager.take_midi_learn_changed()
    }

    /// `processAudio`: runs the engine for `left.len()` samples (at most
    /// [`MAX_BUFFER_SIZE`]) and writes its first two lanes.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let num_samples = left.len();
        debug_assert!(num_samples <= MAX_BUFFER_SIZE && right.len() == num_samples);
        self.process_modulation_changes();
        self.engine.process(num_samples);
        let output = self.engine.output(0);
        for i in 0..num_samples {
            let frame = output.at(i);
            left[i] = frame.get(0);
            right[i] = frame.get(1);
        }
        self.update_memory_output(num_samples);
    }

    /// `updateMemoryOutput(samples, audio)` on the engine's output: the
    /// audio memory, and the oscilloscope window, which restarts on a new
    /// note and spans whole periods of the last note played. The editor
    /// reads both; the audio is unaffected.
    fn update_memory_output(&mut self, samples: usize) {
        let output = self.engine.output(0);
        for i in 0..samples {
            let frame = output.at(i);
            self.audio_memory.push(frame.get(0), frame.get(1));
        }
        self.copy_equalizer_memory();

        let (last_played, num_pressed) = self
            .with_engine(|engine| (engine.last_active_note(), engine.num_pressed_notes() as i32));
        let last_played = utils::clamp(last_played, OUTPUT_WINDOW_MIN_NOTE, OUTPUT_WINDOW_MAX_NOTE);
        let sample_rate = self.engine.sample_rate();
        let output_inc = (sample_rate / OSCILLOSCOPE_MEMORY_SAMPLE_RATE).max(1);
        let oscilloscope_samples = OSCILLOSCOPE_SAMPLES as i32;
        let memory = &mut self.oscilloscope;

        if last_played != 0.0
            && (memory.last_played_note != last_played || num_pressed > memory.last_num_pressed)
        {
            memory.last_played_note = last_played;
            let frequency = utils::midi_note_to_frequency(memory.last_played_note);
            let period = sample_rate as f32 / frequency;
            let window_length = output_inc * OSCILLOSCOPE_MEMORY_RESOLUTION as i32;
            memory.reset_period = period;
            while memory.reset_period < window_length as f32 {
                memory.reset_period += memory.reset_period;
            }
            memory.reset_period = memory.reset_period.min(2.0 * window_length as f32);
            memory.index = 0;
            memory.memory.copy_from_slice(&memory.write);
        }
        memory.last_num_pressed = num_pressed;

        while memory.input_offset < samples as i32 {
            let input_index = memory.input_offset.clamp(0, samples as i32);
            memory.index = memory.index.clamp(0, oscilloscope_samples - 1);
            memory.write[memory.index as usize] = output.at(input_index as usize);
            memory.index += 1;
            if (memory.index * output_inc) as f32 >= memory.reset_period {
                // `int += float`: the sum in float, truncated.
                memory.input_offset = (memory.input_offset as f32
                    + (memory.reset_period - (memory.index * output_inc) as f32))
                    as i32;
                memory.index = 0;
                memory.memory.copy_from_slice(&memory.write);
            }
            memory.input_offset += output_inc;
        }
        memory.input_offset -= samples as i32;
    }

    /// Copies what the equalizer wrote this block into the editor's copy of
    /// its memory. A bypassed equalizer writes nothing, so its memory holds
    /// its last output, as upstream's does.
    fn copy_equalizer_memory(&mut self) {
        let Some(source) = &self.equalizer_source else {
            return;
        };
        let memory = source.borrow();
        let mask = memory.size() as u32 - 1;
        let offset = memory.offset();
        let fresh = offset.wrapping_sub(self.equalizer_offset) & mask;
        let (left, right) = (memory.buffer(0), memory.buffer(1));
        for k in 1..=fresh {
            let index = (self.equalizer_offset.wrapping_add(k) & mask) as usize;
            self.equalizer_memory.push(left[index], right[index]);
        }
        self.equalizer_offset = offset;
    }

    /// The equalizer's output memory (`SynthBase::getEqualizerMemory`),
    /// shared with the editor.
    pub fn equalizer_memory(&self) -> Arc<AudioMemory> {
        self.equalizer_memory.clone()
    }

    /// The spectrogram's audio memory (`SynthBase::audio_memory_`), shared
    /// with the editor.
    pub fn audio_memory(&self) -> Arc<AudioMemory> {
        self.audio_memory.clone()
    }

    /// Fills the writer's snapshot with every status output, each
    /// destination's modulation totals and the oscilloscope memory, and
    /// publishes it. Nothing is allocated.
    pub fn write_telemetry(&self, writer: &mut TelemetryWriter) {
        let (status, totals, oscilloscope) = writer.snapshot_mut().parts_mut();
        for (value, output) in status.iter_mut().zip(&self.status_outputs) {
            *value = output.value().to_array();
        }
        for (total, (mono, poly)) in totals.iter_mut().zip(&self.modulation_totals) {
            total.mono = mono.trigger_value().to_array();
            total.poly = poly.as_ref().map(|poly| poly.trigger_value().to_array());
            total.enabled = mono.owner().is_some_and(|owner| owner.enabled());
        }
        for (frame, value) in oscilloscope.iter_mut().zip(self.oscilloscope.memory.iter()) {
            *frame = [value.get(0), value.get(1)];
        }
        let snapshot = writer.snapshot_mut();
        snapshot.sample_rate = self.engine.sample_rate();
        snapshot.oversampling_amount = self.with_engine(|engine| engine.oversampling_amount());
        writer.publish();
    }

    /// `RenderSynth::prepare`: sample rate, bpm, modulation switches.
    pub fn prepare(&mut self, sample_rate: i32, bpm: f64) {
        self.set_sample_rate(sample_rate);
        self.set_bpm(bpm as f32);
        self.update_all_modulation_switches();
    }

    /// `RenderSynth::renderBlock`: one host block, in engine sub-blocks of at
    /// most [`MAX_BUFFER_SIZE`], each preceded by `correctToTime` and the
    /// block's MIDI that falls inside it. The whole block runs with denormals
    /// flushed to zero, as upstream's sticky `disableDenormalisedNumberSupport`
    /// and the plugin's `ScopedNoDenormals` leave it. `midi` holds `(offset, message)`
    /// pairs relative to the block start, in order.
    pub fn render_block(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        midi: &[(usize, [u8; 3])],
        seconds_time: &mut f64,
        sample_rate: i32,
    ) {
        let _no_denormals = sas_dsp_core::ScopedNoDenormals::new();
        self.process_modulation_changes();
        let total_samples = left.len();
        let sample_time = 1.0 / sample_rate as f64;
        let mut sample_offset = 0;
        while sample_offset < total_samples {
            let num_samples = (total_samples - sample_offset).min(MAX_BUFFER_SIZE);
            self.correct_to_time(*seconds_time);
            for &(position, message) in midi {
                if position >= sample_offset && position < sample_offset + num_samples {
                    self.process_midi(message, (position - sample_offset) as i32);
                    // An offline render has no host to tell.
                    self.midi_value_changes.clear();
                }
            }
            let end = sample_offset + num_samples;
            self.process(
                &mut left[sample_offset..end],
                &mut right[sample_offset..end],
            );
            *seconds_time += num_samples as f64 * sample_time;
            sample_offset = end;
        }
    }
}

/// Settings for [`render`], as `ref-render` takes them.
#[derive(Clone, Debug)]
pub struct RenderSettings {
    pub sample_rate: i32,
    pub block_size: usize,
    pub bpm: f64,
    /// Frames to render.
    pub frames: usize,
}

/// `ref-render`'s main loop: a loaded synth, MIDI events at
/// `floor(seconds * rate + 0.5)`, rendered block by block from t = 0.
/// Returns interleaved stereo.
pub fn render(synth: &mut Synth, events: &[TimedMessage], settings: &RenderSettings) -> Vec<f32> {
    let sample_rate = settings.sample_rate;
    synth.prepare(sample_rate, settings.bpm);

    let timed: Vec<(i64, [u8; 3])> = events
        .iter()
        .map(|event| {
            let sample = (event.seconds * sample_rate as f64 + 0.5).floor() as i64;
            (sample, event.message)
        })
        .collect();

    let mut seconds_time = 0.0;
    let mut interleaved = vec![0.0f32; settings.frames * 2];
    let mut left = vec![0.0f32; settings.block_size];
    let mut right = vec![0.0f32; settings.block_size];
    let mut event_index = 0;
    let mut block_start = 0usize;
    while block_start < settings.frames {
        let n = settings.block_size.min(settings.frames - block_start);
        left[..n].fill(0.0);
        right[..n].fill(0.0);

        let mut midi = Vec::new();
        while event_index < timed.len() && timed[event_index].0 < (block_start + n) as i64 {
            let offset = (timed[event_index].0 - block_start as i64).max(0) as usize;
            midi.push((offset, timed[event_index].1));
            event_index += 1;
        }

        synth.render_block(
            &mut left[..n],
            &mut right[..n],
            &midi,
            &mut seconds_time,
            sample_rate,
        );
        for i in 0..n {
            interleaved[2 * (block_start + i)] = left[i];
            interleaved[2 * (block_start + i) + 1] = right[i];
        }
        block_start += n;
    }
    interleaved
}

#[cfg(test)]
mod tests {
    use super::*;
    use sas_preset::InitContent;

    #[test]
    fn sample_state_is_the_engine_noise_until_a_preset_loads_a_sample() {
        let mut synth = Synth::new();
        let noise = synth.sample_state();
        assert_eq!(noise.name, "White Noise");
        assert_eq!(noise.sample_rate, 44100);
        assert_eq!(noise.left.len(), 44100);
        assert!(noise.right.is_none());
        assert!(noise.left.iter().all(|v| (-0.9..0.9).contains(v)));

        let mut preset = PresetState::new(&InitContent { sample_seed: 1234 });
        preset.sample = SampleState {
            name: "authored".to_string(),
            sample_rate: 48000,
            left: (0..64).map(|i| i as f32 / 64.0).collect(),
            right: Some((0..64).map(|i| -(i as f32) / 64.0).collect()),
        };
        synth.load_preset(&preset);
        assert_eq!(synth.sample_state(), preset.sample);
    }
}
