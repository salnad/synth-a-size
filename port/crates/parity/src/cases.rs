//! Resolution of corpus cases from `migration/corpus`.
//!
//! This mirrors `migration/scripts/run_corpus.py` exactly (layering,
//! approved floor, tolerance exceptions, strict keys). Captures carry the
//! resolution `run_corpus.py` used (`case.json`); `crate::captures` refuses a
//! capture whose resolution differs from this one, so the two cannot drift
//! silently.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use audio_diff::Tolerances;
use serde_json::{Map, Value};

pub use crate::json_bound::JsonBound;

const METRICS: [&str; 4] = ["max_abs_error", "rms_diff_db", "residual_db", "lsd_db"];

/// Owner-approved bound for artifacts declared `"json"`
/// (migration/decisions/owner-decisions-json-and-morphs.md, LEDGER.md
/// 2026-10-21): 4 f32 ulp or 1e-6 absolute, whichever is looser. Cases may
/// only tighten it. Mirrored in run_corpus.py (`JSON_BOUND`).
pub const JSON_BOUND: JsonBound = JsonBound { ulp: 4, abs: 1e-6 };

/// The only keys a case may declare as base64 little-endian f32 data. Any
/// other string (e.g. PCM int16 `audio_file`) stays exact; extending this
/// set is a code change in both resolvers plus an owner decision. Mirrored
/// in run_corpus.py (`BASE64_F32_KEYS`).
pub const BASE64_F32_KEYS: [&str; 1] = ["wave_data"];

/// Owner approvals a no-fast-math allowlist line may cite, and the ledger
/// heading that records each (owner decision SAS-35, 2026-10-21). Adding a
/// key is a code change in both resolvers plus a new owner decision.
/// Mirrored in run_corpus.py (`NOFASTMATH_APPROVALS`).
pub const NOFASTMATH_APPROVALS: [(&str, &str); 1] = [(
    "SAS-35",
    "2026-10-21: owner approval — SAS-35 noisy-reference rule and SAS-36 JSON bound",
)];

/// Targets whose official reference an allowlist line may declare as failing
/// the case's normal limits against that target's no-fast-math build.
/// Mirrored in run_corpus.py (`JUSTIFY_TARGETS`), which gates the declared
/// cases per target in `--justify-nofastmath`.
pub const JUSTIFY_TARGETS: [&str; 2] = ["x86", "macos"];

/// Which reference build a case is compared against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reference {
    /// The official (fast-math) reference, with the case tolerances.
    #[default]
    Official,
    /// The `REF_FAST_MATH=OFF` build, bit for bit (allowlisted cases only).
    NoFastMath,
}

/// How a text artifact is compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactMode {
    Exact,
    Json,
}

/// How a module case's outputs are compared (beyond its tolerances).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Comparison {
    pub reference: Reference,
    /// Declared artifact modes; undeclared artifacts are exact.
    pub artifact_modes: BTreeMap<String, ArtifactMode>,
    pub json_bound: JsonBound,
    pub base64_f32_keys: Vec<String>,
}

impl Comparison {
    /// The effective mode of artifact `name`.
    pub fn artifact_mode(&self, name: &str) -> ArtifactMode {
        match (self.reference, self.artifact_modes.get(name)) {
            (Reference::Official, Some(ArtifactMode::Json)) => ArtifactMode::Json,
            _ => ArtifactMode::Exact,
        }
    }
}

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

const COMPARISON_KEYS: [&str; 3] = ["artifacts", "base64_f32_keys", "json_bound"];

fn case_keys() -> Vec<&'static str> {
    let mut keys = SPEC_KEYS.to_vec();
    keys.extend(COMPARISON_KEYS);
    keys.extend([
        "description",
        "tolerances",
        "tolerance_exception",
        "reference",
    ]);
    keys
}

fn file_keys() -> Vec<&'static str> {
    let mut keys = SPEC_KEYS.to_vec();
    keys.extend(COMPARISON_KEYS);
    keys.extend(["description", "tolerances", "cases"]);
    keys
}

fn render_case_keys() -> Vec<&'static str> {
    let mut keys = RENDER_SETTING_KEYS.to_vec();
    keys.extend([
        "name",
        "preset",
        "midi",
        "tuning",
        "keyboard_map",
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
    pub comparison: Comparison,
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

pub fn nofastmath_allowlist_dir() -> PathBuf {
    repo_root().join("migration/decisions/nofastmath")
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

/// True for a dated ledger heading, `"YYYY-MM-DD: ..."`.
pub fn is_dated_heading(title: &str) -> bool {
    let bytes = title.as_bytes();
    bytes.len() >= 12
        && bytes[..10].iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        })
        && &bytes[10..12] == b": "
}

/// `(heading, body)` of every dated heading (`"YYYY-MM-DD: ..."`) in the
/// ledger, parsed per file so no section crosses a file boundary. Undated
/// headings (e.g. the `# Ledger` title, whose section spans its whole file)
/// never count as approvals. Mirrored in run_corpus.py
/// (`approval_sections`).
pub fn approval_sections(ledger_files: &[&str]) -> Vec<(String, String)> {
    ledger_files
        .iter()
        .flat_map(|text| ledger_sections(text))
        .filter(|(title, _)| is_dated_heading(title))
        .collect()
}

/// Area, module, case and render names are `[A-Za-z0-9_-]+`, so a case id
/// can never extend another id across a token boundary (e.g. `a.b` or `a:x`
/// containing an approval for `a`). Mirrored in run_corpus.py (`check_name`).
pub fn check_name(location: &str, kind: &str, name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid {
        Ok(())
    } else {
        Err(format!(
            "{location}: {kind} name {name:?} must match [A-Za-z0-9_-]+"
        ))
    }
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
    ledger: &[&str],
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
                 tolerance_exception approved in the ledger (migration/AGENTS.md rule 6)"
            ));
        }
        return Ok((tolerances, None));
    };

    let shape = || {
        format!("{case_id}: tolerance_exception must be {{\"approved_in\": \"<ledger heading>\", \"tolerances\": {{...}}}}")
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
            format!("{case_id}: tolerance_exception.approved_in must name a ledger heading")
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
    if !is_dated_heading(approved_in) {
        return Err(format!(
            "{case_id}: tolerance_exception.approved_in must cite a dated ledger heading \
             (\"YYYY-MM-DD: ...\"), not {approved_in:?}"
        ));
    }
    let sections = approval_sections(ledger);
    let bodies: Vec<&String> = sections
        .iter()
        .filter(|(title, _)| title == approved_in)
        .map(|(_, body)| body)
        .collect();
    if bodies.is_empty() {
        return Err(format!(
            "{case_id}: tolerance_exception cites ledger heading {approved_in:?}, which does not exist \
             (migration/LEDGER.md or migration/ledger/)"
        ));
    }
    if !bodies.iter().any(|body| names_case(body, case_id)) {
        return Err(format!(
            "{case_id}: ledger section {approved_in:?} does not name the case"
        ));
    }
    Ok((tolerances, Some(exception.clone())))
}

