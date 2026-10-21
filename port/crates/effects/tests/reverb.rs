// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity against the C++ reference for `effects/reverb` and
//! `effects/reverb_module`
//! (`migration/reference/src/modules/effects/{reverb,reverb_module}.cpp`).
//! Run with `--nocapture` for per-case metrics.

mod common;

use std::collections::BTreeMap;

use common::harness::EffectModuleHarness;
use common::{check_modules, def, ModuleDef};
use sas_dsp_core::testing::InputRate::{Audio, Control};
use sas_effects::{Reverb, ReverbModule};

const REVERB_MODULE_CONTROLS: &[(&str, f32)] = &[
    ("reverb_decay_time", 0.0),
    ("reverb_pre_low_cutoff", 0.0),
    ("reverb_pre_high_cutoff", 110.0),
    ("reverb_low_shelf_cutoff", 0.0),
    ("reverb_low_shelf_gain", 0.0),
    ("reverb_high_shelf_cutoff", 90.0),
    ("reverb_high_shelf_gain", -1.0),
    ("reverb_chorus_amount", 0.223607),
    ("reverb_chorus_frequency", -2.0),
    ("reverb_size", 0.5),
    ("reverb_delay", 0.0),
    ("reverb_dry_wet", 0.25),
];

fn modules() -> BTreeMap<&'static str, ModuleDef> {
    let mut m = BTreeMap::new();
    m.insert(
        "reverb",
        def(
            |_| Reverb::create(),
            vec![
                ("audio", Reverb::AUDIO, Audio),
                ("decay_time", Reverb::DECAY_TIME, Control),
                ("pre_low_cutoff", Reverb::PRE_LOW_CUTOFF, Control),
                ("pre_high_cutoff", Reverb::PRE_HIGH_CUTOFF, Control),
                ("low_cutoff", Reverb::LOW_CUTOFF, Control),
                ("low_gain", Reverb::LOW_GAIN, Control),
                ("high_cutoff", Reverb::HIGH_CUTOFF, Control),
                ("high_gain", Reverb::HIGH_GAIN, Control),
                ("chorus_amount", Reverb::CHORUS_AMOUNT, Control),
                ("chorus_frequency", Reverb::CHORUS_FREQUENCY, Control),
                ("size", Reverb::SIZE, Control),
                ("delay", Reverb::DELAY, Control),
                ("wet", Reverb::WET, Control),
            ],
            &[("audio", 0)],
        ),
    );
    m.insert(
        "reverb_module",
        def(
            |_| EffectModuleHarness::create(|_| ReverbModule::create(), REVERB_MODULE_CONTROLS),
            EffectModuleHarness::inputs(REVERB_MODULE_CONTROLS),
            &[("audio", 0)],
        ),
    );
    m
}

#[test]
fn reverb_family_matches_reference() {
    check_modules(&modules());
}
