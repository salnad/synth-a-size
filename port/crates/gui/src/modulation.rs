// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Dragging a modulation source onto a control: the drag-to-modulate part
//! of upstream's `ModulationManager`
//! (`vital/src/interface/editor_sections/modulation_manager.cpp`) and
//! `ModulationButton`'s mouse handling (`modulation_button.cpp`).
//!
//! A press on a source button selects it (`modulationSelected`). Dragging
//! out of the button starts the map (`startModulationMap`): every visible
//! modulatable control lights up, and while the pointer is over one the
//! source is connected to it for real, so the modulation is heard
//! (`modulationDraggedToComponent`); moving off disconnects it again
//! (`clearTemporaryModulation`). A modifier key makes the connection
//! bipolar (`setTemporaryModulationBipolar`). The connection left at the
//! release stays (`endModulationMap`).
//!
//! Upstream tells the host nothing here: the values go to the synth with
//! `valueChanged` and the connection through the engine's change queue.
//! The port's slot values are host parameters, so the connection that
//! survives the release is written to them then, one gesture per parameter
//! (D18). The ones the drag only passed over reach the engine through the
//! edit bus and never the host.
//!
//! Each frame, [`ModulationManager::begin_frame`] reads the connections,
//! the sections register their modulatable sliders as they draw them
//! ([`ModulationManager::register_destination`]) and their source buttons
//! report their mouse ([`ModulationManager::source_button`]), and
//! [`ModulationManager::finish_frame`] connects what the drag is over and
//! draws the overlay.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use egui::{Id, PointerButton, Pos2, Sense, Ui};
use sas_preset::constants::MAX_MODULATION_CONNECTIONS;
use sas_preset::{GuiConnection, SlotValues};

use crate::colour::Colour;
use crate::controls::modulation_button::{
    menu_source_display_name, modulation_amount_bounds, modulation_area_bounds,
};
use crate::controls::slider::{Placement, Slider, SliderStyle};
use crate::host::Host;
use crate::interface::Route;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::popup::MenuItem;
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::scope::Scope;
use crate::skin::{ColorId, Section, ValueId};

/// `ModulationManager`'s skin override (`kModulationDragDrop`).
const DRAG_DROP_SKIN: &[Section] = &[Section::ModulationDragDrop];

/// `SynthSlider::kLinearWidthPercent`.
const LINEAR_WIDTH_PERCENT: f32 = 0.26;

/// `OpenGlMultiQuad::kAlphaInc`: how far the destinations fade per frame.
const ALPHA_INC: f32 = 0.2;

/// `ModulationButton::MenuId`: disconnect every connection, or (from this
/// id on) one of them.
const MENU_DISCONNECT_ALL: i32 = 1;
const MENU_MODULATION_LIST: i32 = 2;

/// `ModulationButton::mouseDown`'s menu for `source`: "Disconnect from"
/// each destination, in the order the synth keeps its connections
/// (`getSourceConnections`), then "Disconnect all" when there are several.
pub fn disconnect_menu(routes: &[Route], source: &str) -> Vec<MenuItem> {
    let connections: Vec<&Route> = routes
        .iter()
        .filter(|route| route.source == source)
        .collect();
    let mut items: Vec<MenuItem> = connections
        .iter()
        .enumerate()
        .map(|(i, route)| {
            let name = &Host::info(&route.destination).display_name;
            MenuItem::new(
                MENU_MODULATION_LIST + i as i32,
                format!("Disconnect from {name}"),
            )
        })
        .collect();
    if connections.len() > 1 {
        items.push(MenuItem::new(MENU_DISCONNECT_ALL, "Disconnect all"));
    }
    items
}

/// `ModulationButton::disconnectIndex`: the menu's choice `id` for
/// `source`. Upstream tells the host nothing; the slot's values stay.
pub fn disconnect_choice(host: &Host, source: &str, id: i32) {
    let model = host.model();
    let routes = model.modulations();
    let connections = routes.iter().filter(|route| route.source == source);
    if id == MENU_DISCONNECT_ALL {
        for route in connections {
            model.disconnect_modulation(source, &route.destination);
        }
    } else if id >= MENU_MODULATION_LIST {
        if let Some(route) = connections
            .clone()
            .nth((id - MENU_MODULATION_LIST) as usize)
        {
            model.disconnect_modulation(source, &route.destination);
        }
    }
}

/// `ModulationAmountKnob::MenuId`.
const MENU_REMOVE: i32 = 0xff;
const MENU_TOGGLE_BYPASS: i32 = 0x100;
const MENU_TOGGLE_BIPOLAR: i32 = 0x101;
const MENU_TOGGLE_STEREO: i32 = 0x102;