/// `(area, module, case, approval, targets)` of a line that is exactly
/// ``- `<area>/<module>/<case>` (<approval>; <target>[, <target>]): <reason>``,
/// where the names are `[A-Za-z0-9_-]+`, the approval is `[A-Z]+-[0-9]+`,
/// each target is `[a-z0-9]+` and the reason starts with a non-space
/// character.
fn parse_allowlist_line(line: &str) -> Option<(&str, &str, &str, &str, Vec<&str>)> {
    let rest = line.strip_prefix("- `")?;
    let (id, rest) = rest.split_once('`')?;
    let (inner, reason) = rest.strip_prefix(" (")?.split_once("): ")?;
    let (key, targets) = inner.split_once("; ")?;
    let mut parts = id.split('/');
    let (area, module, case) = (parts.next()?, parts.next()?, parts.next()?);
    let name = |n: &str| check_name("", "", n).is_ok();
    let (letters, digits) = key.split_once('-')?;
    let key_ok = !letters.is_empty()
        && letters.chars().all(|c| c.is_ascii_uppercase())
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit());
    let targets: Vec<&str> = targets.split(", ").collect();
    let targets_ok = targets.iter().all(|t| {
        !t.is_empty()
            && t.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    });
    let reason_ok = reason.chars().next().is_some_and(|c| !c.is_whitespace());
    (parts.next().is_none()
        && name(area)
        && name(module)
        && name(case)
        && key_ok
        && targets_ok
        && reason_ok)
        .then_some((area, module, case, key, targets))
}

/// One allowlist line: the approval key and the declared targets.
pub type AllowlistEntry = (String, Vec<String>);

/// `(case_id, approval, targets)` from one allowlist file,
/// `migration/decisions/nofastmath/<area>/<module>.md`. Every line that
/// starts with a list marker (after any indentation) must be exactly
/// ``- `<area>/<module>/<case>` (<approval>; <target>[, <target>]): <reason>``,
/// naming a case of this file's module, an approval in
/// [`NOFASTMATH_APPROVALS`] and distinct targets from [`JUSTIFY_TARGETS`].
/// Other lines are prose. Mirrored in run_corpus.py (`allowlist_entries`).
pub fn allowlist_entries(
    rel: &str,
    area: &str,
    module: &str,
    text: &str,
) -> Result<Vec<(String, String, Vec<String>)>, String> {
    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        match line.trim_start().chars().next() {
            Some('-' | '*' | '+') => {}
            _ => continue,
        }
        let (a, m, c, key, targets) = parse_allowlist_line(line).ok_or_else(|| {
            format!(
                "{rel}:{number}: list lines must be \"- `<area>/<module>/<case>` (<approval>; \
                 <target>[, <target>]): <reason>\", not {line:?}"
            )
        })?;
        if (a, m) != (area, module) {
            return Err(format!(
                "{rel}:{number}: this file lists {area}/{module} cases only, not {a}/{m}/{c}"
            ));
        }
        if !NOFASTMATH_APPROVALS.iter().any(|(k, _)| *k == key) {
            let keys: Vec<&str> = NOFASTMATH_APPROVALS.iter().map(|(k, _)| *k).collect();
            return Err(format!(
                "{rel}:{number}: {key} is not an approval pinned in both resolvers ({keys:?})"
            ));
        }
        let distinct: std::collections::BTreeSet<&&str> = targets.iter().collect();
        if targets.iter().any(|t| !JUSTIFY_TARGETS.contains(t)) || distinct.len() != targets.len() {
            return Err(format!(
                "{rel}:{number}: targets must be distinct names from {JUSTIFY_TARGETS:?}, not {:?}",
                targets.join(", ")
            ));
        }
        entries.push((
            format!("{a}/{m}/{c}"),
            key.to_string(),
            targets.iter().map(|t| t.to_string()).collect(),
        ));
    }
    Ok(entries)
}

/// `{case_id: (approval, targets)}` from `{path relative to the allowlist
/// dir: text}`.
/// Only `README.md` and `<area>/<module>.md` may exist there, and an id may
/// be listed once. Mirrored in run_corpus.py (`allowlist_from_files`).
pub fn allowlist_from_files(
    texts: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, AllowlistEntry>, String> {
    let mut allowlist = BTreeMap::new();
    for (path, text) in texts {
        if path == "README.md" {
            continue;
        }
        let rel = format!("migration/decisions/nofastmath/{path}");
        let parts: Vec<&str> = path.split('/').collect();
        let (area, module) = match parts[..] {
            [area, file] if file.ends_with(".md") => (area, &file[..file.len() - 3]),
            _ => {
                return Err(format!(
                    "{rel}: only <area>/<module>.md allowlist files may live here"
                ))
            }
        };
        check_name(&rel, "area", area)?;
        check_name(&rel, "module", module)?;
        for (case_id, approval, targets) in allowlist_entries(&rel, area, module, text)? {
            if allowlist.contains_key(&case_id) {
                return Err(format!("{rel}: {case_id} is listed twice"));
            }
            allowlist.insert(case_id, (approval, targets));
        }
    }
    Ok(allowlist)
}

/// Every cited approval's ledger heading exists and its section names the
/// approval key. Mirrored in run_corpus.py (`check_allowlist_approvals`).
pub fn check_allowlist_approvals(
    allowlist: &BTreeMap<String, AllowlistEntry>,
    ledger: &[&str],
) -> Result<(), String> {
    let sections = approval_sections(ledger);
    let cited: std::collections::BTreeSet<&String> = allowlist.values().map(|(a, _)| a).collect();
    for approval in cited {
        let heading = NOFASTMATH_APPROVALS
            .iter()
            .find(|(k, _)| k == approval)
            .map(|(_, h)| *h)
            .ok_or_else(|| format!("{approval} is not a pinned approval"))?;
        let named = sections
            .iter()
            .any(|(title, body)| title == heading && names_case(body, approval));
        if !named {
            return Err(format!(
                "no-fast-math allowlist cites {approval}, but the ledger has no section \
                 {heading:?} naming it"
            ));
        }
    }
    Ok(())
}

fn collect_allowlist_texts(
    dir: &Path,
    root: &Path,
    texts: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect_allowlist_texts(&path, root, texts)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let rel = rel.to_string_lossy().replace('\\', "/");
            let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            texts.insert(rel, text);
        }
    }
    Ok(())
}

