// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::SynthModule`: a router with named controls, modulation sources
//! and destinations, and sub-modules.
//!
//! Ports `vital/src/synthesis/framework/synth_module.h` and
//! `synth_module.cpp`.
//!
//! The module data (`data_` upstream, shared by clones) is attached to the
//! router node, so any kernel can be a `SynthModule`: create the node with
//! [`SynthModule::create`] (or [`SynthModule::create_with`] for your own
//! kernel) and use the `Processor` methods below.
//!
//! Parameter details (default, scale, range) come from the parameter
//! registry, which lives in `sas-preset`. The engine installs a lookup with
//! [`set_parameter_lookup`] before building modules.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use sas_simd::poly_utils as utils;
use sas_simd::{PolyFloat, PolyMask};

use crate::operators::{self, ModulationSum, TempoChooser, VariableAdd};
use crate::output::{Input, Output};
use crate::processor::{Processor, ProcessorImpl, ProcessorRef};
use crate::utilities::smooth_value::{CrSmoothValue, SmoothValue};
use crate::utilities::value_switch::ValueSwitch;
use crate::value::Value;

/// `ValueDetails::ValueScale`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ValueScale {
    Indexed,
    #[default]
    Linear,
    Quadratic,
    Cubic,
    Quartic,
    SquareRoot,
    Exponential,
}

/// The fields of `ValueDetails` that the engine graph reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterDetails {
    pub min: f32,
    pub max: f32,
    pub default_value: f32,
    /// Offset added after quadratic/cubic/quartic/root/exponential scaling.
    pub post_offset: f32,
    pub value_scale: ValueScale,
}

impl Default for ParameterDetails {
    fn default() -> Self {
        ParameterDetails {
            min: 0.0,
            max: 1.0,
            default_value: 0.0,
            post_offset: 0.0,
            value_scale: ValueScale::Linear,
        }
    }
}

type Lookup = Arc<dyn Fn(&str) -> ParameterDetails + Send + Sync>;

static PARAMETER_LOOKUP: RwLock<Option<Lookup>> = RwLock::new(None);

/// Installs the parameter registry lookup (`Parameters::getDetails`).
pub fn set_parameter_lookup(lookup: impl Fn(&str) -> ParameterDetails + Send + Sync + 'static) {
    *PARAMETER_LOOKUP.write().unwrap() = Some(Arc::new(lookup));
}

/// `Parameters::getDetails(name)`.
///
/// # Panics
/// If no lookup is installed.
pub fn parameter_details(name: &str) -> ParameterDetails {
    let lookup =
        PARAMETER_LOOKUP.read().unwrap().clone().expect(
            "no parameter lookup installed (sas_dsp_core::synth_module::set_parameter_lookup)",
        );
    lookup(name)
}

/// `vital::StatusOutput`: the latest value of an output, for the UI.
pub struct StatusOutput {
    source: Rc<Output>,
    value: Cell<PolyFloat>,
}

impl StatusOutput {
    /// `kClearValue` (`INT_MIN` as a float).
    pub const CLEAR_VALUE: f32 = i32::MIN as f32;

    pub fn new(source: Rc<Output>) -> StatusOutput {
        StatusOutput {
            source,
            value: Cell::new(PolyFloat::zero()),
        }
    }

    pub fn value(&self) -> PolyFloat {
        self.value.get()
    }

    /// `update(voice_mask)`: the masked voice copied into both voices.
    pub fn update_masked(&self, voice_mask: PolyMask) {
        let masked_value = self.source.at(0) & voice_mask;
        self.value
            .set(masked_value + utils::swap_voices(masked_value));
    }

    /// `update()`.
    pub fn update(&self) {
        self.value.set(self.source.at(0));
    }

    pub fn clear(&self) {
        self.value.set(PolyFloat::splat(Self::CLEAR_VALUE));
    }

    pub fn is_clear_value(&self, value: PolyFloat) -> bool {
        PolyFloat::equal(value, PolyFloat::splat(Self::CLEAR_VALUE)).any_mask() != 0
    }
}