/// Slot `slot`'s control `suffix` (`modulation_{slot + 1}_{suffix}`).
pub fn slot_control(slot: usize, suffix: &str) -> &'static str {
    Host::info(&format!("modulation_{}_{suffix}", slot + 1))
        .name
        .as_str()
}

/// `ModulationAmountKnob::mouseDown`'s menu for slot `slot`: remove it,
/// toggle its bypass, polarity and stereo, and enter a value
/// (`manual_entry` is the slider menu's id for that).
pub fn amount_knob_menu(host: &Host, slot: usize, manual_entry: i32) -> Vec<MenuItem> {
    let on = |suffix: &str| host.value(slot_control(slot, suffix)) != 0.0;
    vec![
        MenuItem::new(MENU_REMOVE, "Remove"),
        MenuItem::new(
            MENU_TOGGLE_BYPASS,
            if on("bypass") { "Unbypass" } else { "Bypass" },
        ),
        MenuItem::new(
            MENU_TOGGLE_BIPOLAR,
            if on("bipolar") {
                "Make Unipolar"
            } else {
                "Make Bipolar"
            },
        ),
        MenuItem::new(
            MENU_TOGGLE_STEREO,
            if on("stereo") {
                "Make Mono"
            } else {
                "Make Stereo"
            },
        ),
        MenuItem::divider(),
        MenuItem::new(manual_entry, "Enter Value"),
    ]
}

/// `ModulationAmountKnob::handleModulationMenuCallback` for the choice
/// `id` on slot `slot`. Removing disconnects it (no host write, as
/// upstream's). The toggles set the slot's host parameter in one gesture
/// (D18; upstream sets the synth's value only). False for an id it
/// doesn't handle.
pub fn amount_knob_choice(host: &Host, slot: usize, id: i32) -> bool {
    let toggle = |suffix: &str| {
        let name = slot_control(slot, suffix);
        let on = host.value(name) != 0.0;
        host.set_once(name, if on { 0.0 } else { 1.0 });
    };
    match id {
        MENU_REMOVE => {
            let model = host.model();
            if let Some(route) = model.modulations().iter().find(|route| route.slot == slot) {
                model.disconnect_modulation(&route.source, &route.destination);
            }
        }
        MENU_TOGGLE_BYPASS => toggle("bypass"),
        MENU_TOGGLE_BIPOLAR => toggle("bipolar"),
        MENU_TOGGLE_STEREO => toggle("stereo"),
        _ => return false,
    }
    true
}

/// How a destination lights up while a source is dragged
/// (`ModulationDestination`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DestinationShape {
    /// A knob: a circle the size of its modulation meter, centred on the
    /// knob (`getFillBounds`, in pixels).
    Rotary(RectF),
    /// A text or curve slider: its whole bounds.
    Rectangle,
    /// `LinearBar`: a band along the slider.
    Horizontal,
    /// `LinearBarVertical`: a band up the slider.
    Vertical,
}

/// A modulatable control drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Destination {
    pub name: &'static str,
    pub bounds: Rect,
    /// The area it shows in (its section, or the scrolled effects column).
    pub clip: Rect,
    pub shape: DestinationShape,
    /// The side its selected source's amount knob goes
    /// (`getModulationPlacement`).
    pub placement: Placement,
    /// Its `kRotaryArc` colour, which its amount knobs take.
    pub arc: Colour,
}

/// The host parameter values a connection made during a drag takes when the
/// drag ends. Shared with the editor window, which commits it if the window
/// closes mid-drag.
#[derive(Clone, Debug, Default)]
pub struct PendingConnection(Arc<Mutex<Option<Pending>>>);

/// Host parameters and the values they take.
type HostValues = Vec<(&'static str, f32)>;

/// A connection's host values, with the route and the synth state they
/// belong to.
#[derive(Clone, Debug)]
struct Pending {
    route: Route,
    /// [`EditorModel::edit_generation`](crate::EditorModel::edit_generation)
    /// when the drag connected it.
    generation: u64,
    values: HostValues,
}

impl PendingConnection {
    fn set(&self, pending: Option<Pending>) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = pending;
    }

    /// Writes the pending values to the host, one gesture each, and forgets
    /// them. Nothing is written if a preset load or state restore has
    /// replaced the synth's state since, or the route no longer holds its
    /// slot.
    pub fn commit(&self, host: &Host) {
        let pending = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        let Some(pending) = pending else {
            return;
        };
        let model = host.model();
        if model.edit_generation() != pending.generation
            || !model.modulations().contains(&pending.route)
        {
            return;
        }
        for (name, value) in pending.values {
            host.set_once(name, value);
        }
    }
}

/// The connection the drag has made (`temporarily_set_destination_`).
#[derive(Debug)]
struct Temporary {
    destination: &'static str,
    connection: GuiConnection,
    values: SlotValues,
}

