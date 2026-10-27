// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Matrix tab: `ModulationMatrix` and its `ModulationMatrixRow`s
//! (`vital/src/interface/editor_sections/modulation_matrix.cpp`).
//!
//! The connections are rows in a scrolled list, connected ones first: each
//! with its bypass toggle (its number), source, bipolar, stereo, morph
//! (power curve), amount and destination. Below the list is the remap
//! section, whose controls come in G4c-3.

use egui::Ui;
use sas_preset::constants::MAX_MODULATION_CONNECTIONS;
use sas_preset::registry::ValueScale;

use crate::controls::button::SynthButton;
use crate::controls::modulation_button::menu_source_display_name;
use crate::controls::paths::Icon;
use crate::controls::scroll_bar::ScrollBar;
use crate::controls::slider::{Slider, SliderStyle};
use crate::fonts::Face;
use crate::host::Host;
use crate::interface::Route;
use crate::layer::Layer;
use crate::layout::Rect;
use crate::modulation::slot_control;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::section::{paint_body, paint_border, paint_tab_shadow, title_width};
use crate::sections::{Overlay, Panel};
use crate::skin::{ColorId, Section, ValueId};

/// `setSkinOverride(Skin::kModulationMatrix)`.
pub const SKIN: &[Section] = &[Section::ModulationMatrix];

/// `kSourceWidthPercent`, `kDestinationWidthPercent`, `kPaddingWidthPercent`.
const SOURCE_WIDTH_PERCENT: f32 = 0.2;
const DESTINATION_WIDTH_PERCENT: f32 = 0.2;
const PADDING_WIDTH_PERCENT: f32 = 0.04;
/// `kMatrixHeightInRows`.
const MATRIX_HEIGHT_IN_ROWS: i32 = 12;
/// `ModulationMatrix::kRowPadding`.
const ROW_PADDING: i32 = 1;
/// `ModulationMatrix::getRowHeight`: 34 at size ratio 1.
const ROW_HEIGHT: f32 = 34.0;
/// `kNoConnectionString`.
const NO_CONNECTION: &str = "-";

/// The tab's areas (`ModulationMatrix::resized`), in editor pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatrixLayout {
    pub bounds: Rect,
    pub title_width: i32,
    pub row_height: i32,
    /// The scrolled list of rows (`viewport_`).
    pub viewport: Rect,
    /// The list's heading and rows (`paintBody(g, matrix_bounds)`).
    pub matrix: Rect,
    /// The remap section below it.
    pub remap: Rect,
    pub scroll_bar: Rect,
}

