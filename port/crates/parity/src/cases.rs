//! Resolution of corpus cases from `migration/corpus`.
//!
//! This mirrors `migration/scripts/run_corpus.py` exactly (layering and
//! tolerance rules). Captures carry the resolution `run_corpus.py` used
//! (`case.json`); `crate::captures` refuses to compare against a capture
//! whose resolution differs from this one, so the two cannot drift silently.

use std::fs;
use std::path::{Path, PathBuf};

use audio_diff::Tolerances;
use serde_json::{Map, Value};

const METRICS: [&str; 4] = ["max_abs_error", "rms_diff_db", "residual_db", "lsd_db"];
const SPEC_KEYS: [&str; 8] = [
    "module",
    "params",
    "sample_rate",
    "block_size",
    "oversample",
    "num_samples",
    "seed",
    "inputs",
];
const CASE_EXTRA_KEYS: [&str; 3] = ["description", "tolerances", "tolerance_exception"];

/// A resolved case, as `run_corpus.py` writes it to `case.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCase {
    pub id: String,
    /// The full resolved object (compared against the capture's `case.json`).
    pub json: Value,
    pub tolerances: Tolerances,
}

pub fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    root.canonicalize().unwrap_or(root)
}

pub fn corpus_dir() -> PathBuf {
    repo_root().join("migration/corpus")
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: invalid JSON: {e}", path.display()))
}

fn as_object<'a>(value: &'a Value, what: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{what} must be a JSON object"))
}

/// Applies the tolerance layers on top of the defaults and enforces that
/// nothing gets looser than the defaults without a `tolerance_exception`.
pub fn resolve_tolerances(
    location: &str,
    defaults: &Map<String, Value>,
    layers: &[&Map<String, Value>],
) -> Result<(Map<String, Value>, Option<String>), String> {
    let mut tolerances = Map::new();
    for (key, value) in defaults {
        let number = value
            .as_f64()
            .ok_or_else(|| format!("{location}: default tolerance {key} must be a number"))?;
        tolerances.insert(key.clone(), Value::from(number));
    }
    let mut exception = None;
    for layer in layers {
        if let Some(layer_tolerances) = layer.get("tolerances") {
            for (key, value) in as_object(layer_tolerances, "tolerances")? {
                if !METRICS.contains(&key.as_str()) {
                    return Err(format!("{location}: unknown tolerance metric {key:?}"));
                }
                let number = match value {
                    Value::Number(n) => n.as_f64().unwrap(),
                    _ => return Err(format!("{location}: tolerance {key} must be a number")),
                };
                tolerances.insert(key.clone(), Value::from(number));
            }
        }
        if let Some(Value::String(text)) = layer.get("tolerance_exception") {
            if !text.trim().is_empty() {
                exception = Some(text.clone());
            }
        }
    }
    let looser: Vec<&String> = defaults
        .iter()
        .filter(|(key, limit)| tolerances[key.as_str()].as_f64() > limit.as_f64())
        .map(|(key, _)| key)
        .collect();
    if !looser.is_empty() && exception.is_none() {
        return Err(format!(
            "{location}: tolerances {looser:?} are looser than the defaults; that needs a \
             tolerance_exception citing recorded human approval (migration/AGENTS.md rule 6)"
        ));
    }
    Ok((tolerances, exception))
}

fn to_tolerances(map: &Map<String, Value>) -> Tolerances {
    let get = |key: &str| map.get(key).and_then(Value::as_f64);
    Tolerances {
        max_abs_error: get("max_abs_error"),
        rms_diff_db: get("rms_diff_db"),
        residual_db: get("residual_db"),
        lsd_db: get("lsd_db"),
    }
}

