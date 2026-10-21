// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::ProducersModule`: the three oscillators and the sampler, their
//! FM/RM cross-wiring and processing order, and the routing of each to
//! filter 1, filter 2, both, the effects or the direct output.
//!
//! Ports `vital/src/synthesis/modules/producers_module.{h,cpp}`.

use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef, SynthModule};
use sas_simd::conversions::float_to_i32;
use sas_simd::PolyFloat;

use crate::oscillator_module::{self, OscillatorModule};
use crate::sample_module::{self, SampleModule};
use crate::synth_oscillator::{
    is_first_modulation, is_second_modulation, SharedWavetable, SynthOscillator,
};
use crate::SharedSample;

/// `vital::kNumOscillators`.
pub const NUM_OSCILLATORS: usize = 3;

// Input indices.
pub const RESET: usize = 0;
pub const RETRIGGER: usize = 1;
pub const MIDI: usize = 2;
pub const ACTIVE_VOICES: usize = 3;
pub const NOTE_COUNT: usize = 4;
pub const NUM_INPUTS: usize = 5;

// Output indices.
pub const TO_FILTER1: usize = 0;
pub const TO_FILTER2: usize = 1;
pub const RAW_OUT: usize = 2;
pub const DIRECT_OUT: usize = 3;
pub const NUM_OUTPUTS: usize = 4;

/// `constants::SourceDestination`.
pub mod source_destination {
    pub const FILTER1: i32 = 0;
    pub const FILTER2: i32 = 1;
    pub const DUAL_FILTERS: i32 = 2;
    pub const EFFECTS: i32 = 3;
    pub const DIRECT_OUT: i32 = 4;
    pub const NUM_SOURCE_DESTINATIONS: i32 = 5;
}

use source_destination as dest;

/// `getFirstModulationIndex(index)`.
pub fn first_modulation_index(index: usize) -> usize {
    if index == 0 {
        1
    } else {
        0
    }
}

/// `getSecondModulationIndex(index)`.
pub fn second_modulation_index(index: usize) -> usize {
    if index == 1 {
        2
    } else {
        first_modulation_index(index) + 1
    }
}

/// `vital::ProducersModule`'s members.
#[derive(Clone)]
pub struct ProducersModule {
    oscillators: [ProcessorRef; NUM_OSCILLATORS],
    oscillator_destinations: [Option<ProcessorRef>; NUM_OSCILLATORS],
    sample_destination: Option<ProcessorRef>,
    sampler: ProcessorRef,
    filter1_on: Option<ProcessorRef>,
    filter2_on: Option<ProcessorRef>,
}

impl Default for ProducersModule {
    fn default() -> Self {
        Self::new()
    }
}

impl ProducersModule {
    /// The kernel of `ProducersModule()`, with its oscillator and sample
    /// modules (see [`ProducersModule::create`]).
    pub fn new() -> Self {
        Self {
            oscillators: std::array::from_fn(|i| {
                OscillatorModule::create(&format!("osc_{}", i + 1))
            }),
            oscillator_destinations: [None, None, None],
            sample_destination: None,
            sampler: SampleModule::create(),
            filter1_on: None,
            filter2_on: None,
        }
    }

    /// `new ProducersModule()`: adds the oscillator modules and the sample
    /// module as disabled sub-modules (they run in [`ProcessorImpl::process`]).
    pub fn create() -> ProcessorRef {
        let kernel = Self::new();
        let submodules = kernel.submodules();
        let module = SynthModule::create_with(kernel, NUM_INPUTS, NUM_OUTPUTS, false);
        Self::add_submodules(&module, &submodules);
        module
    }

    /// The oscillator modules, then the sample module.
    pub fn submodules(&self) -> Vec<ProcessorRef> {
        let mut submodules = self.oscillators.to_vec();
        submodules.push(self.sampler.clone());
        submodules
    }

    /// Adds [`ProducersModule::submodules`] to the router `module` that runs
    /// this kernel, disabled, as the constructor does.
    pub fn add_submodules(module: &ProcessorRef, submodules: &[ProcessorRef]) {
        for submodule in submodules {
            module.add_submodule(submodule);
            module.add_processor(submodule);
            submodule.enable(false);
        }
    }

    /// `getWavetable(index)`.
    pub fn wavetable(&self, index: usize) -> SharedWavetable {
        self.oscillators[index].with::<OscillatorModule, _>(|module, _| module.wavetable().clone())
    }

    /// `getSample()`.
    pub fn sample(&self) -> SharedSample {
        self.sampler
            .with::<SampleModule, _>(|module, _| module.sample().clone())
    }

    /// `samplePhaseOutput()`.
    pub fn sample_phase_output(&self) -> std::rc::Rc<sas_dsp_core::Output> {
        self.sampler
            .with::<SampleModule, _>(|module, _| module.phase_output().clone())
    }

    /// `setFilter1On(on)`.
    pub fn set_filter1_on(&mut self, on: Option<ProcessorRef>) {
        self.filter1_on = on;
    }

    /// `setFilter2On(on)`.
    pub fn set_filter2_on(&mut self, on: Option<ProcessorRef>) {
        self.filter2_on = on;
    }

    fn is_filter1_on(&self) -> bool {
        self.filter1_on.as_ref().is_none_or(|on| on.value() != 0.0)
    }

    fn is_filter2_on(&self) -> bool {
        self.filter2_on.as_ref().is_none_or(|on| on.value() != 0.0)
    }

