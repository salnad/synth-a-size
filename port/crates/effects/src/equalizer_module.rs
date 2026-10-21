// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::EqualizerModule`: a three-band EQ of [`DigitalSvf`]s (low
//! shelf or high pass, band shelf or notch, high shelf or low pass).
//!
//! Ports `vital/src/synthesis/modules/equalizer_module.h` and
//! `equalizer_module.cpp`. Like upstream, `setSampleRate` reaches only the
//! six filters, not the module's own controls.

use std::cell::RefCell;
use std::rc::Rc;

use sas_dsp_core::output::Buffer;
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::{constants, StereoMemory, SynthModule, Value};
use sas_filters::synth_filter::{input, style};
use sas_filters::DigitalSvf;

/// `kAudioMemorySamples`.
pub const AUDIO_MEMORY_SAMPLES: i32 = 1 << 15;

/// `vital::EqualizerModule`.
#[derive(Clone)]
pub struct EqualizerModule {
    low_mode: Option<ProcessorRef>,
    band_mode: Option<ProcessorRef>,
    high_mode: Option<ProcessorRef>,

    high_pass: Option<ProcessorRef>,
    low_shelf: Option<ProcessorRef>,
    notch: Option<ProcessorRef>,
    band_shelf: Option<ProcessorRef>,
    low_pass: Option<ProcessorRef>,
    high_shelf: Option<ProcessorRef>,

    audio_memory: Rc<RefCell<StereoMemory>>,
}

fn node(processor: &Option<ProcessorRef>) -> &ProcessorRef {
    processor
        .as_ref()
        .expect("EqualizerModule::init creates its processors")
}

fn basic_svf() -> ProcessorRef {
    let mut svf = DigitalSvf::new();
    svf.set_drive_compensation(false);
    svf.set_basic(true);
    DigitalSvf::create_with(svf)
}

impl EqualizerModule {
    /// `new EqualizerModule()`.
    pub fn create() -> ProcessorRef {
        SynthModule::create_with(
            EqualizerModule {
                low_mode: None,
                band_mode: None,
                high_mode: None,
                high_pass: None,
                low_shelf: None,
                notch: None,
                band_shelf: None,
                low_pass: None,
                high_shelf: None,
                audio_memory: Rc::new(RefCell::new(StereoMemory::new(AUDIO_MEMORY_SAMPLES))),
            },
            0,
            1,
            false,
        )
    }

    /// `getAudioMemory()`: the last output samples, for display.
    pub fn audio_memory(&self) -> Rc<RefCell<StereoMemory>> {
        self.audio_memory.clone()
    }

    fn filters(&self) -> [&ProcessorRef; 6] {
        [
            node(&self.high_pass),
            node(&self.low_shelf),
            node(&self.band_shelf),
            node(&self.notch),
            node(&self.low_pass),
            node(&self.high_shelf),
        ]
    }
}