impl MatrixLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> MatrixLayout {
        /// `resized`' `kScrollBarWidth`.
        const SCROLL_BAR_WIDTH: f32 = 13.0;
        let size_ratio = scope.size_ratio();
        let row_height = (size_ratio * ROW_HEIGHT) as i32;
        let title_width = title_width(scope) as i32;
        let padding = scope.value(ValueId::Padding) as i32;
        let matrix_height = (row_height + ROW_PADDING) * MATRIX_HEIGHT_IN_ROWS + ROW_PADDING;
        let viewport = Rect::new(
            bounds.x,
            bounds.y + title_width,
            bounds.width,
            matrix_height,
        );
        let matrix_bottom = title_width + matrix_height;
        let remap_y = matrix_bottom + padding;
        let scroll_bar_width = (size_ratio * SCROLL_BAR_WIDTH) as i32;
        MatrixLayout {
            bounds,
            title_width,
            row_height,
            viewport,
            matrix: Rect::new(bounds.x, bounds.y, bounds.width, matrix_bottom),
            remap: Rect::new(
                bounds.x,
                bounds.y + remap_y,
                bounds.width,
                bounds.height - remap_y,
            ),
            scroll_bar: Rect::new(
                bounds.x + bounds.width - scroll_bar_width - 1,
                bounds.y + title_width,
                scroll_bar_width,
                matrix_height,
            ),
        }
    }

    /// The rows' column height (`container_`): the shown rows with the
    /// padding around each, at least the viewport's.
    pub fn list_height(&self, shown: usize) -> i32 {
        (ROW_PADDING + shown as i32 * (self.row_height + ROW_PADDING)).max(self.viewport.height)
    }

    /// The row at `position` in the list (`setRowPositions`), scrolled up by
    /// `scroll` pixels.
    pub fn row(&self, position: usize, scroll: i32) -> Rect {
        Rect::new(
            self.viewport.x,
            self.viewport.y + ROW_PADDING + position as i32 * (self.row_height + ROW_PADDING)
                - scroll,
            self.viewport.width,
            self.row_height,
        )
    }

    /// The headings' columns (`paintBackground`): where the number, source,
    /// bipolar, stereo, morph, amount and destination headings start, in
    /// the section's coordinates.
    fn columns(&self, scope: &Scope) -> [i32; 6] {
        let width = self.bounds.width;
        let x_padding = (width as f32 * PADDING_WIDTH_PERCENT) as i32;
        let y_padding = (scope.size_ratio() * 3.0) as i32;
        let source_width = (width as f32 * SOURCE_WIDTH_PERCENT) as i32;
        let destination_width = (width as f32 * DESTINATION_WIDTH_PERCENT) as i32;
        let component_height = self.row_height - 2 * y_padding;
        let bipolar_x = source_width + self.row_height;
        let stereo_x = bipolar_x + component_height + x_padding;
        let morph_x = stereo_x + component_height + x_padding;
        let amount_x = morph_x + component_height + x_padding;
        let destination_x = width - destination_width - x_padding;
        [
            self.row_height,
            bipolar_x,
            stereo_x,
            morph_x,
            amount_x,
            destination_x,
        ]
    }
}

/// A row's controls (`ModulationMatrixRow::resized`), in editor pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowLayout {
    pub bounds: Rect,
    pub bypass: Rect,
    pub source: Rect,
    pub bipolar: Rect,
    pub stereo: Rect,
    pub power: Rect,
    pub destination: Rect,
    pub amount: Rect,
}

impl RowLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> RowLayout {
        let width = bounds.width;
        let height = bounds.height;
        let x_padding = (width as f32 * PADDING_WIDTH_PERCENT) as i32;
        let y_padding = (scope.size_ratio() * 3.0) as i32;
        let source_width = (width as f32 * SOURCE_WIDTH_PERCENT) as i32;
        let destination_width = (width as f32 * DESTINATION_WIDTH_PERCENT) as i32;
        let component_height = height - 2 * y_padding;
        let slider_height = crate::section::slider_width(scope);
        let text_component_height = (component_height as f32 * 0.7) as i32;
        let text_y = ((height - text_component_height) as f32 / 2.0) as i32;

        let bypass = Rect::new(
            y_padding,
            text_y,
            height - 2 * y_padding,
            text_component_height,
        );
        let source = Rect::new(
            bypass.x + bypass.width + y_padding,
            y_padding,
            source_width,
            component_height,
        );
        let bipolar_x = (source.x + source.width) as f32 + x_padding as f32 / 2.0;
        let bipolar = Rect::new(
            bipolar_x as i32,
            y_padding,
            component_height,
            component_height,
        );
        let stereo_x = bipolar.x + bipolar.width + x_padding;
        let expand = x_padding / 2;
        let stereo = Rect::new(
            stereo_x - expand,
            text_y,
            component_height + 2 * expand,
            text_component_height,
        );
        let power_x = stereo_x + component_height + x_padding;
        let power = Rect::new(power_x, y_padding, component_height, component_height);
        let destination_x = width - x_padding - destination_width;
        let destination = Rect::new(
            destination_x,
            y_padding,
            (destination_width as f32 + x_padding as f32 / 2.0) as i32,
            component_height,
        );
        let widget_margin = scope.value(ValueId::WidgetMargin) as i32;
        let amount_x = power.x + power.width + x_padding - widget_margin;
        let amount_width = (destination_x as f32 - amount_x as f32 - x_padding as f32 / 2.0
            + 2.0 * widget_margin as f32) as i32;
        let amount = Rect::new(
            amount_x,
            (height - slider_height + 1) / 2,
            amount_width,
            slider_height,
        );
        let at = |rect: Rect| rect.translated(bounds.x, bounds.y);
        RowLayout {
            bounds,
            bypass: at(bypass),
            source: at(source),
            bipolar: at(bipolar),
            stereo: at(stereo),
            power: at(power),
            destination: at(destination),
            amount: at(amount),
        }
    }
}