/// The allowlisted case ids, from `migration/decisions/nofastmath/`, with the
/// cited approvals checked against the ledger. Mirrored in run_corpus.py
/// (`read_allowlist`).
fn read_allowlist(ledger: &[&str]) -> Result<Vec<String>, String> {
    Ok(read_allowlist_entries(ledger)?.into_keys().collect())
}

/// `{case_id: (approval, targets)}`, as [`read_allowlist`] checks them.
pub fn read_allowlist_entries(ledger: &[&str]) -> Result<BTreeMap<String, AllowlistEntry>, String> {
    let dir = nofastmath_allowlist_dir();
    let mut texts = BTreeMap::new();
    if dir.is_dir() {
        collect_allowlist_texts(&dir, &dir, &mut texts)?;
    }
    let allowlist = allowlist_from_files(&texts)?;
    check_allowlist_approvals(&allowlist, ledger)?;
    Ok(allowlist)
}

/// `{case id: reason}` from a park file's JSON, such as
/// `migration/corpus/parked_arm64.json`:
/// `{"description": "...", "cases": {"<id>": {"approved_in": "<dated ledger
/// heading>", "reason": "..."}}}`. A park switches off every check for the
/// case on that reference, so it needs a recorded owner approval, like a
/// `tolerance_exception`: `approved_in` must be a dated heading whose
/// section names the case. Every parked id must be a loaded case, and a
/// case whose no-fast-math allowlist targets include `macos` cannot be
/// parked (it is justified only on macOS). Mirrored in run_corpus.py
/// (`resolve_parked`).
pub fn resolve_parked(
    data: &Value,
    location: &str,
    known: &[String],
    allowlist: &BTreeMap<String, AllowlistEntry>,
    ledger: &[&str],
) -> Result<BTreeMap<String, String>, String> {
    let shape = || {
        format!(
            "{location}: must be {{\"description\": \"...\", \"cases\": {{<case id>: {{\"approved_in\": \
             \"<dated ledger heading>\", \"reason\": \"...\"}}}}}}"
        )
    };
    let root = data.as_object().ok_or_else(shape)?;
    let cases = root
        .get("cases")
        .and_then(Value::as_object)
        .ok_or_else(shape)?;
    if root.keys().any(|k| k != "description" && k != "cases")
        || root.get("description").is_some_and(|d| !d.is_string())
    {
        return Err(shape());
    }
    let sections = approval_sections(ledger);
    let mut parked = BTreeMap::new();
    for (case_id, entry) in cases {
        let field = |key: &str| {
            entry
                .get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
        };
        let shaped = entry.as_object().is_some_and(|o| {
            o.len() == 2 && o.contains_key("approved_in") && o.contains_key("reason")
        });
        let (Some(approved_in), Some(reason), true) =
            (field("approved_in"), field("reason"), shaped)
        else {
            return Err(format!(
                "{location}: {case_id} must be {{\"approved_in\": \"<dated ledger heading>\", \"reason\": \
                 \"...\"}} with non-empty strings"
            ));
        };
        if !known.contains(case_id) {
            return Err(format!(
                "{location}: parked id {case_id} is not a loaded case (renamed, removed or moved to pending/?)"
            ));
        }
        let approved_in = approved_in.trim();
        if !is_dated_heading(approved_in) {
            return Err(format!(
                "{location}: {case_id}: approved_in must cite a dated ledger heading \
                 (\"YYYY-MM-DD: ...\"), not {approved_in:?}"
            ));
        }
        let bodies: Vec<&String> = sections
            .iter()
            .filter(|(title, _)| title == approved_in)
            .map(|(_, body)| body)
            .collect();
        if bodies.is_empty() {
            return Err(format!(
                "{location}: {case_id}: approved_in cites ledger heading {approved_in:?}, which does not \
                 exist (migration/LEDGER.md or migration/ledger/)"
            ));
        }
        if !bodies.iter().any(|body| names_case(body, case_id)) {
            return Err(format!(
                "{location}: {case_id}: ledger section {approved_in:?} does not name the case"
            ));
        }
        if allowlist
            .get(case_id)
            .is_some_and(|(_, targets)| targets.iter().any(|t| t == "macos"))
        {
            return Err(format!(
                "{location}: {case_id} is declared for macos in the no-fast-math allowlist: the macOS \
                 proxy compare is its justification, so it cannot be parked there (it stays live)"
            ));
        }
        parked.insert(case_id.clone(), reason.to_string());
    }
    Ok(parked)
}

/// [`resolve_parked`] on a park file, against the loaded cases, the
/// repository allowlist and the ledger.
pub fn load_parked(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let ledger = read_ledger()?;
    let ledger: Vec<&str> = ledger.iter().map(String::as_str).collect();
    let known: Vec<String> = load_module_cases()?
        .into_iter()
        .chain(load_render_cases()?)
        .map(|c| c.id)
        .collect();
    let allowlist = read_allowlist_entries(&ledger)?;
    resolve_parked(
        &read_json(path)?,
        &path.display().to_string(),
        &known,
        &allowlist,
        &ledger,
    )
}

