// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The skin: every colour and metric the interface draws with.
//!
//! Ports `vital/src/interface/look_and_feel/skin.{h,cpp}` (the `Skin` class,
//! without the skin designer window). The default skin is upstream's own
//! `vital/default.vitalskin`, read at build time, so the look stays
//! data-driven: another skin file loads through [`Skin::from_json`].

use std::collections::BTreeMap;

use crate::colour::Colour;

macro_rules! named_enum {
    ($(#[$meta:meta])* $name:ident, $names:ident, { $($variant:ident = $text:literal,)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name { $($variant,)* }

        impl $name {
            /// Every value, in upstream's enum order.
            pub const ALL: &'static [$name] = &[$($name::$variant,)*];

            /// The key upstream's skin files use.
            pub fn name(self) -> &'static str {
                $names[self as usize]
            }
        }

        const $names: &[&str] = &[$($text,)*];
    };
}

named_enum!(
    /// `Skin::SectionOverride`. `All` is upstream's `kNone`, whose overrides
    /// are the top-level values.
    Section, SECTION_NAMES, {
        All = "All",
        Logo = "Logo",
        Header = "Header",
        Overlay = "Overlays",
        Oscillator = "Oscillator",
        Sample = "Sample",
        Sub = "Sub",
        Filter = "Filter",
        Envelope = "Envelope",
        Lfo = "Lfo",
        RandomLfo = "RandomLfo",
        Voice = "Voice",
        Macro = "Macro",
        Keyboard = "Keyboard",
        AllEffects = "All Effects",
        Chorus = "Chorus",
        Compressor = "Compressor",
        Delay = "Delay",
        Distortion = "Distortion",
        Equalizer = "Equalizer",
        FxFilter = "Effects Filter",
        Flanger = "Flanger",
        Phaser = "Phaser",
        Reverb = "Reverb",
        ModulationDragDrop = "Modulation Drag Drop",
        ModulationMatrix = "Modulation Matrix",
        PresetBrowser = "Preset Browser",
        PopupBrowser = "Popup Browser",
        Advanced = "Advanced",
        WavetableEditor = "Wavetable Editor",
    }
);

named_enum!(
    /// `Skin::ValueId` (without `kFrequencyDisplay`, which is not stored).
    ValueId, VALUE_NAMES, {
        BodyRounding = "Body Rounding",
        LabelHeight = "Label Height",
        LabelBackgroundHeight = "Label Background Height",
        LabelBackgroundRounding = "Label Rounding",
        LabelOffset = "Label Offset",
        TextComponentLabelOffset = "Text Component Label Offset",
        RotaryOptionXOffset = "Rotary Option X Offset",
        RotaryOptionYOffset = "Rotary Option Y Offset",
        RotaryOptionWidth = "Rotary Option Width",
        TitleWidth = "Title Width",
        Padding = "Padding",
        LargePadding = "Large Padding",
        SliderWidth = "Slider Width",
        TextComponentHeight = "Text Component Height",
        TextComponentOffset = "Text Component Offset",
        TextComponentFontSize = "Text Component Font Size",
        TextButtonHeight = "Text Button Height",
        ButtonFontSize = "Button Font Size",
        KnobArcSize = "Knob Arc Size",
        KnobArcThickness = "Knob Arc Thickness",
        KnobBodySize = "Knob Body Size",
        KnobHandleLength = "Knob Handle Length",
        KnobModAmountArcSize = "Knob Mod Amount Arc Size",
        KnobModAmountArcThickness = "Knob Mod Amount Arc Thickness",
        KnobModMeterArcSize = "Knob Mod Meter Arc Size",
        KnobModMeterArcThickness = "Knob Mod Meter Arc Thickness",
        KnobOffset = "Knob Offset",
        KnobSectionHeight = "Knob Section Height",
        KnobShadowWidth = "Knob Shadow Width",
        KnobShadowOffset = "Knob Shadow Offset",
        ModulationButtonWidth = "Modulation Button Width",
        ModulationFontSize = "Modulation Font Size",
        WidgetMargin = "Widget Margin",
        WidgetRoundedCorner = "Widget Rounded Corner",
        WidgetLineWidth = "Widget Line Width",
        WidgetLineBoost = "Widget Line Boost",
        WidgetFillCenter = "Widget Fill Center",
        WidgetFillFade = "Widget Fill Fade",
        WidgetFillBoost = "Widget Fill Boost",
        WavetableHorizontalAngle = "Wavetable Horizontal Angle",
        WavetableVerticalAngle = "Wavetable Vertical Angle",
        WavetableDrawWidth = "Wavetable Draw Width",
        WavetableWaveHeight = "Wavetable Draw Height",
        WavetableYOffset = "Wavetable Y Offset",
    }
);