impl Temporary {
    /// What the host is to be told for this connection from `source`.
    fn pending(&self, source: &str, generation: u64) -> Pending {
        Pending {
            route: Route {
                slot: self.connection.slot,
                source: source.to_string(),
                destination: self.destination.to_string(),
            },
            generation,
            values: self.host_values(),
        }
    }

    /// The connection's host parameter changes with the slot's current
    /// values.
    fn host_values(&self) -> HostValues {
        let flag = |on: bool| if on { 1.0 } else { 0.0 };
        self.connection
            .parameter_changes
            .iter()
            .map(|&(name, value)| {
                let value = match name.rsplit('_').next() {
                    Some("amount") => self.values.amount,
                    Some("bipolar") => flag(self.values.bipolar),
                    Some("stereo") => flag(self.values.stereo),
                    Some("bypass") => flag(self.values.bypass),
                    _ => value,
                };
                (name, value)
            })
            .collect()
    }
}

/// A source being dragged (`dragging_`, `current_source_`).
#[derive(Debug)]
struct Drag {
    source: &'static str,
    /// The pointer, in editor pixels (`mouse_drag_position_`).
    pointer: Pos2,
    /// A modifier key is down.
    bipolar: bool,
    temporary: Option<Temporary>,
    /// Released this frame.
    ended: bool,
}

/// A source button's mouse state across frames.
#[derive(Clone, Copy, Debug, Default)]
struct ButtonState {
    down: bool,
    /// Dragged out of the button (`kDraggingOut`).
    out: bool,
}

/// What a source button's mouse did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SourceInput {
    /// Pressed and released on the button without dragging out of it
    /// (`modulationClicked`).
    pub clicked: bool,
    /// A right-click on a connected source: its disconnect menu, here
    /// (pixels).
    pub menu: Option<Pos2>,
}

/// A source button drawn this frame, with its amount knobs' area.
#[derive(Clone, Copy, Debug)]
struct SourceButton {
    name: &'static str,
    area: Rect,
}

/// Which of a slot's amount knobs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KnobKind {
    /// In its source button (`modulation_amount_sliders_`).
    Source,
    /// Beside its destination while its source is selected
    /// (`selected_modulation_sliders_`).
    Selected,
    /// Beside its destination after the pointer went over that control
    /// (`modulation_hover_sliders_`).
    Hover,
}

/// A modulation's amount knob to show this frame (`ModulationAmountKnob`).
#[derive(Clone, Debug)]
pub struct AmountKnob {
    pub kind: KnobKind,
    pub route: Route,
    /// The knob: a slider on the slot's `modulation_N_amount`.
    pub slider: Slider,
    pub bounds: Rect,
}

/// The modulation sources' editor state (`ModulationManager`).
#[derive(Debug, Default)]
pub struct ModulationManager {
    routes: Option<Arc<[Route]>>,
    /// The selected source (`current_modulator_`).
    current: Option<&'static str>,
    /// Where the selected source's button is drawn this frame.
    current_bounds: Option<Rect>,
    drag: Option<Drag>,
    destinations: Vec<Destination>,
    /// The destinations' quads as last drawn, each with its clip, kept to
    /// fade out after the drag.
    quads: Vec<(Quad, Rect)>,
    alpha: f32,
    pending: PendingConnection,
    /// The synth state's generation as of this frame.
    generation: u64,
    /// The source buttons drawn this frame.
    sources: Vec<SourceButton>,
    /// Each destination's `kRotaryArc` colour, as last drawn
    /// (`slider_model_lookup_`'s sliders keep theirs while hidden).
    arcs: HashMap<&'static str, Colour>,
    /// The destination whose other modulations show beside it
    /// (`makeModulationsVisible` from `hoverStarted`).
    hovered: Option<&'static str>,
    /// The amount knobs shown last frame.
    knobs: Vec<AmountKnob>,
}

impl ModulationManager {
    /// The connections, read from `routes` for this frame, with the synth
    /// state's `generation`; forgets last frame's destinations. A
    /// connection the drag made that a preset load, a state restore or
    /// another editor of the synth (the chat) has removed since is
    /// forgotten, and nothing will be written for it.
    pub fn begin_frame(&mut self, routes: Arc<[Route]>, generation: u64) {
        let restored = generation != self.generation;
        self.generation = generation;
        if let Some(drag) = &mut self.drag {
            let gone = drag.temporary.as_ref().is_some_and(|temporary| {
                !routes.iter().any(|route| {
                    route.slot == temporary.connection.slot
                        && route.source == drag.source
                        && route.destination == temporary.destination
                })
            });
            if restored || gone {
                drag.temporary = None;
                self.pending.set(None);
            }
        }
        self.routes = Some(routes);
        self.destinations.clear();
        self.sources.clear();
        self.current_bounds = None;
    }

