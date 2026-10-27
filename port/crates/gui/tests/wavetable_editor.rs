// SPDX-License-Identifier: GPL-3.0-or-later

//! The wavetable editor with a recording synth: an oscillator's edit button
//! opens it in place of the interface and its exit button closes it, with
//! nothing written to the host either way; the playhead moves the wave
//! frame in one gesture (D18); the organizer and the component list edit
//! the creator, committing each edit when it's done.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use nice_plug::context::gui::{GuiContext, GuiContextInner};
use nice_plug::context::PluginApi;
use nice_plug::prelude::{FloatParam, FloatRange, Param, ParamPtr, PluginState};
use sas_engine::document::PresetDocument;
use sas_engine::edit_bus::{AuditionToken, EditBus, EditHandle};
use sas_engine::sas_oscillators::sas_wavetable::creator::component::{
    ComponentType, WavetableComponent,
};
use sas_engine::sas_oscillators::sas_wavetable::creator::group::WavetableGroup;
use sas_engine::sas_oscillators::sas_wavetable::creator::WavetableCreator;
use sas_gui::layout::{FullLayout, Rect};
use sas_gui::local::{MODIFIER_CONTROLS, WAVE_SOURCE_CONTROLS};
use sas_gui::scope::Scope;
use sas_gui::sections::oscillator::{self, OscillatorLayout};
use sas_gui::sections::phase_window_overlays::{PhaseLayout, WindowLayout};
use sas_gui::sections::popup_browser::{self, PopupBrowserLayout};
use sas_gui::sections::selection_list::Selection;
use sas_gui::sections::synthesis::SynthesisLayout;
use sas_gui::sections::wave_source_overlay::OverlayLayout;
use sas_gui::sections::wavetable_edit::{self, KeyframeRef, WavetableEditLayout};
use sas_gui::skin::{Skin, ValueId};
use sas_gui::{EditorModel, Host, HostParam, Interface, OpenGestures, Tab};
use sas_preset::registry::registry;
use sas_preset::user_folders::Kind;
use sas_preset::InitContent;

/// Every registry control, and a wavetable creator per oscillator.
struct Synth {
    params: BTreeMap<&'static str, FloatParam>,
    creators: Mutex<Vec<WavetableCreator>>,
    revision: AtomicU64,
    generation: AtomicU64,
    /// How many edits the editor committed.
    commits: AtomicU64,
    data_directory: Mutex<Option<PathBuf>>,
    added_folders: Mutex<Vec<PathBuf>>,
    /// Each oscillator's last file loaded (`getLastFileLoaded`).
    last_files: Mutex<[Option<PathBuf>; 3]>,
    /// The wavetables loaded, by file name.
    loads: Mutex<Vec<String>>,
    /// The oscillators "Synthesize Preset to Table" asked for.
    resyntheses: Mutex<Vec<usize>>,
    /// An edit bus, for audition tokens.
    edits: EditHandle,
    /// The frames the editor auditioned: oscillator, position and the
    /// creator they were drawn from.
    auditions: Mutex<Vec<(usize, usize, WavetableCreator)>>,
    /// The oscillators whose auditions the editor ended.
    ends: Mutex<Vec<usize>>,
    /// Refuse commits, as a full edit queue does.
    refuse_commits: std::sync::atomic::AtomicBool,
}

impl Synth {
    fn new() -> Arc<Synth> {
        let params = registry()
            .parameters()
            .iter()
            .map(|info| {
                let range = FloatRange::Linear {
                    min: info.min,
                    max: info.max,
                };
                let name: &'static str = info.name.as_str();
                (name, FloatParam::new(name, info.default_value, range))
            })
            .collect();
        let creators = (0..3)
            .map(|_| {
                let mut creator = WavetableCreator::new();
                creator.init();
                creator
            })
            .collect();
        Arc::new(Synth {
            params,
            creators: Mutex::new(creators),
            revision: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            commits: AtomicU64::new(0),
            data_directory: Mutex::new(None),
            added_folders: Mutex::new(Vec::new()),
            last_files: Mutex::default(),
            loads: Mutex::new(Vec::new()),
            resyntheses: Mutex::new(Vec::new()),
            edits: EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 })).0,
            auditions: Mutex::new(Vec::new()),
            ends: Mutex::new(Vec::new()),
            refuse_commits: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn name_of(&self, ptr: ParamPtr) -> &'static str {
        self.params
            .iter()
            .find(|(_, param)| param.as_ptr() == ptr)
            .map(|(name, _)| *name)
            .unwrap()
    }

    /// Replaces oscillator `oscillator`'s creator, as an edit does.
    fn set_creator(&self, oscillator: usize, creator: WavetableCreator) {
        self.creators.lock().unwrap()[oscillator] = creator;
        self.revision.fetch_add(1, Ordering::Relaxed);
    }
}

impl EditorModel for Synth {
    fn preset_name(&self) -> String {
        String::new()
    }

    fn param(&self, name: &str) -> Option<HostParam> {
        self.params.get(name).map(|param| param.as_ptr())
    }

    fn wavetable_creator(&self, oscillator: usize) -> Option<WavetableCreator> {
        self.creators.lock().unwrap().get(oscillator).cloned()
    }

    fn wavetable_name(&self, oscillator: usize) -> String {
        self.creators.lock().unwrap()[oscillator].name.clone()
    }

    fn set_wavetable_creator(&self, oscillator: usize, creator: WavetableCreator) -> bool {
        if self.refuse_commits.load(Ordering::Relaxed) {
            return false;
        }
        self.commits.fetch_add(1, Ordering::Relaxed);
        self.set_creator(oscillator, creator);
        true
    }

    fn wavetable_audition_token(&self, oscillator: usize) -> Option<AuditionToken> {
        self.edits.audition_token(oscillator).ok()
    }

    fn audition_wavetable_frame(
        &self,
        oscillator: usize,
        creator: WavetableCreator,
        position: usize,
        _token: AuditionToken,
    ) -> bool {
        self.auditions
            .lock()
            .unwrap()
            .push((oscillator, position, creator));
        true
    }

    fn end_wavetable_audition(&self, oscillator: usize, _token: AuditionToken) {
        self.ends.lock().unwrap().push(oscillator);
    }

    fn content_revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    fn preset_generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    fn data_directory(&self) -> Option<PathBuf> {
        self.data_directory.lock().unwrap().clone()
    }

    fn additional_folders(&self, kind: Kind) -> Vec<PathBuf> {
        match kind {
            Kind::Wavetables => self.added_folders.lock().unwrap().clone(),
            _ => Vec::new(),
        }
    }

    fn load_wavetable_json(&self, oscillator: usize, json: &str) -> bool {
        let data = sas_common::json::parse(json).unwrap();
        let name = data.get("name").unwrap().to_str().unwrap().to_string();
        self.loads
            .lock()
            .unwrap()
            .push(format!("{oscillator} {name}"));
        true
    }

    fn load_audio_as_wavetable(
        &self,
        oscillator: usize,
        path: &std::path::Path,
        _style: sas_engine::sas_oscillators::sas_wavetable::creator::AudioFileLoadStyle,
    ) -> bool {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        self.loads
            .lock()
            .unwrap()
            .push(format!("{oscillator} audio {name}"));
        true
    }

    fn wavetable_json(&self, oscillator: usize) -> Option<String> {
        Some(self.creators.lock().unwrap()[oscillator].to_json().dump())
    }

    fn resynthesize_to_wavetable(&self, oscillator: usize) {
        self.resyntheses.lock().unwrap().push(oscillator);
    }

