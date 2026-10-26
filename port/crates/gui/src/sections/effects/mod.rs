// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Effects tab: the effect order on the left and the enabled effects
//! stacked in a scrolling column. Ports `EffectsInterface`
//! (`vital/src/interface/editor_sections/effects_interface.cpp`).

pub mod chorus;
pub mod delay;
pub mod distortion;
pub mod flanger;
pub mod order;
pub mod phaser;
pub mod viewers;

use crate::layout::Rect;
use crate::scope::Scope;
use crate::skin::{Section, ValueId};

/// `EffectsInterface`'s skin override (`setSkinOverride(Skin::kAllEffects)`).
pub const SKIN: &[Section] = &[Section::AllEffects];

/// `vital::constants::kNumEffects`.
pub const NUM_EFFECTS: usize = 9;

/// The effects in their default order (`strings::kEffectOrder`, the
/// `effect_chain_order` indices).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Chorus,
    Compressor,
    Delay,
    Distortion,
    Eq,
    FilterFx,
    Flanger,
    Phaser,
    Reverb,
}

impl Effect {
    pub const ALL: [Effect; NUM_EFFECTS] = [
        Effect::Chorus,
        Effect::Compressor,
        Effect::Delay,
        Effect::Distortion,
        Effect::Eq,
        Effect::FilterFx,
        Effect::Flanger,
        Effect::Phaser,
        Effect::Reverb,
    ];

    /// `strings::kEffectOrder`: the prefix of the effect's controls.
    pub fn name(self) -> &'static str {
        match self {
            Effect::Chorus => "chorus",
            Effect::Compressor => "compressor",
            Effect::Delay => "delay",
            Effect::Distortion => "distortion",
            Effect::Eq => "eq",
            Effect::FilterFx => "filter_fx",
            Effect::Flanger => "flanger",
            Effect::Phaser => "phaser",
            Effect::Reverb => "reverb",
        }
    }

    /// The `<name>_on` control that enables it.
    pub fn enable_control(self) -> &'static str {
        match self {
            Effect::Chorus => "chorus_on",
            Effect::Compressor => "compressor_on",
            Effect::Delay => "delay_on",
            Effect::Distortion => "distortion_on",
            Effect::Eq => "eq_on",
            Effect::FilterFx => "filter_fx_on",
            Effect::Flanger => "flanger_on",
            Effect::Phaser => "phaser_on",
            Effect::Reverb => "reverb_on",
        }
    }

    /// The effect's own skin section (`Skin::kAllEffects + 1 + index`).
    pub fn section(self) -> Section {
        match self {
            Effect::Chorus => Section::Chorus,
            Effect::Compressor => Section::Compressor,
            Effect::Delay => Section::Delay,
            Effect::Distortion => Section::Distortion,
            Effect::Eq => Section::Equalizer,
            Effect::FilterFx => Section::FxFilter,
            Effect::Flanger => Section::Flanger,
            Effect::Phaser => Section::Phaser,
            Effect::Reverb => Section::Reverb,
        }
    }

    /// Its sections chain, innermost first: the effect, then the tab.
    pub fn skin(self) -> &'static [Section] {
        match self {
            Effect::Chorus => &[Section::Chorus, Section::AllEffects],
            Effect::Compressor => &[Section::Compressor, Section::AllEffects],
            Effect::Delay => &[Section::Delay, Section::AllEffects],
            Effect::Distortion => &[Section::Distortion, Section::AllEffects],
            Effect::Eq => &[Section::Equalizer, Section::AllEffects],
            Effect::FilterFx => &[Section::FxFilter, Section::AllEffects],
            Effect::Flanger => &[Section::Flanger, Section::AllEffects],
            Effect::Phaser => &[Section::Phaser, Section::AllEffects],
            Effect::Reverb => &[Section::Reverb, Section::AllEffects],
        }
    }

    /// The section's name (`SynthSection` name, the sideways heading).
    pub fn title(self) -> &'static str {
        match self {
            Effect::Chorus => "CHORUS",
            Effect::Compressor => "COMPRESSOR",
            Effect::Delay => "DELAY",
            Effect::Distortion => "DISTORTION",
            Effect::Eq => "EQ",
            Effect::FilterFx => "FILTER",
            Effect::Flanger => "FLANGER",
            Effect::Phaser => "PHASER",
            Effect::Reverb => "REVERB",
        }
    }
}

