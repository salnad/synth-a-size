//! The compact patch description `get_patch` returns: what a model needs to
//! reason about the sound, in a few hundred tokens. The preset JSON itself
//! (about 174 KB, mostly wave data) is never sent.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

use sas_preset::{InitContent, PresetState};

use crate::catalog::{control, controls, module_of, template_of, value_view};

/// Controls differing from their default by more than this are listed.
const EPSILON: f32 = 1e-6;

/// LFO points listed before the rest are elided.
const MAX_LFO_POINTS: usize = 16;

/// JSON of the init state's LFO shape and wavetable, to tell edited ones apart.
fn init_shapes() -> &'static (String, String) {
    static SHAPES: OnceLock<(String, String)> = OnceLock::new();
    SHAPES.get_or_init(|| {
        let init = PresetState::new(&InitContent { sample_seed: 0 });
        (
            init.lfos[0].to_json().dump(),
            init.wavetables[0].to_json().dump(),
        )
    })
}

fn is_default(name: &str, value: f32) -> bool {
    control(name).is_some_and(|info| (value - info.default_value).abs() <= EPSILON)
}

/// Describes `state` for the model.
pub fn describe(state: &PresetState) -> String {
    let mut text = String::new();
    let info = &state.info;
    if !info.preset_name.is_empty() || !info.style.is_empty() {
        let _ = writeln!(
            text,
            "Preset: {:?} style {:?}",
            info.preset_name, info.style
        );
    }

    // Non-default controls by module, in registry order; modules with an
    // `_on` switch that is off are only named.
    let mut modules: Vec<String> = Vec::new();
    let mut edited: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for param in controls() {
        let name = param.name.as_str();
        let module = module_of(name);
        if module.starts_with("modulation_") {
            continue;
        }
        if !modules.contains(&module) {
            modules.push(module.clone());
        }
        let value = state.value(name).unwrap_or(param.default_value);
        if name != format!("{module}_on") && !is_default(name, value) {
            let short = name.strip_prefix(&format!("{module}_")).unwrap_or(name);
            edited
                .entry(module)
                .or_default()
                .push(format!("{short}={}", value_view(param, value)));
        }
    }
    let mut off = Vec::new();
    for module in &modules {
        let on = state
            .value(&format!("{module}_on"))
            .map(|value| value != 0.0);
        if on == Some(false) {
            off.push(module.as_str());
            continue;
        }
        match edited.get(module) {
            Some(values) => {
                let _ = writeln!(text, "{module}: {}", values.join(", "));
            }
            None if on == Some(true) => {
                let _ = writeln!(text, "{module}: on, defaults");
            }
            None => {}
        }
    }
    if !off.is_empty() {
        let _ = writeln!(text, "Off: {}", off.join(", "));
    }

    let routes = describe_modulations(state);
    if routes.is_empty() {
        text.push_str("Modulations: none\n");
    } else {
        text.push_str("Modulations:\n");
        text.push_str(&routes);
    }

    let (init_lfo, init_wavetable) = init_shapes();
    for (index, lfo) in state.lfos.iter().enumerate() {
        if lfo.to_json().dump() == *init_lfo {
            continue;
        }
        let points: Vec<String> = (0..lfo.num_points().min(MAX_LFO_POINTS))
            .map(|i| {
                let (x, y) = lfo.point(i);
                format!("({x:.2},{y:.2})")
            })
            .collect();
        let more = if lfo.num_points() > MAX_LFO_POINTS {
            " ..."
        } else {
            ""
        };
        let _ = writeln!(
            text,
            "lfo_{} shape {:?}{}: {}{more}",
            index + 1,
            lfo.name(),
            if lfo.smooth() { " smooth" } else { "" },
            points.join(" ")
        );
    }
    for (index, wavetable) in state.wavetables.iter().enumerate() {
        if wavetable.to_json().dump() != *init_wavetable {
            let _ = writeln!(
                text,
                "osc_{} wavetable {:?} (edited)",
                index + 1,
                wavetable.name
            );
        }
    }
    text
}

/// One line per used modulation slot: source, destination, amount and the
/// slot's switches that are not at their defaults.
fn describe_modulations(state: &PresetState) -> String {
    let mut text = String::new();
    for (index, slot) in state.modulations().iter().enumerate() {
        if slot.is_free() {
            continue;
        }
        let number = index + 1;
        let mut extras = Vec::new();
        for param in controls() {
            if template_of(&param.name).1 != Some(number as u32)
                || !param.name.starts_with("modulation_")
            {
                continue;
            }
            let value = state.value(&param.name).unwrap_or(param.default_value);
            let short = param
                .name
                .rsplit_once(&format!("{number}_"))
                .map_or("", |(_, s)| s);
            if short == "amount" || !is_default(&param.name, value) || short == "bipolar" {
                extras.push(format!("{short}={}", value_view(param, value)));
            }
        }
        let curve = if slot.line_mapping.is_linear() {
            ""
        } else {
            " curved"
        };
        let _ = writeln!(
            text,
            "  {number}: {} -> {} {}{curve}",
            slot.source,
            slot.destination,
            extras.join(" ")
        );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() -> PresetState {
        PresetState::new(&InitContent { sample_seed: 0 })
    }

    #[test]
    fn the_init_patch_is_short() {
        let text = describe(&init());
        assert!(text.contains("Modulations: none"), "{text}");
        assert!(text.contains("Off: "), "{text}");
        assert!(!text.contains("lfo_1 shape"), "{text}");
        assert!(!text.contains("wavetable"), "{text}");
        assert!(text.len() < 1200, "{} bytes:\n{text}", text.len());
    }

    #[test]
    fn edits_and_routes_are_listed() {
        let mut state = init();
        state.set_value("osc_1_unison_voices", 4.0).unwrap();
        let slot = state.add_modulation("lfo_1", "filter_1_cutoff").unwrap();
        state
            .set_value(&format!("modulation_{}_amount", slot + 1), 0.5)
            .unwrap();
        state.lfos[0].init_saw_up();
        let text = describe(&state);
        assert!(
            text.contains("osc_1: ") && text.contains("unison_voices="),
            "{text}"
        );
        assert!(
            text.contains("1: lfo_1 -> filter_1_cutoff amount=0.5"),
            "{text}"
        );
        assert!(text.contains("lfo_1 shape"), "{text}");
    }
}