    fn last_wavetable_file(&self, oscillator: usize) -> Option<PathBuf> {
        self.last_files.lock().unwrap()[oscillator].clone()
    }

    fn set_last_wavetable_file(&self, oscillator: usize, file: PathBuf) {
        self.last_files.lock().unwrap()[oscillator] = Some(file);
    }
}

/// Records what the editor writes to the host.
struct Recorder {
    synth: Arc<Synth>,
    log: Mutex<Vec<String>>,
}

impl GuiContextInner for Recorder {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }

    unsafe fn raw_begin_set_parameter(&self, param: ParamPtr) {
        let name = self.synth.name_of(param);
        self.log.lock().unwrap().push(format!("begin {name}"));
    }

    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, _normalized: f32) {
        let name = self.synth.name_of(param);
        self.log.lock().unwrap().push(format!("set {name}"));
    }

    unsafe fn raw_end_set_parameter(&self, param: ParamPtr) {
        let name = self.synth.name_of(param);
        self.log.lock().unwrap().push(format!("end {name}"));
    }

    fn get_state(&self) -> PluginState {
        unimplemented!("the editor does not read the plugin state")
    }

    fn set_state(&self, _state: PluginState) {}

    fn request_restart(&self) {}
}

struct Fixture {
    synth: Arc<Synth>,
    recorder: Arc<Recorder>,
    context: GuiContext,
    open: OpenGestures,
    interface: RefCell<Interface>,
    skin: Skin,
}

impl Fixture {
    fn new() -> Fixture {
        let synth = Synth::new();
        let recorder = Arc::new(Recorder {
            synth: synth.clone(),
            log: Mutex::new(Vec::new()),
        });
        let context = GuiContext::new(recorder.clone());
        let mut interface = Interface::default();
        interface.set_tab(Tab::Voice);
        Fixture {
            synth,
            recorder,
            context,
            open: OpenGestures::default(),
            interface: RefCell::new(interface),
            skin: Skin::default(),
        }
    }

    fn harness(&self) -> Harness<'_> {
        let mut harness = Harness::builder()
            .with_size((1400.0, 820.0))
            .with_step_dt(1.0 / 60.0)
            .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
            .build_ui(|ui| {
                let screen = ui.ctx().content_rect();
                let host = Host::tracking(self.synth.as_ref(), Some(&self.context), &self.open);
                self.interface.borrow_mut().show_in(ui, screen, &host)
            });
        harness.step();
        harness
    }

    fn layout(&self) -> FullLayout {
        FullLayout::new(&self.skin, 1400, 820)
    }

    /// Oscillator `index`'s edit button.
    fn edit_button(&self, index: usize) -> Rect {
        let layout = self.layout();
        let top = Scope::top(&self.skin, layout.size_ratio);
        let synthesis = SynthesisLayout::new(&top, layout.main);
        let scope = top.within(oscillator::SKIN);
        OscillatorLayout::new(&scope, synthesis.oscillators[index], false).edit
    }

    fn editor_layout(&self) -> WavetableEditLayout {
        let layout = self.layout();
        let top = Scope::top(&self.skin, layout.size_ratio);
        let bounds = Rect::new(
            layout.bounds.x,
            0,
            layout.bounds.width,
            layout.bounds.height,
        );
        WavetableEditLayout::new(&top, bounds)
    }

    fn editor(&self) -> Option<usize> {
        self.interface.borrow().wavetable_editor()
    }

    fn overlay_layout(&self) -> OverlayLayout {
        let layout = self.layout();
        let top = Scope::top(&self.skin, layout.size_ratio);
        OverlayLayout::wave_source(&top, &self.editor_layout())
    }

    fn overlay_frame(&self, oscillator: usize) -> Option<KeyframeRef> {
        self.interface
            .borrow()
            .wavetable_edit_section(oscillator)
            .overlay_frame()
    }

    fn local(&self, name: &str) -> f32 {
        self.interface.borrow().local_value(name).unwrap()
    }

    fn selected(&self, oscillator: usize) -> Vec<KeyframeRef> {
        self.interface
            .borrow()
            .wavetable_edit_section(oscillator)
            .selected()
            .to_vec()
    }
}

fn centre(rect: Rect) -> Pos2 {
    Pos2::new(
        rect.x as f32 + rect.width as f32 / 2.0,
        rect.y as f32 + rect.height as f32 / 2.0,
    )
}

fn button(harness: &mut Harness, pos: Pos2, pressed: bool) {
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

fn click(harness: &mut Harness, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.step();
    button(harness, pos, true);
    button(harness, pos, false);
    harness.step();
}

const FIRST: KeyframeRef = KeyframeRef {
    group: 0,
    component: 0,
    keyframe: 0,
};

/// `OscillatorSection::buttonClicked` shows that oscillator's editor
/// (`showWavetableEditSection`), and its exit button hides it.
#[test]
fn the_edit_button_opens_the_editor_and_exit_closes_it() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    assert_eq!(fixture.editor(), None);
    click(&mut harness, centre(fixture.edit_button(1)));
    assert_eq!(fixture.editor(), Some(1));
    click(&mut harness, centre(fixture.editor_layout().exit));
    assert_eq!(fixture.editor(), None);
    click(&mut harness, centre(fixture.edit_button(2)));
    assert_eq!(fixture.editor(), Some(2));
    assert!(fixture.recorder.log.lock().unwrap().is_empty());
}

/// Showing selects the first source's first keyframe
/// (`visibilityChanged`'s `selectDefaultFrame`), and the selection stays
/// through edits that keep the keyframe.
#[test]
fn showing_selects_the_first_keyframe() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    assert!(fixture.selected(0).is_empty());
    fixture
        .interface
        .borrow_mut()
        .show_wavetable_editor(Some(0));
    harness.step();
    assert_eq!(fixture.selected(0), [FIRST]);
    assert!(fixture.selected(1).is_empty());

    let mut creator = fixture.synth.wavetable_creator(0).unwrap();
    let mut group = WavetableGroup::new();
    let mut source = WavetableComponent::new(ComponentType::LineSource);
    source.insert_new_keyframe(0);
    group.add_component(source);
    creator.add_group(group);
    fixture.synth.set_creator(0, creator);
    harness.step();
    assert_eq!(fixture.selected(0), [FIRST]);
    let section_rows = fixture
        .interface
        .borrow()
        .wavetable_edit_section(0)
        .creator()
        .map(|creator| creator.groups.len());
    assert_eq!(section_rows, Some(2));
}

/// A preset load resets the editor (`FullInterface::reset`): its
/// selection clears and the default frame is selected again, now or when
/// it next shows.
#[test]
fn a_preset_load_resets_the_selection() {
    let fixture = Fixture::new();
    let mut harness = fixture.harness();
    fixture
        .interface
        .borrow_mut()
        .show_wavetable_editor(Some(0));
    harness.step();
    let mut creator = WavetableCreator::new();
    creator.clear();
    fixture.synth.set_creator(0, creator);
    fixture.synth.generation.fetch_add(1, Ordering::Relaxed);
    harness.step();
    assert!(fixture.selected(0).is_empty());

    let mut creator = WavetableCreator::new();
    creator.init();
    fixture.synth.set_creator(0, creator);
    fixture.interface.borrow_mut().show_wavetable_editor(None);
    harness.step();
    fixture.synth.generation.fetch_add(1, Ordering::Relaxed);
    fixture
        .interface
        .borrow_mut()
        .show_wavetable_editor(Some(0));
    harness.step();
    assert_eq!(fixture.selected(0), [FIRST]);
}

