//! The tools a model calls: their JSON schemas and what they do.
//!
//! Every input is validated before anything changes, and a batch is applied
//! whole or not at all. Results are compact JSON (or plain text for
//! `get_patch`), and errors say how to fix the call.

use serde::Deserialize;
use serde_json::{json, Value};

use sas_preset::display::{engine_from_display, value_from_text};
use sas_preset::{engine, InitContent, PresetState, ValueScale};

use crate::catalog::{self, control, controls, range_text, value_view};
use crate::controller::{Controller, Outcome};
use crate::names::suggest;
use crate::patch;
use crate::session::{Applied, Edit, SynthSession};
use crate::shapes::{self, FrameSpec, LAST_FRAME, LFO_SHAPES, MAX_HARMONICS, WAVE_SHAPES};

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
        ToolSpec {
            name: "connect_modulation",
            description: "Route a modulation source (LFO, envelope, macro, velocity...) to a \
                          modulatable control, as dragging a source onto a knob does. Optional \
                          slot settings: amount (-1..1 of the control's range), bipolar, stereo, \
                          bypass, power (curve). A new route starts as dragging a source onto the \
                          knob does: unipolar unless you pass bipolar=true, amount from the \
                          knob's position (at least 0.25), stereo and bypass off. Connecting an \
                          existing pair updates it.",
            input_schema: modulation_schema(true),
        },
        ToolSpec {
            name: "update_modulation",
            description: "Change the settings of an existing modulation route.",
            input_schema: modulation_schema(false),
        },
        ToolSpec {
            name: "disconnect_modulation",
            description: "Remove a modulation route.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "destination": {"type": "string"}
                },
                "required": ["source", "destination"]
            }),
        },
        ToolSpec {
            name: "set_lfo_shape",
            description: "Set an LFO's shape: a named shape, or 2 to 100 points (x = phase 0..1, \
                          first at x = 0 and last at x = 1, x not decreasing; y = value 0 \
                          bottom..1 top) with optional per-point \
                          curve powers (-20..20, 0 straight). Rate, sync and mode are controls \
                          (lfo_N_*), set with set_params.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "lfo": {"type": "integer", "minimum": 1, "maximum": 8},
                    "shape": {"type": "string", "enum": LFO_SHAPES},
                    "points": {
                        "type": "array",
                        "items": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}
                    },
                    "powers": {"type": "array", "items": {"type": "number"}},
                    "smooth": {"type": "boolean", "default": false},
                    "name": {"type": "string"}
                },
                "required": ["lfo"]
            }),
        },
        ToolSpec {
            name: "set_wavetable",
            description: "Replace an oscillator's wavetable with generated keyframes. Each frame \
                          sits at a position 0..256 (osc_N_wave_frame scans them) and is a basic \
                          shape or a harmonic spectrum (amplitudes of harmonics 1, 2, 3...). \
                          Frames between keyframes morph spectrally.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "oscillator": {"type": "integer", "minimum": 1, "maximum": 3},
                    "name": {"type": "string"},
                    "frames": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 64,
                        "items": {
                            "type": "object",
                            "properties": {
                                "position": {"type": "integer", "minimum": 0, "maximum": LAST_FRAME},
                                "shape": {"type": "string", "enum": WAVE_SHAPES},
                                "harmonics": {"type": "array", "items": {"type": "number"}, "minItems": 1, "maxItems": MAX_HARMONICS}
                            },
                            "required": ["position"]
                        }
                    }
                },
                "required": ["oscillator", "frames"]
            }),
        },
        ToolSpec {
            name: "list_presets",
            description: "List presets in the user's preset folder, optionally filtered by name.",
            input_schema: json!({
                "type": "object",
                "properties": {"query": {"type": "string"}}
            }),
        },
        ToolSpec {
            name: "load_preset",
            description:
                "Load a preset from the user's preset folder (replaces the patch; undoable).",
            input_schema: json!({
                "type": "object",
                "properties": {"name": {"type": "string"}},
                "required": ["name"]
            }),
        },
        ToolSpec {
            name: "save_preset",
            description: "Save the current patch to the user's preset folder. An existing preset \
                          is only replaced with overwrite=true, after asking the user.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "author": {"type": "string"},
                    "comments": {"type": "string"},
                    "style": {"type": "string"},
                    "overwrite": {"type": "boolean", "default": false}
                },
                "required": ["name"]
            }),
        },
        ToolSpec {
            name: "set_preset_info",
            description: "Name the patch and describe it: name, author, comments, style, and \
                          labels for the 4 macros (shown on their knobs). Only the given fields \
                          change.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "author": {"type": "string"},
                    "comments": {"type": "string"},
                    "style": {"type": "string"},
                    "macro_names": {"type": "array", "items": {"type": "string"}, "maxItems": 4}
                }
            }),
        },
        ToolSpec {
            name: "init_patch",
            description: "Start over from the init patch (undoable).",
            input_schema: json!({"type": "object", "properties": {}}),
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
        "connect_modulation" => connect_modulation(controller, args),
        "update_modulation" => update_modulation(controller, args),
        "disconnect_modulation" => disconnect_modulation(controller, args),
        "set_lfo_shape" => set_lfo_shape(controller, args),
        "set_wavetable" => set_wavetable(controller, args),
        "list_presets" => list_presets(controller, args),
        "load_preset" => load_preset(controller, args),
        "save_preset" => save_preset(controller, args),
        "init_patch" => init_patch(controller),
        "set_preset_info" => set_preset_info(controller, args),
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
        "modulatable": engine::is_modulation_destination(name),
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
    apply(controller, edits).map(|_| ToolOutput::ok(json!({"set": landed})))
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

/// Applies `edits`, mapping the outcome to the replies the tools share.
fn apply<S: SynthSession>(
    controller: &mut Controller<S>,
    edits: Vec<Edit>,
) -> Result<Vec<Applied>, ToolOutput> {
    match controller.apply(edits) {
        Ok(Outcome::Applied(applied)) => Ok(applied),
        Ok(Outcome::Pending(reason)) => Err(ToolOutput::ok(json!({
            "status": "needs_confirmation",
            "reason": reason,
            "note": "Nothing was changed yet. Ask the user; the change applies only if they confirm.",
        }))),
        Err(error) => Err(ToolOutput::error(format!("nothing was changed: {error}"))),
    }
}

/// Modulation slot settings a model may give, with their `modulation_N_`
/// suffixes.
const SLOT_SETTINGS: &[&str] = &["amount", "bipolar", "stereo", "bypass", "power"];

fn modulation_schema(connect: bool) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "source": {"type": "string"},
            "destination": {"type": "string"},
            "amount": {"type": "number", "minimum": -1, "maximum": 1},
            "bipolar": {"type": "boolean"},
            "stereo": {"type": "boolean"},
            "bypass": {"type": "boolean"},
            "power": {"type": "number"}
        },
        "required": ["source", "destination"]
    });
    if connect {
        schema["properties"]["source"]["enum"] = json!(engine::modulation_sources());
    }
    schema
}

