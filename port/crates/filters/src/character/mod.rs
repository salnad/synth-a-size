// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Character filter models: comb, formant (with its formant manager and
//! vocal tract) and phaser. Owned by the Character Filters thread (M5b);
//! add one `pub mod` line per ported module below.

use sas_dsp_core::ProcessorRef;

pub mod comb_filter;
pub mod comb_module;
pub mod formant_filter;
pub mod formant_manager;
pub mod formant_module;
pub mod phaser_filter;
pub mod vocal_tract;

pub use comb_filter::CombFilter;
pub use comb_module::CombModule;
pub use formant_filter::FormantFilter;
pub use formant_manager::FormantManager;
pub use formant_module::FormantModule;
pub use phaser_filter::PhaserFilter;
pub use vocal_tract::VocalTract;

use crate::synth_filter::FilterModel;

/// The formant and phaser arms of `SynthFilter::createFilter`
/// (`FormantFilter(0)`, `PhaserFilter(false)`). Returns `None` for every
/// other model (the comb arm is handled, as `None`, in
/// `synth_filter::create_filter`). Like upstream's, the formant filter is a
/// router that needs `init()` before processing.
pub fn create_filter(model: FilterModel) -> Option<ProcessorRef> {
    match model {
        FilterModel::Phase => Some(PhaserFilter::create(false)),
        FilterModel::Formant => Some(FormantFilter::create(0)),
        _ => None,
    }
}
