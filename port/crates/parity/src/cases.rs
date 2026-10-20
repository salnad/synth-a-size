//! Resolution of corpus cases from `migration/corpus`.
//!
//! This mirrors `migration/scripts/run_corpus.py` exactly (layering,
//! approved floor, tolerance exceptions, strict keys). Captures carry the
//! resolution `run_corpus.py` used (`case.json`); `crate::captures` refuses a
//! capture whose resolution differs from this one, so the two cannot drift
//! silently.

use std::fs;
use std::path::{Path, PathBuf};

use audio_diff::Tolerances;
use serde_json::{Map, Value};

const METRICS: [&str; 4] = ["max_abs_error", "rms_diff_db", "residual_db", "lsd_db"];

/// Approved render floor (migration/decisions/reference-determinism.md, owner
/// approval in LEDGER.md 2026-10-20). Mirrored in run_corpus.py.
pub const RENDER_FLOOR: [(&str, f64); 3] = [
    ("residual_db", -50.0),
    ("lsd_db", 0.1),
    ("rms_diff_db", 0.01),
];
/// Approved module floor. Mirrored in run_corpus.py.
pub const MODULE_FLOOR: [(&str, f64); 2] = [("max_abs_error", 1e-4), ("residual_db", -80.0)];

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
const CASES_JSON_KEYS: [&str; 4] = ["comment", "defaults", "renders", "module_defaults"];
const MODULE_DEFAULT_KEYS: [&str; 5] = [
    "comment",
    "sample_rate",
    "block_size",
    "oversample",
    "tolerances",
];
const RENDER_SETTING_KEYS: [&str; 6] =
    ["sample_rate", "block_size", "tail", "length", "bpm", "seed"];

fn case_keys() -> Vec<&'static str> {
    let mut keys = SPEC_KEYS.to_vec();
    keys.extend(["description", "tolerances", "tolerance_exception"]);
    keys
}

fn file_keys() -> Vec<&'static str> {
    let mut keys = SPEC_KEYS.to_vec();
    keys.extend(["description", "tolerances", "cases"]);
    keys
}

fn render_case_keys() -> Vec<&'static str> {
    let mut keys = RENDER_SETTING_KEYS.to_vec();
    keys.extend([
        "name",
        "preset",
        "midi",
        "description",
        "tolerances",
        "tolerance_exception",
    ]);
    keys
}

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

pub fn ledger_path() -> PathBuf {
    repo_root().join("migration/LEDGER.md")
}

pub(crate) fn read_json(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // serde_json rejects NaN / Infinity, like run_corpus.py's parse_constant.
    serde_json::from_str(&text).map_err(|e| format!("{}: invalid JSON: {e}", path.display()))
}

fn as_object<'a>(value: &'a Value, what: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{what} must be a JSON object"))
}

fn check_keys(map: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<(), String> {
    let mut unknown: Vec<&String> = map
        .keys()
        .filter(|k| !allowed.contains(&k.as_str()))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    unknown.sort();
    let hint = if unknown.iter().any(|k| k.as_str() == "tolerance_exception") {
        " (tolerance_exception is case-level only)"
    } else {
        ""
    };
    Err(format!("{what}: unknown keys {unknown:?}{hint}"))
}

fn tolerance_map(value: &Value, what: &str) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();
    for (key, value) in as_object(value, &format!("{what} tolerances"))? {
        if !METRICS.contains(&key.as_str()) {
            return Err(format!("{what}: unknown tolerance metric {key:?}"));
        }
        let number = match value {
            Value::Number(n) => n.as_f64().unwrap(),
            _ => return Err(format!("{what}: tolerance {key} must be a number")),
        };
        out.insert(key.clone(), Value::from(number));
    }
    Ok(out)
}

fn f(map: &Map<String, Value>, key: &str) -> f64 {
    map[key].as_f64().unwrap()
}