    /// The connected slots, as of this frame.
    pub fn routes(&self) -> &[Route] {
        self.routes.as_deref().unwrap_or(&[])
    }

    /// Whether `source` modulates anything (`getSourceConnections`).
    pub fn is_connected(&self, source: &str) -> bool {
        self.routes().iter().any(|route| route.source == source)
    }

    fn connected(&self, source: &str, destination: &str) -> bool {
        self.routes()
            .iter()
            .any(|route| route.source == source && route.destination == destination)
    }

    /// The selected source.
    pub fn current(&self) -> Option<&'static str> {
        self.current
    }

    /// Whether a source is being dragged.
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The host values that a window closing mid-drag must still write.
    pub fn pending_connection(&self) -> PendingConnection {
        self.pending.clone()
    }

    /// A modulatable control drawn at `bounds` within `clip`.
    pub fn register_destination(&mut self, destination: Destination) {
        self.arcs.insert(destination.name, destination.arc);
        self.destinations.push(destination);
    }

    /// Source `name`'s button at `bounds` (pixels): its press, drag and
    /// release (`ModulationButton::mouseDown` / `mouseDrag` / `mouseUp`).
    pub fn source_button(
        &mut self,
        ui: &mut Ui,
        pixels: &PixelPainter,
        id: Id,
        name: &'static str,
        bounds: Rect,
        widget_margin: i32,
    ) -> SourceInput {
        if self.current == Some(name) {
            self.current_bounds = Some(bounds);
        }
        self.sources.push(SourceButton {
            name,
            area: modulation_area_bounds(bounds, widget_margin),
        });
        let response = ui.interact(pixels.rect(bounds.into()), id, Sense::click_and_drag());
        let mut state: ButtonState = ui.data(|data| data.get_temp(id)).unwrap_or_default();
        let (primary_down, secondary_pressed, pointer, any_modifier) = ui.input(|input| {
            (
                input.pointer.button_down(PointerButton::Primary),
                input.pointer.button_pressed(PointerButton::Secondary),
                input.pointer.interact_pos(),
                input.modifiers.any(),
            )
        });
        let to_pixels =
            |pos: Pos2| ((pos - pixels.area().min) * pixels.pixels_per_point()).to_pos2();
        let mut input = SourceInput::default();

        if secondary_pressed && response.hovered() && self.is_connected(name) {
            input.menu = pointer.map(to_pixels);
        }
        if !state.down && primary_down && response.is_pointer_button_down_on() {
            state.down = true;
            // modulationSelected: the source's knobs replace any shown on
            // hover.
            self.current = Some(name);
            self.current_bounds = Some(bounds);
            self.hovered = None;
        }
        if state.down && primary_down {
            if let Some(pointer) = pointer.map(to_pixels) {
                let inside = RectF::from(bounds).contains(pointer.x, pointer.y);
                if !inside && !state.out {
                    state.out = true;
                    self.start_map(name, pointer);
                }
                if state.out {
                    if let Some(drag) = &mut self.drag {
                        drag.pointer = pointer;
                        drag.bipolar = any_modifier;
                    }
                }
            }
        } else if state.down {
            if state.out {
                if let Some(drag) = &mut self.drag {
                    drag.ended = true;
                }
            } else if pointer.is_some_and(|pos| response.rect.contains(pos)) {
                input.clicked = true;
            }
            state = ButtonState::default();
        }
        ui.data_mut(|data| data.insert_temp(id, state));
        input
    }

    /// `startModulationMap`: nothing starts without a free slot
    /// (`hasFreeConnection`).
    fn start_map(&mut self, source: &'static str, pointer: Pos2) {
        if self.routes().len() >= MAX_MODULATION_CONNECTIONS {
            return;
        }
        self.drag = Some(Drag {
            source,
            pointer,
            bipolar: false,
            temporary: None,
            ended: false,
        });
    }

    /// The destination under `pointer` for a drag from `source`: the last
    /// drawn of the controls containing it (`getComponentAt`), not the
    /// source's own control nor one switched off.
    fn destination_at(&self, pointer: Pos2, source: &str) -> Option<&'static str> {
        self.destinations
            .iter()
            .rev()
            .find(|destination| {
                destination.name != source
                    && RectF::from(destination.bounds).contains(pointer.x, pointer.y)
                    && RectF::from(destination.clip).contains(pointer.x, pointer.y)
            })
            .map(|destination| destination.name)
    }

    /// `modulationDragged` for the drag's pointer, then `endModulationMap`
    /// if it was released; then the overlay.
    pub fn finish_frame(&mut self, host: &Host, pixels: &PixelPainter, top: &Scope) {
        if let Some(mut drag) = self.drag.take() {
            if drag.ended {
                self.pending.commit(host);
            } else {
                self.drag_over(host, &mut drag);
                self.drag = Some(drag);
            }
        }
        self.paint(pixels, top);
    }

    /// `modulationDragged`: leaves a destination the pointer moved off,
    /// follows the modifier key on the one it is over, and connects a new
    /// one.
    fn drag_over(&mut self, host: &Host, drag: &mut Drag) {
        let model = host.model();
        let over = self.destination_at(drag.pointer, drag.source);
        if let Some(temporary) = &mut drag.temporary {
            if over != Some(temporary.destination) {
                // clearTemporaryModulation.
                if model
                    .disconnect_modulation(drag.source, temporary.destination)
                    .is_some()
                {
                    drag.temporary = None;
                    self.pending.set(None);
                }
            } else if temporary.values.bipolar != drag.bipolar {
                // setTemporaryModulationBipolar.
                let info = Host::info(temporary.destination);
                let values =
                    SlotValues::dragged_to(info, host.value(temporary.destination), drag.bipolar);
                if model.set_connection_values(drag.source, temporary.destination, values) {
                    temporary.values = values;
                    self.pending
                        .set(Some(temporary.pending(drag.source, self.generation)));
                }
            }
        }
        // modulationDraggedToComponent.
        let Some(name) = over else {
            return;
        };
        if drag.temporary.is_some() || self.connected(drag.source, name) {
            return;
        }
        let values = SlotValues::dragged_to(Host::info(name), host.value(name), drag.bipolar);
        if let Some(connection) = model.connect_modulation(drag.source, name, values) {
            let temporary = Temporary {
                destination: name,
                connection,
                values,
            };
            self.pending
                .set(Some(temporary.pending(drag.source, self.generation)));
            drag.temporary = Some(temporary);
        }
    }

    /// The selected source's outline (`drawCurrentModulator`) and the lit
    /// destinations (`drawModulationDestinations`).
    fn paint(&mut self, pixels: &PixelPainter, top: &Scope) {
        let scope = top.within(DRAG_DROP_SKIN);
        let control = scope.color(ColorId::ModulationMeterControl);
        let full = RectF::from(pixels_bounds(pixels));

        if let Some(bounds) = self.current_bounds {
            let mut outline = Quad::new(QuadKind::RoundedRectangleBorder, bounds.into(), control);
            outline.thickness = if self.drag.is_some() { 2.6 } else { 1.0 };
            let mut batch = QuadBatch::new();
            batch.push(outline);
            batch.paint(pixels, full);
        }

        if let Some(drag) = &self.drag {
            self.quads = self.destination_quads(&scope, top, drag.source);
        }
        let target = if self.drag.is_some() { 1.0 } else { 0.0 };
        self.alpha = if self.alpha < target {
            (self.alpha + ALPHA_INC).min(target)
        } else {
            (self.alpha - ALPHA_INC).max(target)
        };
        if self.alpha > 0.0 {
            for (quad, clip) in &self.quads {
                let mut quad = *quad;
                quad.alpha_mult = self.alpha;
                let mut batch = QuadBatch::new();
                batch.push(quad);
                batch.paint(pixels, RectF::from(*clip));
            }
        }
    }

    /// The dragged source's ring at the pointer, until it is over a
    /// destination (`drawDraggingModulation`), over the amount knobs.
    pub fn paint_drag(&self, host: &Host, pixels: &PixelPainter, top: &Scope) {
        let scope = top.within(DRAG_DROP_SKIN);
        let control = scope.color(ColorId::ModulationMeterControl);
        let full = RectF::from(pixels_bounds(pixels));
        let Some(drag) = &self.drag else {
            return;
        };
        if drag.temporary.is_some() {
            return;
        }
        const RADIUS_WIDTH_RATIO: f32 = 0.022;
        const THICKNESS_WIDTH_RATIO: f32 = 0.003;
        let width = full.width;
        let radius = RADIUS_WIDTH_RATIO * width;
        let percent = source_value(host, drag.source);
        let mut ring = Quad::new(
            QuadKind::Ring,
            RectF::new(
                drag.pointer.x - radius,
                drag.pointer.y - radius,
                2.0 * radius,
                2.0 * radius,
            ),
            control,
        );
        ring.alt_color = scope
            .color(ColorId::WidgetBackground)
            .interpolated_with(control, percent);
        ring.thickness = width * THICKNESS_WIDTH_RATIO;
        let mut batch = QuadBatch::new();
        batch.push(ring);
        batch.paint(pixels, full);
    }

    /// `setDestinationQuadBounds` for every control the drag can reach:
    /// those already connected to `source` are left out (upstream moves
    /// their quads off screen).
    fn destination_quads(&self, scope: &Scope, top: &Scope, source: &str) -> Vec<(Quad, Rect)> {
        let colour = scope.color(ColorId::LightenScreen);
        let margin = scope.value(ValueId::WidgetMargin) as i32 as f32;
        let rounding = top.value(ValueId::LabelBackgroundRounding);
        self.destinations
            .iter()
            .filter(|destination| {
                destination.name != source && !self.connected(source, destination.name)
            })
            .map(|destination| {
                let bounds = RectF::from(destination.bounds);
                let quad = match destination.shape {
                    DestinationShape::Rotary(fill) => Quad::new(QuadKind::Circle, fill, colour),
                    DestinationShape::Rectangle => {
                        let mut quad = Quad::new(QuadKind::RoundedRectangle, bounds, colour);
                        quad.rounding = rounding;
                        quad
                    }
                    DestinationShape::Horizontal | DestinationShape::Vertical => {
                        let fill = linear_fill(bounds, destination.shape, margin);
                        let mut quad = Quad::new(QuadKind::RoundedRectangle, fill, colour);
                        quad.rounding = rounding;
                        quad
                    }
                };
                (quad, destination.clip)
            })
            .collect()
    }
}

