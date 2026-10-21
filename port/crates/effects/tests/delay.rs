// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity against the C++ reference for the delay family:
//! `effects/stereo_delay`, `effects/multi_delay` and `effects/delay_module`
//! (`migration/reference/src/modules/effects/{stereo_delay,multi_delay,delay_module}.cpp`).
//! Run with `--nocapture` for per-case metrics.

mod common;

use std::collections::BTreeMap;

use common::harness::EffectModuleHarness;
use common::{check_modules, def, param_i32, ModuleDef};
use sas_dsp_core::testing::InputRate::{Audio, Control};
use sas_effects::{DelayModule, MultiDelay, StereoDelay};

const DELAY_MODULE_CONTROLS: &[(&str, f32)] = &[
    ("delay_frequency", 2.0),
    ("delay_sync", 1.0),
    ("delay_tempo", 9.0),
    ("delay_aux_frequency", 2.0),
    ("delay_aux_sync", 1.0),
    ("delay_aux_tempo", 9.0),
    ("delay_feedback", 0.5),
    ("delay_dry_wet", 0.3334),
    ("delay_filter_cutoff", 60.0),
    ("delay_filter_spread", 1.0),
    ("delay_style", 0.0),
];

macro_rules! delay_inputs {
    ($ty:ty) => {
        vec![
            ("audio", <$ty>::AUDIO, Audio),
            ("wet", <$ty>::WET, Control),
            ("frequency", <$ty>::FREQUENCY, Control),
            ("frequency_aux", <$ty>::FREQUENCY_AUX, Control),
            ("feedback", <$ty>::FEEDBACK, Control),
            ("damping", <$ty>::DAMPING, Control),
            ("style", <$ty>::STYLE, Control),
            ("filter_cutoff", <$ty>::FILTER_CUTOFF, Control),
            ("filter_spread", <$ty>::FILTER_SPREAD, Control),
        ]
    };
}

fn modules() -> BTreeMap<&'static str, ModuleDef> {
    let mut m = BTreeMap::new();
    m.insert(
        "stereo_delay",
        def(
            |p| StereoDelay::create(param_i32(p, "size", 88200)),
            delay_inputs!(StereoDelay),
            &[("audio", 0)],
        ),
    );
    m.insert(
        "multi_delay",
        def(
            |p| MultiDelay::create(param_i32(p, "size", 3841)),
            delay_inputs!(MultiDelay),
            &[("audio", 0)],
        ),
    );
    m.insert(
        "delay_module",
        def(
            |_| EffectModuleHarness::create(DelayModule::create, DELAY_MODULE_CONTROLS),
            EffectModuleHarness::inputs(DELAY_MODULE_CONTROLS),
            &[("audio", 0)],
        ),
    );
    m
}

#[test]
fn delay_family_matches_reference() {
    check_modules(&modules());
}
