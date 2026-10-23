// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The whole editor: upstream's `FullInterface`.
//!
//! Ports the shell of `vital/src/interface/editor_sections/full_interface.cpp`
//! and `header_section.cpp`: the background, the header (logo area, tab
//! selector, preset selector) and the plain-bodied sections around the
//! edges. The sections' contents (oscillators, filters, modulators, effects,
//! matrix, keyboard) arrive in later milestones; until then their areas show
//! the background, as upstream's do before their children paint.
//!
//! Upstream's logo is replaced by the product name (migration/AGENTS.md
//! rule 2).

use egui::{Sense, Ui};

use crate::colour::Colour;
use crate::fonts::Face;
use crate::layout::{FullLayout, Rect};
use crate::paint::{PixelPainter, RectF};
use crate::scope::Scope;
use crate::skin::{ColorId, Section, Skin, ValueId};

/// What the editor shows of the synth's state.
pub trait EditorModel: Send + Sync + 'static {
    /// `SynthBase::getPresetName`: the loaded preset's name, empty for the
    /// init preset.
    fn preset_name(&self) -> String;
}

/// The product name, drawn where upstream draws its logo.
pub const PRODUCT_NAME: &str = "synth-a-size";

/// The header's tabs (`HeaderSection`'s `tab_selector_` names).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Voice,
    Effects,
    Matrix,
    Advanced,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Voice, Tab::Effects, Tab::Matrix, Tab::Advanced];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Voice => "VOICE",
            Tab::Effects => "EFFECTS",
            Tab::Matrix => "MATRIX",
            Tab::Advanced => "ADVANCED",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The editor's own state: the skin and what is selected.
pub struct Interface {
    skin: Skin,
    tab: Tab,
}

impl Default for Interface {
    fn default() -> Interface {
        Interface::new(Skin::default())
    }
}

impl Interface {
    pub fn new(skin: Skin) -> Interface {
        Interface {
            skin,
            tab: Tab::Voice,
        }
    }

    pub fn skin(&self) -> &Skin {
        &self.skin
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    /// Draws the editor into `ui`'s whole area and handles its input.
    pub fn show(&mut self, ui: &mut Ui, model: &dyn EditorModel) {
        let area = ui.max_rect();
        self.show_in(ui, area, model);
    }

    /// Draws the editor into `area` (points) and handles its input.
    pub fn show_in(&mut self, ui: &mut Ui, area: egui::Rect, model: &dyn EditorModel) {
        let pixels_per_point = ui.ctx().pixels_per_point();
        let width = (area.width() * pixels_per_point).round() as i32;
        let height = (area.height() * pixels_per_point).round() as i32;
        let layout = FullLayout::new(&self.skin, width, height);
        let painter = ui.painter_at(area);
        let pixels = PixelPainter::new(&painter, area.min, pixels_per_point);

        self.paint_background(&pixels, &layout, width, height);
        self.paint_header(&pixels, &layout, model);
        self.handle_tabs(ui, &pixels, &layout);
    }

    fn top(&self, layout: &FullLayout) -> Scope<'_> {
        Scope::top(&self.skin, layout.size_ratio)
    }

    /// `FullInterface::paintBackground`, for the sections ported so far.
    fn paint_background(
        &self,
        pixels: &PixelPainter,
        layout: &FullLayout,
        width: i32,
        height: i32,
    ) {
        let top = self.top(layout);
        pixels.fill_rect(
            Rect::new(0, 0, width, height),
            top.color(ColorId::Background),
        );

        let header_body = top.within(&[Section::Header]).color(ColorId::Body);
        for bar in layout.bars {
            pixels.fill_rect(bar, header_body);
        }

        // VoiceSection, PortamentoSection and BendSection paint their body
        // and border; KeyboardInterface its body. All four use the Keyboard
        // skin section.
        let keyboard = top.within(&[Section::Keyboard]);
        for bounds in [layout.voice, layout.portamento, layout.bend] {
            paint_body(pixels, &keyboard, bounds);
            paint_border(pixels, &keyboard, bounds);
        }
        paint_body(pixels, &keyboard, layout.keyboard);
    }

    /// `HeaderSection::paintBackground` and its children's paint: the logo
    /// area, the tab selector and the preset selector.
    fn paint_header(&self, pixels: &PixelPainter, layout: &FullLayout, model: &dyn EditorModel) {
        let header = self.top(layout).within(&[Section::Header]);
        let bounds = layout.header.bounds;
        let logo_section_width = (header.standard_knob_size() + header.padding()) as i32;
        pixels.fill_rect(
            Rect::new(bounds.x, bounds.y, logo_section_width, bounds.height),
            header.color(ColorId::Body),
        );

        self.paint_logo(pixels, layout);
        self.paint_tabs(pixels, layout);
        self.paint_preset_selector(pixels, layout, model);
    }

    /// The product name in the logo area, sized to fit it.
    fn paint_logo(&self, pixels: &PixelPainter, layout: &FullLayout) {
        let logo = self.top(layout).within(&[Section::Logo, Section::Header]);
        let bounds = layout.header.logo;
        // The name spans the logo button's width (`LogoSection::resized`:
        // a square as tall as the header less 2 * kLogoPaddingY).
        let point_height = bounds.width as f32 * 0.19;
        pixels.text_centred(
            PRODUCT_NAME,
            bounds,
            Face::TitleRegular,
            point_height,
            logo.color(ColorId::WidgetPrimary1),
        );
    }

