//! Sound-design evals: prompts with checks on the patch the model makes.
//! They need a real model, so they run by hand (`sas-llm --eval`), never in
//! CI; the checks themselves are tested here.

use serde::Deserialize;

use sas_llm_control::catalog::control;
use sas_llm_control::sas_preset::display::display_number;
use sas_llm_control::sas_preset::PresetState;

/// One eval: a prompt on the init patch, and what the result must satisfy.
#[derive(Clone, Debug, Deserialize)]
pub struct EvalCase {
    pub name: String,
    pub prompt: String,
    pub checks: Vec<Check>,
}

/// A condition on the resulting patch. Control bounds are display numbers.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Check {
    Control {
        control: String,
        #[serde(default)]
        min: Option<f32>,
        #[serde(default)]
        max: Option<f32>,
    },
    /// A modulation route from `route[0]` to `route[1]`.
    Route { route: (String, String) },
    /// A route from this source to any destination.
    RouteFrom { route_from: String },
    /// LFO `lfo_shape` (1-based) is no longer the init triangle.
    LfoShape { lfo_shape: usize },
    /// Oscillator `wavetable` (1-based) has a new wavetable.
    Wavetable { wavetable: usize },
}

/// Whether `state` satisfies `check`, compared with the init patch `init`.
pub fn check(state: &PresetState, init: &PresetState, check: &Check) -> Result<(), String> {
    match check {
        Check::Control {
            control: name,
            min,
            max,
        } => {
            let info = control(name).ok_or_else(|| format!("unknown control {name}"))?;
            let value = display_number(info, state.value(name).unwrap_or(info.default_value));
            if min.is_some_and(|min| value < min) || max.is_some_and(|max| value > max) {
                return Err(format!("{name} is {value}, wanted {min:?}..{max:?}"));
            }
            Ok(())
        }
        Check::Route {
            route: (source, destination),
        } => state
            .modulation_index(source, destination)
            .map(|_| ())
            .ok_or_else(|| format!("no route {source} -> {destination}")),
        Check::RouteFrom { route_from } => state
            .modulations()
            .iter()
            .any(|slot| slot.source == *route_from)
            .then_some(())
            .ok_or_else(|| format!("no route from {route_from}")),
        Check::LfoShape { lfo_shape } => {
            let index = lfo_shape.checked_sub(1).ok_or("LFOs are numbered from 1")?;
            let (now, before) = (state.lfos.get(index), init.lfos.get(index));
            match (now, before) {
                (Some(now), Some(before)) if now.to_json().dump() != before.to_json().dump() => {
                    Ok(())
                }
                (Some(_), Some(_)) => Err(format!("lfo_{lfo_shape} shape unchanged")),
                _ => Err(format!("no lfo_{lfo_shape}")),
            }
        }
        Check::Wavetable { wavetable } => {
            let index = wavetable
                .checked_sub(1)
                .ok_or("oscillators are numbered from 1")?;
            let (now, before) = (state.wavetables.get(index), init.wavetables.get(index));
            match (now, before) {
                (Some(now), Some(before)) if now.to_json().dump() != before.to_json().dump() => {
                    Ok(())
                }
                (Some(_), Some(_)) => Err(format!("osc_{wavetable} wavetable unchanged")),
                _ => Err(format!("no osc_{wavetable}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sas_llm_control::sas_preset::InitContent;

    fn init() -> PresetState {
        PresetState::new(&InitContent { sample_seed: 0 })
    }

    #[test]
    fn checks_read_display_values_and_routes() {
        let base = init();
        let mut state = init();
        state.set_value("env_1_attack", 1.0).unwrap();
        state.add_modulation("lfo_2", "osc_1_level").unwrap();
        state.lfos[3].init_square();
        let parse = |text: &str| -> Check { serde_json::from_str(text).unwrap() };
        assert!(check(
            &state,
            &base,
            &parse(r#"{"control": "env_1_attack", "min": 0.5}"#)
        )
        .is_ok());
        assert!(check(
            &state,
            &base,
            &parse(r#"{"control": "env_1_attack", "max": 0.5}"#)
        )
        .is_err());
        assert!(check(
            &state,
            &base,
            &parse(r#"{"route": ["lfo_2", "osc_1_level"]}"#)
        )
        .is_ok());
        assert!(check(&state, &base, &parse(r#"{"route_from": "lfo_2"}"#)).is_ok());
        assert!(check(&state, &base, &parse(r#"{"route_from": "env_3"}"#)).is_err());
        assert!(check(&state, &base, &parse(r#"{"lfo_shape": 4}"#)).is_ok());
        assert!(check(&state, &base, &parse(r#"{"lfo_shape": 1}"#)).is_err());
        assert!(check(&state, &base, &parse(r#"{"wavetable": 1}"#)).is_err());
    }

    #[test]
    fn the_eval_set_parses_and_names_real_controls() {
        let text = include_str!("../evals/sound_design.json");
        let cases: Vec<EvalCase> = serde_json::from_str(text).unwrap();
        assert!(cases.len() >= 20, "{} evals", cases.len());
        for case in &cases {
            for item in &case.checks {
                if let Check::Control { control: name, .. } = item {
                    assert!(
                        control(name).is_some(),
                        "{}: unknown control {name}",
                        case.name
                    );
                }
            }
        }
    }
}
