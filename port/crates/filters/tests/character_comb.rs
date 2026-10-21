// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `CombFilter` and `CombModule` against the C++ reference for
//! every case of `migration/corpus/modules/filters/comb_filter.json` and
//! `comb_module.json` (ref-modules in
//! `migration/reference/src/modules/filters/comb_{filter,module}.cpp`).

mod character_support;

use character_support::{
    assert_all_cases_tested, assert_capture_matches_semantics, run_case, synth_filter_inputs,
    Audio, Control, InputDef, Trigger,
};
use sas_filters::character::{CombFilter, CombModule};
use sas_parity::{assert_parity, ModuleCase};

fn run_filter(case_name: &str) -> Vec<[f32; 4]> {
    let case = ModuleCase::load(&format!("filters/comb_filter/{case_name}"));
    assert_capture_matches_semantics(&case);
    let size = case
        .params()
        .get("size")
        .and_then(serde_json::Value::as_i64)
        .map_or(CombModule::MAX_FEEDBACK_SAMPLES, |size| size as i32);
    let filter = CombFilter::create(size);
    run_case(&case, &filter, &synth_filter_inputs(), 0)
}

fn comb_module_inputs() -> Vec<InputDef> {
    vec![
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
    ]
}

fn run_module(case_name: &str) -> Vec<[f32; 4]> {
    let case = ModuleCase::load(&format!("filters/comb_module/{case_name}"));
    assert_capture_matches_semantics(&case);
    let module = CombModule::create();
    module.init();
    run_case(&case, &module, &comb_module_inputs(), 0)
}

macro_rules! cases {
    ($file:literal, $run:ident, $all:ident: $($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                let output = $run(stringify!($name));
                assert_parity!(concat!($file, "/", stringify!($name)), output);
            }
        )*

        #[test]
        fn $all() {
            assert_all_cases_tested($file, &[$(stringify!($name)),*]);
        }
    };
}

mod comb_filter {
    use super::*;

    cases!("filters/comb_filter", run_filter, every_comb_filter_case_is_tested:
        comb_cutoff_sweep,
        comb_resonance_blend_ramps,
        positive_flange_sweep,
        negative_flange_sweep,
        band_spread_comb,
        band_spread_positive_flange,
        band_spread_negative_flange,
        styles_past_band_spread,
        out_of_range_controls,
        per_lane_controls_and_resets,
        impulse_ringing,
        comb_96k_block128,
        comb_oversample2,
    );
}

mod comb_module {
    use super::*;

    cases!("filters/comb_module", run_module, every_comb_module_case_is_tested:
        comb_style_sweep,
        flange_with_resets,
        band_spread_full_range,
    );
}
