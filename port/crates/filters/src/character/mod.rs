// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Character filter models: comb, formant (with its formant manager and
//! vocal tract) and phaser. Owned by the Character Filters thread (M5b);
//! add one `pub mod` line per ported module below.

use sas_dsp_core::ProcessorRef;

use crate::synth_filter::FilterModel;

/// The comb, formant and phaser arms of `SynthFilter::createFilter`
/// (`CombFilter(1)`, `FormantFilter(0)`, `PhaserFilter(false)`). Returns
/// `None` for the models not ported yet and for every other model.
pub fn create_filter(model: FilterModel) -> Option<ProcessorRef> {
    match model {
        FilterModel::Comb | FilterModel::Formant | FilterModel::Phase => None,
        _ => None,
    }
}