#[derive(Deserialize)]
struct ModulationArgs {
    source: String,
    destination: String,
    #[serde(default)]
    amount: Option<Value>,
    #[serde(default)]
    bipolar: Option<bool>,
    #[serde(default)]
    stereo: Option<bool>,
    #[serde(default)]
    bypass: Option<bool>,
    #[serde(default)]
    power: Option<Value>,
}

impl ModulationArgs {
    /// The given slot settings as (`modulation_N_` suffix, engine value),
    /// converted with slot 1's ranges (every slot has the same).
    fn settings(&self) -> Result<Vec<(&'static str, f32)>, String> {
        let flag = |value: bool| Value::from(if value { 1 } else { 0 });
        let given = [
            self.amount.clone(),
            self.bipolar.map(flag),
            self.stereo.map(flag),
            self.bypass.map(flag),
            self.power.clone(),
        ];
        let mut settings = Vec::new();
        for (suffix, value) in SLOT_SETTINGS.iter().zip(given) {
            if let Some(value) = value {
                let (_, engine, _) = engine_value(&format!("modulation_1_{suffix}"), &value)?;
                settings.push((*suffix, engine));
            }
        }
        Ok(settings)
    }

    fn check_names(&self) -> Result<(), String> {
        if !engine::is_modulation_source(&self.source) {
            let sources = engine::modulation_sources();
            let close = suggest(&self.source, sources.iter().map(String::as_str), 3);
            return Err(format!(
                "unknown modulation source {:?}; did you mean {}? Sources: {}",
                self.source,
                close.join(", "),
                sources.join(", ")
            ));
        }
        if !engine::is_modulation_destination(&self.destination) {
            let destinations = engine::modulation_destinations();
            let close = suggest(&self.destination, destinations.iter().copied(), 3);
            return Err(format!(
                "{:?} is not a modulatable control; did you mean {}?",
                self.destination,
                close.join(", ")
            ));
        }
        Ok(())
    }
}

