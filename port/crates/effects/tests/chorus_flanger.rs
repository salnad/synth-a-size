// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity against the C++ reference for `effects/chorus_module` and
//! `effects/flanger_module`
//! (`migration/reference/src/modules/effects/{chorus,flanger}_module.cpp`).
//! Run with `--nocapture` for per-case metrics.

mod common;

use std::collections::BTreeMap;

use common::harness::EffectModuleHarness;
use common::{check_modules_with_parked, def, ModuleDef};
use sas_effects::{ChorusModule, FlangerModule};

const CHORUS_MODULE_CONTROLS: &[(&str, f32)] = &[
    ("chorus_voices", 4.0),
    ("chorus_frequency", -3.0),
    ("chorus_sync", 1.0),
    ("chorus_tempo", 4.0),
    ("chorus_feedback", 0.4),
    ("chorus_dry_wet", 0.5),
    ("chorus_cutoff", 60.0),
    ("chorus_spread", 1.0),
    ("chorus_mod_depth", 0.5),
    ("chorus_delay_1", -9.0),
    ("chorus_delay_2", -7.0),
];

const FLANGER_MODULE_CONTROLS: &[(&str, f32)] = &[
    ("flanger_frequency", 2.0),
    ("flanger_sync", 1.0),
    ("flanger_tempo", 4.0),
    ("flanger_center", 64.0),
    ("flanger_feedback", 0.5),
    ("flanger_dry_wet", 0.5),
    ("flanger_mod_depth", 0.5),
    ("flanger_phase_offset", 0.333_333_33),
];

fn modules() -> BTreeMap<&'static str, ModuleDef> {
    let mut m = BTreeMap::new();
    m.insert(
        "chorus_module",
        def(
            |_| EffectModuleHarness::create(ChorusModule::create, CHORUS_MODULE_CONTROLS),
            EffectModuleHarness::inputs(CHORUS_MODULE_CONTROLS),
            &[("audio", 0)],
        ),
    );
    m.insert(
        "flanger_module",
        def(
            |_| EffectModuleHarness::create(FlangerModule::create, FLANGER_MODULE_CONTROLS),
            EffectModuleHarness::inputs(FLANGER_MODULE_CONTROLS),
            &[("audio", 0)],
        ),
    );
    m
}

/// No module is fully parked: the full-range chorus and flanger cases are in
/// `migration/corpus/modules/effects/pending/` and the loaded chorus cases are
/// interim (LEDGER.md, M6 PR 4).
const PARKED: &[&str] = &[];

#[test]
fn chorus_and_flanger_match_reference() {
    check_modules_with_parked(&modules(), PARKED);
}
