// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity against the C++ reference for `effects/equalizer_module` and
//! `effects/distortion_module` (the `DigitalSvf`-based effect modules,
//! `migration/reference/src/modules/effects/{equalizer,distortion}_module.cpp`).
//! Run with `--nocapture` for per-case metrics.

mod common;

use std::collections::BTreeMap;

use common::harness::EffectModuleHarness;
use common::{check_modules, def, ModuleDef};
use sas_effects::{DistortionModule, EqualizerModule};

const EQUALIZER_MODULE_CONTROLS: &[(&str, f32)] = &[
    ("eq_low_mode", 0.0),
    ("eq_low_cutoff", 40.0),
    ("eq_low_gain", 0.0),
    ("eq_low_resonance", 0.3163),
    ("eq_band_mode", 0.0),
    ("eq_band_cutoff", 80.0),
    ("eq_band_gain", 0.0),
    ("eq_band_resonance", 0.4473),
    ("eq_high_mode", 0.0),
    ("eq_high_cutoff", 100.0),
    ("eq_high_gain", 0.0),
    ("eq_high_resonance", 0.3163),
];

const DISTORTION_MODULE_CONTROLS: &[(&str, f32)] = &[
    ("distortion_type", 0.0),
    ("distortion_drive", 0.0),
    ("distortion_mix", 1.0),
    ("distortion_filter_order", 0.0),
    ("distortion_filter_cutoff", 80.0),
    ("distortion_filter_resonance", 0.5),
    ("distortion_filter_blend", 0.0),
];

fn modules() -> BTreeMap<&'static str, ModuleDef> {
    let mut m = BTreeMap::new();
    m.insert(
        "equalizer_module",
        def(
            |_| {
                EffectModuleHarness::create(
                    |_| EqualizerModule::create(),
                    EQUALIZER_MODULE_CONTROLS,
                )
            },
            EffectModuleHarness::inputs(EQUALIZER_MODULE_CONTROLS),
            &[("audio", 0)],
        ),
    );
    m.insert(
        "distortion_module",
        def(
            |_| {
                EffectModuleHarness::create(
                    |_| DistortionModule::create(),
                    DISTORTION_MODULE_CONTROLS,
                )
            },
            EffectModuleHarness::inputs(DISTORTION_MODULE_CONTROLS),
            &[("audio", 0)],
        ),
    );
    m
}

#[test]
fn equalizer_and_distortion_modules_match_reference() {
    check_modules(&modules());
}
