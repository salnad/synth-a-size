// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity against the C++ reference for `effects/multiband_compressor` and
//! `effects/compressor_module`
//! (`migration/reference/src/modules/effects/{multiband_compressor,compressor_module}.cpp`).
//! Run with `--nocapture` for per-case metrics.

mod common;

use std::collections::BTreeMap;

use common::harness::EffectModuleHarness;
use common::{check_modules, def, ModuleDef};
use sas_dsp_core::testing::InputRate::{Audio, Control};
use sas_effects::{CompressorModule, MultibandCompressor};

const COMPRESSOR_MODULE_CONTROLS: &[(&str, f32)] = &[
    ("compressor_attack", 0.5),
    ("compressor_release", 0.5),
    ("compressor_low_gain", 16.3),
    ("compressor_band_gain", 11.7),
    ("compressor_high_gain", 16.3),
    ("compressor_enabled_bands", 0.0),
    ("compressor_low_upper_ratio", 0.9),
    ("compressor_band_upper_ratio", 0.857),
    ("compressor_high_upper_ratio", 1.0),
    ("compressor_low_lower_ratio", 0.8),
    ("compressor_band_lower_ratio", 0.8),
    ("compressor_high_lower_ratio", 0.8),
    ("compressor_low_upper_threshold", -28.0),
    ("compressor_band_upper_threshold", -25.0),
    ("compressor_high_upper_threshold", -30.0),
    ("compressor_low_lower_threshold", -35.0),
    ("compressor_band_lower_threshold", -36.0),
    ("compressor_high_lower_threshold", -35.0),
    ("compressor_mix", 1.0),
];

fn modules() -> BTreeMap<&'static str, ModuleDef> {
    type M = MultibandCompressor;
    let mut m = BTreeMap::new();
    m.insert(
        "multiband_compressor",
        def(
            |_| M::create(),
            vec![
                ("audio", M::AUDIO, Audio),
                ("low_upper_ratio", M::LOW_UPPER_RATIO, Control),
                ("band_upper_ratio", M::BAND_UPPER_RATIO, Control),
                ("high_upper_ratio", M::HIGH_UPPER_RATIO, Control),
                ("low_lower_ratio", M::LOW_LOWER_RATIO, Control),
                ("band_lower_ratio", M::BAND_LOWER_RATIO, Control),
                ("high_lower_ratio", M::HIGH_LOWER_RATIO, Control),
                ("low_upper_threshold", M::LOW_UPPER_THRESHOLD, Control),
                ("band_upper_threshold", M::BAND_UPPER_THRESHOLD, Control),
                ("high_upper_threshold", M::HIGH_UPPER_THRESHOLD, Control),
                ("low_lower_threshold", M::LOW_LOWER_THRESHOLD, Control),
                ("band_lower_threshold", M::BAND_LOWER_THRESHOLD, Control),
                ("high_lower_threshold", M::HIGH_LOWER_THRESHOLD, Control),
                ("low_output_gain", M::LOW_OUTPUT_GAIN, Control),
                ("band_output_gain", M::BAND_OUTPUT_GAIN, Control),
                ("high_output_gain", M::HIGH_OUTPUT_GAIN, Control),
                ("attack", M::ATTACK, Control),
                ("release", M::RELEASE, Control),
                ("enabled_bands", M::ENABLED_BANDS, Control),
                ("mix", M::MIX, Control),
            ],
            &[
                ("audio", M::AUDIO_OUT),
                ("low_input_ms", M::LOW_INPUT_MEAN_SQUARED),
                ("band_input_ms", M::BAND_INPUT_MEAN_SQUARED),
                ("high_input_ms", M::HIGH_INPUT_MEAN_SQUARED),
                ("low_output_ms", M::LOW_OUTPUT_MEAN_SQUARED),
                ("band_output_ms", M::BAND_OUTPUT_MEAN_SQUARED),
                ("high_output_ms", M::HIGH_OUTPUT_MEAN_SQUARED),
            ],
        ),
    );
    m.insert(
        "compressor_module",
        def(
            |_| {
                EffectModuleHarness::create(
                    |_| CompressorModule::create(),
                    COMPRESSOR_MODULE_CONTROLS,
                )
            },
            EffectModuleHarness::inputs(COMPRESSOR_MODULE_CONTROLS),
            &[("audio", 0)],
        ),
    );
    m
}

#[test]
fn compressor_family_matches_reference() {
    check_modules(&modules());
}
