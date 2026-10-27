// SPDX-License-Identifier: GPL-3.0-or-later

//! The wavetable editor with a recording synth: an oscillator's edit button
//! opens it in place of the interface, and its exit button closes it, with
//! nothing written to the host either way.

use std::cell::RefCell;
use std::collections::BTreeMap;
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
use sas_gui::scope::Scope;
use sas_gui::sections::oscillator::{self, OscillatorLayout};
use sas_gui::sections::synthesis::SynthesisLayout;
use sas_gui::sections::wavetable_edit::{KeyframeRef, WavetableEditLayout};
use sas_gui::skin::Skin;
use sas_gui::{EditorModel, Host, HostParam, Interface, OpenGestures, Tab};
use sas_preset::registry::registry;

/// Every registry control, and a wavetable creator per oscillator.
struct Synth {
    params: BTreeMap<&'static str, FloatParam>,
    creators: Mutex<Vec<WavetableCreator>>,
    revision: AtomicU64,
    generation: AtomicU64,
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

    fn content_revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    fn preset_generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
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