named_enum!(
    /// `Skin::ColorId`.
    ColorId, COLOR_NAMES, {
        Background = "Background",
        Body = "Body",
        BodyHeading = "Body Heading Background",
        HeadingText = "Heading Text",
        PresetText = "Preset Text",
        BodyText = "Body Text",
        Border = "Border",
        LabelBackground = "Label Background",
        LabelConnection = "Label Connection",
        PowerButtonOn = "Power Button On",
        PowerButtonOff = "Power Button Off",
        OverlayScreen = "Overlay Screen",
        LightenScreen = "Lighten Screen",
        Shadow = "Shadow",
        PopupSelectorBackground = "Popup Selector Background",
        PopupBackground = "Popup Background",
        PopupBorder = "Popup Border",
        TextComponentBackground = "Text Component Background",
        TextComponentText = "Text Component Text",
        RotaryArc = "Rotary Arc",
        RotaryArcDisabled = "Rotary Arc Disabled",
        RotaryArcUnselected = "Rotary Arc Unselected",
        RotaryArcUnselectedDisabled = "Rotary Arc Unselected Disabled",
        RotaryHand = "Rotary Hand",
        RotaryBody = "Rotary Body",
        RotaryBodyBorder = "Rotary Body Border",
        LinearSlider = "Linear Slider",
        LinearSliderDisabled = "Linear Slider Disabled",
        LinearSliderUnselected = "Linear Slider Unselected",
        LinearSliderThumb = "Linear Slider Thumb",
        LinearSliderThumbDisabled = "Linear Slider Thumb Disabled",
        WidgetCenterLine = "Widget Center Line",
        WidgetPrimary1 = "Widget Primary 1",
        WidgetPrimary2 = "Widget Primary 2",
        WidgetPrimaryDisabled = "Widget Primary Disabled",
        WidgetSecondary1 = "Widget Secondary 1",
        WidgetSecondary2 = "Widget Secondary 2",
        WidgetSecondaryDisabled = "Widget Secondary Disabled",
        WidgetAccent1 = "Widget Accent 1",
        WidgetAccent2 = "Widget Accent 2",
        WidgetBackground = "Widget Background",
        ModulationMeter = "Modulation Meter",
        ModulationMeterLeft = "Modulation Meter Left",
        ModulationMeterRight = "Modulation Meter Right",
        ModulationMeterControl = "Modulation Meter Control",
        ModulationButtonSelected = "Modulation Button Selected",
        ModulationButtonDragging = "Modulation Button Dragging",
        ModulationButtonUnselected = "Modulation Button Unselected",
        IconSelectorIcon = "Icon Selector Icon",
        IconButtonOff = "Icon Button Off",
        IconButtonOffHover = "Icon Button Off Hover",
        IconButtonOffPressed = "Icon Button Off Pressed",
        IconButtonOn = "Icon Button On",
        IconButtonOnHover = "Icon Button On Hover",
        IconButtonOnPressed = "Icon Button On Pressed",
        UiButton = "UI Button",
        UiButtonText = "UI Button Text",
        UiButtonHover = "UI Button Hover",
        UiButtonPressed = "UI Button Press",
        UiActionButton = "UI Action Button",
        UiActionButtonHover = "UI Action Button Hover",
        UiActionButtonPressed = "UI Action Button Press",
        TextEditorBackground = "Text Editor Background",
        TextEditorBorder = "Text Editor Border",
        TextEditorCaret = "Text Editor Caret",
        TextEditorSelection = "Text Editor Selection",
    }
);

/// Upstream's default skin, `vital/default.vitalskin`.
pub const DEFAULT_SKIN_JSON: &str = include_str!("../../../../vital/default.vitalskin");

/// `Skin`: top-level colours and values plus per-section overrides.
#[derive(Clone, Debug, PartialEq)]
pub struct Skin {
    colors: Vec<Colour>,
    values: Vec<f32>,
    color_overrides: Vec<BTreeMap<ColorId, Colour>>,
    value_overrides: Vec<BTreeMap<ValueId, f32>>,
}

impl Default for Skin {
    /// `Skin::loadDefaultSkin`.
    fn default() -> Skin {
        let data: serde_json::Value =
            serde_json::from_str(DEFAULT_SKIN_JSON).expect("the default skin is valid JSON");
        Skin::from_json(&data)
    }
}

impl Skin {
    /// `Skin::shouldScaleValue`: whether a value is in pixels at the default
    /// window size, and so scales with the window.
    pub fn should_scale_value(id: ValueId) -> bool {
        !matches!(
            id,
            ValueId::WidgetFillFade
                | ValueId::WidgetFillBoost
                | ValueId::WidgetLineBoost
                | ValueId::KnobHandleLength
                | ValueId::WidgetFillCenter
                | ValueId::WavetableHorizontalAngle
                | ValueId::WavetableVerticalAngle
        )
    }