/// The reply for a modulation route: its slot and settings.
fn route_reply<S: SynthSession>(controller: &Controller<S>, slot: usize, created: bool) -> Value {
    let mut reply = json!({"slot": slot + 1, "created": created});
    for suffix in SLOT_SETTINGS {
        let name = format!("modulation_{}_{suffix}", slot + 1);
        if let (Some(info), Some(value)) = (control(&name), controller.session().control(&name)) {
            reply[*suffix] = Value::String(value_view(info, value));
        }
    }
    reply
}

fn connect_modulation<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    let args: ModulationArgs = parse("connect_modulation", args)?;
    args.check_names().map_err(ToolOutput::error)?;
    let settings = args.settings().map_err(ToolOutput::error)?;
    let applied = apply(
        controller,
        vec![Edit::ConnectModulation {
            source: args.source,
            destination: args.destination,
            settings,
        }],
    )?;
    match applied.first() {
        Some(Applied::Connected { slot, created }) => {
            Ok(ToolOutput::ok(route_reply(controller, *slot, *created)))
        }
        _ => Ok(ToolOutput::ok(json!({"connected": true}))),
    }
}

fn update_modulation<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    let args: ModulationArgs = parse("update_modulation", args)?;
    let slot = controller
        .session()
        .snapshot()
        .modulation_index(&args.source, &args.destination)
        .ok_or_else(|| {
            ToolOutput::error(format!(
                "{} is not connected to {}; use connect_modulation",
                args.source, args.destination
            ))
        })?;
    let settings = args.settings().map_err(ToolOutput::error)?;
    if settings.is_empty() {
        return Err(ToolOutput::error("give at least one setting to change"));
    }
    let edits = settings
        .into_iter()
        .map(|(suffix, value)| {
            let name = control(&format!("modulation_{}_{suffix}", slot + 1)).expect("slot control");
            Edit::SetControl {
                name: name.name.as_str(),
                value,
            }
        })
        .collect();
    apply(controller, edits)?;
    Ok(ToolOutput::ok(route_reply(controller, slot, false)))
}

fn disconnect_modulation<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        source: String,
        destination: String,
    }
    let args: Args = parse("disconnect_modulation", args)?;
    apply(
        controller,
        vec![Edit::DisconnectModulation {
            source: args.source,
            destination: args.destination,
        }],
    )?;
    Ok(ToolOutput::ok(json!({"disconnected": true})))
}

fn set_lfo_shape<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        lfo: usize,
        #[serde(default)]
        shape: Option<String>,
        #[serde(default)]
        points: Option<Vec<(f32, f32)>>,
        #[serde(default)]
        powers: Option<Vec<f32>>,
        #[serde(default)]
        smooth: bool,
        #[serde(default)]
        name: Option<String>,
    }
    let args: Args = parse("set_lfo_shape", args)?;
    if !(1..=8).contains(&args.lfo) {
        return Err(ToolOutput::error("lfo must be 1..8"));
    }
    let line = match (&args.shape, &args.points) {
        (Some(shape), None) => shapes::named_lfo(shape),
        (None, Some(points)) => shapes::lfo_from_points(
            points,
            args.powers.as_deref(),
            args.smooth,
            args.name.as_deref().unwrap_or("Custom"),
        ),
        _ => Err("give either a shape or points".to_string()),
    }
    .map_err(ToolOutput::error)?;
    let points = line.num_points();
    apply(
        controller,
        vec![Edit::SetLfoShape {
            lfo: args.lfo - 1,
            line: Box::new(line),
        }],
    )?;
    Ok(ToolOutput::ok(json!({"lfo": args.lfo, "points": points})))
}

