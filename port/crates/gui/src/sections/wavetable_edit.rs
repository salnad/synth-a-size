// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_gui_basics, GPL-3.0-only).
// SPDX-License-Identifier: GPL-3.0-or-later AND GPL-3.0-only

//! The wavetable editor, shown in place of the interface when an
//! oscillator's edit button is clicked. Ports `WavetableEditSection`
//! (`vital/src/interface/editor_sections/wavetable_edit_section.cpp`) and
//! the parts it lays out from `vital/src/interface/wavetable/`:
//! `WavetableOrganizer`, `WavetableComponentList`, `WavetablePlayhead` and
//! `WavetablePlayheadInfo`.
//!
//! It shows the frame under the playhead in the waveform, amplitude and
//! phase displays, the components in the list and their keyframes in the
//! organizer, with the first source's first keyframe selected. The timeline
//! edits the creator: the playhead moves the oscillator's wave frame, the
//! organizer selects, creates, deletes and drags keyframes, and the list
//! adds, moves, resets and removes components and groups
//! (`sas_wavetable::creator::edits`). Each edit is committed to the model
//! when it is done (`EditorModel::set_wavetable_creator`). The exit button
//! closes the editor. The component overlays and the section's menus come
//! in the parts that follow. The organizer counts double-clicks with JUCE's
//! multiple-click rules, as the sliders do (`controls::clicks`).
//!
//! `WavetablePlayheadInfo` draws its number with JUCE's component painting,
//! which `FullInterface` turns off for its OpenGL context
//! (`setComponentPaintingEnabled(false)`), so upstream never shows it, and
//! neither does the port.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use egui::{PointerButton, Pos2, Sense};
use sas_engine::edit_bus::AuditionToken;
use sas_engine::sas_oscillators::sas_wavetable::creator::component::WavetableComponent;
use sas_engine::sas_oscillators::sas_wavetable::creator::edits::{
    self, KeyframeIndex, MODIFIER_TYPES, SOURCE_TYPES,
};
use sas_engine::sas_oscillators::sas_wavetable::creator::WavetableCreator;
use sas_engine::sas_oscillators::sas_wavetable::wave_frame::{NUM_REAL_COMPLEX, WAVEFORM_SIZE};
use sas_engine::sas_oscillators::sas_wavetable::{
    WaveFrame, Wavetable, NUM_OSCILLATOR_WAVE_FRAMES,
};
use sas_preset::user_folders::{self, Kind, WAVETABLE_EXTENSION};

use super::line_source_overlay::{LineSourceLayout, LineSourceOverlay};
use super::phase_window_overlays::{PhaseLayout, PhaseOverlay, WindowLayout, WindowOverlay};
use super::wave_source_overlay::{
    paint_wave_source_overlay, show_wave_source_overlay, FrameEdit, OverlayLayout,
    WaveSourceOverlay, WaveSourcePart,
};
use super::{popup_browser, MenuOwner, Panel};
use crate::controls::bars::BarRenderer;
use crate::controls::clicks::Clicks;
use crate::controls::file_dialog::{self, DialogKind, FileRequest};
use crate::controls::icon;
use crate::controls::path::Path;
use crate::controls::paths;
use crate::controls::scroll_bar::ScrollBar;
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::controls::ui_button;
use crate::fonts::Face;
use crate::host::Host;
use crate::interface::EditorModel;
use crate::layout::{PresetSelectorLayout, Rect};
use crate::local;
use crate::paint::{PixelPainter, RectF};
use crate::popup::{Menu, MenuItem};
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::section::{paint_body, paint_tab_shadow};
use crate::skin::{ColorId, Section, ValueId};
use crate::text::{Justify, Overflow};

/// The editor's skin override (`setSkinOverride(Skin::kWavetableEditor)`).
pub const SKIN: &[Section] = &[Section::WavetableEditor];

/// `getTopHeight`'s `kTopHeight`, before the size ratio.
const TOP_HEIGHT: f32 = 48.0;
/// `getFrameEditBounds`'s `kHeightRatio`.
const FRAME_EDIT_HEIGHT_RATIO: f32 = 0.66;
/// `WavetableOrganizer::kHandleHeightPercent`.
const HANDLE_HEIGHT_PERCENT: f32 = 1.0 / 8.0;
/// `vital::kNumOscillatorWaveFrames`: the organizer's and playhead's
/// positions.
const NUM_FRAMES: usize = NUM_OSCILLATOR_WAVE_FRAMES as usize;
/// `WavetableOrganizer::kDrawSkip` and `kDrawSkipLarge`.
const ORGANIZER_DRAW_SKIP: usize = 4;
const ORGANIZER_DRAW_SKIP_LARGE: usize = 32;
/// `WavetablePlayhead::kLineSkip` and `kBigLineSkip`.
const PLAYHEAD_LINE_SKIP: usize = 4;
const PLAYHEAD_BIG_LINE_SKIP: usize = 16;
/// The keyframe diamonds' border (`setThickness(2.0f)`).
const DIAMOND_THICKNESS: f32 = 2.0;
/// The section's starting `zoom_`.
const DEFAULT_ZOOM: f32 = 8.0;
/// `WavetableComponentList::resized`'s `kScrollBarWidth`, before the size
/// ratio.
const SCROLL_BAR_WIDTH: f32 = 13.0;
/// The component list's row menus (`WavetableComponentList::ComponentRowMenu`).
pub const MENU_RESET: i32 = 1;
pub const MENU_REMOVE: i32 = 2;
pub const MENU_MOVE_UP: i32 = 3;
pub const MENU_MOVE_DOWN: i32 = 4;
/// The organizer's menu (`WavetableOrganizer::OrganizerMenu`).
pub const MENU_CREATE_KEYFRAME: i32 = 1;
pub const MENU_REMOVE_KEYFRAMES: i32 = 2;
/// `WavetableEditSection::kObscureAmount`: how far a display an overlay
/// covers fades out.
const OBSCURE_AMOUNT: f32 = 0.4;
/// The gear menu's items (`BarEditorMenu`): the amplitudes' scale, then the
/// frequency displays' zoom, `1 << (item - kZoom1)`.
pub const MENU_POWER_SCALE: i32 = 1;
pub const MENU_AMPLITUDE_SCALE: i32 = 2;
pub const MENU_ZOOM_1: i32 = 3;
pub const MENU_ZOOM_16: i32 = 7;
/// The title menu's items (`MenuItems`).
pub const MENU_SAVE_AS_WAVETABLE: i32 = 1;
pub const MENU_IMPORT_WAVETABLE: i32 = 2;
pub const MENU_EXPORT_WAVETABLE: i32 = 3;
pub const MENU_EXPORT_WAV: i32 = 4;
pub const MENU_RESYNTHESIZE: i32 = 5;

/// `textMouseDown`'s `kBrowserWidth` and `kBrowserHeight`, before the size
/// ratio.
const BROWSER_WIDTH: f32 = 600.0;
const BROWSER_HEIGHT: f32 = 400.0;
/// `updateFrequencyDomain`'s epsilons.
const AMPLITUDE_EPSILON: f32 = 0.000_000_1;
const PHASE_EPSILON: f32 = 0.0001;

/// `WavetableEditSection::resized` and the geometry of its parts, in
/// interface coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WavetableEditLayout {
    pub bounds: Rect,
    /// `getFrameEditBounds` and `getTimelineBounds`, the two bodies.
    pub frame_edit: Rect,
    pub timeline: Rect,
    pub exit: Rect,
    /// `oscillator_waveform_`, `frequency_amplitudes_` and
    /// `frequency_phases_`.
    pub waveform: Rect,
    pub amplitudes: Rect,
    pub phases: Rect,
    /// `frequency_amplitude_settings_`, the gear over the amplitudes.
    pub settings: Rect,
    pub playhead_info: Rect,
    pub playhead: Rect,
    pub component_list: Rect,
    pub organizer: Rect,
    /// `preset_selector_`, its round amount, and the menu button beside it.
    pub preset: PresetSelectorLayout,
    pub preset_round_amount: f32,
    pub menu: Rect,
    /// `WavetableOrganizer::handleWidth`: the organizer's row height and
    /// keyframe size, and the component list's row height.
    pub handle_width: i32,
}

impl WavetableEditLayout {
    /// `resized` with the section at `bounds`; `top` is the interface's
    /// scope.
    pub fn new(top: &Scope, bounds: Rect) -> WavetableEditLayout {
        let scope = top.within(SKIN);
        let ratio = scope.size_ratio();
        let (width, height) = (bounds.width, bounds.height);
        let padding = scope.padding() as i32;
        let large_padding = scope.value(ValueId::LargePadding) as i32;
        let widget_margin = scope.widget_margin() as i32;
        let title_width = (ratio * TOP_HEIGHT) as i32;
        let button_height = (20.0 * ratio) as i32;

        let exit = Rect::new(width - title_width, padding, title_width, title_width);

        let frame_edit = Rect::new(
            large_padding,
            title_width,
            width - 2 * large_padding,
            ((height - title_width) as f32 * FRAME_EDIT_HEIGHT_RATIO) as i32,
        );
        // getTimelineBounds subtracts the unrounded padding.
        let timeline_height =
            ((height - frame_edit.bottom() - large_padding) as f32 - scope.padding()) as i32;
        let timeline = Rect::new(
            large_padding,
            frame_edit.bottom() + large_padding,
            frame_edit.width,
            timeline_height,
        );

        let edit_x = frame_edit.x + widget_margin;
        let edit_width = frame_edit.width - 2 * widget_margin;
        let osc_height = (frame_edit.height as f32 * 0.58) as i32;
        let amp_height = (frame_edit.height as f32 * 0.26) as i32;
        let phase_height = frame_edit.height - osc_height - amp_height - 4 * widget_margin;
        let waveform = Rect::new(edit_x, frame_edit.y + widget_margin, edit_width, osc_height);
        let amplitudes = Rect::new(
            edit_x,
            waveform.bottom() + widget_margin,
            edit_width,
            amp_height,
        );
        let settings = Rect::new(edit_x, amplitudes.y, button_height, button_height);
        let phases = Rect::new(
            edit_x,
            amplitudes.bottom() + widget_margin,
            edit_width,
            phase_height,
        );

        let playhead_height = (timeline.height as f32 * HANDLE_HEIGHT_PERCENT) as i32;
        let organizer_x = timeline.x + timeline.width / 4;
        let organizer_width = timeline.right() - organizer_x;
        let info_width = organizer_x - timeline.x;
        let playhead_info = Rect::new(timeline.x, timeline.y, info_width, playhead_height);
        let playhead = Rect::new(organizer_x, timeline.y, organizer_width, playhead_height);
        let below_playhead = timeline.height - playhead_height;
        let component_list = Rect::new(
            timeline.x,
            timeline.y + playhead_height,
            info_width,
            below_playhead,
        );
        let organizer = Rect::new(
            organizer_x,
            timeline.y + playhead_height,
            organizer_width,
            below_playhead,
        );
        let handle_width = 1 + 2 * (organizer.height as f32 * HANDLE_HEIGHT_PERCENT * 0.5) as i32;

        let preset_selector_width = width / 3;
        let preset_selector_height = (title_width as f32 * 0.6) as i32;
        let preset_selector_buffer = ((title_width - preset_selector_height) as f32 * 0.5) as i32;
        let preset_selector_x = (width - preset_selector_width + 2 * preset_selector_height) / 2;
        let preset_selector = Rect::new(
            preset_selector_x,
            preset_selector_buffer,
            preset_selector_width - preset_selector_height,
            preset_selector_height,
        );
        let menu = Rect::new(
            preset_selector.right(),
            preset_selector_buffer,
            preset_selector_height,
            preset_selector_height,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        WavetableEditLayout {
            bounds,
            frame_edit: frame_edit.translated(dx, dy),
            timeline: timeline.translated(dx, dy),
            exit: exit.translated(dx, dy),
            waveform: waveform.translated(dx, dy),
            amplitudes: amplitudes.translated(dx, dy),
            phases: phases.translated(dx, dy),
            settings: settings.translated(dx, dy),
            playhead_info: playhead_info.translated(dx, dy),
            playhead: playhead.translated(dx, dy),
            component_list: component_list.translated(dx, dy),
            organizer: organizer.translated(dx, dy),
            preset: PresetSelectorLayout::new(preset_selector.translated(dx, dy)),
            preset_round_amount: preset_selector_height as f32 / 2.0,
            menu: menu.translated(dx, dy),
            handle_width,
        }
    }

    /// `WavetableComponentList::positionGroups`'s cell height.
    pub fn cell_height(&self) -> i32 {
        self.handle_width - 2
    }

    /// The top of component list row `row`.
    fn row_y(&self, row: usize) -> i32 {
        self.component_list.y + row as i32 * self.handle_width + 2
    }

    /// A component's name in row `row`: sources start one row height in,
    /// modifiers two (`names_`).
    pub fn component_name(&self, row: usize, modifier: bool) -> Rect {
        let x = self.handle_width * if modifier { 2 } else { 1 };
        Rect::new(
            self.component_list.x + x,
            self.row_y(row),
            self.component_list.width - x,
            self.cell_height(),
        )
    }

    /// A component's menu button in row `row` (`menu_buttons_`).
    pub fn component_menu(&self, row: usize) -> Rect {
        let cell = self.cell_height();
        Rect::new(
            self.component_list.right() - cell,
            self.row_y(row),
            cell,
            cell,
        )
    }

    /// The "Add Modifier" (`modifier`) or "Add Source" button in row `row`,
    /// running past the list's right edge as upstream's do.
    pub fn add_button(&self, scope: &Scope, row: usize, modifier: bool) -> Rect {
        let indent = self.handle_width * if modifier { 2 } else { 1 };
        let rounding = scope.within(SKIN).value(ValueId::LabelBackgroundRounding);
        Rect::new(
            self.component_list.x + (indent as f32 - rounding) as i32,
            self.row_y(row),
            self.component_list.width,
            self.cell_height(),
        )
    }

    /// The plus icon on an add button in row `row` (`plus_icons_`).
    pub fn add_plus(&self, scope: &Scope, row: usize, modifier: bool) -> Rect {
        let indent = self.handle_width * if modifier { 2 } else { 1 };
        let rounding = scope.within(SKIN).value(ValueId::LabelBackgroundRounding);
        let cell = self.cell_height();
        Rect::new(
            self.component_list.x + (indent as f32 - rounding * 0.5) as i32,
            self.row_y(row),
            cell,
            cell,
        )
    }

    /// `WavetablePlayhead::padding_` (`setPadding(handleWidth() / 2.0f)`).
    fn playhead_padding(&self) -> f32 {
        self.handle_width as f32 / 2.0
    }

    /// `WavetableOrganizer::frame_width_`: the organizer's pixels per
    /// position.
    fn frame_width(&self) -> f32 {
        (self.organizer.width - self.handle_width + 1) as f32 / (NUM_FRAMES as f32 - 1.0)
    }
}

/// The component overlays (`WavetableOverlayFactory::createOverlay`), by
/// the type of the component whose keyframe they edit. An overlay is added
/// as a variant here, with its arms in [`OverlayKind::of`] and
/// [`OverlayKind::obscures`] and in the section's `overlay_input`,
/// `paint_overlay` and `show_overlay`; the section draws the current one
/// twice around its displays, as upstream does, and fades the displays it
/// covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    /// A `WaveSourceOverlay`, for a Wave Source (0) or a Shepard Tone
    /// source (1): the index of the section's overlay state.
    WaveSource(usize),
    /// A Phase Shift's `PhaseModifierOverlay`.
    Phase,
    /// A Wave Window's `WaveWindowOverlay`.
    Window,
    /// A Line Source's `WaveLineSourceOverlay`.
    LineSource,
}

