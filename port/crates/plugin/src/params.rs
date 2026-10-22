// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Host parameters, one per engine control of the registry.
//!
//! Ports the parameter side of `vital/src/plugin/synth_plugin.cpp` and
//! `vital/src/plugin/value_bridge.h`: upstream adds a `ValueBridge` for every
//! registry entry the engine has a control for, in registry index order, and
//! uses `bypass` as the host's bypass parameter: 772 of the 794 registry
//! entries. The 22 legacy entries without a control
//! (`sas_preset::engine::NON_CONTROL_PARAMETERS`) are not exposed, as
//! upstream.
//!
//! The stable host ID of a parameter is its registry name (the key it has in
//! a preset's `settings`), so IDs never depend on order and survive registry
//! additions. The VST3 parameter ID is nice-plug's hash of that name.
//!
//! Each parameter stores the engine value itself (the plain value). Indexed
//! controls with fewer than 300 steps are stepped for the host
//! (`ValueBridge::isDiscrete` / `getNumSteps`); other indexed controls are
//! continuous and snap host edits to whole numbers. Both round like
//! `ValueBridge::convertToEngineValue`. A restored preset's own value stands
//! until the control is set, while the parameter still represents it (see
//! [`ControlParam::matches`]), so out-of-range, non-integer and -0.0 values
//! round-trip exactly.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use nice_plug::params::internals::ParamPtr;
use nice_plug::params::InternalParamMut;
use nice_plug::plugin::ParamValue;
use nice_plug::prelude::*;
use sas_preset::{engine, registry, ParamInfo, PresetState, ValueScale};

use crate::state::{self, PresetDocument};

/// How a control is presented to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKind {
    Continuous,
    /// Indexed with fewer than 300 steps (`ValueBridge::isDiscrete`).
    Stepped,
    /// `bypass`.
    Bypass,
}

/// `ValueBridge::isDiscrete`: indexed, with `round(max - min)` below
/// `kMaxIndexedSteps`.
pub fn host_kind(info: &ParamInfo) -> HostKind {
    const MAX_INDEXED_STEPS: f32 = 300.0;
    if info.name == "bypass" {
        HostKind::Bypass
    } else if info.value_scale == ValueScale::Indexed
        && (info.max - info.min).round() < MAX_INDEXED_STEPS
    {
        HostKind::Stepped
    } else {
        HostKind::Continuous
    }
}

impl HostKind {
    /// The saved host value (`PluginState::params`) for engine value `value`.
    pub fn param_value(self, value: f32) -> ParamValue {
        match self {
            HostKind::Continuous => ParamValue::F32(value),
            HostKind::Stepped => ParamValue::I32(value.round() as i32),
            HostKind::Bypass => ParamValue::Bool(value != 0.0),
        }
    }
}

/// The host parameter behind one control.
pub enum ControlParam {
    Float(FloatParam),
    /// Stepped indexed control; host edits round like
    /// `ValueBridge::convertToEngineValue`.
    Stepped(IntParam),
    /// `bypass`, flagged as the host bypass parameter.
    Bypass(BoolParam),
}

impl ControlParam {
    /// The value the engine uses (after any host modulation).
    pub fn value(&self) -> f32 {
        match self {
            ControlParam::Float(param) => param.modulated_plain_value(),
            ControlParam::Stepped(param) => param.modulated_plain_value() as f32,
            ControlParam::Bypass(param) => bool_value(param.modulated_plain_value()),
        }
    }

    /// The value saved in the preset (before any host modulation).
    pub fn saved_value(&self) -> f32 {
        match self {
            ControlParam::Float(param) => param.unmodulated_plain_value(),
            ControlParam::Stepped(param) => param.unmodulated_plain_value() as f32,
            ControlParam::Bypass(param) => bool_value(param.unmodulated_plain_value()),
        }
    }

    /// Whether host value `current` (a [`value`](Self::value) or
    /// [`saved_value`](Self::saved_value)) is how this parameter represents
    /// the preset value `preset`. nice-plug ignores a set that compares equal
    /// to the current value, so a preset's -0.0 reads back as 0.0; stepped and
    /// bypass parameters hold `preset` rounded. While this holds and the
    /// control is untouched since the restore, the preset's own value stands.
    pub fn matches(&self, preset: f32, current: f32) -> bool {
        match self {
            ControlParam::Float(_) => preset == current,
            ControlParam::Stepped(_) => preset.round() == current,
            ControlParam::Bypass(_) => (preset != 0.0) == (current != 0.0),
        }
    }

