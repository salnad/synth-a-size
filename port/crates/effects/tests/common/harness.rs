// SPDX-License-Identifier: GPL-3.0-or-later

//! Mirror of `migration/reference/src/modules/effects/effect_module_harness.h`:
//! drives one effect module the way the engine does (controls set once per
//! block, `on` toggles `enable()` as `ReorderableEffectChain` does, a
//! disabled effect passes its input through).

use std::rc::Rc;

use sas_dsp_core::output::Output;
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::testing::InputRate;
use sas_dsp_core::Value;

#[derive(Clone)]
pub struct EffectModuleHarness {
    beats_per_second: ProcessorRef,
    module: ProcessorRef,
    controls: Vec<ProcessorRef>,
}

impl EffectModuleHarness {
    pub const AUDIO: usize = 0;
    pub const ON: usize = 1;
    pub const BEATS_PER_SECOND: usize = 2;
    pub const FIRST_CONTROL: usize = 3;

    pub fn create(
        create: impl FnOnce(&Rc<Output>) -> ProcessorRef,
        control_names: &[(&str, f32)],
    ) -> ProcessorRef {
        let beats_per_second = Value::create_control_rate(2.0);
        let module = create(&beats_per_second.output(0));
        module.init();
        let all = module.get_controls();
        let controls = control_names
            .iter()
            .map(|(name, _)| {
                all.get(*name)
                    .unwrap_or_else(|| panic!("module has no control {name}"))
                    .clone()
            })
            .collect();
        Processor::new(
            EffectModuleHarness {
                beats_per_second,
                module,
                controls,
            },
            Self::FIRST_CONTROL + control_names.len(),
            1,
            false,
            1,
        )
    }

    /// `harnessInputs(controls)`: name, input index and rate.
    pub fn inputs(control_names: &[(&'static str, f32)]) -> Vec<(&'static str, usize, InputRate)> {
        let mut inputs = vec![
            ("audio", Self::AUDIO, InputRate::Audio),
            ("on", Self::ON, InputRate::Control),
            (
                "beats_per_second",
                Self::BEATS_PER_SECOND,
                InputRate::Control,
            ),
        ];
        for (i, (name, _)) in control_names.iter().enumerate() {
            inputs.push((*name, Self::FIRST_CONTROL + i, InputRate::Control));
        }
        inputs
    }
}

impl ProcessorImpl for EffectModuleHarness {
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
        self.beats_per_second
            .set_value(this.input(Self::BEATS_PER_SECOND).at(0));
        for (i, control) in self.controls.iter().enumerate() {
            control.set_value(this.input(Self::FIRST_CONTROL + i).at(0).get(0));
        }

        let audio_in = this.input_buffer(Self::AUDIO);
        let on = this.input(Self::ON).at(0).get(0) != 0.0;
        if on != self.module.enabled() {
            self.module.enable(on);
        }

        let result = if on {
            self.module.process_with_input(&audio_in, num_samples);
            self.module.output(0).buffer()
        } else {
            audio_in
        };
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            dest[i].set(result[i].get());
        }
    }
}
