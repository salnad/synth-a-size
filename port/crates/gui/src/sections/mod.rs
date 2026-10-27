// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface's sections and the plumbing they share.
//!
//! Each section ports one `SynthSection` subclass from
//! `vital/src/interface/editor_sections/`: its `resized` (a layout struct
//! with interface-pixel rectangles, checked against upstream's fixtures),
//! its `paintBackground`, and its controls. [`Panel`] stands in for what
//! `SynthSection` does for every control: drawing it, routing its popups to
//! the interface and its changes to the host.

pub mod advanced;
pub mod bend;
pub mod delete_section;
pub mod effects;
pub mod envelope;
pub mod extra_mod;
pub mod filter;
pub mod keyboard;
pub mod lfo;
pub mod modulation;
pub mod oscillator;
pub mod oscillator_advanced;
pub mod portamento;
pub mod preset_browser;
pub mod preset_selector;
pub mod random;
pub mod sample;
pub mod selection_list;
pub mod synthesis;
pub mod voice;
pub mod volume;

use egui::Ui;

use crate::controls::button::SynthButton;
use crate::controls::line_editor::LineAction;
use crate::controls::slider::{Slider, SliderStyle};
use crate::host::Host;
use crate::layer::Layer;
use crate::layout::Rect;
use crate::modulation::{
    rotary_fill, AmountKnob, Destination, DestinationShape, ModulationManager,
};
use crate::paint::{PixelPainter, RectF};
use crate::popup::{DualMenu, Menu, MenuItem, MenuOutcome, ValuePopup};
use crate::scope::Scope;
use crate::skin::ColorId;

/// `SynthSlider::MenuOptions` ids the port offers.
pub const MENU_DEFAULT_VALUE: i32 = 1;
pub const MENU_MANUAL_ENTRY: i32 = 2;