    fn saved_param_value(&self) -> ParamValue {
        match self {
            ControlParam::Float(param) => ParamValue::F32(param.unmodulated_plain_value()),
            ControlParam::Stepped(param) => ParamValue::I32(param.unmodulated_plain_value()),
            ControlParam::Bypass(param) => ParamValue::Bool(param.unmodulated_plain_value()),
        }
    }

    fn as_ptr(&self) -> ParamPtr {
        match self {
            ControlParam::Float(param) => param.as_ptr(),
            ControlParam::Stepped(param) => param.as_ptr(),
            ControlParam::Bypass(param) => param.as_ptr(),
        }
    }
}

fn bool_value(value: bool) -> f32 {
    if value {
        1.0
    } else {
        0.0
    }
}

/// One exposed control: its registry entry and host parameter.
pub struct Control {
    pub info: &'static ParamInfo,
    pub group: String,
    pub param: ControlParam,
}

/// Lock-free set of control indices whose value changed since the audio
/// thread last looked. Parameter callbacks mark, the audio thread drains.
pub(crate) struct ChangeSet {
    words: Box<[AtomicU64]>,
}

impl ChangeSet {
    fn new(len: usize) -> ChangeSet {
        ChangeSet {
            words: (0..len.div_ceil(64)).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    fn mark(&self, index: usize) {
        self.words[index / 64].fetch_or(1 << (index % 64), Ordering::AcqRel);
    }

    fn is_marked(&self, index: usize) -> bool {
        self.words[index / 64].load(Ordering::Acquire) & (1 << (index % 64)) != 0
    }

    fn clear(&self) {
        for word in self.words.iter() {
            word.store(0, Ordering::Release);
        }
    }

    /// Calls `f` with every marked index (ascending) and clears them.
    pub(crate) fn drain(&self, mut f: impl FnMut(usize)) {
        for (word_index, word) in self.words.iter().enumerate() {
            let mut bits = word.swap(0, Ordering::AcqRel);
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                f(word_index * 64 + bit);
                bits &= bits - 1;
            }
        }
    }
}

/// The plugin's parameters plus the preset state that is not a parameter
/// (modulation routing, wavetables, sample, LFO shapes, save info, tuning).
pub struct SynthParams {
    controls: Vec<Control>,
    by_name: HashMap<&'static str, usize>,
    bypass: usize,
    pub(crate) changes: Arc<ChangeSet>,
    /// Controls set since the document was last replaced (by the host, or by
    /// the wrapper applying a state without a preset).
    touched: Arc<ChangeSet>,
    /// Control values in here are stale; the parameters hold the live ones.
    document: Mutex<PresetDocument>,
    /// Bumped whenever `document` is replaced, so the audio thread reloads it.
    /// Starts at 0, the generation the processor starts with: the initial
    /// document describes the engine as constructed, so it is not loaded.
    pub(crate) document_generation: AtomicU64,
}

impl SynthParams {
    /// Parameters at their registry defaults, around `document` (normally the
    /// init preset).
    pub fn new(document: PresetDocument) -> SynthParams {
        let names: Vec<&'static str> = registry()
            .parameters()
            .iter()
            .map(|info| info.name.as_str())
            .filter(|name| engine::is_control(name))
            .collect();
        let changes = Arc::new(ChangeSet::new(names.len()));
        let touched = Arc::new(ChangeSet::new(names.len()));
        let controls: Vec<Control> = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let info = registry().get(name).expect("registry entry");
                Control {
                    info,
                    group: group_of(name),
                    param: make_param(info, index, &changes, &touched),
                }
            })
            .collect();
        let by_name = names
            .iter()
            .enumerate()
            .map(|(index, name)| (*name, index))
            .collect();
        let bypass = names
            .iter()
            .position(|name| *name == "bypass")
            .expect("bypass control");
        SynthParams {
            controls,
            by_name,
            bypass,
            changes,
            touched,
            document: Mutex::new(document),
            document_generation: AtomicU64::new(0),
        }
    }

    /// The exposed controls in host (registry index) order.
    pub fn controls(&self) -> &[Control] {
        &self.controls
    }

    /// The control named `name`.
    pub fn control(&self, name: &str) -> Option<&Control> {
        self.by_name.get(name).map(|&index| &self.controls[index])
    }

    pub fn bypassed(&self) -> bool {
        self.controls[self.bypass].param.value() != 0.0
    }

    pub(crate) fn document(&self) -> MutexGuard<'_, PresetDocument> {
        self.document
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn try_document(&self) -> Option<MutexGuard<'_, PresetDocument>> {
        match self.document.try_lock() {
            Ok(guard) => Some(guard),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }

    /// The whole current preset: the stored document with every control set
    /// from its parameter.
    pub fn preset_document(&self) -> PresetDocument {
        let mut document = self.document().clone();
        self.write_controls(&mut document.state);
        document
    }

    /// [`preset_document`](Self::preset_document) without the tuning, as
    /// saved to a preset file.
    pub fn preset_state(&self) -> PresetState {
        self.preset_document().state
    }

    /// Whether the restored preset's own value `preset` still stands for
    /// control `index`, whose parameter reads `current`: the control has not
    /// been set since the restore, and its parameter represents the value
    /// ([`ControlParam::matches`]). A control moved away and back no longer
    /// stands for the preset's value.
    pub(crate) fn preset_value_stands(&self, index: usize, preset: f32, current: f32) -> bool {
        !self.touched.is_marked(index) && self.controls[index].param.matches(preset, current)
    }

    fn write_controls(&self, preset: &mut PresetState) {
        for (index, control) in self.controls.iter().enumerate() {
            let name = control.info.name.as_str();
            let saved = control.param.saved_value();
            let value = match preset.value(name) {
                Some(value) if self.preset_value_stands(index, value, saved) => value,
                _ => saved,
            };
            preset
                .set_value(name, value)
                .expect("every exposed parameter is a control");
        }
    }

    /// Replaces the non-parameter state with `document` (its control values
    /// are set on the parameters by the wrapper, from `filter_state`).
    fn replace_document(&self, document: PresetDocument) {
        let mut current = self.document();
        *current = document;
        // The wrapper set the parameters from this preset just before.
        self.touched.clear();
        // Bumped under the lock, so a reader holding it sees a matching pair.
        self.document_generation.fetch_add(1, Ordering::AcqRel);
    }

    /// The state nice-plug's wrapper saves for the host: every parameter's
    /// saved value plus the preset JSON.
    pub fn capture_state(&self) -> PluginState {
        let params = self
            .controls
            .iter()
            .map(|control| (control.info.name.clone(), control.param.saved_param_value()))
            .collect();
        PluginState {
            version: env!("CARGO_PKG_VERSION").to_string(),
            params,
            fields: self.serialize_fields(),
        }
    }

    /// Applies `state` the way nice-plug's wrapper restores a host state:
    /// [`state::filter_state`], then the parameter values, then the preset.
    /// Only for use without a host (offline rendering, tests); inside a host,
    /// pass the state to `GuiContext::set_state` so the host is notified.
    pub fn restore_state(&self, mut state: PluginState) {
        state::filter_state(&mut state);
        for (name, value) in &state.params {
            let Some(control) = self.control(name) else {
                continue;
            };
            // SAFETY: the setters only store atomics; without a host nothing
            // else is setting these parameters.
            unsafe {
                match (&control.param, value) {
                    (ControlParam::Float(param), ParamValue::F32(value)) => {
                        param._internal_set_plain_value(*value);
                    }
                    (ControlParam::Stepped(param), ParamValue::I32(value)) => {
                        param._internal_set_plain_value(*value);
                    }
                    (ControlParam::Bypass(param), ParamValue::Bool(value)) => {
                        param._internal_set_plain_value(*value);
                    }
                    _ => {}
                }
            }
        }
        self.deserialize_fields(&state.fields);
    }

    /// Loads the preset JSON `text` on top of the current document, like
    /// `SynthPlugin::setStateInformation`. On error the state is unchanged.
    pub(crate) fn restore_document(&self, text: &str) -> Result<(), sas_preset::Error> {
        let mut document = self.document().clone();
        document.load_json_str(text)?;
        self.replace_document(document);
        Ok(())
    }
}

