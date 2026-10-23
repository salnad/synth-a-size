//! The controls a model can read and set, and how they are described to it.
//!
//! A control is any registry parameter the engine reads
//! (`sas_preset::engine::is_control`): the 772 host parameters. Values are
//! shown and accepted in display units, the way the host's text field shows
//! them (`sas_preset::display`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

use sas_preset::display::{display_text, option_name};
use sas_preset::{engine, registry, ParamInfo, ValueScale};

/// Prefixes of numbered modules (`osc_1_level`, `lfo_8_frequency`, ...).
const NUMBERED: &[&str] = &[
    "osc_",
    "env_",
    "lfo_",
    "random_",
    "filter_",
    "macro_control_",
    "modulation_",
];

/// Unnumbered modules, matched by name prefix.
const MODULES: &[&str] = &[
    "filter_fx",
    "chorus",
    "compressor",
    "delay",
    "distortion",
    "eq",
    "flanger",
    "phaser",
    "reverb",
    "sample",
];

/// Every control, in registry order.
pub fn controls() -> &'static [&'static ParamInfo] {
    static CONTROLS: OnceLock<Vec<&'static ParamInfo>> = OnceLock::new();
    CONTROLS.get_or_init(|| {
        registry()
            .parameters()
            .iter()
            .filter(|info| engine::is_control(&info.name))
            .collect()
    })
}

/// The control named `name`.
pub fn control(name: &str) -> Option<&'static ParamInfo> {
    registry()
        .get(name)
        .filter(|info| engine::is_control(&info.name))
}

/// Splits a numbered name into its template and number:
/// `osc_2_level` → (`osc_N_level`, 2). Other names come back unchanged.
pub fn template_of(name: &str) -> (String, Option<u32>) {
    for prefix in NUMBERED {
        if let Some(rest) = name.strip_prefix(prefix) {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(number) = digits.parse() {
                return (format!("{prefix}N{}", &rest[digits.len()..]), Some(number));
            }
        }
    }
    (name.to_string(), None)
}

/// The module a control belongs to: `osc_2`, `filter_fx`, `reverb`, or
/// `global`.
pub fn module_of(name: &str) -> String {
    if let (_, Some(number)) = template_of(name) {
        let prefix = NUMBERED
            .iter()
            .find(|prefix| name.starts_with(*prefix))
            .unwrap();
        return format!("{prefix}{number}");
    }
    MODULES
        .iter()
        .find(|module| {
            name.strip_prefix(*module)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('_'))
        })
        .map_or_else(|| "global".to_string(), |module| module.to_string())
}

/// The range a model may set, in display units: the option names of an
/// indexed control with a string table, else `min..max`.
pub fn range_text(info: &ParamInfo) -> String {
    if let Some(options) = info.options() {
        let count = (info.max - info.min).round() as usize + 1;
        return options
            .iter()
            .take(count)
            .copied()
            .collect::<Vec<_>>()
            .join("/");
    }
    let low = display_text(info, info.min);
    let high = display_text(info, info.max);
    if info.value_scale == ValueScale::Indexed {
        format!("{low}..{high} (whole steps)")
    } else {
        format!("{low}..{high}")
    }
}

/// One line per control for the system prompt, with each numbered family
/// written once as a template (`osc_N_level (N=1..3)`). About 2.2k tokens.
pub fn catalog_text() -> String {
    let mut families: BTreeMap<String, (Vec<u32>, &ParamInfo)> = BTreeMap::new();
    let mut order = Vec::new();
    for info in controls() {
        let (template, number) = template_of(&info.name);
        let family = families.entry(template.clone()).or_insert_with(|| {
            order.push(template.clone());
            (Vec::new(), *info)
        });
        family.0.extend(number);
    }
    let mut text = String::from("name | range (display units) | default\n");
    for template in order {
        let (numbers, info) = &families[&template];
        let numbering = match (numbers.iter().min(), numbers.iter().max()) {
            (Some(low), Some(high)) => format!(" (N={low}..{high})"),
            _ => String::new(),
        };
        let _ = writeln!(
            text,
            "{template}{numbering} | {} | {}",
            range_text(info),
            display_text(info, info.default_value)
        );
    }
    text
}

/// A control's value as a model sees it: display text, plus the option name
/// for indexed controls.
pub fn value_view(info: &ParamInfo, value: f32) -> String {
    match option_name(info, value) {
        Some(option) => option.to_string(),
        None => display_text(info, value),
    }
}

/// Controls whose name or display name matches every word of `query`, in
/// registry order, at most `limit`.
pub fn find(query: &str, limit: usize) -> Vec<&'static ParamInfo> {
    let words: Vec<String> = query
        .split(|c: char| c.is_whitespace() || c == '_')
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    controls()
        .iter()
        .filter(|info| {
            let haystack = format!("{} {}", info.name, info.display_name).to_ascii_lowercase();
            words.iter().all(|word| haystack.contains(word.as_str()))
        })
        .take(limit)
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_host_parameter_is_a_control() {
        assert_eq!(controls().len(), 772);
    }

    #[test]
    fn templates_and_modules() {
        assert_eq!(
            template_of("osc_2_level"),
            ("osc_N_level".to_string(), Some(2))
        );
        assert_eq!(
            template_of("filter_fx_cutoff"),
            ("filter_fx_cutoff".to_string(), None)
        );
        assert_eq!(
            template_of("modulation_64_amount"),
            ("modulation_N_amount".to_string(), Some(64))
        );
        assert_eq!(module_of("osc_2_level"), "osc_2");
        assert_eq!(module_of("filter_fx_cutoff"), "filter_fx");
        assert_eq!(module_of("reverb_size"), "reverb");
        assert_eq!(module_of("volume"), "global");
        assert_eq!(module_of("delay_style"), "delay");
    }

    #[test]
    fn catalog_lists_each_family_once() {
        let text = catalog_text();
        assert!(text.contains("osc_N_level (N=1..3) |"));
        assert!(text.contains("modulation_N_amount (N=1..64) |"));
        assert!(text.contains("\nvolume | -80dB..6.02dB | -6.02dB\n"));
        assert!(!text.contains("osc_1_level"));
        // The token budget in docs: about 2.2k tokens at ~4 bytes per token.
        assert!(text.len() < 12_000, "catalog is {} bytes", text.len());
    }

    #[test]
    fn options_are_listed_by_name() {
        let style = control("delay_style").unwrap();
        assert!(range_text(style).contains("Ping Pong"));
        assert_eq!(value_view(style, 2.0), "Ping Pong");
    }

    #[test]
    fn find_matches_every_word() {
        let found: Vec<_> = find("filter 1 cutoff", 10)
            .iter()
            .map(|i| i.name.as_str())
            .collect();
        assert!(found.contains(&"filter_1_cutoff"));
        assert!(found.iter().all(|name| name.contains("cutoff")));
        assert_eq!(find("reverb", 3).len(), 3);
    }
}