impl ModulationManager {
    /// The amount knobs to show this frame: in each source button drawn
    /// (`positionModulationAmountSlidersInside`), beside the selected
    /// source's destinations (`makeCurrentModulatorAmountsVisible`), and
    /// beside the control the pointer last went over for its other sources
    /// (`makeModulationsVisible`). `pointer` is in pixels.
    pub fn amount_knobs(
        &mut self,
        host: &Host,
        top: &Scope,
        pointer: Option<Pos2>,
        editor_width: i32,
    ) -> Vec<AmountKnob> {
        if self.drag.is_none() {
            if let Some(name) = pointer.and_then(|pointer| self.control_at(pointer)) {
                self.hovered = Some(name);
            }
        }
        let scope = top.within(DRAG_DROP_SKIN);
        let mut knobs = Vec::new();
        self.source_knobs(host, top, editor_width, &mut knobs);
        self.selected_knobs(host, &scope, top.size_ratio(), &mut knobs);
        self.hover_knobs(host, &scope, top.size_ratio(), &mut knobs);
        self.knobs = knobs.clone();
        knobs
    }

    /// The amount knobs shown last frame.
    pub fn shown_knobs(&self) -> &[AmountKnob] {
        &self.knobs
    }

    /// The control under `pointer` (any modulatable one drawn).
    fn control_at(&self, pointer: Pos2) -> Option<&'static str> {
        self.destinations
            .iter()
            .rev()
            .find(|destination| {
                RectF::from(destination.bounds).contains(pointer.x, pointer.y)
                    && RectF::from(destination.clip).contains(pointer.x, pointer.y)
            })
            .map(|destination| destination.name)
    }

    /// `positionModulationAmountSliders` for each source button drawn: its
    /// connections' knobs in rows of three when they fit
    /// (`kMaxModulationsAcross`). Those that don't go in its callout.
    fn source_knobs(
        &self,
        host: &Host,
        top: &Scope,
        editor_width: i32,
        knobs: &mut Vec<AmountKnob>,
    ) {
        const MAX_MODULATIONS_ACROSS: i32 = 3;
        const RIGHT_POPUP_POSITION_X: i32 = 150;
        for source in &self.sources {
            let connections: Vec<&Route> = self
                .routes()
                .iter()
                .filter(|route| route.source == source.name)
                .collect();
            let area = source.area;
            let max_height = (MAX_MODULATIONS_ACROSS * area.height) / area.width.max(1);
            if connections.is_empty()
                || connections.len() as i32 > MAX_MODULATIONS_ACROSS * max_height
            {
                continue;
            }
            for (i, route) in connections.into_iter().enumerate() {
                let bounds = modulation_amount_bounds(area, i);
                let placement = if editor_width - bounds.right() < RIGHT_POPUP_POSITION_X {
                    Placement::Left
                } else if bounds.x < RIGHT_POPUP_POSITION_X {
                    Placement::Right
                } else {
                    Placement::Below
                };
                let arc = self.destination_arc(top, &route.destination);
                let prefix = intern(format!("{}: ", Host::info(&route.destination).display_name));
                knobs.push(AmountKnob {
                    kind: KnobKind::Source,
                    route: route.clone(),
                    slider: knob_slider(host, KnobKind::Source, route, arc, prefix, placement),
                    bounds,
                });
            }
        }
    }

    /// `makeCurrentModulatorAmountsVisible`: the selected source's knobs,
    /// each beside its destination on the side it asks for, in the
    /// modulation meter control colour (`setCurrentModulator`).
    fn selected_knobs(
        &self,
        host: &Host,
        scope: &Scope,
        size_ratio: f32,
        knobs: &mut Vec<AmountKnob>,
    ) {
        let Some(source) = self.current else {
            return;
        };
        let width = (size_ratio * 24.0) as i32;
        let arc = scope.color(ColorId::ModulationMeterControl);
        let prefix = intern(format!("{}: ", menu_source_display_name(source)));
        for route in self.routes().iter().filter(|route| route.source == source) {
            let Some(destination) = self.drawn(&route.destination) else {
                continue;
            };
            let bounds = destination.bounds;
            let (center_x, center_y) = (bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
            let knob = match destination.placement {
                Placement::Below => Rect::new(center_x - width / 2, bounds.bottom(), width, width),
                Placement::Above => Rect::new(center_x - width / 2, bounds.y - width, width, width),
                Placement::Left => Rect::new(bounds.x - width, center_y - width / 2, width, width),
                Placement::Right => Rect::new(bounds.right(), center_y - width / 2, width, width),
            };
            knobs.push(AmountKnob {
                kind: KnobKind::Selected,
                route: route.clone(),
                slider: knob_slider(
                    host,
                    KnobKind::Selected,
                    route,
                    arc,
                    prefix,
                    destination.placement,
                ),
                bounds: knob,
            });
        }
    }

    /// `makeModulationsVisible`: the other sources' knobs beside the
    /// hovered control, centred on it in a row (above or below) or column
    /// (left or right), leaving a gap where the selected source's knob is.
    fn hover_knobs(
        &self,
        host: &Host,
        scope: &Scope,
        size_ratio: f32,
        knobs: &mut Vec<AmountKnob>,
    ) {
        let Some(name) = self.hovered else {
            return;
        };
        let Some(destination) = self.drawn(name) else {
            return;
        };
        let mut current_showing = false;
        let mut row: Vec<Option<&Route>> = Vec::new();
        for route in self
            .routes()
            .iter()
            .filter(|route| route.destination == name)
        {
            if self.current.is_some_and(|current| current == route.source) {
                current_showing = true;
            } else {
                row.push(Some(route));
            }
        }
        if current_showing {
            row.insert(row.len().div_ceil(2), None);
            if row.len().is_multiple_of(2) {
                row.push(None);
            }
        }
        let width = (size_ratio * 24.0) as i32;
        let bounds = destination.bounds;
        let offset = width * row.len() as i32 / 2;
        let (mut x, mut y, delta_x, delta_y) = match destination.placement {
            Placement::Below => (
                bounds.x + bounds.width / 2 - offset,
                bounds.bottom(),
                width,
                0,
            ),
            Placement::Above => (
                bounds.x + bounds.width / 2 - offset,
                bounds.y - width,
                width,
                0,
            ),
            Placement::Left => (
                bounds.x - width,
                bounds.y + bounds.height / 2 - offset,
                0,
                width,
            ),
            Placement::Right => (
                bounds.right(),
                bounds.y + bounds.height / 2 - offset,
                0,
                width,
            ),
        };
        let arc = scope.color(ColorId::RotaryArc);
        for route in row {
            if let Some(route) = route {
                let prefix = intern(format!("{}: ", menu_source_display_name(&route.source)));
                knobs.push(AmountKnob {
                    kind: KnobKind::Hover,
                    route: route.clone(),
                    slider: knob_slider(
                        host,
                        KnobKind::Hover,
                        route,
                        arc,
                        prefix,
                        destination.placement,
                    ),
                    bounds: Rect::new(x, y, width, width),
                });
            }
            x += delta_x;
            y += delta_y;
        }
    }

    /// The destination drawn this frame named `name`.
    fn drawn(&self, name: &str) -> Option<&Destination> {
        self.destinations
            .iter()
            .find(|destination| destination.name == name)
    }

    /// Destination `name`'s `kRotaryArc` colour: as last drawn, or from its
    /// section's skin.
    fn destination_arc(&self, top: &Scope, name: &str) -> Colour {
        if let Some(&arc) = self.arcs.get(name) {
            return arc;
        }
        top.within(destination_skin(name)).color(ColorId::RotaryArc)
    }
}

