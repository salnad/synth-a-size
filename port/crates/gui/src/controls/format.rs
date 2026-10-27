// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_core, ISC).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The text a slider shows for its value.
//!
//! Ports `SynthSlider::getAdjustedValue`, `getValueFromAdjusted`,
//! `getSliderTextFromValue`, `formatValue` and `getValueFromText`
//! (`vital/src/interface/editor_components/synth_slider.cpp`). These differ
//! from the host's text (`ValueBridge`, `sas_preset::display`): the slider
//! applies `post_offset` before inverting and multiplying, and cuts the
//! number to a fixed count of characters.

use sas_preset::registry::{ParamInfo, ValueScale};

/// How a slider prints its value (`SynthSlider`'s formatting settings).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Format {
    /// `max_display_characters_` (`kDefaultFormatLength`).
    pub max_characters: usize,
    /// `max_decimal_places_` (`kDefaultFormatDecimalPlaces`).
    pub max_decimal_places: usize,
    /// `use_suffix_`: append the units.
    pub use_suffix: bool,
    /// `display_multiply_`, when a section overrides the parameter's.
    pub display_multiply: Option<f32>,
    /// `display_exponential_base_`.
    pub exponential_base: f32,
    /// `string_lookup_` (`setStringLookup`): option names for a slider whose
    /// control has none of its own.
    pub string_lookup: Option<&'static [&'static str]>,
}

impl Default for Format {
    fn default() -> Format {
        Format {
            max_characters: 5,
            max_decimal_places: 5,
            use_suffix: true,
            display_multiply: None,
            exponential_base: 2.0,
            string_lookup: None,
        }
    }
}

/// `SynthSlider::getAdjustedValue`: engine value to displayed number.
pub fn adjusted_value(info: &ParamInfo, format: &Format, value: f64) -> f64 {
    let mut adjusted = value;
    match info.value_scale {
        ValueScale::Quadratic => adjusted *= adjusted,
        ValueScale::Cubic => adjusted = adjusted * adjusted * adjusted,
        ValueScale::Quartic => {
            adjusted *= adjusted;
            adjusted *= adjusted;
        }
        ValueScale::Exponential => adjusted = format.exponential_base.powf(adjusted as f32) as f64,
        ValueScale::SquareRoot => adjusted = (adjusted.max(0.0) as f32).sqrt() as f64,
        ValueScale::Indexed | ValueScale::Linear => {}
    }
    adjusted += info.post_offset as f64;
    if info.display_invert {
        adjusted = 1.0 / adjusted;
    }
    match format.display_multiply {
        Some(multiply) if multiply != 0.0 => adjusted * multiply as f64,
        _ => adjusted * info.display_multiply as f64,
    }
}

/// `SynthSlider::getValueFromAdjusted`: displayed number to engine value.
pub fn value_from_adjusted(info: &ParamInfo, format: &Format, value: f64) -> f64 {
    let mut readjusted = match format.display_multiply {
        Some(multiply) if multiply != 0.0 => value / multiply as f64,
        _ => value / info.display_multiply as f64,
    };
    if info.display_invert {
        readjusted = 1.0 / readjusted;
    }
    readjusted -= info.post_offset as f64;
    match info.value_scale {
        ValueScale::Quadratic => (readjusted.max(0.0) as f32).sqrt() as f64,
        ValueScale::Cubic => (readjusted.max(0.0) as f32).powf(1.0 / 3.0) as f64,
        ValueScale::Quartic => (readjusted.max(0.0) as f32).sqrt().sqrt() as f64,
        ValueScale::Exponential => readjusted.ln() / (format.exponential_base as f64).ln(),
        ValueScale::SquareRoot => readjusted * readjusted,
        ValueScale::Indexed | ValueScale::Linear => readjusted,
    }
}

/// `SynthSlider::formatValue`.
pub fn format_value(info: &ParamInfo, format: &Format, value: f32) -> String {
    let mut text = if info.value_scale == ValueScale::Indexed {
        juce_float(value)
    } else {
        let mut text = if format.max_decimal_places == 0 {
            juce_float(value.round())
        } else {
            format!("{:.*}", format.max_decimal_places, value)
        };
        let mut characters = format.max_characters;
        if text.starts_with('-') {
            characters += 1;
        }
        text.truncate(characters.min(text.len()));
        if text.ends_with('.') {
            text = text.replace('.', "");
        }
        text
    };
    if format.use_suffix {
        text.push_str(info.display_units);
    }
    text
}

