//! Locating, validating and (when allowed) building reference captures.
//!
//! Captures are the render tree produced by
//! `migration/scripts/run_corpus.py render --out <dir>`:
//! - `SAS_PARITY_CAPTURES` points at it (absolute, or relative to the repo
//!   root); the default is `<repo>/build/parity`.
//! - If a capture is missing or stale, it is rendered on demand (building the
//!   C++ reference first if needed), unless `SAS_PARITY_NO_BUILD` is set, as
//!   it is in CI, where captures come from the reference job's artifact.
//! - `SAS_PARITY_REFRESH=1` re-renders every case once per process (use it
//!   after changing C++ module code).
//! - `"reference": "nofastmath"` cases read the `REF_FAST_MATH=OFF` build's
//!   tree instead: `SAS_PARITY_CAPTURES_NOFASTMATH` (default
//!   `<repo>/build/parity-nofastmath`), rendered on demand with a reference
//!   built into `migration/reference/build-nofastmath`. In CI it comes from
//!   the calibration job's `parity-captures-nofastmath` artifact.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::cases::{corpus_dir, module_hash_file, read_json, repo_root, Reference, ResolvedCase};

const IDENTITY_KEYS: [&str; 7] = [
    "upstream_sha",
    "compiler",
    "glibc",
    "fast_math",
    "opt_flags",
    "arch",
    "libm_variants",
];

/// The official reference's capture tree (`SAS_PARITY_CAPTURES`).
pub fn captures_dir() -> PathBuf {
    captures_dir_for(Reference::Official)
}

/// The capture tree of one reference build.
pub fn captures_dir_for(reference: Reference) -> PathBuf {
    let (variable, default) = match reference {
        Reference::Official => ("SAS_PARITY_CAPTURES", "build/parity"),
        Reference::NoFastMath => ("SAS_PARITY_CAPTURES_NOFASTMATH", "build/parity-nofastmath"),
    };
    match std::env::var_os(variable) {
        Some(dir) if !dir.is_empty() => {
            let path = PathBuf::from(dir);
            if path.is_absolute() {
                path
            } else {
                repo_root().join(path)
            }
        }
        _ => repo_root().join(default),
    }
}

fn tree(case: &ResolvedCase) -> PathBuf {
    captures_dir_for(case.comparison.reference)
}

/// Directory of one case inside its capture tree.
pub fn case_dir(case: &ResolvedCase) -> PathBuf {
    match case.id.strip_prefix("render/") {
        Some(_) => tree(case).join("renders"),
        None => tree(case).join("modules").join(&case.id),
    }
}

fn case_json_path(case: &ResolvedCase) -> PathBuf {
    match case.id.strip_prefix("render/") {
        Some(name) => tree(case).join("renders").join(format!("{name}.case.json")),
        None => case_dir(case).join("case.json"),
    }
}

/// The metadata file the reference wrote next to the data.
pub fn metadata_path(case: &ResolvedCase) -> PathBuf {
    match case.id.strip_prefix("render/") {
        Some(name) => tree(case).join("renders").join(format!("{name}.wav.json")),
        None => case_dir(case).join("capture.json"),
    }
}

fn data_present(case: &ResolvedCase) -> bool {
    let data = match case.id.strip_prefix("render/") {
        Some(name) => tree(case)
            .join("renders")
            .join(format!("{name}.wav"))
            .is_file(),
        None => true,
    };
    data && metadata_path(case).is_file()
}

enum State {
    Fresh,
    Missing,
    /// Present, but rendered from a different resolution of the case files.
    Stale(String),
}

fn state(case: &ResolvedCase) -> State {
    if !data_present(case) {
        return State::Missing;
    }
    let path = case_json_path(case);
    let Ok(text) = fs::read_to_string(&path) else {
        return State::Missing;
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(captured) if captured == case.json => State::Fresh,
        Ok(captured) => State::Stale(format!(
            "capture resolved as\n{}\nbut the case files now resolve to\n{}",
            serde_json::to_string_pretty(&captured).unwrap(),
            serde_json::to_string_pretty(&case.json).unwrap()
        )),
        Err(e) => State::Stale(format!("{}: {e}", path.display())),
    }
}

fn no_build() -> bool {
    std::env::var_os("SAS_PARITY_NO_BUILD").is_some_and(|v| !v.is_empty() && v != "0")
}