/// The skin sections a destination's control is drawn in.
fn destination_skin(name: &str) -> &'static [Section] {
    use crate::sections::{effects, envelope, extra_mod, filter, lfo, oscillator, random, sample};
    if let Some(effect) = effects::Effect::ALL
        .into_iter()
        .find(|effect| name.starts_with(&format!("{}_", effect.name())))
    {
        return effect.skin();
    }
    let prefixed = |prefix: &str| name.starts_with(prefix);
    if prefixed("filter_") {
        filter::SKIN
    } else if prefixed("osc_") {
        oscillator::SKIN
    } else if prefixed("sample_") {
        sample::SKIN
    } else if prefixed("env_") {
        envelope::SKIN
    } else if prefixed("lfo_") {
        lfo::SKIN
    } else if prefixed("random_") {
        random::SKIN
    } else if prefixed("macro_control_") {
        extra_mod::MACRO_SKIN
    } else {
        &[]
    }
}

/// A `'static` copy of `text`, kept once per distinct text (the popup
/// prefixes, a few dozen at most).
fn intern(text: String) -> &'static str {
    static TEXTS: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut texts = TEXTS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(&interned) = texts.get(&text) {
        return interned;
    }
    let interned: &'static str = Box::leak(text.clone().into_boxed_str());
    texts.insert(text, interned);
    interned
}

