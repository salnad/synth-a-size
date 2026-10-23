// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! JUCE's `Colour` as the skin stores it: 0xAARRGGBB, not premultiplied.

/// A colour as 0xAARRGGBB, like JUCE's `Colour::getARGB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Colour(pub u32);

impl Colour {
    pub const BLACK: Colour = Colour(0xff000000);
    /// A default-constructed JUCE `Colour`.
    pub const TRANSPARENT_BLACK: Colour = Colour(0);

    /// `Colour::fromString`: hex digits accumulate and anything else is
    /// skipped, so "8ffffff" is 0x08ffffff and "0" is transparent black.
    pub fn from_string(text: &str) -> Colour {
        let mut value: u32 = 0;
        for digit in text.chars().filter_map(|c| c.to_digit(16)) {
            value = (value << 4) | digit;
        }
        Colour(value)
    }

    pub fn alpha(self) -> u8 {
        (self.0 >> 24) as u8
    }

    pub fn red(self) -> u8 {
        (self.0 >> 16) as u8
    }

    pub fn green(self) -> u8 {
        (self.0 >> 8) as u8
    }

    pub fn blue(self) -> u8 {
        self.0 as u8
    }

    pub fn to_egui(self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.red(), self.green(), self.blue(), self.alpha())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_juce() {
        assert_eq!(Colour::from_string("ff262a2d"), Colour(0xff262a2d));
        assert_eq!(Colour::from_string("8ffffff"), Colour(0x08ffffff));
        assert_eq!(Colour::from_string("0"), Colour(0));
        assert_eq!(Colour::from_string("#ff-00-11-22"), Colour(0xff001122));
    }
}