/// Parameter group (VST3 unit) of a control, from its name.
fn group_of(name: &str) -> String {
    let numbered = [
        ("env_", "Envelope"),
        ("lfo_", "LFO"),
        ("random_", "Random LFO"),
        ("osc_", "Oscillator"),
        ("modulation_", "Modulation"),
        ("filter_", "Filter"),
    ];
    for (prefix, group) in numbered {
        if let Some(rest) = name.strip_prefix(prefix) {
            if let Some((id, _)) = rest.split_once('_') {
                let id = if id == "fx" { "FX" } else { id };
                return format!("{group} {id}");
            }
        }
    }
    let effects = [
        ("chorus_", "Chorus"),
        ("compressor_", "Compressor"),
        ("delay_", "Delay"),
        ("distortion_", "Distortion"),
        ("eq_", "EQ"),
        ("flanger_", "Flanger"),
        ("phaser_", "Phaser"),
        ("reverb_", "Reverb"),
        ("sample_", "Sample"),
        ("macro_control_", "Macros"),
        ("portamento_", "Voice"),
        ("voice_", "Voice"),
    ];
    for (prefix, group) in effects {
        if name.starts_with(prefix) {
            return group.to_string();
        }
    }
    String::new()
}

fn make_param(
    info: &'static ParamInfo,
    index: usize,
    changes: &Arc<ChangeSet>,
    touched: &Arc<ChangeSet>,
) -> ControlParam {
    let mark = {
        let changes = changes.clone();
        let touched = touched.clone();
        Arc::new(move || {
            touched.mark(index);
            changes.mark(index);
        })
    };
    match host_kind(info) {
        HostKind::Bypass => {
            return ControlParam::Bypass(
                BoolParam::new(info.display_name.clone(), info.default_value != 0.0)
                    .make_bypass()
                    .with_callback(Arc::new(move |_| mark())),
            );
        }
        HostKind::Stepped => {
            return ControlParam::Stepped(
                IntParam::new(
                    info.display_name.clone(),
                    info.default_value.round() as i32,
                    IntRange::Linear {
                        min: info.min.round() as i32,
                        max: info.max.round() as i32,
                    },
                )
                .with_callback(Arc::new(move |_| mark()))
                .with_value_to_string(Arc::new(move |value| display_text(info, value as f32)))
                .with_string_to_value(Arc::new(move |text| {
                    value_from_text(info, text).map(|value| value.round() as i32)
                })),
            );
        }
        HostKind::Continuous => {}
    }
    let mut param = FloatParam::new(
        info.display_name.clone(),
        info.default_value,
        FloatRange::Linear {
            min: info.min,
            max: info.max,
        },
    )
    .with_callback(Arc::new(move |_| mark()))
    .with_value_to_string(Arc::new(move |value| display_text(info, value)))
    .with_string_to_value(Arc::new(move |text| value_from_text(info, text)));
    if info.value_scale == ValueScale::Indexed {
        param = param.with_step_size(1.0);
    }
    ControlParam::Float(param)
}