/// `EffectsInterface::resized` and `setEffectPositions`, in interface
/// pixels with the column scrolled to the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectsLayout {
    pub bounds: Rect,
    /// The effect order (`effect_chain_order`).
    pub order: Rect,
    /// The scrolling column's visible area (`viewport_`).
    pub viewport: Rect,
    /// The column of effects (`container_`), as tall as its contents.
    pub container: Rect,
    /// Each enabled effect's section, by effect.
    pub effects: [Option<Rect>; NUM_EFFECTS],
    pub scroll_bar: Rect,
}

impl EffectsLayout {
    /// `order` lists the effects in processing order (`effect_chain_order`
    /// decoded); `enabled` is indexed by effect.
    pub fn new(
        scope: &Scope,
        bounds: Rect,
        order: [usize; NUM_EFFECTS],
        enabled: [bool; NUM_EFFECTS],
    ) -> EffectsLayout {
        /// `EffectsInterface::resized`'s `kEffectOrderWidthPercent`.
        const EFFECT_ORDER_WIDTH_PERCENT: f32 = 0.2;
        let width = bounds.width;
        let order_width = (width as f32 * EFFECT_ORDER_WIDTH_PERCENT) as i32;
        let large_padding = scope.value(ValueId::LargePadding) as i32;
        let shadow_width = component_shadow_width(scope);
        let viewport_x = order_width + large_padding - shadow_width;
        let viewport_width = width - viewport_x - large_padding + 2 * shadow_width;

        // setEffectPositions.
        let padding = scope.padding() as i32;
        let start_x = order_width + large_padding;
        let effect_width = width - start_x - large_padding;
        let knob_section_height = scope.knob_section_height() as i32;
        let widget_margin = scope.widget_margin() as i32;
        let effect_height = 2 * knob_section_height - widget_margin;
        let mut effects = [None; NUM_EFFECTS];
        let mut y = 0;
        for &effect in &order {
            if enabled[effect] {
                effects[effect] = Some(
                    Rect::new(shadow_width, y, effect_width, effect_height)
                        .translated(bounds.x + viewport_x, bounds.y),
                );
                y += effect_height + padding;
            }
        }

        EffectsLayout {
            bounds,
            order: Rect::new(bounds.x, bounds.y, order_width, bounds.height),
            viewport: Rect::new(
                bounds.x + viewport_x,
                bounds.y,
                viewport_width,
                bounds.height,
            ),
            container: Rect::new(bounds.x + viewport_x, bounds.y, viewport_width, y - padding),
            effects,
            scroll_bar: Rect::new(
                bounds.x + width - large_padding + 1,
                bounds.y,
                large_padding - 2,
                bounds.height,
            ),
        }
    }
}

/// `SynthSection::getComponentShadowWidth`.
pub fn component_shadow_width(scope: &Scope) -> i32 {
    (scope.size_ratio() * 2.0).round() as i32
}

/// `Rectangle::withLeft`: the same right edge.
pub fn with_left(rect: Rect, left: i32) -> Rect {
    Rect::new(left, rect.y, (rect.right() - left).max(0), rect.height)
}

/// `Rectangle::withTop`: the same bottom edge.
pub fn with_top(rect: Rect, top: i32) -> Rect {
    Rect::new(rect.x, top, rect.width, (rect.bottom() - top).max(0))
}

/// `SynthSection::getDividedAreaUnbuffered`.
pub fn divided_area_unbuffered(full: Rect, sections: i32, section: i32, buffer: i32) -> Rect {
    let component_width = (full.width - (sections + 1) * buffer) as f32 / (1.0 * sections as f32);
    let x = full.x
        + (section as f32 * (component_width + buffer as f32) + buffer as f32).round() as i32;
    let right =
        full.x + ((section as f32 + 1.0) * (component_width + buffer as f32)).round() as i32;
    Rect::new(x, full.y, right - x, full.height)
}

