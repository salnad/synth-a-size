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

pub mod bend;
pub mod keyboard;
pub mod portamento;
pub mod voice;
pub mod volume;

use egui::Ui;

use crate::controls::button::SynthButton;
use crate::controls::slider::Slider;
use crate::host::Host;
use crate::layer::Layer;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::popup::{Menu, MenuItem, MenuOutcome, ValuePopup};
use crate::scope::Scope;

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
}

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
    pub text_entry: Option<TextEntry>,
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
                    }
                }
            }
        }
    }
}

/// One section's controls being shown this frame.
pub struct Panel<'a, 'b> {
    pub ui: &'a mut Ui,
    pub host: &'a Host<'b>,
    pub pixels: &'a PixelPainter<'a>,
    pub scope: Scope<'a>,
    pub overlay: &'a mut Overlay,
    layer: Layer,
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
            layer: Layer::new(),
            clip,
        }
    }

    /// Shows `slider` at `bounds`: handles its mouse, routes its popup, menu
    /// and text entry to the overlay, and draws it.
    pub fn slider(&mut self, slider: &Slider, bounds: Rect) {
        let event = slider.interact(self.ui, self.host, self.pixels, bounds);
        let value = self.host.value(slider.name);
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
            self.overlay.menu = Some(Menu::new(
                vec![
                    MenuItem::new(MENU_DEFAULT_VALUE, "Set to Default Value"),
                    MenuItem::new(MENU_MANUAL_ENTRY, "Enter Value"),
                ],
                egui::pos2(
                    (position.x - area.min.x) * ppp,
                    (position.y - area.min.y) * ppp,
                ),
                MenuOwner::Slider(*slider, slider.text_entry_bounds(&self.scope, bounds)),
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
        slider.paint(
            self.pixels,
            &mut self.layer,
            &self.scope,
            bounds,
            value,
            event.hover,
        );
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

    pub fn button(&mut self, button: &SynthButton, bounds: Rect) {
        button.show(
            self.ui,
            self.host,
            self.pixels,
            &mut self.layer,
            &self.scope,
            bounds,
        );
    }

    /// A quad drawn with the section's controls.
    pub fn quad(&mut self, quad: crate::primitives::Quad) {
        self.layer.quad(quad);
    }

    /// Text over the section's controls.
    pub fn text_centred(
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
        self.layer.paint(self.pixels, RectF::from(self.clip));
    }
}
