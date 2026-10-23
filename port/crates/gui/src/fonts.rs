// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface's typefaces.
//!
//! Ports `vital/src/interface/look_and_feel/fonts.{h,cpp}`: the same five
//! faces from `vital/fonts/` (Lato, Montserrat and Droid Sans Mono; see
//! `vital/fonts/LICENSE`), registered as egui font families.
//!
//! On macOS, JUCE's CoreText fonts make a point height
//! (`Font::withPointHeight`) the em size, which is what egui's font size
//! means too, so sizes carry over unchanged. JUCE's FreeType fonts (Linux,
//! where the ref-gui reference renders) instead make the em size
//! `points * unitsPerEm / ascender`: 1.3% larger for Lato, 3.3% for
//! Montserrat and 7.7% for Droid Sans Mono. The port follows macOS, its
//! target, so text in the Linux reference is that much larger.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

/// `Fonts`' faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// `proportional_regular`: Lato Regular.
    Regular,
    /// `proportional_light`: Lato Light.
    Light,
    /// `proportional_title`: Montserrat Light.
    Title,
    /// `proportional_title_regular`: Montserrat Regular.
    TitleRegular,
    /// `monospace`: Droid Sans Mono.
    Mono,
}

impl Face {
    const ALL: [Face; 5] = [
        Face::Regular,
        Face::Light,
        Face::Title,
        Face::TitleRegular,
        Face::Mono,
    ];

    fn key(self) -> &'static str {
        match self {
            Face::Regular => "proportional_regular",
            Face::Light => "proportional_light",
            Face::Title => "proportional_title",
            Face::TitleRegular => "proportional_title_regular",
            Face::Mono => "monospace",
        }
    }

    fn data(self) -> &'static [u8] {
        match self {
            Face::Regular => include_bytes!("../../../../vital/fonts/Lato-Regular.ttf"),
            Face::Light => include_bytes!("../../../../vital/fonts/Lato-Light.ttf"),
            Face::Title => include_bytes!("../../../../vital/fonts/Montserrat-Light.otf"),
            Face::TitleRegular => include_bytes!("../../../../vital/fonts/Montserrat-Regular.ttf"),
            Face::Mono => include_bytes!("../../../../vital/fonts/DroidSansMono.ttf"),
        }
    }

    pub fn family(self) -> FontFamily {
        FontFamily::Name(self.key().into())
    }

    /// The face at JUCE point height `point_height`.
    pub fn at(self, point_height: f32) -> FontId {
        FontId::new(point_height, self.family())
    }
}

/// Font definitions with every face as its own family. egui's own
/// proportional and monospace families map to Lato Regular and Droid Sans
/// Mono, so stock widgets match.
pub fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();
    for face in Face::ALL {
        fonts.font_data.insert(
            face.key().to_string(),
            Arc::new(FontData::from_static(face.data())),
        );
        fonts
            .families
            .insert(face.family(), vec![face.key().to_string()]);
    }
    fonts.families.insert(
        FontFamily::Proportional,
        vec![Face::Regular.key().to_string()],
    );
    fonts
        .families
        .insert(FontFamily::Monospace, vec![Face::Mono.key().to_string()]);
    fonts
}
