// SPDX-License-Identifier: GPL-3.0-or-later

//! What the `sas-gui` editor reads from the plugin.

use std::sync::{Arc, Mutex};

use sas_engine::keyboard::KeyboardHandle;
use sas_engine::telemetry::Telemetry;
use sas_engine::Tuning;
use sas_gui::{EditorModel, HostParam};

use crate::midi_learn::MidiLearn;
use crate::params::SynthParams;

/// The editor's view of one plugin instance.
pub struct PluginModel {
    params: Arc<SynthParams>,
    keyboard: KeyboardHandle,
    telemetry: Telemetry,
    midi_learn: Arc<MidiLearn>,
    /// The preset name and the edit bus revision it was read at, so the
    /// document mirror is read only after it changes.
    preset_name: Mutex<(u64, String)>,
}

impl PluginModel {
    pub fn new(
        params: Arc<SynthParams>,
        telemetry: Telemetry,
        keyboard: KeyboardHandle,
        midi_learn: Arc<MidiLearn>,
    ) -> PluginModel {
        let edits = params.edit_handle();
        let revision = edits.revision();
        let name = edits.with_document(|document| document.state.info.preset_name.clone());
        PluginModel {
            params,
            keyboard,
            telemetry,
            midi_learn,
            preset_name: Mutex::new((revision, name)),
        }
    }
}

impl EditorModel for PluginModel {
    fn preset_name(&self) -> String {
        let edits = self.params.edit_handle();
        let revision = edits.revision();
        let mut cached = self
            .preset_name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cached.0 != revision {
            *cached = (
                revision,
                edits.with_document(|document| document.state.info.preset_name.clone()),
            );
        }
        cached.1.clone()
    }

    fn telemetry(&self) -> Option<Telemetry> {
        Some(self.telemetry.clone())
    }

    fn param(&self, name: &str) -> Option<HostParam> {
        self.params
            .control(name)
            .map(|control| control.param.as_ptr())
    }

    fn note_on(&self, note: u8, velocity: f32) {
        self.keyboard.note_on(note, velocity);
    }

    fn note_off(&self, note: u8) {
        self.keyboard.note_off(note);
    }

    fn held_notes(&self) -> u128 {
        self.keyboard.held_notes()
    }

    fn tuning_name(&self) -> String {
        self.params
            .edit_handle()
            .with_document(|document| document.tuning.name())
    }

    fn tuning(&self) -> Tuning {
        self.params
            .edit_handle()
            .with_document(|document| document.tuning.clone())
    }

    fn load_tuning(&self, tuning: Tuning) {
        // A full queue drops the load: the queue only fills while the audio
        // thread is stalled, and the name shown stays the old one.
        let _ = self.params.edit_handle().set_tuning(tuning);
    }

    fn arm_midi_learn(&self, name: &str) {
        self.midi_learn.arm(name);
    }

    fn cancel_midi_learn(&self) {
        self.midi_learn.cancel();
    }

    fn clear_midi_learn(&self, name: &str) {
        self.midi_learn.clear(name);
    }

    fn is_midi_mapped(&self, name: &str) -> bool {
        self.midi_learn.is_midi_mapped(name)
    }

    fn midi_learn_ccs(&self, name: &str) -> Vec<i32> {
        self.midi_learn.learned_ccs(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sas_engine::document::PresetDocument;
    use sas_engine::edit_bus::EditBus;
    use sas_engine::keyboard::keyboard;
    use sas_preset::InitContent;

    #[test]
    fn the_preset_name_follows_renames() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            params.clone(),
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
        );
        assert_eq!(model.preset_name(), "");
        let edits = params.edit_handle();
        let mut info = edits.with_document(|document| document.state.info.clone());
        info.preset_name = "Renamed".to_string();
        edits.set_info(info);
        assert_eq!(model.preset_name(), "Renamed");
    }

    #[test]
    fn a_loaded_tuning_names_the_tuning() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            Arc::new(SynthParams::new(edits)),
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
        );
        let default_name = model.tuning_name();
        let mut tuning = Tuning::new();
        tuning.set_name("Septimal");
        model.load_tuning(tuning);
        assert_eq!(model.tuning_name(), "Septimal");
        assert_ne!(default_name, "Septimal");
    }

    /// `SynthBase::loadTuningFile` loads into the playing tuning, so a
    /// keyboard map loaded after a scale keeps the scale.
    #[test]
    fn a_keyboard_map_layers_on_the_loaded_scale() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            Arc::new(SynthParams::new(edits)),
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
        );
        let tunings = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../migration/corpus/tunings");
        let scale = tunings.join("corpus_septimal.scl");
        let map = tunings.join("corpus_septimal.kbm");

        let mut tuning = model.tuning();
        tuning.load_file(&scale).unwrap();
        model.load_tuning(tuning);
        let mut tuning = model.tuning();
        tuning.load_file(&map).unwrap();
        model.load_tuning(tuning);

        let mut expected = Tuning::new();
        expected.load_file(&scale).unwrap();
        expected.load_file(&map).unwrap();
        assert_eq!(model.tuning().state_to_json(), expected.state_to_json());
        assert_eq!(model.tuning_name(), expected.name());

        let mut map_only = Tuning::new();
        map_only.load_file(&map).unwrap();
        assert_ne!(model.tuning().state_to_json(), map_only.state_to_json());
        let mut scale_only = Tuning::new();
        scale_only.load_file(&scale).unwrap();
        assert_ne!(model.tuning().state_to_json(), scale_only.state_to_json());
    }

    #[test]
    fn keyboard_notes_reach_the_keyboard() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let (handle, mut consumer) = keyboard();
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            Arc::new(SynthParams::new(edits)),
            telemetry,
            handle,
            Arc::new(MidiLearn::in_memory()),
        );
        model.note_on(60, 0.5);
        assert_eq!(model.held_notes(), 1 << 60);
        model.note_off(60);
        assert_eq!(model.held_notes(), 0);
        consumer.collect();
        consumer.schedule(64);
        assert_eq!(consumer.pop(), Some((0, [0x90, 60, 64])));
        assert_eq!(
            consumer.pop().map(|(_, message)| message),
            Some([0x80, 60, 0])
        );
    }
}
