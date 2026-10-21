// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `FormantFilter` (with `FormantManager`) and `FormantModule`
//! (with `VocalTract`) against the C++ reference for every case of
//! `migration/corpus/modules/filters/formant_filter.json` and
//! `formant_module.json` (ref-modules in
//! `migration/reference/src/modules/filters/formant_{filter,module}.cpp`).

mod support;

use sas_dsp_core::synth_module::{set_parameter_lookup, ParameterDetails, ValueScale};
use sas_dsp_core::Processor;
use sas_filters::character::{FormantFilter, FormantModule};
use sas_filters::synth_filter::input;
use serde_json::Value as Json;
use support::{check_file, param_f32, Audio, Control, ModuleDef, Trigger};

/// `filters/formant_filter.cpp`: module "formant_filter" (initialised by
/// the factory).
fn formant_filter_module() -> ModuleDef {
    ModuleDef {
        create: |params| {
            let filter = FormantFilter::create(param_f32(params, "style", 0.0) as usize);
            filter.init();
            filter
        },
        inputs: vec![
            ("audio", input::AUDIO, Audio),
            ("reset", input::RESET, Trigger),
            ("resonance", input::RESONANCE, Control),
            ("interpolate_x", input::INTERPOLATE_X, Audio),
            ("interpolate_y", input::INTERPOLATE_Y, Audio),
            ("transpose", input::TRANSPOSE, Audio),
            ("spread", input::SPREAD, Control),
        ],
        outputs: vec![("audio", 0)],
    }
}

/// The `filter_1_formant_*` entries of upstream's parameter registry
/// (`filter_parameter_list`, `vital/src/common/synth_parameters.cpp`), all
/// linear: min, max, default.
fn formant_parameter_details(name: &str) -> ParameterDetails {
    let (min, max, default_value) = match name {
        "filter_1_formant_x" => (0.0, 1.0, 0.5),
        "filter_1_formant_y" => (0.0, 1.0, 0.5),
        "filter_1_formant_transpose" => (-12.0, 12.0, 0.0),
        "filter_1_formant_resonance" => (0.3, 1.0, 0.85),
        "filter_1_formant_spread" => (-1.0, 1.0, 0.0),
        _ => panic!("parameter {name} is not part of FormantModule"),
    };
    ParameterDetails {
        min,
        max,
        default_value,
        post_offset: 0.0,
        value_scale: ValueScale::Linear,
    }
}

fn set_controls(module: &Processor, params: &Json) {
    let Some(controls) = params.get("controls").and_then(Json::as_object) else {
        return;
    };
    let all = module.get_controls();
    for (name, value) in controls {
        let control = all
            .get(name)
            .unwrap_or_else(|| panic!("unknown control {name}"));
        control.set_value(value.as_f64().expect("numeric control value") as f32);
    }
}

/// `filters/formant_module.cpp`: module "formant_module"
/// (`FormantModule("filter_1")`, initialised, controls set from
/// `params.controls`).
fn formant_module_module() -> ModuleDef {
    ModuleDef {
        create: |params| {
            let module = FormantModule::create("filter_1");
            module.init();
            set_controls(&module, params);
            module
        },
        inputs: vec![
            ("audio", FormantModule::AUDIO, Audio),
            ("reset", FormantModule::RESET, Trigger),
            ("resonance", FormantModule::RESONANCE, Control),
            ("blend", FormantModule::BLEND, Control),
            ("style", FormantModule::STYLE, Control),
        ],
        outputs: vec![("audio", 0)],
    }
}

#[test]
fn formant_filter_matches_reference() {
    check_file("formant_filter", &formant_filter_module());
}

#[test]
fn formant_module_matches_reference() {
    set_parameter_lookup(formant_parameter_details);
    check_file("formant_module", &formant_module_module());
}