/// One slot's controls (`ModulationMatrixRow`).
struct Row {
    amount: Slider,
    power: Slider,
    bipolar: SynthButton,
    stereo: SynthButton,
    bypass: SynthButton,
}

impl Row {
    fn new(slot: usize) -> Row {
        let number: &'static str = Box::leak((slot + 1).to_string().into_boxed_str());
        Row {
            amount: Slider::new(slot_control(slot, "amount"), SliderStyle::Horizontal).bipolar(),
            power: Slider::new(slot_control(slot, "power"), SliderStyle::Curve),
            bipolar: SynthButton::icon(slot_control(slot, "bipolar"), Icon::Bipolar),
            stereo: SynthButton::just_text(slot_control(slot, "stereo"), "L/R", "L/R"),
            bypass: SynthButton::just_text(slot_control(slot, "bypass"), number, "X"),
        }
    }
}

/// The tab's state and controls.
pub struct MatrixTab {
    rows: Vec<Row>,
    /// `selected_index_`: the row whose remap shows.
    selected: usize,
    scroll: ScrollBar,
}

impl Default for MatrixTab {
    fn default() -> MatrixTab {
        MatrixTab {
            rows: (0..MAX_MODULATION_CONNECTIONS).map(Row::new).collect(),
            selected: 0,
            scroll: ScrollBar::default(),
        }
    }
}

impl MatrixTab {
    /// The rows in list order (`row_order_` after `sort` with the default
    /// column, the number): connected slots first, each group by slot.
    /// Then how many show (`checkNumModulationsShown`): the connected ones
    /// and one more.
    pub fn order(routes: &[Route]) -> (Vec<usize>, usize) {
        let connected = |slot: usize| routes.iter().any(|route| route.slot == slot);
        let mut order: Vec<usize> = (0..MAX_MODULATION_CONNECTIONS).collect();
        order.sort_by_key(|&slot| !connected(slot));
        let num_connected = order.iter().filter(|&&slot| connected(slot)).count();
        (order, (num_connected + 1).min(MAX_MODULATION_CONNECTIONS))
    }