/// What an open menu is choosing for.
#[derive(Clone, Copy, Debug)]
pub enum MenuOwner {
    /// A slider's right-click menu (`SynthSlider::createPopupMenu`), with
    /// where its text entry would go.
    Slider(Slider, Rect),
    /// The Voice section's stereo mode selector.
    StereoMode,
    /// A `TextSelector`'s options; the selected id is the value.
    Selector(Slider),
    /// A section's list for control `name`: the selected id is the value,
    /// set in one gesture (`SynthBase::valueChangedInternal`), then `then`
    /// runs with it (the section's follow-up, e.g. resolving an FM cycle).
    /// The host applies the set later, so `then` takes the value rather
    /// than reading it back.
    Control {
        name: &'static str,
        then: Option<fn(&Host, f32)>,
    },
    /// A filter's model and style list (`FilterSection::setFilterSelected`):
    /// the id is `model * FILTER_STYLE_IDS + style`.
    FilterType {
        model: &'static str,
        style: &'static str,
    },
    /// An LFO editor's menu; the editor carries out the choice
    /// (`Overlay::line_action`).
    Line(LineAction),
    /// An oscillator's wavetable view, by index (`Wavetable3d::mouseDown`):
    /// the selection goes to [`Overlay::wavetable_action`] for the view.
    Wavetable(usize),
    /// An editor setting's list (the Advanced tab's tuning, frequency units
    /// and skin): the choice's id goes to the function, with the egui
    /// context for any dialog it opens.
    Setting(fn(&Host, &egui::Context, i32)),
    /// A modulation source's disconnect menu.
    ModulationSource(&'static str),
    /// A modulation amount knob's menu (`ModulationAmountKnob`): its
    /// slider, where its text entry goes, and its slot.
    ModulationAmount(Slider, Rect, usize),
}

/// Room for each filter model's styles in a [`MenuOwner::FilterType`] id.
pub const FILTER_STYLE_IDS: i32 = 16;

/// A slider's text entry (`SynthSlider::showTextEntry`).
#[derive(Clone, Debug)]
pub struct TextEntry {
    pub slider: Slider,
    /// Interface pixels.
    pub bounds: Rect,
    pub text: String,
    pub focused: bool,
}

impl TextEntry {
    /// A text entry over `bounds` (interface pixels) holding the slider's
    /// current value (`getRawTextFromValue`).
    pub fn new(host: &Host, slider: &Slider, bounds: Rect) -> TextEntry {
        let value = host.value(slider.name);
        TextEntry {
            slider: *slider,
            bounds,
            text: crate::controls::format::raw_text(slider.info, &slider.format, value),
            focused: false,
        }
    }
}

/// The interface-wide popups (`FullInterface`'s popup display and
/// selector, and the slider text entries).
#[derive(Debug, Default)]
pub struct Overlay {
    /// The value bubble and the slider showing it.
    pub value_popup: Option<(&'static str, ValuePopup)>,
    pub menu: Option<Menu<MenuOwner>>,
    /// The two-column selector (`FullInterface::dualPopupSelector`).
    pub dual_menu: Option<DualMenu<MenuOwner>>,
    /// A choice from an LFO editor's menu, for that editor's next frame.
    pub line_action: Option<LineAction>,
    pub text_entry: Option<TextEntry>,
    /// A wavetable view's menu choice, (oscillator, item), for the view to
    /// carry out on its next frame (`Wavetable3d::respondToMenuCallback`).
    pub wavetable_action: Option<(usize, i32)>,
    /// The modulation sources: the selected one and a drag in progress.
    pub modulation: ModulationManager,
}

impl Overlay {
    /// The selection menu, if open, and what its selection does.
    pub fn show_menu(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        top: &Scope,
        area: Rect,
    ) {
        if let Some(mut menu) = self.menu.take() {
            match menu.show(ui, pixels, top, area) {
                MenuOutcome::Open => self.menu = Some(menu),
                MenuOutcome::Closed => self.menu = None,
                MenuOutcome::Selected(id) => {
                    self.menu = None;
                    match menu.owner {
                        MenuOwner::Slider(slider, _) if id == MENU_DEFAULT_VALUE => {
                            host.set_once(slider.name, slider.info.default_value);
                        }
                        MenuOwner::Slider(slider, entry_bounds) if id == MENU_MANUAL_ENTRY => {
                            self.text_entry = Some(TextEntry::new(host, &slider, entry_bounds));
                        }
                        MenuOwner::Slider(..) => {}
                        MenuOwner::StereoMode => host.set_once("stereo_mode", id as f32),
                        MenuOwner::Selector(slider) => host.set_once(slider.name, id as f32),
                        MenuOwner::Wavetable(oscillator) => {
                            self.wavetable_action = Some((oscillator, id));
                        }
                        MenuOwner::Control { name, then } => {
                            host.set_once(name, id as f32);
                            if let Some(then) = then {
                                then(host, id as f32);
                            }
                        }
                        MenuOwner::FilterType { .. } => apply_choice(host, menu.owner, id),
                        MenuOwner::Setting(then) => then(host, ui.ctx(), id),
                        MenuOwner::Line(action) => {
                            self.line_action = Some(LineAction {
                                choice: id,
                                ..action
                            })
                        }
                        MenuOwner::ModulationSource(source) => {
                            crate::modulation::disconnect_choice(host, source, id)
                        }
                        MenuOwner::ModulationAmount(slider, entry_bounds, slot) => {
                            if id == MENU_MANUAL_ENTRY {
                                self.text_entry = Some(TextEntry::new(host, &slider, entry_bounds));
                            } else {
                                crate::modulation::amount_knob_choice(host, slot, id);
                            }
                        }
                    }
                }
            }
        }
        if let Some(mut menu) = self.dual_menu.take() {
            match menu.show(ui, pixels, top, area) {
                MenuOutcome::Open => self.dual_menu = Some(menu),
                MenuOutcome::Closed => {}
                MenuOutcome::Selected(id) => {
                    // The dual selector stays open after a choice.
                    apply_choice(host, menu.owner, id);
                    self.dual_menu = Some(menu);
                }
            }
        }
    }
}

/// What choosing `id` in a section's list does, for the owners both lists
/// can have.
fn apply_choice(host: &Host, owner: MenuOwner, id: i32) {
    if let MenuOwner::FilterType { model, style } = owner {
        host.set_once(model, (id / FILTER_STYLE_IDS) as f32);
        host.set_once(style, (id % FILTER_STYLE_IDS) as f32);
    }
}

/// One section's controls being shown this frame.
pub struct Panel<'a, 'b> {
    pub ui: &'a mut Ui,
    pub host: &'a Host<'b>,
    pub pixels: &'a PixelPainter<'a>,
    pub scope: Scope<'a>,
    pub overlay: &'a mut Overlay,
    /// The modulated controls' meters, drawn under the controls
    /// (`ModulationManager::renderMeters` runs before the components
    /// render).
    meters: Layer,
    layer: Layer,
    /// The always-on-top components (`setAlwaysOnTop`), drawn after the
    /// rest: a switched-off section's overlay, then its power button.
    on_top: Layer,
    clip: Rect,
}

impl<'a, 'b> Panel<'a, 'b> {
    pub fn new(
        ui: &'a mut Ui,
        host: &'a Host<'b>,
        pixels: &'a PixelPainter<'a>,
        scope: Scope<'a>,
        overlay: &'a mut Overlay,
        clip: Rect,
    ) -> Panel<'a, 'b> {
        Panel {
            ui,
            host,
            pixels,
            scope,
            overlay,
            meters: Layer::new(),
            layer: Layer::new(),
            on_top: Layer::new(),
            clip,
        }
    }