/// Shows oscillator 1's editor and returns the harness.
fn opened(fixture: &Fixture) -> Harness<'_> {
    fixture
        .interface
        .borrow_mut()
        .show_wavetable_editor(Some(0));
    let mut harness = fixture.harness();
    harness.step();
    harness
}

fn frame_width(layout: &WavetableEditLayout) -> f32 {
    (layout.organizer.width - layout.handle_width + 1) as f32 / 256.0
}

/// The organizer's point for `position` in row `row`, inside the handle of
/// a keyframe there.
fn organizer_point(layout: &WavetableEditLayout, row: i32, position: i32) -> Pos2 {
    let handle = layout.handle_width;
    // The first pixel whose position is `position` (positions truncate).
    let x = layout.organizer.x + (frame_width(layout) * position as f32).ceil() as i32 + handle / 2;
    let y = layout.organizer.y + row * handle + handle / 2;
    Pos2::new(x as f32, y as f32)
}

/// The playhead's point for `position` (`WavetablePlayhead::mouseEvent`).
fn playhead_point(layout: &WavetableEditLayout, position: i32) -> Pos2 {
    let playhead = layout.playhead;
    let padding = layout.handle_width as f32 / 2.0;
    let active_width = playhead.width as f32 - 2.0 * padding + 1.0;
    let x = playhead.x as f32 + padding + position as f32 * active_width / 256.0;
    Pos2::new(x.round(), (playhead.y + playhead.height / 2) as f32)
}

fn press(harness: &mut Harness, pos: Pos2, button: PointerButton, pressed: bool) {
    harness.event(Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

fn drag(harness: &mut Harness, from: Pos2, to: Pos2) {
    harness.event(Event::PointerMoved(from));
    harness.step();
    press(harness, from, PointerButton::Primary, true);
    let middle = Pos2::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
    for pos in [middle, to] {
        harness.event(Event::PointerMoved(pos));
        harness.step();
    }
    press(harness, to, PointerButton::Primary, false);
    harness.step();
}

fn double_click(harness: &mut Harness, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.step();
    for _ in 0..2 {
        press(harness, pos, PointerButton::Primary, true);
        press(harness, pos, PointerButton::Primary, false);
    }
    harness.step();
}

fn right_click(harness: &mut Harness, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    harness.step();
    press(harness, pos, PointerButton::Secondary, true);
    press(harness, pos, PointerButton::Secondary, false);
    harness.step();
}

/// Clicks row `row` of a menu whose top left is at `top_left`.
fn choose_row(harness: &mut Harness, top_left: Pos2, row: usize) {
    // Rows are 24 pixels from the top of the list, under the body rounding.
    let pos = Pos2::new(top_left.x + 20.0, top_left.y + 12.0 + 24.0 * row as f32);
    click(harness, pos);
    harness.step();
}

fn positions(fixture: &Fixture, group: usize, component: usize) -> Vec<i32> {
    fixture.synth.wavetable_creator(0).unwrap().groups[group].components[component]
        .keyframe_positions()
}

fn component_names(fixture: &Fixture) -> Vec<Vec<&'static str>> {
    fixture
        .synth
        .wavetable_creator(0)
        .unwrap()
        .groups
        .iter()
        .map(|group| {
            group
                .components
                .iter()
                .map(|component| component.component_type().name())
                .collect()
        })
        .collect()
}

fn log(fixture: &Fixture) -> Vec<String> {
    std::mem::take(&mut *fixture.recorder.log.lock().unwrap())
}

/// A wave source with keyframes at `positions` as oscillator 1's table.
fn keyframes_at(fixture: &Fixture, positions: &[i32]) {
    let mut creator = WavetableCreator::new();
    creator.clear();
    let mut group = WavetableGroup::new();
    let mut source = WavetableComponent::new(ComponentType::WaveSource);
    for &position in positions {
        source.insert_new_keyframe(position);
    }
    group.add_component(source);
    creator.add_group(group);
    fixture.synth.set_creator(0, creator);
}

/// `WavetablePlayhead::mouseDown` / `mouseDrag` set the wave frame slider:
/// one gesture for the drag (D18), and nothing when the value doesn't
/// change.
#[test]
fn dragging_the_playhead_moves_the_wave_frame_in_one_gesture() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    drag(
        &mut harness,
        playhead_point(&layout, 64),
        playhead_point(&layout, 128),
    );
    let log = log(&fixture);
    assert_eq!(
        log.first().map(String::as_str),
        Some("begin osc_1_wave_frame")
    );
    assert_eq!(log.last().map(String::as_str), Some("end osc_1_wave_frame"));
    assert_eq!(
        log.iter()
            .filter(|entry| entry.starts_with("begin"))
            .count(),
        1
    );
    assert!(log[1..log.len() - 1]
        .iter()
        .all(|entry| entry == "set osc_1_wave_frame"));
    assert_eq!(
        fixture
            .interface
            .borrow()
            .wavetable_edit_section(0)
            .playhead(),
        128
    );

    // The host's value is still 0 (the recorder applies nothing): a click
    // at 0 changes nothing.
    click(&mut harness, playhead_point(&layout, 0));
    assert!(log_is_empty(&fixture));
}

fn log_is_empty(fixture: &Fixture) -> bool {
    fixture.recorder.log.lock().unwrap().is_empty()
}

/// Dragging a keyframe (`mouseDrag`) moves it and the playhead with it;
/// the release (`mouseUp`) puts it in place and commits once.
#[test]
fn dragging_a_keyframe_moves_it_and_commits_on_release() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let from = organizer_point(&layout, 0, 0);
    let dx = (100.0 * frame_width(&layout)).round();
    let to = Pos2::new(from.x + dx, from.y);
    drag(&mut harness, from, to);
    let expected = (dx / frame_width(&layout)) as i32;
    assert_eq!(positions(&fixture, 0, 0), [expected]);
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
    assert_eq!(fixture.selected(0), [FIRST]);
    let log = log(&fixture);
    assert_eq!(
        log.first().map(String::as_str),
        Some("begin osc_1_wave_frame")
    );
    assert_eq!(log.last().map(String::as_str), Some("end osc_1_wave_frame"));
}

/// `mouseDoubleClick`, which JUCE sends after the second click's
/// `mouseUp`: in empty space it creates a keyframe and selects it, and on a
/// keyframe it deletes it.
#[test]
fn double_clicks_create_and_delete_keyframes() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    double_click(&mut harness, organizer_point(&layout, 0, 128));
    assert_eq!(positions(&fixture, 0, 0), [0, 128]);
    let created = KeyframeRef {
        keyframe: 1,
        ..FIRST
    };
    assert_eq!(fixture.selected(0), [created]);
    // Past JUCE's double-click timeout, so the next presses count afresh.
    for _ in 0..30 {
        harness.step();
    }
    double_click(&mut harness, organizer_point(&layout, 0, 128));
    assert_eq!(positions(&fixture, 0, 0), [0]);
    assert!(fixture.selected(0).is_empty());
    assert!(log_is_empty(&fixture));
}

