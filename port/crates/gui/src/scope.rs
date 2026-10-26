// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_gui_basics, GPL-3.0-only).
// SPDX-License-Identifier: GPL-3.0-or-later AND GPL-3.0-only

//! Skin lookups from inside a section.
//!
//! Ports `SynthSection::findValue` and JUCE's `findColour(id, true)` as
//! upstream uses them (`vital/src/interface/editor_sections/synth_section.cpp`):
//! a section reads its own override first, then each enclosing section's,
//! then the top level. Values in pixels scale by the size ratio.

use crate::colour::Colour;
use crate::skin::{ColorId, Section, Skin, ValueId};

/// Where a lookup happens: the sections enclosing it, innermost first, and
/// the interface's size ratio (`SynthSection::size_ratio_`).
#[derive(Clone, Copy, Debug)]
pub struct Scope<'a> {
    skin: &'a Skin,
    chain: &'a [Section],
    size_ratio: f32,
}

impl<'a> Scope<'a> {
    /// The top level (`FullInterface`).
    pub fn top(skin: &'a Skin, size_ratio: f32) -> Scope<'a> {
        Scope {
            skin,
            chain: &[],
            size_ratio,
        }
    }

    /// The same lookup inside the sections `chain`, innermost first.
    pub fn within(self, chain: &'a [Section]) -> Scope<'a> {
        Scope { chain, ..self }
    }

    pub fn skin(&self) -> &'a Skin {
        self.skin
    }

    pub fn size_ratio(&self) -> f32 {
        self.size_ratio
    }

    /// `SynthSection::findValue`.
    pub fn value(&self, id: ValueId) -> f32 {
        let value = self
            .chain
            .iter()
            .find_map(|&section| self.skin.value_override(section, id))
            .unwrap_or_else(|| self.skin.value(id));
        if Skin::should_scale_value(id) {
            self.size_ratio * value
        } else {
            value
        }
    }

    /// `Component::findColour(id, true)` on a section.
    pub fn color(&self, id: ColorId) -> Colour {
        self.chain
            .iter()
            .find_map(|&section| self.skin.color_override(section, id))
            .unwrap_or_else(|| self.skin.color(id))
    }

    /// `SynthSection::getPadding`.
    pub fn padding(&self) -> f32 {
        self.value(ValueId::Padding)
    }

    /// `SynthSection::getKnobSectionHeight`.
    pub fn knob_section_height(&self) -> f32 {
        self.value(ValueId::KnobSectionHeight)
    }

    /// `SynthSection::getStandardKnobSize`.
    pub fn standard_knob_size(&self) -> f32 {
        self.value(ValueId::KnobArcSize)
    }

    /// `SynthSection::getWidgetMargin`.
    pub fn widget_margin(&self) -> f32 {
        self.value(ValueId::WidgetMargin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_win_and_pixels_scale() {
        let skin = Skin::default();
        let top = Scope::top(&skin, 2.0);
        assert_eq!(top.value(ValueId::KnobSectionHeight), 142.0);
        assert_eq!(
            top.within(&[Section::Sample])
                .value(ValueId::KnobSectionHeight),
            140.0
        );
        assert_eq!(top.value(ValueId::KnobHandleLength), 0.5);
        let chorus = [Section::Chorus, Section::AllEffects];
        assert_eq!(
            top.within(&chorus).color(ColorId::PowerButtonOff),
            Colour(0xff848686)
        );
        assert_eq!(
            top.within(&[Section::Header]).color(ColorId::Body),
            Colour(0x00333333)
        );
        assert_eq!(top.color(ColorId::Body), skin.color(ColorId::Body));
    }
}