/// `ModulationAmountKnob` for `route`'s slot: shows its popup on hover, a
/// double click does nothing, and its text entry is twice as wide
/// (`setTextEntrySizePercent(2.0f, 1.0f)`). A linear or indexed
/// destination's amount reads in its own units (`setModulationSliderScale`).
fn knob_slider(
    host: &Host,
    kind: KnobKind,
    route: &Route,
    arc: Colour,
    prefix: &'static str,
    placement: Placement,
) -> Slider {
    use sas_preset::registry::ValueScale;
    let number = route.slot + 1;
    let name = Host::info(&format!("modulation_{number}_amount"))
        .name
        .as_str();
    let bypass = host.value(&format!("modulation_{number}_bypass")) != 0.0;
    let mut slider = Slider::new(name, SliderStyle::ModulationKnob { arc, bypass });
    slider.instance = 1 + kind as u8;
    slider.show_popup_on_hover = true;
    slider.double_click_returns = false;
    slider.text_entry_width_percent = 2.0;
    slider.popup_prefix = Some(prefix);
    slider.popup_placement = placement;
    let destination = Host::info(&route.destination);
    if matches!(
        destination.value_scale,
        ValueScale::Linear | ValueScale::Indexed
    ) {
        slider.format.display_multiply = Some(destination.max - destination.min);
    }
    slider
}