impl OverlayKind {
    /// The overlay for `component`'s keyframes, if it has one yet.
    pub fn of(component: &WavetableComponent) -> Option<OverlayKind> {
        match component {
            WavetableComponent::WaveSource(_) => Some(OverlayKind::WaveSource(0)),
            WavetableComponent::ShepardToneSource(_) => Some(OverlayKind::WaveSource(1)),
            WavetableComponent::PhaseModifier(_) => Some(OverlayKind::Phase),
            WavetableComponent::WaveWindow(_) => Some(OverlayKind::Window),
            WavetableComponent::LineSource(_) => Some(OverlayKind::LineSource),
            _ => None,
        }
    }

    /// The displays the overlay covers (`setTimeDomainBounds`,
    /// `setFrequencyAmplitudeBounds` and `setPhaseBounds` returning true),
    /// which fade (`obscure_time_domain_` and the rest).
    pub fn obscures(self) -> Obscured {
        match self {
            OverlayKind::WaveSource(_) => Obscured {
                time_domain: true,
                amplitudes: true,
                phases: true,
            },
            // Its phase editor draws over the waveform, but
            // `setTimeDomainBounds` returns false.
            OverlayKind::Phase => Obscured::default(),
            OverlayKind::Window | OverlayKind::LineSource => Obscured {
                time_domain: true,
                ..Obscured::default()
            },
        }
    }
}

/// Which of the editor's displays an overlay covers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Obscured {
    pub time_domain: bool,
    pub amplitudes: bool,
    pub phases: bool,
}

/// Where a keyframe is in the creator (upstream's `WavetableKeyframe*`).
pub type KeyframeRef = KeyframeIndex;

/// A menu the editor opened, for the choice that comes back through
/// [`super::Overlay::wavetable_edit_action`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMenu {
    /// "Add Source": the choice is an index into `SOURCE_TYPES`.
    AddSource,
    /// A group's "Add Modifier": an index into `MODIFIER_TYPES`.
    AddModifier(usize),
    /// A source row's menu (`groupMenuClicked`), by group.
    Group(usize),
    /// A modifier row's menu (`modifierMenuClicked`), by group and
    /// component.
    Modifier(usize, usize),
    /// The organizer's menu, opened at this organizer point
    /// (`menu_created_position_`).
    Organizer(i32, i32),
    /// The gear's menu over the amplitudes
    /// (`frequency_amplitude_settings_`).
    Settings,
    /// The title's menu, beside the name (`showPopupMenu`).
    Title,
    /// A Wave Source overlay's editor menu, by overlay (Wave Sources, then
    /// Shepard Tone sources) and editor.
    WaveSource(usize, WaveSourcePart),
}

/// What a press on the organizer is doing (`WavetableOrganizer::MouseMode`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum OrganizerMode {
    Waiting,
    Selecting,
    /// Dragging the selection by `dragged`, whose handle started at
    /// `start_x` (`dragged_start_x_`).
    Dragging {
        dragged: KeyframeRef,
        start_x: i32,
    },
}

/// A press on the organizer, in organizer pixels.
#[derive(Clone, Copy, Debug)]
struct OrganizerPress {
    mode: OrganizerMode,
    down: (i32, i32),
    current: (i32, i32),
    /// Pressed with the popup-menu button.
    popup: bool,
    /// The drag moved a keyframe.
    moved: bool,
}

/// A component's row in the list and in the organizer.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentRow {
    pub group: usize,
    pub component: usize,
    /// `WavetableComponentFactory::getComponentName`.
    pub name: &'static str,
    /// The row in both views: each group's components, then a row for its
    /// "Add Modifier" button.
    pub row: usize,
    /// Its keyframes' positions, in order.
    pub positions: Vec<i32>,
    /// `hasKeyframes`: false for the Shepard tone source, whose one frame
    /// spans the organizer.
    pub has_keyframes: bool,
}

/// The components of `creator`, row by row.
pub fn component_rows(creator: &WavetableCreator) -> Vec<ComponentRow> {
    let mut rows = Vec::new();
    let mut row = 0;
    for (group_index, group) in creator.groups.iter().enumerate() {
        for (component_index, component) in group.components.iter().enumerate() {
            rows.push(ComponentRow {
                group: group_index,
                component: component_index,
                name: component.component_type().name(),
                row,
                positions: component.keyframe_positions(),
                has_keyframes: component.has_keyframes(),
            });
            row += 1;
        }
        row += 1;
    }
    rows
}

/// The editor of one oscillator's wavetable. Like upstream's three
/// sections, it keeps its playhead, selection and display settings while
/// hidden.
pub struct WavetableEditSection {
    oscillator: usize,
    /// The model's copy of the creator, and the content revision it was
    /// read at.
    creator: Option<(u64, WavetableCreator)>,
    /// `WavetablePlayhead::position_`.
    playhead: usize,
    /// `WavetableOrganizer::currently_selected_`.
    selected: Vec<KeyframeRef>,
    /// `zoom_` and `power_scale_`: the frequency displays' settings.
    zoom: f32,
    power_scale: bool,
    /// Whether the section showed last frame (`visibilityChanged`).
    visible: bool,
    /// The model's preset generation when the creator was last read: a
    /// load since then resets the section (`FullInterface::reset`).
    generation: Option<u64>,
    /// A press on the organizer, while it lasts.
    organizer: Option<OrganizerPress>,
    /// The organizer's recent presses, for double-clicks.
    clicks: Clicks,
    /// A press on the playhead, while it lasts.
    playhead_held: bool,
    /// The wave frame's host gesture is open (D18).
    wave_frame_begun: bool,
    /// The component list's scroll position and bar.
    scroll: ScrollBar,
    /// The keyframe the overlays last heard of (`frameSelected`): set when
    /// the organizer selects one keyframe, cleared when it deselects or
    /// selects several.
    overlay_frame: Option<KeyframeRef>,
    /// Bumped each time the organizer selects one keyframe
    /// (`frameSelected`), when the overlays load it.
    frame_selections: u64,
    /// The Wave Source overlays: Wave Sources', then Shepard Tone sources'.
    wave_source: [WaveSourceOverlay; 2],
    /// The drag on an overlay's editors that is auditioning, if any.
    audition: PendingAudition,
    /// The popup browser a press on the name asked for since the last
    /// call: its bounds and the section's (`textMouseDown`).
    browse_request: Option<(Rect, Rect)>,
    /// "Save As Wavetable" was chosen since the last call
    /// (`FullInterface::saveWavetable`).
    save_request: bool,
    /// A keyframe to select once the creator is read (the reference
    /// tool's `--wavetable-select`).
    requested_selection: Option<KeyframeRef>,
    /// The Phase Shift and Wave Window overlays.
    phase: PhaseOverlay,
    window: WindowOverlay,
    line_source: LineSourceOverlay,
}

impl WavetableEditSection {
    pub fn new(oscillator: usize) -> WavetableEditSection {
        WavetableEditSection {
            oscillator,
            creator: None,
            playhead: 0,
            selected: Vec::new(),
            zoom: DEFAULT_ZOOM,
            power_scale: true,
            visible: false,
            generation: None,
            organizer: None,
            clicks: Clicks::default(),
            playhead_held: false,
            wave_frame_begun: false,
            scroll: ScrollBar::default(),
            overlay_frame: None,
            frame_selections: 0,
            wave_source: local::WAVE_SOURCE_CONTROLS[oscillator.min(2)].map(WaveSourceOverlay::new),
            audition: PendingAudition::default(),
            browse_request: None,
            save_request: false,
            requested_selection: None,
            phase: PhaseOverlay::new(local::MODIFIER_CONTROLS[oscillator.min(2)]),
            window: WindowOverlay::new(local::MODIFIER_CONTROLS[oscillator.min(2)]),
            line_source: LineSourceOverlay::new(
                local::LINE_SOURCE_CONTROLS[oscillator.min(2)],
                oscillator,
            ),
        }
    }

    /// The drag that is auditioning, shared with the editor window, which
    /// ends it if the window closes or fails during it.
    pub fn pending_audition(&self) -> PendingAudition {
        self.audition.clone()
    }

    pub fn oscillator(&self) -> usize {
        self.oscillator
    }

    pub fn playhead(&self) -> usize {
        self.playhead
    }

    pub fn selected(&self) -> &[KeyframeRef] {
        &self.selected
    }

    /// The keyframe the overlays show, if any (`frameSelected`'s last).
    pub fn overlay_frame(&self) -> Option<KeyframeRef> {
        self.overlay_frame
    }

    /// `zoom_`: the frequency displays' zoom.
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// `power_scale_`: the amplitudes drawn on the power scale.
    pub fn power_scale(&self) -> bool {
        self.power_scale
    }

    /// The popup browser asked for since the last call: the browser's
    /// bounds and the section's.
    pub fn take_browse_request(&mut self) -> Option<(Rect, Rect)> {
        self.browse_request.take()
    }

    /// Whether "Save As Wavetable" was chosen since the last call.
    pub fn take_save_request(&mut self) -> bool {
        std::mem::take(&mut self.save_request)
    }

    /// The creator as last read from the model.
    pub fn creator(&self) -> Option<&WavetableCreator> {
        self.creator.as_ref().map(|(_, creator)| creator)
    }

