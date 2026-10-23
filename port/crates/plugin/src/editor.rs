// SPDX-License-Identifier: GPL-3.0-or-later

//! What the `sas-gui` editor reads from the plugin.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use sas_gui::EditorModel;

use crate::params::SynthParams;

/// The editor's view of one plugin instance.
pub struct PluginModel {
    params: Arc<SynthParams>,
    /// The preset name and the document generation it was read at, so the
    /// document lock is taken only when a new document arrives.
    preset_name: Mutex<(u64, String)>,
}

impl PluginModel {
    pub fn new(params: Arc<SynthParams>) -> PluginModel {
        let name = params.document().state.info.preset_name.clone();
        let generation = params.document_generation.load(Ordering::Acquire);
        PluginModel {
            params,
            preset_name: Mutex::new((generation, name)),
        }
    }
}

impl EditorModel for PluginModel {
    fn preset_name(&self) -> String {
        let generation = self.params.document_generation.load(Ordering::Acquire);
        let mut cached = self
            .preset_name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cached.0 != generation {
            *cached = (
                generation,
                self.params.document().state.info.preset_name.clone(),
            );
        }
        cached.1.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::PresetDocument;
    use sas_preset::InitContent;

    #[test]
    fn init_preset_has_no_name() {
        let params = Arc::new(SynthParams::new(PresetDocument::init(&InitContent {
            sample_seed: 0,
        })));
        assert_eq!(PluginModel::new(params).preset_name(), "");
    }
}