/// JUCE's click count: a second press that drags (4 points or more) or is
/// held over 0.3 s is a single click (`isLongPressOrDrag`), and presses
/// with different modifiers don't chain, so none of these deletes the
/// keyframe.
#[test]
fn drags_long_presses_and_other_modifiers_are_not_double_clicks() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let from = organizer_point(&layout, 0, 0);
    let dx = (100.0 * frame_width(&layout)).round();
    let to = Pos2::new(from.x + dx, from.y);
    click(&mut harness, from);
    drag(&mut harness, from, to);
    let moved = (dx / frame_width(&layout)) as i32;
    assert_eq!(positions(&fixture, 0, 0), [moved]);
    assert_eq!(fixture.selected(0), [FIRST]);

    let wait = |harness: &mut Harness| {
        for _ in 0..30 {
            harness.step();
        }
    };
    wait(&mut harness);
    click(&mut harness, to);
    press(&mut harness, to, PointerButton::Primary, true);
    for _ in 0..20 {
        harness.step();
    }
    press(&mut harness, to, PointerButton::Primary, false);
    assert_eq!(positions(&fixture, 0, 0), [moved]);

    wait(&mut harness);
    click(&mut harness, to);
    harness.event(Event::ModifiersChanged(Modifiers::SHIFT));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::SHIFT,
        });
        harness.step();
    }
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    harness.step();
    assert_eq!(positions(&fixture, 0, 0), [moved]);
    assert_eq!(fixture.selected(0), [FIRST]);
}

/// The organizer's menu: "Create Keyframe" in empty space, "Remove
/// Keyframe" over a selected keyframe (`organizerCallback`).
#[test]
fn the_organizer_menu_creates_and_removes_keyframes() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let empty = organizer_point(&layout, 0, 200);
    right_click(&mut harness, empty);
    choose_row(&mut harness, empty, 0);
    assert_eq!(positions(&fixture, 0, 0), [0, 200]);
    let created = KeyframeRef {
        keyframe: 1,
        ..FIRST
    };
    assert_eq!(fixture.selected(0), [created]);

    right_click(&mut harness, empty);
    choose_row(&mut harness, empty, 0);
    assert_eq!(positions(&fixture, 0, 0), [0]);
    assert!(fixture.selected(0).is_empty());
}

/// A drag from empty space selects the keyframes in its box.
#[test]
fn a_selection_box_selects_the_keyframes_in_it() {
    let fixture = Fixture::new();
    keyframes_at(&fixture, &[0, 128, 256]);
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    drag(
        &mut harness,
        organizer_point(&layout, 0, 90),
        organizer_point(&layout, 0, 180),
    );
    let middle = KeyframeRef {
        keyframe: 1,
        ..FIRST
    };
    assert_eq!(fixture.selected(0), [middle]);
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 0);
}

/// "Add Source" and "Add Modifier" (`addSource`, `addComponent`), then the
/// row menus: the selection follows a moved group, and a removed modifier
/// leaves the list.
#[test]
fn the_component_list_adds_moves_and_removes() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let top = Scope::top(&fixture.skin, fixture.layout().size_ratio);

    let add_source = layout.add_button(&top, 2, false);
    click(&mut harness, centre(add_source));
    choose_row(
        &mut harness,
        Pos2::new(add_source.x as f32, add_source.bottom() as f32),
        1,
    );
    assert_eq!(
        component_names(&fixture),
        [vec!["Wave Source"], vec!["Line Source"]]
    );
    let line = KeyframeRef { group: 1, ..FIRST };
    assert_eq!(fixture.selected(0), [line]);

    let add_modifier = layout.add_button(&top, 1, true);
    click(&mut harness, centre(add_modifier));
    choose_row(
        &mut harness,
        Pos2::new(add_modifier.x as f32, add_modifier.bottom() as f32),
        4,
    );
    assert_eq!(
        component_names(&fixture),
        [vec!["Wave Source", "Wave Folder"], vec!["Line Source"]]
    );

    // The Line Source's row is now row 3: "Move Group Up", "Reset Source",
    // "Remove Group".
    let menu = layout.component_menu(3);
    click(&mut harness, centre(menu));
    choose_row(
        &mut harness,
        Pos2::new(menu.x as f32, menu.bottom() as f32),
        0,
    );
    assert_eq!(
        component_names(&fixture),
        [vec!["Line Source"], vec!["Wave Source", "Wave Folder"]]
    );
    assert_eq!(fixture.selected(0), [FIRST]);

    // The Wave Folder's row is now row 3: "Reset", "Remove".
    let menu = layout.component_menu(3);
    click(&mut harness, centre(menu));
    choose_row(
        &mut harness,
        Pos2::new(menu.x as f32, menu.bottom() as f32),
        1,
    );
    assert_eq!(
        component_names(&fixture),
        [vec!["Line Source"], vec!["Wave Source"]]
    );
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 4);
    assert!(log_is_empty(&fixture));
}

/// `frameSelected`: showing the editor selects the first keyframe, so the
/// Wave Source overlay shows it; a click on empty space deselects and hides
/// it; a press on the keyframe selects it again; a selection box that takes
/// it hides the overlay, as `selectFrames` deselects first.
#[test]
fn the_overlay_follows_the_frame_selection() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    assert_eq!(fixture.overlay_frame(0), Some(FIRST));

    click(&mut harness, organizer_point(&layout, 0, 128));
    assert_eq!(fixture.overlay_frame(0), None);
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, organizer_point(&layout, 0, 0));
    assert_eq!(fixture.overlay_frame(0), Some(FIRST));
    for _ in 0..30 {
        harness.step();
    }

    let from = organizer_point(&layout, 0, 60);
    let to = Pos2::new(
        organizer_point(&layout, 0, 0).x - layout.handle_width as f32,
        from.y,
    );
    drag(&mut harness, from, to);
    assert_eq!(fixture.selected(0), [FIRST]);
    assert_eq!(fixture.overlay_frame(0), None);
}

/// `WaveSourceOverlay::sliderValueChanged`: choosing "Spectral Blend" in the
/// interpolation selector gives the source linear interpolation in the
/// frequency domain and commits once; nothing reaches the host.
#[test]
fn the_interpolation_selector_sets_the_sources_interpolation() {
    let fixture = Fixture::new();
    // The init source interpolates in frequency; start it in time so the
    // choice shows.
    let mut creator = fixture.synth.wavetable_creator(0).unwrap();
    if let WavetableComponent::WaveSource(source) = &mut creator.groups[0].components[0] {
        source.interpolation_mode = 0;
        source.keyframes.interpolation_style = 0;
    }
    fixture.synth.set_creator(0, creator);
    let mut harness = opened(&fixture);
    let overlay = fixture.overlay_layout();
    click(&mut harness, centre(overlay.interpolation));
    let top_left = Pos2::new(
        overlay.interpolation.x as f32,
        overlay.interpolation.bottom() as f32,
    );
    choose_row(&mut harness, top_left, 2);
    for _ in 0..3 {
        harness.step();
    }

    let controls = WAVE_SOURCE_CONTROLS[0][0];
    assert_eq!(fixture.local(controls.interpolation), 2.0);
    let creator = fixture.synth.wavetable_creator(0).unwrap();
    let WavetableComponent::WaveSource(source) = &creator.groups[0].components[0] else {
        panic!("the init preset starts with a wave source");
    };
    assert_eq!(source.keyframes.interpolation_style, 1);
    assert_eq!(source.interpolation_mode, 1);
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
    assert!(log_is_empty(&fixture));
}

