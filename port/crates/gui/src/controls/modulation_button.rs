// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `ModulationButton` and `ModulationTabSelector`: the modulation sources'
//! buttons, stacked as tabs beside the envelopes, LFOs and randoms, or in a
//! row for the keyboard sources.
//!
//! Ports `ModulationButton::paintBackground` / `getMeterBounds` and the
//! click that selects a tab
//! (`vital/src/interface/editor_components/modulation_button.cpp`), and
//! `ModulationTabSelector`'s layout and painting (`modulation_tab_selector.cpp`).
//! Dragging a source onto a control, its connections menu and its meter come
//! with drag-to-modulate.

use egui::{Id, PointerButton, Sense, Ui};

use crate::fonts::Face;
use crate::layout::Rect;
use crate::paint::{Gradient, GradientKind, PixelPainter, RectF};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `ModulationButton::kFontAreaHeightRatio`.
const FONT_AREA_HEIGHT_RATIO: f32 = 0.3;
/// `ModulationButton::kMeterAreaRatio`.
const METER_AREA_RATIO: f32 = 0.05;

/// `ModulationMatrix::getUiSourceDisplayName`: `env_1` is "ENV 1",
/// `note_in_octave` "NOTE IN OCTAVE", `aftertouch` "AFTER TOUCH".
pub fn source_display_name(name: &str) -> String {
    if name == "aftertouch" {
        return "AFTER TOUCH".to_string();
    }
    let name = name.replacen("control_", "", 1);
    name.split('_')
        .filter(|token| !token.is_empty())
        .map(str::to_uppercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `ModulationButton::getMeterBounds`, in the button's coordinates.
fn meter_bounds(width: i32, height: i32) -> Rect {
    const MIN_METER_WIDTH: i32 = 4;
    let meter_width =
        MIN_METER_WIDTH.max(((width as f32 * METER_AREA_RATIO / 2.0).round() * 2.0) as i32);
    Rect::new(1, 1, meter_width, height - 2)
}

/// One source's button.
#[derive(Clone, Debug)]
pub struct ModulationButton {
    /// The source's name (`env_1`, `velocity`, ...).
    pub name: &'static str,
    /// `overrideText`.
    pub text_override: Option<&'static str>,
}

/// How a button is drawn (`ModulationTabSelector`'s settings).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonStyle {
    pub selected: bool,
    /// `connectRight`: joined to the section on its right.
    pub connect_right: bool,
    /// `drawBorders`.
    pub draw_border: bool,
    /// `setFontSize` (the skin's modulation font size).
    pub font_size: f32,
}

impl ModulationButton {
    pub fn new(name: &'static str) -> ModulationButton {
        ModulationButton {
            name,
            text_override: None,
        }
    }

    pub fn text(&self) -> String {
        self.text_override
            .map(str::to_string)
            .unwrap_or_else(|| source_display_name(self.name))
    }

    /// `ModulationButton::paintBackground` at `bounds`, clipped to them as
    /// `ModulationTabSelector::paintBackground` does.
    pub fn paint_background(
        &self,
        pixels: &PixelPainter,
        scope: &Scope,
        bounds: Rect,
        style: ButtonStyle,
    ) {
        if bounds.width == 0 || bounds.height == 0 {
            return;
        }
        pixels.with_clip(bounds, |pixels| {
            self.paint_clipped(pixels, scope, bounds, style)
        });
    }

    fn paint_clipped(
        &self,
        pixels: &PixelPainter,
        scope: &Scope,
        bounds: Rect,
        style: ButtonStyle,
    ) {
        const SHADOW_AREA: f32 = 0.04;
        let colour = if style.selected {
            scope.color(ColorId::ModulationButtonSelected)
        } else {
            scope.color(ColorId::ModulationButtonUnselected)
        };
        let rounding = scope.value(ValueId::BodyRounding) as i32 as f32;
        let (x, y) = (bounds.x as f32, bounds.y as f32);
        let width = bounds.width;
        let adjusted_width = if style.connect_right {
            width * 2
        } else {
            width
        };
        let area = RectF::new(x, y, adjusted_width as f32, bounds.height as f32);
        pixels.fill_rounded_rect(area, rounding, colour);

        let meter = meter_bounds(bounds.width, bounds.height).translated(bounds.x, bounds.y);
        let meter_width = meter.width as f32;
        let widget_background = scope.color(ColorId::WidgetBackground);
        pixels.fill_rounded_rect(meter, meter_width / 2.0, widget_background);
        pixels.fill_rect(
            RectF::new(
                meter.x as f32 + meter_width / 2.0,
                meter.y as f32,
                meter_width / 2.0,
                meter.height as f32,
            ),
            widget_background,
        );
        if style.draw_border {
            pixels.draw_rounded_rect(
                area.reduced(0.5),
                rounding,
                1.0,
                scope.color(ColorId::Border),
            );
        }

        let font_area_height = (FONT_AREA_HEIGHT_RATIO * width as f32) as i32;
        pixels.text_centred(
            &self.text(),
            Rect::new(
                meter.right(),
                bounds.y,
                bounds.right() - meter.right(),
                font_area_height,
            ),
            Face::Regular,
            style.font_size,
            scope.color(ColorId::BodyText),
        );

        if style.connect_right && !style.selected {
            let shadow_width = (width as f32 * SHADOW_AREA) as i32;
            let shadow = scope.color(ColorId::Shadow);
            let right = bounds.right() as f32;
            let gradient = Gradient::new(
                GradientKind::Linear,
                (right, 0.0),
                shadow,
                (right - shadow_width as f32, 0.0),
                shadow.with_alpha(0),
            );
            pixels.fill_rect_gradient(
                Rect::new(
                    bounds.right() - shadow_width,
                    bounds.y,
                    shadow_width,
                    bounds.height,
                ),
                &gradient,
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ButtonState {
    down: bool,
}

/// A row or column of source buttons (`ModulationTabSelector`).
#[derive(Clone, Debug)]
pub struct ModulationTabSelector {
    pub buttons: Vec<ModulationButton>,
    pub vertical: bool,
    /// `enableSelections`: a click selects the tab.
    pub selections: bool,
    /// `setMinModulationsShown`; 0 shows them all.
    pub min_shown: usize,
    pub connect_right: bool,
    pub draw_borders: bool,
}

impl ModulationTabSelector {
    /// `ModulationTabSelector(prefix, number)`: `prefix_1` to `prefix_N`.
    pub fn numbered(names: &[&'static str]) -> ModulationTabSelector {
        ModulationTabSelector {
            buttons: names
                .iter()
                .map(|name| ModulationButton::new(name))
                .collect(),
            vertical: true,
            selections: false,
            min_shown: 0,
            connect_right: false,
            draw_borders: false,
        }
    }

    /// `getNumModulationsToShow`. `connected` says whether a source has a
    /// modulation; with a minimum, the tabs show up to one past the last
    /// connected one.
    pub fn num_shown(&self, connected: impl Fn(&str) -> bool) -> usize {
        let count = self.buttons.len();
        let mut num = count;
        if self.min_shown > 0 {
            num = self.min_shown;
            for i in self.min_shown - 1..count {
                if connected(self.buttons[i].name) {
                    num = i + 2;
                }
            }
        }
        num.min(count)
    }

    /// `checkNumShown`: the first `num_shown` buttons' bounds in `bounds`.
    pub fn button_bounds(&self, bounds: Rect, num_shown: usize) -> Vec<Rect> {
        let mut rects = Vec::with_capacity(num_shown);
        if self.vertical {
            let cell_height = (bounds.height + 1) as f32 / num_shown as f32;
            let mut y = 0;
            for i in 0..num_shown {
                let last_y = y;
                y = ((i + 1) as f32 * cell_height).round() as i32;
                rects.push(Rect::new(
                    bounds.x,
                    bounds.y + last_y,
                    bounds.width,
                    y - last_y - 1,
                ));
            }
        } else {
            let cell_width = (bounds.width + 1) as f32 / num_shown as f32;
            let mut x = 0;
            for i in 0..num_shown {
                let last_x = x;
                x = ((i + 1) as f32 * cell_width).round() as i32;
                rects.push(Rect::new(
                    bounds.x + last_x,
                    bounds.y,
                    x - last_x - 1,
                    bounds.height,
                ));
            }
        }
        rects
    }

    /// `ModulationTabSelector::paintBackground`: the background, the
    /// selector's shadow, and each shown button.
    pub fn paint_background(
        &self,
        pixels: &PixelPainter,
        scope: &Scope,
        bounds: Rect,
        num_shown: usize,
        selected: Option<usize>,
    ) {
        pixels.fill_rect(bounds, scope.color(ColorId::Background));
        // ModulationTabSelector::paintTabShadow.
        pixels.fill_rounded_rect(
            bounds,
            scope.value(ValueId::BodyRounding) as i32 as f32,
            scope.color(ColorId::Shadow),
        );
        let font_size = scope.value(ValueId::ModulationFontSize);
        for (i, rect) in self
            .button_bounds(bounds, num_shown)
            .into_iter()
            .enumerate()
        {
            let style = ButtonStyle {
                selected: self.selections && selected == Some(i),
                connect_right: self.connect_right,
                draw_border: self.draw_borders,
                font_size,
            };
            self.buttons[i].paint_background(pixels, scope, rect, style);
        }
    }

    /// The buttons' mouse handling: a press and release on a button clicks
    /// it (`ModulationButton::mouseUp`'s `modulationClicked`). Returns the
    /// clicked button.
    pub fn interact(
        &self,
        ui: &mut Ui,
        pixels: &PixelPainter,
        id: Id,
        bounds: Rect,
        num_shown: usize,
    ) -> Option<usize> {
        let mut clicked = None;
        for (i, rect) in self
            .button_bounds(bounds, num_shown)
            .into_iter()
            .enumerate()
        {
            let button_id = id.with(i);
            let response =
                ui.interact(pixels.rect(rect.into()), button_id, Sense::click_and_drag());
            let mut state: ButtonState =
                ui.data(|data| data.get_temp(button_id)).unwrap_or_default();
            let primary_down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
            let down = (state.down || response.is_pointer_button_down_on()) && primary_down;
            if state.down && !down {
                let pointer = ui.input(|input| input.pointer.interact_pos());
                if pointer.is_some_and(|pos| response.rect.contains(pos)) {
                    clicked = Some(i);
                }
            }
            state.down = down;
            ui.data_mut(|data| data.insert_temp(button_id, state));
        }
        clicked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_names_are_shown_in_capitals() {
        assert_eq!(source_display_name("env_1"), "ENV 1");
        assert_eq!(source_display_name("random_2"), "RANDOM 2");
        assert_eq!(source_display_name("note_in_octave"), "NOTE IN OCTAVE");
        assert_eq!(source_display_name("aftertouch"), "AFTER TOUCH");
    }

    #[test]
    fn tabs_split_the_selector_with_a_one_pixel_gap() {
        let selector = ModulationTabSelector::numbered(&["env_1", "env_2", "env_3"]);
        let rects = selector.button_bounds(Rect::new(802, 48, 68, 213), 3);
        assert_eq!(
            rects,
            vec![
                Rect::new(802, 48, 68, 70),
                Rect::new(802, 119, 68, 71),
                Rect::new(802, 191, 68, 70)
            ]
        );
    }

    #[test]
    fn one_tab_past_the_last_connected_source_is_shown() {
        let mut selector = ModulationTabSelector::numbered(&[
            "lfo_1", "lfo_2", "lfo_3", "lfo_4", "lfo_5", "lfo_6",
        ]);
        selector.min_shown = 4;
        assert_eq!(selector.num_shown(|_| false), 4);
        assert_eq!(selector.num_shown(|name| name == "lfo_4"), 5);
        assert_eq!(selector.num_shown(|name| name == "lfo_6"), 6);
    }
}
