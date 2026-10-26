// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the top-level sections go.
//!
//! Ports the geometry of `FullInterface::resized`
//! (`vital/src/interface/editor_sections/full_interface.cpp`),
//! `HeaderSection::resized` (`header_section.cpp`) and the preset selector's
//! `resized` (`editor_components/synth_preset_selector.cpp`,
//! `preset_selector.cpp`). Everything is in integer pixels with upstream's
//! own float-to-int truncations. [`FullLayout::new`] takes the window size in
//! physical pixels, as upstream lays out on high-DPI screens.
//!
//! The rectangles match upstream exactly at display scale 1 for any window
//! size, and at higher display scales when the window keeps the default
//! aspect ratio. Upstream's letterbox branches mix logical and physical
//! pixels when the display scale is above 1; the port letterboxes in physical
//! pixels throughout (`migration/DIVERGENCES.md` D15).

use crate::scope::Scope;
use crate::skin::{Section, Skin, ValueId};

/// `juce::Rectangle<int>`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> i32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height
    }

    /// `Rectangle::withRight`.
    pub fn with_right(&self, right: i32) -> Rect {
        Rect::new(self.x, self.y, (right - self.x).max(0), self.height)
    }

    /// `Rectangle::translated`.
    pub fn translated(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }
}

/// `HeaderSection::kTopHeight` in `FullInterface::resized`.
const TOP_HEIGHT: i32 = 48;
/// `vital::kDefaultWindowWidth` / `kDefaultWindowHeight`.
pub const DEFAULT_WIDTH: i32 = 1400;
pub const DEFAULT_HEIGHT: i32 = 820;

/// The header's parts, in interface coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeaderLayout {
    pub bounds: Rect,
    pub logo: Rect,
    pub tabs: Rect,
    pub preset_selector: Rect,
    /// `SynthPresetSelector::selector_`, the `PresetSelector` itself.
    pub preset: PresetSelectorLayout,
    pub save_button: Rect,
    pub menu_button: Rect,
    pub volume: Rect,
    pub oscilloscope: Rect,
}

/// `PresetSelector`'s parts, in interface coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresetSelectorLayout {
    pub bounds: Rect,
    pub text: Rect,
    /// `PlainTextComponent::setTextSize`: the title font's point height.
    pub text_size: f32,
    pub previous: Rect,
    pub next: Rect,
}

/// The top-level sections, in interface coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FullLayout {
    /// `SynthSection::size_ratio_`.
    pub size_ratio: f32,
    /// The area the interface draws in (letterboxed when the window's aspect
    /// ratio differs from the default).
    pub bounds: Rect,
    pub header: HeaderLayout,
    /// The Voice tab (`synthesis`), the Matrix tab and the Advanced tab
    /// (`master_controls`) all fill this.
    pub main: Rect,
    /// The Effects tab, one large padding wider than `main`.
    pub effects: Rect,
    pub modulation: Rect,
    pub portamento: Rect,
    pub voice: Rect,
    pub bend: Rect,
    pub extra_mod: Rect,
    pub keyboard: Rect,
    /// `FullInterface::paintBackground`'s two bars between the columns.
    pub bars: [Rect; 2],
}

impl FullLayout {
    /// `FullInterface::resized` for a window `width` × `height` pixels.
    pub fn new(skin: &Skin, width: i32, height: i32) -> FullLayout {
        let mut left = 0;
        let mut top = 0;
        let mut full_width = width;
        let mut full_height = height;

        let width_ratio = width as f32 / (1.0f32 * DEFAULT_WIDTH as f32);
        let height_ratio = height as f32 / (1.0f32 * DEFAULT_HEIGHT as f32);
        let mut ratio = width_ratio;
        if width_ratio > height_ratio + 1.0f32 / DEFAULT_HEIGHT as f32 {
            ratio = height_ratio;
            full_width = (height_ratio * DEFAULT_WIDTH as f32) as i32;
            left = (width - full_width) / 2;
        }
        if height_ratio > width_ratio + 1.0f32 / DEFAULT_HEIGHT as f32 {
            ratio = width_ratio;
            full_height = (ratio * DEFAULT_HEIGHT as f32) as i32;
            top = (height - full_height) / 2;
        }
        let (width, height) = (full_width, full_height);
        let scope = Scope::top(skin, ratio);

        let padding = scope.padding() as i32;
        let voice_padding = scope.value(ValueId::LargePadding) as i32;
        let extra_mod_width = scope.value(ValueId::ModulationButtonWidth) as i32;
        let main_x = left + extra_mod_width + 2 * voice_padding;
        let top_height = (TOP_HEIGHT as f32 * ratio) as i32;

        let knob_section_height = scope.knob_section_height() as i32;
        let keyboard_section_height = (knob_section_height as f32 * 0.7f32) as i32;
        let voice_height = height - top_height - keyboard_section_height;

        let section_one_width = (350.0 * ratio) as i32;
        let section_two_width = section_one_width;
        let audio_width = section_one_width + section_two_width + padding;
        let modulation_width = width - audio_width - extra_mod_width - 4 * voice_padding;

        let tab_offset = extra_mod_width + 2 * voice_padding;
        let header_bounds = Rect::new(left, top, width, top_height);
        let main = Rect::new(main_x, top + top_height, audio_width, voice_height);
        let effects = main.with_right(main.right() + voice_padding);
        let modulation_height = voice_height - knob_section_height - padding;
        let modulation = Rect::new(
            main.right() + voice_padding,
            main.y,
            modulation_width,
            modulation_height,
        );

        let voice_y = top + height - knob_section_height - keyboard_section_height;
        let portamento_width = 4 * scope.value(ValueId::ModulationButtonWidth) as i32;
        let portamento_x = modulation.right() - portamento_width;
        let portamento = Rect::new(portamento_x, voice_y, portamento_width, knob_section_height);

        let voice_width = modulation.width - portamento_width - padding;
        let voice = Rect::new(modulation.x, voice_y, voice_width, knob_section_height);

        let bend = Rect::new(
            left + voice_padding,
            top + height - knob_section_height - padding,
            extra_mod_width,
            knob_section_height,
        );

        let extra_mod_height = height - top_height - knob_section_height - padding - 1;
        let extra_mod = Rect::new(
            left + voice_padding,
            top + top_height,
            extra_mod_width,
            extra_mod_height,
        );

        let keyboard_height = keyboard_section_height - voice_padding - padding;
        let keyboard_x = extra_mod.right() + voice_padding;
        let keyboard_width = modulation.right() - keyboard_x;
        let keyboard = Rect::new(
            keyboard_x,
            top + height - keyboard_height - padding,
            keyboard_width,
            keyboard_height,
        );

        let header = HeaderLayout::new(skin, ratio, header_bounds, tab_offset);

        let bar_width = 6 * padding;
        let bar_y = header.bounds.bottom();
        let bar_height = keyboard.y - bar_y;
        let bars = [
            Rect::new(extra_mod.right() + padding, bar_y, bar_width, bar_height),
            Rect::new(main.right() + padding, bar_y, bar_width, bar_height),
        ];

        FullLayout {
            size_ratio: ratio,
            bounds: Rect::new(left, top, width, height),
            header,
            main,
            effects,
            modulation,
            portamento,
            voice,
            bend,
            extra_mod,
            keyboard,
            bars,
        }
    }
}