/// The grid sizes' `IncrementerButtons` are under the always-on-top grid
/// sliders, which take the press: a click on either half leaves the size
/// alone.
#[test]
fn the_grid_incrementers_are_inert() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let overlay = fixture.overlay_layout();
    let controls = WAVE_SOURCE_CONTROLS[0][0];
    click(&mut harness, centre(overlay.grid_x_buttons[0]));
    for _ in 0..30 {
        harness.step();
    }
    click(&mut harness, centre(overlay.grid_y_buttons[1]));
    assert_eq!(fixture.local(controls.grid_x), 6.0);
    assert_eq!(fixture.local(controls.grid_y), 4.0);
    assert!(log_is_empty(&fixture));
}

/// After a selection box takes one keyframe (the overlay hidden), a
/// component list edit selects it again (`componentsChanged`,
/// `recreateVisibleFrames`, `selectFrame`), so the overlay comes back.
#[test]
fn a_list_edit_reselects_a_boxed_keyframe() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let from = organizer_point(&layout, 0, 60);
    let to = Pos2::new(
        organizer_point(&layout, 0, 0).x - layout.handle_width as f32,
        from.y,
    );
    drag(&mut harness, from, to);
    assert_eq!(fixture.selected(0), [FIRST]);
    assert_eq!(fixture.overlay_frame(0), None);

    let top = Scope::top(&fixture.skin, fixture.layout().size_ratio);
    let add_modifier = layout.add_button(&top, 1, true);
    click(&mut harness, centre(add_modifier));
    choose_row(
        &mut harness,
        Pos2::new(add_modifier.x as f32, add_modifier.bottom() as f32),
        4,
    );
    assert_eq!(fixture.selected(0), [FIRST]);
    assert_eq!(fixture.overlay_frame(0), Some(FIRST));
}

/// The gear over the amplitudes opens upstream's bar editor menu, which
/// sets the amplitudes' scale and the frequency displays' zoom
/// (`barViewerCallback`).
#[test]
fn the_gear_menu_sets_the_scale_and_the_zoom() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let settings = fixture.editor_layout().settings;
    let below = Pos2::new(settings.x as f32, settings.bottom() as f32);
    let state = |fixture: &Fixture| {
        let interface = fixture.interface.borrow();
        let section = interface.wavetable_edit_section(0);
        (section.power_scale(), section.zoom())
    };
    // The section starts on the power scale at 8x (`zoom_(8)`).
    assert_eq!(state(&fixture), (true, 8.0));
    click(&mut harness, centre(settings));
    choose_row(&mut harness, below, 1);
    assert_eq!(state(&fixture), (false, 8.0));
    click(&mut harness, centre(settings));
    // Power Scale, Amplitude Scale, a divider, then Zoom 1x to 16x.
    choose_row(&mut harness, below, 4);
    assert_eq!(state(&fixture), (false, 2.0));
    click(&mut harness, centre(settings));
    choose_row(&mut harness, below, 7);
    assert_eq!(state(&fixture), (false, 16.0));
    click(&mut harness, centre(settings));
    choose_row(&mut harness, below, 0);
    assert_eq!(state(&fixture), (true, 16.0));
    assert!(fixture.recorder.log.lock().unwrap().is_empty());
}

/// A data folder with two wavetables, and an added folder with a third.
fn wavetable_folders(fixture: &Fixture, tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("sas-editor-tables-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for file in [
        "Data/Wavetables/Alpha.vitaltable",
        "Data/Wavetables/Beta.vitaltable",
        "Added/Gamma.vitaltable",
    ] {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"version":"1.0.0","name":"Inside","groups":[]}"#).unwrap();
    }
    *fixture.synth.data_directory.lock().unwrap() = Some(root.join("Data"));
    *fixture.synth.added_folders.lock().unwrap() = vec![root.join("Added")];
    root
}

fn loads(fixture: &Fixture) -> Vec<String> {
    std::mem::take(&mut *fixture.synth.loads.lock().unwrap())
}

/// The name's arrows load the wavetable before or after the creator's last
/// file among the user's and the added folders' (`prevClicked`,
/// `nextClicked`, `getShiftedFile`), the first with none.
#[test]
fn the_editor_arrows_step_from_the_last_file() {
    let fixture = Fixture::new();
    let root = wavetable_folders(&fixture, "arrows");
    let mut harness = opened(&fixture);
    let preset = fixture.editor_layout().preset;
    click(&mut harness, centre(preset.next));
    assert_eq!(loads(&fixture), ["0 Gamma"]);
    click(&mut harness, centre(preset.next));
    assert_eq!(loads(&fixture), ["0 Alpha"]);
    click(&mut harness, centre(preset.previous));
    assert_eq!(loads(&fixture), ["0 Gamma"]);
    assert_eq!(
        fixture.synth.last_wavetable_file(0),
        Some(root.join("Added/Gamma.vitaltable"))
    );
    std::fs::remove_dir_all(&root).unwrap();
}

/// A press on the name opens the popup browser for the oscillator's
/// wavetables below it (`textMouseDown`), listing the user's and the added
/// folders' with the last file selected; a file chosen in it loads.
#[test]
fn the_editor_name_opens_the_browser() {
    let fixture = Fixture::new();
    let root = wavetable_folders(&fixture, "browser");
    fixture
        .synth
        .set_last_wavetable_file(0, root.join("Data/Wavetables/Beta.vitaltable"));
    let mut harness = opened(&fixture);
    let preset = fixture.editor_layout().preset;
    click(&mut harness, centre(preset.text));
    let (folders, files) = fixture.interface.borrow().popup_browser_rows().unwrap();
    assert!(folders.contains(&Selection::Path(root.join("Added"))));
    assert_eq!(
        files,
        [
            Selection::Path(root.join("Added/Gamma.vitaltable")),
            Selection::Path(root.join("Data/Wavetables/Alpha.vitaltable")),
            Selection::Path(root.join("Data/Wavetables/Beta.vitaltable")),
        ]
    );

    let layout = fixture.layout();
    let top = Scope::top(&fixture.skin, layout.size_ratio);
    let padding = top.within(popup_browser::SKIN).value(ValueId::Padding);
    let browser = Rect::new(preset.bounds.x, preset.bounds.bottom(), 600, 400);
    let files = PopupBrowserLayout::new(1.0, padding, browser).files;
    click(
        &mut harness,
        Pos2::new(
            files.x as f32 + files.width as f32 / 2.0,
            (files.y + 24 + 12) as f32,
        ),
    );
    assert_eq!(loads(&fixture), ["0 Alpha"]);
    std::fs::remove_dir_all(&root).unwrap();
}

/// `OscillatorSection::loadFile` sends only a `.vitaltable` through the
/// editor's `loadFile`, which records the creator's last file; audio loads
/// from the oscillator directly (`loadAudioAsWavetable`), unrecorded.
#[test]
fn the_oscillators_audio_loads_arent_the_last_file() {
    let fixture = Fixture::new();
    let root = wavetable_folders(&fixture, "oscillator-last");
    let wav = root.join("Data/Wavetables/Loop.wav");
    std::fs::write(&wav, "").unwrap();
    let model = fixture.synth.as_ref();
    assert!(popup_browser::load_wavetable_file(model, 0, &wav));
    assert_eq!(loads(&fixture), ["0 audio Loop.wav"]);
    assert_eq!(model.last_wavetable_file(0), None);
    let table = root.join("Data/Wavetables/Alpha.vitaltable");
    assert!(popup_browser::load_wavetable_file(model, 0, &table));
    assert_eq!(model.last_wavetable_file(0), Some(table));
    std::fs::remove_dir_all(&root).unwrap();
}

