// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::SynthFilter`: the interface every filter model implements, its
//! shared input layout, styles, settings snapshot and coefficient lookup.
//!
//! Ports `vital/src/synthesis/filters/synth_filter.h` and `synth_filter.cpp`.
//!
//! Upstream filters inherit from both `Processor` and `SynthFilter`. Here a
//! filter is a [`ProcessorImpl`] kernel that also implements [`SynthFilter`]
//! and keeps its own `filter_state: FilterState` field (upstream's protected
//! `filter_state_`). Code that holds the node calls `setupFilter` through
//! the concrete type, as upstream's callers do (`FormantFilter` on its
//! `DigitalSvf`s):
//!
//! ```text
//! svf.with::<DigitalSvf, _>(|svf, this| svf.setup_filter(this, &state));
//! ```
//!
//! [`create_filter`] is `SynthFilter::createFilter`. The character models
//! (comb, formant, phaser) are created in [`crate::character::create_filter`].

use std::sync::LazyLock;

use sas_dsp_core::{OneDimLookup, Processor, ProcessorImpl, ProcessorRef};
use sas_simd::common::PI;
use sas_simd::poly_utils as utils;
use sas_simd::{futils, PolyFloat};

/// `SynthFilter` input indices (the anonymous enum).
pub mod input {
    pub const AUDIO: usize = 0;
    pub const RESET: usize = 1;
    pub const MIDI_CUTOFF: usize = 2;
    pub const RESONANCE: usize = 3;
    pub const DRIVE_GAIN: usize = 4;
    pub const GAIN: usize = 5;
    pub const STYLE: usize = 6;
    pub const PASS_BLEND: usize = 7;
    pub const INTERPOLATE_X: usize = 8;
    pub const INTERPOLATE_Y: usize = 9;
    pub const TRANSPOSE: usize = 10;
    pub const SPREAD: usize = 11;
    /// `kNumInputs`.
    pub const NUM_INPUTS: usize = 12;
}

/// `SynthFilter::Style` (compared against [`FilterState::style`]).
pub mod style {
    pub const K12_DB: i32 = 0;
    pub const K24_DB: i32 = 1;
    pub const NOTCH_PASS_SWAP: i32 = 2;
    pub const DUAL_NOTCH_BAND: i32 = 3;
    pub const BAND_PEAK_NOTCH: i32 = 4;
    pub const SHELVING: i32 = 5;
    /// `kNumStyles`.
    pub const NUM_STYLES: i32 = 6;
}

/// `constants::FilterModel` (`vital/src/common/synth_constants.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum FilterModel {
    Analog = 0,
    Dirty,
    Ladder,
    Digital,
    Diode,
    Formant,
    Comb,
    Phase,
}

impl FilterModel {
    /// `kNumFilterModels`.
    pub const NUM_FILTER_MODELS: i32 = 8;

    /// The model with index `value`, if any.
    pub fn from_index(value: i32) -> Option<FilterModel> {
        use FilterModel::*;
        [Analog, Dirty, Ladder, Digital, Diode, Formant, Comb, Phase]
            .get(usize::try_from(value).ok()?)
            .copied()
    }
}

const MAX_DRIVE_GAIN: f32 = 36.0;
const MIN_DRIVE_GAIN: f32 = 0.0;

/// `SynthFilter::computeOnePoleFilterCoefficient`.
#[inline]
pub fn compute_one_pole_filter_coefficient(frequency_ratio: f32) -> f32 {
    const MAX_RADS: f32 = 0.499 * PI;
    let scaled = frequency_ratio * PI;
    std_min(MAX_RADS, scaled / (scaled + 1.0)).tan()
}

/// `std::min(a, b)`: `b < a ? b : a`.
#[inline(always)]
pub(crate) fn std_min(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}

/// `SynthFilter::CoefficientLookup`
/// (`OneDimLookup<computeOnePoleFilterCoefficient, 2048>`).
pub type CoefficientLookup = OneDimLookup;

/// `CoefficientLookup` resolution.
pub const COEFFICIENT_LOOKUP_RESOLUTION: usize = 2048;

static COEFFICIENT_LOOKUP: LazyLock<CoefficientLookup> = LazyLock::new(|| {
    OneDimLookup::new(
        compute_one_pole_filter_coefficient,
        COEFFICIENT_LOOKUP_RESOLUTION,
        1.0,
    )
});

/// `SynthFilter::getCoefficientLookup()` (the static `coefficient_lookup_`).
pub fn coefficient_lookup() -> &'static CoefficientLookup {
    &COEFFICIENT_LOOKUP
}