/// `ValueBridge::skewValue`.
fn skew(info: &ParamInfo, value: f32) -> f32 {
    match info.value_scale {
        ValueScale::Quadratic => value * value,
        ValueScale::Cubic => value * value * value,
        ValueScale::Quartic => {
            let squared = value * value;
            squared * squared
        }
        ValueScale::Exponential if info.display_invert => 1.0 / 2.0f32.powf(value),
        ValueScale::Exponential => 2.0f32.powf(value),
        ValueScale::SquareRoot => value.sqrt(),
        ValueScale::Indexed | ValueScale::Linear => value,
    }
}

/// `ValueBridge::unskewValue`.
fn unskew(info: &ParamInfo, value: f32) -> f32 {
    match info.value_scale {
        ValueScale::Quadratic => value.sqrt(),
        ValueScale::Cubic => value.powf(1.0 / 3.0),
        ValueScale::Quartic => value.powf(1.0 / 4.0),
        ValueScale::Exponential if info.display_invert => (1.0 / value).log2(),
        ValueScale::Exponential => value.log2(),
        ValueScale::SquareRoot => value * value,
        ValueScale::Indexed | ValueScale::Linear => value,
    }
}

/// `ValueBridge::getText`: the option name of an indexed parameter, else the
/// displayed number and units.
fn display_text(info: &ParamInfo, value: f32) -> String {
    if let Some(options) = info.options() {
        let index = value.min(info.max).max(0.0) as usize;
        if let Some(option) = options.get(index) {
            return option.to_string();
        }
    }
    let display = info.display_multiply * skew(info, value) + info.post_offset;
    format!("{}{}", format_number(display), info.display_units)
        .trim()
        .to_string()
}

