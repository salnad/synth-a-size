//! The tools a model calls: their JSON schemas and what they do.
//!
//! Every input is validated before anything changes, and a batch is applied
//! whole or not at all. Results are compact JSON (or plain text for
//! `get_patch`), and errors say how to fix the call.

use serde::Deserialize;
use serde_json::{json, Value};

use sas_preset::display::{engine_from_display, value_from_text};
use sas_preset::ValueScale;

use crate::catalog::{self, control, controls, range_text, value_view};
use crate::controller::Controller;
use crate::names::suggest;
use crate::patch;
use crate::session::{Edit, SynthSession};

/// A tool as offered to a model.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema of the arguments object.
    pub input_schema: Value,
}

/// A tool's result: text for the model, flagged when the call failed.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    fn ok(value: Value) -> ToolOutput {
        ToolOutput {
            content: value.to_string(),
            is_error: false,
        }
    }

    fn text(text: String) -> ToolOutput {
        ToolOutput {
            content: text,
            is_error: false,
        }
    }

    fn error(message: impl Into<String>) -> ToolOutput {
        ToolOutput {
            content: message.into(),
            is_error: true,
        }
    }
}

/// Every tool, in the order offered.
pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "get_patch",
            description: "Describe the current patch: modules that are on and their controls that \
                          differ from default, modules that are off, modulation routes, edited LFO \
                          shapes and wavetables. Values are in display units.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        ToolSpec {
            name: "find_params",
            description: "Search controls by words in their name (e.g. \"filter 1 cutoff\", \
                          \"reverb\"). Returns each match's range in display units, default and \
                          current value.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
                },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "get_params",
            description: "Current values of the named controls, in display units.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "names": {"type": "array", "items": {"type": "string"}, "minItems": 1}
                },
                "required": ["names"]
            }),
        },
        ToolSpec {
            name: "set_params",
            description: "Set controls, all or none. Each value is in display units: a number in \
                          the control's units (dB, semitones, secs, %...), a string with units \
                          (\"-12 dB\"), or an option name (\"Ping Pong\"). Values are clamped to \
                          the range and the reply gives the values that landed. Raising the master \
                          volume past a threshold waits for the user to confirm.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "changes": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "value": {"type": ["number", "string"]}
                            },
                            "required": ["name", "value"]
                        }
                    }
                },
                "required": ["changes"]
            }),
        },
        ToolSpec {
            name: "checkpoint",
            description: "Remember the current patch under a label, to return to with undo.",
            input_schema: json!({
                "type": "object",
                "properties": {"label": {"type": "string"}},
                "required": ["label"]
            }),
        },
        ToolSpec {
            name: "undo",
            description: "Undo the last edit steps (each of your turns is one step), or return \
                          to a checkpoint.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "steps": {"type": "integer", "minimum": 1, "default": 1},
                    "checkpoint": {"type": "string"}
                }
            }),
        },
    ]
}

/// The system-prompt section listing every control (templated families).
pub fn catalog() -> String {
    catalog::catalog_text()
}

fn parse<T: for<'de> Deserialize<'de>>(tool: &str, args: &Value) -> Result<T, ToolOutput> {
    serde_json::from_value(args.clone())
        .map_err(|error| ToolOutput::error(format!("invalid arguments for {tool}: {error}")))
}

/// The error for a name that is not a control, with suggestions.
fn unknown_control(name: &str) -> String {
    let names = controls().iter().map(|info| info.name.as_str());
    let suggestions = suggest(name, names, 3);
    if suggestions.is_empty() {
        format!("unknown control {name:?}; use find_params to search")
    } else {
        format!(
            "unknown control {name:?}; did you mean {}?",
            suggestions.join(", ")
        )
    }
}

