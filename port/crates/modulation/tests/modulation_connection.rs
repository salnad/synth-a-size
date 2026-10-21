// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity of `ModulationConnectionProcessor` and `LineMap` against the C++
//! reference (`migration/corpus/modules/modulation/{modulation_connection,
//! line_map}.json`).

mod common;

use common::*;
use sas_modulation::{LineMap, ModulationConnectionProcessor};

#[test]
fn line_map_matches_reference() {
    check_module(&ModuleDef::new(
        "line_map",
        |params| LineMap::create(line_shape(params, "shape")),
        &[("phase", 0, Control)],
        &[("value", LineMap::VALUE), ("phase", LineMap::PHASE)],
    ));
}

#[test]
fn modulation_connection_matches_reference() {
    check_module(&ModuleDef::new(
        "modulation_connection",
        |params| {
            let index = params.get("index").and_then(|v| v.as_i64()).unwrap_or(0);
            let processor = ModulationConnectionProcessor::create(index as i32);
            processor.init();
            processor.with::<ModulationConnectionProcessor, _>(|kernel, _| {
                let map = kernel.line_map_generator();
                apply_line_shape(&mut map.borrow_mut(), params, "map");
            });
            set_controls(&processor, params);
            processor.with::<ModulationConnectionProcessor, _>(|kernel, _| {
                kernel.set_destination_scale(param(params, "destination_scale", 1.0))
            });
            processor.set_control_rate(param_bool(params, "control_rate", true));
            let changes: Vec<(u64, f32)> = params
                .get("destination_scale_changes")
                .and_then(|v| v.as_array())
                .map(|changes| {
                    changes
                        .iter()
                        .map(|c| {
                            (
                                c["block"].as_u64().unwrap(),
                                c["scale"].as_f64().unwrap() as f32,
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            per_block(processor, move |processor, block, _frames| {
                for &(at, scale) in &changes {
                    if at == block {
                        processor.with::<ModulationConnectionProcessor, _>(|kernel, _| {
                            kernel.set_destination_scale(scale)
                        });
                    }
                }
            })
        },
        &[
            (
                "modulation_input",
                ModulationConnectionProcessor::MODULATION_INPUT,
                Audio,
            ),
            (
                "modulation_amount",
                ModulationConnectionProcessor::MODULATION_AMOUNT,
                Control,
            ),
            (
                "modulation_power",
                ModulationConnectionProcessor::MODULATION_POWER,
                Control,
            ),
            ("reset", ModulationConnectionProcessor::RESET, Trigger),
        ],
        &[
            (
                "modulation_output",
                ModulationConnectionProcessor::MODULATION_OUTPUT,
            ),
            (
                "modulation_pre_scale",
                ModulationConnectionProcessor::MODULATION_PRE_SCALE,
            ),
            (
                "modulation_source",
                ModulationConnectionProcessor::MODULATION_SOURCE,
            ),
        ],
    ));
}
