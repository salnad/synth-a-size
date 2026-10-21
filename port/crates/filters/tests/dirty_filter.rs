// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity for `DirtyFilter`: every `filters/*` case of the generic
//! "filter" ref-module with model "dirty" (`SynthFilter::createFilter`).

mod support;

#[test]
fn dirty_matches_reference() {
    support::check_filter_model("dirty");
}