    /// Reads the creator again when the model's content has changed, and
    /// keeps the selection to the keyframes that still exist. Returns true
    /// when a preset loaded since the last read.
    fn refresh(&mut self, model: &dyn EditorModel) -> bool {
        let generation = model.preset_generation();
        let loaded = self.generation.is_some_and(|read| read != generation);
        self.generation = Some(generation);
        let revision = model.content_revision();
        if !loaded
            && self
                .creator
                .as_ref()
                .is_some_and(|(read, _)| *read == revision)
        {
            return false;
        }
        let overlay_pressed = self.wave_source.iter().any(WaveSourceOverlay::pressed)
            || self.phase.pressed()
            || self.window.pressed()
            || self.line_source.pressed();
        if !loaded && (self.organizer.is_some() || overlay_pressed) {
            // A keyframe drag, or drawing on an overlay, edits the copy
            // until it's committed.
            return false;
        }
        self.creator = model
            .wavetable_creator(self.oscillator)
            .map(|creator| (revision, creator));
        let rows = self.creator().map(component_rows).unwrap_or_default();
        let exists = |selected: &KeyframeRef| {
            rows.iter().any(|row| {
                row.group == selected.group
                    && row.component == selected.component
                    && selected.keyframe < row.positions.len()
            })
        };
        self.selected.retain(exists);
        self.overlay_frame = self.overlay_frame.filter(exists);
        loaded
    }

    /// `WavetableOrganizer::selectDefaultFrame`: the first component's
    /// first keyframe.
    fn select_default_frame(&mut self) {
        let first = self.creator().and_then(|creator| {
            let group = creator.groups.first()?;
            let component = group.components.first()?;
            (component.num_frames() > 0).then_some(KeyframeRef {
                group: 0,
                component: 0,
                keyframe: 0,
            })
        });
        if let Some(keyframe) = first {
            self.select_frame(keyframe);
        }
    }

    /// `WavetableOrganizer::selectFrame`: `keyframe` alone is selected, and
    /// the overlays hear of it.
    fn select_frame(&mut self, keyframe: KeyframeRef) {
        self.selected = vec![keyframe];
        self.overlay_frame = Some(keyframe);
        self.frame_selections += 1;
    }

    /// `WavetableOrganizer::deselect`: nothing is selected, and the overlays
    /// hear of no keyframe.
    fn deselect(&mut self) {
        self.selected.clear();
        self.overlay_frame = None;
    }

    /// `visibilityChanged` and the content checks before a frame: called
    /// each frame the section shows.
    pub fn update(&mut self, model: &dyn EditorModel) {
        if self.refresh(model) {
            // reset: the organizer's selection clears, and the default
            // frame is selected again if the section shows.
            self.deselect();
            if self.visible {
                self.select_default_frame();
            }
        }
        if !self.visible && self.selected.is_empty() {
            self.select_default_frame();
        }
        self.visible = true;
        if let Some(keyframe) = self.requested_selection.take() {
            let exists = self
                .creator()
                .and_then(|creator| creator.groups.get(keyframe.group))
                .and_then(|group| group.components.get(keyframe.component))
                .is_some_and(|component| keyframe.keyframe < component.num_frames());
            if exists {
                self.select_frame(keyframe);
            }
        }
    }

    /// Selects `keyframe` when the section next shows, as a click on it in
    /// the organizer does (`WavetableOrganizer::selectFrame`), if it exists.
    pub fn request_selection(&mut self, keyframe: KeyframeRef) {
        self.requested_selection = Some(keyframe);
    }

    /// The section stopped showing.
    pub fn hidden(&mut self, model: &dyn EditorModel) {
        self.visible = false;
        self.audition.end(model);
    }

    /// `paintBackgroundShadow` and `paintBackground`, with the children's
    /// backgrounds.
    pub fn paint_background(
        &self,
        pixels: &PixelPainter,
        top: &Scope,
        layout: &WavetableEditLayout,
    ) {
        let scope = top.within(SKIN);
        paint_tab_shadow(pixels, &scope, layout.frame_edit);
        paint_tab_shadow(pixels, &scope, layout.timeline);
        paint_body(pixels, &scope, layout.frame_edit);
        paint_body(pixels, &scope, layout.timeline);

        paint_organizer_background(pixels, &scope, layout);
        paint_playhead_background(pixels, &scope, layout);
        pixels.fill_rounded_rect(
            layout.preset.bounds,
            scope.value(ValueId::WidgetRoundedCorner),
            scope.color(ColorId::PopupSelectorBackground),
        );
        let widget_background = scope.color(ColorId::WidgetBackground);
        for display in [layout.waveform, layout.amplitudes, layout.phases] {
            pixels.fill_rect(display, widget_background);
        }
    }

    /// Draws the section over its background and handles its buttons.
    /// Returns true when the exit button is clicked
    /// (`FullInterface::hideWavetableEditSection`).
    pub fn show(&mut self, panel: &mut Panel, layout: &WavetableEditLayout) -> bool {
        self.respond_to_menu(panel, layout);
        let scroll = self.handle_scroll(panel, layout);
        self.handle_playhead(panel, layout);
        self.handle_organizer(panel, layout, scroll);

        let model = panel.host.model();
        let frame = model
            .wavetable(self.oscillator)
            .as_deref()
            .map(|wavetable| frame_samples(wavetable, self.playhead));
        let overlay = self.current_overlay();
        if let Some((kind, keyframe)) = overlay {
            self.overlay_input(panel, layout, kind, keyframe);
        }
        // The displays show the frame under the playhead as drawn.
        let frame = model
            .wavetable(self.oscillator)
            .as_deref()
            .map(|wavetable| frame_samples(wavetable, self.playhead))
            .or(frame);
        // The current overlay draws twice, before and after the displays
        // (see `wave_source_overlay`'s module docs).
        if let Some((kind, keyframe)) = overlay {
            self.paint_overlay(panel, layout, kind, keyframe);
        }
        let obscured = overlay.map_or(Obscured::default(), |(kind, _)| kind.obscures());
        self.show_displays(panel, layout, frame.as_ref(), obscured);
        if let Some((kind, keyframe)) = overlay {
            self.show_overlay(panel, layout, kind, keyframe);
        }
        // renderCorners, after the overlay.
        for display in [layout.waveform, layout.amplitudes, layout.phases] {
            panel.widget_corners(display);
        }

        let colours = ShapeColours::icon(&panel.scope);
        let shape = |shape| ShapeButton { shape, colours };
        let settings = panel.shape_button(
            "wavetable settings",
            shape(Shape::Icon {
                key: "gear",
                path: paths::gear(),
            }),
            layout.settings,
        );
        if settings.clicked {
            let items = vec![
                MenuItem::new(MENU_POWER_SCALE, "Power Scale"),
                MenuItem::new(MENU_AMPLITUDE_SCALE, "Amplitude Scale"),
                MenuItem::divider(),
                MenuItem::new(MENU_ZOOM_1, "Zoom 1x"),
                MenuItem::new(MENU_ZOOM_1 + 1, "Zoom 2x"),
                MenuItem::new(MENU_ZOOM_1 + 2, "Zoom 4x"),
                MenuItem::new(MENU_ZOOM_1 + 3, "Zoom 8x"),
                MenuItem::new(MENU_ZOOM_16, "Zoom 16x"),
            ];
            let position = Pos2::new(layout.settings.x as f32, layout.settings.bottom() as f32);
            panel.overlay.menu = Some(Menu::new(
                items,
                position,
                MenuOwner::WavetableEdit(self.oscillator, EditMenu::Settings),
            ));
        }

        let preset = layout.preset;
        let text = self.creator().map(preset_selector_text).unwrap_or_default();
        panel.text_centred(
            &text,
            preset.text,
            Face::Title,
            preset.text_size,
            panel.scope.color(ColorId::PresetText),
        );
        // textMouseDown: the browser below the name, from its left.
        let text = panel.pixels.rect(preset.text.into());
        let name = panel.ui.interact(
            text,
            panel.ui.scope_id().with("wavetable editor name"),
            Sense::click(),
        );
        if name.hovered() && panel.ui.input(|input| input.pointer.primary_pressed()) {
            let ratio = panel.scope.size_ratio();
            let browser = Rect::new(
                preset.bounds.x,
                preset.bounds.bottom(),
                (BROWSER_WIDTH * ratio) as i32,
                (BROWSER_HEIGHT * ratio) as i32,
            );
            self.browse_request = Some((browser, layout.bounds));
        }
        let previous =
            panel.shape_button("wavetable previous", shape(Shape::Prev), preset.previous);
        let next = panel.shape_button("wavetable next", shape(Shape::Next), preset.next);
        let shift = if previous.clicked {
            -1
        } else if next.clicked {
            1
        } else {
            0
        };
        if shift != 0 {
            self.load_shifted_file(model, shift);
        }
        let (key, path) = sized_path("wavetable menu", layout.menu.height, paths::menu);
        let menu = panel.shape_button(
            "wavetable menu",
            shape(Shape::Icon { key, path }),
            layout.menu,
        );
        if menu.clicked {
            let items = vec![
                MenuItem::new(MENU_SAVE_AS_WAVETABLE, "Save As Wavetable"),
                MenuItem::new(MENU_IMPORT_WAVETABLE, "Import Wavetable"),
                MenuItem::new(MENU_EXPORT_WAVETABLE, "Export Wavetable"),
                MenuItem::new(MENU_EXPORT_WAV, "Export to .wav File"),
                MenuItem::new(MENU_RESYNTHESIZE, "Synthesize Preset to Table"),
            ];
            let position = Pos2::new(layout.menu.x as f32, layout.menu.bottom() as f32);
            panel.overlay.menu = Some(Menu::new(
                items,
                position,
                MenuOwner::WavetableEdit(self.oscillator, EditMenu::Title),
            ));
        }
        self.take_dialog_answers(panel);
        let exit = panel.shape_button(
            "wavetable exit",
            shape(Shape::Icon {
                key: "exit x",
                path: paths::exit_x(),
            }),
            layout.exit,
        );

        let rows = self.creator().map(component_rows).unwrap_or_default();
        let group_sizes: Vec<usize> = self
            .creator()
            .map(|creator| {
                creator
                    .groups
                    .iter()
                    .map(|group| group.components.len())
                    .collect()
            })
            .unwrap_or_default();
        self.show_component_list(panel, layout, &rows, &group_sizes, scroll);
        self.show_organizer(panel, layout, &rows, scroll);
        self.show_playhead(panel, layout);
        exit.clicked
    }

    /// The title menu's dialogs, once answered: an imported file loads
    /// (`importWavetable`), and the exports are written
    /// (`exportWavetable`, `exportToWav`).
    fn take_dialog_answers(&self, panel: &Panel) {
        let ctx = panel.ui.ctx();
        let model = panel.host.model();
        let oscillator = self.oscillator;
        if let Some(Some(file)) = file_dialog::take_result(ctx, dialog_id("import", oscillator)) {
            if file.exists() {
                popup_browser::editor_load_wavetable_file(model, oscillator, &file);
            }
        }
        if let Some(Some(file)) = file_dialog::take_result(ctx, dialog_id("export", oscillator)) {
            let _ = export_wavetable(model, oscillator, &file);
        }
        if let Some(Some(file)) = file_dialog::take_result(ctx, dialog_id("export wav", oscillator))
        {
            let _ = export_wav(model, oscillator, &file);
        }
    }

    /// `prevClicked` and `nextClicked`: the wavetable `shift` places from the
    /// creator's last file among the user's and the added folders'
    /// (`getShiftedFile`), loaded through the editor's `loadFile`.
    fn load_shifted_file(&self, model: &dyn EditorModel, shift: i32) {
        let mut directories = user_folders::root(model.data_directory().as_deref())
            .map(|root| user_folders::directories(&root, Kind::Wavetables))
            .unwrap_or_default();
        directories.extend(model.additional_folders(Kind::Wavetables));
        let files = user_folders::files_of_type(&directories, popup_browser::WAVETABLE_EXTENSIONS);
        let current = model.last_wavetable_file(self.oscillator);
        if let Some(file) = user_folders::shifted_file(files, current.as_deref(), shift) {
            popup_browser::editor_load_wavetable_file(model, self.oscillator, &file);
        }
    }

    /// Commits the edited creator to the model, which renders it
    /// (`render`). The next frame reads the rendered creator back, as
    /// upstream's next edit starts from the creator it rendered. Returns
    /// false when the model refused it (a full edit queue).
    fn commit(&self, model: &dyn EditorModel) -> bool {
        match &self.creator {
            Some((_, creator)) => model.set_wavetable_creator(self.oscillator, creator.clone()),
            None => false,
        }
    }

    /// The component list's column height (`component_container_`).
    fn list_height(&self, layout: &WavetableEditLayout) -> i32 {
        let rows: usize = self
            .creator()
            .map(|creator| {
                creator
                    .groups
                    .iter()
                    .map(|group| group.components.len() + 1)
                    .sum()
            })
            .unwrap_or(0);
        let row_height = layout.handle_width;
        let button_y = rows as i32 * row_height + 2;
        (button_y as f32 + layout.cell_height() as f32 + row_height as f32 * 0.5) as i32
    }

