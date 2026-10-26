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

use std::sync::{Arc, Mutex};

use egui::{Id, PointerButton, Pos2, Sense, Ui};
use sas_preset::constants::MAX_MODULATION_CONNECTIONS;
use sas_preset::{GuiConnection, SlotValues};

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
/// each destination, then "Disconnect all" when there are several. The
/// connections are in slot order (upstream lists them in the order they
/// were made).
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
}

/// The host parameter values a connection made during a drag takes when the
/// drag ends. Shared with the editor window, which commits it if the window
/// closes mid-drag.
#[derive(Clone, Debug, Default)]
pub struct PendingConnection(Arc<Mutex<Option<HostValues>>>);

/// Host parameters and the values they take.
type HostValues = Vec<(&'static str, f32)>;

impl PendingConnection {
    fn set(&self, changes: Option<HostValues>) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = changes;
    }

    /// Writes the pending values to the host, one gesture each, and forgets
    /// them.
    pub fn commit(&self, host: &Host) {
        let changes = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        for (name, value) in changes.into_iter().flatten() {
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
}

impl ModulationManager {
    /// The connections, read from `routes` for this frame; forgets last
    /// frame's destinations.
    pub fn begin_frame(&mut self, routes: Arc<[Route]>) {
        self.routes = Some(routes);
        self.destinations.clear();
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
    ) -> SourceInput {
        if self.current == Some(name) {
            self.current_bounds = Some(bounds);
        }
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
            self.current = Some(name);
            self.current_bounds = Some(bounds);
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
        self.paint(host, pixels, top);
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
                if model.set_connection_values(temporary.connection.slot, values) {
                    temporary.values = values;
                    self.pending.set(Some(temporary.host_values()));
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
            self.pending.set(Some(temporary.host_values()));
            drag.temporary = Some(temporary);
        }
    }

    /// The selected source's outline (`drawCurrentModulator`), the lit
    /// destinations (`drawModulationDestinations`) and the dragged source's
    /// ring (`drawDraggingModulation`).
    fn paint(&mut self, host: &Host, pixels: &PixelPainter, top: &Scope) {
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