/// `SynthSection::getDividedAreaBuffered`.
pub fn divided_area_buffered(full: Rect, sections: i32, section: i32, buffer: i32) -> Rect {
    let area = divided_area_unbuffered(full, sections, section, buffer);
    Rect::new(
        area.x - buffer,
        area.y,
        area.width + 2 * buffer,
        area.height,
    )
}

/// `SynthSection::placeTempoControls`: the free or tempo slider and the
/// sync selector beside it, as (slider, sync).
pub fn place_tempo_controls(
    scope: &Scope,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> (Rect, Rect) {
    const MAX_SYNC_WIDTH_RATIO: f32 = 0.35;
    let sync_width =
        (width as f32 * MAX_SYNC_WIDTH_RATIO).min(scope.value(ValueId::TextComponentHeight)) as i32;
    let sync_y = (y as f32
        + (height - sync_width) as f32 / 2.0
        + scope.value(ValueId::TextComponentOffset)) as i32;
    (
        Rect::new(x, y, width - sync_width, height),
        Rect::new(x + width - sync_width, sync_y, sync_width, sync_width),
    )
}

use egui::{Sense, Ui, UiBuilder};
use sas_preset::upgrade::{decode_float_to_order, encode_order_to_float};

use self::chorus::ChorusSection;
use self::delay::DelaySection;
use self::distortion::DistortionSection;
use self::flanger::FlangerSection;
use self::order::{effect_index_from_y, effect_y, OrderLayout};
use self::phaser::PhaserSection;
use super::{Overlay, Panel};
use crate::colour::Colour;
use crate::controls::button::SynthButton;
use crate::controls::icon::UpstreamIcon;
use crate::controls::slider::Slider;
use crate::fonts::Face;
use crate::host::Host;
use crate::layout::FullLayout;
use crate::paint::{PixelPainter, RectF};
use crate::primitives::{Quad, QuadBatch, QuadKind};
use crate::section::{self, paint_border, Heading};
use crate::skin::ColorId;

/// The `effect_chain_order` control.
pub const ORDER_CONTROL: &str = "effect_chain_order";

/// The effects in processing order, from the `effect_chain_order` value
/// (`DragDropEffectOrder::setAllValues`).
pub fn decode_order(value: f32) -> [usize; NUM_EFFECTS] {
    let mut order = [0i32; NUM_EFFECTS];
    decode_float_to_order(&mut order, value, NUM_EFFECTS);
    order.map(|effect| effect as usize)
}

/// `vital::utils::encodeOrderToFloat`.
pub fn encode_order(order: [usize; NUM_EFFECTS]) -> f32 {
    encode_order_to_float(&order.map(|effect| effect as i32))
}

/// An effect tile being dragged (`DragDropEffectOrder::currently_dragged_`).
#[derive(Clone, Copy, Debug)]
struct Drag {
    effect: usize,
    /// `last_dragged_index_`: its place in the order now.
    place: usize,
    /// `mouse_down_y_`, relative to the order's top.
    mouse_down_y: f32,
    /// `dragged_starting_y_`: the tile's top when the drag started.
    starting_y: i32,
    /// The tile's top now, relative to the order's top.
    y: i32,
    /// The order as the drag has left it (`effect_order_`), drawn and sent
    /// to the host rather than read back from it, which may apply a set a
    /// frame later.
    order: [usize; NUM_EFFECTS],
    /// Whether the drag has moved the effect, and so holds the order's host
    /// gesture.
    moved: bool,
}

/// The Effects tab (`EffectsInterface` and its sections).
pub struct EffectsTab {
    /// `viewport_.getViewPositionY()`, in pixels.
    scroll: f32,
    /// The scroll bar's `OpenGlScrollQuad::hover_amount_`.
    scroll_hover: f32,
    /// Whether the scroll bar is pressed (`OpenGlScrollBar::mouseDown`).
    scroll_pressed: bool,
    /// Whether that press began on the thumb, so a drag moves it
    /// (`ScrollBar::isDraggingThumb`).
    scroll_thumb_drag: bool,
    drag: Option<Drag>,
    /// `currently_hovered_`: the tile under the pointer.
    hovered: Option<usize>,
    power_buttons: [SynthButton; NUM_EFFECTS],
    chorus: ChorusSection,
    delay: DelaySection,
    distortion: DistortionSection,
    flanger: FlangerSection,
    phaser: PhaserSection,
}

impl Default for EffectsTab {
    fn default() -> EffectsTab {
        EffectsTab {
            scroll: 0.0,
            scroll_hover: -1.0,
            scroll_pressed: false,
            scroll_thumb_drag: false,
            drag: None,
            hovered: None,
            power_buttons: Effect::ALL.map(|effect| SynthButton::power(effect.enable_control())),
            chorus: ChorusSection::default(),
            delay: DelaySection::default(),
            distortion: DistortionSection::default(),
            flanger: FlangerSection::default(),
            phaser: PhaserSection::default(),
        }
    }
}

impl EffectsTab {
    /// `viewport_.setViewPosition(0, y)`: scrolls the column down `y`
    /// pixels, kept within the column when next drawn.
    pub fn set_scroll(&mut self, y: f32) {
        self.scroll = y;
    }

    /// `viewport_.getViewPositionY()`.
    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    /// Draws the tab in `layout.effects` and handles its input.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        layout: &FullLayout,
        top: &Scope,
        overlay: &mut Overlay,
    ) {
        let scope = top.within(SKIN);
        let mut order = match self.drag {
            Some(drag) => drag.order,
            None => decode_order(host.value(ORDER_CONTROL)),
        };
        let enabled = Effect::ALL.map(|effect| host.value(effect.enable_control()) != 0.0);
        let fx = EffectsLayout::new(&scope, layout.effects, order, enabled);
        let order_layout = OrderLayout::new(&scope, fx.order, order);

        // EffectsInterface::paintBackground, then the column's background.
        let large_padding = scope.value(ValueId::LargePadding) as i32;
        let background = scope.color(ColorId::Background);
        pixels.fill_rect(
            fx.bounds.with_right(fx.bounds.right() - large_padding / 2),
            background,
        );
        paint_order_background(pixels, &scope, fx.order);

        self.handle_order(ui, host, pixels, &scope, &fx, &mut order);
        let order_layout = if self.drag.is_some() {
            OrderLayout::new(&scope, fx.order, order)
        } else {
            order_layout
        };
        self.show_order(ui, host, pixels, top, overlay, &fx, &order_layout, enabled);

        let max_scroll = (fx.container.height - fx.viewport.height).max(0) as f32;
        self.handle_scrolling(ui, pixels, &fx, max_scroll);
        self.show_container(ui, host, pixels, top, overlay, &fx, enabled);
        paint_scroll_bar(
            pixels,
            &scope,
            &fx,
            self.scroll,
            self.scroll_hover,
            self.scroll_pressed,
        );
    }

    /// `DragDropEffectOrder`'s mouse handling: hover, press, drag, release.
    fn handle_order(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        scope: &Scope,
        fx: &EffectsLayout,
        order: &mut [usize; NUM_EFFECTS],
    ) {
        let bounds = fx.order;
        let rect = pixels.rect(bounds.into());
        let response = ui.interact(
            rect,
            ui.scope_id().with("effect order"),
            Sense::click_and_drag(),
        );
        let ppp = pixels.pixels_per_point();
        let local_y = |pos: egui::Pos2| (pos.y - rect.min.y) * ppp;
        let power_rects: Vec<egui::Rect> = OrderLayout::new(scope, bounds, *order)
            .power_buttons
            .iter()
            .map(|&button| pixels.rect(button.into()))
            .collect();
        let over_power = |pos: egui::Pos2| power_rects.iter().any(|rect| rect.contains(pos));

        // mouseMove / mouseExit.
        self.hovered = response
            .hover_pos()
            .map(|pos| order[effect_index_from_y(scope, bounds.height, local_y(pos))]);

        let (pressed, press_origin, primary_down, pointer) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.press_origin(),
                input.pointer.primary_down(),
                input.pointer.latest_pos(),
            )
        });
        if pressed && self.drag.is_none() {
            // mouseDown, where the press began: the tile under it, unless the
            // press is on its power button.
            if let Some(origin) = press_origin.filter(|&pos| rect.contains(pos) && !over_power(pos))
            {
                let y = local_y(origin);
                let place = effect_index_from_y(scope, bounds.height, y);
                let starting_y = effect_y(scope, bounds.height, place);
                self.drag = Some(Drag {
                    effect: order[place],
                    place,
                    mouse_down_y: y,
                    starting_y,
                    y: starting_y,
                    order: *order,
                    moved: false,
                });
            }
        }
        let Some(mut drag) = self.drag else {
            return;
        };
        if !primary_down {
            // mouseUp: the tile settles into its place, and the order's
            // gesture ends.
            if drag.moved {
                host.end(ORDER_CONTROL);
            }
            self.drag = None;
            return;
        }
        if let Some(pos) = pointer {
            // mouseDrag.
            let y = local_y(pos);
            let tile_height = OrderLayout::new(scope, bounds, drag.order).tiles[drag.effect].height;
            let delta = (y - drag.mouse_down_y) as i32;
            drag.y = (drag.starting_y + delta).clamp(0, bounds.height - tile_height);
            let next = effect_index_from_y(scope, bounds.height, y);
            if next != drag.place {
                // moveEffect, sending the new order inside one gesture.
                move_effect(&mut drag.order, drag.place, next);
                drag.place = next;
                if !drag.moved {
                    host.begin(ORDER_CONTROL);
                    drag.moved = true;
                }
                host.set(ORDER_CONTROL, encode_order(drag.order));
            }
        }
        *order = drag.order;
        self.drag = Some(drag);
    }

    /// The order's tiles and their power buttons (`DraggableEffect::paint`).
    #[allow(clippy::too_many_arguments)]
    fn show_order(
        &self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        top: &Scope,
        overlay: &mut Overlay,
        fx: &EffectsLayout,
        layout: &OrderLayout,
        enabled: [bool; NUM_EFFECTS],
    ) {
        let parent_body = top.within(SKIN).color(ColorId::Body);
        let dragged = self.drag.map(|drag| drag.effect);
        let mut effects: Vec<Effect> = Effect::ALL.to_vec();
        // The dragged tile is drawn last, on top (setAlwaysOnTop).
        effects.sort_by_key(|effect| Some(*effect as usize) == dragged);
        for effect in effects {
            let index = effect as usize;
            let mut tile = layout.tiles[index];
            let mut button = layout.power_buttons[index];
            if let Some(drag) = self.drag.filter(|drag| drag.effect == index) {
                let dy = fx.order.y + drag.y - tile.y;
                tile = tile.translated(0, dy);
                button = button.translated(0, dy);
            }
            let scope = top.within(effect.skin());
            paint_tile(
                pixels,
                &scope,
                parent_body,
                effect,
                tile,
                enabled[index],
                self.hovered == Some(index),
            );
            let mut panel = Panel::new(ui, host, pixels, scope, overlay, fx.order);
            panel.button(&self.power_buttons[index], button);
            panel.finish();
        }
    }

    /// Wheel scrolling over the column (`Viewport`), when its contents are
    /// taller than it, and the scroll bar (`OpenGlScrollBar`): dragging its
    /// thumb scrolls with it, and a press on the track beside the thumb
    /// pages towards the press (`ScrollBar::mouseDown`).
    fn handle_scrolling(
        &mut self,
        ui: &mut Ui,
        pixels: &PixelPainter,
        fx: &EffectsLayout,
        max_scroll: f32,
    ) {
        /// `OpenGlScrollQuad::kHoverChange`.
        const HOVER_CHANGE: f32 = 0.2;
        let viewport = pixels.rect(fx.viewport.into());
        if max_scroll > 0.0 && ui.rect_contains_pointer(viewport) {
            let wheel = ui.input(|input| input.smooth_scroll_delta.y);
            self.scroll -= wheel * pixels.pixels_per_point();
        }
        let mut hover = false;
        self.scroll_pressed = false;
        if max_scroll > 0.0 {
            let bar = pixels.rect(fx.scroll_bar.into());
            let response = ui.interact(
                bar,
                ui.scope_id().with("effects scroll bar"),
                Sense::click_and_drag(),
            );
            hover = response.hovered() || response.dragged();
            self.scroll_pressed = response.is_pointer_button_down_on();
            let ppp = pixels.pixels_per_point();
            let total = fx.container.height as f32;
            let scale = total / fx.scroll_bar.height as f32;
            let pressed = ui.input(|input| input.pointer.primary_pressed());
            if pressed && response.is_pointer_button_down_on() {
                self.scroll_thumb_drag = false;
                if let Some(pos) = response.interact_pointer_pos() {
                    let y = (pos.y - bar.min.y) * ppp * scale;
                    let page = fx.viewport.height as f32;
                    if y < self.scroll {
                        self.scroll -= page;
                    } else if y > self.scroll + page {
                        self.scroll += page;
                    } else {
                        self.scroll_thumb_drag = true;
                    }
                }
            }
            if response.dragged() && self.scroll_thumb_drag {
                self.scroll += response.drag_delta().y * ppp * scale;
            }
        }
        // OpenGlScrollQuad::render: the bar widens while hovered.
        let last_hover = self.scroll_hover;
        self.scroll_hover = if hover {
            (self.scroll_hover + HOVER_CHANGE).min(1.0)
        } else {
            (self.scroll_hover - HOVER_CHANGE).max(0.0)
        };
        if self.scroll_hover != last_hover {
            ui.ctx().request_repaint();
        }
        self.scroll = self.scroll.clamp(0.0, max_scroll);
    }

    /// The enabled effects, scrolled and clipped to the viewport.
    #[allow(clippy::too_many_arguments)]
    fn show_container(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        top: &Scope,
        overlay: &mut Overlay,
        fx: &EffectsLayout,
        enabled: [bool; NUM_EFFECTS],
    ) {
        let clip = pixels.rect(fx.viewport.into());
        let clipped = pixels.painter().with_clip_rect(clip);
        let pixels = PixelPainter::new(&clipped, pixels.area(), pixels.pixels_per_point());
        // Its own id, so the sections' power buttons keep state apart from
        // the order list's buttons for the same controls.
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt("effect sections")
                .max_rect(ui.max_rect()),
        );
        child.set_clip_rect(clip);
        let scroll = self.scroll.round() as i32;
        for effect in Effect::ALL {
            let index = effect as usize;
            let Some(bounds) = fx.effects[index].filter(|_| enabled[index]) else {
                continue;
            };
            let bounds = bounds.translated(0, -scroll);
            let scope = top.within(effect.skin());
            match effect {
                Effect::Chorus => self.chorus.show(
                    &mut child,
                    host,
                    &pixels,
                    &scope,
                    overlay,
                    bounds,
                    fx.viewport,
                ),
                Effect::Delay => self.delay.show(
                    &mut child,
                    host,
                    &pixels,
                    &scope,
                    overlay,
                    bounds,
                    fx.viewport,
                ),
                Effect::Distortion => self.distortion.show(
                    &mut child,
                    host,
                    &pixels,
                    &scope,
                    overlay,
                    bounds,
                    fx.viewport,
                ),
                Effect::Flanger => self.flanger.show(
                    &mut child,
                    host,
                    &pixels,
                    &scope,
                    overlay,
                    bounds,
                    fx.viewport,
                ),
                Effect::Phaser => self.phaser.show(
                    &mut child,
                    host,
                    &pixels,
                    &scope,
                    overlay,
                    bounds,
                    fx.viewport,
                ),
                // G6b (compressor, EQ, reverb) and G3b's filter: the section
                // body until then.
                _ => paint_section_background(&pixels, &scope, effect, bounds, &[], &[]),
            }
            // SynthSection::setActivator: the power button tops the heading.
            let mut panel = Panel::new(&mut child, host, &pixels, scope, overlay, fx.viewport);
            panel.button(
                &self.power_buttons[index],
                section::power_button_bounds(&scope, bounds),
            );
            panel.finish();
        }
    }
}

