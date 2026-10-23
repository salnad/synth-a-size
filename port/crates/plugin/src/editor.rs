// SPDX-License-Identifier: GPL-3.0-or-later

//! What the `sas-gui` editor reads from the plugin.

use std::sync::{Arc, Mutex};

use sas_gui::{EditorModel, HostParam};

use crate::params::SynthParams;

/// The editor's view of one plugin instance.
pub struct PluginModel {
    params: Arc<SynthParams>,
    /// The preset name and the edit bus revision it was read at, so the
    /// document mirror is read only after it changes.
    preset_name: Mutex<(u64, String)>,
}

impl PluginModel {
    pub fn new(params: Arc<SynthParams>) -> PluginModel {
        let edits = params.edit_handle();
        let revision = edits.revision();
        let name = edits.with_document(|document| document.state.info.preset_name.clone());
        PluginModel {
            params,
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

    fn param(&self, name: &str) -> Option<HostParam> {
        self.params
            .control(name)
            .map(|control| control.param.as_ptr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sas_engine::document::PresetDocument;
    use sas_engine::edit_bus::EditBus;
    use sas_preset::InitContent;

    #[test]
    fn the_preset_name_follows_renames() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        let model = PluginModel::new(params.clone());
        assert_eq!(model.preset_name(), "");
        let edits = params.edit_handle();
        let mut info = edits.with_document(|document| document.state.info.clone());
        info.preset_name = "Renamed".to_string();
        edits.set_info(info);
        assert_eq!(model.preset_name(), "Renamed");
    }
}