    /// The list's scroll bar, and the wheel over the list and the
    /// organizer (`componentsScrolled`, `WavetableOrganizer::mouseWheelMove`).
    /// Returns the scroll offset in pixels.
    fn handle_scroll(&mut self, panel: &mut Panel, layout: &WavetableEditLayout) -> i32 {
        let list = layout.component_list;
        let total = self.list_height(layout) as f32;
        let visible = list.height as f32;
        let ratio = panel.scope.size_ratio();
        let bar = Rect::new(
            list.x,
            list.y,
            (ratio * SCROLL_BAR_WIDTH) as i32,
            list.height,
        );
        let viewport = Rect::new(
            list.x,
            list.y,
            layout.organizer.right() - list.x,
            list.height,
        );
        let id = panel
            .ui
            .scope_id()
            .with(("wavetable list scroll bar", self.oscillator));
        let ppp = panel.pixels.pixels_per_point();
        self.scroll.handle(
            panel.ui,
            panel.pixels,
            id,
            viewport,
            bar,
            total,
            visible,
            ppp,
        );
        self.scroll.position as i32
    }

    /// `WavetablePlayhead::mouseDown` / `mouseDrag`: the position under the
    /// pointer, which moves the wave frame.
    fn handle_playhead(&mut self, panel: &mut Panel, layout: &WavetableEditLayout) {
        let playhead = layout.playhead;
        let id = panel
            .ui
            .scope_id()
            .with(("wavetable playhead", self.oscillator));
        let rect = panel.pixels.rect(playhead.into());
        let response = panel.ui.interact(rect, id, Sense::click_and_drag());
        let primary_down = panel
            .ui
            .input(|input| input.pointer.button_down(PointerButton::Primary));
        let held = response.is_pointer_button_down_on() && primary_down;
        if held {
            if let Some(pos) = panel.ui.input(|input| input.pointer.interact_pos()) {
                let x = to_pixels(panel.pixels, pos).0 - playhead.x as f32;
                let padding = layout.playhead_padding();
                let active_width = playhead.width as f32 - 2.0 * padding + 1.0;
                let position = ((NUM_FRAMES as f32 - 1.0) * (x.round() - padding) / active_width)
                    .round() as i32;
                self.move_playhead(panel.host, position.clamp(0, NUM_FRAMES as i32 - 1));
            }
        } else if self.playhead_held {
            self.end_wave_frame(panel.host);
        }
        self.playhead_held = held;
    }

    /// `WavetablePlayhead::setPosition`, whose `playheadMoved` sets the
    /// oscillator's wave frame slider. The slider tells the host only when
    /// the value changes, in one gesture for the action (D18).
    fn move_playhead(&mut self, host: &Host, position: i32) {
        self.playhead = position as usize;
        let name = wave_frame_name(self.oscillator);
        if host.value(name) != position as f32 {
            if !self.wave_frame_begun {
                host.begin(name);
                self.wave_frame_begun = true;
            }
            host.set(name, position as f32);
        }
    }

    /// Ends the wave frame's gesture, if one is open.
    fn end_wave_frame(&mut self, host: &Host) {
        if self.wave_frame_begun {
            host.end(wave_frame_name(self.oscillator));
            self.wave_frame_begun = false;
        }
    }

    /// The organizer's mouse (`WavetableOrganizer::mouseDown`, `mouseDrag`,
    /// `mouseUp`, `mouseDoubleClick`).
    fn handle_organizer(&mut self, panel: &mut Panel, layout: &WavetableEditLayout, scroll: i32) {
        let organizer = layout.organizer;
        let id = panel
            .ui
            .scope_id()
            .with(("wavetable organizer", self.oscillator));
        let rect = panel.pixels.rect(organizer.into());
        let response = panel.ui.interact(rect, id, Sense::click_and_drag());
        let (primary_down, secondary_down, modifiers, pointer, now) = panel.ui.input(|input| {
            (
                input.pointer.button_down(PointerButton::Primary),
                input.pointer.button_down(PointerButton::Secondary),
                input.modifiers,
                input.pointer.interact_pos(),
                input.time,
            )
        });
        // MouseEvent::x and y round the position.
        let local = |pos: Pos2| {
            let (x, y) = to_pixels(panel.pixels, pos);
            (
                x.round() as i32 - organizer.x,
                y.round() as i32 - organizer.y,
            )
        };
        let offset = -scroll;
        match self.organizer {
            None => {
                if !(response.is_pointer_button_down_on() && (primary_down || secondary_down)) {
                    return;
                }
                let Some(pos) = pointer else {
                    return;
                };
                let popup =
                    secondary_down || (cfg!(target_os = "macos") && modifiers.ctrl && primary_down);
                let button = if secondary_down {
                    PointerButton::Secondary
                } else {
                    PointerButton::Primary
                };
                self.clicks.press(now, pos, button, modifiers);
                // mouse_down_position_ = e.position.toInt() truncates.
                let (x, y) = to_pixels(panel.pixels, pos);
                let down = (x as i32 - organizer.x, y as i32 - organizer.y);
                self.organizer_down(layout, offset, local(pos), down, popup);
            }
            Some(press) => {
                let held = if press.popup && !primary_down {
                    secondary_down
                } else {
                    primary_down || secondary_down
                };
                let at = pointer.map(local).unwrap_or(press.current);
                if let (true, Some(pos)) = (held, pointer) {
                    self.clicks.drag(pos);
                }
                if held {
                    if at != press.current {
                        self.organizer_drag(panel.host, layout, at);
                    }
                } else {
                    // Component::internalMouseUp's click count.
                    let clicks = self.clicks.count(now);
                    self.organizer_up(panel, layout, offset, at, clicks);
                }
            }
        }
    }

    /// `mouseDown`: a press on a keyframe starts dragging the selection,
    /// selecting the keyframe first if it isn't selected.
    fn organizer_down(
        &mut self,
        layout: &WavetableEditLayout,
        offset: i32,
        at: (i32, i32),
        down: (i32, i32),
        popup: bool,
    ) {
        let mut press = OrganizerPress {
            mode: OrganizerMode::Waiting,
            down,
            current: at,
            popup,
            moved: false,
        };
        if let Some((keyframe, start_x)) = self.frame_at(layout, offset, at) {
            press.mode = OrganizerMode::Dragging {
                dragged: keyframe,
                start_x,
            };
            if !self.selected.contains(&keyframe) {
                self.select_frame(keyframe);
            }
        }
        self.organizer = Some(press);
    }

    /// `mouseDrag`: a move from empty space starts a selection box; a
    /// keyframe drag moves the selection and the playhead with it.
    fn organizer_drag(&mut self, host: &Host, layout: &WavetableEditLayout, at: (i32, i32)) {
        let Some(mut press) = self.organizer else {
            return;
        };
        press.current = at;
        let x_movement = at.0 - press.down.0;
        match press.mode {
            OrganizerMode::Waiting => {
                if x_movement != 0 {
                    press.mode = OrganizerMode::Selecting;
                }
            }
            OrganizerMode::Dragging { dragged, start_x } => {
                let frame_width = layout.frame_width();
                let target = unclamped_position(start_x + x_movement, frame_width);
                if let Some((_, creator)) = &mut self.creator {
                    edits::drag_keyframes(creator, &self.selected, dragged, target);
                    press.moved = true;
                }
                // frameDragged moves the playhead.
                let clamped = target.clamp(0, NUM_FRAMES as i32 - 1);
                self.move_playhead(host, clamped);
            }
            OrganizerMode::Selecting => {}
        }
        self.organizer = Some(press);
    }

    /// `mouseUp`: a click on empty space deselects, a selection box selects
    /// the keyframes in it, and a drag puts the moved keyframes in order and
    /// commits. The popup-menu button then opens the organizer's menu over
    /// a component with keyframes.
    fn organizer_up(
        &mut self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        offset: i32,
        at: (i32, i32),
        clicks: u32,
    ) {
        let Some(press) = self.organizer.take() else {
            return;
        };
        let model = panel.host.model();
        match press.mode {
            OrganizerMode::Waiting => self.deselect(),
            OrganizerMode::Selecting => {
                let rows = self.creator().map(component_rows).unwrap_or_default();
                // selectFrames deselects first, so the overlays hear of no
                // keyframe.
                self.deselect();
                self.selected = box_selection(layout, offset, &rows, press.down, at);
            }
            OrganizerMode::Dragging { .. } => {
                if press.moved {
                    if let Some((_, creator)) = &mut self.creator {
                        self.selected = edits::finish_keyframe_drag(creator, &self.selected);
                        // An overlay's keyframe is the one selected, which
                        // moved with the rest.
                        if self.overlay_frame.is_some() {
                            self.overlay_frame = self.selected.first().copied();
                        }
                    }
                    self.commit(model);
                }
                self.end_wave_frame(panel.host);
            }
        }
        if clicks >= 2 && !press.popup {
            // JUCE sends mouseDoubleClick after the second click's mouseUp.
            self.organizer_double_click(model, layout, offset, at);
        }
        if !press.popup {
            return;
        }
        let rows = self.creator().map(component_rows).unwrap_or_default();
        let row = row_from_y(layout, offset, at.1);
        if !rows
            .iter()
            .any(|component| component.row == row && component.has_keyframes)
        {
            return;
        }
        let item = if self.selected.len() > 1 {
            MenuItem::new(MENU_REMOVE_KEYFRAMES, "Remove Keyframes")
        } else if self.selected.len() == 1 {
            MenuItem::new(MENU_REMOVE_KEYFRAMES, "Remove Keyframe")
        } else {
            MenuItem::new(MENU_CREATE_KEYFRAME, "Create Keyframe")
        };
        let organizer = layout.organizer;
        let position = Pos2::new((organizer.x + at.0) as f32, (organizer.y + at.1) as f32);
        panel.overlay.menu = Some(Menu::new(
            vec![item],
            position,
            MenuOwner::WavetableEdit(self.oscillator, EditMenu::Organizer(at.0, at.1)),
        ));
    }

    /// `mouseDoubleClick` over a component with keyframes: deletes the
    /// keyframe under the pointer, or creates one there.
    fn organizer_double_click(
        &mut self,
        model: &dyn EditorModel,
        layout: &WavetableEditLayout,
        offset: i32,
        at: (i32, i32),
    ) {
        let rows = self.creator().map(component_rows).unwrap_or_default();
        let row = row_from_y(layout, offset, at.1);
        let Some(component) = rows.iter().find(|component| component.row == row) else {
            return;
        };
        if !component.has_keyframes {
            return;
        }
        if let Some((keyframe, _)) = self.frame_at(layout, offset, at) {
            self.deselect();
            if let Some((_, creator)) = &mut self.creator {
                edits::delete_keyframe(creator, keyframe);
            }
            self.commit(model);
        } else {
            self.create_keyframe_at(model, layout, offset, at);
        }
    }

    /// `createKeyframeAtPosition`: a keyframe for the component in the row
    /// under `at`, selected.
    fn create_keyframe_at(
        &mut self,
        model: &dyn EditorModel,
        layout: &WavetableEditLayout,
        offset: i32,
        at: (i32, i32),
    ) {
        let rows = self.creator().map(component_rows).unwrap_or_default();
        let row = row_from_y(layout, offset, at.1);
        let Some(component) = rows.iter().find(|component| component.row == row) else {
            return;
        };
        let (group, index) = (component.group, component.component);
        let x = at.0 - layout.handle_width / 2;
        let position = unclamped_position(x, layout.frame_width()).clamp(0, NUM_FRAMES as i32 - 1);
        let Some((_, creator)) = &mut self.creator else {
            return;
        };
        if let Some(keyframe) = edits::create_keyframe(creator, group, index, position) {
            self.select_frame(KeyframeRef {
                group,
                component: index,
                keyframe,
            });
            self.commit(model);
        }
    }

    /// `getFrameAtMouseEvent`: the keyframe whose diamond is under `at`,
    /// with its handle's left edge.
    fn frame_at(
        &self,
        layout: &WavetableEditLayout,
        offset: i32,
        at: (i32, i32),
    ) -> Option<(KeyframeRef, i32)> {
        let rows = self.creator().map(component_rows)?;
        let row_index = row_from_y(layout, offset, at.1);
        let row = rows.iter().find(|row| row.row == row_index)?;
        let handle = layout.handle_width;
        let frame_width = layout.frame_width();
        let position = unclamped_position(at.0 - handle, frame_width);
        let keyframe = |index: usize| KeyframeRef {
            group: row.group,
            component: row.component,
            keyframe: index,
        };
        if !row.has_keyframes {
            return (!row.positions.is_empty()).then_some((keyframe(0), 0));
        }
        let index = row.positions.iter().take_while(|&&p| position >= p).count();
        let frame_position = *row.positions.get(index)?;
        let x = (frame_width * frame_position as f32) as i32;
        let y = row.row as i32 * handle + offset;
        // DraggableFrame::isInside: within the diamond.
        let (local_x, local_y) = (at.0 - x, at.1 - y);
        let x_distance = local_x.min(handle - local_x);
        let y_distance = local_y.min(handle - local_y);
        (x_distance + y_distance >= handle / 2).then_some((keyframe(index), x))
    }

