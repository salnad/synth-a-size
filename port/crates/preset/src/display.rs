// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Displayed values of parameters: the conversions of upstream's
//! `ValueBridge` (`vital/src/plugin/value_bridge.h`) between a parameter's
//! engine value and the number and units its host text field shows.
//!
//! Shared by the plugin's host parameters and the LLM control layer.

use crate::registry::{ParamInfo, ValueScale};

/// `ValueBridge::skewValue`: engine value to unscaled display value.
pub fn skew(info: &ParamInfo, value: f32) -> f32 {
    match info.value_scale {
        ValueScale::Quadratic => value * value,
        ValueScale::Cubic => value * value * value,
        ValueScale::Quartic => {
            let squared = value * value;
            squared * squared
        }
        ValueScale::Exponential if info.display_invert => 1.0 / 2.0f32.powf(value),
        ValueScale::Exponential => 2.0f32.powf(value),
        ValueScale::SquareRoot => value.sqrt(),
        ValueScale::Indexed | ValueScale::Linear => value,
    }
}

/// `ValueBridge::unskewValue`: the inverse of [`skew`].
pub fn unskew(info: &ParamInfo, value: f32) -> f32 {
    match info.value_scale {
        ValueScale::Quadratic => value.sqrt(),
        ValueScale::Cubic => value.powf(1.0 / 3.0),
        ValueScale::Quartic => value.powf(1.0 / 4.0),
        ValueScale::Exponential if info.display_invert => (1.0 / value).log2(),
        ValueScale::Exponential => value.log2(),
        ValueScale::SquareRoot => value * value,
        ValueScale::Indexed | ValueScale::Linear => value,
    }
}

/// The displayed number for engine value `value`, without units.
pub fn display_number(info: &ParamInfo, value: f32) -> f32 {
    info.display_multiply * skew(info, value) + info.post_offset
}

/// The engine value whose displayed number is `number` (not clamped).
pub fn engine_from_display(info: &ParamInfo, number: f32) -> f32 {
    unskew(info, (number - info.post_offset) / info.display_multiply)
}

/// The option name of an indexed parameter at engine value `value`, if it
/// has a string table.
pub fn option_name(info: &ParamInfo, value: f32) -> Option<&'static str> {
    let options = info.options()?;
    options.get(value.min(info.max).max(0.0) as usize).copied()
}

/// `ValueBridge::getText`: the option name of an indexed parameter, else the
/// displayed number and units.
pub fn display_text(info: &ParamInfo, value: f32) -> String {
    if let Some(option) = option_name(info, value) {
        return option.to_string();
    }
    format!(
        "{}{}",
        format_number(display_number(info, value)),
        info.display_units
    )
    .trim()
    .to_string()
}

/// Up to four decimals, without trailing zeros.
pub fn format_number(value: f32) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_string()
    } else {
        text.to_string()
    }
}

/// Inverse of [`display_text`] (`ValueBridge::getValueForText`, which also
/// accepts the option names and removes the post offset). Not clamped: the
/// caller clamps to the parameter's range (D1 in `migration/DIVERGENCES.md`).
pub fn value_from_text(info: &ParamInfo, text: &str) -> Option<f32> {
    let text = text.trim();
    if let Some(options) = info.options() {
        if let Some(index) = options
            .iter()
            .position(|option| option.eq_ignore_ascii_case(text))
        {
            return Some(index as f32);
        }
    }
    let number_end = text
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0)))
        .map_or(text.len(), |(i, _)| i);
    let number: f32 = text[..number_end].parse().ok()?;
    let value = engine_from_display(info, number);
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry;

    #[test]
    fn display_text_matches_the_value_bridge() {
        let volume = registry().get("volume").unwrap();
        // sqrt(5473.0404) - 80 = -6.02 dB.
        assert_eq!(display_text(volume, volume.default_value), "-6.02dB");
        let delay_style = registry().get("delay_style").unwrap();
        assert_eq!(display_text(delay_style, 2.0), "Ping Pong");
        let cutoff = registry().get("filter_1_cutoff").unwrap();
        assert_eq!(display_text(cutoff, 60.0), "0 semitones");
        let attack = registry().get("env_1_attack").unwrap();
        assert_eq!(display_text(attack, 1.0), "1 secs");
    }

    #[test]
    fn text_input_inverts_the_display() {
        for (name, value) in [
            ("volume", 5473.0404f32),
            ("filter_1_cutoff", 72.0),
            ("lfo_1_frequency", 3.0),
            ("delay_frequency", 2.5),
            ("env_1_attack", 0.7),
            ("osc_1_level", 0.5),
        ] {
            let info = registry().get(name).unwrap();
            let parsed = value_from_text(info, &display_text(info, value)).unwrap();
            assert!(
                (parsed - value).abs() <= 1e-3 * value.abs().max(1.0),
                "{name}: {value} -> {} -> {parsed}",
                display_text(info, value)
            );
        }
        let delay_style = registry().get("delay_style").unwrap();
        assert_eq!(value_from_text(delay_style, "ping pong"), Some(2.0));
    }
}
