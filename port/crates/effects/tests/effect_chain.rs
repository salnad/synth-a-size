// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity for `ReorderableEffectChain` (and `FilterFxModule` inside it)
//! against `effects/reorderable_effect_chain`. The harness mirrors
//! `migration/reference/src/modules/effects/reorderable_effect_chain.cpp`.

mod common;

use std::collections::BTreeMap;
use std::rc::Rc;

use common::{check_modules, def, ModuleDef};
use sas_dsp_core::output::Output;
use sas_dsp_core::processor::{Processor, ProcessorImpl, ProcessorRef};
use sas_dsp_core::testing::InputRate;
use sas_dsp_core::Value;
use sas_effects::ReorderableEffectChain;

/// `kControls` of the ref-module: name and default (the parameter default).
const CONTROLS: &[(&str, f32)] = &[
    ("chorus_on", 0.0),
    ("compressor_on", 0.0),
    ("delay_on", 0.0),
    ("distortion_on", 0.0),
    ("eq_on", 0.0),
    ("filter_fx_on", 0.0),
    ("flanger_on", 0.0),
    ("phaser_on", 0.0),
    ("reverb_on", 0.0),
    ("filter_fx_mix", 1.0),
    ("filter_fx_cutoff", 60.0),
    ("filter_fx_resonance", 0.5),
    ("filter_fx_drive", 0.0),
    ("filter_fx_blend", 0.0),
    ("filter_fx_style", 0.0),
    ("filter_fx_model", 0.0),
    ("filter_fx_blend_transpose", 42.0),
    ("filter_fx_keytrack", 0.0),
    ("filter_fx_formant_x", 0.5),
    ("filter_fx_formant_y", 0.5),
    ("filter_fx_formant_transpose", 0.0),
    ("filter_fx_formant_resonance", 0.85),
    ("filter_fx_formant_spread", 0.0),
];

#[derive(Clone)]
struct ChainHarness {
    order: ProcessorRef,
    beats_per_second: ProcessorRef,
    keytrack: ProcessorRef,
    chain: ProcessorRef,
    controls: Vec<ProcessorRef>,
}

impl ChainHarness {
    const AUDIO: usize = 0;
    const ORDER: usize = 1;
    const BEATS_PER_SECOND: usize = 2;
    const KEYTRACK: usize = 3;
    const FIRST_CONTROL: usize = 4;

    fn create() -> ProcessorRef {
        let order = Value::create_control_rate(0.0);
        let beats_per_second = Value::create_control_rate(2.0);
        let keytrack = Value::create_control_rate(0.0);
        let chain =
            ReorderableEffectChain::create(&beats_per_second.output(0), &keytrack.output(0));
        chain.plug(order.output(0), ReorderableEffectChain::ORDER);
        chain.init();

        let all = chain.get_controls();
        let controls = CONTROLS
            .iter()
            .map(|(name, _)| {
                all.get(*name)
                    .unwrap_or_else(|| panic!("chain has no control {name}"))
                    .clone()
            })
            .collect();
        Processor::new(
            ChainHarness {
                order,
                beats_per_second,
                keytrack,
                chain,
                controls,
            },
            Self::FIRST_CONTROL + CONTROLS.len(),
            1,
            false,
            1,
        )
    }

    fn inputs() -> Vec<(&'static str, usize, InputRate)> {
        let mut inputs = vec![
            ("audio", Self::AUDIO, InputRate::Audio),
            ("order", Self::ORDER, InputRate::Control),
            (
                "beats_per_second",
                Self::BEATS_PER_SECOND,
                InputRate::Control,
            ),
            ("keytrack", Self::KEYTRACK, InputRate::Control),
        ];
        for (i, (name, _)) in CONTROLS.iter().enumerate() {
            inputs.push((*name, Self::FIRST_CONTROL + i, InputRate::Control));
        }
        inputs
    }
}

impl ProcessorImpl for ChainHarness {
    sas_dsp_core::processor_boilerplate!();

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.processor_set_sample_rate(sample_rate);
        self.chain.set_sample_rate(sample_rate);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.processor_set_oversample_amount(oversample);
        self.chain.set_oversample_amount(oversample);
    }

    fn hard_reset(&mut self, _this: &Processor) {
        self.chain.hard_reset();
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        self.beats_per_second
            .set_value(this.input(Self::BEATS_PER_SECOND).at(0));
        self.keytrack.set_value(this.input(Self::KEYTRACK).at(0));
        self.order.set_value(this.input(Self::ORDER).at(0));
        for (i, control) in self.controls.iter().enumerate() {
            control.set_value(this.input(Self::FIRST_CONTROL + i).at(0).get(0));
        }

        let audio_in = this.input_buffer(Self::AUDIO);
        self.chain.process_with_input(&audio_in, num_samples);
        let result = self.chain.output(0).buffer();
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            dest[i].set(result[i].get());
        }
    }
}

fn modules() -> BTreeMap<&'static str, ModuleDef> {
    let mut m = BTreeMap::new();
    m.insert(
        "reorderable_effect_chain",
        def(
            |_| ChainHarness::create(),
            ChainHarness::inputs(),
            &[("audio", 0)],
        ),
    );
    m
}

#[test]
fn effect_chain_matches_reference() {
    check_modules(&modules());
}

#[test]
fn every_effect_has_an_on_control_and_the_eq_memory_is_exposed() {
    common::params::install();
    let beats_per_second: Rc<Output> = Value::create_control_rate(2.0).output(0);
    let keytrack: Rc<Output> = Value::create_control_rate(0.0).output(0);
    let chain = ReorderableEffectChain::create(&beats_per_second, &keytrack);
    chain.init();
    let controls = chain.get_controls();
    for name in sas_effects::EFFECT_ORDER {
        assert!(controls.contains_key(&format!("{name}_on")), "{name}_on");
    }
    assert!(
        chain.with::<ReorderableEffectChain, _>(|chain, _| chain.get_equalizer_memory().is_some())
    );
}