/// The editor's own loads take every file that isn't a `.vitaltable` as
/// audio, a `.flac` and an upper-case `.WAV` included, where upstream's
/// takes only `.wav` (D32); each becomes the creator's last file.
#[test]
fn the_editor_loads_flac_and_upper_case_wav_as_audio() {
    let fixture = Fixture::new();
    let root = std::env::temp_dir().join(format!("sas-editor-audio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let folder = root.join("Wavetables");
    std::fs::create_dir_all(&folder).unwrap();
    for file in ["Delta.flac", "Echo.WAV"] {
        std::fs::write(folder.join(file), "").unwrap();
    }
    *fixture.synth.data_directory.lock().unwrap() = Some(root.clone());
    let mut harness = opened(&fixture);
    let preset = fixture.editor_layout().preset;
    click(&mut harness, centre(preset.next));
    assert_eq!(loads(&fixture), ["0 audio Delta.flac"]);
    assert_eq!(
        fixture.synth.last_wavetable_file(0),
        Some(folder.join("Delta.flac"))
    );
    click(&mut harness, centre(preset.next));
    assert_eq!(loads(&fixture), ["0 audio Echo.WAV"]);
    assert_eq!(
        fixture.synth.last_wavetable_file(0),
        Some(folder.join("Echo.WAV"))
    );
    std::fs::remove_dir_all(&root).unwrap();
}

/// The title menu (`showPopupMenu`): "Save As Wavetable" opens the save
/// section for the wavetable over the editor (`saveWavetable`), and
/// "Synthesize Preset to Table" renders the patch into the oscillator's
/// wavetable (`resynthesizeToWavetable`).
#[test]
fn the_title_menu_saves_and_resynthesizes() {
    let fixture = Fixture::new();
    let root = wavetable_folders(&fixture, "title-menu");
    let mut harness = opened(&fixture);
    let menu = fixture.editor_layout().menu;
    let below = Pos2::new(menu.x as f32, menu.bottom() as f32);
    click(&mut harness, centre(menu));
    choose_row(&mut harness, below, 4);
    assert_eq!(*fixture.synth.resyntheses.lock().unwrap(), [0]);

    assert!(!fixture.interface.borrow().save_section_state().0);
    click(&mut harness, centre(menu));
    choose_row(&mut harness, below, 0);
    harness.step();
    assert!(fixture.interface.borrow().save_section_state().0);
    assert_eq!(fixture.editor(), Some(0));
    std::fs::remove_dir_all(&root).unwrap();
}

/// "Export Wavetable" writes the wavetable's JSON with the wavetable
/// extension, and "Export to .wav File" 256 rendered frames at 88.2 kHz
/// with the frame size in the `clm ` chunk, each in place of the extension
/// the dialog gave (`withFileExtension`).
#[test]
fn the_exports_write_the_json_and_the_rendered_frames() {
    let fixture = Fixture::new();
    let dir = std::env::temp_dir().join(format!("sas-editor-exports-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let model = fixture.synth.as_ref();
    wavetable_edit::export_wavetable(model, 0, &dir.join("Table.txt")).unwrap();
    let written = std::fs::read_to_string(dir.join("Table.vitaltable")).unwrap();
    assert_eq!(written, model.wavetable_json(0).unwrap());

    wavetable_edit::export_wav(model, 0, &dir.join("Frames")).unwrap();
    let bytes = std::fs::read(dir.join("Frames.wav")).unwrap();
    assert_eq!(bytes.len(), 154 + 256 * 2048 * 2);
    assert_eq!(
        &bytes[104..145],
        sas_gui::wavetable_export::clm_text().as_bytes()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Oscillator 1's first keyframe's waveform, as committed.
fn committed_samples(fixture: &Fixture) -> Vec<f32> {
    let creator = fixture.synth.wavetable_creator(0).unwrap();
    let WavetableComponent::WaveSource(source) = &creator.groups[0].components[0] else {
        panic!("the init preset starts with a wave source");
    };
    source.keyframes.keyframes[0].data.wave_frame.time_domain[..2048].to_vec()
}

/// A point in oscillator 1's waveform display, from its top left.
fn waveform_point(layout: &WavetableEditLayout, x: i32, y: i32) -> Pos2 {
    Pos2::new(
        (layout.waveform.x + x) as f32,
        (layout.waveform.y + y) as f32,
    )
}

/// `WaveSourceEditor`'s drawing with the default 6 by 4 grid: a press
/// snaps to the nearest grid point, a move that isn't near one draws
/// nothing new, and reaching the next draws the line between them. Each
/// change auditions the frame under the playhead (`frameChanged`); the
/// release commits once (`frameDoneEditing`), with nothing ended and
/// nothing written to the host.
#[test]
fn drawing_on_the_waveform_snaps_to_the_grid_and_commits_on_release() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let (width, height) = (layout.waveform.width, layout.waveform.height);
    // getSnappedPoint: width * round(x) / grid, truncated.
    let column = |i: i32| (width as f32 * i as f32 / 6.0) as i32;
    let row = (height as f32 / 4.0) as i32;
    let from = waveform_point(&layout, column(3) + 2, row + 1);
    let to = waveform_point(&layout, column(4) - 1, row - 2);
    drag(&mut harness, from, to);

    let samples = committed_samples(&fixture);
    let expected = -2.0 * row as f32 / height as f32 + 1.0;
    let index = |x: i32| (2048.0 * x as f32 / width as f32).floor() as usize;
    for (i, &sample) in samples
        .iter()
        .enumerate()
        .take(index(column(4)))
        .skip(index(column(3)))
    {
        assert_eq!(sample, expected, "sample {i}");
    }
    assert_ne!(samples[index(column(4)) + 10], expected);
    let auditions = fixture.synth.auditions.lock().unwrap();
    assert!(!auditions.is_empty());
    assert!(auditions
        .iter()
        .all(|(oscillator, position, _)| (*oscillator, *position) == (0, 0)));
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
    assert!(fixture.synth.ends.lock().unwrap().is_empty());
    assert!(log_is_empty(&fixture));
}

/// Without a grid, a press draws where it is.
#[test]
fn without_a_grid_drawing_follows_the_pointer() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let overlay = fixture.overlay_layout();
    // Down the grid size's slider, to 0.
    let grid = centre(overlay.grid_x);
    drag(&mut harness, grid, grid + egui::vec2(0.0, 400.0));
    assert_eq!(fixture.local(WAVE_SOURCE_CONTROLS[0][0].grid_x), 0.0);
    let layout = fixture.editor_layout();
    let width = layout.waveform.width;
    let point = waveform_point(&layout, width / 3, 0);
    click(&mut harness, point);
    let index = (2048.0 * (width / 3) as f32 / width as f32).floor() as usize;
    assert_eq!(committed_samples(&fixture)[index], 1.0);
}

/// The waveform's menu: Flip Vertical negates the keyframe's waveform.
/// The right-click's release commits first, as upstream's `mouseUp` reports
/// `valuesChanged` after a popup-menu press too, then the choice commits.
#[test]
fn flip_vertical_negates_the_waveform() {
    let fixture = Fixture::new();
    let before = committed_samples(&fixture);
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let press = centre(layout.waveform);
    right_click(&mut harness, press);
    choose_row(&mut harness, press, 1);
    let after = committed_samples(&fixture);
    for (b, a) in before.iter().zip(&after) {
        assert_eq!(*a, -*b);
    }
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 2);
}

/// The amplitudes' menu: Clear sets every harmonic to nothing, so the
/// frame falls silent; a right-click on the bars commits nothing itself.
#[test]
fn clearing_the_amplitudes_silences_the_frame() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let press = centre(layout.amplitudes);
    right_click(&mut harness, press);
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 0);
    choose_row(&mut harness, press, 0);
    assert!(committed_samples(&fixture)
        .iter()
        .all(|&sample| sample == 0.0));
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
}

/// Drawing on the amplitudes auditions while dragging and commits on
/// release, the frame rebuilt from the bars and normalised.
#[test]
fn drawing_on_the_amplitudes_rebuilds_the_frame() {
    let fixture = Fixture::new();
    let before = committed_samples(&fixture);
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let bars = layout.amplitudes;
    // Below the gear in the display's corner.
    let y = (bars.y + layout.settings.height + 4) as f32;
    let from = Pos2::new((bars.x + 3) as f32, y);
    let to = Pos2::new((bars.x + 60) as f32, y);
    drag(&mut harness, from, to);
    let after = committed_samples(&fixture);
    assert_ne!(after, before);
    let peak = after
        .iter()
        .fold(0.0f32, |peak, &sample| peak.max(sample.abs()));
    assert!((peak - 1.0).abs() < 1e-3, "{peak}");
    assert!(!fixture.synth.auditions.lock().unwrap().is_empty());
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
}

/// A window that closes during a drag ends its audition
/// (`EditorModel::end_wavetable_audition`), once; a committed drag leaves
/// nothing to end.
#[test]
fn closing_during_a_drag_ends_the_audition() {
    let fixture = Fixture::new();
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let row = layout.waveform.height / 2;
    let from = waveform_point(&layout, 5, row);
    let to = waveform_point(&layout, layout.waveform.width / 3, row);
    drag(&mut harness, from, to);
    let auditions = fixture.interface.borrow().pending_auditions();
    for audition in &auditions {
        audition.end(fixture.synth.as_ref());
    }
    assert!(fixture.synth.ends.lock().unwrap().is_empty());

    for _ in 0..30 {
        harness.step();
    }
    harness.event(Event::PointerMoved(from));
    harness.step();
    press(&mut harness, from, PointerButton::Primary, true);
    harness.event(Event::PointerMoved(to));
    harness.step();
    assert!(!fixture.synth.auditions.lock().unwrap().is_empty());
    for audition in &auditions {
        audition.end(fixture.synth.as_ref());
        audition.end(fixture.synth.as_ref());
    }
    assert_eq!(*fixture.synth.ends.lock().unwrap(), [0]);
}

/// A drag whose commit is refused at its release (a full edit queue) ends
/// its audition rather than leave the drawn frames playing, and the editor
/// reads the saved creator back.
#[test]
fn a_refused_commit_ends_the_audition() {
    let fixture = Fixture::new();
    let before = committed_samples(&fixture);
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    fixture.synth.refuse_commits.store(true, Ordering::Relaxed);
    let row = layout.waveform.height / 2;
    drag(
        &mut harness,
        waveform_point(&layout, 5, row),
        waveform_point(&layout, layout.waveform.width / 3, row),
    );
    harness.step();
    assert!(!fixture.synth.auditions.lock().unwrap().is_empty());
    assert_eq!(*fixture.synth.ends.lock().unwrap(), [0]);
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 0);
    assert_eq!(committed_samples(&fixture), before);
    let interface = fixture.interface.borrow();
    let creator = interface.wavetable_edit_section(0).creator().unwrap();
    let WavetableComponent::WaveSource(source) = &creator.groups[0].components[0] else {
        panic!("the init preset starts with a wave source");
    };
    assert_eq!(
        source.keyframes.keyframes[0].data.wave_frame.time_domain[..2048],
        before[..]
    );
    // Nothing is left to end.
    for audition in interface.pending_auditions() {
        audition.end(fixture.synth.as_ref());
    }
    assert_eq!(*fixture.synth.ends.lock().unwrap(), [0]);
}

