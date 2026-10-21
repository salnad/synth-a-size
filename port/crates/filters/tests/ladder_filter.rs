// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity for `LadderFilter`: every `filters/*` case of the generic
//! "filter" ref-module with model "ladder" (`SynthFilter::createFilter`).

mod support;

#[test]
fn ladder_matches_reference() {
    support::check_filter_model("ladder");
}
