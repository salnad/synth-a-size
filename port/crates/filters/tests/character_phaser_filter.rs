// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `PhaserFilter` against the C++ reference for every case of
//! `migration/corpus/modules/filters/phaser_filter.json` (ref-module
//! `phaser_filter`, `migration/reference/src/modules/filters/phaser_filter.cpp`).

mod support;

use sas_filters::character::PhaserFilter;
use support::{check_file, param_bool, synth_filter_inputs, ModuleDef};

/// `filters/phaser_filter.cpp`: module "phaser_filter".
fn phaser_filter_module() -> ModuleDef {
    ModuleDef {
        create: |params| PhaserFilter::create(param_bool(params, "clean", false)),
        inputs: synth_filter_inputs(),
        outputs: vec![("audio", 0)],
    }
}

#[test]
fn phaser_filter_matches_reference() {
    check_file("phaser_filter", &phaser_filter_module());
}
