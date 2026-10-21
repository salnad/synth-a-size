// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::FilterModule`: one filter slot of a voice. It holds every filter
//! model, enables the one the `<prefix>_model` control selects, and mixes
//! its output with the dry input by `<prefix>_mix`.
//!
//! Ports `vital/src/synthesis/modules/filter_module.h` and
//! `filter_module.cpp`.

use std::rc::Rc;

use sas_dsp_core::operators::cr;
use sas_dsp_core::output::Output;
use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef, SynthModule};
use sas_simd::poly_utils as utils;
use sas_simd::PolyFloat;

use crate::character::{CombModule, FormantModule, PhaserFilter};
use crate::synth_filter::{input as filter_input, FilterModel};
use crate::{DigitalSvf, DiodeFilter, DirtyFilter, LadderFilter, SallenKeyFilter};

/// `vital::FilterModule`.
pub struct FilterModule {
    last_model: i32,
    prefix: String,
    create_on_value: bool,
    mono: bool,
    on: Option<ProcessorRef>,
    filter_model: Option<ProcessorRef>,
    mix: PolyFloat,
    filter_mix: Option<Rc<Output>>,
    comb_filter: ProcessorRef,
    digital_svf: ProcessorRef,
    diode_filter: ProcessorRef,
    dirty_filter: ProcessorRef,
    formant_filter: ProcessorRef,
    ladder_filter: ProcessorRef,
    phaser_filter: ProcessorRef,
    sallen_key_filter: ProcessorRef,
}

impl FilterModule {
    pub const AUDIO: usize = 0;
    pub const RESET: usize = 1;
    pub const KEYTRACK: usize = 2;
    pub const MIDI: usize = 3;
    pub const NUM_INPUTS: usize = 4;

    /// `new FilterModule(prefix)`: `SynthModule(kNumInputs, 1)` with every
    /// filter model added. Call `init()` before processing.
    pub fn create(prefix: &str) -> ProcessorRef {
        Self::create_with(prefix, |_| {})
    }

    /// `create`, with `configure` applied to the kernel before the node
    /// exists (for `setCreateOnValue` / `setMono`, which upstream calls
    /// before `init`).
    pub fn create_with(prefix: &str, configure: impl FnOnce(&mut FilterModule)) -> ProcessorRef {
        let mut kernel = FilterModule {
            last_model: -1,
            prefix: prefix.to_string(),
            create_on_value: true,
            mono: false,
            on: None,
            filter_model: None,
            mix: PolyFloat::splat(0.0),
            filter_mix: None,
            comb_filter: CombModule::create(),
            digital_svf: DigitalSvf::create(),
            diode_filter: DiodeFilter::create(),
            dirty_filter: DirtyFilter::create(),
            formant_filter: FormantModule::create(prefix),
            ladder_filter: LadderFilter::create(),
            phaser_filter: PhaserFilter::create(false),
            sallen_key_filter: SallenKeyFilter::create(),
        };
        configure(&mut kernel);

        let module = SynthModule::create_with(kernel, Self::NUM_INPUTS, 1, false);
        module.with::<FilterModule, _>(|filter, this| {
            this.add_submodule(&filter.comb_filter);
            this.add_submodule(&filter.formant_filter);
            this.add_processor(&filter.comb_filter);
            this.add_processor(&filter.digital_svf);
            this.add_processor(&filter.diode_filter);
            this.add_processor(&filter.dirty_filter);
            this.add_processor(&filter.formant_filter);
            this.add_processor(&filter.ladder_filter);
            this.add_processor(&filter.phaser_filter);
            this.add_processor(&filter.sallen_key_filter);
        });
        module
    }

    pub fn set_create_on_value(&mut self, create_on_value: bool) {
        self.create_on_value = create_on_value;
    }

    /// `setMono(mono)`; call before `init`.
    pub fn set_mono(&mut self, mono: bool) {
        self.mono = mono;
        self.formant_filter
            .with::<FormantModule, _>(|formant, _| formant.set_mono(mono));
    }

    /// `getOnValue()`.
    pub fn get_on_value(&self) -> Option<&ProcessorRef> {
        self.on.as_ref()
    }

    /// `createModControl(name, audio_rate, smooth_value, internal_modulation)`.
    fn create_mod_control(
        &self,
        this: &Processor,
        name: &str,
        audio_rate: bool,
        smooth_value: bool,
        internal_modulation: Option<&Rc<Output>>,
    ) -> Rc<Output> {
        if self.mono {
            return this.create_mono_mod_control(
                name,
                audio_rate,
                smooth_value,
                internal_modulation,
            );
        }
        this.create_poly_mod_control(
            name,
            audio_rate,
            smooth_value,
            internal_modulation,
            Some(&this.input(Self::RESET)),
        )
    }

