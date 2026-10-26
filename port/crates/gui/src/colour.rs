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

    /// `Colour::withAlpha` (a byte).
    pub fn with_alpha(self, alpha: u8) -> Colour {
        Colour(((alpha as u32) << 24) | (self.0 & 0x00ff_ffff))
    }

    /// `Colour::getFloatAlpha`.
    pub fn float_alpha(self) -> f32 {
        self.alpha() as f32 / 255.0
    }

    /// `Colour::withAlpha(float)`: the alpha rounded to a byte.
    pub fn with_float_alpha(self, alpha: f32) -> Colour {
        let alpha = (255.0 * alpha).round().clamp(0.0, 255.0) as u32;
        Colour((alpha << 24) | (self.0 & 0x00ff_ffff))
    }

    /// `Colour::withMultipliedAlpha` (`withAlpha` rounds to a byte).
    pub fn with_multiplied_alpha(self, multiplier: f32) -> Colour {
        if multiplier >= 1.0 {
            return self;
        }
        let alpha = (multiplier * (self.alpha() as f32 / 255.0) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u32;
        Colour((alpha << 24) | (self.0 & 0x00ff_ffff))
    }

    /// `Colour::withSaturation(0.0f)`: the grey of the colour's HSB
    /// brightness (its brightest channel).
    pub fn desaturated(self) -> Colour {
        let [_, r, g, b] = self.0.to_be_bytes();
        let grey = r.max(g).max(b) as u32;
        Colour((self.0 & 0xff00_0000) | (grey << 16) | (grey << 8) | grey)
    }

    /// `Colour::interpolatedWith`: a tween of the premultiplied colours in
    /// 1/255 steps.
    pub fn interpolated_with(self, other: Colour, proportion: f32) -> Colour {
        if proportion <= 0.0 {
            return self;
        }
        if proportion >= 1.0 {
            return other;
        }
        let amount = (proportion * 255.0).round() as i32;
        let premultiply = |colour: Colour| {
            let alpha = colour.alpha() as i32;
            let channel = |value: u8| {
                if alpha == 0xff {
                    value as i32
                } else {
                    (value as i32 * alpha + 0x7f) >> 8
                }
            };
            [
                alpha,
                channel(colour.red()),
                channel(colour.green()),
                channel(colour.blue()),
            ]
        };
        let (from, to) = (premultiply(self), premultiply(other));
        let mut mixed = [0i32; 4];
        for i in 0..4 {
            mixed[i] = (from[i] + (((to[i] - from[i]) * amount) >> 8)).clamp(0, 255);
        }
        let alpha = mixed[0];
        let unpremultiply = |value: i32| {
            if alpha == 0xff {
                value
            } else if alpha == 0 {
                0
            } else {
                (value * 0xff / alpha).min(0xff)
            }
        };
        Colour(
            ((alpha as u32) << 24)
                | ((unpremultiply(mixed[1]) as u32) << 16)
                | ((unpremultiply(mixed[2]) as u32) << 8)
                | unpremultiply(mixed[3]) as u32,
        )
    }

    /// `Colour::darker`: each channel scaled by 1 / (1 + amount).
    pub fn darker(self, amount: f32) -> Colour {
        let scale = 1.0 / (1.0 + amount.max(0.0));
        let channel = |value: u8| ((value as f32 * scale) as u32).min(255);
        Colour(
            ((self.alpha() as u32) << 24)
                | (channel(self.red()) << 16)
                | (channel(self.green()) << 8)
                | channel(self.blue()),
        )
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

    #[test]
    fn darker_and_alpha_like_juce() {
        assert_eq!(Colour(0xffaa88ff).darker(0.8), Colour(0xff5e4b8d));
        assert_eq!(
            Colour(0xff6b6b6b).with_multiplied_alpha(0.5),
            Colour(0x806b6b6b)
        );
        assert_eq!(
            Colour(0x806b6b6b).with_multiplied_alpha(1.5),
            Colour(0x806b6b6b)
        );
    }

    #[test]
    fn interpolation_tweens_in_byte_steps() {
        let (a, b) = (Colour(0xff848789), Colour(0xffdddddd));
        assert_eq!(a.interpolated_with(b, 0.0), a);
        assert_eq!(a.interpolated_with(b, 1.0), b);
        // amount 128 of 256: 0x84 + ((0xdd - 0x84) * 128 >> 8) = 0xb0.
        assert_eq!(a.interpolated_with(b, 0.5).red(), 0xb0);
    }
}