/// `vital::ModuleData`.
#[derive(Default)]
pub struct ModuleData {
    pub owned_mono_processors: Vec<ProcessorRef>,
    pub sub_modules: Vec<ProcessorRef>,

    pub controls: BTreeMap<String, ProcessorRef>,
    pub mod_sources: BTreeMap<String, Rc<Output>>,
    pub status_outputs: BTreeMap<String, Rc<StatusOutput>>,
    pub mono_mod_destinations: BTreeMap<String, ProcessorRef>,
    pub poly_mod_destinations: BTreeMap<String, ProcessorRef>,
    pub mono_modulation_readout: BTreeMap<String, Rc<Output>>,
    pub poly_modulation_readout: BTreeMap<String, Rc<Output>>,
    pub mono_modulation_switches: BTreeMap<String, ProcessorRef>,
    pub poly_modulation_switches: BTreeMap<String, ProcessorRef>,
}

/// Kernel of a plain `vital::SynthModule`.
#[derive(Clone, Default)]
pub struct SynthModule;

impl ProcessorImpl for SynthModule {
    crate::processor_boilerplate!();
}

impl SynthModule {
    /// `new SynthModule(num_inputs, num_outputs, control_rate)`.
    pub fn create(num_inputs: usize, num_outputs: usize, control_rate: bool) -> ProcessorRef {
        Self::create_with(SynthModule, num_inputs, num_outputs, control_rate)
    }

    /// A `SynthModule` node with a custom kernel (a `SynthModule` subclass).
    pub fn create_with(
        kernel: impl ProcessorImpl,
        num_inputs: usize,
        num_outputs: usize,
        control_rate: bool,
    ) -> ProcessorRef {
        let processor = Processor::new_router(kernel, num_inputs, num_outputs, control_rate);
        *processor.module_data.borrow_mut() = Some(Rc::new(RefCell::new(ModuleData::default())));
        processor
    }
}

fn insert_missing<V: Clone>(target: &mut BTreeMap<String, V>, source: &BTreeMap<String, V>) {
    for (key, value) in source {
        target.entry(key.clone()).or_insert_with(|| value.clone());
    }
}

impl Processor {
    /// True when this node is a `SynthModule`.
    pub fn is_synth_module(&self) -> bool {
        self.module_data.borrow().is_some()
    }

    /// The module data (`data_`), shared with clones.
    ///
    /// # Panics
    /// If this node is not a `SynthModule`.
    pub fn module_data(&self) -> Rc<RefCell<ModuleData>> {
        self.module_data
            .borrow()
            .clone()
            .expect("not a SynthModule")
    }

    /// `SynthModule::enable` (the default for module kernels).
    pub fn synth_module_enable(&self, enable: bool) {
        if self.enabled() == enable {
            return;
        }
        self.processor_enable(enable);
        self.enable_owned_processors(enable);
    }

    /// `enableOwnedProcessors(enable)`.
    pub fn enable_owned_processors(&self, enable: bool) {
        let (owned, subs) = {
            let data = self.module_data();
            let data = data.borrow();
            (data.owned_mono_processors.clone(), data.sub_modules.clone())
        };
        for processor in owned {
            processor.enable(enable);
        }
        for module in subs {
            module.enable(enable);
        }
    }

    /// `addMonoProcessor(processor, own)`.
    pub fn add_mono_processor(&self, processor: &ProcessorRef, own: bool) {
        self.get_mono_router().add_processor(processor);
        if own {
            self.module_data()
                .borrow_mut()
                .owned_mono_processors
                .push(processor.clone());
        }
    }

    /// `addIdleMonoProcessor(processor)`.
    pub fn add_idle_mono_processor(&self, processor: &ProcessorRef) {
        self.get_mono_router().add_idle_processor(processor);
    }

    /// `addSubmodule(module)`.
    pub fn add_submodule(&self, module: &ProcessorRef) {
        self.module_data()
            .borrow_mut()
            .sub_modules
            .push(module.clone());
    }

    fn sub_modules(&self) -> Vec<ProcessorRef> {
        self.module_data().borrow().sub_modules.clone()
    }