    /// `setModel(new_model)`.
    fn set_model(&mut self, this: &Processor, new_model: i32) {
        let is = |model: FilterModel| new_model == model as i32;
        self.comb_filter.enable(is(FilterModel::Comb));
        self.digital_svf.enable(is(FilterModel::Digital));
        self.diode_filter.enable(is(FilterModel::Diode));
        self.dirty_filter.enable(is(FilterModel::Dirty));
        self.formant_filter.enable(is(FilterModel::Formant));
        self.ladder_filter.enable(is(FilterModel::Ladder));
        self.phaser_filter.enable(is(FilterModel::Phase));
        self.sallen_key_filter.enable(is(FilterModel::Analog));

        if new_model == self.last_model {
            return;
        }

        let to_reset = match FilterModel::from_index(new_model) {
            Some(FilterModel::Analog) => Some(&self.sallen_key_filter),
            Some(FilterModel::Comb) => Some(&self.comb_filter),
            Some(FilterModel::Digital) => Some(&self.digital_svf),
            Some(FilterModel::Diode) => Some(&self.diode_filter),
            Some(FilterModel::Dirty) => Some(&self.dirty_filter),
            Some(FilterModel::Formant) => Some(&self.formant_filter),
            Some(FilterModel::Ladder) => Some(&self.ladder_filter),
            Some(FilterModel::Phase) => Some(&self.phaser_filter),
            None => None,
        };
        if let Some(to_reset) = to_reset {
            if let Some(local) = this.get_local_processor(to_reset) {
                local.hard_reset();
            }
        }

        self.last_model = new_model;
    }
}

impl Clone for FilterModule {
    /// `clone()`: a copy with `last_model_ = -1`.
    fn clone(&self) -> Self {
        FilterModule {
            last_model: -1,
            prefix: self.prefix.clone(),
            create_on_value: self.create_on_value,
            mono: self.mono,
            on: self.on.clone(),
            filter_model: self.filter_model.clone(),
            mix: self.mix,
            filter_mix: self.filter_mix.clone(),
            comb_filter: self.comb_filter.clone(),
            digital_svf: self.digital_svf.clone(),
            diode_filter: self.diode_filter.clone(),
            dirty_filter: self.dirty_filter.clone(),
            formant_filter: self.formant_filter.clone(),
            ladder_filter: self.ladder_filter.clone(),
            phaser_filter: self.phaser_filter.clone(),
            sallen_key_filter: self.sallen_key_filter.clone(),
        }
    }
}

impl ProcessorImpl for FilterModule {
    sas_dsp_core::processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let prefix = self.prefix.clone();
        let keytrack_amount =
            self.create_mod_control(this, &format!("{prefix}_keytrack"), false, false, None);
        let current_keytrack = cr::Multiply::create();
        current_keytrack.use_input(&this.input(Self::KEYTRACK), 0);
        current_keytrack.plug(&keytrack_amount, 1);

        let midi_cutoff = self.create_mod_control(
            this,
            &format!("{prefix}_cutoff"),
            true,
            true,
            Some(&current_keytrack.output(0)),
        );
        let resonance =
            self.create_mod_control(this, &format!("{prefix}_resonance"), false, false, None);
        let drive = self.create_mod_control(this, &format!("{prefix}_drive"), false, false, None);
        let blend = self.create_mod_control(this, &format!("{prefix}_blend"), false, false, None);
        let blend_transpose = self.create_mod_control(
            this,
            &format!("{prefix}_blend_transpose"),
            false,
            false,
            None,
        );

        if self.create_on_value {
            self.on = Some(this.create_base_control(&format!("{prefix}_on"), false, false));
        }
        let filter_style = this.create_base_control(&format!("{prefix}_style"), false, false);
        self.filter_model =
            Some(this.create_base_control(&format!("{prefix}_model"), false, false));

        self.filter_mix =
            Some(self.create_mod_control(this, &format!("{prefix}_mix"), false, false, None));

        let audio = this.input(Self::AUDIO);
        let reset = this.input(Self::RESET);
        let out = this.output(0);

        let comb = &self.comb_filter;
        comb.use_input(&audio, CombModule::AUDIO);
        comb.plug(&filter_style, CombModule::STYLE);
        comb.use_input(&reset, CombModule::RESET);
        comb.use_input(&this.input(Self::MIDI), CombModule::MIDI);
        comb.plug(&midi_cutoff, CombModule::MIDI_CUTOFF);
        comb.plug(&blend_transpose, CombModule::MIDI_BLEND_TRANSPOSE);
        comb.plug(&blend, CombModule::FILTER_CUTOFF_BLEND);
        comb.plug(&resonance, CombModule::RESONANCE);
        comb.use_output(&out, 0);

        let svf = &self.digital_svf;
        svf.use_input(&audio, filter_input::AUDIO);
        svf.plug(&filter_style, filter_input::STYLE);
        svf.plug(&blend, filter_input::PASS_BLEND);
        svf.use_input(&reset, filter_input::RESET);
        svf.plug(&midi_cutoff, filter_input::MIDI_CUTOFF);
        svf.plug(&resonance, filter_input::RESONANCE);
        svf.plug(&drive, filter_input::DRIVE_GAIN);
        svf.use_output(&out, 0);