fn refresh_requested() -> bool {
    std::env::var_os("SAS_PARITY_REFRESH").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Serialises rendering within this process (tests run on threads) and
/// remembers which cases were already refreshed.
fn process_lock() -> &'static Mutex<HashSet<String>> {
    static LOCK: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Cross-process lock (several test binaries may render at once): an
/// atomically created directory holding the owner's pid, removed on drop. A
/// lock left behind by a killed run (owner no longer alive) is taken over.
struct DirLock(PathBuf);

fn process_alive(pid: u32) -> bool {
    if Path::new("/proc").is_dir() {
        return Path::new("/proc").join(pid.to_string()).exists();
    }
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(true)
}

/// True if the lock at `path` belongs to a process that no longer exists (or
/// has no owner recorded and is older than a minute).
pub(crate) fn lock_is_stale(path: &Path) -> bool {
    match fs::read_to_string(path.join("pid"))
        .ok()
        .and_then(|p| p.trim().parse::<u32>().ok())
    {
        Some(pid) => pid != std::process::id() && !process_alive(pid),
        None => fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|t| t.elapsed().unwrap_or_default() > Duration::from_secs(60))
            .unwrap_or(false),
    }
}

impl DirLock {
    fn acquire(path: PathBuf) -> DirLock {
        let start = Instant::now();
        fs::create_dir_all(path.parent().unwrap()).ok();
        loop {
            match fs::create_dir(&path) {
                Ok(()) => {
                    fs::write(path.join("pid"), std::process::id().to_string()).ok();
                    return DirLock(path);
                }
                Err(_) if lock_is_stale(&path) => {
                    eprintln!("sas-parity: removing stale lock {}", path.display());
                    fs::remove_dir_all(&path).ok();
                }
                Err(_) if start.elapsed() < Duration::from_secs(30 * 60) => {
                    thread::sleep(Duration::from_millis(250))
                }
                Err(e) => panic!(
                    "sas-parity: could not take lock {} ({e}); remove it if no build is running",
                    path.display()
                ),
            }
        }
    }
}