    /// `getControls()`: this module's and all sub-modules' controls.
    pub fn get_controls(&self) -> BTreeMap<String, ProcessorRef> {
        let mut all_controls = self.module_data().borrow().controls.clone();
        for module in self.sub_modules() {
            insert_missing(&mut all_controls, &module.get_controls());
        }
        all_controls
    }

    /// `getModulationSource(name)`.
    pub fn get_modulation_source(&self, name: &str) -> Option<Rc<Output>> {
        if let Some(source) = self.module_data().borrow().mod_sources.get(name) {
            return Some(source.clone());
        }
        self.sub_modules()
            .iter()
            .find_map(|module| module.get_modulation_source(name))
    }

    /// `getStatusOutput(name)`.
    pub fn get_status_output(&self, name: &str) -> Option<Rc<StatusOutput>> {
        if let Some(status) = self.module_data().borrow().status_outputs.get(name) {
            return Some(status.clone());
        }
        self.sub_modules()
            .iter()
            .find_map(|module| module.get_status_output(name))
    }

    /// `getModulationDestination(name, poly)`.
    pub fn get_modulation_destination(&self, name: &str, poly: bool) -> Option<ProcessorRef> {
        let poly_destination = self.get_poly_modulation_destination(name);
        if poly && poly_destination.is_some() {
            return poly_destination;
        }
        self.get_mono_modulation_destination(name)
    }

    /// `getMonoModulationDestination(name)`.
    pub fn get_mono_modulation_destination(&self, name: &str) -> Option<ProcessorRef> {
        if let Some(d) = self.module_data().borrow().mono_mod_destinations.get(name) {
            return Some(d.clone());
        }
        self.sub_modules()
            .iter()
            .find_map(|module| module.get_mono_modulation_destination(name))
    }

    /// `getPolyModulationDestination(name)`.
    pub fn get_poly_modulation_destination(&self, name: &str) -> Option<ProcessorRef> {
        if let Some(d) = self.module_data().borrow().poly_mod_destinations.get(name) {
            return Some(d.clone());
        }
        self.sub_modules()
            .iter()
            .find_map(|module| module.get_poly_modulation_destination(name))
    }

    /// `getModulationSwitch(name, poly)`.
    pub fn get_modulation_switch(&self, name: &str, poly: bool) -> Option<ProcessorRef> {
        if poly {
            self.get_poly_modulation_switch(name)
        } else {
            self.get_mono_modulation_switch(name)
        }
    }

    /// `getMonoModulationSwitch(name)`.
    pub fn get_mono_modulation_switch(&self, name: &str) -> Option<ProcessorRef> {
        if let Some(s) = self
            .module_data()
            .borrow()
            .mono_modulation_switches
            .get(name)
        {
            return Some(s.clone());
        }
        self.sub_modules()
            .iter()
            .find_map(|module| module.get_mono_modulation_switch(name))
    }

    /// `getPolyModulationSwitch(name)`.
    pub fn get_poly_modulation_switch(&self, name: &str) -> Option<ProcessorRef> {
        if let Some(s) = self
            .module_data()
            .borrow()
            .poly_modulation_switches
            .get(name)
        {
            return Some(s.clone());
        }
        self.sub_modules()
            .iter()
            .find_map(|module| module.get_poly_modulation_switch(name))
    }

    /// `updateAllModulationSwitches()`: a switch routes the modulated value
    /// only while something modulates its destination.
    pub fn update_all_modulation_switches(&self) {
        let (mono_switches, poly_switches, mono_destinations, poly_destinations) = {
            let data = self.module_data();
            let data = data.borrow();
            (
                data.mono_modulation_switches.clone(),
                data.poly_modulation_switches.clone(),
                data.mono_mod_destinations.clone(),
                data.poly_mod_destinations.clone(),
            )
        };

        for (name, mod_switch) in &mono_switches {
            let mut enable = mono_destinations[name].connected_inputs() > 1;
            if let Some(poly) = poly_destinations.get(name) {
                enable = enable || poly.connected_inputs() > 0;
            }
            mod_switch.set_value(if enable { 1.0 } else { 0.0 });
        }

        for (name, mod_switch) in &poly_switches {
            let enable = poly_destinations[name].connected_inputs() > 0;
            mod_switch.set_value(if enable { 1.0 } else { 0.0 });
        }

        for module in self.sub_modules() {
            module.update_all_modulation_switches();
        }
    }

