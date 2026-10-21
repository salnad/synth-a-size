// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity for the fixed building blocks: `LinkwitzRileyFilter`
//! (`filters/linkwitz_riley`) and `DcFilter` (`filters/dc_filter`).

mod support;

use sas_filters::{dc_filter, linkwitz_riley_filter, DcFilter, LinkwitzRileyFilter};
use support::{check_file, param_f32, Audio, ModuleDef, Trigger};

/// `filters/linkwitz_riley.cpp`.
fn linkwitz_riley_module() -> ModuleDef {
    ModuleDef {
        create: |params| LinkwitzRileyFilter::create(param_f32(params, "cutoff", 120.0)),
        inputs: vec![("audio", linkwitz_riley_filter::AUDIO, Audio)],
        outputs: vec![
            ("low", linkwitz_riley_filter::AUDIO_LOW),
            ("high", linkwitz_riley_filter::AUDIO_HIGH),
        ],
    }
}

/// `filters/dc_filter.cpp`.
fn dc_filter_module() -> ModuleDef {
    ModuleDef {
        create: |_| DcFilter::create(),
        inputs: vec![
            ("audio", dc_filter::AUDIO, Audio),
            ("reset", dc_filter::RESET, Trigger),
        ],
        outputs: vec![("audio", 0)],
    }
}

#[test]
fn linkwitz_riley_matches_reference() {
    check_file("linkwitz_riley", &linkwitz_riley_module());
}

#[test]
fn dc_filter_matches_reference() {
    check_file("dc_filter", &dc_filter_module());
}
