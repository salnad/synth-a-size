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
use sas_engine::sas_oscillators::sas_wavetable::creator::component::{
    ComponentType, WavetableComponent,
};
use sas_engine::sas_oscillators::sas_wavetable::creator::group::WavetableGroup;
use sas_engine::sas_oscillators::sas_wavetable::creator::WavetableCreator;
use sas_gui::layout::{FullLayout, Rect};
use sas_gui::local::WAVE_SOURCE_CONTROLS;
use sas_gui::scope::Scope;
use sas_gui::sections::oscillator::{self, OscillatorLayout};
use sas_gui::sections::popup_browser::{self, PopupBrowserLayout};
use sas_gui::sections::selection_list::Selection;
use sas_gui::sections::synthesis::SynthesisLayout;
use sas_gui::sections::wave_source_overlay::OverlayLayout;
use sas_gui::sections::wavetable_edit::{KeyframeRef, WavetableEditLayout};
use sas_gui::skin::{Skin, ValueId};
use sas_gui::{EditorModel, Host, HostParam, Interface, OpenGestures, Tab};
use sas_preset::registry::registry;
use sas_preset::user_folders::Kind;

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
        self.commits.fetch_add(1, Ordering::Relaxed);
        self.set_creator(oscillator, creator);
        true
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