impl ProcessorImpl for EqualizerModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let pass = Value::create_control_rate(style::K12_DB as f32);
        let notch_style = Value::create_control_rate(style::NOTCH_PASS_SWAP as f32);
        let shelving = Value::create_control_rate(style::SHELVING as f32);
        let value_zero = Value::create_control_rate(0.0);
        let value_one = Value::create_control_rate(1.0);
        let value_two = Value::create_control_rate(2.0);

        let high_pass = basic_svf();
        let low_shelf = DigitalSvf::create();
        let band_shelf = DigitalSvf::create();
        let notch = basic_svf();
        let low_pass = basic_svf();
        let high_shelf = DigitalSvf::create();

        this.add_idle_processor(&high_pass);
        this.add_idle_processor(&low_shelf);
        this.add_idle_processor(&notch);
        this.add_idle_processor(&band_shelf);
        this.add_idle_processor(&low_pass);
        this.add_idle_processor(&high_shelf);
        low_pass.use_output(&this.output(0), 0);
        high_shelf.use_output(&this.output(0), 0);

        self.low_mode = Some(this.create_base_control("eq_low_mode", false, false));
        self.band_mode = Some(this.create_base_control("eq_band_mode", false, false));
        self.high_mode = Some(this.create_base_control("eq_high_mode", false, false));

        let low_cutoff_midi = this.create_mono_mod_control("eq_low_cutoff", true, true, None);
        let band_cutoff_midi = this.create_mono_mod_control("eq_band_cutoff", true, true, None);
        let high_cutoff_midi = this.create_mono_mod_control("eq_high_cutoff", true, true, None);

        let low_resonance = this.create_mono_mod_control("eq_low_resonance", false, false, None);
        let band_resonance = this.create_mono_mod_control("eq_band_resonance", false, false, None);
        let high_resonance = this.create_mono_mod_control("eq_high_resonance", false, false, None);

        let low_decibels = this.create_mono_mod_control("eq_low_gain", false, false, None);
        let band_decibels = this.create_mono_mod_control("eq_band_gain", false, false, None);
        let high_decibels = this.create_mono_mod_control("eq_high_gain", false, false, None);

        high_pass.plug(&pass, input::STYLE);
        high_pass.plug(&value_two, input::PASS_BLEND);
        high_pass.plug(&low_cutoff_midi, input::MIDI_CUTOFF);
        high_pass.plug(&low_resonance, input::RESONANCE);

        low_shelf.plug(&shelving, input::STYLE);
        low_shelf.plug(&value_zero, input::PASS_BLEND);
        low_shelf.plug(&low_cutoff_midi, input::MIDI_CUTOFF);
        low_shelf.plug(&low_resonance, input::RESONANCE);
        low_shelf.plug(&low_decibels, input::GAIN);

        band_shelf.plug(&shelving, input::STYLE);
        band_shelf.plug(&value_one, input::PASS_BLEND);
        band_shelf.plug(&band_cutoff_midi, input::MIDI_CUTOFF);
        band_shelf.plug(&band_resonance, input::RESONANCE);
        band_shelf.plug(&band_decibels, input::GAIN);

        notch.plug(&notch_style, input::STYLE);
        notch.plug(&value_one, input::PASS_BLEND);
        notch.plug(&band_cutoff_midi, input::MIDI_CUTOFF);
        notch.plug(&band_resonance, input::RESONANCE);

        low_pass.plug(&pass, input::STYLE);
        low_pass.plug(&value_zero, input::PASS_BLEND);
        low_pass.plug(&high_cutoff_midi, input::MIDI_CUTOFF);
        low_pass.plug(&high_resonance, input::RESONANCE);

        high_shelf.plug(&shelving, input::STYLE);
        high_shelf.plug(&value_two, input::PASS_BLEND);
        high_shelf.plug(&high_cutoff_midi, input::MIDI_CUTOFF);
        high_shelf.plug(&high_resonance, input::RESONANCE);
        high_shelf.plug(&high_decibels, input::GAIN);

        self.high_pass = Some(high_pass);
        self.low_shelf = Some(low_shelf);
        self.band_shelf = Some(band_shelf);
        self.notch = Some(notch);
        self.low_pass = Some(low_pass);
        self.high_shelf = Some(high_shelf);

        this.router_init();
    }

    fn hard_reset(&mut self, _this: &Processor) {
        for filter in self.filters() {
            filter.reset(constants::full_mask());
        }
    }

    fn enable(&mut self, this: &Processor, enable: bool) {
        this.synth_module_enable(enable);
        this.router_process(1);
        if enable {
            for filter in self.filters() {
                filter.reset(constants::full_mask());
            }
        }
    }

    fn set_sample_rate(&mut self, _this: &Processor, sample_rate: i32) {
        for filter in self.filters() {
            filter.set_sample_rate(sample_rate);
        }
    }

    fn process_with_input(&mut self, this: &Processor, audio_in: &Buffer, num_samples: usize) {
        this.router_process(num_samples);

        let low_processor = if node(&self.low_mode).value() != 0.0 {
            node(&self.high_pass)
        } else {
            node(&self.low_shelf)
        };
        let band_processor = if node(&self.band_mode).value() != 0.0 {
            node(&self.notch)
        } else {
            node(&self.band_shelf)
        };
        let high_processor = if node(&self.high_mode).value() != 0.0 {
            node(&self.low_pass)
        } else {
            node(&self.high_shelf)
        };

        low_processor.process_with_input(audio_in, num_samples);
        band_processor.process_with_input(&low_processor.output(0).buffer(), num_samples);
        high_processor.process_with_input(&band_processor.output(0).buffer(), num_samples);

        let output_buffer = high_processor.output(0).buffer();
        let mut memory = self.audio_memory.borrow_mut();
        for sample in output_buffer.iter().take(num_samples) {
            memory.push(sample.get());
        }
    }
}