/// The editor's area in pixels.
fn pixels_bounds(pixels: &PixelPainter) -> Rect {
    let size = pixels.area().size() * pixels.pixels_per_point();
    Rect::new(0, 0, size.x.round() as i32, size.y.round() as i32)
}

/// `ModulationDestination::getFillBounds` for a linear slider: a band along
/// it, widened by `kBufferPercent` of the slider's track each side.
fn linear_fill(bounds: RectF, shape: DestinationShape, margin: f32) -> RectF {
    const BUFFER_PERCENT: f32 = 0.4;
    if shape == DestinationShape::Horizontal {
        let height = bounds.height;
        let mut y = height * 0.5 * (1.0 - LINEAR_WIDTH_PERCENT);
        let mut glow_height = height * LINEAR_WIDTH_PERCENT;
        y -= 2.0 * glow_height * BUFFER_PERCENT;
        glow_height += 4.0 * BUFFER_PERCENT * glow_height;
        return RectF::new(
            bounds.x + margin,
            bounds.y + y,
            bounds.width - 2.0 * margin,
            glow_height,
        );
    }
    let width = bounds.width;
    let mut x = width * 0.5 * (1.0 - LINEAR_WIDTH_PERCENT);
    let mut glow_width = width * LINEAR_WIDTH_PERCENT;
    x -= 2.0 * glow_width * BUFFER_PERCENT;
    glow_width += 4.0 * BUFFER_PERCENT * glow_width;
    RectF::new(
        bounds.x + x,
        bounds.y + margin,
        glow_width,
        bounds.height - 2.0 * margin,
    )
}

/// `modulation_source_readouts_[source]->value()[0]`: the source's output
/// now, 0 without telemetry.
fn source_value(host: &Host, source: &str) -> f32 {
    host.model()
        .telemetry()
        .and_then(|telemetry| telemetry.read(|snapshot| snapshot.status_by_name(source)))
        .map_or(0.0, |value| value[0])
}

/// `ModulationDestination::getFillBounds` for a knob: a circle
/// `knob_size_scale` times the modulation meter's size, centred on the
/// knob and moved down by the knob offset.
pub fn rotary_fill(scope: &Scope, bounds: Rect, knob_size_scale: f32) -> RectF {
    let offset = scope.value(ValueId::KnobOffset);
    let width = knob_size_scale * scope.value(ValueId::KnobModMeterArcSize);
    RectF::new(
        bounds.x as f32 + (bounds.width as f32 - width) / 2.0,
        bounds.y as f32 + offset + (bounds.height as f32 - width) / 2.0,
        width,
        width,
    )
}
