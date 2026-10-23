// SPDX-License-Identifier: GPL-3.0-or-later

//! What the `sas-gui` editor reads from the plugin.

use std::sync::{Arc, Mutex};

use sas_gui::{EditorModel, HostParam};

use crate::params::SynthParams;

/// The editor's view of one plugin instance.
pub struct PluginModel {
    params: Arc<SynthParams>,
    /// The preset name and the edit bus generation it was read at, so the
    /// document mirror is read only when a new document arrives.
    preset_name: Mutex<(u64, String)>,
}

impl PluginModel {
    pub fn new(params: Arc<SynthParams>) -> PluginModel {
        let edits = params.edit_handle();
        let generation = edits.generation();
        let name = edits.with_document(|document| document.state.info.preset_name.clone());
        PluginModel {
            params,
            preset_name: Mutex::new((generation, name)),
        }
    }
}

impl EditorModel for PluginModel {
    fn preset_name(&self) -> String {
        let edits = self.params.edit_handle();
        let generation = edits.generation();
        let mut cached = self
            .preset_name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cached.0 != generation {
            *cached = (
                generation,
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
    fn init_preset_has_no_name() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        assert_eq!(PluginModel::new(params).preset_name(), "");
    }
}