impl HeaderLayout {
    /// `HeaderSection::resized`, with the header at `bounds`.
    fn new(skin: &Skin, size_ratio: f32, bounds: Rect, tab_offset: i32) -> HeaderLayout {
        let scope = Scope::top(skin, size_ratio).within(&[Section::Header]);
        let height = bounds.height;
        let width = bounds.width;
        let widget_margin = scope.widget_margin() as i32;
        let large_padding = scope.value(ValueId::LargePadding) as i32;
        let logo_width = scope.value(ValueId::ModulationButtonWidth) as i32;
        let logo = Rect::new(large_padding, 0, logo_width, height);

        let preset_selector_width = width / 3;
        let preset_selector_height = (height as f32 * 0.6f32) as i32;
        let preset_selector_buffer = ((height - preset_selector_height) as f32 * 0.5f32) as i32;
        let preset_selector_x = (width - preset_selector_width + 2 * preset_selector_height) / 2;
        let preset_selector = Rect::new(
            preset_selector_x,
            preset_selector_buffer,
            preset_selector_width,
            preset_selector_height,
        );

        let component_padding = (12.0 * size_ratio) as i32;
        let volume_width = (width - preset_selector.right() - 2 * component_padding) / 2;
        let oscilloscope_width = volume_width - widget_margin;
        let oscilloscope_x = width - oscilloscope_width - large_padding;
        let volume = Rect::new(
            preset_selector.right() + component_padding,
            0,
            volume_width,
            height,
        );
        let oscilloscope = Rect::new(
            oscilloscope_x,
            widget_margin,
            oscilloscope_width,
            height - 2 * widget_margin,
        );

        let tabs_width = preset_selector_x - component_padding - tab_offset;
        let tabs = Rect::new(tab_offset, 0, tabs_width, height);

        // SynthPresetSelector::resized.
        let selector_height = preset_selector.height;
        let selector_padding = (0.2f32 * selector_height as f32) as i32;
        let selector = Rect::new(
            0,
            0,
            preset_selector.width - 2 * selector_height - selector_padding,
            selector_height,
        );
        let save_button = Rect::new(
            preset_selector.width - 2 * selector_height,
            0,
            selector_height,
            selector_height,
        );
        let menu_button = Rect::new(
            preset_selector.width - selector_height,
            0,
            selector_height,
            selector_height,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        let (px, py) = (preset_selector.x + dx, preset_selector.y + dy);
        HeaderLayout {
            bounds,
            logo: logo.translated(dx, dy),
            tabs: tabs.translated(dx, dy),
            preset_selector: preset_selector.translated(dx, dy),
            preset: PresetSelectorLayout::new(selector.translated(px, py)),
            save_button: save_button.translated(px, py),
            menu_button: menu_button.translated(px, py),
            volume: volume.translated(dx, dy),
            oscilloscope: oscilloscope.translated(dx, dy),
        }
    }
}

impl PresetSelectorLayout {
    /// `PresetSelector::resized` without a text component, with the
    /// selector at `bounds`.
    pub fn new(bounds: Rect) -> PresetSelectorLayout {
        /// `PresetSelector::kDefaultFontHeightRatio`.
        const FONT_HEIGHT_RATIO: f32 = 0.63;
        let height = bounds.height;
        PresetSelectorLayout {
            bounds,
            text: Rect::new(height, 0, bounds.width - 2 * height, height)
                .translated(bounds.x, bounds.y),
            text_size: height as f32 * FONT_HEIGHT_RATIO,
            previous: Rect::new(0, 0, height, height).translated(bounds.x, bounds.y),
            next: Rect::new(bounds.width - height, 0, height, height)
                .translated(bounds.x, bounds.y),
        }
    }
}