/// The controls background lies over the top of the amplitudes and takes
/// the presses there, as upstream's `ControlsBackground` on top of the bar
/// editor does: a press on a title strip, or in a gap of the background,
/// draws no bars and commits nothing.
#[test]
fn the_controls_background_takes_presses_over_the_bars() {
    let fixture = Fixture::new();
    let before = committed_samples(&fixture);
    let mut harness = opened(&fixture);
    let layout = fixture.editor_layout();
    let overlay = fixture.overlay_layout();
    let title = overlay.titles[1];
    let y = title.y.max(layout.amplitudes.y) + 2;
    assert!(y < title.bottom());
    click(&mut harness, Pos2::new(centre(title).x, y as f32));
    for _ in 0..30 {
        harness.step();
    }
    // The gap between the interpolation selector and the grid sizes.
    let gap = Pos2::new(
        (overlay.interpolation.right() + 1) as f32,
        centre(overlay.grid_x).y,
    );
    drag(&mut harness, gap, gap + egui::vec2(0.0, 2.0));
    assert!(fixture.synth.auditions.lock().unwrap().is_empty());
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 0);
    assert_eq!(committed_samples(&fixture), before);
}

/// The reference tool's `--wavetable-select`: a keyframe asked for before
/// the editor shows is selected once it reads the creator, in place of the
/// default first keyframe, and the overlays hear of it; one that doesn't
/// exist leaves the default.
#[test]
fn a_requested_keyframe_is_selected_when_the_editor_shows() {
    let fixture = Fixture::new();
    let mut creator = fixture.synth.wavetable_creator(0).unwrap();
    creator.groups[0].components[0].insert_new_keyframe(128);
    fixture.synth.set_creator(0, creator);
    let second = KeyframeRef {
        group: 0,
        component: 0,
        keyframe: 1,
    };
    fixture
        .interface
        .borrow_mut()
        .select_wavetable_keyframe(0, second);
    let mut harness = opened(&fixture);
    harness.step();
    assert_eq!(fixture.selected(0), [second]);
    assert_eq!(fixture.overlay_frame(0), Some(second));
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 0);

    let other = Fixture::new();
    other.interface.borrow_mut().select_wavetable_keyframe(
        0,
        KeyframeRef {
            group: 3,
            component: 0,
            keyframe: 0,
        },
    );
    let mut harness = opened(&other);
    harness.step();
    assert_eq!(other.selected(0), [FIRST]);
}

/// A wave source, a Phase Shift and a Wave Window, each with a keyframe at
/// 0, as oscillator 1's table, with the editor open on the modifier in row
/// `row` (its keyframe selected, so its overlay shows).
fn modifier_selected(fixture: &Fixture, row: usize) -> Harness<'_> {
    let mut creator = WavetableCreator::new();
    creator.clear();
    let mut group = WavetableGroup::new();
    for kind in [
        ComponentType::WaveSource,
        ComponentType::PhaseModifier,
        ComponentType::WaveWindow,
    ] {
        let mut component = WavetableComponent::new(kind);
        component.insert_new_keyframe(0);
        group.add_component(component);
    }
    creator.add_group(group);
    fixture.synth.set_creator(0, creator);
    let mut harness = opened(fixture);
    let layout = fixture.editor_layout();
    click(&mut harness, organizer_point(&layout, row as i32, 0));
    let keyframe = KeyframeRef {
        group: 0,
        component: row,
        keyframe: 0,
    };
    assert_eq!(fixture.overlay_frame(0), Some(keyframe));
    harness
}

fn phase_keyframe(fixture: &Fixture) -> (f32, f32, i32) {
    let creator = fixture.synth.wavetable_creator(0).unwrap();
    let WavetableComponent::PhaseModifier(modifier) = &creator.groups[0].components[1] else {
        panic!("row 1 is the Phase Shift");
    };
    let data = &modifier.keyframes.keyframes[0].data;
    (data.phase, data.mix, modifier.settings.phase_style)
}