/// Defaults must hold every floor metric, at the floor or stricter.
pub fn check_defaults(
    what: &str,
    tolerances: &Value,
    floor: &[(&str, f64)],
) -> Result<Map<String, Value>, String> {
    let tolerances = tolerance_map(tolerances, what)?;
    for (metric, limit) in floor {
        match tolerances.get(*metric) {
            None => {
                return Err(format!(
                    "{what}: missing {metric}; the approved floor requires it ({limit})"
                ))
            }
            Some(value) if value.as_f64().unwrap() > *limit => {
                return Err(format!(
                    "{what}: {metric} = {} is looser than the approved floor {limit}",
                    value.as_f64().unwrap()
                ))
            }
            _ => {}
        }
    }
    Ok(tolerances)
}

/// `(heading, body)` for every markdown heading outside code fences; the body
/// runs to the next heading of the same or a higher level.
pub fn ledger_sections(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut headings = Vec::new();
    let mut in_fence = false;
    for (index, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || !line.starts_with('#') {
            continue;
        }
        let level = line.len() - line.trim_start_matches('#').len();
        let rest = &line[level..];
        let title = rest.trim();
        if !title.is_empty() && (rest.starts_with(' ') || rest.starts_with('\t')) {
            headings.push((index, level, title.to_string()));
        }
    }
    let mut sections = Vec::new();
    for (position, (index, level, title)) in headings.iter().enumerate() {
        let end = headings[position + 1..]
            .iter()
            .find(|(_, later, _)| later <= level)
            .map_or(lines.len(), |(later_index, _, _)| *later_index);
        sections.push((title.clone(), lines[index + 1..end].join("\n")));
    }
    sections
}

fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '-')
}

/// True if `case_id` appears in `text` as a whole token: the characters
/// around it are not id characters (letters, digits, `_` `-` `/`). So an
/// approval for `filters/x/a` is not satisfied by a section naming
/// `filters/x/ab`. Mirrored in run_corpus.py (`names_case`).
pub fn names_case(text: &str, case_id: &str) -> bool {
    text.match_indices(case_id).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + case_id.len()..].chars().next();
        !before.is_some_and(is_id_char) && !after.is_some_and(is_id_char)
    })
}

/// Layers file and case tolerances over the defaults and enforces the
/// exception rules. Returns the resolved tolerances and the exception (if any).
pub fn resolve_tolerances(
    case_id: &str,
    defaults: &Map<String, Value>,
    file_level: &Map<String, Value>,
    case: &Map<String, Value>,
    ledger: &str,
) -> Result<(Map<String, Value>, Option<Value>), String> {
    if file_level.contains_key("tolerance_exception") {
        return Err(format!(
            "{case_id}: tolerance_exception is only allowed on individual cases"
        ));
    }
    let mut tolerances = defaults.clone();
    for (level, layer) in [("file", file_level), ("case", case)] {
        if let Some(value) = layer.get("tolerances") {
            tolerances.extend(tolerance_map(value, &format!("{case_id} ({level} level)"))?);
        }
    }
    let mut looser: Vec<&String> = defaults
        .keys()
        .filter(|k| f(&tolerances, k) > f(defaults, k))
        .collect();
    looser.sort();

    let Some(exception) = case.get("tolerance_exception") else {
        if !looser.is_empty() {
            return Err(format!(
                "{case_id}: tolerances {looser:?} are looser than the defaults; that needs a case-level \
                 tolerance_exception approved in LEDGER.md (migration/AGENTS.md rule 6)"
            ));
        }
        return Ok((tolerances, None));
    };

    let shape = || {
        format!("{case_id}: tolerance_exception must be {{\"approved_in\": \"<LEDGER.md heading>\", \"tolerances\": {{...}}}}")
    };
    let object = exception.as_object().ok_or_else(shape)?;
    if object.len() != 2
        || !object.contains_key("approved_in")
        || !object.contains_key("tolerances")
    {
        return Err(shape());
    }
    let approved_in = object["approved_in"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            format!("{case_id}: tolerance_exception.approved_in must name a LEDGER.md heading")
        })?;
    let approved = tolerance_map(
        &object["tolerances"],
        &format!("{case_id} tolerance_exception"),
    )?;
    if approved.is_empty() {
        return Err(format!(
            "{case_id}: tolerance_exception.tolerances is empty"
        ));
    }
    for (metric, value) in &approved {
        if !looser.contains(&metric) {
            return Err(format!(
                "{case_id}: tolerance_exception approves {metric}, but the case does not loosen it"
            ));
        }
        if f(&tolerances, metric) != value.as_f64().unwrap() {
            return Err(format!(
                "{case_id}: {metric} = {} differs from the approved value {}; changing it needs a new approval",
                f(&tolerances, metric),
                value.as_f64().unwrap()
            ));
        }
    }
    let unapproved: Vec<&&String> = looser
        .iter()
        .filter(|k| !approved.contains_key(k.as_str()))
        .collect();
    if !unapproved.is_empty() {
        return Err(format!(
            "{case_id}: {unapproved:?} loosened without approval in tolerance_exception"
        ));
    }
    let sections = ledger_sections(ledger);
    let bodies: Vec<&String> = sections
        .iter()
        .filter(|(title, _)| title == approved_in)
        .map(|(_, body)| body)
        .collect();
    if bodies.is_empty() {
        return Err(format!(
            "{case_id}: tolerance_exception cites LEDGER.md heading {approved_in:?}, which does not exist"
        ));
    }
    if !bodies.iter().any(|body| names_case(body, case_id)) {
        return Err(format!(
            "{case_id}: LEDGER.md section {approved_in:?} does not name the case"
        ));
    }
    Ok((tolerances, Some(exception.clone())))
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

