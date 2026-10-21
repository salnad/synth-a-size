// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The voice's filter section: `FilterModule` (one slot, every model) and
//! `FiltersModule` (both slots and their routing).

pub mod filter_module;
pub mod filters_module;

pub use filter_module::FilterModule;
pub use filters_module::FiltersModule;
