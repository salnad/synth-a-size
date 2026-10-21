// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity against the C++ reference for `effects/phaser` and
//! `effects/phaser_module`
//! (`migration/reference/src/modules/effects/{phaser,phaser_module}.cpp`).
//! Run with `--nocapture` for per-case metrics.

mod common;

use std::collections::BTreeMap;

use common::harness::EffectModuleHarness;
use common::{check_modules, def, ModuleDef};
use sas_dsp_core::testing::InputRate::{Audio, Control};
use sas_effects::{Phaser, PhaserModule};

const PHASER_MODULE_CONTROLS: &[(&str, f32)] = &[
    ("phaser_frequency", -3.0),
    ("phaser_sync", 1.0),
    ("phaser_tempo", 3.0),
    ("phaser_feedback", 0.5),
    ("phaser_dry_wet", 1.0),
    ("phaser_center", 80.0),
    ("phaser_mod_depth", 24.0),
    ("phaser_phase_offset", 0.333_333_33),
    ("phaser_blend", 1.0),
];

fn modules() -> BTreeMap<&'static str, ModuleDef> {
    let mut m = BTreeMap::new();
    m.insert(
        "phaser",
        def(
            |_| {
                let phaser = Phaser::create();
                phaser.init();
                phaser
            },
            vec![
                ("audio", Phaser::AUDIO, Audio),
                ("mix", Phaser::MIX, Control),
                ("rate", Phaser::RATE, Control),
                ("feedback_gain", Phaser::FEEDBACK_GAIN, Control),
                ("center", Phaser::CENTER, Audio),
                ("mod_depth", Phaser::MOD_DEPTH, Control),
                ("phase_offset", Phaser::PHASE_OFFSET, Control),
                ("blend", Phaser::BLEND, Control),
            ],
            &[
                ("audio", Phaser::AUDIO_OUTPUT),
                ("cutoff", Phaser::CUTOFF_OUTPUT),
            ],
        ),
    );
    m.insert(
        "phaser_module",
        def(
            |_| EffectModuleHarness::create(PhaserModule::create, PHASER_MODULE_CONTROLS),
            EffectModuleHarness::inputs(PHASER_MODULE_CONTROLS),
            &[("audio", 0)],
        ),
    );
    m
}

#[test]
fn phaser_family_matches_reference() {
    check_modules(&modules());
}