    /// `getModulationSources()`: merges sub-modules' sources into this
    /// module's map (as upstream does) and returns it.
    pub fn get_modulation_sources(&self) -> BTreeMap<String, Rc<Output>> {
        for module in self.sub_modules() {
            let sub = module.get_modulation_sources();
            insert_missing(&mut self.module_data().borrow_mut().mod_sources, &sub);
        }
        self.module_data().borrow().mod_sources.clone()
    }

    /// `getMonoModulationDestinations()` (merges sub-modules' entries).
    pub fn get_mono_modulation_destinations(&self) -> BTreeMap<String, ProcessorRef> {
        for module in self.sub_modules() {
            let sub = module.get_mono_modulation_destinations();
            insert_missing(
                &mut self.module_data().borrow_mut().mono_mod_destinations,
                &sub,
            );
        }
        self.module_data().borrow().mono_mod_destinations.clone()
    }

    /// `getPolyModulationDestinations()` (merges sub-modules' entries).
    pub fn get_poly_modulation_destinations(&self) -> BTreeMap<String, ProcessorRef> {
        for module in self.sub_modules() {
            let sub = module.get_poly_modulation_destinations();
            insert_missing(
                &mut self.module_data().borrow_mut().poly_mod_destinations,
                &sub,
            );
        }
        self.module_data().borrow().poly_mod_destinations.clone()
    }

    /// `getMonoModulations()` (merges sub-modules' entries).
    pub fn get_mono_modulations(&self) -> BTreeMap<String, Rc<Output>> {
        for module in self.sub_modules() {
            let sub = module.get_mono_modulations();
            insert_missing(
                &mut self.module_data().borrow_mut().mono_modulation_readout,
                &sub,
            );
        }
        self.module_data().borrow().mono_modulation_readout.clone()
    }

    /// `getPolyModulations()` (merges sub-modules' entries).
    pub fn get_poly_modulations(&self) -> BTreeMap<String, Rc<Output>> {
        for module in self.sub_modules() {
            let sub = module.get_poly_modulations();
            insert_missing(
                &mut self.module_data().borrow_mut().poly_modulation_readout,
                &sub,
            );
        }
        self.module_data().borrow().poly_modulation_readout.clone()
    }

    /// `createBaseControl(name, audio_rate, smooth_value)`.
    pub fn create_base_control(
        &self,
        name: &str,
        audio_rate: bool,
        smooth_value: bool,
    ) -> ProcessorRef {
        let default_value = parameter_details(name).default_value;

        let value = match (audio_rate, smooth_value) {
            (true, true) => {
                let value = SmoothValue::create(default_value);
                self.add_mono_processor(&value, false);
                value
            }
            (true, false) => {
                let value = Value::create(default_value, false);
                self.add_idle_mono_processor(&value);
                value
            }
            (false, true) => {
                let value = CrSmoothValue::create(default_value);
                self.add_mono_processor(&value, false);
                value
            }
            (false, false) => {
                let value = Value::create_control_rate(default_value);
                self.add_idle_mono_processor(&value);
                value
            }
        };

        self.module_data()
            .borrow_mut()
            .controls
            .insert(name.to_string(), value.clone());
        value
    }