/// `DragDropEffectOrder::moveEffect` on the order (without notifying).
pub fn move_effect(order: &mut [usize; NUM_EFFECTS], start: usize, end: usize) {
    if start == end {
        return;
    }
    let moving = order[start];
    if start < end {
        order.copy_within(start + 1..=end, start);
    } else {
        order.copy_within(end..start, end + 1);
    }
    order[end] = moving;
}

/// `DragDropEffectOrder::paintBackground`: the connection down the middle.
fn paint_order_background(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    const CONNECTION_WIDTH: f32 = 0.1;
    let width = (bounds.width as f32 * CONNECTION_WIDTH) as i32;
    let center = (bounds.width as f32 / 2.0 + scope.value(ValueId::WidgetRoundedCorner)) as i32;
    pixels.fill_rect(
        Rect::new(
            bounds.x + center - width / 2,
            bounds.y,
            width,
            bounds.height,
        ),
        scope.color(ColorId::LightenScreen),
    );
}

/// `DraggableEffect::paint`.
fn paint_tile(
    pixels: &PixelPainter,
    scope: &Scope,
    parent_body: Colour,
    effect: Effect,
    bounds: Rect,
    enabled: bool,
    hover: bool,
) {
    const LEFT_PADDING: f32 = 0.07;
    const ICON_SIZE: f32 = 0.6;
    const TEXTURE_ROWS: usize = 2;
    const TEXTURE_COLUMNS: usize = 3;
    const TEXTURE_Y_START: f32 = 0.13;
    const TEXTURE_PADDING: f32 = 0.45;
    const TEXTURE_CIRCLE_RADIUS_PERCENT: f32 = 0.25;

    let round_amount = scope.value(ValueId::BodyRounding) as i32 as f32;
    pixels.fill_rounded_rect(bounds, round_amount, parent_body);
    let colour = if enabled {
        scope.color(ColorId::PowerButtonOn)
    } else {
        scope.color(ColorId::PowerButtonOff)
    };
    pixels.draw_rounded_rect(RectF::from(bounds).reduced(0.5), round_amount, 1.0, colour);

    let (width, height) = (bounds.width as f32, bounds.height as f32);
    let text_x = width * LEFT_PADDING;
    let name = effect.name().split('_').next().unwrap_or_default();
    let text = format!("{}{}", name[..1].to_uppercase(), &name[1..]);
    pixels.text_left(
        &text,
        RectF::new(
            bounds.x as f32 + text_x,
            bounds.y as f32,
            width - text_x,
            height,
        ),
        Face::Regular,
        scope.size_ratio() * 12.0,
        colour,
    );

    let icon_width = ICON_SIZE * height;
    let icon_x = width / 2.0 + (width / 2.0 - icon_width) / 2.0;
    let icon_y = (height - icon_width) / 2.0;
    tile_icon(effect).fill(
        pixels,
        RectF::new(
            bounds.x as f32 + icon_x,
            bounds.y as f32 + icon_y,
            icon_width,
            icon_width,
        ),
        colour,
    );

    if hover {
        let dots = scope
            .color(ColorId::LightenScreen)
            .with_multiplied_alpha(1.5);
        let spacing = width * (1.0 - 2.0 * TEXTURE_PADDING) / (TEXTURE_COLUMNS as f32 - 1.0);
        let radius = spacing * TEXTURE_CIRCLE_RADIUS_PERCENT;
        let x = width * TEXTURE_PADDING;
        let y = height * TEXTURE_Y_START;
        for c in 0..TEXTURE_COLUMNS {
            for r in 0..TEXTURE_ROWS {
                for cy in [y + spacing * r as f32, height - y - spacing * r as f32] {
                    pixels.fill_ellipse(
                        RectF::new(
                            bounds.x as f32 + x + spacing * c as f32 - radius,
                            bounds.y as f32 + cy - radius,
                            2.0 * radius,
                            2.0 * radius,
                        ),
                        dots,
                    );
                }
            }
        }
    }
}