    /// Carries out a choice from one of the editor's menus
    /// (`componentRowCallback`, `componentRowGroupCallback`, `addSource`,
    /// `addComponent`, `organizerCallback`).
    fn respond_to_menu(&mut self, panel: &mut Panel, layout: &WavetableEditLayout) {
        let Some((oscillator, menu, id)) = panel.overlay.wavetable_edit_action else {
            return;
        };
        if oscillator != self.oscillator {
            return;
        }
        panel.overlay.wavetable_edit_action = None;
        let model = panel.host.model();
        if menu == EditMenu::Title {
            // menuCallback.
            let ctx = panel.ui.ctx().clone();
            match id {
                MENU_SAVE_AS_WAVETABLE => self.save_request = true,
                MENU_IMPORT_WAVETABLE => {
                    let filters = vec![(
                        "Wavetable".to_string(),
                        popup_browser::WAVETABLE_EXTENSIONS
                            .iter()
                            .map(|extension| extension.to_string())
                            .collect(),
                    )];
                    let request = FileRequest::open("Import Wavetable", filters);
                    file_dialog::open(&ctx, dialog_id("import", self.oscillator), &request);
                }
                MENU_EXPORT_WAVETABLE => {
                    let request =
                        save_request("Export Wavetable", "Wavetable", WAVETABLE_EXTENSION);
                    file_dialog::open(&ctx, dialog_id("export", self.oscillator), &request);
                }
                MENU_EXPORT_WAV => {
                    let request = save_request("Export to .wav File", "WAV", "wav");
                    file_dialog::open(&ctx, dialog_id("export wav", self.oscillator), &request);
                }
                MENU_RESYNTHESIZE => model.resynthesize_to_wavetable(self.oscillator),
                _ => {}
            }
            return;
        }
        if menu == EditMenu::Settings {
            // barViewerCallback.
            match id {
                MENU_POWER_SCALE => self.power_scale = true,
                MENU_AMPLITUDE_SCALE => self.power_scale = false,
                MENU_ZOOM_1..=MENU_ZOOM_16 => self.zoom = (1 << (id - MENU_ZOOM_1)) as f32,
                _ => {}
            }
            return;
        }
        if let (EditMenu::Organizer(x, y), MENU_CREATE_KEYFRAME) = (menu, id) {
            // createKeyframeAtMenu.
            let offset = -(self.scroll.position as i32);
            self.create_keyframe_at(model, layout, offset, (x, y));
            return;
        }
        let Some((_, creator)) = &mut self.creator else {
            return;
        };
        if let EditMenu::WaveSource(kind, part) = menu {
            self.wave_source_menu(panel, layout, kind, part, id);
            return;
        }
        let selected = &mut self.selected;
        let overlay_frame = &mut self.overlay_frame;
        let frame_selections = &mut self.frame_selections;
        match (menu, id) {
            (EditMenu::AddSource, index) => {
                let Some(&source_type) = SOURCE_TYPES.get(index as usize) else {
                    return;
                };
                edits::add_source(creator, source_type);
                // componentAdded selects the new source's keyframe.
                let keyframe = KeyframeRef {
                    group: creator.groups.len() - 1,
                    component: 0,
                    keyframe: 0,
                };
                *selected = vec![keyframe];
                *overlay_frame = Some(keyframe);
                *frame_selections += 1;
            }
            (EditMenu::AddModifier(group), index) => {
                let Some(&modifier_type) = MODIFIER_TYPES.get(index as usize) else {
                    return;
                };
                edits::add_modifier(creator, group, modifier_type);
            }
            (EditMenu::Group(group), MENU_RESET) | (EditMenu::Modifier(group, _), MENU_RESET) => {
                let component = match menu {
                    EditMenu::Modifier(_, component) => component,
                    _ => 0,
                };
                edits::reset_component(creator, group, component);
                // componentRemoved, then componentAdded selects its keyframe.
                let keyframe = KeyframeRef {
                    group,
                    component,
                    keyframe: 0,
                };
                *selected = vec![keyframe];
                *overlay_frame = Some(keyframe);
                *frame_selections += 1;
            }
            (EditMenu::Group(group), MENU_REMOVE) => {
                edits::remove_group(creator, group);
                // componentRemoved selects the rest or deselects: either way
                // the overlays hear of no keyframe.
                *overlay_frame = None;
                selected.retain(|keyframe| keyframe.group != group);
                for keyframe in selected.iter_mut() {
                    if keyframe.group > group {
                        keyframe.group -= 1;
                    }
                }
            }
            (EditMenu::Group(group), MENU_MOVE_UP) if group > 0 => {
                edits::move_group_up(creator, group);
                swap_groups(selected, group, group - 1);
                swap_groups(overlay_frame.as_mut_slice(), group, group - 1);
            }
            (EditMenu::Group(group), MENU_MOVE_DOWN) => {
                edits::move_group_down(creator, group);
                swap_groups(selected, group, group + 1);
                swap_groups(overlay_frame.as_mut_slice(), group, group + 1);
            }
            (EditMenu::Modifier(group, component), MENU_REMOVE) => {
                edits::remove_component(creator, group, component);
                *overlay_frame = None;
                selected
                    .retain(|keyframe| keyframe.group != group || keyframe.component != component);
                for keyframe in selected.iter_mut() {
                    if keyframe.group == group && keyframe.component > component {
                        keyframe.component -= 1;
                    }
                }
            }
            (EditMenu::Modifier(group, component), MENU_MOVE_UP) if component > 0 => {
                edits::move_modifier_up(creator, group, component);
                swap_components(selected, group, component, component - 1);
                swap_components(
                    overlay_frame.as_mut_slice(),
                    group,
                    component,
                    component - 1,
                );
            }
            (EditMenu::Modifier(group, component), MENU_MOVE_DOWN) => {
                edits::move_modifier_down(creator, group, component);
                swap_components(selected, group, component, component + 1);
                swap_components(
                    overlay_frame.as_mut_slice(),
                    group,
                    component,
                    component + 1,
                );
            }
            (EditMenu::Organizer(_, _), MENU_REMOVE_KEYFRAMES) => {
                // deleteSelectedKeyframes, highest index first so the rest
                // stay put.
                *overlay_frame = None;
                let mut doomed = std::mem::take(selected);
                doomed.sort_by_key(|keyframe| std::cmp::Reverse(keyframe.keyframe));
                for keyframe in doomed {
                    edits::delete_keyframe(creator, keyframe);
                }
            }
            _ => return,
        }
        if !matches!(menu, EditMenu::Organizer(..)) {
            self.recreate_visible_frames();
        }
        self.commit(model);
    }

    /// `WavetableOrganizer::componentsChanged` after a component list edit
    /// (`recreateVisibleFrames`): one selected keyframe is selected again,
    /// so its overlay shows, and several are (`selectFrames`), which hides
    /// it.
    fn recreate_visible_frames(&mut self) {
        match self.selected.as_slice() {
            [keyframe] => self.select_frame(*keyframe),
            [] => {}
            _ => self.overlay_frame = None,
        }
    }

    /// The overlay to show, if any, with the keyframe it edits: the
    /// overlay for the type of the component whose keyframe the overlays
    /// last heard of (`frameSelected`, `WavetableOverlayFactory`).
    fn current_overlay(&self) -> Option<(OverlayKind, KeyframeRef)> {
        let keyframe = self.overlay_frame?;
        let component = self
            .creator()?
            .groups
            .get(keyframe.group)?
            .components
            .get(keyframe.component)?;
        Some((OverlayKind::of(component)?, keyframe))
    }

    /// The current overlay's controls and editors, before anything draws.
    fn overlay_input(
        &mut self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        kind: OverlayKind,
        keyframe: KeyframeRef,
    ) {
        match kind {
            OverlayKind::WaveSource(index) => {
                self.apply_interpolation(panel, index, keyframe);
                self.edit_wave_source(panel, layout, index, keyframe);
            }
            OverlayKind::Phase => {
                let phase_layout = PhaseLayout::new(&panel.scope, layout);
                let (selections, salt) = (self.frame_selections, self.oscillator);
                let Some(WavetableComponent::PhaseModifier(modifier)) =
                    component_mut(&mut self.creator, keyframe)
                else {
                    return;
                };
                self.phase.sync(panel.host, selections, keyframe, modifier);
                let edit = self.phase.handle(
                    panel,
                    layout,
                    &phase_layout,
                    modifier,
                    keyframe.keyframe,
                    salt,
                );
                self.frame_edited(panel.host.model(), edit);
            }
            OverlayKind::Window => {
                let (selections, salt) = (self.frame_selections, self.oscillator);
                let Some(WavetableComponent::WaveWindow(modifier)) =
                    component_mut(&mut self.creator, keyframe)
                else {
                    return;
                };
                self.window.sync(panel.host, selections, keyframe, modifier);
                let edit = self
                    .window
                    .handle(panel, layout, modifier, keyframe.keyframe, salt);
                self.frame_edited(panel.host.model(), edit);
            }
            OverlayKind::LineSource => {
                let selections = self.frame_selections;
                let Some(WavetableComponent::LineSource(source)) =
                    component_mut(&mut self.creator, keyframe)
                else {
                    return;
                };
                self.line_source
                    .sync(panel.host, selections, keyframe, source);
                let edit = self
                    .line_source
                    .handle(panel, layout, source, keyframe.keyframe);
                self.frame_edited(panel.host.model(), edit);
            }
        }
    }

    /// The current overlay's first drawing, before the displays (upstream's
    /// sub-section render).
    fn paint_overlay(
        &self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        kind: OverlayKind,
        keyframe: KeyframeRef,
    ) {
        match kind {
            OverlayKind::WaveSource(index) => paint_wave_source_overlay(
                panel,
                layout,
                &OverlayLayout::wave_source(&panel.scope, layout),
                &self.wave_source[index],
                self.wave_frame(keyframe),
            ),
            OverlayKind::Phase | OverlayKind::Window => {
                self.paint_modifier_overlay(panel, layout, kind, keyframe, false)
            }
            OverlayKind::LineSource => self
                .line_source
                .paint(panel, &LineSourceLayout::new(&panel.scope, layout)),
        }
    }

    /// The current overlay's second drawing, after the displays
    /// (`renderOpenGlComponents`' render of `current_overlay_`), which also
    /// handles its controls.
    fn show_overlay(
        &self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        kind: OverlayKind,
        keyframe: KeyframeRef,
    ) {
        match kind {
            OverlayKind::WaveSource(index) => show_wave_source_overlay(
                panel,
                layout,
                &OverlayLayout::wave_source(&panel.scope, layout),
                &self.wave_source[index],
                self.wave_frame(keyframe),
            ),
            OverlayKind::Phase | OverlayKind::Window => {
                self.paint_modifier_overlay(panel, layout, kind, keyframe, true)
            }
            OverlayKind::LineSource => {
                let line = self
                    .component(keyframe)
                    .and_then(|component| match component {
                        WavetableComponent::LineSource(source) => source
                            .keyframes
                            .keyframes
                            .get(keyframe.keyframe)
                            .map(|data| &data.data.line_generator),
                        _ => None,
                    });
                let line_layout = LineSourceLayout::new(&panel.scope, layout);
                self.line_source.show(panel, layout, &line_layout, line);
            }
        }
    }

    /// The component `keyframe` belongs to.
    fn component(&self, keyframe: KeyframeRef) -> Option<&WavetableComponent> {
        self.creator()?
            .groups
            .get(keyframe.group)?
            .components
            .get(keyframe.component)
    }

    /// The Phase Shift or Wave Window overlay's drawing; the second also
    /// hands its controls their input.
    fn paint_modifier_overlay(
        &self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        kind: OverlayKind,
        keyframe: KeyframeRef,
        handle_controls: bool,
    ) {
        match (kind, self.component(keyframe)) {
            (OverlayKind::Phase, Some(WavetableComponent::PhaseModifier(modifier))) => {
                let phase = modifier
                    .keyframes
                    .keyframes
                    .get(keyframe.keyframe)
                    .map(|data| data.data.phase);
                let phase_layout = PhaseLayout::new(&panel.scope, layout);
                self.phase
                    .paint(panel, layout, &phase_layout, phase, handle_controls);
            }
            (OverlayKind::Window, Some(WavetableComponent::WaveWindow(modifier))) => {
                let window = modifier
                    .keyframes
                    .keyframes
                    .get(keyframe.keyframe)
                    .map(|data| (data.data.left_position, data.data.right_position));
                let window_layout = WindowLayout::new(&panel.scope, layout);
                self.window
                    .paint(panel, layout, &window_layout, window, handle_controls);
            }
            _ => {}
        }
    }

