// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor's line shapes through the plugin's model: a modulation
//! slot's remap curve and an LFO's shape read back as they were set.

use std::sync::Arc;

use sas_common::line_generator::LineGenerator;
use sas_engine::document::PresetDocument;
use sas_engine::edit_bus::EditBus;
use sas_engine::keyboard::keyboard;
use sas_engine::telemetry::Telemetry;
use sas_gui::EditorModel;
use sas_plugin::editor::PluginModel;
use sas_plugin::midi_learn::MidiLearn;
use sas_plugin::params::SynthParams;
use sas_preset::InitContent;

#[test]
fn remap_curves_and_lfo_shapes_read_back_as_set() {
    let (edits, _consumer) = EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
    let params = Arc::new(SynthParams::new(edits));
    let (_writer, telemetry) = Telemetry::new(Arc::default());
    let model = PluginModel::new(
        params,
        telemetry,
        keyboard().0,
        Arc::new(MidiLearn::in_memory()),
        None,
    );

    let linear = model.line_mapping(3).expect("every slot has a curve");
    assert_eq!(linear.num_points(), 2);
    let mut curve = LineGenerator::new(LineGenerator::DEFAULT_RESOLUTION);
    curve.init_saw_up();
    curve.render();
    assert!(model.set_line_mapping(3, curve.clone()));
    assert_eq!(
        model.line_mapping(3).unwrap().to_json().dump(),
        curve.to_json().dump()
    );
    assert_eq!(
        model.line_mapping(4).unwrap().to_json().dump(),
        linear.to_json().dump()
    );

    let mut shape = LineGenerator::new(LineGenerator::DEFAULT_RESOLUTION);
    shape.init_square();
    shape.render();
    assert!(model.set_lfo_shape(2, shape.clone()));
    assert_eq!(
        model.lfo_shape(2).unwrap().to_json().dump(),
        shape.to_json().dump()
    );
    assert!(model.line_mapping(64).is_none());
}
