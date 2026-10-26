// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `TabSelector`: a row of named tabs with a light over the selected one.
//! The header's tabs and the EQ and reverb band selectors.
//!
//! Ports `vital/src/interface/editor_components/tab_selector.cpp`.

use egui::{Id, Sense, Ui};

use crate::fonts::Face;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::skin::ColorId;
use crate::text::{Justify, Overflow};

/// `TabSelector::kDefaultFontHeightPercent`.
pub const DEFAULT_FONT_HEIGHT_PERCENT: f32 = 0.26;
/// `TabSelector::paint`'s `kLightHeightPercent`.
const LIGHT_HEIGHT_PERCENT: f32 = 0.08;

#[derive(Clone, Copy, Debug)]
pub struct TabSelector {
    /// `names_`: one per tab.
    pub names: &'static [&'static str],
    /// `font_height_percent_`.
    pub font_height_percent: f32,
    /// `active_`: the highlight is grey when false.
    pub active: bool,
}

impl TabSelector {
    pub fn new(names: &'static [&'static str]) -> TabSelector {
        TabSelector {
            names,
            font_height_percent: DEFAULT_FONT_HEIGHT_PERCENT,
            active: true,
        }
    }

    /// `TabSelector::getTabX`: where tab `position` starts.
    pub fn tab_x(&self, width: i32, position: usize) -> i32 {
        tab_x(width, self.names.len(), position)
    }

    /// `mouseDown` / `mouseDrag` / `mouseUp`: the tab under the pointer is
    /// selected while the mouse is down on the selector. Returns whether
    /// the selection changed.
    pub fn interact(
        &self,
        ui: &mut Ui,
        pixels: &PixelPainter,
        id: Id,
        bounds: Rect,
        selected: &mut usize,
    ) -> bool {
        let rect = pixels.rect(bounds.into());
        let response = ui.interact(rect, id, Sense::click_and_drag());
        if !response.is_pointer_button_down_on() {
            return false;
        }
        let Some(pointer) = response.interact_pointer_pos() else {
            return false;
        };
        let x = (pointer.x - rect.min.x) * pixels.pixels_per_point();
        let index = (x * self.names.len() as f32 / bounds.width as f32) as i32;
        // Slider::setValue clamps to the range.
        let index = index.clamp(0, self.names.len() as i32 - 1) as usize;
        let changed = index != *selected;
        *selected = index;
        changed
    }

    /// `TabSelector::paint`.
    pub fn paint(&self, pixels: &PixelPainter, scope: &Scope, bounds: Rect, selected: usize) {
        let light_height = ((bounds.height as f32 * LIGHT_HEIGHT_PERCENT) as i32).max(1);
        let mut highlight = scope.color(ColorId::WidgetPrimary1);
        if !self.active {
            highlight = highlight.desaturated();
        }

        pixels.fill_rect(
            Rect::new(bounds.x, bounds.y, bounds.width, light_height),
            scope.color(ColorId::LightenScreen),
        );
        let from = self.tab_x(bounds.width, selected);
        let to = self.tab_x(bounds.width, selected + 1);
        pixels.fill_rect(
            Rect::new(bounds.x + from, bounds.y, to - from, light_height),
            highlight,
        );

        let point_height = bounds.height as f32 * self.font_height_percent;
        for (i, name) in self.names.iter().enumerate() {
            let from = self.tab_x(bounds.width, i);
            let to = self.tab_x(bounds.width, i + 1);
            let colour = if i == selected {
                highlight
            } else {
                scope.color(ColorId::TextComponentText)
            };
            let cell = Rect::new(bounds.x + from, bounds.y, to - from, bounds.height);
            let style = (Face::Light, point_height, colour);
            pixels.text_line(name, cell, style, Justify::Centred, Overflow::Ellipsis);
        }
    }
}

/// `TabSelector::getTabX` for `num_types` tabs across `width`.
pub fn tab_x(width: i32, num_types: usize, position: usize) -> i32 {
    ((width + 1) as f32 * position as f32 / num_types as f32).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_positions_match_upstream() {
        assert_eq!(
            (0..=4).map(|i| tab_x(395, 4, i)).collect::<Vec<_>>(),
            vec![0, 99, 198, 297, 396]
        );
    }
}