    fn oscillator_processor(module: &ProcessorRef) -> ProcessorRef {
        module.with::<OscillatorModule, _>(|module, _| {
            module
                .oscillator()
                .expect("initialized oscillator module")
                .clone()
        })
    }
}

/// Adds `source` to the routing outputs `destination` selects
/// (`ProducersModule::process`).
fn route(
    this: &Processor,
    source: &[std::cell::Cell<PolyFloat>],
    destination: i32,
    filter1_on: bool,
    filter2_on: bool,
    num_samples: usize,
) {
    let raw = destination == dest::EFFECTS;
    let filter1 = destination == dest::FILTER1 || destination == dest::DUAL_FILTERS;
    let filter2 = destination == dest::FILTER2 || destination == dest::DUAL_FILTERS;
    let direct_out = destination == dest::DIRECT_OUT;
    let add = |output: usize| {
        let buffer = this.output_buffer(output);
        for i in 0..num_samples {
            buffer[i].set(buffer[i].get() + source[i].get());
        }
    };
    if raw
        || (!filter2 && filter1 && !filter1_on)
        || (!filter1 && filter2 && !filter2_on)
        || (filter1 && filter2 && !filter1_on && !filter2_on)
    {
        add(RAW_OUT);
    }
    if filter1 {
        add(TO_FILTER1);
    }
    if filter2 {
        add(TO_FILTER2);
    }
    if direct_out {
        add(DIRECT_OUT);
    }
}

impl ProcessorImpl for ProducersModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        for i in 0..NUM_OSCILLATORS {
            let number = (i + 1).to_string();
            self.oscillator_destinations[i] =
                Some(this.create_base_control(&format!("osc_{number}_destination"), false, false));

            let oscillator = &self.oscillators[i];
            oscillator.use_input(&this.input(RESET), oscillator_module::RESET);
            oscillator.use_input(&this.input(RETRIGGER), oscillator_module::RETRIGGER);
            oscillator.use_input(&this.input(MIDI), oscillator_module::MIDI);
            oscillator.use_input(&this.input(ACTIVE_VOICES), oscillator_module::ACTIVE_VOICES);
        }

        self.sample_destination =
            Some(this.create_base_control("sample_destination", false, false));
        self.sampler
            .use_input(&this.input(RESET), sample_module::RESET);
        self.sampler
            .use_input(&this.input(NOTE_COUNT), sample_module::NOTE_COUNT);
        self.sampler
            .use_input(&this.input(MIDI), sample_module::MIDI);

        this.router_init();
        for i in 0..NUM_OSCILLATORS {
            let first = self.oscillators[first_modulation_index(i)].output(oscillator_module::RAW);
            let second =
                self.oscillators[second_modulation_index(i)].output(oscillator_module::RAW);
            let sample = self.sampler.output(sample_module::RAW);
            Self::oscillator_processor(&self.oscillators[i]).with::<SynthOscillator, _>(
                |oscillator, _| {
                    oscillator.set_first_oscillator_output(Some(first));
                    oscillator.set_second_oscillator_output(Some(second));
                    oscillator.set_sample_output(Some(sample));
                },
            );
        }
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        this.router_process(num_samples);

        this.get_local_processor(&self.sampler)
            .expect("sampler is in this router")
            .process(num_samples);

        let mut distortion_types = [0i32; NUM_OSCILLATORS];
        let mut processed = [false; NUM_OSCILLATORS];
        for (kind, oscillator) in distortion_types.iter_mut().zip(&self.oscillators) {
            *kind = oscillator.with::<OscillatorModule, _>(|module, _| module.distortion_type());
        }

        let mut num_processed = 0;
        let mut index = 0;
        let mut i = 0;
        while i < NUM_OSCILLATORS * NUM_OSCILLATORS && num_processed < NUM_OSCILLATORS {
            let first_source = first_modulation_index(index);
            let second_source = second_modulation_index(index);
            if (!is_first_modulation(distortion_types[index]) || processed[first_source])
                && (!is_second_modulation(distortion_types[index]) || processed[second_source])
                && !processed[index]
            {
                num_processed += 1;
                processed[index] = true;
                this.get_local_processor(&self.oscillators[index])
                    .expect("oscillator is in this router")
                    .process(num_samples);
            }
            index = (index + 1) % NUM_OSCILLATORS;
            i += 1;
        }

        for output in [TO_FILTER1, TO_FILTER2, RAW_OUT, DIRECT_OUT] {
            for sample in this.output_buffer(output).iter().take(num_samples) {
                sample.set(PolyFloat::zero());
            }
        }

        let filter1_on = self.is_filter1_on();
        let filter2_on = self.is_filter2_on();

        for i in 0..NUM_OSCILLATORS {
            let buffer = self.oscillators[i].output_buffer(oscillator_module::LEVELLED);
            let destination = float_to_i32(
                self.oscillator_destinations[i]
                    .as_ref()
                    .expect("initialized")
                    .value(),
            );
            route(
                this,
                &buffer,
                destination,
                filter1_on,
                filter2_on,
                num_samples,
            );
        }

        let sample = self.sampler.output_buffer(sample_module::LEVELLED);
        let sample_destination = float_to_i32(
            self.sample_destination
                .as_ref()
                .expect("initialized")
                .value(),
        );
        route(
            this,
            &sample,
            sample_destination,
            filter1_on,
            filter2_on,
            num_samples,
        );
    }
}