    /// `ModulationMatrix::paintBackgroundShadow`, `paintBackground` and its
    /// rows, then the scroll bar and the borders.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        top: &Scope,
        overlay: &mut Overlay,
        bounds: Rect,
    ) {
        let scope = top.within(SKIN);
        let layout = MatrixLayout::new(&scope, bounds);
        let routes = host.model().modulations();
        let (order, shown) = MatrixTab::order(&routes);

        paint_tab_shadow(pixels, &scope, layout.matrix);
        paint_tab_shadow(pixels, &scope, layout.remap);
        paint_body(pixels, &scope, layout.matrix);
        paint_body(pixels, &scope, layout.remap);
        paint_headings(pixels, &scope, &layout);
        // The list's background under the heading.
        pixels.fill_rounded_rect_clipped(
            layout.matrix,
            scope.value(ValueId::BodyRounding),
            scope.color(ColorId::Background),
            Rect::new(
                bounds.x,
                bounds.y + layout.title_width,
                bounds.width,
                bounds.height - layout.title_width,
            ),
        );

        let total = layout.list_height(shown) as f32;
        let visible = layout.viewport.height as f32;
        let id = ui.scope_id().with("modulation matrix scroll");
        let ppp = pixels.pixels_per_point();
        self.scroll.handle(
            ui,
            pixels,
            id,
            layout.viewport,
            layout.scroll_bar,
            total,
            visible,
            ppp,
        );
        let scroll = self.scroll.position.round() as i32;

        pixels.with_clip(RectF::from(layout.viewport), |clipped| {
            let mut panel = Panel::new(ui, host, clipped, scope, overlay, layout.viewport);
            for (position, &slot) in order.iter().take(shown).enumerate() {
                let row = RowLayout::new(&scope, layout.row(position, scroll));
                let route = routes.iter().find(|route| route.slot == slot);
                self.show_row(&mut panel, &row, slot, route);
            }
            panel.finish();
        });

        self.scroll
            .paint(pixels, &scope, layout.scroll_bar, total, visible);
        paint_border(pixels, &scope, layout.matrix);
        paint_border(pixels, &scope, layout.remap);
    }

    /// `ModulationMatrixRow`: its background (`paintBackground`), the
    /// selection highlight, its controls, and the off overlay while it has
    /// no connection (`setActive(connected())`).
    fn show_row(&self, panel: &mut Panel, row: &RowLayout, slot: usize, route: Option<&Route>) {
        let scope = panel.scope;
        let pixels = panel.pixels;
        pixels.fill_rect(row.bounds, scope.color(ColorId::Body));
        let rounding = scope.value(ValueId::WidgetRoundedCorner) as i32 as f32;
        for rect in [row.source, row.destination] {
            pixels.fill_rounded_rect(
                rect,
                rounding,
                scope.color(ColorId::TextComponentBackground),
            );
        }
        // paintKnobShadows: SynthSlider::drawShadow behind the power curve.
        pixels.fill_rounded_rect(
            row.power,
            scope.value(ValueId::WidgetRoundedCorner),
            scope.color(ColorId::WidgetBackground),
        );
        let mut layer = Layer::new();
        // highlight_, an OpenGlQuad over the row.
        if slot == self.selected {
            layer.quad(Quad::new(
                QuadKind::Color,
                row.bounds.into(),
                scope.color(ColorId::LightenScreen),
            ));
        }
        let connected = route.is_some();
        let (source, destination) = match route {
            Some(route) => (
                menu_source_display_name(&route.source),
                menu_source_display_name(&route.destination),
            ),
            None => (NO_CONNECTION.to_string(), NO_CONNECTION.to_string()),
        };
        for (text, rect) in [(source, row.source), (destination, row.destination)] {
            selector_text(&mut layer, &scope, rect, &text);
        }
        layer.paint(panel.pixels, RectF::from(row.bounds));

        let controls = &self.rows[slot];
        panel.button(&controls.bypass, row.bypass);
        panel.button(&controls.bipolar, row.bipolar);
        panel.button(&controls.stereo, row.stereo);

        let host = panel.host;
        let bipolar = host.value(controls.bipolar.name) != 0.0;
        let mut power = controls.power;
        power.bipolar = bipolar;
        power.active = connected;
        panel.slider(&power, row.power);

        let mut amount = controls.amount;
        amount.active = connected;
        // updateDisplayValue: a linear or indexed destination's amount
        // reads in its units.
        if let Some(route) = route {
            let info = Host::info(&route.destination);
            if matches!(info.value_scale, ValueScale::Linear | ValueScale::Indexed) {
                amount.format.display_multiply = Some(info.max - info.min);
            }
        }
        panel.slider(&amount, row.amount);

        if !connected {
            panel.off_overlay(row.bounds);
        }
    }
}

