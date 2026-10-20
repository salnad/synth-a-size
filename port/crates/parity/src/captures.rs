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

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::cases::{repo_root, ResolvedCase};

pub fn captures_dir() -> PathBuf {
    match std::env::var_os("SAS_PARITY_CAPTURES") {
        Some(dir) if !dir.is_empty() => {
            let path = PathBuf::from(dir);
            if path.is_absolute() {
                path
            } else {
                repo_root().join(path)
            }
        }
        _ => repo_root().join("build/parity"),
    }
}

/// Directory of one case inside the capture tree.
pub fn case_dir(case: &ResolvedCase) -> PathBuf {
    match case.id.strip_prefix("render/") {
        Some(_) => captures_dir().join("renders"),
        None => captures_dir().join("modules").join(&case.id),
    }
}

fn case_json_path(case: &ResolvedCase) -> PathBuf {
    match case.id.strip_prefix("render/") {
        Some(name) => captures_dir()
            .join("renders")
            .join(format!("{name}.case.json")),
        None => case_dir(case).join("case.json"),
    }
}

fn data_present(case: &ResolvedCase) -> bool {
    match case.id.strip_prefix("render/") {
        Some(name) => captures_dir()
            .join("renders")
            .join(format!("{name}.wav"))
            .is_file(),
        None => fs::read_dir(case_dir(case))
            .map(|entries| {
                entries.flatten().any(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.starts_with("out_") && name.ends_with(".npy")
                })
            })
            .unwrap_or(false),
    }
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

/// Cross-process lock (several test binaries may render at once):
/// an atomically created directory, removed on drop.
struct DirLock(PathBuf);

impl DirLock {
    fn acquire(path: PathBuf) -> DirLock {
        let start = Instant::now();
        fs::create_dir_all(path.parent().unwrap()).ok();
        loop {
            match fs::create_dir(&path) {
                Ok(()) => return DirLock(path),
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
    let build = root.join("migration/reference/build");
    if !build.join("ref-render").is_file() || !build.join("ref-module").is_file() {
        run(
            &mut Command::new(root.join("migration/scripts/build_reference.sh")),
            "migration/scripts/build_reference.sh",
        );
    }
    run(
        Command::new("python3")
            .arg(root.join("migration/scripts/run_corpus.py"))
            .arg("render")
            .arg("--out")
            .arg(captures_dir())
            .arg("--only")
            .arg(&case.id),
        &format!("run_corpus.py render --only {}", case.id),
    );
}

fn instructions() -> String {
    format!(
        "build the reference and render captures with:\n  \
         migration/scripts/build_reference.sh\n  \
         python3 migration/scripts/run_corpus.py render --out {}\n\
         (or unset SAS_PARITY_NO_BUILD to let sas-parity do it)",
        captures_dir().display()
    )
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
                captures_dir().display(),
                instructions()
            ),
            State::Stale(why) => panic!(
                "sas-parity: reference capture for {} is stale: {why}\n{}",
                case.id,
                instructions()
            ),
        }
    }
    if matches!(current, State::Fresh) && !needs_refresh {
        return case_dir(case);
    }

    let _lock = DirLock::acquire(captures_dir().with_extension("lock"));
    if needs_refresh || !matches!(state(case), State::Fresh) {
        render(case);
    }
    refreshed.insert(case.id.clone());
    match state(case) {
        State::Fresh => case_dir(case),
        State::Missing => panic!(
            "sas-parity: rendering {} produced no capture in {}",
            case.id,
            captures_dir().display()
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
