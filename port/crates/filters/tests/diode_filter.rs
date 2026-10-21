// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity for `DiodeFilter`: every `filters/*` case of the generic
//! "filter" ref-module with model "diode" (`SynthFilter::createFilter`).

mod support;

#[test]
fn diode_matches_reference() {
    support::check_filter_model("diode");
}