    /// Shows `slider` at `bounds`: handles its mouse, routes its popup, menu
    /// and text entry to the overlay, and draws it.
    pub fn slider(&mut self, slider: &Slider, bounds: Rect) {
        let slider = &slider.displayed(self.host.model().hz_frequency_display());
        self.register_destination(slider, bounds);
        let (value, hover) = self.slider_input(slider, bounds);
        self.meter(slider, bounds, value);
        slider.paint(
            self.pixels,
            &mut self.layer,
            &self.scope,
            bounds,
            value,
            hover,
        );
    }

    /// `slider`'s `ModulationMeter`, if the engine is modulating it and it
    /// is on (`renderMeters`: `isModulated() && slider->isActive()`).
    fn meter(&mut self, slider: &Slider, bounds: Rect, value: f32) {
        if !slider.active {
            return;
        }
        let Some(current) = self.overlay.modulation.meter_value(slider.name) else {
            return;
        };
        let manager = Scope::top(self.scope.skin(), self.scope.size_ratio());
        if let Some(quad) = crate::modulation_meter::meter_quad(
            slider,
            bounds,
            value,
            current,
            &self.scope,
            &manager,
        ) {
            self.meters.quad(quad);
        }
    }

    /// `ModulationManager`'s `ModulationDestination` for `slider`, if a
    /// source can modulate it now: it is a modulation destination and its
    /// section is on.
    fn register_destination(&mut self, slider: &Slider, bounds: Rect) {
        if !slider.active || !sas_preset::engine::is_modulation_destination(slider.name) {
            return;
        }
        let shape = match slider.style {
            SliderStyle::Text | SliderStyle::Curve => DestinationShape::Rectangle,
            SliderStyle::Rotary => {
                DestinationShape::Rotary(rotary_fill(&self.scope, bounds, slider.knob_size_scale))
            }
            SliderStyle::Horizontal | SliderStyle::Arrow { .. } => DestinationShape::Horizontal,
            SliderStyle::Vertical | SliderStyle::Wheel { .. } => DestinationShape::Vertical,
            // Amount knobs take a drag as an aux modulation instead.
            SliderStyle::ModulationKnob { .. } => return,
        };
        self.overlay.modulation.register_destination(Destination {
            name: slider.name,
            bounds,
            clip: self.clip,
            shape,
            placement: slider.modulation_placement,
            arc: self.scope.color(ColorId::RotaryArc),
        });
    }

