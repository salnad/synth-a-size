// SPDX-License-Identifier: GPL-3.0-or-later

//! "Resynthesize Preset to Wavetable" renders on a thread with a seed counter
//! of its own, so building its synth and its file source draws nothing from
//! the process's counter (which would change the seeds of every synth and
//! wavetable built later in the session).
//!
//! The seed counter is process-wide, so this runs alone in its own binary.

use std::sync::Arc;

use sas_engine::document::PresetDocument;
use sas_engine::edit_bus::EditBus;
use sas_engine::keyboard::keyboard;
use sas_engine::telemetry::Telemetry;
use sas_gui::EditorModel;
use sas_plugin::editor::PluginModel;
use sas_plugin::midi_learn::MidiLearn;
use sas_plugin::params::SynthParams;
use sas_preset::InitContent;

/// The seed counter's next value, without consuming it.
fn peek_next_seed() -> i32 {
    let next = sas_common::random::take_next_seed();
    sas_common::random::set_next_seed(next);
    next
}

#[test]
fn resynthesis_leaves_the_process_seed_counter_alone() {
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

    let next_seed = peek_next_seed();
    let revision = model.content_revision();
    model.resynthesize_to_wavetable(1);
    model.resynthesize_to_wavetable(2);
    model.wait_for_resynthesis();

    assert_eq!(model.wavetable_name(1), "Resynthesize");
    assert_eq!(model.wavetable_name(2), "Resynthesize");
    assert_eq!(model.wavetable_name(0), "Init");
    assert!(model.content_revision() > revision);
    // Each resynthesis restarts its thread's counter, so both oscillators
    // got the same table.
    assert_eq!(model.wavetable_json(1), model.wavetable_json(2));
    assert_eq!(
        peek_next_seed(),
        next_seed,
        "a resynthesis drew a seed from the process's counter"
    );
}
