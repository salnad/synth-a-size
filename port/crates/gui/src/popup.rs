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
use crate::text::{Justify, Overflow};

/// `PopupDisplay::kHeight` / `PopupList::kRowHeight`.
const POPUP_HEIGHT: f32 = 24.0;
const ROW_HEIGHT: f32 = 24.0;
/// `PopupList::getBrowseWidth`'s `kMinWidth`.
const MIN_LIST_WIDTH: f32 = 150.0;
/// `PopupList::kScrollBarWidth`.
const SCROLL_BAR_WIDTH: f32 = 15.0;

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
        pixels.text_line(
            &self.text,
            bounds,
            (Face::Light, font_size, scope.color(ColorId::BodyText)),
            Justify::Centred,
            Overflow::Fit,
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
                let style = (Face::Light, font_size, text_colour);
                pixels.text_line(&item.name, row, style, Justify::Left, Overflow::Ellipsis);
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

/// One left-hand row of a [`DualMenu`] and the right-hand rows it shows.
/// Without items it is chosen itself, with `id`; with `id` -1 and no name
/// it is a divider (`PopupItems` with no sub-items).
#[derive(Clone, Debug, PartialEq)]
pub struct DualMenuGroup {
    pub id: i32,
    pub name: String,
    pub items: Vec<MenuItem>,
}

impl DualMenuGroup {
    /// A named group of `items`.
    pub fn new(name: impl Into<String>, items: Vec<MenuItem>) -> DualMenuGroup {
        DualMenuGroup {
            id: 0,
            name: name.into(),
            items,
        }
    }

    /// A left-hand row chosen by itself.
    pub fn item(id: i32, name: impl Into<String>) -> DualMenuGroup {
        DualMenuGroup {
            id,
            name: name.into(),
            items: Vec::new(),
        }
    }

    pub fn divider() -> DualMenuGroup {
        DualMenuGroup::item(-1, "")
    }

    fn is_divider(&self) -> bool {
        self.id < 0 && self.items.is_empty()
    }
}

/// An open `DualPopupSelector`: groups on the left, the selected group's
/// items on the right. Choosing either side applies the right-hand
/// selection and leaves the menu open; a press outside closes it.
#[derive(Clone, Debug, PartialEq)]
pub struct DualMenu<T> {
    pub groups: Vec<DualMenuGroup>,
    /// The selected group, if any (`showSelections` selects none when no
    /// item is marked).
    pub left_selected: Option<usize>,
    pub right_selected: usize,
    /// Where it was asked for, in pixels.
    pub position: Pos2,
    /// `getDualPopupWidth`, in pixels.
    pub width: i32,
    pub owner: T,
    pub shown: bool,
    /// The right-hand list's scroll position and bar (`PopupList`'s
    /// `view_position_` and `scroll_bar_`).
    pub right_scroll: crate::controls::scroll_bar::ScrollBar,
}

impl<T> DualMenu<T> {
    pub fn new(
        groups: Vec<DualMenuGroup>,
        left_selected: Option<usize>,
        right_selected: usize,
        position: Pos2,
        width: i32,
        owner: T,
    ) -> DualMenu<T> {
        DualMenu {
            groups,
            left_selected,
            right_selected,
            position,
            width,
            owner,
            shown: false,
            right_scroll: Default::default(),
        }
    }

    /// `DualPopupSelector::setPosition` inside the interface's `area`: tall
    /// enough for the left list.
    pub fn bounds(&self, scope: &Scope, area: Rect) -> Rect {
        let rounding = scope.value(ValueId::BodyRounding) as i32;
        let row_height = (scope.size_ratio() * ROW_HEIGHT) as i32;
        let height = row_height * self.groups.len() as i32 + 2 * rounding;
        let mut x = self.position.x as i32;
        let mut y = self.position.y as i32;
        if x + self.width > area.right() {
            x -= self.width;
        }
        if y + height > area.bottom() {
            y = area.bottom() - height;
        }
        Rect::new(x, y, self.width, height)
    }

    /// `DualPopupSelector::resized`: the two lists.
    pub fn lists(&self, scope: &Scope, bounds: Rect) -> (Rect, Rect) {
        let rounding = scope.value(ValueId::BodyRounding) as i32;
        let height = bounds.height - 2 * rounding;
        let left = Rect::new(
            bounds.x + 1,
            bounds.y + rounding,
            bounds.width / 2 - 2,
            height,
        );
        let right_x = left.right() + 1;
        let right = Rect::new(
            right_x,
            bounds.y + rounding,
            bounds.right() - right_x - 1,
            height,
        );
        (left, right)
    }

    /// Draws the lists and handles the mouse. A release on a left row
    /// selects that group and its item at the same place, if it has one of
    /// the same name, else its first (`DualPopupSelector::newSelection`);
    /// a release on a right row selects that item. Either returns the
    /// item's id as `Selected`.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        pixels: &PixelPainter,
        top: &Scope,
        area: Rect,
    ) -> MenuOutcome {
        let scope = top.within(POPUP_SECTIONS);
        let bounds = self.bounds(&scope, area);
        let (left, right) = self.lists(&scope, bounds);
        let row_height = (top.size_ratio() * ROW_HEIGHT) as i32;
        let rounding = scope.value(ValueId::BodyRounding);

        let response = ui.interact(
            pixels.rect(bounds.into()),
            ui.scope_id().with("dual popup selector"),
            Sense::click_and_drag(),
        );
        let pointer = ui.input(|input| input.pointer.interact_pos());
        let pressed = ui.input(|input| input.pointer.any_pressed());
        let to_pixels = |pos: Pos2| {
            (
                (pos.x - pixels.area().min.x) * pixels.pixels_per_point(),
                (pos.y - pixels.area().min.y) * pixels.pixels_per_point(),
            )
        };
        let right_items = self
            .left_selected
            .map(|index| self.groups[index].items.clone())
            .unwrap_or_default();
        // PopupList: the right list scrolls when its rows are taller than
        // it, with a scroll bar at its right edge.
        let scroll_bar_width = (SCROLL_BAR_WIDTH * top.size_ratio()) as i32;
        let scroll_bar = Rect::new(
            right.right() - scroll_bar_width,
            right.y,
            scroll_bar_width,
            right.height,
        );
        let total = (row_height as usize * right_items.len()) as f32;
        self.right_scroll.handle(
            ui,
            pixels,
            ui.scope_id().with("dual popup selector right"),
            right,
            scroll_bar,
            total,
            right.height as f32,
            pixels.pixels_per_point(),
        );
        let view = self.right_scroll.position;
        let row_in = |list: Rect, count: usize, offset: f32, pos: Pos2| -> Option<usize> {
            let (px, py) = to_pixels(pos);
            if px < list.x as f32
                || px > list.right() as f32
                || py < list.y as f32
                || py > list.bottom() as f32
            {
                return None;
            }
            let row = ((py - list.y as f32 + offset) / row_height as f32).floor() as i32;
            (row >= 0 && (row as usize) < count).then_some(row as usize)
        };
        // PopupList::getRowFromPosition: a divider is no row.
        let hover_left = pointer
            .filter(|_| response.hovered())
            .and_then(|p| row_in(left, self.groups.len(), 0.0, p))
            .filter(|&row| !self.groups[row].is_divider());
        let hover_right = pointer
            .filter(|_| response.hovered())
            .and_then(|p| row_in(right, right_items.len(), view, p));

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
        let divider = Rect::new(
            bounds.x + bounds.width / 2 - 1,
            bounds.y + 1,
            1,
            bounds.height - 2,
        );
        quads.push(Quad::new(
            QuadKind::Color,
            divider.into(),
            scope.color(ColorId::Border),
        ));
        quads.paint(pixels, RectF::from(bounds));

        // PopupList::renderOpenGlComponents with showSelected: the hover in
        // the lighten colour, then the selection.
        let highlight = scope.color(ColorId::WidgetPrimary1).darker(0.8);
        let lighten = scope.color(ColorId::LightenScreen);
        let row_rect = |list: Rect, row: usize, offset: f32| {
            RectF::new(
                list.x as f32,
                (list.y + row as i32 * row_height) as f32 - offset,
                list.width as f32,
                row_height as f32,
            )
        };
        for (list, hovered, selected, offset) in [
            (left, hover_left, self.left_selected, 0.0),
            (
                right,
                hover_right,
                Some(self.right_selected).filter(|&r| r < right_items.len()),
                view,
            ),
        ] {
            let mut rows = QuadBatch::additive();
            if let Some(row) = hovered {
                rows.push(Quad::new(
                    QuadKind::Color,
                    row_rect(list, row, offset),
                    lighten,
                ));
            }
            if let Some(row) = selected {
                rows.push(Quad::new(
                    QuadKind::Color,
                    row_rect(list, row, offset),
                    highlight,
                ));
            }
            rows.paint(pixels, RectF::from(list));
        }

        let padding = row_height / 4;
        let text_colour = scope.color(ColorId::TextComponentText);
        let font_size = row_height as f32 * 0.55;
        let names_left: Vec<(&str, bool)> = self
            .groups
            .iter()
            .map(|g| (g.name.as_str(), g.is_divider()))
            .collect();
        let names_right: Vec<(&str, bool)> = right_items
            .iter()
            .map(|i| (i.name.as_str(), false))
            .collect();
        for (list, names, offset) in [(left, names_left, 0), (right, names_right, view as i32)] {
            pixels.with_clip(RectF::from(list), |clipped| {
                for (index, (name, divider)) in names.iter().enumerate() {
                    let y = list.y + row_height * index as i32 - offset;
                    if *divider {
                        // PopupList::redoImage: a line across the middle.
                        let middle = (row_height as f32 * (index as f32 + 0.5)) as i32 - offset;
                        clipped.fill_rect(
                            Rect::new(
                                list.x + padding,
                                list.y + middle,
                                list.width - 2 * padding,
                                1,
                            ),
                            lighten,
                        );
                        continue;
                    }
                    let row = Rect::new(list.x + padding, y, list.width - 2 * padding, row_height);
                    let style = (Face::Light, font_size, text_colour);
                    clipped.text_line(name, row, style, Justify::Left, Overflow::Ellipsis);
                }
            });
        }
        self.right_scroll
            .paint(pixels, &scope, scroll_bar, total, right.height as f32);

        if response.drag_stopped() || response.clicked() {
            if let Some(pos) = pointer {
                if let Some(row) = row_in(left, self.groups.len(), 0.0, pos) {
                    if self.groups[row].is_divider() {
                        return MenuOutcome::Open;
                    }
                    if self.groups[row].items.is_empty() {
                        // newSelection with no sub-items: the row itself.
                        self.left_selected = Some(row);
                        self.right_scroll = Default::default();
                        return MenuOutcome::Selected(self.groups[row].id);
                    }
                    let items = &self.groups[row].items;
                    let keep = self.right_selected < items.len()
                        && self.right_selected < right_items.len()
                        && items[self.right_selected].name == right_items[self.right_selected].name;
                    if Some(row) != self.left_selected {
                        self.right_scroll = Default::default();
                    }
                    self.left_selected = Some(row);
                    if !keep {
                        self.right_selected = 0;
                    }
                    if let Some(item) = self.groups[row].items.get(self.right_selected) {
                        return MenuOutcome::Selected(item.id);
                    }
                } else if let Some(row) = row_in(right, right_items.len(), view, pos) {
                    self.right_selected = row;
                    return MenuOutcome::Selected(right_items[row].id);
                }
            }
        }
        if pressed && self.shown && !response.hovered() {
            return MenuOutcome::Closed;
        }
        self.shown = true;
        MenuOutcome::Open
    }
}