    /// The wave frame of a Wave Source or Shepard Tone source keyframe
    /// (`WaveSource::getWaveFrame`).
    fn wave_frame(&self, keyframe: KeyframeRef) -> Option<&WaveFrame> {
        let component = self
            .creator()?
            .groups
            .get(keyframe.group)?
            .components
            .get(keyframe.component)?;
        match component {
            WavetableComponent::WaveSource(source)
            | WavetableComponent::ShepardToneSource(source) => source
                .keyframes
                .keyframes
                .get(keyframe.keyframe)
                .map(|keyframe| &*keyframe.data.wave_frame),
            _ => None,
        }
    }

    /// `WaveSourceOverlay::sliderValueChanged` for the interpolation
    /// selector: a new value sets the source's interpolation and commits.
    /// The selector never reads the source's own setting (upstream never
    /// calls `setInterpolationType`), so it starts at "None".
    fn apply_interpolation(&mut self, panel: &mut Panel, kind: usize, keyframe: KeyframeRef) {
        let Some(value) = self.wave_source[kind].interpolation_changed(panel.host) else {
            return;
        };
        if let Some((_, creator)) = &mut self.creator {
            edits::set_wave_source_interpolation(
                creator,
                keyframe.group,
                keyframe.component,
                value,
            );
        }
        self.commit(panel.host.model());
    }

    /// The overlay's editors: the bars load for a newly selected keyframe,
    /// then the mice draw on its frame. A drag auditions the frame under the
    /// playhead; its release, or a menu choice, commits.
    fn edit_wave_source(
        &mut self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        kind: usize,
        keyframe: KeyframeRef,
    ) {
        let (selections, power_scale, zoom) = (self.frame_selections, self.power_scale, self.zoom);
        let overlay = &mut self.wave_source[kind];
        let Some(frame) = keyframe_frame(&mut self.creator, keyframe) else {
            return;
        };
        overlay.sync(selections, keyframe, frame, power_scale, zoom);
        let controls = OverlayLayout::wave_source(&panel.scope, layout).controls;
        let edit = overlay.handle_mouse(panel, layout, controls, frame, (self.oscillator, kind));
        self.frame_edited(panel.host.model(), edit);
    }

    /// A choice from a Wave Source overlay's editor menu.
    fn wave_source_menu(
        &mut self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        kind: usize,
        part: WaveSourcePart,
        choice: i32,
    ) {
        let Some((OverlayKind::WaveSource(current), keyframe)) = self.current_overlay() else {
            return;
        };
        if current != kind {
            return;
        }
        let bounds = match part {
            WaveSourcePart::Waveform => layout.waveform,
            WaveSourcePart::Amplitudes => layout.amplitudes,
            WaveSourcePart::Phases => layout.phases,
        };
        let overlay = &mut self.wave_source[kind];
        let Some(frame) = keyframe_frame(&mut self.creator, keyframe) else {
            return;
        };
        let edit = overlay.menu_choice(part, choice, bounds, frame);
        self.frame_edited(panel.host.model(), edit);
    }

    /// `frameChanged` and `frameDoneEditing`.
    fn frame_edited(&mut self, model: &dyn EditorModel, edit: Option<FrameEdit>) {
        match edit {
            Some(FrameEdit::Changed) => self.audition_frame(model),
            Some(FrameEdit::Done) => {
                if self.commit(model) {
                    self.audition.clear();
                } else {
                    // The drawn frames mustn't keep playing what the saved
                    // creator doesn't hold: end the drag, and read the saved
                    // creator back.
                    self.audition.end(model);
                    if let Some((read, _)) = &mut self.creator {
                        *read = u64::MAX;
                    }
                    // The bars load again from the saved frame.
                    self.frame_selections += 1;
                }
            }
            None => {}
        }
    }

    /// `frameChanged`: the frame under the playhead renders into the playing
    /// table (`render(position)`), without committing. The drag's token is
    /// read at its first change.
    fn audition_frame(&mut self, model: &dyn EditorModel) {
        let Some((_, creator)) = &self.creator else {
            return;
        };
        let token = match self.audition.token(self.oscillator) {
            Some(token) => token,
            None => {
                let Some(token) = model.wavetable_audition_token(self.oscillator) else {
                    return;
                };
                self.audition.start(self.oscillator, token);
                token
            }
        };
        // The table is dropped before the frame renders into it, so it isn't
        // copied.
        let frames = model
            .wavetable(self.oscillator)
            .map_or(1, |wavetable| wavetable.num_frames());
        let position = self.playhead.min(frames.max(1) - 1);
        model.audition_wavetable_frame(self.oscillator, creator.clone(), position, token);
    }

    /// The three displays of the frame under the playhead
    /// (`updateTimeDomain`, `updateFrequencyDomain`, `setColors`), each
    /// faded while an overlay covers it (`kObscureAmount`).
    fn show_displays(
        &self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        frame: Option<&[f32; WAVEFORM_SIZE]>,
        obscured: Obscured,
    ) {
        let scope = &panel.scope;
        let primary = scope.color(ColorId::WidgetPrimaryDisabled);
        let secondary = scope.color(ColorId::WidgetSecondaryDisabled);
        let fade = |colour: crate::colour::Colour, obscure: bool| {
            if obscure {
                colour.interpolated_with(colour.with_alpha(0), OBSCURE_AMOUNT)
            } else {
                colour
            }
        };
        let fade_alpha = 1.0 - scope.value(ValueId::WidgetFillFade);

        let mut waveform = LineRenderer::new(WAVEFORM_SIZE, false);
        waveform.fill = true;
        waveform.fit = true;
        waveform.line_width = scope.value(ValueId::WidgetLineWidth);
        waveform.color = fade(primary, obscured.time_domain);
        let fill = fade(secondary, obscured.time_domain);
        waveform.set_fill_colors(fill.with_multiplied_alpha(fade_alpha), fill);
        let mut amplitudes = BarRenderer::new(NUM_REAL_COMPLEX, true);
        amplitudes.square_scale = true;
        amplitudes.power_scale = self.power_scale;
        amplitudes.scale = self.zoom;
        amplitudes.color = fade(secondary, obscured.amplitudes);
        let mut phases = BarRenderer::new(NUM_REAL_COMPLEX, true);
        phases.scale = self.zoom;
        phases.color = fade(secondary, obscured.phases);

        // `resized` and `setLineValues`: the points in the display's pixels.
        let width = layout.waveform.width as f32;
        let adjust = layout.waveform.height as f32 / 2.0;
        for i in 0..WAVEFORM_SIZE {
            waveform.set_x_at(i, i as f32 * width / (WAVEFORM_SIZE as f32 - 1.0));
            waveform.set_y_at(i, adjust);
        }
        if let Some(frame) = frame {
            for (i, &sample) in frame.iter().enumerate() {
                waveform.set_y_at(i, adjust * (1.0 - sample));
            }
            for (i, (amplitude, phase)) in frequency_domain(frame).into_iter().enumerate() {
                amplitudes.set_scaled_y(i, amplitude);
                phases.set_y(i, phase);
            }
        }
        let mut nudged: RectF = layout.waveform.into();
        nudged.y += GL_TIE_NUDGE;
        waveform.paint(panel.pixels, nudged, true);
        amplitudes.paint(panel.pixels, layout.amplitudes.into());
        phases.paint(panel.pixels, layout.phases.into());
    }

    /// The organizer's rows, keyframes, selection box and playhead line
    /// (`setRowQuads`, `setFrameQuads`, `positionSelectionBox`,
    /// `playheadMoved`), with the rows scrolled with the list.
    fn show_organizer(
        &self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        rows: &[ComponentRow],
        scroll: i32,
    ) {
        let scope = &panel.scope;
        let organizer = layout.organizer;
        let handle = layout.handle_width;
        let offset = -scroll;
        let lighten = scope.color(ColorId::LightenScreen);
        let edge = scope.color(ColorId::Background);
        let primary = scope.color(ColorId::WidgetPrimary1);
        let unselected_alt = scope.color(ColorId::WidgetPrimaryDisabled);
        let mut quads = Vec::new();
        for row in rows {
            let y = organizer.y + offset + row.row as i32 * handle + 2;
            quads.push(Quad::new(
                QuadKind::Color,
                RectF::new(
                    organizer.x as f32,
                    y as f32,
                    organizer.width as f32,
                    (handle - 2) as f32,
                ),
                lighten,
            ));
        }

        let frame_width = layout.frame_width();
        let mut unselected = Vec::new();
        let mut selected = Vec::new();
        for row in rows {
            let y = organizer.y + offset + row.row as i32 * handle;
            for (index, &position) in row.positions.iter().enumerate() {
                let bounds = if row.has_keyframes {
                    let x = (frame_width * position as f32) as i32;
                    Rect::new(organizer.x + x, y, handle, handle)
                } else {
                    Rect::new(organizer.x, y, organizer.width, handle)
                };
                let keyframe = KeyframeRef {
                    group: row.group,
                    component: row.component,
                    keyframe: index,
                };
                let mut quad = Quad::new(QuadKind::Diamond, bounds.into(), edge);
                quad.thickness = DIAMOND_THICKNESS;
                if self.selected.contains(&keyframe) {
                    quad.alt_color = primary;
                    selected.push(quad);
                } else {
                    quad.alt_color = unselected_alt;
                    unselected.push(quad);
                }
            }
        }
        quads.extend(unselected);
        quads.extend(selected);

        if let Some(press) = self
            .organizer
            .filter(|press| press.mode == OrganizerMode::Selecting)
        {
            let selection = selection_box(layout, offset, press.down, press.current);
            quads.push(Quad::new(
                QuadKind::Color,
                selection.translated(organizer.x, organizer.y).into(),
                lighten,
            ));
        }

        let x = organizer.x + handle / 2 + (self.playhead as f32 * frame_width) as i32;
        quads.push(Quad::new(
            QuadKind::Color,
            RectF::new(x as f32, organizer.y as f32, 1.0, organizer.height as f32),
            primary,
        ));
        let clip = RectF::from(organizer);
        panel.layer().custom(move |pixels| {
            let mut batch = crate::primitives::QuadBatch::new();
            for quad in quads {
                batch.push(quad);
            }
            batch.paint(pixels, clip);
        });
    }