impl Drop for DirLock {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn run(command: &mut Command, what: &str) {
    let output = command
        .current_dir(repo_root())
        .output()
        .unwrap_or_else(|e| panic!("sas-parity: could not run {what}: {e}"));
    if !output.status.success() {
        panic!(
            "sas-parity: {what} failed ({})\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn render(case: &ResolvedCase) {
    let root = repo_root();
    let reference = case.comparison.reference;
    let build = match reference {
        Reference::Official => root.join("migration/reference/build"),
        Reference::NoFastMath => root.join("migration/reference/build-nofastmath"),
    };
    if !build.join("ref-render").is_file() || !build.join("ref-module").is_file() {
        let mut command = Command::new(root.join("migration/scripts/build_reference.sh"));
        if reference == Reference::NoFastMath {
            command
                .env("REF_BUILD_DIR", &build)
                .arg("-DREF_FAST_MATH=OFF");
        }
        run(&mut command, "migration/scripts/build_reference.sh");
    }
    run(
        Command::new("python3")
            .arg(root.join("migration/scripts/run_corpus.py"))
            .arg("render")
            .arg("--out")
            .arg(tree(case))
            .arg("--ref-render")
            .arg(build.join("ref-render"))
            .arg("--ref-module")
            .arg(build.join("ref-module"))
            .arg("--only")
            .arg(&case.id),
        &format!("run_corpus.py render --only {}", case.id),
    );
}

fn instructions(case: &ResolvedCase) -> String {
    match case.comparison.reference {
        Reference::Official => format!(
            "build the reference and render captures with:\n  \
             migration/scripts/build_reference.sh\n  \
             python3 migration/scripts/run_corpus.py render --out {}\n\
             (or unset SAS_PARITY_NO_BUILD to let sas-parity do it)",
            tree(case).display()
        ),
        Reference::NoFastMath => format!(
            "{} is a \"reference\": \"nofastmath\" case; build the REF_FAST_MATH=OFF reference and \
             render its captures with:\n  \
             REF_BUILD_DIR=migration/reference/build-nofastmath migration/scripts/build_reference.sh -DREF_FAST_MATH=OFF\n  \
             python3 migration/scripts/run_corpus.py render --out {} \\\n    \
             --ref-render migration/reference/build-nofastmath/ref-render \\\n    \
             --ref-module migration/reference/build-nofastmath/ref-module\n\
             (CI: the parity-captures-nofastmath artifact; or unset SAS_PARITY_NO_BUILD)",
            case.id,
            tree(case).display()
        ),
    }
}

/// Makes sure a fresh capture exists for `case` and returns its directory.
pub fn ensure(case: &ResolvedCase) -> PathBuf {
    let mut refreshed = process_lock().lock().unwrap_or_else(|e| e.into_inner());
    let needs_refresh = refresh_requested() && !refreshed.contains(&case.id);
    let current = state(case);
    if no_build() {
        match current {
            State::Fresh => return case_dir(case),
            State::Missing => panic!(
                "sas-parity: no reference capture for {} in {}; {}",
                case.id,
                tree(case).display(),
                instructions(case)
            ),
            State::Stale(why) => panic!(
                "sas-parity: reference capture for {} is stale: {why}\n{}",
                case.id,
                instructions(case)
            ),
        }
    }
    if matches!(current, State::Fresh) && !needs_refresh {
        return case_dir(case);
    }

    let _lock = DirLock::acquire(tree(case).with_extension("lock"));
    if needs_refresh || !matches!(state(case), State::Fresh) {
        render(case);
    }
    refreshed.insert(case.id.clone());
    match state(case) {
        State::Fresh => case_dir(case),
        State::Missing => panic!(
            "sas-parity: rendering {} produced no capture in {}",
            case.id,
            tree(case).display()
        ),
        State::Stale(why) => panic!(
            "sas-parity: {} was just rendered but its case.json differs from sas-parity's \
             resolution; run_corpus.py and port/crates/parity/src/cases.rs disagree: {why}",
            case.id
        ),
    }
}

/// Where failing comparisons dump the port output and report.
pub fn failures_dir() -> PathBuf {
    match std::env::var_os("SAS_PARITY_FAILURES") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => repo_root().join("build/parity-failures"),
    }
}

pub fn path_display(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

/// The harness hash, identical to `harness::fnv1a64` in harness_io.cpp and
/// `fnv1a64` in run_corpus.py: FNV-1a 64 with offset basis
/// 1469598103934665603. That is not the standard basis (14695981039346656037,
/// one digit longer); it is kept so every committed hash stays valid.
pub fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash: u64 = 1469598103934665603;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

pub fn f32_bytes(data: &[f32]) -> Vec<u8> {
    data.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn build_identity(case: &ResolvedCase) -> Value {
    let path = metadata_path(case);
    let metadata = read_json(&path).unwrap_or_else(|e| panic!("sas-parity: {e}"));
    let build = &metadata["build"];
    Value::Object(
        IDENTITY_KEYS
            .iter()
            .map(|k| (k.to_string(), build.get(*k).cloned().unwrap_or(Value::Null)))
            .collect(),
    )
}

fn hash_entries(path: &Path) -> Vec<Value> {
    if !path.is_file() {
        return Vec::new();
    }
    read_json(path)
        .unwrap_or_else(|e| panic!("sas-parity: {e}"))
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn warned_unknown_identity() -> &'static Mutex<bool> {
    static WARNED: OnceLock<Mutex<bool>> = OnceLock::new();
    WARNED.get_or_init(|| Mutex::new(false))
}

/// Checks loaded capture data against the committed hash for this build
/// identity, so a test provably compares against the pinned reference.
/// `key` is the output name (`audio`, `artifact/saved.json`) for modules and
/// ignored for renders. Captures from a build identity with no committed
/// hashes are accepted locally (with a warning) but rejected in CI
/// (`SAS_PARITY_NO_BUILD`).
pub fn verify_committed_hash(case: &ResolvedCase, key: &str, digest: &str) {
    let identity = build_identity(case);
    let render_hashes = corpus_dir().join("reference_hashes.json");
    let nofastmath = case.comparison.reference == Reference::NoFastMath;
    if nofastmath && identity["fast_math"] != Value::Bool(false) {
        panic!(
            "sas-parity: {} is a \"reference\": \"nofastmath\" case, but the capture in {} was not \
             rendered by a REF_FAST_MATH=OFF build (fast_math: {})",
            case.id,
            tree(case).display(),
            identity["fast_math"]
        );
    }
    let (hash_file, hash_key) = match case.id.strip_prefix("render/") {
        Some(name) => (render_hashes.clone(), name.to_string()),
        None => (
            module_hash_file(case),
            format!("{}:{key}", case.id.rsplit('/').next().unwrap()),
        ),
    };
    let entries = hash_entries(&hash_file);
    if let Some(entry) = entries.iter().find(|e| e["identity"] == identity) {
        match entry["hashes"].get(&hash_key).and_then(Value::as_str) {
            Some(expected) if expected == digest => return,
            Some(expected) => panic!(
                "sas-parity: capture data for {} ({key}) hashes to {digest}, but {} pins {expected}. \
                 The capture is stale or corrupted: re-render it (SAS_PARITY_REFRESH=1).",
                case.id,
                path_display(&hash_file)
            ),
            None => panic!(
                "sas-parity: {} has no committed hash for {hash_key:?}; write it with \
                 `run_corpus.py hashes <CI-identity tree> --write`",
                path_display(&hash_file)
            ),
        }
    }
    let known_identities: Vec<Value> = hash_entries(&render_hashes)
        .iter()
        .map(|e| e["identity"].clone())
        .collect();
    let known = if nofastmath {
        nofastmath_identity_known(&identity, &known_identities)
    } else {
        known_identities.contains(&identity)
    };
    if known {
        let (tree_name, flag) = if nofastmath {
            ("parity-captures-nofastmath", " --reference nofastmath")
        } else {
            ("parity-captures", "")
        };
        panic!(
            "sas-parity: {} has no hashes for the CI build identity; write them from a CI-identity tree \
             (`run_corpus.py hashes <{tree_name}> --write{flag}`)",
            path_display(&hash_file)
        );
    }
    if no_build() {
        panic!(
            "sas-parity: captures in {} come from a build identity with no committed hashes:\n{}\n\
             CI must run a known build identity.",
            tree(case).display(),
            serde_json::to_string_pretty(&identity).unwrap()
        );
    }
    let mut warned = warned_unknown_identity()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if !*warned {
        eprintln!(
            "sas-parity: warning: local captures come from a build identity with no committed hashes; \
             capture data cannot be checked against the pinned reference on this machine"
        );
        *warned = true;
    }
}

/// A no-fast-math identity is known when it is a CI identity with only the
/// fast-math setting (and so the optimisation flags) changed. Mirrored in
/// run_corpus.py (`nofastmath_identity_known`).
pub(crate) fn nofastmath_identity_known(identity: &Value, known: &[Value]) -> bool {
    if identity["fast_math"] != Value::Bool(false) {
        return false;
    }
    let strip = |value: &Value| -> Value {
        let mut object = value.as_object().cloned().unwrap_or_default();
        object.remove("fast_math");
        object.remove("opt_flags");
        Value::Object(object)
    };
    known
        .iter()
        .any(|k| k["fast_math"] == Value::Bool(true) && strip(k) == strip(identity))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dead_pid() -> u32 {
        let mut child = Command::new("true").spawn().expect("spawn true");
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    // N5 regression: a killed run left build/parity.lock behind and the next
    // run waited 30 minutes.
    #[test]
    fn lock_left_by_a_dead_process_is_taken_over() {
        let dir = std::env::temp_dir().join(format!("sas-parity-lock-test-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("pid"), dead_pid().to_string()).unwrap();
        assert!(lock_is_stale(&dir));

        let start = Instant::now();
        let lock = DirLock::acquire(dir.clone());
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(
            fs::read_to_string(dir.join("pid")).unwrap(),
            std::process::id().to_string()
        );
        assert!(!lock_is_stale(&dir), "a live owner's lock is not stale");
        drop(lock);
        assert!(!dir.exists());
    }

    #[test]
    fn nofastmath_identity_is_a_ci_identity_with_only_fast_math_changed() {
        let ci = serde_json::json!({ "compiler": "13.3.0", "fast_math": true, "opt_flags": "-O3 -ffast-math" });
        let proxy =
            serde_json::json!({ "compiler": "13.3.0", "fast_math": false, "opt_flags": "-O3" });
        assert!(nofastmath_identity_known(&proxy, std::slice::from_ref(&ci)));
        let other =
            serde_json::json!({ "compiler": "14.2.0", "fast_math": false, "opt_flags": "-O3" });
        assert!(!nofastmath_identity_known(
            &other,
            std::slice::from_ref(&ci)
        ));
        assert!(!nofastmath_identity_known(&ci, std::slice::from_ref(&ci)));
    }

    // Pinned vectors of the harness hash (see fnv1a64); run_corpus.py's
    // tests pin the same values.
    #[test]
    fn fnv_matches_the_harness_variant() {
        assert_eq!(fnv1a64(b""), "14650fb0739d0383");
        assert_eq!(fnv1a64(b"a"), "44bd8ad473cd9906");
    }
}