pub(crate) fn call<S: SynthSession>(
    controller: &mut Controller<S>,
    name: &str,
    args: &Value,
) -> ToolOutput {
    let result = match name {
        "get_patch" => Ok(ToolOutput::text(patch::describe(
            &controller.session().snapshot(),
        ))),
        "find_params" => find_params(controller, args),
        "get_params" => get_params(controller, args),
        "set_params" => set_params(controller, args),
        "checkpoint" => checkpoint(controller, args),
        "undo" => undo(controller, args),
        _ => Err(ToolOutput::error(format!(
            "unknown tool {name:?}; tools: {}",
            specs()
                .iter()
                .map(|spec| spec.name)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    };
    result.unwrap_or_else(|error| error)
}

fn describe_control<S: SynthSession>(controller: &Controller<S>, name: &str) -> Option<Value> {
    let info = control(name)?;
    let value = controller
        .session()
        .control(name)
        .unwrap_or(info.default_value);
    Some(json!({
        "name": info.name,
        "range": range_text(info),
        "default": value_view(info, info.default_value),
        "value": value_view(info, value),
    }))
}

fn find_params<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        query: String,
        #[serde(default)]
        limit: Option<usize>,
    }
    let args: Args = parse("find_params", args)?;
    let limit = args.limit.unwrap_or(20).clamp(1, 100);
    let found: Vec<Value> = catalog::find(&args.query, limit)
        .iter()
        .filter_map(|info| describe_control(controller, &info.name))
        .collect();
    if found.is_empty() {
        return Err(ToolOutput::error(unknown_control(&args.query)));
    }
    Ok(ToolOutput::ok(Value::Array(found)))
}

fn get_params<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        names: Vec<String>,
    }
    let args: Args = parse("get_params", args)?;
    let mut values = serde_json::Map::new();
    for name in &args.names {
        let info = control(name).ok_or_else(|| ToolOutput::error(unknown_control(name)))?;
        let value = controller
            .session()
            .control(name)
            .unwrap_or(info.default_value);
        values.insert(name.clone(), Value::String(value_view(info, value)));
    }
    Ok(ToolOutput::ok(Value::Object(values)))
}

/// The engine value for a display-unit `value` of control `name`, clamped to
/// its range and rounded for stepped controls; and whether it was clamped.
fn engine_value(name: &str, value: &Value) -> Result<(&'static str, f32, bool), String> {
    let info = control(name).ok_or_else(|| unknown_control(name))?;
    let raw = match value {
        Value::Number(number) => number
            .as_f64()
            .map(|number| engine_from_display(info, number as f32))
            .filter(|value| value.is_finite()),
        Value::String(text) => value_from_text(info, text),
        _ => None,
    }
    .ok_or_else(|| format!("{name}: cannot read {value} (range {})", range_text(info)))?;
    let mut clamped = raw.clamp(info.min, info.max);
    if info.value_scale == ValueScale::Indexed {
        clamped = clamped.round();
    }
    let was_clamped = (clamped - raw).abs() > 1e-4 * raw.abs().max(1.0);
    Ok((info.name.as_str(), clamped, was_clamped))
}

fn set_params<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Change {
        name: String,
        value: Value,
    }
    #[derive(Deserialize)]
    struct Args {
        changes: Vec<Change>,
    }
    let args: Args = parse("set_params", args)?;
    let mut edits = Vec::new();
    let mut landed = Vec::new();
    let mut errors = Vec::new();
    for change in &args.changes {
        match engine_value(&change.name, &change.value) {
            Ok((name, value, clamped)) => {
                edits.push(Edit::SetControl { name, value });
                let info = control(name).expect("validated");
                let mut entry = json!({"name": name, "value": value_view(info, value)});
                if clamped {
                    entry["clamped"] = Value::Bool(true);
                }
                landed.push(entry);
            }
            Err(error) => errors.push(error),
        }
    }
    if !errors.is_empty() {
        return Err(ToolOutput::error(format!(
            "nothing was changed: {}",
            errors.join("; ")
        )));
    }
    match controller.apply(edits) {
        Ok(None) => Ok(ToolOutput::ok(json!({"set": landed}))),
        Ok(Some(reason)) => Ok(ToolOutput::ok(json!({
            "status": "needs_confirmation",
            "reason": reason,
            "note": "Nothing was changed yet. Ask the user; the change applies only if they confirm.",
        }))),
        Err(error) => Err(ToolOutput::error(error.to_string())),
    }
}

fn checkpoint<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        label: String,
    }
    let args: Args = parse("checkpoint", args)?;
    controller.push_snapshot(Some(args.label.clone()));
    Ok(ToolOutput::ok(json!({"checkpoint": args.label})))
}

fn undo<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        steps: Option<usize>,
        #[serde(default)]
        checkpoint: Option<String>,
    }
    let args: Args = parse("undo", args)?;
    let steps = args.steps.unwrap_or(1).max(1);
    match controller.undo(steps, args.checkpoint.as_deref()) {
        Ok(0) => Ok(ToolOutput::ok(
            json!({"undone": 0, "note": "nothing to undo"}),
        )),
        Ok(count) => Ok(ToolOutput::ok(json!({"undone": count}))),
        Err(error) => Err(ToolOutput::error(error.to_string())),
    }
}
