// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity for `SallenKeyFilter`: every `filters/*` case of the generic
//! "filter" ref-module with model "analog" (`SynthFilter::createFilter`).

mod support;

#[test]
fn analog_matches_reference() {
    support::check_filter_model("analog");
}