    /// A text toggle whose state isn't a parameter, with id `salt`; true
    /// when clicked.
    pub fn text_toggle(
        &mut self,
        salt: impl std::hash::Hash + std::fmt::Debug,
        text: &str,
        on: bool,
        bounds: Rect,
    ) -> bool {
        let id = self.ui.scope_id().with(salt);
        crate::controls::button::text_toggle(
            self.ui,
            self.pixels,
            &mut self.layer,
            &self.scope,
            id,
            text,
            on,
            bounds,
        )
    }

    /// A shape button with id `salt` in this section's scope.
    pub fn shape_button(
        &mut self,
        salt: impl std::hash::Hash + std::fmt::Debug,
        button: crate::controls::shape_button::ShapeButton,
        bounds: Rect,
    ) -> crate::controls::shape_button::ShapeEvent {
        let id = self.ui.scope_id().with(salt);
        crate::controls::shape_button::show(
            self.ui,
            self.pixels,
            &mut self.layer,
            &self.scope,
            id,
            button,
            bounds,
        )
    }

    /// [`slider`](Self::slider) without the drawing (`InvisibleSlider`),
    /// for controls that draw themselves or are drawn by something else:
    /// returns the value to draw (the held value while dragging, so the
    /// drawing doesn't lag the host) and whether the pointer is over it.
    pub fn slider_input(&mut self, slider: &Slider, bounds: Rect) -> (f32, bool) {
        self.slider_input_with_menu(slider, bounds, |slider, entry_bounds| {
            (
                vec![
                    MenuItem::new(MENU_DEFAULT_VALUE, "Set to Default Value"),
                    MenuItem::new(MENU_MANUAL_ENTRY, "Enter Value"),
                ],
                MenuOwner::Slider(*slider, entry_bounds),
            )
        })
    }

    /// A modulation's amount knob (`ModulationAmountKnob`): a slider on its
    /// slot's amount with the knob's own menu. A middle click toggles its
    /// bypass (`toggleBypass`), in one gesture (D18).
    pub fn amount_knob(&mut self, knob: &AmountKnob) {
        let host = self.host;
        let slot = knob.route.slot;
        let (value, hover) =
            self.slider_input_with_menu(&knob.slider, knob.bounds, |slider, entry_bounds| {
                (
                    crate::modulation::amount_knob_menu(host, slot, MENU_MANUAL_ENTRY),
                    MenuOwner::ModulationAmount(*slider, entry_bounds, slot),
                )
            });
        let middle = self
            .ui
            .input(|input| input.pointer.button_pressed(egui::PointerButton::Middle));
        if middle && hover {
            let bypass = crate::modulation::slot_control(slot, "bypass");
            let on = host.value(bypass) != 0.0;
            host.set_once(bypass, if on { 0.0 } else { 1.0 });
        }
        knob.slider.paint(
            self.pixels,
            &mut self.layer,
            &self.scope,
            knob.bounds,
            value,
            hover,
        );
    }

    /// [`slider_input`](Self::slider_input) with `menu` making the
    /// right-click menu's items and owner, from the slider and where its
    /// text entry goes.
    fn slider_input_with_menu(
        &mut self,
        slider: &Slider,
        bounds: Rect,
        menu: impl FnOnce(&Slider, Rect) -> (Vec<MenuItem>, MenuOwner),
    ) -> (f32, bool) {
        let slider = &slider.displayed(self.host.model().hz_frequency_display());
        let event = slider.interact(self.ui, self.host, self.pixels, bounds);
        let value = event.value.unwrap_or_else(|| self.host.value(slider.name));
        if event.popup {
            let popup = ValuePopup {
                text: slider.popup_text(value),
                source: bounds,
                placement: slider.popup_placement,
            };
            self.overlay.value_popup = Some((slider.name, popup));
        } else if !event.hover
            && self
                .overlay
                .value_popup
                .as_ref()
                .is_some_and(|(name, _)| *name == slider.name)
        {
            // SynthSlider::mouseExit hides the popup.
            self.overlay.value_popup = None;
        }
        if let Some(position) = event.menu_at {
            let area = self.pixels.area();
            let ppp = self.pixels.pixels_per_point();
            let (items, owner) = menu(slider, slider.text_entry_bounds(&self.scope, bounds));
            self.overlay.menu = Some(Menu::new(
                items,
                egui::pos2(
                    (position.x - area.min.x) * ppp,
                    (position.y - area.min.y) * ppp,
                ),
                owner,
            ));
        }
        if event.text_entry {
            self.open_text_entry(slider, bounds);
        }
        if event.selector {
            let items = slider
                .selector_options()
                .into_iter()
                .enumerate()
                .map(|(index, name)| MenuItem::new(index as i32, name))
                .collect();
            let position = egui::pos2(bounds.x as f32, bounds.bottom() as f32);
            self.overlay.menu = Some(Menu::new(items, position, MenuOwner::Selector(*slider)));
        }
        (value, event.hover)
    }