    /// `Skin::jsonToState`, after `Skin::updateJson`. Colours a skin leaves
    /// out stay black and values read 0, as upstream.
    pub fn from_json(data: &serde_json::Value) -> Skin {
        let data = update_json(data.clone());
        let mut skin = Skin {
            colors: vec![Colour::BLACK; ColorId::ALL.len()],
            values: vec![0.0; ValueId::ALL.len()],
            color_overrides: vec![BTreeMap::new(); Section::ALL.len()],
            value_overrides: vec![BTreeMap::new(); Section::ALL.len()],
        };

        if let Some(overrides) = data.get("overrides") {
            for &section in Section::ALL {
                let Some(entries) = overrides.get(section.name()) else {
                    continue;
                };
                for &id in ColorId::ALL {
                    if let Some(text) = entries.get(id.name()).and_then(|v| v.as_str()) {
                        skin.color_overrides[section as usize]
                            .insert(id, Colour::from_string(text));
                    }
                }
                for &id in ValueId::ALL {
                    if let Some(value) = entries.get(id.name()).and_then(|v| v.as_f64()) {
                        skin.value_overrides[section as usize].insert(id, value as f32);
                    }
                }
            }
        }

        for &id in ColorId::ALL {
            if let Some(text) = data.get(id.name()).and_then(|v| v.as_str()) {
                skin.colors[id as usize] = Colour::from_string(text);
            }
        }
        for &id in ValueId::ALL {
            skin.values[id as usize] =
                data.get(id.name()).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        }
        skin
    }

    /// `Skin::getColor(ColorId)`: the top-level colour.
    pub fn color(&self, id: ColorId) -> Colour {
        self.colors[id as usize]
    }

    /// `Skin::getValue(ValueId)`: the top-level value, unscaled.
    pub fn value(&self, id: ValueId) -> f32 {
        self.values[id as usize]
    }

    /// The colour section `section`'s own overrides set, if any.
    pub fn color_override(&self, section: Section, id: ColorId) -> Option<Colour> {
        self.color_overrides[section as usize].get(&id).copied()
    }

    /// The value section `section`'s own overrides set, if any.
    pub fn value_override(&self, section: Section, id: ValueId) -> Option<f32> {
        self.value_overrides[section as usize].get(&id).copied()
    }
}

/// `Skin::updateJson`: upgrades skins saved by older versions.
fn update_json(mut data: serde_json::Value) -> serde_json::Value {
    let version = data
        .get("synth_version")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let Some(object) = data.as_object_mut() else {
        return data;
    };
    if version < 0x608 {
        for (new, old) in [
            ("Knob Arc Size", "Knob Size"),
            ("Knob Arc Thickness", "Knob Thickness"),
            ("Knob Handle Length", "Knob Handle Radial Amount"),
            ("Knob Mod Amount Arc Size", "Knob Mod Amount Size"),
            ("Knob Mod Amount Arc Thickness", "Knob Mod Amount Thickness"),
            ("Knob Mod Meter Arc Size", "Knob Mod Meter Size"),
            ("Knob Mod Meter Arc Thickness", "Knob Mod Meter Thickness"),
        ] {
            let value = object.get(old).cloned().unwrap_or(serde_json::Value::Null);
            object.insert(new.to_string(), value);
        }
    }
    if !object.contains_key("Widget Fill Boost") {
        object.insert("Widget Fill Boost".to_string(), serde_json::json!(1.6f32));
    }
    if !object.contains_key("Widget Line Boost") {
        object.insert("Widget Line Boost".to_string(), serde_json::json!(1.0f32));
    }
    if version < 0x609 {
        object.insert(
            "Modulation Meter".to_string(),
            serde_json::json!("ffffffff"),
        );
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_skin_reads_upstream_values() {
        let skin = Skin::default();
        assert_eq!(skin.color(ColorId::Background), Colour(0xff262a2d));
        assert_eq!(skin.color(ColorId::Border), Colour(0x08ffffff));
        assert_eq!(skin.value(ValueId::KnobSectionHeight), 71.0);
        assert_eq!(skin.value(ValueId::WidgetMargin), 5.5);
        assert_eq!(
            skin.color_override(Section::Header, ColorId::Body),
            Some(Colour(0x00333333))
        );
        assert_eq!(
            skin.value_override(Section::Sample, ValueId::KnobSectionHeight),
            Some(70.0)
        );
        assert_eq!(skin.color_override(Section::Voice, ColorId::Body), None);
    }

    #[test]
    fn every_default_key_is_known() {
        let data: serde_json::Value = serde_json::from_str(DEFAULT_SKIN_JSON).unwrap();
        let known: Vec<&str> = ColorId::ALL
            .iter()
            .map(|id| id.name())
            .chain(ValueId::ALL.iter().map(|id| id.name()))
            .chain(["overrides", "synth_version"])
            .collect();
        for key in data.as_object().unwrap().keys() {
            assert!(known.contains(&key.as_str()), "unknown skin key {key}");
        }
        for section in data["overrides"].as_object().unwrap().keys() {
            assert!(
                Section::ALL.iter().any(|s| s.name() == section),
                "unknown section {section}"
            );
        }
    }

    #[test]
    fn scaled_values_match_upstream() {
        assert!(Skin::should_scale_value(ValueId::Padding));
        assert!(!Skin::should_scale_value(ValueId::KnobHandleLength));
        assert!(!Skin::should_scale_value(ValueId::WavetableVerticalAngle));
    }
}