/// `SynthSlider::getSliderTextFromValue`: the option name, or the adjusted
/// value formatted.
pub fn slider_text(info: &ParamInfo, format: &Format, value: f32) -> String {
    if let Some(options) = format.string_lookup.or_else(|| info.options()) {
        let index = (value as i32).clamp(0, info.max as i32) as usize;
        if let Some(option) = options.get(index) {
            return option.to_string();
        }
    }
    format_value(
        info,
        format,
        adjusted_value(info, format, value as f64) as f32,
    )
}

/// `SynthSlider::getRawTextFromValue`: what the text entry starts with.
pub fn raw_text(info: &ParamInfo, format: &Format, value: f32) -> String {
    juce_double(adjusted_value(info, format, value as f64))
}

/// `SynthSlider::getValueFromText`: an option name, a percentage of the
/// range, or a displayed number.
pub fn value_from_text(info: &ParamInfo, format: &Format, text: &str) -> f64 {
    let cleaned: String = text
        .chars()
        .filter(|c| *c != ' ')
        .collect::<String>()
        .to_lowercase();
    if let Some(options) = format.string_lookup.or_else(|| info.options()) {
        for (index, option) in options.iter().enumerate().take(info.max as usize + 1) {
            if cleaned == option.to_lowercase() {
                return index as f64;
            }
        }
    }
    if text.ends_with('%') && info.display_units != "%" {
        let t = 0.01 * juce_double_value(&cleaned.replace('%', ""));
        return (info.max - info.min) as f64 * t + info.min as f64;
    }
    value_from_adjusted(info, format, juce_double_value(text))
}

/// JUCE's `String(float)`: the default stream formatting (six significant
/// digits, no trailing zeros).
fn juce_float(value: f32) -> String {
    juce_double(value as f64)
}

fn juce_double(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let digits = 6;
    let magnitude = value.abs().log10().floor() as i32;
    if (-5..digits).contains(&magnitude) {
        let decimals = (digits - 1 - magnitude).max(0) as usize;
        let text = format!("{value:.decimals$}");
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            text
        }
    } else {
        let text = format!("{value:.*e}", (digits - 1) as usize);
        let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
        let mantissa = if mantissa.contains('.') {
            mantissa.trim_end_matches('0').trim_end_matches('.')
        } else {
            mantissa
        };
        let exponent: i32 = exponent.parse().unwrap_or(0);
        format!(
            "{mantissa}e{}{:02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.abs()
        )
    }
}

/// JUCE's `String::getDoubleValue`: the leading number, or 0.
fn juce_double_value(text: &str) -> f64 {
    let text = text.trim_start();
    let mut end = 0;
    for (index, c) in text.char_indices() {
        let sign = (c == '-' || c == '+') && (index == 0 || text[..index].ends_with(['e', 'E']));
        if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || sign {
            end = index + c.len_utf8();
        } else {
            break;
        }
    }
    let mut candidate = &text[..end];
    while !candidate.is_empty() {
        if let Ok(value) = candidate.parse::<f64>() {
            return value;
        }
        candidate = &candidate[..candidate.len() - 1];
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use sas_preset::registry::registry;

    fn info(name: &str) -> &'static ParamInfo {
        registry().get(name).unwrap()
    }

    #[test]
    fn juce_number_formatting() {
        assert_eq!(juce_double(8.0), "8");
        assert_eq!(juce_double(0.5), "0.5");
        assert_eq!(juce_double(-12.25), "-12.25");
        assert_eq!(juce_double(1.0 / 3.0), "0.333333");
        assert_eq!(juce_double(123456789.0), "1.23457e+08");
        assert_eq!(juce_double_value(" 12.5dB"), 12.5);
        assert_eq!(juce_double_value("abc"), 0.0);
    }

    #[test]
    fn values_format_like_synth_slider() {
        let format = Format::default();
        // Linear percentage: velocity tracking at 0.
        assert_eq!(slider_text(info("velocity_track"), &format, 0.0), "0.000%");
        assert_eq!(
            slider_text(info("velocity_track"), &format, -0.5),
            "-50.00%"
        );
        // Indexed without suffix: polyphony.
        let polyphony = Format {
            use_suffix: false,
            ..Format::default()
        };
        assert_eq!(slider_text(info("polyphony"), &polyphony, 8.0), "8");
        // Option names: stereo mode.
        assert_eq!(slider_text(info("stereo_mode"), &format, 1.0), "ROTATE");
    }

    #[test]
    fn text_round_trips_through_the_adjusted_value() {
        let format = Format::default();
        let glide = info("portamento_time");
        let value = value_from_text(glide, &format, &raw_text(glide, &format, -3.0));
        assert!((value + 3.0).abs() < 1e-5, "{value}");
        let velocity = info("velocity_track");
        assert!((value_from_text(velocity, &format, "50%") - 0.5).abs() < 1e-6);
        assert!((value_from_text(velocity, &format, "25") - 0.25).abs() < 1e-6);
    }
}