fn is_artifact_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Artifact modes, the JSON bound, base64-f32 keys and the reference of a
/// module case. Returns the resolved comparison and the entries that differ
/// from the defaults (exact artifacts, official reference), which are added
/// to the resolved case. Mirrored in run_corpus.py (`resolve_comparison`).
pub fn resolve_comparison(
    case_id: &str,
    file_level: &Map<String, Value>,
    case: &Map<String, Value>,
    allowlist: &[String],
) -> Result<(Comparison, Map<String, Value>), String> {
    let layers = [("file", file_level), ("case", case)];
    let mut modes = BTreeMap::new();
    for (level, layer) in layers {
        let Some(artifacts) = layer.get("artifacts") else {
            continue;
        };
        let artifacts = artifacts
            .as_object()
            .ok_or_else(|| format!("{case_id} ({level} level): artifacts must be an object"))?;
        for (name, mode) in artifacts {
            if !is_artifact_name(name) {
                return Err(format!(
                    "{case_id}: artifact name {name:?} must match [A-Za-z0-9_.-]+"
                ));
            }
            let mode = match mode.as_str() {
                Some("exact") => ArtifactMode::Exact,
                Some("json") => ArtifactMode::Json,
                _ => {
                    return Err(format!(
                        "{case_id}: artifact {name} mode must be one of [\"exact\", \"json\"], not {mode}"
                    ))
                }
            };
            modes.insert(name.clone(), mode);
        }
    }

    let mut keys: Option<Vec<String>> = None;
    for (level, layer) in layers {
        let Some(value) = layer.get("base64_f32_keys") else {
            continue;
        };
        let names: Option<std::collections::BTreeSet<String>> = value.as_array().and_then(|a| {
            a.iter()
                .map(|k| k.as_str().filter(|s| !s.is_empty()).map(str::to_string))
                .collect()
        });
        let names = names.ok_or_else(|| {
            format!("{case_id} ({level} level): base64_f32_keys must be a list of key names")
        })?;
        if let Some(key) = names
            .iter()
            .find(|k| !BASE64_F32_KEYS.contains(&k.as_str()))
        {
            return Err(format!(
                "{case_id}: {key:?} cannot be declared in base64_f32_keys; only {BASE64_F32_KEYS:?} hold \
                 base64 f32 data (pinned in both resolvers)"
            ));
        }
        keys = Some(names.into_iter().collect());
    }

    let mut bound = JSON_BOUND;
    for (level, layer) in layers {
        let Some(value) = layer.get("json_bound") else {
            continue;
        };
        let object = value
            .as_object()
            .filter(|o| !o.is_empty() && o.keys().all(|k| k == "ulp" || k == "abs"))
            .ok_or_else(|| {
                format!("{case_id} ({level} level): json_bound must be {{\"ulp\": <int>, \"abs\": <number>}}")
            })?;
        let ulp = match object.get("ulp") {
            None => None,
            Some(v) => Some(v.as_u64().ok_or_else(|| {
                format!("{case_id}: json_bound.ulp must be a non-negative integer")
            })?),
        };
        let abs = match object.get("abs") {
            None => None,
            Some(v) => Some(v.as_f64().filter(|a| *a >= 0.0).ok_or_else(|| {
                format!("{case_id}: json_bound.abs must be a non-negative number")
            })?),
        };
        // Same order as run_corpus.py: "abs" is checked before "ulp".
        if let Some(abs) = abs.filter(|a| *a > JSON_BOUND.abs) {
            return Err(format!(
                "{case_id}: json_bound abs = {abs} is looser than the approved JSON bound {}; the \
                 bound is pinned in both resolvers and only tightens",
                JSON_BOUND.abs
            ));
        }
        if let Some(ulp) = ulp.filter(|u| *u > JSON_BOUND.ulp) {
            return Err(format!(
                "{case_id}: json_bound ulp = {ulp} is looser than the approved JSON bound {}; the \
                 bound is pinned in both resolvers and only tightens",
                JSON_BOUND.ulp
            ));
        }
        bound.ulp = ulp.unwrap_or(bound.ulp);
        bound.abs = abs.unwrap_or(bound.abs);
    }

    let reference = match case.get("reference").map(Value::as_str) {
        None | Some(Some("official")) => Reference::Official,
        Some(Some("nofastmath")) => Reference::NoFastMath,
        Some(_) => {
            return Err(format!(
                "{case_id}: reference must be one of [\"official\", \"nofastmath\"], not {}",
                case["reference"]
            ))
        }
    };
    let mut extra = Map::new();
    if reference == Reference::NoFastMath {
        // The JSON bound is part of the normal limits the calibration
        // justification uses, so it is always the pinned one here.
        bound = JSON_BOUND;
        if case.contains_key("json_bound") {
            return Err(format!(
                "{case_id}: a reference \"nofastmath\" case cannot set json_bound"
            ));
        }
        if !allowlist.iter().any(|id| id == case_id) {
            let mut parts = case_id.split('/');
            let (area, module) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            return Err(format!(
                "{case_id}: reference \"nofastmath\" is only allowed for case ids listed in \
                 migration/decisions/nofastmath/{area}/{module}.md"
            ));
        }
        for key in ["tolerances", "tolerance_exception"] {
            if case.contains_key(key) {
                return Err(format!(
                    "{case_id}: a reference \"nofastmath\" case cannot set {key}: it is compared exactly, \
                     and its normal limits (the module defaults) decide in calibration whether it qualifies"
                ));
            }
        }
        // Compared bit for bit; the declared artifact modes still define the
        // normal limits the calibration justification uses.
        extra.insert("reference".into(), Value::from("nofastmath"));
    }

    let has_json = modes.values().any(|m| *m == ArtifactMode::Json);
    if !modes.is_empty() {
        let object = modes
            .iter()
            .map(|(name, mode)| {
                let mode = match mode {
                    ArtifactMode::Exact => "exact",
                    ArtifactMode::Json => "json",
                };
                (name.clone(), Value::from(mode))
            })
            .collect();
        extra.insert("artifacts".into(), Value::Object(object));
    }
    let keys = keys.unwrap_or_default();
    if has_json {
        let mut object = Map::new();
        object.insert("ulp".into(), Value::from(bound.ulp));
        object.insert("abs".into(), Value::from(bound.abs));
        extra.insert("json_bound".into(), Value::Object(object));
        if !keys.is_empty() {
            extra.insert(
                "base64_f32_keys".into(),
                Value::Array(keys.iter().cloned().map(Value::from).collect()),
            );
        }
    }
    let comparison = Comparison {
        reference,
        artifact_modes: modes,
        json_bound: bound,
        base64_f32_keys: if has_json { keys } else { Vec::new() },
    };
    Ok((comparison, extra))
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
pub fn render_cases_from(cases_json: &Value, ledger: &[&str]) -> Result<Vec<ResolvedCase>, String> {
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
        check_name("cases.json", "render case", name)?;
        let id = format!("render/{name}");
        check_keys(case, &render_case_keys(), &id)?;
        for key in ["preset", "midi"] {
            if !case.get(key).is_some_and(Value::is_string) {
                return Err(format!("{id}: needs {key:?}"));
            }
        }
        for key in ["tuning", "keyboard_map"] {
            if case.get(key).is_some_and(|value| !value.is_string()) {
                return Err(format!("{id}: {key:?} must be a corpus path"));
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
            comparison: Comparison::default(),
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
    ledger: &[&str],
    allowlist: &[String],
) -> Result<Vec<ResolvedCase>, String> {
    check_name(relative_file, "area", area)?;
    check_name(relative_file, "module", module)?;
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
        check_name(relative_file, "case", name)?;
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
        let (comparison, extra) = resolve_comparison(&id, &file_level, case, allowlist)?;
        let (tolerances, exception) = if comparison.reference == Reference::NoFastMath {
            // Normal limits are the module defaults: file-level tightening
            // must not make the official reference "fail" on purpose.
            (default_tolerances.clone(), None)
        } else {
            resolve_tolerances(&id, &default_tolerances, &file_level, case, ledger)?
        };
        let typed = to_tolerances(&tolerances);
        let mut entry = Map::new();
        entry.insert("id".into(), Value::String(id.clone()));
        entry.insert("file".into(), Value::String(relative_file.to_string()));
        entry.insert("spec".into(), Value::Object(spec));
        entry.insert("tolerances".into(), Value::Object(tolerances));
        if let Some(exception) = exception {
            entry.insert("tolerance_exception".into(), exception);
        }
        entry.extend(extra);
        resolved.push(ResolvedCase {
            id,
            json: Value::Object(entry),
            tolerances: typed,
            comparison,
        });
    }
    Ok(resolved)
}

fn read_ledger() -> Result<Vec<String>, String> {
    read_ledger_at(&repo_root())
}

/// The text of every ledger file: `migration/LEDGER.md` (frozen), then each
/// entry file in `migration/ledger/` in file-name (date) order, skipping
/// `README.md`. Each is parsed on its own. Mirrored in
/// migration/scripts/ledger.py (`read_files`).
pub fn read_ledger_at(root: &Path) -> Result<Vec<String>, String> {
    let read =
        |path: &Path| fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    let mut parts = Vec::new();
    let frozen = root.join("migration/LEDGER.md");
    if frozen.is_file() {
        parts.push(read(&frozen)?);
    }
    let entries = root.join("migration/ledger");
    if entries.is_dir() {
        let mut names: Vec<String> = fs::read_dir(&entries)
            .map_err(|e| format!("{}: {e}", entries.display()))?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".md") && n != "README.md")
            .collect();
        names.sort();
        for name in names {
            parts.push(read(&entries.join(name))?);
        }
    }
    Ok(parts)
}

