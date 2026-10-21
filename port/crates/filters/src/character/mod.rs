// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Character filter models: comb, formant (with its formant manager and
//! vocal tract) and phaser. Owned by the Character Filters thread (M5b);
//! add one `pub mod` line per ported module below.

use sas_dsp_core::ProcessorRef;

pub mod comb_filter;
pub mod comb_module;
pub mod phaser_filter;

pub use comb_filter::CombFilter;
pub use comb_module::CombModule;
pub use phaser_filter::PhaserFilter;

use crate::synth_filter::FilterModel;

/// The formant and phaser arms of `SynthFilter::createFilter`
/// (`FormantFilter(0)`, `PhaserFilter(false)`). Returns `None` for the
/// models not ported yet and for every other model (the comb arm is
/// handled, as `None`, in `synth_filter::create_filter`).
pub fn create_filter(model: FilterModel) -> Option<ProcessorRef> {
    match model {
        FilterModel::Phase => Some(PhaserFilter::create(false)),
        FilterModel::Formant => None,
        _ => None,
    }
}
