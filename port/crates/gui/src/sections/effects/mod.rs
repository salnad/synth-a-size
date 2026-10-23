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

/// `SynthSection::getTitleWidth`.
pub fn title_width(scope: &Scope) -> i32 {
    scope.value(ValueId::TitleWidth) as i32
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
