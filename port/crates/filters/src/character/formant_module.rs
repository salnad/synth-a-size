// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::FormantModule`: the filter section's formant model, a
//! `SynthModule` that switches between two `FormantFilter` styles and the
//! `VocalTract`, with its own formant X/Y, transpose, resonance and spread
//! controls (`<prefix>_formant_*` parameters).
//!
//! Ports `vital/src/synthesis/modules/formant_module.h` and
//! `formant_module.cpp`.

use std::rc::Rc;

use sas_dsp_core::output::Output;
use sas_dsp_core::sas_simd::{utils as mono, PolyMask};
use sas_dsp_core::{processor_boilerplate, Processor, ProcessorImpl, ProcessorRef, SynthModule};

use super::formant_filter::FormantFilter;
use super::vocal_tract::VocalTract;
use crate::synth_filter::input as filter_input;

/// `vital::FormantModule`.
#[derive(Clone)]
pub struct FormantModule {
    prefix: String,
    /// `formant_filters_`: the global (original) style processors.
    formant_filters: Vec<ProcessorRef>,
    last_style: usize,
    mono: bool,
}

impl FormantModule {
    pub const AUDIO: usize = 0;
    pub const RESET: usize = 1;
    pub const RESONANCE: usize = 2;
    pub const BLEND: usize = 3;
    pub const STYLE: usize = 4;
    pub const NUM_INPUTS: usize = 5;

    /// `new FormantModule(prefix)`: `SynthModule(kNumInputs, 1)`. Call
    /// `init()` before processing.
    pub fn create(prefix: &str) -> ProcessorRef {
        SynthModule::create_with(Self::new(prefix), Self::NUM_INPUTS, 1, false)
    }

    /// The kernel of `FormantModule(prefix)`, for `setMono` before
    /// creating the node.
    pub fn new(prefix: &str) -> FormantModule {
        FormantModule {
            prefix: prefix.to_string(),
            formant_filters: Vec::new(),
            last_style: 0,
            mono: false,
        }
    }

    pub fn set_mono(&mut self, mono: bool) {
        self.mono = mono;
    }

    /// `createModControl(name, audio_rate, smooth_value)`.
    fn create_mod_control(
        &self,
        this: &Processor,
        name: &str,
        audio_rate: bool,
        smooth_value: bool,
    ) -> Rc<Output> {
        if self.mono {
            return this.create_mono_mod_control(name, audio_rate, smooth_value, None);
        }
        this.create_poly_mod_control(
            name,
            audio_rate,
            smooth_value,
            None,
            Some(&this.input(Self::RESET)),
        )
    }

    fn set_style(&mut self, this: &Processor, new_style: usize) {
        if self.last_style == new_style {
            return;
        }

        self.formant_filters[self.last_style].enable(false);
        self.formant_filters[new_style].enable(true);
        self.last_style = new_style;
        self.reset(this, PolyMask::full());
    }
}

impl ProcessorImpl for FormantModule {
    processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let prefix = self.prefix.clone();
        let formant_x = self.create_mod_control(this, &format!("{prefix}_formant_x"), true, true);
        let formant_y = self.create_mod_control(this, &format!("{prefix}_formant_y"), true, true);
        let formant_transpose =
            self.create_mod_control(this, &format!("{prefix}_formant_transpose"), true, true);
        let formant_resonance =
            self.create_mod_control(this, &format!("{prefix}_formant_resonance"), false, false);
        let formant_spread =
            self.create_mod_control(this, &format!("{prefix}_formant_spread"), false, false);

        self.formant_filters.clear();
        for style in 0..FormantFilter::NUM_FORMANT_STYLES {
            let formant_filter = FormantFilter::create(style);
            self.formant_filters.push(formant_filter.clone());
            this.add_processor(&formant_filter);
            formant_filter.enable(false);

            formant_filter.use_input(&this.input(Self::AUDIO), filter_input::AUDIO);
            formant_filter.use_input(&this.input(Self::RESET), filter_input::RESET);
            formant_filter.plug(&formant_spread, filter_input::SPREAD);
            formant_filter.plug(&formant_x, filter_input::INTERPOLATE_X);
            formant_filter.plug(&formant_y, filter_input::INTERPOLATE_Y);
            formant_filter.plug(&formant_transpose, filter_input::TRANSPOSE);
            formant_filter.plug(&formant_resonance, filter_input::RESONANCE);
            formant_filter.use_output(&this.output(0), 0);
        }

        let vocal_tract = VocalTract::create();
        vocal_tract.use_input(&this.input(Self::AUDIO), VocalTract::AUDIO);
        vocal_tract.use_input(&this.input(Self::RESET), VocalTract::RESET);
        vocal_tract.use_input(&this.input(Self::BLEND), VocalTract::BLEND);
        vocal_tract.plug(&formant_x, VocalTract::TONGUE_POSITION);
        vocal_tract.plug(&formant_y, VocalTract::TONGUE_HEIGHT);
        vocal_tract.use_output(&this.output(0), 0);
        self.formant_filters.push(vocal_tract.clone());
        this.add_processor(&vocal_tract);
        vocal_tract.enable(false);

        self.formant_filters[self.last_style].enable(true);

        this.router_init();
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let max_style = (FormantFilter::TOTAL_FORMANT_FILTERS - 1) as f32;
        let style = mono::clamp(this.input(Self::STYLE).at(0).get(0), 0.0, max_style) as usize;
        self.set_style(this, style);

        this.router_process(num_samples);
    }

    fn reset(&mut self, this: &Processor, reset_mask: PolyMask) {
        if let Some(local) = this.get_local_processor(&self.formant_filters[self.last_style]) {
            local.reset(reset_mask);
        }
    }

    fn hard_reset(&mut self, this: &Processor) {
        if let Some(local) = this.get_local_processor(&self.formant_filters[self.last_style]) {
            local.hard_reset();
        }
    }
}