        let diode = &self.diode_filter;
        diode.use_input(&audio, filter_input::AUDIO);
        diode.use_input(&reset, filter_input::RESET);
        diode.plug(&resonance, filter_input::RESONANCE);
        diode.plug(&filter_style, filter_input::STYLE);
        diode.plug(&blend, filter_input::PASS_BLEND);
        diode.plug(&midi_cutoff, filter_input::MIDI_CUTOFF);
        diode.plug(&drive, filter_input::DRIVE_GAIN);
        diode.use_output(&out, 0);

        let dirty = &self.dirty_filter;
        dirty.use_input(&audio, filter_input::AUDIO);
        dirty.use_input(&reset, filter_input::RESET);
        dirty.plug(&resonance, filter_input::RESONANCE);
        dirty.plug(&filter_style, filter_input::STYLE);
        dirty.plug(&blend, filter_input::PASS_BLEND);
        dirty.plug(&midi_cutoff, filter_input::MIDI_CUTOFF);
        dirty.plug(&drive, filter_input::DRIVE_GAIN);
        dirty.use_output(&out, 0);

        let formant = &self.formant_filter;
        formant.use_input(&audio, FormantModule::AUDIO);
        formant.use_input(&reset, FormantModule::RESET);
        formant.plug(&blend, FormantModule::BLEND);
        formant.plug(&filter_style, FormantModule::STYLE);
        formant.use_output(&out, 0);

        let ladder = &self.ladder_filter;
        ladder.use_input(&audio, filter_input::AUDIO);
        ladder.use_input(&reset, filter_input::RESET);
        ladder.plug(&resonance, filter_input::RESONANCE);
        ladder.plug(&filter_style, filter_input::STYLE);
        ladder.plug(&blend, filter_input::PASS_BLEND);
        ladder.plug(&midi_cutoff, filter_input::MIDI_CUTOFF);
        ladder.plug(&drive, filter_input::DRIVE_GAIN);
        ladder.use_output(&out, 0);

        let phaser = &self.phaser_filter;
        phaser.use_input(&audio, filter_input::AUDIO);
        phaser.use_input(&reset, filter_input::RESET);
        phaser.plug(&resonance, filter_input::RESONANCE);
        phaser.plug(&filter_style, filter_input::STYLE);
        phaser.plug(&blend_transpose, filter_input::TRANSPOSE);
        phaser.plug(&blend, filter_input::PASS_BLEND);
        phaser.plug(&midi_cutoff, filter_input::MIDI_CUTOFF);
        phaser.plug(&drive, filter_input::DRIVE_GAIN);
        phaser.use_output(&out, 0);

        let sallen_key = &self.sallen_key_filter;
        sallen_key.plug(&filter_style, filter_input::STYLE);
        sallen_key.use_input(&audio, filter_input::AUDIO);
        sallen_key.plug(&blend, filter_input::PASS_BLEND);
        sallen_key.use_input(&reset, filter_input::RESET);
        sallen_key.plug(&midi_cutoff, filter_input::MIDI_CUTOFF);
        sallen_key.plug(&resonance, filter_input::RESONANCE);
        sallen_key.plug(&drive, filter_input::DRIVE_GAIN);
        sallen_key.use_output(&out, 0);

        self.comb_filter.enable(false);
        self.digital_svf.enable(false);
        self.diode_filter.enable(false);
        self.dirty_filter.enable(false);
        self.formant_filter.enable(false);
        self.ladder_filter.enable(false);
        self.sallen_key_filter.enable(false);

        this.add_processor(&current_keytrack);
        this.router_init();
    }

    /// Upstream hard-resets the original (global) filter processors.
    fn hard_reset(&mut self, _this: &Processor) {
        self.comb_filter.hard_reset();
        self.digital_svf.hard_reset();
        self.diode_filter.hard_reset();
        self.dirty_filter.hard_reset();
        self.formant_filter.hard_reset();
        self.ladder_filter.hard_reset();
        self.phaser_filter.hard_reset();
        self.sallen_key_filter.hard_reset();
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let on = match &self.on {
            None => true,
            Some(on) => on.value() > 0.5,
        };
        let model = self
            .filter_model
            .as_ref()
            .expect("init ran")
            .value()
            .round() as i32;
        self.set_model(this, model);

        if on {
            this.router_process(num_samples);

            let mut current_mix = self.mix;
            let filter_mix = self.filter_mix.as_ref().expect("init ran");
            self.mix = utils::clamp(filter_mix.buffer()[0].get(), 0.0, 1.0);
            current_mix = utils::mask_load(current_mix, self.mix, this.get_reset_mask(Self::RESET));
            let delta_mix = (self.mix - current_mix) * (1.0f32 / num_samples as f32);

            let audio_out = this.output_buffer(0);
            let audio_in = this.input_buffer(Self::AUDIO);
            for i in 0..num_samples {
                current_mix += delta_mix;
                audio_out[i].set(utils::interpolate(
                    audio_in[i].get(),
                    audio_out[i].get(),
                    current_mix,
                ));
            }
        } else {
            let audio_out = this.output_buffer(0);
            for value in audio_out.iter().take(num_samples) {
                value.set(PolyFloat::splat(0.0));
            }
        }
    }
}