/// Resolves all render cases from `cases.json`.
pub fn render_cases_from(cases_json: &Value) -> Result<Vec<ResolvedCase>, String> {
    let root = as_object(cases_json, "cases.json")?;
    let defaults = as_object(
        root.get("defaults").ok_or("cases.json: no defaults")?,
        "defaults",
    )?;
    let default_tolerances = as_object(
        defaults
            .get("tolerances")
            .ok_or("defaults: no tolerances")?,
        "defaults.tolerances",
    )?;
    let renders = root
        .get("renders")
        .and_then(Value::as_array)
        .ok_or("cases.json: no renders array")?;

    let mut resolved = Vec::new();
    for case in renders {
        let case = as_object(case, "render case")?;
        let name = case
            .get("name")
            .and_then(Value::as_str)
            .ok_or("render case without a name")?;
        let id = format!("render/{name}");
        let mut merged: Map<String, Value> = defaults
            .iter()
            .filter(|(k, _)| k.as_str() != "tolerances")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (key, value) in case {
            if key != "tolerances" && key != "tolerance_exception" {
                merged.insert(key.clone(), value.clone());
            }
        }
        let (tolerances, exception) = resolve_tolerances(&id, default_tolerances, &[case])?;
        let typed = to_tolerances(&tolerances);
        merged.insert("tolerances".into(), Value::Object(tolerances));
        if let Some(exception) = exception {
            merged.insert("tolerance_exception".into(), Value::String(exception));
        }
        merged.insert("id".into(), Value::String(id.clone()));
        resolved.push(ResolvedCase {
            id,
            json: Value::Object(merged),
            tolerances: typed,
        });
    }
    Ok(resolved)
}

/// Resolves the cases of one module case file. `area`/`module` come from
/// its path (`modules/<area>/<module>.json`), `relative_file` is the path
/// relative to the repository root.
pub fn module_cases_from(
    area: &str,
    module: &str,
    relative_file: &str,
    file: &Value,
    module_defaults: &Map<String, Value>,
) -> Result<Vec<ResolvedCase>, String> {
    let data = as_object(file, relative_file)?;
    let case_keys: Vec<&str> = SPEC_KEYS
        .iter()
        .chain(CASE_EXTRA_KEYS.iter())
        .copied()
        .collect();
    for key in data.keys() {
        if key != "cases" && !case_keys.contains(&key.as_str()) {
            return Err(format!("{relative_file}: unknown key {key:?}"));
        }
    }
    let cases = data
        .get("cases")
        .and_then(Value::as_object)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| format!("{relative_file}: needs a non-empty \"cases\" object"))?;
    let file_level: Map<String, Value> = data
        .iter()
        .filter(|(k, _)| k.as_str() != "cases")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let default_tolerances = as_object(
        module_defaults
            .get("tolerances")
            .ok_or("module_defaults: no tolerances")?,
        "module_defaults.tolerances",
    )?;

    let mut resolved = Vec::new();
    for (name, case) in cases {
        let id = format!("{area}/{module}/{name}");
        let case = as_object(case, &id)?;
        for key in case.keys() {
            if !case_keys.contains(&key.as_str()) {
                return Err(format!("{id}: unknown key {key:?}"));
            }
        }
        let mut spec = Map::new();
        for layer in [module_defaults, &file_level, case] {
            for key in SPEC_KEYS {
                if let Some(value) = layer.get(key) {
                    spec.insert(key.to_string(), value.clone());
                }
            }
        }
        if !spec.contains_key("module") {
            return Err(format!("{id}: no \"module\" (ref-module registry name)"));
        }
        if !spec.contains_key("num_samples") {
            return Err(format!("{id}: no \"num_samples\""));
        }
        let (tolerances, exception) =
            resolve_tolerances(&id, default_tolerances, &[&file_level, case])?;
        let typed = to_tolerances(&tolerances);
        let mut entry = Map::new();
        entry.insert("id".into(), Value::String(id.clone()));
        entry.insert("file".into(), Value::String(relative_file.to_string()));
        entry.insert("spec".into(), Value::Object(spec));
        entry.insert("tolerances".into(), Value::Object(tolerances));
        if let Some(exception) = exception {
            entry.insert("tolerance_exception".into(), Value::String(exception));
        }
        resolved.push(ResolvedCase {
            id,
            json: Value::Object(entry),
            tolerances: typed,
        });
    }
    Ok(resolved)
}

pub fn load_render_cases() -> Result<Vec<ResolvedCase>, String> {
    render_cases_from(&read_json(&corpus_dir().join("cases.json"))?)
}