    /// `WavetableComponentList::positionGroups`: a row per component with
    /// its name and menu button, an "Add Modifier" button after each
    /// group's rows and "Add Source" after the last, scrolled and clipped
    /// to the list, with its scroll bar. The buttons open their menus
    /// (`menuClicked`, `addModifierClicked`, `addSourceClicked`).
    fn show_component_list(
        &self,
        panel: &mut Panel,
        layout: &WavetableEditLayout,
        rows: &[ComponentRow],
        group_sizes: &[usize],
        scroll: i32,
    ) {
        let list = layout.component_list;
        let row_height = layout.handle_width;
        let rounding = panel.scope.value(ValueId::LabelBackgroundRounding);
        let cell_height = layout.cell_height();
        let lighten = panel.scope.color(ColorId::LightenScreen);
        let name_size = row_height as f32 * 0.5;
        let scrolled = |rect: Rect| rect.translated(0, -scroll);

        // `component_backgrounds_`: a rounded rectangle from a row height
        // less the rounding in (two for a modifier) past the right edge.
        let mut backgrounds = Vec::new();
        let mut names = Vec::new();
        for row in rows {
            let modifier = row.component > 0;
            let name = scrolled(layout.component_name(row.row, modifier));
            let mut quad = Quad::new(
                QuadKind::RoundedRectangle,
                RectF::new(
                    name.x as f32 - rounding,
                    name.y as f32,
                    list.width as f32 + row_height as f32,
                    cell_height as f32,
                ),
                lighten,
            );
            quad.rounding = rounding;
            backgrounds.push(quad);
            names.push((row.name, name));
        }

        // The add buttons (`OpenGlToggleButton`s with `setUiButton(false)`:
        // a UI button, its text light at 0.45 of its height, indented past
        // the plus) and their plus icons.
        let mut buttons = Vec::new();
        let mut row = 0;
        for (group, &size) in group_sizes.iter().enumerate() {
            row += size;
            buttons.push((
                "Add Modifier",
                EditMenu::AddModifier(group),
                scrolled(layout.add_button(&panel.scope, row, true)),
                scrolled(layout.add_plus(&panel.scope, row, true)),
            ));
            row += 1;
        }
        buttons.push((
            "Add Source",
            EditMenu::AddSource,
            scrolled(layout.add_button(&panel.scope, row, false)),
            scrolled(layout.add_plus(&panel.scope, row, false)),
        ));
        let mut button_colours = Vec::new();
        for &(text, menu, button, _) in &buttons {
            let (clicked, colour) =
                ui_button(panel, ("wavetable add", text, menu_key(menu)), button, list);
            button_colours.push(colour);
            if clicked {
                let (items, menu) = match menu {
                    EditMenu::AddModifier(group) => {
                        (names_menu(&MODIFIER_TYPES), EditMenu::AddModifier(group))
                    }
                    _ => (names_menu(&SOURCE_TYPES), EditMenu::AddSource),
                };
                let position = Pos2::new(button.x as f32, button.bottom() as f32);
                panel.overlay.menu = Some(Menu::new(
                    items,
                    position,
                    MenuOwner::WavetableEdit(self.oscillator, menu),
                ));
            }
        }

        let button_text = panel.scope.color(ColorId::UiButtonText);
        let button_font_size = 0.45 * cell_height as f32;
        let (plus_key, plus_path) = sized_path("plus", cell_height, paths::plus);
        let text_buffer = (rounding * 0.5 + cell_height as f32) as i32;
        let ratio = panel.scope.size_ratio();
        let bar = Rect::new(
            list.x,
            list.y,
            (ratio * SCROLL_BAR_WIDTH) as i32,
            list.height,
        );
        let total = self.list_height(layout) as f32;
        let scroll_quad = self
            .scroll
            .quad(&panel.scope, bar, total, list.height as f32);
        let painted: Vec<_> = buttons
            .iter()
            .zip(button_colours)
            .map(|(&(text, _, button, plus), colour)| (text, button, plus, colour))
            .collect();
        panel.layer().custom(move |pixels| {
            pixels.with_clip(list, |pixels| {
                let mut batch = crate::primitives::QuadBatch::new();
                for quad in backgrounds {
                    batch.push(quad);
                }
                for &(_, button, _, colour) in &painted {
                    let mut quad = Quad::new(QuadKind::RoundedRectangle, button.into(), colour);
                    quad.rounding = rounding;
                    batch.push(quad);
                }
                batch.paint(pixels, list.into());
                for (name, bounds) in names {
                    pixels.text_line(
                        name,
                        bounds,
                        (Face::Light, name_size, crate::colour::Colour(0xffff_ffff)),
                        Justify::Left,
                        Overflow::Fit,
                    );
                }
                for (text, button, plus, _) in painted {
                    let text_bounds = Rect::new(
                        button.x + text_buffer,
                        button.y,
                        button.width - 2 * text_buffer,
                        button.height,
                    );
                    pixels.text_line(
                        text,
                        text_bounds,
                        (Face::Light, button_font_size, button_text),
                        Justify::Left,
                        Overflow::Fit,
                    );
                    icon::fill_path_to_fit(pixels, plus_key, plus_path, plus.into(), button_text);
                }
                // The scroll bar is always on top.
                if let Some(quad) = scroll_quad {
                    let mut batch = crate::primitives::QuadBatch::new();
                    batch.push(quad);
                    batch.paint(pixels, bar.into());
                }
            });
        });

        let colours = ShapeColours::icon(&panel.scope);
        let (menu_key, menu_path) = sized_path("wavetable row menu", cell_height, paths::menu);
        for row in rows {
            let bounds = scrolled(layout.component_menu(row.row));
            if bounds.bottom() <= list.y || bounds.y >= list.bottom() {
                continue;
            }
            let event = panel.shape_button(
                ("wavetable component menu", self.oscillator, row.row),
                ShapeButton {
                    shape: Shape::Icon {
                        key: menu_key,
                        path: menu_path,
                    },
                    colours,
                },
                bounds,
            );
            if event.clicked {
                let (items, menu) = self.row_menu(row, group_sizes);
                let position = Pos2::new(bounds.x as f32, bounds.bottom() as f32);
                panel.overlay.menu = Some(Menu::new(
                    items,
                    position,
                    MenuOwner::WavetableEdit(self.oscillator, menu),
                ));
            }
        }
    }

    /// A row's menu (`groupMenuClicked`, `modifierMenuClicked`).
    fn row_menu(&self, row: &ComponentRow, group_sizes: &[usize]) -> (Vec<MenuItem>, EditMenu) {
        let mut items = Vec::new();
        if row.component == 0 {
            if row.group > 0 {
                items.push(MenuItem::new(MENU_MOVE_UP, "Move Group Up"));
            }
            if row.group + 1 < group_sizes.len() {
                items.push(MenuItem::new(MENU_MOVE_DOWN, "Move Group Down"));
            }
            items.push(MenuItem::new(MENU_RESET, "Reset Source"));
            items.push(MenuItem::new(MENU_REMOVE, "Remove Group"));
            (items, EditMenu::Group(row.group))
        } else {
            if row.component > 1 {
                items.push(MenuItem::new(MENU_MOVE_UP, "Move Up"));
            }
            if row.component + 1 < group_sizes[row.group] {
                items.push(MenuItem::new(MENU_MOVE_DOWN, "Move Down"));
            }
            items.push(MenuItem::new(MENU_RESET, "Reset"));
            items.push(MenuItem::new(MENU_REMOVE, "Remove"));
            (items, EditMenu::Modifier(row.group, row.component))
        }
    }

    /// `WavetablePlayhead::setPositionQuad`.
    fn show_playhead(&self, panel: &mut Panel, layout: &WavetableEditLayout) {
        let playhead = layout.playhead;
        let padding = layout.playhead_padding();
        let active_width = playhead.width as f32 - 2.0 * padding + 1.0;
        let x = ((active_width * self.playhead as f32) / (NUM_FRAMES as f32 - 1.0) - 0.5 + padding)
            as i32;
        panel.quad(Quad::new(
            QuadKind::Color,
            RectF::new(
                (playhead.x + x) as f32,
                playhead.y as f32,
                1.0,
                playhead.height as f32,
            ),
            panel.scope.color(ColorId::WidgetPrimary1),
        ));
    }
}

/// Each oscillator's wave frame control, which the playhead sets
/// (`setWaveFrameSlider`).
const WAVE_FRAME_NAMES: [&str; 3] = ["osc_1_wave_frame", "osc_2_wave_frame", "osc_3_wave_frame"];

fn wave_frame_name(oscillator: usize) -> &'static str {
    WAVE_FRAME_NAMES[oscillator.min(WAVE_FRAME_NAMES.len() - 1)]
}

/// How far a waveform is nudged down so that an edge exactly between two
/// rows fills the lower one, as GL's rasterizer does (the fill meets its
/// centre on a row boundary at odd display heights), in pixels.
pub(super) const GL_TIE_NUDGE: f32 = 0.01;

/// The component `keyframe` belongs to, in `creator`, to edit.
fn component_mut(
    creator: &mut Option<(u64, WavetableCreator)>,
    keyframe: KeyframeRef,
) -> Option<&mut WavetableComponent> {
    creator
        .as_mut()?
        .1
        .groups
        .get_mut(keyframe.group)?
        .components
        .get_mut(keyframe.component)
}

/// `pos` (points) in interface pixels.
pub(super) fn to_pixels(pixels: &PixelPainter, pos: Pos2) -> (f32, f32) {
    let area = pixels.area();
    let ppp = pixels.pixels_per_point();
    ((pos.x - area.min.x) * ppp, (pos.y - area.min.y) * ppp)
}

/// `getUnclampedPositionFromX`: the position at organizer x, truncated as
/// C++'s float-to-int conversion does.
fn unclamped_position(x: i32, frame_width: f32) -> i32 {
    (x as f32 / frame_width) as i32
}

/// `getRowFromY` for organizer y with the rows scrolled by `offset`
/// (`draw_vertical_offset_`).
fn row_from_y(layout: &WavetableEditLayout, offset: i32, y: i32) -> usize {
    ((y - offset) as f32 / layout.handle_width as f32).max(0.0) as usize
}

/// `positionSelectionBox`: the selection box for a drag from `down` to
/// `at`, in organizer pixels.
fn selection_box(
    layout: &WavetableEditLayout,
    offset: i32,
    down: (i32, i32),
    at: (i32, i32),
) -> Rect {
    let handle = layout.handle_width;
    let frame_width = layout.frame_width();
    let position =
        |x: i32| unclamped_position(x - handle / 2, frame_width).clamp(0, NUM_FRAMES as i32 - 1);
    let (position_start, position_end) = (position(down.0), position(at.0));
    let (row_start, row_end) = (
        row_from_y(layout, offset, down.1) as i32,
        row_from_y(layout, offset, at.1) as i32,
    );
    let position_left = position_start.min(position_end);
    let position_right = position_start.max(position_end);
    let row_top = row_start.min(row_end);
    let row_bottom = row_start.max(row_end) + 1;
    let x = (position_left as f32 * frame_width).round() as i32;
    let y = (row_top as f32 * handle as f32).round() as i32 + offset + 1;
    let width = (position_right as f32 * frame_width).round() as i32 - x;
    let height = (row_bottom as f32 * handle as f32).round() as i32 + offset - y;
    Rect::new(x + handle / 2 - 1, y, width + 2, height)
}

/// `mouseUp` after a selection box: each row's keyframes within the box.
fn box_selection(
    layout: &WavetableEditLayout,
    offset: i32,
    rows: &[ComponentRow],
    down: (i32, i32),
    at: (i32, i32),
) -> Vec<KeyframeRef> {
    let quad = selection_box(layout, offset, down, at);
    let handle = layout.handle_width;
    let frame_width = layout.frame_width();
    let position =
        |x: i32| unclamped_position(x - handle / 2, frame_width).clamp(0, NUM_FRAMES as i32 - 1);
    let row_start = row_from_y(layout, offset, quad.y);
    let row_end = row_from_y(layout, offset, quad.bottom());
    let start_position = position(quad.x);
    let end_position = position(quad.right());
    let mut selection = Vec::new();
    for row in rows {
        if row.row < row_start || row.row >= row_end {
            continue;
        }
        // getIndexFromPosition: keyframes at or before a position.
        let index = |position: i32| row.positions.iter().take_while(|&&p| position >= p).count();
        for keyframe in index(start_position - 1)..index(end_position) {
            selection.push(KeyframeRef {
                group: row.group,
                component: row.component,
                keyframe,
            });
        }
    }
    selection
}

/// The selection after groups `a` and `b` swap (upstream's keyframe
/// pointers follow their groups).
fn swap_groups(selected: &mut [KeyframeRef], a: usize, b: usize) {
    for keyframe in selected {
        if keyframe.group == a {
            keyframe.group = b;
        } else if keyframe.group == b {
            keyframe.group = a;
        }
    }
}

/// The selection after components `a` and `b` of group `group` swap.
fn swap_components(selected: &mut [KeyframeRef], group: usize, a: usize, b: usize) {
    for keyframe in selected {
        if keyframe.group != group {
            continue;
        }
        if keyframe.component == a {
            keyframe.component = b;
        } else if keyframe.component == b {
            keyframe.component = a;
        }
    }
}

/// A UI button (`OpenGlButtonComponent::renderUiButton`) at `bounds`, seen
/// through `clip`: whether it was clicked (pressed and released over it),
/// and its background colour.
fn ui_button(
    panel: &mut Panel,
    salt: impl std::hash::Hash + std::fmt::Debug,
    bounds: Rect,
    clip: Rect,
) -> (bool, crate::colour::Colour) {
    let left = bounds.x.max(clip.x);
    let top = bounds.y.max(clip.y);
    let right = bounds.right().min(clip.right());
    let bottom = bounds.bottom().min(clip.bottom());
    if right <= left || bottom <= top {
        return (false, panel.scope.color(ColorId::UiButton));
    }
    let id = panel.ui.scope_id().with(salt);
    let visible = Rect::new(left, top, right - left, bottom - top);
    ui_button::interact(panel.ui, panel.pixels, &panel.scope, id, false, visible)
}

/// A key for an add button's id.
/// The title menu's dialog `what` for oscillator `oscillator`.
fn dialog_id(what: &str, oscillator: usize) -> egui::Id {
    egui::Id::unique("wavetable editor dialog")
        .with(what)
        .with(oscillator)
}

/// A save dialog for `extension` files with no name in it, as upstream's
/// `FileChooser(title, File(), "*.<extension>")`.
fn save_request(title: &str, filter: &str, extension: &str) -> FileRequest {
    FileRequest {
        kind: DialogKind::Save,
        file_name: None,
        ..FileRequest::open(
            title,
            vec![(filter.to_string(), vec![extension.to_string()])],
        )
    }
}

