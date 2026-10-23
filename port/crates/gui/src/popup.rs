// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface's popups: the value bubble a control shows while it is
//! used, and the selection list a right click or a selector opens.
//!
//! Ports `PopupDisplay`, `PopupList` and `SinglePopupSelector`
//! (`vital/src/interface/editor_sections/popup_browser.cpp`) and the
//! placement `FullInterface::popupDisplay` / `popupSelector` give them.

use egui::{Pos2, Sense, Ui};

use crate::controls::slider::Placement;
use crate::fonts::Face;
use crate::layout::Rect;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::scope::Scope;
use crate::skin::{ColorId, Section, ValueId};

/// `PopupDisplay::kHeight` / `PopupList::kRowHeight`.
const POPUP_HEIGHT: f32 = 24.0;
const ROW_HEIGHT: f32 = 24.0;
/// `PopupList::getBrowseWidth`'s `kMinWidth`.
const MIN_LIST_WIDTH: f32 = 150.0;

/// The skin sections the popups draw with.
const POPUP_SECTIONS: &[Section] = &[Section::PopupBrowser];

/// A value bubble next to a control.
#[derive(Clone, Debug, PartialEq)]
pub struct ValuePopup {
    pub text: String,
    /// The control's bounds (pixels).
    pub source: Rect,
    pub placement: Placement,
}

impl ValuePopup {
    /// `PopupDisplay::setContent`: where the bubble goes.
    pub fn bounds(&self, pixels: &PixelPainter, size_ratio: f32) -> Rect {
        let height = (POPUP_HEIGHT * size_ratio) as i32;
        let padding = height / 4;
        let buffer = padding * 2 + 2;
        let font_size = height as f32 * 0.5;
        let width = pixels.text_width(&self.text, Face::Light, font_size) as i32 + buffer;
        let source = self.source;
        let middle_x = source.x + source.width / 2;
        let middle_y = source.y + source.height / 2;
        match self.placement {
            Placement::Above => Rect::new(middle_x - width / 2, source.y - height, width, height),
            Placement::Below => Rect::new(middle_x - width / 2, source.bottom(), width, height),
            Placement::Left => Rect::new(source.x - width, middle_y - height / 2, width, height),
            Placement::Right => Rect::new(source.right(), middle_y - height / 2, width, height),
        }
    }

    /// `PopupDisplay`: a rounded body, a one-pixel border and the text.
    pub fn paint(&self, pixels: &PixelPainter, top: &Scope) {
        let scope = top.within(POPUP_SECTIONS);
        let bounds = self.bounds(pixels, top.size_ratio());
        let rounding = scope.value(ValueId::BodyRounding) as i32 as f32;
        let mut quads = QuadBatch::new();
        let mut body = Quad::new(
            QuadKind::RoundedRectangle,
            bounds.into(),
            scope.color(ColorId::Body),
        );
        body.rounding = rounding;
        quads.push(body);
        let mut border = Quad::new(
            QuadKind::RoundedRectangleBorder,
            bounds.into(),
            scope.color(ColorId::Border),
        );
        border.rounding = rounding;
        border.thickness = 1.0;
        quads.push(border);
        quads.paint(pixels, RectF::from(bounds));
        let font_size = (POPUP_HEIGHT * top.size_ratio()) as i32 as f32 * 0.5;
        pixels.text_centred(
            &self.text,
            bounds,
            Face::Light,
            font_size,
            scope.color(ColorId::BodyText),
        );
    }
}

/// One row of a selection list (`PopupItems`). A negative id is a divider.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub id: i32,
    pub name: String,
}

impl MenuItem {
    pub fn new(id: i32, name: impl Into<String>) -> MenuItem {
        MenuItem {
            id,
            name: name.into(),
        }
    }

    pub fn divider() -> MenuItem {
        MenuItem::new(-1, "")
    }
}

/// An open `SinglePopupSelector`.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu<T> {
    pub items: Vec<MenuItem>,
    /// Where it was asked for, in pixels.
    pub position: Pos2,
    /// What the selection is for.
    pub owner: T,
    /// Whether the menu has been shown. The press that opens a menu is not
    /// a press outside it.
    pub shown: bool,
}

impl<T> Menu<T> {
    pub fn new(items: Vec<MenuItem>, position: Pos2, owner: T) -> Menu<T> {
        Menu {
            items,
            position,
            owner,
            shown: false,
        }
    }
}

/// What the user did with an open menu this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuOutcome {
    Open,
    Selected(i32),
    Closed,
}

impl<T> Menu<T> {
    fn row_height(size_ratio: f32) -> i32 {
        (size_ratio * ROW_HEIGHT) as i32
    }