    /// Opens `slider`'s text entry over `bounds`.
    pub fn open_text_entry(&mut self, slider: &Slider, bounds: Rect) {
        let entry = TextEntry::new(
            self.host,
            slider,
            slider.text_entry_bounds(&self.scope, bounds),
        );
        self.overlay.text_entry = Some(entry);
    }

    /// Returns true when the button was clicked (pressed and released on
    /// it).
    pub fn button(&mut self, button: &SynthButton, bounds: Rect) -> bool {
        button.show(
            self.ui,
            self.host,
            self.pixels,
            &mut self.layer,
            &self.scope,
            bounds,
        )
    }

    /// A quad drawn with the section's controls.
    pub fn quad(&mut self, quad: crate::primitives::Quad) {
        self.layer.quad(quad);
    }

    /// `OpenGlComponent::renderCorners`: a widget's corners rounded off in
    /// the body colour.
    pub fn widget_corners(&mut self, bounds: Rect) {
        let rounding = self.scope.value(crate::skin::ValueId::WidgetRoundedCorner);
        let body = self.scope.color(crate::skin::ColorId::Body);
        for corner in crate::primitives::Quad::corners(bounds.into(), rounding, body) {
            self.layer.quad(corner);
        }
    }

    /// The layer the section's controls draw into, for components that
    /// paint themselves.
    pub fn layer(&mut self) -> &mut Layer {
        &mut self.layer
    }

    /// `SynthSection::OffOverlay` over `section`, on top of its controls,
    /// for an inactive section.
    pub fn off_overlay(&mut self, section: Rect) {
        self.on_top
            .quad(crate::section::off_overlay(&self.scope, section));
    }

    /// `SynthSection::setActivator`: the power button and, while it is off,
    /// the overlay over the section (both always on top).
    pub fn activator(&mut self, button: &SynthButton, section: Rect) {
        if !button.is_on(self.host) {
            self.off_overlay(section);
        }
        let bounds = crate::section::power_button_bounds(&self.scope, section);
        button.show(
            self.ui,
            self.host,
            self.pixels,
            &mut self.on_top,
            &self.scope,
            bounds,
        );
    }

    /// A `PlainTextComponent`'s centred text over the section's controls,
    /// fitted like `Graphics::drawFittedText`.
    pub fn text_centred(
        &mut self,
        text: &str,
        bounds: Rect,
        face: crate::fonts::Face,
        size: f32,
        colour: crate::colour::Colour,
    ) {
        self.layer.fitted_text(text, bounds, face, size, colour);
    }

    /// Centred text over the section's controls, curtailed like
    /// `Graphics::drawText(..., false)`.
    pub fn curtailed_text(
        &mut self,
        text: &str,
        bounds: Rect,
        face: crate::fonts::Face,
        size: f32,
        colour: crate::colour::Colour,
    ) {
        self.layer.text_centred(text, bounds, face, size, colour);
    }

    /// Draws the section's controls (quads, then text), clipped to the
    /// section.
    pub fn finish(self) {
        self.meters.paint(self.pixels, RectF::from(self.clip));
        self.layer.paint(self.pixels, RectF::from(self.clip));
        self.on_top.paint(self.pixels, RectF::from(self.clip));
    }
}