pub fn load_module_cases() -> Result<Vec<ResolvedCase>, String> {
    let cases_json = read_json(&corpus_dir().join("cases.json"))?;
    let module_defaults = cases_json
        .get("module_defaults")
        .and_then(Value::as_object)
        .ok_or("cases.json: no module_defaults")?
        .clone();

    let modules_dir = corpus_dir().join("modules");
    let mut files = Vec::new();
    for area in fs::read_dir(&modules_dir).map_err(|e| format!("{}: {e}", modules_dir.display()))? {
        let area = area.map_err(|e| e.to_string())?.path();
        if !area.is_dir() {
            continue;
        }
        for file in fs::read_dir(&area).map_err(|e| e.to_string())? {
            let file = file.map_err(|e| e.to_string())?.path();
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            if name.ends_with(".json") && !name.ends_with(".hashes.json") {
                files.push(file);
            }
        }
    }
    files.sort();

    let root = repo_root();
    let mut resolved = Vec::new();
    for file in files {
        let area = file
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let module = file.file_stem().unwrap().to_string_lossy().to_string();
        let relative = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .to_string_lossy()
            .to_string();
        resolved.extend(module_cases_from(
            &area,
            &module,
            &relative,
            &read_json(&file)?,
            &module_defaults,
        )?);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn defaults() -> Map<String, Value> {
        json!({
            "sample_rate": 44100, "block_size": 64, "oversample": 1,
            "comment": "ignored",
            "tolerances": { "max_abs_error": 1e-4, "residual_db": -80.0 }
        })
        .as_object()
        .unwrap()
        .clone()
    }

    fn resolve(file: Value) -> Result<Vec<ResolvedCase>, String> {
        module_cases_from("filters", "test", "x.json", &file, &defaults())
    }

    #[test]
    fn layers_spec_and_inherits_default_tolerances() {
        let cases = resolve(json!({
            "module": "filter", "params": { "model": "digital" }, "block_size": 32,
            "cases": { "a": { "num_samples": 100, "block_size": 16 }, "b": { "num_samples": 50 } }
        }))
        .unwrap();
        assert_eq!(cases[0].id, "filters/test/a");
        assert_eq!(cases[0].json["spec"]["block_size"], 16);
        assert_eq!(cases[1].json["spec"]["block_size"], 32);
        assert_eq!(cases[1].json["spec"]["sample_rate"], 44100);
        assert!(cases[1].json["spec"].get("comment").is_none());
        assert_eq!(cases[1].tolerances.residual_db, Some(-80.0));
        assert_eq!(cases[1].tolerances.max_abs_error, Some(1e-4));
    }

    #[test]
    fn stricter_tolerances_are_allowed() {
        let cases = resolve(json!({
            "module": "envelope",
            "cases": { "a": { "num_samples": 10, "tolerances": { "residual_db": -100, "lsd_db": 0.01 } } }
        }))
        .unwrap();
        assert_eq!(cases[0].tolerances.residual_db, Some(-100.0));
        assert_eq!(cases[0].tolerances.lsd_db, Some(0.01));
    }

    #[test]
    fn looser_tolerances_need_an_exception() {
        let looser = json!({
            "module": "filter",
            "cases": { "a": { "num_samples": 10, "tolerances": { "residual_db": -70 } } }
        });
        assert!(resolve(looser).unwrap_err().contains("looser"));

        let excepted = json!({
            "module": "filter",
            "cases": { "a": { "num_samples": 10, "tolerances": { "residual_db": -70 },
                              "tolerance_exception": "LEDGER.md 2026-10-21: approved by owner" } }
        });
        assert_eq!(
            resolve(excepted).unwrap()[0].tolerances.residual_db,
            Some(-70.0)
        );
    }

    #[test]
    fn rejects_unknown_keys_and_metrics() {
        assert!(resolve(
            json!({ "module": "f", "cases": { "a": { "num_samples": 1, "tolerance": {} } } })
        )
        .unwrap_err()
        .contains("unknown key"));
        assert!(
            resolve(json!({ "module": "f", "cases": { "a": { "num_samples": 1,
            "tolerances": { "max_error": 1 } } } }))
            .unwrap_err()
            .contains("unknown tolerance metric")
        );
        assert!(resolve(json!({ "module": "f", "cases": {} })).is_err());
        assert!(resolve(json!({ "cases": { "a": { "num_samples": 1 } } })).is_err());
    }

    #[test]
    fn repository_case_files_resolve() {
        let modules = load_module_cases().unwrap();
        assert!(!modules.is_empty());
        let renders = load_render_cases().unwrap();
        assert!(!renders.is_empty());
        let mut ids: Vec<&str> = modules
            .iter()
            .chain(&renders)
            .map(|c| c.id.as_str())
            .collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate case ids");
    }
}