fn window_keyframe(fixture: &Fixture) -> (f32, f32) {
    let creator = fixture.synth.wavetable_creator(0).unwrap();
    let WavetableComponent::WaveWindow(modifier) = &creator.groups[0].components[2] else {
        panic!("row 2 is the Wave Window");
    };
    let data = &modifier.keyframes.keyframes[0].data;
    (data.left_position, data.right_position)
}

fn phase_layout(fixture: &Fixture) -> PhaseLayout {
    let top = Scope::top(&fixture.skin, fixture.layout().size_ratio);
    PhaseLayout::new(&top, &fixture.editor_layout())
}

/// The auditions so far all played oscillator 1's playhead frame (0), and
/// there was at least one.
fn auditioned_the_playhead(fixture: &Fixture) -> bool {
    let auditions = fixture.synth.auditions.lock().unwrap();
    !auditions.is_empty()
        && auditions
            .iter()
            .all(|(oscillator, position, _)| (*oscillator, *position) == (0, 0))
}

/// `PhaseEditor` over the waveform: a drag turns the phase by a whole
/// turn per display width (`updatePhase`), auditioning as it goes
/// (`phaseChanged(phase, false)`), and the release commits once.
#[test]
fn dragging_over_the_waveform_shifts_the_phase() {
    let fixture = Fixture::new();
    let mut harness = modifier_selected(&fixture, 1);
    let layout = fixture.editor_layout();
    let width = layout.waveform.width;
    let y = layout.waveform.height / 2;
    drag(
        &mut harness,
        waveform_point(&layout, width / 4, y),
        waveform_point(&layout, width / 4 + width / 8, y),
    );

    let (phase, mix, _) = phase_keyframe(&fixture);
    let expected = 2.0 * std::f32::consts::PI * (width / 8) as f32 / width as f32;
    assert!((phase - expected).abs() < 1e-5, "{phase} != {expected}");
    assert_eq!(mix, 1.0);
    assert!(auditioned_the_playhead(&fixture));
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
    assert!(fixture.synth.ends.lock().unwrap().is_empty());
    assert!(log_is_empty(&fixture));
}

/// The controls' phase editor turns the same phase, by a turn per its own
/// width.
#[test]
fn the_controls_phase_editor_shifts_the_phase() {
    let fixture = Fixture::new();
    let mut harness = modifier_selected(&fixture, 1);
    let slider = phase_layout(&fixture).slider;
    let from = centre(slider);
    let to = Pos2::new(from.x - (slider.width / 4) as f32, from.y);
    drag(&mut harness, from, to);

    let (phase, _, _) = phase_keyframe(&fixture);
    let expected = -2.0 * std::f32::consts::PI * (slider.width / 4) as f32 / slider.width as f32;
    assert!((phase - expected).abs() < 1e-5, "{phase} != {expected}");
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
    assert!(log_is_empty(&fixture));
}

/// The mix slider (`sliderValueChanged`) auditions while it drags and
/// commits once when the drag ends (`sliderDragEnded`); it's a local
/// control, so the host hears nothing.
#[test]
fn the_mix_slider_commits_when_its_drag_ends() {
    let fixture = Fixture::new();
    let mut harness = modifier_selected(&fixture, 1);
    let mix = phase_layout(&fixture).mix;
    let from = centre(mix);
    let to = Pos2::new(mix.x as f32 + 2.0, from.y);
    drag(&mut harness, from, to);

    let (phase, mix_value, _) = phase_keyframe(&fixture);
    assert_eq!(phase, 0.0);
    assert!(mix_value < 1.0);
    assert_eq!(fixture.local(MODIFIER_CONTROLS[0].phase_mix), mix_value);
    assert!(auditioned_the_playhead(&fixture));
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
    assert!(log_is_empty(&fixture));
}

/// The style selector sets the modifier's phase style and commits once.
#[test]
fn the_style_selector_sets_the_phase_style() {
    let fixture = Fixture::new();
    let mut harness = modifier_selected(&fixture, 1);
    let style = phase_layout(&fixture).style;
    click(&mut harness, centre(style));
    choose_row(
        &mut harness,
        Pos2::new(style.x as f32, style.bottom() as f32),
        2,
    );
    for _ in 0..3 {
        harness.step();
    }

    assert_eq!(fixture.local(MODIFIER_CONTROLS[0].phase_style), 2.0);
    assert_eq!(phase_keyframe(&fixture).2, 2);
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
    assert!(log_is_empty(&fixture));
}

/// `WaveWindowEditor`: a press near an edge grabs it (`getHover`) and the
/// drag moves it, auditioning; the release commits once, and the position
/// sliders follow (`windowChanged`).
#[test]
fn dragging_a_window_edge_moves_it() {
    let fixture = Fixture::new();
    let mut harness = modifier_selected(&fixture, 2);
    let layout = fixture.editor_layout();
    let width = layout.waveform.width;
    let y = layout.waveform.height / 2;
    let left = (0.25 * width as f32) as i32;
    let to = width / 2;
    drag(
        &mut harness,
        waveform_point(&layout, left + 1, y),
        waveform_point(&layout, to, y),
    );

    let (left_position, right_position) = window_keyframe(&fixture);
    assert_eq!(left_position, to as f32 / width as f32);
    assert_eq!(right_position, 0.75);
    let controls = MODIFIER_CONTROLS[0];
    assert_eq!(fixture.local(controls.window_left), left_position);
    assert_eq!(fixture.local(controls.window_right), 0.75);
    assert!(auditioned_the_playhead(&fixture));
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
    assert!(log_is_empty(&fixture));
}

/// A press away from both edges grabs nothing: no audition, no commit.
#[test]
fn a_press_between_the_edges_grabs_nothing() {
    let fixture = Fixture::new();
    let mut harness = modifier_selected(&fixture, 2);
    let layout = fixture.editor_layout();
    let width = layout.waveform.width;
    let y = layout.waveform.height / 2;
    drag(
        &mut harness,
        waveform_point(&layout, width / 2, y),
        waveform_point(&layout, width / 2 + 20, y),
    );

    assert_eq!(window_keyframe(&fixture), (0.25, 0.75));
    assert!(fixture.synth.auditions.lock().unwrap().is_empty());
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 0);
}

/// The shape selector sets the modifier's window shape and commits once.
#[test]
fn the_shape_selector_sets_the_window_shape() {
    let fixture = Fixture::new();
    let mut harness = modifier_selected(&fixture, 2);
    let top = Scope::top(&fixture.skin, fixture.layout().size_ratio);
    let shape = WindowLayout::new(&top, &fixture.editor_layout()).shape;
    click(&mut harness, centre(shape));
    choose_row(
        &mut harness,
        Pos2::new(shape.x as f32, shape.bottom() as f32),
        3,
    );
    for _ in 0..3 {
        harness.step();
    }

    assert_eq!(fixture.local(MODIFIER_CONTROLS[0].window_shape), 3.0);
    let creator = fixture.synth.wavetable_creator(0).unwrap();
    let WavetableComponent::WaveWindow(modifier) = &creator.groups[0].components[2] else {
        panic!("row 2 is the Wave Window");
    };
    assert_eq!(modifier.settings.window_shape, 3);
    assert_eq!(fixture.synth.commits.load(Ordering::Relaxed), 1);
    assert!(log_is_empty(&fixture));
}