/// `getPathForEffect`.
fn tile_icon(effect: Effect) -> UpstreamIcon {
    match effect {
        Effect::Chorus => UpstreamIcon::Chorus,
        Effect::Compressor => UpstreamIcon::Compressor,
        Effect::Delay => UpstreamIcon::Delay,
        Effect::Distortion => UpstreamIcon::Distortion,
        Effect::Eq => UpstreamIcon::Equalizer,
        Effect::FilterFx => UpstreamIcon::EffectsFilter,
        Effect::Flanger => UpstreamIcon::Flanger,
        Effect::Phaser => UpstreamIcon::Phaser,
        Effect::Reverb => UpstreamIcon::Reverb,
    }
}

/// `OpenGlScrollBar`: a rounded bar over the visible part of the column,
/// shown while the column is taller than the viewport (JUCE's auto-hide).
#[allow(clippy::too_many_arguments)]
fn paint_scroll_bar(
    pixels: &PixelPainter,
    scope: &Scope,
    fx: &EffectsLayout,
    scroll: f32,
    hover: f32,
    pressed: bool,
) {
    let total = fx.container.height as f32;
    let visible = fx.viewport.height as f32;
    if total <= visible || total <= 0.0 {
        return;
    }
    let bar = fx.scroll_bar;
    let start = scroll / total;
    let end = (scroll + visible) / total;
    // setShrinkLeft: the left half of the track at rest, all of it hovered.
    let width = bar.width as f32 * (1.0 + hover.max(0.0)) / 2.0;
    let rect = RectF::new(
        bar.x as f32,
        bar.y as f32 + start * bar.height as f32,
        width,
        (end - start) * bar.height as f32,
    );
    let color = scope.color(ColorId::LightenScreen);
    let color = if pressed {
        color.overlaid_with(color)
    } else {
        color
    };
    let mut quad = Quad::new(QuadKind::RoundedRectangle, rect, color);
    quad.rounding = bar.width as f32 * 0.25;
    let mut batch = QuadBatch::new();
    batch.push(quad);
    batch.paint(pixels, RectF::from(bar));
}