pub fn load_render_cases() -> Result<Vec<ResolvedCase>, String> {
    let ledger = read_ledger()?;
    let ledger: Vec<&str> = ledger.iter().map(String::as_str).collect();
    render_cases_from(&read_json(&corpus_dir().join("cases.json"))?, &ledger)
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
    let ledger: Vec<&str> = ledger.iter().map(String::as_str).collect();
    let allowlist = read_allowlist(&ledger)?;

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
            &allowlist,
        )?);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LEDGER_TEXT: &str = "# Ledger\n\n## 2026-10-21: approve looser filters/test/a\n\n\
        Owner approved residual_db -70 for filters/test/a.\n\n## 2026-10-22: unrelated\n\nNothing.\n";
    const LEDGER: &[&str] = &[LEDGER_TEXT];

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
        module_cases_from("filters", "test", "x.json", &file, &defaults, LEDGER, &[])
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
            &[ledger],
            &[],
        )
        .unwrap_err();
        assert!(err.contains("does not name the case"), "{err}");
    }

    // Reviewer nit: "a.b" or "a:x" would contain an approval for "a" across a
    // token boundary, so names are restricted at load.
    #[test]
    fn names_are_restricted_to_id_characters() {
        let file =
            |name: &str| json!({ "module": "filter", "cases": { name: { "num_samples": 1 } } });
        for name in ["a.b", "a:x", "a b", "a/b", "", "ä"] {
            let err = resolve(file(name)).unwrap_err();
            assert!(err.contains("must match [A-Za-z0-9_-]+"), "{name:?}: {err}");
        }
        assert!(resolve(file("Case_2-b")).is_ok());

        let ok = json!({ "module": "filter", "cases": { "a": { "num_samples": 1 } } });
        for (area, module) in [
            ("filters", "dig.svf"),
            ("filters", "svf:x"),
            ("fil.ters", "svf"),
            ("", "svf"),
        ] {
            let err = module_cases_from(area, module, "x.json", &ok, &defaults(), LEDGER, &[])
                .unwrap_err();
            assert!(
                err.contains("must match [A-Za-z0-9_-]+"),
                "{area}/{module}: {err}"
            );
        }

        for name in ["init.saw", "init:saw", ""] {
            let render = json!({ "name": name, "preset": "p.vital", "midi": "m.mid" });
            let err =
                render_cases_from(&cases_json(floor_tolerances(), render), LEDGER).unwrap_err();
            assert!(err.contains("must match [A-Za-z0-9_-]+"), "{name:?}: {err}");
        }
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
    fn render_cases_take_a_tuning_file_and_keyboard_map() {
        let render = json!({ "name": "r", "preset": "p.vital", "midi": "m.mid",
                             "tuning": "tunings/t.scl", "keyboard_map": "tunings/t.kbm" });
        let cases = render_cases_from(&cases_json(floor_tolerances(), render), LEDGER).unwrap();
        assert_eq!(cases[0].json["tuning"], "tunings/t.scl");
        assert_eq!(cases[0].json["keyboard_map"], "tunings/t.kbm");
        let render = json!({ "name": "r", "preset": "p.vital", "midi": "m.mid", "tuning": 1 });
        let err = render_cases_from(&cases_json(floor_tolerances(), render), LEDGER).unwrap_err();
        assert!(err.contains("must be a corpus path"), "{err}");
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

    const MORPH: &str = "filters/test/morph";

    fn resolve_morph(
        case: Value,
        file_level: Value,
        allowlist: &[&str],
    ) -> Result<ResolvedCase, String> {
        let mut file = file_level;
        file["module"] = json!("filter");
        file["cases"] = json!({ "morph": case });
        let allowlist: Vec<String> = allowlist.iter().map(|s| s.to_string()).collect();
        module_cases_from(
            "filters",
            "test",
            "x.json",
            &file,
            &defaults(),
            LEDGER,
            &allowlist,
        )
        .map(|mut cases| cases.remove(0))
    }

    #[test]
    fn undeclared_cases_resolve_as_before() {
        let case = resolve_morph(json!({ "num_samples": 1 }), json!({}), &[]).unwrap();
        let keys: Vec<&String> = case.json.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["file", "id", "spec", "tolerances"]);
        assert_eq!(case.comparison, Comparison::default());
    }

    #[test]
    fn json_artifacts_get_the_pinned_bound() {
        let case = resolve_morph(
            json!({ "artifacts": { "saved.json": "json", "notes.txt": "exact" },
                    "base64_f32_keys": ["wave_data"] }),
            json!({}),
            &[],
        )
        .unwrap();
        assert_eq!(
            case.json["artifacts"],
            json!({ "saved.json": "json", "notes.txt": "exact" })
        );
        assert_eq!(case.json["json_bound"], json!({ "ulp": 4, "abs": 1e-6 }));
        assert_eq!(case.json["base64_f32_keys"], json!(["wave_data"]));
        assert_eq!(
            case.comparison.artifact_mode("saved.json"),
            ArtifactMode::Json
        );
        assert_eq!(
            case.comparison.artifact_mode("notes.txt"),
            ArtifactMode::Exact
        );
        assert_eq!(
            case.comparison.artifact_mode("other.json"),
            ArtifactMode::Exact
        );

        let tighter = resolve_morph(
            json!({ "json_bound": { "ulp": 0, "abs": 0 } }),
            json!({ "artifacts": { "saved.json": "json" } }),
            &[],
        )
        .unwrap();
        assert_eq!(tighter.json["json_bound"], json!({ "ulp": 0, "abs": 0.0 }));
        assert_eq!(
            tighter.comparison.json_bound,
            JsonBound { ulp: 0, abs: 0.0 }
        );
    }

    #[test]
    fn json_bound_cannot_be_loosened() {
        let json_artifact = json!({ "saved.json": "json" });
        for (case, file) in [
            (
                json!({ "artifacts": json_artifact, "json_bound": { "ulp": 5 } }),
                json!({}),
            ),
            (
                json!({ "artifacts": json_artifact, "json_bound": { "abs": 1e-5 } }),
                json!({}),
            ),
            (
                json!({ "num_samples": 1 }),
                json!({ "artifacts": json_artifact, "json_bound": { "ulp": 96 } }),
            ),
        ] {
            let err = resolve_morph(case, file, &[]).unwrap_err();
            assert!(err.contains("looser than the approved JSON bound"), "{err}");
        }
        let err = resolve_morph(
            json!({ "artifacts": json_artifact, "json_bound": { "ulp": 2.5 } }),
            json!({}),
            &[],
        )
        .unwrap_err();
        assert!(
            err.contains("json_bound.ulp must be a non-negative integer"),
            "{err}"
        );
        let err = resolve_morph(
            json!({ "artifacts": json_artifact, "json_bound": { "rel": 1 } }),
            json!({}),
            &[],
        )
        .unwrap_err();
        assert!(err.contains("json_bound must be"), "{err}");
        let err = resolve_morph(
            json!({ "artifacts": { "saved.json": "loose" } }),
            json!({}),
            &[],
        )
        .unwrap_err();
        assert!(err.contains("mode must be one of"), "{err}");
    }

    // Reviewer note on #34: declaring "audio_file" let int16 PCM pass the f32
    // bound (reinterpreted words are denormals within 1e-6).
    #[test]
    fn only_pinned_base64_f32_keys_can_be_declared() {
        let json_artifact = json!({ "saved.json": "json" });
        for (case, file) in [
            (
                json!({ "artifacts": json_artifact, "base64_f32_keys": ["audio_file"] }),
                json!({}),
            ),
            (
                json!({ "artifacts": json_artifact, "base64_f32_keys": ["wave_data", "x"] }),
                json!({}),
            ),
            (
                json!({ "artifacts": json_artifact }),
                json!({ "base64_f32_keys": ["audio_file"] }),
            ),
        ] {
            let err = resolve_morph(case, file, &[]).unwrap_err();
            assert!(
                err.contains("cannot be declared in base64_f32_keys"),
                "{err}"
            );
        }
        assert_eq!(BASE64_F32_KEYS, ["wave_data"]);
    }

    // Reviewer note on #34: a tightened json_bound made the official
    // reference "fail" the justification on purpose.
    #[test]
    fn nofastmath_cases_always_use_the_pinned_json_bound() {
        let case = resolve_morph(
            json!({ "reference": "nofastmath" }),
            json!({ "artifacts": { "saved.json": "json" }, "json_bound": { "ulp": 0, "abs": 0 } }),
            &[MORPH],
        )
        .unwrap();
        assert_eq!(case.json["json_bound"], json!({ "ulp": 4, "abs": 1e-6 }));
        assert_eq!(case.comparison.json_bound, JSON_BOUND);
        let err = resolve_morph(
            json!({ "reference": "nofastmath", "json_bound": { "ulp": 0 } }),
            json!({ "artifacts": { "saved.json": "json" } }),
            &[MORPH],
        )
        .unwrap_err();
        assert!(err.contains("cannot set json_bound"), "{err}");
    }

    #[test]
    fn nofastmath_needs_the_decision_doc_allowlist() {
        let nofastmath = json!({ "reference": "nofastmath" });
        for allowlist in [&[][..], &["filters/test/morph_2", "filters/test"][..]] {
            let err = resolve_morph(nofastmath.clone(), json!({}), allowlist).unwrap_err();
            assert!(err.contains("only allowed for case ids listed"), "{err}");
        }
        let case = resolve_morph(
            json!({ "reference": "nofastmath", "artifacts": { "saved.json": "json" } }),
            json!({ "tolerances": { "residual_db": -120 } }),
            &[MORPH],
        )
        .unwrap();
        assert_eq!(case.comparison.reference, Reference::NoFastMath);
        assert_eq!(case.json["reference"], "nofastmath");
        // The declared modes stay (normal limits for calibration), but the
        // comparison itself is exact.
        assert_eq!(case.json["json_bound"], json!({ "ulp": 4, "abs": 1e-6 }));
        assert_eq!(
            case.comparison.artifact_mode("saved.json"),
            ArtifactMode::Exact
        );
        // Normal limits are the module defaults; file-level tightening is ignored.
        assert_eq!(case.tolerances.residual_db, Some(-80.0));
        assert_eq!(case.json["tolerances"], defaults()["tolerances"]);

        for (key, value) in [
            (
                "tolerance_exception",
                approved(json!({ "residual_db": -70 })),
            ),
            ("tolerances", json!({ "max_abs_error": 1e-12 })),
        ] {
            let mut morph = json!({ "reference": "nofastmath" });
            morph[key] = value;
            let err = resolve_morph(morph, json!({}), &[MORPH]).unwrap_err();
            assert!(err.contains(&format!("cannot set {key}")), "{err}");
        }
        let err = resolve_morph(json!({ "num_samples": 1 }), nofastmath, &[MORPH]).unwrap_err();
        assert!(err.contains("unknown keys"), "{err}");
        let err = resolve_morph(json!({ "reference": "x86" }), json!({}), &[]).unwrap_err();
        assert!(err.contains("reference must be one of"), "{err}");
        let official = resolve_morph(json!({ "reference": "official" }), json!({}), &[]).unwrap();
        assert!(official.json.get("reference").is_none());
    }

    const APPROVAL_LEDGER: &str = "# Ledger\n\n## 2026-10-21: owner approval — SAS-35 \
        noisy-reference rule and SAS-36 JSON bound\n\nSAS-35: approved.\n";

    fn allowlist(files: &[(&str, &str)]) -> Result<BTreeMap<String, AllowlistEntry>, String> {
        let texts = files
            .iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect();
        allowlist_from_files(&texts)
    }

    #[test]
    fn allowlist_files_are_per_module_and_strict() {
        let good = "# filters/test\n\nProse with `filters/test/other` is ignored.\n\n\
                    - `filters/test/morph` (SAS-35; x86): noise-level phases\n\
                    - `filters/test/b` (SAS-35; macos, x86): both\n";
        let parsed =
            allowlist(&[("filters/test.md", good), ("README.md", "- anything\n")]).unwrap();
        assert_eq!(parsed.keys().collect::<Vec<_>>(), ["filters/test/b", MORPH]);
        assert_eq!(
            parsed[MORPH],
            ("SAS-35".to_string(), vec!["x86".to_string()])
        );
        assert_eq!(parsed["filters/test/b"].1, ["macos", "x86"]);
        check_allowlist_approvals(&parsed, &[APPROVAL_LEDGER]).unwrap();

        for (file, text, fragment) in [
            (
                "filters/test.md",
                "- `filters/test/*` (SAS-35; x86): glob\n",
                "list lines must be",
            ),
            (
                "filters/test.md",
                "- `filters/test/morph` (SAS-35): no target\n",
                "list lines must be",
            ),
            (
                "filters/test.md",
                "- `filters/test/morph` (SAS-35; x86,macos): separator\n",
                "list lines must be",
            ),
            (
                "filters/test.md",
                "- `filters/test/morph` (SAS-35; arm): x\n",
                "targets must be distinct",
            ),
            (
                "filters/test.md",
                "- `filters/test/morph` (SAS-35; x86, x86): x\n",
                "targets must be distinct",
            ),
            (
                "filters/test.md",
                "- `filters/test/morph`: no approval\n",
                "list lines must be",
            ),
            (
                "filters/test.md",
                "- `filters/test/morph` (SAS-35; x86):\n",
                "list lines must be",
            ),
            (
                "filters/test.md",
                "  - `filters/test/morph` (SAS-35; x86): indented\n",
                "list lines must be",
            ),
            (
                "filters/test.md",
                "* `filters/test/morph` (SAS-35; x86): star\n",
                "list lines must be",
            ),
            (
                "filters/test.md",
                "- `filters/test/morph/x` (SAS-35; x86): deep\n",
                "list lines must be",
            ),
            (
                "filters/test.md",
                "- `filters/other/morph` (SAS-35; x86): x\n",
                "lists filters/test cases only",
            ),
            (
                "filters/test.md",
                "- `filters/test/morph` (SAS-99; x86): x\n",
                "not an approval pinned",
            ),
            ("filters/test.txt", "", "only <area>/<module>.md"),
            ("filters/sub/test.md", "", "only <area>/<module>.md"),
            ("test.md", "", "only <area>/<module>.md"),
            ("filters/te.st.md", "", "module name"),
        ] {
            let err = allowlist(&[(file, text)]).unwrap_err();
            assert!(err.contains(fragment), "{file}: {text:?}: {err}");
        }
        let twice =
            "- `filters/test/morph` (SAS-35; x86): a\n- `filters/test/morph` (SAS-35; x86): b\n";
        assert!(allowlist(&[("filters/test.md", twice)])
            .unwrap_err()
            .contains("listed twice"));

        // The cited approval's heading must exist and name the key.
        for ledger in [
            LEDGER_TEXT,
            "## 2026-10-21: owner approval — SAS-35 noisy-reference rule and SAS-36 JSON bound\n\n\
             SAS-350 only.\n",
        ] {
            let err = check_allowlist_approvals(&parsed, &[ledger]).unwrap_err();
            assert!(err.contains("cites SAS-35"), "{err}");
        }

        let ledger = read_ledger().unwrap();
        let ledger: Vec<&str> = ledger.iter().map(String::as_str).collect();
        let repository = read_allowlist(&ledger).unwrap();
        assert!(
            repository.contains(&"wavetable/render/wave_linear_freq_disjoint_spectra".to_string())
        );
    }

    #[test]
    fn ledger_is_the_frozen_file_then_entry_files_in_date_order() {
        let root = std::env::temp_dir().join(format!("sas-parity-ledger-{}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        fs::create_dir_all(root.join("migration/ledger")).unwrap();
        fs::write(
            root.join("migration/LEDGER.md"),
            "# Ledger\n\n## 2026-10-20: old\n\nHistory.\n",
        )
        .unwrap();
        fs::write(
            root.join("migration/ledger/2026-10-23-approve-b.md"),
            "## 2026-10-23: approve looser filters/test/b\n\nOwner approved filters/test/b.\n",
        )
        .unwrap();
        fs::write(
            root.join("migration/ledger/2026-10-21-a.md"),
            "## 2026-10-21: a\n\nFirst.\n",
        )
        .unwrap();
        fs::write(
            root.join("migration/ledger/README.md"),
            "# How to add an entry\n",
        )
        .unwrap();
        let files = read_ledger_at(&root).unwrap();
        fs::remove_dir_all(&root).ok();
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let titles: Vec<String> = files
            .iter()
            .flat_map(|text| ledger_sections(text))
            .map(|(t, _)| t)
            .collect();
        assert_eq!(
            titles,
            [
                "Ledger",
                "2026-10-20: old",
                "2026-10-21: a",
                "2026-10-23: approve looser filters/test/b"
            ]
        );

        // A tolerance_exception approved in an entry file resolves.
        let file = json!({ "module": "filter", "cases": { "b": { "num_samples": 10,
            "tolerances": { "residual_db": -70 },
            "tolerance_exception": { "approved_in": "2026-10-23: approve looser filters/test/b",
                                     "tolerances": { "residual_db": -70 } } } } });
        let cases = module_cases_from("filters", "test", "x.json", &file, &defaults(), &files, &[])
            .unwrap();
        assert_eq!(cases[0].tolerances.residual_db, Some(-70.0));
        let err = module_cases_from("filters", "test", "x.json", &file, &defaults(), LEDGER, &[])
            .unwrap_err();
        assert!(
            err.contains("migration/LEDGER.md or migration/ledger/"),
            "{err}"
        );
    }

    // Parity Reviewer on #50: "approved_in": "Ledger" was accepted, because
    // the "# Ledger" section spans the whole file (and then every entry).
    #[test]
    fn approved_in_must_be_a_dated_heading_within_one_file() {
        let case = |heading: &str| {
            json!({ "module": "filter", "cases": { "a": { "num_samples": 1,
                "tolerances": { "residual_db": -60 },
                "tolerance_exception": { "approved_in": heading, "tolerances": { "residual_db": -60 } } } } })
        };
        let resolve = |heading: &str, files: &[&str]| {
            module_cases_from(
                "filters",
                "test",
                "x.json",
                &case(heading),
                &defaults(),
                files,
                &[],
            )
        };
        let frozen = "# Ledger\n\nIntro.\n\n## 2026-10-20: old\n\nHistory.\n";
        let progress = "## 2026-10-22: comb filter work\n\nfilters/test/a measured -60 dB.\n";
        let err = resolve("Ledger", &[frozen, progress]).unwrap_err();
        assert!(err.contains("must cite a dated ledger heading"), "{err}");
        let repository = read_ledger().unwrap();
        let repository: Vec<&str> = repository.iter().map(String::as_str).collect();
        let err = resolve("Ledger", &repository).unwrap_err();
        assert!(err.contains("must cite a dated ledger heading"), "{err}");

        let undated = "## Approval for filters/test/a\n\nfilters/test/a approved.\n";
        let err = resolve("Approval for filters/test/a", &[frozen, undated]).unwrap_err();
        assert!(err.contains("must cite a dated ledger heading"), "{err}");

        // A dated level-1 heading cannot reach into the next file.
        let notes = "# 2026-10-22: notes\n\nNothing approved.\n";
        let err = resolve("2026-10-22: notes", &[frozen, notes, progress]).unwrap_err();
        assert!(err.contains("does not name the case"), "{err}");

        let approval =
            "## 2026-10-23: approve looser filters/test/a\n\nOwner approved filters/test/a.\n";
        let cases = resolve(
            "2026-10-23: approve looser filters/test/a",
            &[frozen, approval],
        )
        .unwrap();
        assert_eq!(cases[0].tolerances.residual_db, Some(-60.0));

        assert!(is_dated_heading("2026-10-21: x"));
        for title in [
            "Ledger",
            "2026-10-21 x",
            "2026-1-21: x",
            "x 2026-10-21: y",
            "2026-10-21:x",
        ] {
            assert!(!is_dated_heading(title), "{title}");
        }
    }

    // Parity Reviewer on #56: parks need a recorded approval and cannot cancel
    // the macOS justification of a macos-declared case.
    const PARK_KNOWN: [&str; 2] = ["filters/x/a", "filters/x/b"];
    const PARK_LEDGER: [&str; 3] = [
        "# Ledger\n\nfilters/x/a and filters/x/b are mentioned here.\n",
        "## 2026-10-21: owner approval — park\n\nfilters/x/a is parked on macOS.\n",
        "## Park approval\n\nfilters/x/b\n",
    ];

    fn park(heading: &str) -> Value {
        json!({ "approved_in": heading, "reason": "noisy (A11)" })
    }

    fn resolve_park_file(
        data: Value,
        allowlist: &[(&str, &[&str])],
    ) -> Result<BTreeMap<String, String>, String> {
        let known: Vec<String> = PARK_KNOWN.iter().map(|s| s.to_string()).collect();
        let allowlist = allowlist
            .iter()
            .map(|(id, targets)| {
                let targets = targets.iter().map(|t| t.to_string()).collect();
                (id.to_string(), ("SAS-35".to_string(), targets))
            })
            .collect();
        resolve_parked(&data, "parked_arm64.json", &known, &allowlist, &PARK_LEDGER)
    }

    fn resolve_parks(cases: Value) -> Result<BTreeMap<String, String>, String> {
        resolve_park_file(json!({ "description": "d", "cases": cases }), &[])
    }

    #[test]
    fn approved_park_resolves() {
        let parked =
            resolve_parks(json!({ "filters/x/a": park("2026-10-21: owner approval — park") }))
                .unwrap();
        assert_eq!(parked["filters/x/a"], "noisy (A11)");
        assert!(resolve_parks(json!({})).unwrap().is_empty());
    }

    #[test]
    fn park_needs_a_dated_heading_that_names_the_case() {
        for (id, heading, fragment) in [
            ("filters/x/a", "2026-10-22: nothing", "does not exist"),
            (
                "filters/x/b",
                "2026-10-21: owner approval — park",
                "does not name the case",
            ),
            (
                "filters/x/b",
                "Park approval",
                "must cite a dated ledger heading",
            ),
            ("filters/x/a", "Ledger", "must cite a dated ledger heading"),
        ] {
            let err = resolve_parks(json!({ id: park(heading) })).unwrap_err();
            assert!(err.contains(fragment), "{heading}: {err}");
        }
    }

    #[test]
    fn park_file_shape() {
        let good = park("2026-10-21: owner approval — park");
        let mut extra = good.clone();
        extra["extra"] = json!(1);
        for entry in [
            json!("noisy"),
            json!({ "approved_in": "2026-10-21: owner approval — park" }),
            extra,
            json!({ "approved_in": "2026-10-21: owner approval — park", "reason": "" }),
            json!({ "approved_in": "2026-10-21: owner approval — park", "reason": 3 }),
            json!({ "approved_in": null, "reason": "x" }),
        ] {
            let err = resolve_parks(json!({ "filters/x/a": entry })).unwrap_err();
            assert!(err.contains("must be {"), "{err}");
        }
        for data in [
            json!({ "cases": [], "description": "d" }),
            json!({ "description": "d" }),
            json!({ "cases": {}, "extra": 1 }),
            json!({ "cases": {}, "description": 3 }),
            json!([]),
        ] {
            let err = resolve_park_file(data.clone(), &[]).unwrap_err();
            assert!(err.contains("must be {"), "{data}: {err}");
        }
        let err = resolve_parks(json!({ "filters/x/gone": good })).unwrap_err();
        assert!(err.contains("is not a loaded case"), "{err}");
    }

    #[test]
    fn macos_declared_case_cannot_be_parked() {
        let data = json!({ "cases": { "filters/x/a": park("2026-10-21: owner approval — park") } });
        let err =
            resolve_park_file(data.clone(), &[("filters/x/a", &["x86", "macos"])]).unwrap_err();
        assert!(err.contains("declared for macos"), "{err}");
        assert!(resolve_park_file(data, &[("filters/x/a", &["x86"])]).is_ok());
    }

    #[test]
    fn repository_park_file_resolves() {
        load_parked(&corpus_dir().join("parked_arm64.json")).unwrap();
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