/// `SynthFilter::FilterState`: the per-block settings read from the inputs.
#[derive(Clone, Debug)]
pub struct FilterState {
    pub midi_cutoff: PolyFloat,
    /// `midi_cutoff_buffer`: the cutoff input's source buffer (audio rate).
    /// `None` until [`FilterState::load_settings`] runs (upstream `nullptr`).
    pub midi_cutoff_buffer: Option<sas_dsp_core::Buffer>,
    pub resonance_percent: PolyFloat,
    pub drive: PolyFloat,
    pub drive_percent: PolyFloat,
    pub gain: PolyFloat,
    pub style: i32,
    pub pass_blend: PolyFloat,
    pub interpolate_x: PolyFloat,
    pub interpolate_y: PolyFloat,
    pub transpose: PolyFloat,
}

impl Default for FilterState {
    fn default() -> Self {
        FilterState {
            midi_cutoff: PolyFloat::splat(1.0),
            midi_cutoff_buffer: None,
            resonance_percent: PolyFloat::splat(0.0),
            drive: PolyFloat::splat(1.0),
            drive_percent: PolyFloat::splat(0.0),
            gain: PolyFloat::splat(0.0),
            style: 0,
            pass_blend: PolyFloat::splat(0.0),
            interpolate_x: PolyFloat::splat(0.5),
            interpolate_y: PolyFloat::splat(0.5),
            transpose: PolyFloat::splat(0.0),
        }
    }
}

impl FilterState {
    /// `FilterState::loadSettings(processor)`.
    pub fn load_settings(&mut self, processor: &Processor) {
        let cutoff = processor.input(input::MIDI_CUTOFF);
        self.midi_cutoff = cutoff.at(0);
        self.midi_cutoff_buffer = Some(cutoff.buffer());
        self.resonance_percent = processor.input(input::RESONANCE).at(0);
        let input_drive = utils::clamp(
            processor.input(input::DRIVE_GAIN).at(0),
            MIN_DRIVE_GAIN,
            MAX_DRIVE_GAIN,
        );
        self.drive_percent =
            (input_drive - MIN_DRIVE_GAIN) * (1.0 / (MAX_DRIVE_GAIN - MIN_DRIVE_GAIN));
        self.drive = futils::db_to_magnitude(input_drive);
        self.gain = processor.input(input::GAIN).at(0);
        self.style = processor.input(input::STYLE).at(0).get(0) as i32;
        self.pass_blend = utils::clamp(processor.input(input::PASS_BLEND).at(0), 0.0, 2.0);
        self.interpolate_x = processor.input(input::INTERPOLATE_X).at(0);
        self.interpolate_y = processor.input(input::INTERPOLATE_Y).at(0);
        self.transpose = processor.input(input::TRANSPOSE).at(0);
    }

    /// `midi_cutoff_buffer[index]`.
    ///
    /// # Panics
    /// Before [`FilterState::load_settings`] (upstream dereferences `nullptr`).
    #[inline(always)]
    pub fn midi_cutoff_at(&self, index: usize) -> PolyFloat {
        self.midi_cutoff_buffer
            .as_ref()
            .expect("midi_cutoff_buffer is set by load_settings")[index]
            .get()
    }
}

/// `vital::SynthFilter`.
pub trait SynthFilter: ProcessorImpl {
    /// `setupFilter(filter_state)`. `this` is the filter's own node
    /// (upstream reads `getSampleRate()` and friends from `this`).
    fn setup_filter(&mut self, this: &Processor, filter_state: &FilterState);
}

/// `SynthFilter::createFilter(model)`. Returns the filter's node, or
/// `None` for a model that is not ported yet.
///
/// Always `None` for [`FilterModel::Comb`]: upstream builds `CombFilter(1)`,
/// whose processing reads past its 2-float memory (undefined behaviour), so
/// there is no defined output to reproduce. The engine's comb filter
/// (`CombModule`, inside `FilterModule`) is built with its full memory.
pub fn create_filter(model: FilterModel) -> Option<ProcessorRef> {
    match model {
        FilterModel::Comb => None,
        FilterModel::Formant | FilterModel::Phase => crate::character::create_filter(model),
        FilterModel::Digital => Some(crate::digital_svf::DigitalSvf::create()),
        FilterModel::Ladder => Some(crate::ladder_filter::LadderFilter::create()),
        FilterModel::Diode => Some(crate::diode_filter::DiodeFilter::create()),
        FilterModel::Analog => Some(crate::sallen_key_filter::SallenKeyFilter::create()),
        FilterModel::Dirty => None,
    }
}