fn set_wavetable<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        oscillator: usize,
        #[serde(default)]
        name: Option<String>,
        frames: Vec<FrameSpec>,
    }
    let args: Args = parse("set_wavetable", args)?;
    if !(1..=3).contains(&args.oscillator) {
        return Err(ToolOutput::error("oscillator must be 1..3"));
    }
    let name = args.name.unwrap_or_else(|| "Generated".to_string());
    let creator = shapes::wavetable(&name, &args.frames).map_err(ToolOutput::error)?;
    let frames = creator.last_keyframe_position();
    apply(
        controller,
        vec![Edit::SetWavetable {
            oscillator: args.oscillator - 1,
            creator: Box::new(creator),
        }],
    )?;
    Ok(ToolOutput::ok(json!({
        "oscillator": args.oscillator,
        "keyframes": args.frames.len(),
        "last_frame": frames,
    })))
}

fn library<S: SynthSession>(
    controller: &Controller<S>,
) -> Result<crate::library::PresetLibrary, ToolOutput> {
    controller
        .library()
        .cloned()
        .ok_or_else(|| ToolOutput::error("no preset folder is configured"))
}

fn list_presets<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        query: String,
    }
    let args: Args = parse("list_presets", args)?;
    let names = library(controller)?
        .list(&args.query, 50)
        .map_err(ToolOutput::error)?;
    Ok(ToolOutput::ok(json!({"presets": names})))
}

fn load_preset<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        name: String,
    }
    let args: Args = parse("load_preset", args)?;
    let state = library(controller)?
        .load(&args.name, &controller.session().snapshot())
        .map_err(ToolOutput::error)?;
    apply(controller, vec![Edit::LoadPreset(Box::new(state))])?;
    Ok(ToolOutput::ok(json!({"loaded": args.name})))
}

fn save_preset<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        name: String,
        #[serde(default)]
        author: Option<String>,
        #[serde(default)]
        comments: Option<String>,
        #[serde(default)]
        style: Option<String>,
        #[serde(default)]
        overwrite: bool,
    }
    let args: Args = parse("save_preset", args)?;
    let library = library(controller)?;
    let mut state = controller.session().snapshot();
    state.info.preset_name = args.name.clone();
    if let Some(author) = args.author {
        state.info.author = author;
    }
    if let Some(comments) = args.comments {
        state.info.comments = comments;
    }
    if let Some(style) = args.style {
        state.info.style = style;
    }
    library
        .save(&args.name, &state, args.overwrite)
        .map_err(ToolOutput::error)?;
    Ok(ToolOutput::ok(json!({"saved": args.name})))
}

fn init_patch<S: SynthSession>(controller: &mut Controller<S>) -> Result<ToolOutput, ToolOutput> {
    let state = PresetState::new(&InitContent { sample_seed: 0 });
    apply(controller, vec![Edit::LoadPreset(Box::new(state))])?;
    Ok(ToolOutput::ok(json!({"init": true})))
}

fn set_preset_info<S: SynthSession>(
    controller: &mut Controller<S>,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct Args {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        author: Option<String>,
        #[serde(default)]
        comments: Option<String>,
        #[serde(default)]
        style: Option<String>,
        #[serde(default)]
        macro_names: Option<Vec<String>>,
    }
    let args: Args = parse("set_preset_info", args)?;
    let mut info = controller.session().snapshot().info;
    let macros = args.macro_names.unwrap_or_default();
    if macros.len() > info.macros.len() {
        return Err(ToolOutput::error(format!(
            "there are {} macros",
            info.macros.len()
        )));
    }
    let given = [&args.name, &args.author, &args.comments, &args.style];
    if given.iter().all(|field| field.is_none()) && macros.is_empty() {
        return Err(ToolOutput::error("give at least one field to change"));
    }
    if let Some(name) = args.name {
        info.preset_name = name;
    }
    if let Some(author) = args.author {
        info.author = author;
    }
    if let Some(comments) = args.comments {
        info.comments = comments;
    }
    if let Some(style) = args.style {
        info.style = style;
    }
    for (slot, name) in info.macros.iter_mut().zip(macros) {
        *slot = name;
    }
    let reply = json!({
        "name": info.preset_name,
        "style": info.style,
        "macros": (0..info.macros.len()).map(|i| info.macro_name(i)).collect::<Vec<_>>(),
    });
    apply(controller, vec![Edit::SetInfo(Box::new(info))])?;
    Ok(ToolOutput::ok(reply))
}