    /// `createBaseModControl(name, audio_rate, smooth_value, internal_modulation)`.
    pub fn create_base_mod_control(
        &self,
        name: &str,
        audio_rate: bool,
        smooth_value: bool,
        internal_modulation: Option<&Rc<Output>>,
    ) -> Rc<Output> {
        let base_val = self.create_base_control(name, audio_rate, smooth_value);

        let mono_total = if audio_rate {
            ModulationSum::create(0)
        } else {
            operators::cr::VariableAdd::create(0)
        };

        mono_total.plug_next(&base_val);
        self.add_mono_processor(&mono_total, false);
        {
            let data = self.module_data();
            let mut data = data.borrow_mut();
            data.mono_mod_destinations
                .insert(name.to_string(), mono_total.clone());
            data.mono_modulation_readout
                .insert(name.to_string(), mono_total.output(0));
        }

        let control_switch = ValueSwitch::create(0.0);
        control_switch.plug_next(&base_val);
        control_switch.plug_next(&mono_total);

        if let Some(internal_modulation) = internal_modulation {
            mono_total.plug_next(internal_modulation);
        } else {
            ValueSwitch::add_processor(&control_switch, &mono_total);
        }

        self.add_idle_mono_processor(&control_switch);
        if smooth_value || internal_modulation.is_some() {
            control_switch.set_value(1.0);
        } else {
            control_switch.set_value(0.0);
        }
        self.module_data()
            .borrow_mut()
            .mono_modulation_switches
            .insert(name.to_string(), control_switch.clone());

        control_switch.output(ValueSwitch::SWITCH)
    }

    /// Appends the scaling processor for `details.value_scale` (shared tail
    /// of `createMonoModControl` and `createPolyModControl`).
    fn create_scale(
        &self,
        details: &ParameterDetails,
        control_rate_total: Rc<Output>,
        poly: bool,
        exponential_offset: bool,
    ) -> Rc<Output> {
        let scale = match details.value_scale {
            ValueScale::Quadratic => Some(if details.post_offset != 0.0 {
                operators::cr::Quadratic::create(details.post_offset)
            } else {
                operators::cr::Square::create()
            }),
            ValueScale::Cubic => {
                debug_assert!(details.post_offset == 0.0);
                Some(if details.post_offset != 0.0 {
                    operators::cr::Cubic::create(details.post_offset)
                } else {
                    operators::cr::Cube::create()
                })
            }
            ValueScale::Quartic => {
                debug_assert!(details.post_offset == 0.0);
                Some(if details.post_offset != 0.0 {
                    operators::cr::Quartic::create(details.post_offset)
                } else {
                    operators::cr::Quart::create()
                })
            }
            ValueScale::Exponential => Some(operators::cr::ExponentialScale::create(
                details.min,
                details.max,
                2.0,
                if exponential_offset {
                    details.post_offset
                } else {
                    0.0
                },
            )),
            ValueScale::SquareRoot => Some(operators::cr::Root::create(details.post_offset)),
            ValueScale::Indexed | ValueScale::Linear => None,
        };

        match scale {
            Some(scale) => {
                scale.plug(&control_rate_total, 0);
                if poly {
                    self.add_processor(&scale);
                } else {
                    self.add_mono_processor(&scale, true);
                }
                scale.output(0)
            }
            None => control_rate_total,
        }
    }

    /// `createMonoModControl(name, audio_rate, smooth_value, internal_modulation)`.
    pub fn create_mono_mod_control(
        &self,
        name: &str,
        audio_rate: bool,
        smooth_value: bool,
        internal_modulation: Option<&Rc<Output>>,
    ) -> Rc<Output> {
        let details = parameter_details(name);
        let control_rate_total =
            self.create_base_mod_control(name, audio_rate, smooth_value, internal_modulation);
        if audio_rate {
            return control_rate_total;
        }
        self.create_scale(&details, control_rate_total, false, false)
    }