/// `exportWavetable`: the wavetable's JSON (`stateToJson`), written to
/// `file` with the wavetable extension in place of any it has.
pub fn export_wavetable(
    model: &dyn EditorModel,
    oscillator: usize,
    file: &std::path::Path,
) -> std::io::Result<()> {
    let Some(data) = model.wavetable_json(oscillator) else {
        return Ok(());
    };
    std::fs::write(file.with_extension(WAVETABLE_EXTENSION), data)
}

/// `exportToWav`: the wavetable rendered to 256 frames
/// (`renderToBuffer`), written to `file` with the `.wav` extension in
/// place of any it has, as a mono 16-bit file at 88.2 kHz with the frame
/// size in its `clm ` chunk ([`crate::wavetable_export`]).
pub fn export_wav(
    model: &dyn EditorModel,
    oscillator: usize,
    file: &std::path::Path,
) -> std::io::Result<()> {
    use crate::wavetable_export::{clm_text, wav_bytes, NUM_FRAMES, SAMPLE_RATE};
    let Some(mut creator) = model.wavetable_creator(oscillator) else {
        return Ok(());
    };
    let samples = creator.render_to_buffer(NUM_FRAMES);
    std::fs::write(
        file.with_extension("wav"),
        wav_bytes(&samples, SAMPLE_RATE, &clm_text()),
    )
}

fn menu_key(menu: EditMenu) -> usize {
    match menu {
        EditMenu::AddModifier(group) => group,
        _ => usize::MAX,
    }
}

/// A menu of component types' names, with their indices as ids
/// (`addSourceClicked`, `addModifierClicked`).
fn names_menu(
    types: &[sas_engine::sas_oscillators::sas_wavetable::creator::component::ComponentType],
) -> Vec<MenuItem> {
    types
        .iter()
        .enumerate()
        .map(|(index, kind)| MenuItem::new(index as i32, kind.name()))
        .collect()
}

/// `setPresetSelectorText`: the name, then the author after a dash when
/// there is one (`PresetSelector::setText(left, center, right)`).
fn preset_selector_text(creator: &WavetableCreator) -> String {
    if creator.author.is_empty() {
        creator.name.clone()
    } else {
        format!("{}  -  {}", creator.name, creator.author)
    }
}

/// The wave frame of a Wave Source or Shepard Tone source keyframe in
/// `creator`, to draw on.
fn keyframe_frame(
    creator: &mut Option<(u64, WavetableCreator)>,
    keyframe: KeyframeRef,
) -> Option<&mut WaveFrame> {
    let (_, creator) = creator.as_mut()?;
    let component = creator
        .groups
        .get_mut(keyframe.group)?
        .components
        .get_mut(keyframe.component)?;
    match component {
        WavetableComponent::WaveSource(source) | WavetableComponent::ShepardToneSource(source) => {
            source
                .keyframes
                .keyframes
                .get_mut(keyframe.keyframe)
                .map(|keyframe| &mut *keyframe.data.wave_frame)
        }
        _ => None,
    }
}

/// A drag on a Wave Source overlay that is auditioning: its oscillator and
/// token. Shared with the editor window, which ends it if the window closes
/// or fails during the drag (`EditorModel::end_wavetable_audition`).
#[derive(Clone, Debug, Default)]
pub struct PendingAudition(Arc<Mutex<Option<(usize, AuditionToken)>>>);

impl PendingAudition {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<(usize, AuditionToken)>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn token(&self, oscillator: usize) -> Option<AuditionToken> {
        self.lock()
            .filter(|(pending, _)| *pending == oscillator)
            .map(|(_, token)| token)
    }

    fn start(&self, oscillator: usize, token: AuditionToken) {
        *self.lock() = Some((oscillator, token));
    }

    /// The drag committed: there is nothing to end.
    fn clear(&self) {
        *self.lock() = None;
    }

    /// Ends the drag, if one is auditioning: the playing and drawn tables go
    /// back to the saved creator.
    pub fn end(&self, model: &dyn EditorModel) {
        let pending = self.lock().take();
        if let Some((oscillator, token)) = pending {
            model.end_wavetable_audition(oscillator, token);
        }
    }
}

/// The time-domain samples of frame `position` of `wavetable`
/// (`getBuffer(position)`).
fn frame_samples(wavetable: &Wavetable, position: usize) -> [f32; WAVEFORM_SIZE] {
    *wavetable.buffer(position.min(NUM_FRAMES - 1))
}

/// `updateFrequencyDomain`: each harmonic's amplitude and its phase as a
/// fraction of pi, for the first half of the spectrum.
pub fn frequency_domain(frame: &[f32; WAVEFORM_SIZE]) -> Vec<(f32, f32)> {
    let mut compute = WaveFrame::new();
    compute.load_time_domain(frame);
    (0..WAVEFORM_SIZE / 2)
        .map(|i| {
            let value = compute.frequency_domain[i];
            let amplitude = value.abs() / WAVEFORM_SIZE as f32;
            let mut phase = if amplitude > AMPLITUDE_EPSILON {
                value.arg()
            } else {
                -std::f32::consts::PI / 2.0
            };
            if phase >= std::f32::consts::PI - PHASE_EPSILON {
                phase = -std::f32::consts::PI;
            }
            (amplitude, phase / std::f32::consts::PI)
        })
        .collect()
}

/// `WavetableOrganizer::paintBackground`: the background, the half-handle
/// strips at either end and a line every few positions.
fn paint_organizer_background(pixels: &PixelPainter, scope: &Scope, layout: &WavetableEditLayout) {
    let organizer = layout.organizer;
    pixels.fill_rect(organizer, scope.color(ColorId::Background));
    let lighten = scope.color(ColorId::LightenScreen);
    let half_handle = (0.5 * layout.handle_width as f32) as i32;
    let half_lighten = lighten.with_multiplied_alpha(0.5);
    pixels.fill_rect(
        Rect::new(organizer.x, organizer.y, half_handle, organizer.height),
        half_lighten,
    );
    pixels.fill_rect(
        Rect::new(
            organizer.right() - half_handle,
            organizer.y,
            half_handle,
            organizer.height,
        ),
        half_lighten,
    );
    let frame_width = layout.frame_width();
    for i in (ORGANIZER_DRAW_SKIP..NUM_FRAMES - 1).step_by(ORGANIZER_DRAW_SKIP) {
        let colour = if i % ORGANIZER_DRAW_SKIP_LARGE == 0 {
            lighten
        } else {
            half_lighten
        };
        let x = (i as f32 * frame_width + half_handle as f32) as i32;
        pixels.fill_rect(
            Rect::new(organizer.x + x, organizer.y, 1, organizer.height),
            colour,
        );
    }
}

/// `WavetablePlayhead::paintBackground`: a tick every few positions, taller
/// every sixteenth.
fn paint_playhead_background(pixels: &PixelPainter, scope: &Scope, layout: &WavetableEditLayout) {
    let playhead = layout.playhead;
    let padding = layout.playhead_padding();
    let active_width = playhead.width as f32 - 2.0 * padding + 1.0;
    let lighten = scope.color(ColorId::LightenScreen);
    let small_line_height = playhead.height / 3;
    let big_line_height = 2 * small_line_height;
    for i in (0..NUM_FRAMES).step_by(PLAYHEAD_LINE_SKIP) {
        let x = (padding - 0.5 + i as f32 * active_width / (NUM_FRAMES as f32 - 1.0)) as i32;
        let line_height = if i % PLAYHEAD_BIG_LINE_SKIP == 0 {
            big_line_height
        } else {
            small_line_height
        };
        pixels.fill_rect(
            Rect::new(
                playhead.x + x,
                playhead.bottom() - line_height,
                1,
                line_height,
            ),
            lighten,
        );
    }
}

/// A pixel-snapped icon built for a button `height` pixels tall, with its
/// cache key; one is kept per name and height.
fn sized_path(
    name: &'static str,
    height: i32,
    make: fn(i32) -> Path,
) -> (&'static str, &'static Path) {
    type Paths = HashMap<(&'static str, i32), (&'static str, &'static Path)>;
    static PATHS: OnceLock<Mutex<Paths>> = OnceLock::new();
    let mut cache = PATHS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *cache.entry((name, height)).or_insert_with(|| {
        let key: &'static str = Box::leak(format!("{name} {height}").into_boxed_str());
        (key, Box::leak(Box::new(make(height))))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sas_engine::sas_oscillators::sas_wavetable::creator::component::{
        ComponentType, WavetableComponent,
    };
    use sas_engine::sas_oscillators::sas_wavetable::creator::group::WavetableGroup;

    /// A sine at the first harmonic is half its amplitude in bin 1, at a
    /// phase of minus a half turn (`std::arg` of a sine's bin), and the
    /// silent bins show `-kPi / 2` (`updateFrequencyDomain`).
    #[test]
    fn a_sines_frequency_domain() {
        let mut frame = [0.0f32; WAVEFORM_SIZE];
        for (i, sample) in frame.iter_mut().enumerate() {
            *sample = (2.0 * std::f32::consts::PI * i as f32 / WAVEFORM_SIZE as f32).sin();
        }
        let bins = frequency_domain(&frame);
        assert_eq!(bins.len(), WAVEFORM_SIZE / 2);
        assert!((bins[1].0 - 0.5).abs() < 1e-5, "{:?}", bins[1]);
        assert!((bins[1].1 + 0.5).abs() < 1e-4, "{:?}", bins[1]);
        assert!(bins[3].0 < 1e-5);
        assert_eq!(bins[3].1, -0.5);
    }

    /// Each group's components take a row, then a row for its "Add
    /// Modifier" button.
    #[test]
    fn rows_leave_a_row_after_each_group() {
        let mut creator = WavetableCreator::new();
        creator.clear();
        let mut first = WavetableGroup::new();
        let mut source = WavetableComponent::new(ComponentType::WaveSource);
        source.insert_new_keyframe(0);
        source.insert_new_keyframe(128);
        first.add_component(source);
        let mut fold = WavetableComponent::new(ComponentType::WaveFolder);
        fold.insert_new_keyframe(0);
        first.add_component(fold);
        creator.add_group(first);
        let mut second = WavetableGroup::new();
        let mut line = WavetableComponent::new(ComponentType::LineSource);
        line.insert_new_keyframe(64);
        second.add_component(line);
        creator.add_group(second);

        let rows = component_rows(&creator);
        let summary: Vec<_> = rows
            .iter()
            .map(|row| (row.name, row.row, row.positions.clone()))
            .collect();
        assert_eq!(
            summary,
            [
                ("Wave Source", 0, vec![0, 128]),
                ("Wave Folder", 1, vec![0]),
                ("Line Source", 3, vec![64]),
            ]
        );
    }

    /// Wave Sources and Shepard Tone sources get the two Wave Source
    /// overlays, covering every display; a Line Source, a Phase Shift and a
    /// Wave Window get theirs, the Line Source's and the Wave Window's
    /// covering the waveform and the Phase Shift's nothing; other
    /// components have no overlay yet.
    #[test]
    fn components_map_to_their_overlays() {
        let overlay = |kind| OverlayKind::of(&WavetableComponent::new(kind));
        assert_eq!(
            overlay(ComponentType::WaveSource),
            Some(OverlayKind::WaveSource(0))
        );
        assert_eq!(
            overlay(ComponentType::ShepardToneSource),
            Some(OverlayKind::WaveSource(1))
        );
        assert_eq!(
            overlay(ComponentType::LineSource),
            Some(OverlayKind::LineSource)
        );
        assert_eq!(
            overlay(ComponentType::PhaseModifier),
            Some(OverlayKind::Phase)
        );
        assert_eq!(
            overlay(ComponentType::WaveWindow),
            Some(OverlayKind::Window)
        );
        assert_eq!(overlay(ComponentType::FileSource), None);
        let all = Obscured {
            time_domain: true,
            amplitudes: true,
            phases: true,
        };
        assert_eq!(OverlayKind::WaveSource(0).obscures(), all);
        let waveform = Obscured {
            time_domain: true,
            ..Obscured::default()
        };
        assert_eq!(OverlayKind::LineSource.obscures(), waveform);
        assert_eq!(OverlayKind::Window.obscures(), waveform);
        assert_eq!(OverlayKind::Phase.obscures(), Obscured::default());
    }

    #[test]
    fn the_author_follows_the_name() {
        let mut creator = WavetableCreator::new();
        creator.name = "Bells".to_string();
        assert_eq!(preset_selector_text(&creator), "Bells");
        creator.author = "Someone".to_string();
        assert_eq!(preset_selector_text(&creator), "Bells  -  Someone");
    }
}
