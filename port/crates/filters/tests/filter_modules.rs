// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity for `FilterModule` and `FiltersModule` through the control harness
//! of `migration/reference/src/modules/filters/filter_modules.cpp`: the
//! module's inputs come first, every named control is set from its input
//! (lane 0) once per block before processing, and the module's output is
//! copied out. Controls are created from the `sas-preset` parameter
//! registry, as the engine creates them.

mod support;

use sas_dsp_core::synth_module::{set_parameter_lookup, ParameterDetails, ValueScale};
use sas_dsp_core::testing::InputRate;
use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_filters::modules::{FilterModule, FiltersModule};
use sas_preset::registry::{self, ValueScale as PresetScale};
use support::{check_module, param_bool, Audio, Control, ModuleDef, Trigger};

/// `Parameters::getDetails(name)` from the `sas-preset` registry.
fn registry_details(name: &str) -> ParameterDetails {
    let info = registry::lookup(name).unwrap_or_else(|| panic!("unknown parameter {name:?}"));
    ParameterDetails {
        min: info.min,
        max: info.max,
        default_value: info.default_value,
        post_offset: info.post_offset,
        value_scale: match info.value_scale {
            PresetScale::Indexed => ValueScale::Indexed,
            PresetScale::Linear => ValueScale::Linear,
            PresetScale::Quadratic => ValueScale::Quadratic,
            PresetScale::Cubic => ValueScale::Cubic,
            PresetScale::Quartic => ValueScale::Quartic,
            PresetScale::SquareRoot => ValueScale::SquareRoot,
            PresetScale::Exponential => ValueScale::Exponential,
        },
    }
}

/// `SynthModuleHarness` in `filter_modules.cpp`.
#[derive(Clone)]
struct SynthModuleHarness {
    module: ProcessorRef,
    num_module_inputs: usize,
    controls: Vec<ProcessorRef>,
}

impl SynthModuleHarness {
    fn create(
        module: ProcessorRef,
        module_inputs: &[usize],
        control_names: &[String],
    ) -> ProcessorRef {
        let harness = Processor::new(
            SynthModuleHarness {
                module: module.clone(),
                num_module_inputs: module_inputs.len(),
                controls: Vec::new(),
            },
            module_inputs.len() + control_names.len(),
            1,
            false,
            1,
        );
        for (i, &index) in module_inputs.iter().enumerate() {
            module.use_input(&harness.input(i), index);
        }
        module.init();

        let controls = module.get_controls();
        let controls: Vec<ProcessorRef> = control_names
            .iter()
            .map(|name| {
                controls
                    .get(name)
                    .unwrap_or_else(|| panic!("no control {name}"))
                    .clone()
            })
            .collect();
        harness.with::<SynthModuleHarness, _>(|kernel, _| kernel.controls = controls);
        harness
    }
}

impl ProcessorImpl for SynthModuleHarness {
    sas_dsp_core::processor_boilerplate!();

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.processor_set_sample_rate(sample_rate);
        self.module.set_sample_rate(sample_rate);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.processor_set_oversample_amount(oversample);
        self.module.set_oversample_amount(oversample);
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.module.hard_reset();
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        for (i, control) in self.controls.iter().enumerate() {
            control.set_value(this.input(self.num_module_inputs + i).at(0).get(0));
        }

        self.module.process(num_samples);
        let source = self.module.output_buffer(0);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            dest[i].set(source[i].get());
        }
    }
}

/// `filterControls(prefix)` in `filter_modules.cpp`.
fn filter_controls(prefix: &str) -> Vec<String> {
    [
        "on",
        "model",
        "style",
        "cutoff",
        "resonance",
        "drive",
        "blend",
        "blend_transpose",
        "keytrack",
        "mix",
        "formant_x",
        "formant_y",
        "formant_transpose",
        "formant_resonance",
        "formant_spread",
    ]
    .iter()
    .map(|name| format!("{prefix}_{name}"))
    .collect()
}

fn filters_controls() -> Vec<String> {
    let mut controls = vec![
        "filter_1_filter_input".to_string(),
        "filter_2_filter_input".to_string(),
    ];
    controls.extend(filter_controls("filter_1"));
    controls.extend(filter_controls("filter_2"));
    controls
}

fn harness_inputs(
    module_inputs: &[(&'static str, InputRate)],
    controls: &[String],
) -> Vec<(&'static str, usize, InputRate)> {
    let mut inputs: Vec<_> = module_inputs
        .iter()
        .enumerate()
        .map(|(i, &(name, rate))| (name, i, rate))
        .collect();
    for (i, name) in controls.iter().enumerate() {
        let name: &'static str = Box::leak(name.clone().into_boxed_str());
        inputs.push((name, module_inputs.len() + i, Control));
    }
    inputs
}

#[test]
fn filter_module_matches_reference() {
    set_parameter_lookup(registry_details);
    check_module(
        "filter_module",
        &ModuleDef {
            create: |params| {
                let mono = param_bool(params, "mono", false);
                let module = FilterModule::create_with("filter_1", |filter| filter.set_mono(mono));
                SynthModuleHarness::create(
                    module,
                    &[
                        FilterModule::AUDIO,
                        FilterModule::RESET,
                        FilterModule::KEYTRACK,
                        FilterModule::MIDI,
                    ],
                    &filter_controls("filter_1"),
                )
            },
            inputs: harness_inputs(
                &[
                    ("audio", Audio),
                    ("reset", Trigger),
                    ("keytrack", Control),
                    ("midi", Control),
                ],
                &filter_controls("filter_1"),
            ),
            outputs: vec![("audio", 0)],
        },
    );
}

#[test]
fn filters_module_matches_reference() {
    set_parameter_lookup(registry_details);
    check_module(
        "filters_module",
        &ModuleDef {
            create: |_| {
                SynthModuleHarness::create(
                    FiltersModule::create(),
                    &[
                        FiltersModule::FILTER_1_INPUT,
                        FiltersModule::FILTER_2_INPUT,
                        FiltersModule::KEYTRACK,
                        FiltersModule::MIDI,
                        FiltersModule::RESET,
                    ],
                    &filters_controls(),
                )
            },
            inputs: harness_inputs(
                &[
                    ("filter_1_input", Audio),
                    ("filter_2_input", Audio),
                    ("keytrack", Control),
                    ("midi", Control),
                    ("reset", Trigger),
                ],
                &filters_controls(),
            ),
            outputs: vec![("audio", 0)],
        },
    );
}