/// `SynthSection::paintBackground` for an effect section at `bounds`:
/// the container and sideways heading, the rotary knobs' shadows, the
/// viewers' backgrounds, then the border.
pub fn paint_section_background(
    pixels: &PixelPainter,
    scope: &Scope,
    effect: Effect,
    bounds: Rect,
    knobs: &[(&Slider, Rect)],
    viewers: &[Rect],
) {
    section::paint_container(pixels, scope, bounds, Heading::Sideways);
    section::paint_sideways_heading_text(pixels, scope, effect.title(), bounds, true);
    for &(slider, rect) in knobs {
        slider.paint_shadow(pixels, scope, rect);
    }
    for &viewer in viewers {
        // OpenGlComponent::paintBackground.
        pixels.fill_rect(viewer, scope.color(ColorId::WidgetBackground));
    }
    paint_border(pixels, scope, bounds);
}

/// A viewer's rounded corners (`OpenGlComponent::renderCorners`), in the
/// section's body colour.
pub fn viewer_corners(panel: &mut Panel, viewer: Rect) {
    let rounding = panel.scope.value(ValueId::WidgetRoundedCorner);
    let body = panel.scope.color(ColorId::Body);
    for quad in Quad::corners(viewer.into(), rounding, body) {
        panel.quad(quad);
    }
}

/// The bounds a frequency's text background and label span: from the
/// slider to the right of its sync selector.
pub fn frequency_bounds(frequency: Rect, sync: Rect) -> Rect {
    Rect::new(
        frequency.x,
        frequency.y,
        sync.right() - frequency.x,
        frequency.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `DragDropEffectOrder::moveEffect`: the effects between the two
    /// positions shift one place towards the start, and the moved one
    /// lands at the end position.
    #[test]
    fn moving_an_effect_shifts_the_ones_between() {
        let mut order = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        move_effect(&mut order, 1, 4);
        assert_eq!(order, [0, 2, 3, 4, 1, 5, 6, 7, 8]);
        move_effect(&mut order, 6, 0);
        assert_eq!(order, [6, 0, 2, 3, 4, 1, 5, 7, 8]);
        move_effect(&mut order, 3, 3);
        assert_eq!(order, [6, 0, 2, 3, 4, 1, 5, 7, 8]);
    }
}