/// Up to four decimals, without trailing zeros.
fn format_number(value: f32) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

/// Inverse of [`display_text`] (`ValueBridge::getValueForText`, which also
/// accepts the option names and removes the post offset).
fn value_from_text(info: &ParamInfo, text: &str) -> Option<f32> {
    let text = text.trim();
    if let Some(options) = info.options() {
        if let Some(index) = options
            .iter()
            .position(|option| option.eq_ignore_ascii_case(text))
        {
            return Some(index as f32);
        }
    }
    let number_end = text
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0)))
        .map_or(text.len(), |(i, _)| i);
    let number: f32 = text[..number_end].parse().ok()?;
    let value = unskew(info, (number - info.post_offset) / info.display_multiply);
    value.is_finite().then_some(value)
}

// SAFETY: every `ParamPtr` points into `self.controls`, which lives as long as
// `self` (the wrapper keeps the `Arc<SynthParams>` alive) and never moves.
unsafe impl Params for SynthParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        self.controls
            .iter()
            .map(|control| {
                (
                    control.info.name.clone(),
                    control.param.as_ptr(),
                    control.group.clone(),
                )
            })
            .collect()
    }

    fn serialize_fields(&self) -> BTreeMap<String, String> {
        BTreeMap::from([(
            state::PRESET_FIELD.to_string(),
            self.preset_document().to_json_string(),
        )])
    }

    fn deserialize_fields(&self, serialized: &BTreeMap<String, String>) {
        if let Some(text) = serialized.get(state::PRESET_FIELD) {
            if let Err(error) = self.restore_document(text) {
                nice_plug::nice_warn!("cannot restore the saved preset: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_set_drains_each_index_once() {
        let set = ChangeSet::new(130);
        for index in [0, 63, 64, 129, 64] {
            set.mark(index);
        }
        let mut seen = Vec::new();
        set.drain(|index| seen.push(index));
        assert_eq!(seen, [0, 63, 64, 129]);
        set.drain(|_| panic!("drained twice"));
    }

    #[test]
    fn groups_follow_the_upstream_sections() {
        assert_eq!(group_of("filter_fx_cutoff"), "Filter FX");
        assert_eq!(group_of("filter_2_cutoff"), "Filter 2");
        assert_eq!(group_of("modulation_64_amount"), "Modulation 64");
        assert_eq!(group_of("osc_3_wave_frame"), "Oscillator 3");
        assert_eq!(group_of("random_1_style"), "Random LFO 1");
        assert_eq!(group_of("reverb_size"), "Reverb");
        assert_eq!(group_of("volume"), "");
    }

    #[test]
    fn display_text_matches_the_value_bridge() {
        let volume = registry().get("volume").unwrap();
        // sqrt(5473.0404) - 80 = -6.02 dB.
        assert_eq!(display_text(volume, volume.default_value), "-6.02dB");
        let delay_style = registry().get("delay_style").unwrap();
        assert_eq!(display_text(delay_style, 2.0), "Ping Pong");
        let cutoff = registry().get("filter_1_cutoff").unwrap();
        assert_eq!(display_text(cutoff, 60.0), "0 semitones");
        let attack = registry().get("env_1_attack").unwrap();
        assert_eq!(display_text(attack, 1.0), "1 secs");
    }

    #[test]
    fn text_input_inverts_the_display() {
        for (name, value) in [
            ("volume", 5473.0404f32),
            ("filter_1_cutoff", 72.0),
            ("lfo_1_frequency", 3.0),
            ("delay_frequency", 2.5),
            ("env_1_attack", 0.7),
            ("osc_1_level", 0.5),
        ] {
            let info = registry().get(name).unwrap();
            let parsed = value_from_text(info, &display_text(info, value)).unwrap();
            assert!(
                (parsed - value).abs() <= 1e-3 * value.abs().max(1.0),
                "{name}: {value} -> {} -> {parsed}",
                display_text(info, value)
            );
        }
        let delay_style = registry().get("delay_style").unwrap();
        assert_eq!(value_from_text(delay_style, "ping pong"), Some(2.0));
    }
}