/// `ModulationMatrix::paintBackground`'s headings: a lighten-screen line
/// before each column and its name centred above it, the sorted column's
/// (the number) in the regular face, the rest light.
fn paint_headings(pixels: &PixelPainter, scope: &Scope, layout: &MatrixLayout) {
    let (x, y) = (layout.bounds.x, layout.bounds.y);
    let title = layout.title_width;
    let columns = layout.columns(scope);
    let lighten = scope.color(ColorId::LightenScreen);
    for column in columns {
        pixels.fill_rect(Rect::new(x + column, y, 1, title), lighten);
    }
    let [number_end, bipolar_x, stereo_x, morph_x, amount_x, destination_x] = columns;
    let width = layout.bounds.width;
    let x_padding = (width as f32 * PADDING_WIDTH_PERCENT) as i32;
    let size = title as f32 * 0.4;
    let colour = scope.color(ColorId::TextComponentText);
    let text = |label: &str, left: f32, right: f32, face: Face| {
        pixels.text_centred(
            label,
            RectF::new(x as f32 + left, y as f32, right - left, title as f32),
            face,
            size,
            colour,
        );
    };
    text("#", 0.0, number_end as f32, Face::Regular);
    text("SOURCE", number_end as f32, bipolar_x as f32, Face::Light);
    text("BIPOLAR", bipolar_x as f32, stereo_x as f32, Face::Light);
    text("STEREO", stereo_x as f32, morph_x as f32, Face::Light);
    text("MORPH", morph_x as f32, amount_x as f32, Face::Light);
    text("AMOUNT", amount_x as f32, destination_x as f32, Face::Light);
    text(
        "DESTINATION",
        destination_x as f32 - 0.5 * x_padding as f32,
        width as f32,
        Face::Light,
    );
}

/// `ModulationSelector` drawn by `TextLookAndFeel::drawRotarySlider`: its
/// selection's name centred at half its height. It is an `OpenGlSlider`, not
/// a `SynthSlider`, so the skin's text size and offset and the inactive
/// colour don't apply.
fn selector_text(layer: &mut Layer, scope: &Scope, bounds: Rect, text: &str) {
    /// `drawRotarySlider`'s `kTextPercentage`.
    const TEXT_PERCENTAGE: f32 = 0.5;
    let font_size = bounds.height as f32 * TEXT_PERCENTAGE;
    let colour = scope.color(ColorId::TextComponentText);
    layer.text_centred(text, RectF::from(bounds), Face::Light, font_size, colour);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::Skin;

    fn route(slot: usize) -> Route {
        Route {
            slot,
            source: "lfo_1".into(),
            destination: "osc_1_level".into(),
        }
    }

    /// `ModulationMatrixRow::resized` at upstream's default size: the
    /// bounds ref-gui gives row 0 (laid out at the section's 704 width).
    #[test]
    fn a_row_lays_out_as_upstreams() {
        let skin = Skin::default();
        let scope = Scope::top(&skin, 1.0).within(SKIN);
        let row = RowLayout::new(&scope, Rect::new(88, 81, 704, 34));
        assert_eq!(row.bypass, Rect::new(91, 88, 28, 19));
        assert_eq!(row.source, Rect::new(122, 84, 140, 28));
        assert_eq!(row.bipolar, Rect::new(276, 84, 28, 28));
        assert_eq!(row.stereo, Rect::new(318, 88, 56, 19));
        assert_eq!(row.power, Rect::new(388, 84, 28, 28));
        assert_eq!(row.amount, Rect::new(439, 86, 181, 24));
        assert_eq!(row.destination, Rect::new(624, 84, 154, 28));
    }

    /// `checkNumModulationsShown` and `sort`'s last `compareConnected`:
    /// connected slots first, then the rest, each by slot; one row more
    /// than connected shows.
    #[test]
    fn connected_rows_come_first_with_one_empty_row_after() {
        let (order, shown) = MatrixTab::order(&[]);
        assert_eq!(&order[..3], [0, 1, 2]);
        assert_eq!(shown, 1);
        let (order, shown) = MatrixTab::order(&[route(5), route(2)]);
        assert_eq!(&order[..4], [2, 5, 0, 1]);
        assert_eq!(shown, 3);
    }
}
