// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity against the C++ reference for `effects/distortion`
//! (`migration/reference/src/modules/effects/distortion.cpp`). Run with
//! `--nocapture` for per-case metrics.

mod common;

use std::collections::BTreeMap;

use common::{check_modules, def, ModuleDef};
use sas_dsp_core::testing::InputRate::{Audio, Control};
use sas_effects::Distortion;

fn modules() -> BTreeMap<&'static str, ModuleDef> {
    let mut m = BTreeMap::new();
    m.insert(
        "distortion",
        def(
            |_| Distortion::create(),
            vec![
                ("audio", Distortion::AUDIO, Audio),
                ("type", Distortion::TYPE, Control),
                ("drive", Distortion::DRIVE, Audio),
            ],
            &[
                ("audio", Distortion::AUDIO_OUT),
                ("drive", Distortion::DRIVE_OUT),
            ],
        ),
    );
    m
}

#[test]
fn distortion_matches_reference() {
    check_modules(&modules());
}
