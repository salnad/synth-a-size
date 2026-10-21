// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `CombFilter` and `CombModule` against the C++ reference for
//! every case of `migration/corpus/modules/filters/comb_filter.json` and
//! `comb_module.json` (ref-modules in
//! `migration/reference/src/modules/filters/comb_{filter,module}.cpp`).

mod support;

use sas_filters::character::{CombFilter, CombModule};
use support::{check_file, param_f32, synth_filter_inputs, Audio, Control, ModuleDef, Trigger};

/// `filters/comb_filter.cpp`: module "comb_filter".
fn comb_filter_module() -> ModuleDef {
    ModuleDef {
        create: |params| {
            let size = param_f32(params, "size", CombModule::MAX_FEEDBACK_SAMPLES as f32);
            CombFilter::create(size as i32)
        },
        inputs: synth_filter_inputs(),
        outputs: vec![("audio", 0)],
    }
}

/// `filters/comb_module.cpp`: module "comb_module" (initialised by the
/// factory).
fn comb_module_module() -> ModuleDef {
    ModuleDef {
        create: |_| {
            let module = CombModule::create();
            module.init();
            module
        },
        inputs: vec![
            ("audio", CombModule::AUDIO, Audio),
            ("reset", CombModule::RESET, Trigger),
            ("midi_cutoff", CombModule::MIDI_CUTOFF, Audio),
            (
                "midi_blend_transpose",
                CombModule::MIDI_BLEND_TRANSPOSE,
                Control,
            ),
            (
                "filter_cutoff_blend",
                CombModule::FILTER_CUTOFF_BLEND,
                Control,
            ),
            ("style", CombModule::STYLE, Control),
            ("resonance", CombModule::RESONANCE, Control),
            ("midi", CombModule::MIDI, Control),
        ],
        outputs: vec![("audio", 0)],
    }
}

#[test]
fn comb_filter_matches_reference() {
    check_file("comb_filter", &comb_filter_module());
}

#[test]
fn comb_module_matches_reference() {
    check_file("comb_module", &comb_module_module());
}
