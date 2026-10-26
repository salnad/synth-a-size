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
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use nice_plug::params::internals::ParamPtr;
use nice_plug::params::InternalParamMut;
use nice_plug::plugin::ParamValue;
use nice_plug::prelude::*;
use sas_engine::edit_bus::EditHandle;
use sas_preset::display::{display_text, value_from_text};
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

    /// The host-facing normalized value (before host modulation), as
    /// nice-plug passes it to the host.
    fn normalized(&self) -> f32 {
        match self {
            ControlParam::Float(param) => param.unmodulated_normalized_value(),
            ControlParam::Stepped(param) => param.unmodulated_normalized_value(),
            ControlParam::Bypass(param) => param.unmodulated_normalized_value(),
        }
    }

    /// Sets the normalized value, firing the change callback if it differs.
    ///
    /// # Safety
    /// Only stores atomics; a host edit racing it leaves whichever lands
    /// last.
    unsafe fn set_normalized(&self, normalized: f32) {
        unsafe {
            match self {
                ControlParam::Float(param) => param._internal_set_normalized_value(normalized),
                ControlParam::Stepped(param) => param._internal_set_normalized_value(normalized),
                ControlParam::Bypass(param) => param._internal_set_normalized_value(normalized),
            };
        }
    }

    /// The host parameter, for the editor.
    pub fn as_ptr(&self) -> ParamPtr {
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

    fn unmark(&self, index: usize) {
        self.words[index / 64].fetch_and(!(1 << (index % 64)), Ordering::AcqRel);
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

/// No value recorded (not a normalized f32: its bits are a NaN).
const NO_VALUE: u32 = u32::MAX;

/// Forwarded values remembered per control: the host may echo a few frames'
/// worth back if its blocks are longer than the editor's frames.
const FORWARDED_SLOTS: usize = 4;

/// Recognises the host echoing back a value the editor forwarded for a
/// learned MIDI move after newer MIDI moved the control on: per control, the
/// last value MIDI set and the values forwarded since, all as the normalized
/// f32 bits nice-plug passes to the host.
struct EchoGuard {
    midi: Box<[AtomicU32]>,
    /// When MIDI last moved each control, in the processor's running sample
    /// count (audio thread only).
    midi_at: Box<[AtomicU64]>,
    forwarded: Box<[[AtomicU32; FORWARDED_SLOTS]]>,
    /// The editor's next slot per control (only the editor writes it).
    next: Box<[AtomicUsize]>,
}

impl EchoGuard {
    fn new(len: usize) -> EchoGuard {
        EchoGuard {
            midi: (0..len).map(|_| AtomicU32::new(NO_VALUE)).collect(),
            midi_at: (0..len).map(|_| AtomicU64::new(0)).collect(),
            forwarded: (0..len)
                .map(|_| std::array::from_fn(|_| AtomicU32::new(NO_VALUE)))
                .collect(),
            next: (0..len).map(|_| AtomicUsize::new(0)).collect(),
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
    /// The edit bus and its mirror of the document (modulation routing,
    /// shapes, wavetables, sample, save info, tuning). Control values in the
    /// mirror are stale; the parameters hold the live ones. A restored
    /// document replaces the mirror and moves the bus to a new generation,
    /// so the audio thread reloads it. Generation 0, the processor's
    /// starting point, is the initial document, which describes the engine
    /// as constructed and is not loaded.
    edits: EditHandle,
    /// Controls a learned MIDI controller moved while the editor was open,
    /// for the editor to tell the host.
    midi_moved: ChangeSet,
    /// Editor windows open. A host may open a new window before the old
    /// one's close arrives, so this counts rather than flags.
    editor_windows: AtomicUsize,
    echo_guard: EchoGuard,
    /// The sample rate the host activated the plugin at (`f32` bits), for
    /// work off the audio thread that renders as the engine would.
    sample_rate: AtomicU32,
}

impl SynthParams {
    /// Parameters at their registry defaults, around the edit bus `edits`,
    /// whose mirror is the starting document (normally the init preset).
    pub fn new(edits: EditHandle) -> SynthParams {
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
            edits,
            midi_moved: ChangeSet::new(names.len()),
            editor_windows: AtomicUsize::new(0),
            echo_guard: EchoGuard::new(names.len()),
            sample_rate: AtomicU32::new(44_100.0f32.to_bits()),
        }
    }

    /// The sample rate the host activated the plugin at (44.1 kHz before
    /// the first activation).
    pub fn sample_rate(&self) -> f32 {
        f32::from_bits(self.sample_rate.load(Ordering::Relaxed))
    }

    pub(crate) fn set_sample_rate(&self, sample_rate: f32) {
        self.sample_rate
            .store(sample_rate.to_bits(), Ordering::Relaxed);
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

    /// The edit bus, for the editor and the chat layer: modulation routing,
    /// shapes, wavetables and the sample change through it; every control
    /// changes through its host parameter.
    pub fn edit_handle(&self) -> &EditHandle {
        &self.edits
    }

    /// The whole current preset: the stored document with every control set
    /// from its parameter.
    pub fn preset_document(&self) -> PresetDocument {
        let mut document = self.edits.with_document(PresetDocument::clone);
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
        // The wrapper set the parameters from this preset just before.
        self.touched.clear();
        self.edits.begin_restore(document);
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

    /// A learned MIDI controller set control `name` to `value` in the engine
    /// (`SynthBase::valueChangedThroughMidi`): the parameter takes the value,
    /// so the editor shows it and saved states keep it, and counts as set
    /// since the last restore. It is not sent back to the engine as a host
    /// change, as upstream's `ValueBridge` guards `setValueNotifyHost` from
    /// coming back through `parameterChanged`. While the editor is open it
    /// tells the host ([`drain_midi_moved`](Self::drain_midi_moved));
    /// otherwise the host is not told (see `migration/DIVERGENCES.md`, D16).
    /// `now` is the processor's running sample count at the message.
    pub(crate) fn set_from_midi(&self, name: &str, value: f32, now: u64) {
        let Some(&index) = self.by_name.get(name) else {
            return;
        };
        // SAFETY: the setters only store atomics. A host edit of the same
        // parameter racing this one leaves whichever value lands last.
        unsafe {
            match &self.controls[index].param {
                ControlParam::Float(param) => {
                    param._internal_set_plain_value(value);
                }
                ControlParam::Stepped(param) => {
                    param._internal_set_plain_value(value.round() as i32);
                }
                ControlParam::Bypass(param) => {
                    param._internal_set_plain_value(value != 0.0);
                }
            }
        }
        self.changes.unmark(index);
        let normalized = self.controls[index].param.normalized();
        self.echo_guard.midi[index].store(normalized.to_bits(), Ordering::Release);
        self.echo_guard.midi_at[index].store(now, Ordering::Release);
        if self.editor_windows.load(Ordering::Acquire) > 0 {
            self.midi_moved.mark(index);
        }
    }

    /// The editor is about to forward `normalized` (as it passes it to the
    /// host) for control `name`, a learned MIDI move: remembered so the
    /// host's echo of it can be recognised if MIDI has moved on by then.
    pub fn record_forwarded(&self, name: &str, normalized: f32) {
        let Some(&index) = self.by_name.get(name) else {
            return;
        };
        let slot = self.echo_guard.next[index].fetch_add(1, Ordering::Relaxed) % FORWARDED_SLOTS;
        self.echo_guard.forwarded[index][slot].store(normalized.to_bits(), Ordering::Release);
    }

    /// Audio thread, for a host change of control `index`: whether it is the
    /// host echoing a value the editor forwarded while newer MIDI has since
    /// moved the control. If so the parameter takes the last MIDI value back
    /// (the engine already has it), the forwarded value is forgotten, and the
    /// control stays marked moved so the editor forwards its current value
    /// next frame. Upstream's echo carries the current value and so never
    /// steps the control back.
    ///
    /// A stale echo arrives within a few frames of the MIDI move that
    /// overtook it, so only a change within `window` samples of the last
    /// MIDI move (at sample `now`) can be one. Later matches are automation
    /// the host recorded from forwarded values, played back once the knob
    /// went idle.
    pub(crate) fn take_stale_echo(&self, index: usize, now: u64, window: u64) -> bool {
        let param = &self.controls[index].param;
        let host = param.normalized().to_bits();
        let midi = self.echo_guard.midi[index].load(Ordering::Acquire);
        if midi == NO_VALUE || host == midi {
            return false;
        }
        let midi_at = self.echo_guard.midi_at[index].load(Ordering::Acquire);
        if now.saturating_sub(midi_at) > window {
            return false;
        }
        let forwarded = self.echo_guard.forwarded[index].iter().any(|slot| {
            slot.compare_exchange(host, NO_VALUE, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        });
        if !forwarded {
            return false;
        }
        // SAFETY: only stores atomics (see `set_normalized`).
        unsafe { param.set_normalized(f32::from_bits(midi)) };
        self.changes.unmark(index);
        if self.editor_windows.load(Ordering::Acquire) > 0 {
            self.midi_moved.mark(index);
        }
        true
    }

    /// An editor window opened (`true`) or closed. While any is open,
    /// controls a learned MIDI controller moves are kept for
    /// [`drain_midi_moved`](Self::drain_midi_moved); moves while none is
    /// open are not, so a window that opens later reports none of them.
    pub fn set_editor_open(&self, open: bool) {
        if open {
            if self.editor_windows.load(Ordering::Acquire) == 0 {
                self.midi_moved.clear();
            }
            self.editor_windows.fetch_add(1, Ordering::AcqRel);
        } else {
            let mut open = self.editor_windows.load(Ordering::Acquire);
            while open > 0 {
                match self.editor_windows.compare_exchange_weak(
                    open,
                    open - 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break,
                    Err(current) => open = current,
                }
            }
            let closed_last = open == 1;
            if closed_last {
                self.midi_moved.clear();
            }
        }
    }

    /// Calls `f` with each control a learned MIDI controller moved since the
    /// last call while the editor was open, in registry order.
    pub fn drain_midi_moved(&self, mut f: impl FnMut(&'static str)) {
        self.midi_moved
            .drain(|index| f(self.controls[index].info.name.as_str()));
    }

    /// Loads the preset JSON `text` on top of the current document, like
    /// `SynthPlugin::setStateInformation`. On error the state is unchanged.
    pub(crate) fn restore_document(&self, text: &str) -> Result<(), sas_preset::Error> {
        let mut document = self.edits.with_document(PresetDocument::clone);
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
}