fn check_cases_json(cases_json: &Value) -> Result<&Map<String, Value>, String> {
    let root = as_object(cases_json, "cases.json")?;
    check_keys(root, &CASES_JSON_KEYS, "cases.json")?;
    Ok(root)
}

/// Resolves all render cases from `cases.json`.
pub fn render_cases_from(cases_json: &Value, ledger: &str) -> Result<Vec<ResolvedCase>, String> {
    let root = check_cases_json(cases_json)?;
    let defaults = as_object(
        root.get("defaults").ok_or("cases.json: missing defaults")?,
        "defaults",
    )?;
    let mut default_keys = RENDER_SETTING_KEYS.to_vec();
    default_keys.push("tolerances");
    check_keys(defaults, &default_keys, "cases.json defaults")?;
    let default_tolerances = check_defaults(
        "cases.json defaults.tolerances",
        defaults
            .get("tolerances")
            .unwrap_or(&Value::Object(Map::new())),
        &RENDER_FLOOR,
    )?;
    let empty = Vec::new();
    let renders = root
        .get("renders")
        .and_then(Value::as_array)
        .unwrap_or(&empty);

    let mut resolved = Vec::new();
    for case in renders {
        let case = as_object(case, "render case")?;
        let name = case
            .get("name")
            .and_then(Value::as_str)
            .ok_or("cases.json: every render case needs a name")?;
        let id = format!("render/{name}");
        check_keys(case, &render_case_keys(), &id)?;
        for key in ["preset", "midi"] {
            if !case.get(key).is_some_and(Value::is_string) {
                return Err(format!("{id}: needs {key:?}"));
            }
        }
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
        let (tolerances, exception) =
            resolve_tolerances(&id, &default_tolerances, &Map::new(), case, ledger)?;
        let typed = to_tolerances(&tolerances);
        merged.insert("tolerances".into(), Value::Object(tolerances));
        if let Some(exception) = exception {
            merged.insert("tolerance_exception".into(), exception);
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
    ledger: &str,
) -> Result<Vec<ResolvedCase>, String> {
    check_keys(
        module_defaults,
        &MODULE_DEFAULT_KEYS,
        "cases.json module_defaults",
    )?;
    let default_tolerances = check_defaults(
        "cases.json module_defaults.tolerances",
        module_defaults
            .get("tolerances")
            .unwrap_or(&Value::Object(Map::new())),
        &MODULE_FLOOR,
    )?;
    let data = as_object(file, relative_file)?;
    check_keys(data, &file_keys(), relative_file)?;
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

    let mut resolved = Vec::new();
    for (name, case) in cases {
        let id = format!("{area}/{module}/{name}");
        let case = as_object(case, &id)?;
        check_keys(case, &case_keys(), &id)?;
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
        let (tolerances, exception) =
            resolve_tolerances(&id, &default_tolerances, &file_level, case, ledger)?;
        let typed = to_tolerances(&tolerances);
        let mut entry = Map::new();
        entry.insert("id".into(), Value::String(id.clone()));
        entry.insert("file".into(), Value::String(relative_file.to_string()));
        entry.insert("spec".into(), Value::Object(spec));
        entry.insert("tolerances".into(), Value::Object(tolerances));
        if let Some(exception) = exception {
            entry.insert("tolerance_exception".into(), exception);
        }
        resolved.push(ResolvedCase {
            id,
            json: Value::Object(entry),
            tolerances: typed,
        });
    }
    Ok(resolved)
}

fn read_ledger() -> Result<String, String> {
    let path = ledger_path();
    fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn load_render_cases() -> Result<Vec<ResolvedCase>, String> {
    render_cases_from(
        &read_json(&corpus_dir().join("cases.json"))?,
        &read_ledger()?,
    )
}

/// Path of a module case file's hash file (`<module>.hashes.json`).
pub fn module_hash_file(case: &ResolvedCase) -> PathBuf {
    let file = case.json["file"].as_str().unwrap_or_default();
    repo_root().join(format!("{}.hashes.json", file.trim_end_matches(".json")))
}

pub fn load_module_cases() -> Result<Vec<ResolvedCase>, String> {
    let cases_json = read_json(&corpus_dir().join("cases.json"))?;
    let root = check_cases_json(&cases_json)?;
    let module_defaults = root
        .get("module_defaults")
        .and_then(Value::as_object)
        .ok_or("cases.json: missing module_defaults")?
        .clone();
    let ledger = read_ledger()?;

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
            &ledger,
        )?);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LEDGER: &str = "# Ledger\n\n## 2026-10-21: approve looser filters/test/a\n\n\
        Owner approved residual_db -70 for filters/test/a.\n\n## 2026-10-22: unrelated\n\nNothing.\n";

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

    fn resolve_with(
        file: Value,
        defaults: Map<String, Value>,
    ) -> Result<Vec<ResolvedCase>, String> {
        module_cases_from("filters", "test", "x.json", &file, &defaults, LEDGER)
    }

    fn resolve(file: Value) -> Result<Vec<ResolvedCase>, String> {
        resolve_with(file, defaults())
    }

    fn approved(tolerances: Value) -> Value {
        json!({ "approved_in": "2026-10-21: approve looser filters/test/a", "tolerances": tolerances })
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
    fn looser_tolerances_without_exception_are_rejected() {
        let err = resolve(json!({
            "module": "filter",
            "cases": { "a": { "num_samples": 10, "tolerances": { "residual_db": -70 } } }
        }))
        .unwrap_err();
        assert!(err.contains("looser"), "{err}");
    }

    // B1 regression: the reviewer's free-text "x" exception unlocked
    // residual_db -10 / max_abs_error 1.0.
    #[test]
    fn free_text_exception_is_rejected() {
        let err = resolve(json!({
            "module": "filter",
            "cases": { "a": { "num_samples": 10,
                "tolerances": { "residual_db": -10, "max_abs_error": 1.0 },
                "tolerance_exception": "x" } }
        }))
        .unwrap_err();
        assert!(err.contains("tolerance_exception must be"), "{err}");
    }

    // B1 regression: a file-level exception covered every (future) case.
    #[test]
    fn file_level_exception_is_rejected() {
        let err = resolve(json!({
            "module": "filter",
            "tolerance_exception": approved(json!({ "residual_db": 0 })),
            "cases": { "a": { "num_samples": 10, "tolerances": { "residual_db": 0 } } }
        }))
        .unwrap_err();
        assert!(err.contains("case-level only"), "{err}");
    }

    // B1 regression: an approved case could be loosened further.
    #[test]
    fn loosening_past_the_approved_value_is_rejected() {
        let err = resolve(json!({
            "module": "filter",
            "cases": { "a": { "num_samples": 10, "tolerances": { "residual_db": -60 },
                "tolerance_exception": approved(json!({ "residual_db": -70 })) } }
        }))
        .unwrap_err();
        assert!(err.contains("differs from the approved value"), "{err}");
    }

    #[test]
    fn exact_approved_value_with_ledger_entry_is_accepted() {
        let cases = resolve(json!({
            "module": "filter",
            "cases": { "a": { "num_samples": 10, "tolerances": { "residual_db": -70 },
                "tolerance_exception": approved(json!({ "residual_db": -70 })) } }
        }))
        .unwrap();
        assert_eq!(cases[0].tolerances.residual_db, Some(-70.0));
        assert_eq!(cases[0].tolerances.max_abs_error, Some(1e-4));
    }

    // B1 regression: nothing checked that the approval exists.
    #[test]
    fn exception_must_cite_an_existing_ledger_heading_naming_the_case() {
        let missing = resolve(json!({
            "module": "filter",
            "cases": { "a": { "num_samples": 10, "tolerances": { "residual_db": -70 },
                "tolerance_exception": { "approved_in": "2026-10-21: no such heading",
                                         "tolerances": { "residual_db": -70 } } } }
        }))
        .unwrap_err();
        assert!(missing.contains("does not exist"), "{missing}");

        let wrong_section = resolve(json!({
            "module": "filter",
            "cases": { "a": { "num_samples": 10, "tolerances": { "residual_db": -70 },
                "tolerance_exception": { "approved_in": "2026-10-22: unrelated",
                                         "tolerances": { "residual_db": -70 } } } }
        }))
        .unwrap_err();
        assert!(
            wrong_section.contains("does not name the case"),
            "{wrong_section}"
        );

        let other_case = resolve(json!({
            "module": "filter",
            "cases": { "b": { "num_samples": 10, "tolerances": { "residual_db": -70 },
                "tolerance_exception": approved(json!({ "residual_db": -70 })) } }
        }))
        .unwrap_err();
        assert!(
            other_case.contains("does not name the case"),
            "{other_case}"
        );
    }

    // Reviewer note 2: the case id must match on token boundaries.
    #[test]
    fn ledger_must_name_the_case_as_a_whole_token() {
        for (text, expected) in [
            ("Owner approved filters/test/a.", true),
            ("approved `filters/test/a` today", true),
            ("filters/test/a", true),
            ("see filters/test/a:value", true),
            ("(filters/test/a)", true),
            ("only filters/test/ab is approved", false),
            ("xfilters/test/a", false),
            ("filters/test/a_2", false),
            ("filters/test/a-b", false),
            ("filters/test/a/extra", false),
            ("other/filters/test/a", false),
            ("filters/test/ab and later filters/test/a", true),
        ] {
            assert_eq!(names_case(text, "filters/test/a"), expected, "{text:?}");
        }

        let ledger = "## 2026-10-23: approve\n\nOwner approved filters/test/ab only.\n";
        let err = module_cases_from(
            "filters",
            "test",
            "x.json",
            &json!({ "module": "filter", "cases": { "a": { "num_samples": 10,
                "tolerances": { "residual_db": -70 },
                "tolerance_exception": { "approved_in": "2026-10-23: approve",
                                         "tolerances": { "residual_db": -70 } } } } }),
            &defaults(),
            ledger,
        )
        .unwrap_err();
        assert!(err.contains("does not name the case"), "{err}");
    }

    #[test]
    fn exception_must_cover_exactly_the_loosened_metrics() {
        let stale = resolve(json!({
            "module": "filter",
            "cases": { "a": { "num_samples": 10,
                "tolerance_exception": approved(json!({ "residual_db": -70 })) } }
        }))
        .unwrap_err();
        assert!(stale.contains("does not loosen it"), "{stale}");

        let partial = resolve(json!({
            "module": "filter",
            "cases": { "a": { "num_samples": 10, "tolerances": { "residual_db": -70, "max_abs_error": 1e-3 },
                "tolerance_exception": approved(json!({ "residual_db": -70 })) } }
        }))
        .unwrap_err();
        assert!(partial.contains("loosened without approval"), "{partial}");
    }

    // B2 regression: loosening module_defaults (or dropping a metric) passed.
    #[test]
    fn defaults_cannot_go_below_the_approved_floor() {
        let file = json!({ "module": "filter", "cases": { "a": { "num_samples": 10 } } });
        let mut loose = defaults();
        loose["tolerances"] = json!({ "residual_db": -20 });
        let err = resolve_with(file.clone(), loose).unwrap_err();
        assert!(err.contains("missing max_abs_error"), "{err}");

        let mut looser = defaults();
        looser["tolerances"] = json!({ "max_abs_error": 1e-4, "residual_db": -20 });
        let err = resolve_with(file.clone(), looser).unwrap_err();
        assert!(err.contains("looser than the approved floor"), "{err}");

        let mut stricter = defaults();
        stricter["tolerances"] =
            json!({ "max_abs_error": 1e-5, "residual_db": -90, "lsd_db": 0.1 });
        let cases = resolve_with(file, stricter).unwrap();
        assert_eq!(cases[0].tolerances.residual_db, Some(-90.0));
    }

    fn cases_json(defaults_tolerances: Value, render: Value) -> Value {
        json!({
            "defaults": { "sample_rate": 44100, "block_size": 64, "tail": 2.0, "bpm": 120, "seed": 0,
                          "tolerances": defaults_tolerances },
            "renders": [render],
            "module_defaults": {}
        })
    }

    fn floor_tolerances() -> Value {
        json!({ "rms_diff_db": 0.01, "residual_db": -50.0, "lsd_db": 0.1 })
    }

    #[test]
    fn render_defaults_cannot_go_below_the_approved_floor() {
        let render = json!({ "name": "r", "preset": "p.vital", "midi": "m.mid" });
        assert!(render_cases_from(&cases_json(floor_tolerances(), render.clone()), LEDGER).is_ok());
        let err = render_cases_from(
            &cases_json(
                json!({ "rms_diff_db": 0.01, "residual_db": -50.0, "lsd_db": 0.5 }),
                render.clone(),
            ),
            LEDGER,
        )
        .unwrap_err();
        assert!(err.contains("lsd_db"), "{err}");
        let err = render_cases_from(
            &cases_json(json!({ "rms_diff_db": 0.01, "residual_db": -50.0 }), render),
            LEDGER,
        )
        .unwrap_err();
        assert!(err.contains("missing lsd_db"), "{err}");
    }

    // N4 regression: a typo like "tolerance" in a render case was ignored.
    #[test]
    fn render_cases_reject_unknown_keys() {
        let render = json!({ "name": "r", "preset": "p.vital", "midi": "m.mid", "tolerance": {} });
        let err = render_cases_from(&cases_json(floor_tolerances(), render), LEDGER).unwrap_err();
        assert!(err.contains("unknown keys"), "{err}");
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
    fn nan_and_infinity_are_rejected() {
        assert!(serde_json::from_str::<Value>(r#"{"residual_db": NaN}"#).is_err());
        assert!(serde_json::from_str::<Value>(r#"{"residual_db": Infinity}"#).is_err());
    }

    #[test]
    fn ledger_sections_ignore_code_fences() {
        let sections = ledger_sections("## A\nx\n```\n## not a heading\n```\n### B\ny\n## C\nz\n");
        let titles: Vec<&str> = sections.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(titles, ["A", "B", "C"]);
        assert!(sections[0].1.contains("## not a heading"));
        assert!(
            sections[0].1.contains('y'),
            "A's section includes its subsection B"
        );
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