    /// `createPolyModControl(name, audio_rate, smooth_value, internal_modulation, reset)`.
    pub fn create_poly_mod_control(
        &self,
        name: &str,
        audio_rate: bool,
        smooth_value: bool,
        internal_modulation: Option<&Rc<Output>>,
        reset: Option<&Rc<Input>>,
    ) -> Rc<Output> {
        let details = parameter_details(name);
        let base_control = self.create_base_mod_control(name, audio_rate, smooth_value, None);

        let poly_total = if audio_rate {
            let total = ModulationSum::create(0);
            if let Some(reset) = reset {
                total.use_input(reset, ModulationSum::RESET);
            }
            total
        } else {
            operators::cr::VariableAdd::create(0)
        };
        self.add_processor(&poly_total);
        self.module_data()
            .borrow_mut()
            .poly_mod_destinations
            .insert(name.to_string(), poly_total.clone());

        let modulation_total = if audio_rate {
            operators::Add::create()
        } else {
            operators::cr::Add::create()
        };
        modulation_total.plug(&base_control, 0);
        modulation_total.plug(&poly_total, 1);
        self.add_processor(&modulation_total);

        self.module_data()
            .borrow_mut()
            .poly_modulation_readout
            .insert(name.to_string(), poly_total.output(0));

        let control_switch = ValueSwitch::create(0.0);
        control_switch.plug_next(&base_control);
        control_switch.plug_next(&modulation_total);

        if let Some(internal_modulation) = internal_modulation {
            poly_total.plug_next(internal_modulation);
            control_switch.set_value(1.0);
        } else {
            ValueSwitch::add_processor(&control_switch, &poly_total);
            ValueSwitch::add_processor(&control_switch, &modulation_total);
            control_switch.set_value(0.0);
        }
        self.add_idle_processor(&control_switch);
        self.module_data()
            .borrow_mut()
            .poly_modulation_switches
            .insert(name.to_string(), control_switch.clone());

        let control_rate_total = control_switch.output(ValueSwitch::SWITCH);
        if audio_rate {
            return control_rate_total;
        }
        self.create_scale(&details, control_rate_total, true, true)
    }

    /// `createTempoSyncSwitch(name, frequency, beats_per_second, poly, midi)`.
    pub fn create_tempo_sync_switch(
        &self,
        name: &str,
        frequency: &ProcessorRef,
        beats_per_second: &Rc<Output>,
        poly: bool,
        midi: Option<&Rc<Input>>,
    ) -> Rc<Output> {
        let tempo_name = format!("{name}_tempo");
        let tempo = if poly {
            self.create_poly_mod_control(&tempo_name, false, false, None, None)
        } else {
            self.create_mono_mod_control(&tempo_name, false, false, None)
        };

        let sync = Value::create_control_rate(1.0);
        self.module_data()
            .borrow_mut()
            .controls
            .insert(format!("{name}_sync"), sync.clone());
        self.add_idle_processor(&sync);

        let tempo_chooser = TempoChooser::create();
        tempo_chooser.plug(&sync, TempoChooser::SYNC);
        tempo_chooser.plug(&tempo, TempoChooser::TEMPO_INDEX);
        tempo_chooser.plug(frequency, TempoChooser::FREQUENCY);
        tempo_chooser.plug(beats_per_second, TempoChooser::BEATS_PER_SECOND);

        if let Some(midi) = midi {
            let transpose_name = format!("{name}_keytrack_transpose");
            let tune_name = format!("{name}_keytrack_tune");
            let (keytrack_transpose, keytrack_tune) = if poly {
                (
                    self.create_poly_mod_control(&transpose_name, false, false, None, None),
                    self.create_poly_mod_control(&tune_name, false, false, None, None),
                )
            } else {
                (
                    self.create_mono_mod_control(&transpose_name, false, false, None),
                    self.create_mono_mod_control(&tune_name, false, false, None),
                )
            };

            tempo_chooser.plug(&keytrack_transpose, TempoChooser::KEYTRACK_TRANSPOSE);
            tempo_chooser.plug(&keytrack_tune, TempoChooser::KEYTRACK_TUNE);
            tempo_chooser.use_input(midi, TempoChooser::MIDI);
        }

        if poly {
            self.add_processor(&tempo_chooser);
        } else {
            self.add_mono_processor(&tempo_chooser, true);
        }

        tempo_chooser.output(0)
    }

    /// `createStatusOutput(name, source)`.
    pub fn create_status_output(&self, name: &str, source: &Rc<Output>) {
        self.module_data()
            .borrow_mut()
            .status_outputs
            .insert(name.to_string(), Rc::new(StatusOutput::new(source.clone())));
    }

    /// Registers a modulation source (`data_->mod_sources[name] = output`).
    pub fn add_modulation_source(&self, name: &str, source: &Rc<Output>) {
        self.module_data()
            .borrow_mut()
            .mod_sources
            .insert(name.to_string(), source.clone());
    }
}

// Silence an unused-import warning when VariableAdd is only used via cr::.
#[allow(unused)]
fn _uses(_: VariableAdd) {}