    /// `TabSelector::paint`.
    fn paint_tabs(&self, pixels: &PixelPainter, layout: &FullLayout) {
        const LIGHT_HEIGHT_PERCENT: f32 = 0.08;
        const FONT_HEIGHT_PERCENT: f32 = 0.26;
        let header = self.top(layout).within(&[Section::Header]);
        let bounds = layout.header.tabs;
        let selected = self.tab.index();
        let light_height = ((bounds.height as f32 * LIGHT_HEIGHT_PERCENT) as i32).max(1);
        let highlight = header.color(ColorId::WidgetPrimary1);

        pixels.fill_rect(
            Rect::new(bounds.x, bounds.y, bounds.width, light_height),
            header.color(ColorId::LightenScreen),
        );
        let from = tab_x(bounds.width, selected);
        let to = tab_x(bounds.width, selected + 1);
        pixels.fill_rect(
            Rect::new(bounds.x + from, bounds.y, to - from, light_height),
            highlight,
        );

        let point_height = bounds.height as f32 * FONT_HEIGHT_PERCENT;
        for tab in Tab::ALL {
            let from = tab_x(bounds.width, tab.index());
            let to = tab_x(bounds.width, tab.index() + 1);
            let colour = if tab == self.tab {
                highlight
            } else {
                header.color(ColorId::TextComponentText)
            };
            let cell = Rect::new(bounds.x + from, bounds.y, to - from, bounds.height);
            pixels.text_centred(tab.label(), cell, Face::Light, point_height, colour);
        }
    }

    /// `PresetSelector::paintBackground` and its text and arrow buttons.
    fn paint_preset_selector(
        &self,
        pixels: &PixelPainter,
        layout: &FullLayout,
        model: &dyn EditorModel,
    ) {
        let header = self.top(layout).within(&[Section::Header]);
        let preset = layout.header.preset;
        pixels.fill_rounded_rect(
            preset.bounds,
            header.value(ValueId::WidgetRoundedCorner),
            header.color(ColorId::PopupSelectorBackground),
        );

        let mut name = model.preset_name();
        if name.is_empty() {
            name = "Init Preset".to_string();
        }
        pixels.text_centred(
            &name,
            preset.text,
            Face::Title,
            preset.text_size,
            header.color(ColorId::PresetText),
        );

        let arrow = header.color(ColorId::IconButtonOff);
        paint_arrow(
            pixels,
            preset.previous,
            &[(0.65, 0.3), (0.35, 0.5), (0.65, 0.7)],
            arrow,
        );
        paint_arrow(
            pixels,
            preset.next,
            &[(0.35, 0.3), (0.65, 0.5), (0.35, 0.7)],
            arrow,
        );
    }

    /// `TabSelector::mouseEvent`: a click or drag selects the tab under it.
    fn handle_tabs(&mut self, ui: &mut Ui, pixels: &PixelPainter, layout: &FullLayout) {
        let bounds = layout.header.tabs;
        let response = ui.interact(
            pixels.rect(bounds.into()),
            ui.scope_id().with("tabs"),
            Sense::click_and_drag(),
        );
        if !(response.clicked() || response.dragged()) {
            return;
        }
        if let Some(pointer) = response.interact_pointer_pos() {
            let x = (pointer.x - pixels.rect(bounds.into()).min.x) * pixels.pixels_per_point();
            let index = (x * Tab::ALL.len() as f32 / bounds.width as f32) as i32;
            self.tab = Tab::ALL[index.clamp(0, Tab::ALL.len() as i32 - 1) as usize];
        }
    }
}

/// `TabSelector::getTabX`.
fn tab_x(width: i32, position: usize) -> i32 {
    let num_types = Tab::ALL.len() as f32;
    ((width + 1) as f32 * position as f32 / num_types).round() as i32
}

/// `SynthSection::paintBody`.
fn paint_body(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    pixels.fill_rounded_rect(
        bounds,
        scope.value(ValueId::BodyRounding),
        scope.color(ColorId::Body),
    );
}

/// `SynthSection::paintBorder`.
fn paint_border(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    let rounding = scope.value(ValueId::BodyRounding) as i32 as f32;
    pixels.draw_rounded_rect(
        RectF::from(bounds).reduced(0.5),
        rounding,
        1.0,
        scope.color(ColorId::Border),
    );
}

/// An `OpenGlShapeButton` arrow: `PresetSelector`'s chevron path (unit
/// coordinates), stroked 0.05 of the button's size with round ends.
fn paint_arrow(pixels: &PixelPainter, bounds: Rect, path: &[(f32, f32)], colour: Colour) {
    let size = bounds.width.min(bounds.height) as f32;
    let points: Vec<(f32, f32)> = path
        .iter()
        .map(|&(x, y)| (bounds.x as f32 + x * size, bounds.y as f32 + y * size))
        .collect();
    pixels.polyline(&points, 0.05 * size, colour);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_positions_match_upstream() {
        assert_eq!(
            (0..=4).map(|i| tab_x(395, i)).collect::<Vec<_>>(),
            vec![0, 99, 198, 297, 396]
        );
    }
}
