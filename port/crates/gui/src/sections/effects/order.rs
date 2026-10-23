// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effect order: one tile per effect, dragged to reorder and switched on
//! with its power button. Ports `DragDropEffectOrder` and `DraggableEffect`
//! (`vital/src/interface/editor_components/drag_drop_effect_order.cpp`).

use super::{title_width, NUM_EFFECTS};
use crate::layout::Rect;
use crate::scope::Scope;

/// `DragDropEffectOrder::kEffectPadding`.
const EFFECT_PADDING: f32 = 6.0;

/// `DragDropEffectOrder::resized` and `DraggableEffect::resized`, in
/// interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderLayout {
    pub bounds: Rect,
    /// Each effect's tile, by effect, at its place in the order.
    pub tiles: [Rect; NUM_EFFECTS],
    /// Each tile's power button (`<effect>_on`), by effect.
    pub power_buttons: [Rect; NUM_EFFECTS],
}

impl OrderLayout {
    /// `order` lists the effects in processing order.
    pub fn new(scope: &Scope, bounds: Rect, order: [usize; NUM_EFFECTS]) -> OrderLayout {
        let padding = scope.size_ratio() * EFFECT_PADDING;
        let mut tiles = [Rect::default(); NUM_EFFECTS];
        let mut power_buttons = [Rect::default(); NUM_EFFECTS];
        let title = title_width(scope);
        for (index, &effect) in order.iter().enumerate() {
            let from_y = effect_y(scope, bounds.height, index);
            let to_y = effect_y(scope, bounds.height, index + 1);
            let height = ((to_y - from_y) as f32 - padding) as i32;
            tiles[effect] = Rect::new(bounds.x, bounds.y + from_y, bounds.width, height);
            power_buttons[effect] = Rect::new(bounds.x, bounds.y + from_y, title, title);
        }
        OrderLayout {
            bounds,
            tiles,
            power_buttons,
        }
    }
}

/// `DragDropEffectOrder::getEffectY`: where the tile at place `index`
/// starts, relative to the order's top.
pub fn effect_y(scope: &Scope, height: i32, index: usize) -> i32 {
    let padding = (scope.size_ratio() * EFFECT_PADDING) as i32;
    ((1.0 * index as f32 * (height + padding) as f32) / NUM_EFFECTS as f32).round() as i32
}

/// `DragDropEffectOrder::getEffectIndexFromY`: the place under `y`
/// (relative to the order's top).
pub fn effect_index_from_y(scope: &Scope, height: i32, y: f32) -> usize {
    let padding = scope.size_ratio() * EFFECT_PADDING;
    let index = (NUM_EFFECTS as f32 * (y + padding / 2.0) / (height as f32 + padding)) as i32;
    index.clamp(0, NUM_EFFECTS as i32 - 1) as usize
}