    /// `SinglePopupSelector::setPosition` inside the interface's `area`.
    pub fn bounds(&self, pixels: &PixelPainter, scope: &Scope, area: Rect) -> Rect {
        let size_ratio = scope.size_ratio();
        let rounding = scope.value(ValueId::BodyRounding) as i32;
        let row_height = Self::row_height(size_ratio);
        let padding = row_height / 4;
        let buffer = padding * 2 + 2;
        let font_size = row_height as f32 * 0.55;
        let mut width = (MIN_LIST_WIDTH * size_ratio) as i32;
        for item in &self.items {
            width =
                width.max(pixels.text_width(&item.name, Face::Light, font_size) as i32 + buffer);
        }
        let height = row_height * self.items.len() as i32 + 2 * rounding;
        let mut x = self.position.x as i32;
        let mut y = self.position.y as i32;
        if x + width > area.right() {
            x -= width;
        }
        if y + height > area.bottom() {
            y = area.bottom() - height;
        }
        Rect::new(x, y, width, height)
    }

    /// Draws the list and handles the mouse: hovering highlights a row, a
    /// release over a row selects it, and a press anywhere else closes it
    /// (`focusLost`) without consuming the press.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        pixels: &PixelPainter,
        top: &Scope,
        area: Rect,
    ) -> MenuOutcome {
        let scope = top.within(POPUP_SECTIONS);
        let bounds = self.bounds(pixels, &scope, area);
        let rounding = scope.value(ValueId::BodyRounding) as i32;
        let list = Rect::new(
            bounds.x + 1,
            bounds.y + rounding,
            bounds.width - 2,
            bounds.height - 2 * rounding,
        );
        let row_height = Self::row_height(top.size_ratio());

        let response = ui.interact(
            pixels.rect(bounds.into()),
            ui.scope_id().with("popup selector"),
            Sense::click_and_drag(),
        );
        let pointer = ui.input(|input| input.pointer.interact_pos());
        let pressed = ui.input(|input| input.pointer.any_pressed());
        let row_at = |pos: Pos2| -> Option<usize> {
            let px = (pos.x - pixels.area().min.x) * pixels.pixels_per_point();
            let py = (pos.y - pixels.area().min.y) * pixels.pixels_per_point();
            if px < list.x as f32 || px > list.right() as f32 {
                return None;
            }
            let row = ((py - list.y as f32) / row_height as f32).floor();
            let row = row as i32;
            if row < 0 || row as usize >= self.items.len() || self.items[row as usize].id < 0 {
                return None;
            }
            Some(row as usize)
        };
        let hovered = pointer
            .filter(|_| response.hovered() || response.dragged())
            .and_then(row_at);

        let mut quads = QuadBatch::new();
        let mut body = Quad::new(
            QuadKind::RoundedRectangle,
            bounds.into(),
            scope.color(ColorId::Body),
        );
        body.rounding = rounding as f32;
        quads.push(body);
        let mut border = Quad::new(
            QuadKind::RoundedRectangleBorder,
            bounds.into(),
            scope.color(ColorId::Border),
        );
        border.rounding = rounding as f32;
        border.thickness = 1.0;
        quads.push(border);
        quads.paint(pixels, RectF::from(bounds));

        if let Some(row) = hovered {
            let mut hover = QuadBatch::additive();
            let row_rect = RectF::new(
                list.x as f32,
                (list.y + row as i32 * row_height) as f32,
                list.width as f32,
                row_height as f32,
            );
            hover.push(Quad::new(
                QuadKind::Color,
                row_rect,
                scope.color(ColorId::WidgetPrimary1).darker(0.8),
            ));
            hover.paint(pixels, RectF::from(list));
        }

        let padding = row_height / 4;
        let text_colour = scope.color(ColorId::TextComponentText);
        let lighten = scope.color(ColorId::LightenScreen);
        let font_size = row_height as f32 * 0.55;
        for (index, item) in self.items.iter().enumerate() {
            let row_y = list.y + row_height * index as i32;
            if item.id < 0 {
                let y = (row_height as f32 * (index as f32 + 0.5)) as i32 + list.y;
                pixels.fill_rect(
                    Rect::new(list.x + padding, y, list.width - 2 * padding, 1),
                    lighten,
                );
            } else {
                let row = Rect::new(
                    list.x + padding,
                    row_y,
                    list.width - 2 * padding,
                    row_height,
                );
                pixels.text_left(&item.name, row, Face::Light, font_size, text_colour);
            }
        }

        if response.drag_stopped() || response.clicked() {
            if let Some(row) = pointer.and_then(row_at) {
                return MenuOutcome::Selected(self.items[row].id);
            }
        }
        if pressed && self.shown && !response.hovered() {
            return MenuOutcome::Closed;
        }
        self.shown = true;
        MenuOutcome::Open
    }
}
