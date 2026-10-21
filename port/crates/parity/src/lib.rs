//! Parity test helper: compare port output against the C++ reference.
//!
//! Every comparison uses the tolerances of the case as resolved from
//! `migration/corpus` (`cases.json` and `modules/<area>/<module>.json`). There
//! is deliberately no way to pass or override tolerances from a test
//! (migration/AGENTS.md rule 6).
//!
//! Case ids:
//! - module cases: `"<area>/<module>/<case>"`, optionally `":<output>"` to
//!   pick an output when the module has more than one
//!   (e.g. `"modulation/envelope/adsr:value"`);
//! - full-patch renders: `"render/<name>"`.
//!
//! ```ignore
//! use sas_parity::{assert_parity, ModuleCase};
//!
//! #[test]
//! fn digital_svf_lowpass_saw_sweep() {
//!     let case = ModuleCase::load("filters/digital_svf/lowpass_saw_sweep");
//!     let audio = case.input("audio");            // Vec<[f32; 4]>, poly lanes
//!     let cutoff = case.input("midi_cutoff");
//!     let output: Vec<[f32; 4]> = run_my_port(&case, &audio, &cutoff);
//!     assert_parity!("filters/digital_svf/lowpass_saw_sweep", output);
//! }
//! ```
//!
//! Captures are located (or rendered on demand) as described in
//! [`captures`]; see migration/AGENTS.md for the workflow.

pub mod captures;
pub mod cases;
pub mod json_bound;

use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

pub use audio_diff::{Audio, Report, Tolerances};
use serde_json::Value;

use cases::{ArtifactMode, Comparison, Reference, ResolvedCase};

/// Asserts that `output` matches the reference capture of case `id` within
/// the case's tolerances. Panics with the metrics on failure and dumps the
/// port output and report under `build/parity-failures/`.
///
/// `output` can be `Vec<[f32; 4]>` / `&[[f32; 4]]` (module cases: one entry
/// per frame, poly lanes voice 0 L, voice 0 R, voice 1 L, voice 1 R),
/// `Vec<[f32; 2]>` (stereo renders), an interleaved `Vec<f32>` / `&[f32]`
/// (channel count taken from the reference), or an [`Audio`].
/// Asserts that `actual` (`&str`, `String`, `&[u8]`, `Vec<u8>`) matches text
/// artifact `name` of module case `id`, e.g.
/// `assert_artifact!("wavetable/creator/init", "saved.json", json)`. The
/// mode comes from the case file only: byte for byte by default, or under
/// the owner-approved JSON bound if the case declares
/// `"artifacts": {"<name>": "json"}` (see [`json_bound`]).
#[macro_export]
macro_rules! assert_artifact {
    ($id:expr, $name:expr, $actual:expr $(,)?) => {
        $crate::ModuleCase::load($id).assert_artifact($name, &$actual)
    };
}

#[macro_export]
macro_rules! assert_parity {
    ($id:expr, $output:expr $(,)?) => {
        $crate::assert_parity($id, &$output)
    };
}

/// Function form of [`assert_parity!`].
#[track_caller]
pub fn assert_parity<O: ParityOutput + ?Sized>(id: &str, output: &O) {
    match parse_id(id) {
        ParsedId::Render => RenderCase::load(id).assert_parity(output),
        ParsedId::Module { case, output: name } => {
            let case = ModuleCase::load(&case);
            let name = name.unwrap_or_else(|| case.single_output_name());
            case.assert_parity(&name, output)
        }
    }
}

/// Debugging only: like [`assert_parity`], but returns the report instead of
/// panicking. Parity tests must assert with [`assert_parity!`]; reviewers
/// treat thresholds applied to this report in tests as blocking.
#[doc(hidden)]
pub fn parity_report<O: ParityOutput + ?Sized>(id: &str, output: &O) -> Report {
    match parse_id(id) {
        ParsedId::Render => RenderCase::load(id).report(output),
        ParsedId::Module { case, output: name } => {
            let case = ModuleCase::load(&case);
            let name = name.unwrap_or_else(|| case.single_output_name());
            case.report(&name, output)
        }
    }
}

/// Every case id in the corpus (module cases, then renders), minus the cases
/// parked on this reference (see [`parked_case_ids`]).
pub fn all_case_ids() -> Vec<String> {
    let modules = cases::load_module_cases().unwrap_or_else(|e| panic!("sas-parity: {e}"));
    let renders = cases::load_render_cases().unwrap_or_else(|e| panic!("sas-parity: {e}"));
    let parked = parked_case_ids();
    modules
        .into_iter()
        .chain(renders)
        .map(|c| c.id)
        .filter(|id| {
            let skip = parked.contains(id);
            if skip {
                eprintln!("sas-parity: {id} is parked on this reference (SAS_PARITY_PARKED)");
            }
            !skip
        })
        .collect()
}

/// Case ids parked on one reference only: the keys of `"cases"` in the JSON
/// file named by `SAS_PARITY_PARKED`. Only the macOS arm64 job sets it
/// (`migration/corpus/parked_arm64.json`); elsewhere every case runs.
pub fn parked_case_ids() -> Vec<String> {
    let Some(path) = std::env::var_os("SAS_PARITY_PARKED").filter(|v| !v.is_empty()) else {
        return Vec::new();
    };
    let path = PathBuf::from(path);
    let value =
        cases::read_json(&path).unwrap_or_else(|e| panic!("sas-parity: SAS_PARITY_PARKED: {e}"));
    let parked = value
        .get("cases")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("sas-parity: {}: missing \"cases\" object", path.display()));
    parked.keys().cloned().collect()
}

enum ParsedId {
    Render,
    Module {
        case: String,
        output: Option<String>,
    },
}

fn parse_id(id: &str) -> ParsedId {
    if id.starts_with("render/") {
        return ParsedId::Render;
    }
    let (case, output) = match id.split_once(':') {
        Some((case, output)) => (case.to_string(), Some(output.to_string())),
        None => (id.to_string(), None),
    };
    if case.split('/').count() != 3 {
        panic!(
            "sas-parity: case id {id:?} must be \"<area>/<module>/<case>[:<output>]\" or \"render/<name>\""
        );
    }
    ParsedId::Module { case, output }
}

/// Values a test can hand to the comparison.
pub trait ParityOutput {
    /// Interleaved samples.
    fn samples(&self) -> Cow<'_, [f32]>;
    /// Channel count, or `None` to use the reference's.
    fn channels(&self) -> Option<usize>;
}

impl<const N: usize> ParityOutput for [[f32; N]] {
    fn samples(&self) -> Cow<'_, [f32]> {
        Cow::Owned(self.iter().flatten().copied().collect())
    }
    fn channels(&self) -> Option<usize> {
        Some(N)
    }
}

impl<const N: usize> ParityOutput for Vec<[f32; N]> {
    fn samples(&self) -> Cow<'_, [f32]> {
        self.as_slice().samples()
    }
    fn channels(&self) -> Option<usize> {
        Some(N)
    }
}

impl ParityOutput for [f32] {
    fn samples(&self) -> Cow<'_, [f32]> {
        Cow::Borrowed(self)
    }
    fn channels(&self) -> Option<usize> {
        None
    }
}

impl ParityOutput for Vec<f32> {
    fn samples(&self) -> Cow<'_, [f32]> {
        Cow::Borrowed(self)
    }
    fn channels(&self) -> Option<usize> {
        None
    }
}

impl ParityOutput for Audio {
    fn samples(&self) -> Cow<'_, [f32]> {
        Cow::Borrowed(&self.data)
    }
    fn channels(&self) -> Option<usize> {
        Some(self.channels)
    }
}

impl<T: ParityOutput + ?Sized> ParityOutput for &T {
    fn samples(&self) -> Cow<'_, [f32]> {
        (**self).samples()
    }
    fn channels(&self) -> Option<usize> {
        (**self).channels()
    }
}

fn to_audio<O: ParityOutput + ?Sized>(output: &O, reference: &Audio, label: &str) -> Audio {
    let channels = output.channels().unwrap_or(reference.channels);
    let samples = output.samples().into_owned();
    if channels == 0 || !samples.len().is_multiple_of(channels) {
        panic!(
            "sas-parity: {label}: {} samples do not divide into {channels} channels",
            samples.len()
        );
    }
    Audio::new(channels, reference.sample_rate, samples)
}

fn describe_mismatch(label: &str, expected: &[u8], actual: &[u8], dumped: &Path) -> String {
    let offset = expected
        .iter()
        .zip(actual)
        .position(|(a, b)| a != b)
        .unwrap_or(expected.len().min(actual.len()));
    let line = expected[..offset].iter().filter(|&&b| b == b'\n').count() + 1;
    let line_start = expected[..offset]
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |p| p + 1);
    let snippet = |bytes: &[u8]| {
        let end = bytes[line_start.min(bytes.len())..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |p| line_start + p);
        String::from_utf8_lossy(
            &bytes[line_start.min(bytes.len())..end.max(line_start.min(bytes.len()))],
        )
        .to_string()
    };
    format!(
        "artifact parity FAILED for {label} (exact match required)\n  \
         reference {} bytes / port {} bytes, first difference at byte {offset} (line {line}, column {})\n  \
         reference: {:?}\n  port:      {:?}\n  port output: {}",
        expected.len(),
        actual.len(),
        offset - line_start + 1,
        snippet(expected),
        snippet(actual),
        captures::path_display(dumped)
    )
}

fn compare_or_panic(
    label: &str,
    reference: &Audio,
    test: &Audio,
    tolerances: &Tolerances,
) -> Report {
    audio_diff::compare(reference, test, tolerances, &audio_diff::Options::default())
        .unwrap_or_else(|e| panic!("sas-parity: {label}: cannot compare with the reference: {e}"))
}

fn fail(label: &str, source: &str, test: &Audio, report: &Report) -> ! {
    let dir = captures::failures_dir().join(label.replace(':', "__"));
    fs::create_dir_all(&dir).ok();
    let dumped = dir.join("port_output.npy");
    fs::write(&dumped, audio_diff::audio::to_npy(test)).ok();
    fs::write(
        dir.join("report.json"),
        serde_json::to_string_pretty(report).unwrap_or_default(),
    )
    .ok();

    let mut text = format!(
        "parity FAILED for {label} (tolerances from {source})\n  frames: reference {} / port {}\n",
        report.frames_ref, report.frames_test
    );
    for check in &report.checks {
        text.push_str(&format!(
            "  [{}] {} = {} (limit {})\n",
            if check.pass { "pass" } else { "FAIL" },
            check.metric,
            check.value,
            check.limit
        ));
    }
    if report.frames_ref != report.frames_test {
        text.push_str("  [FAIL] length differs\n");
    }
    text.push_str(&format!(
        "  port output: {}\n  report: {}\n",
        captures::path_display(&dumped),
        captures::path_display(&dir.join("report.json"))
    ));
    panic!("{text}");
}

/// Bit-for-bit comparison for `"reference": "nofastmath"` cases.
#[track_caller]
fn assert_bit_exact(label: &str, reference: &Audio, test: &Audio) {
    let same_shape = reference.channels == test.channels && reference.frames == test.frames;
    let differing: Vec<usize> = reference
        .data
        .iter()
        .zip(&test.data)
        .enumerate()
        .filter(|(_, (a, b))| a.to_bits() != b.to_bits())
        .map(|(i, _)| i)
        .collect();
    if same_shape && differing.is_empty() {
        return;
    }
    let dir = captures::failures_dir().join(label.replace(':', "__"));
    fs::create_dir_all(&dir).ok();
    let dumped = dir.join("port_output.npy");
    fs::write(&dumped, audio_diff::audio::to_npy(test)).ok();
    let mut text = format!(
        "parity FAILED for {label} (reference \"nofastmath\": bit-exact against the \
         REF_FAST_MATH=OFF capture)\n  shape: reference {} frames x {} channels / port {} x {}\n",
        reference.frames, reference.channels, test.frames, test.channels
    );
    if let Some(&first) = differing.first() {
        let (a, b) = (reference.data[first], test.data[first]);
        text.push_str(&format!(
            "  {} values differ; first at index {first} (frame {}, channel {}): reference {a:?} \
             ({:#010x}), port {b:?} ({:#010x})\n",
            differing.len(),
            first / reference.channels.max(1),
            first % reference.channels.max(1),
            a.to_bits(),
            b.to_bits()
        ));
    }
    text.push_str(&format!(
        "  port output: {}",
        captures::path_display(&dumped)
    ));
    panic!("{text}");
}

fn to_frames(audio: Audio, what: &str) -> Vec<[f32; 4]> {
    if audio.channels != 4 {
        panic!(
            "sas-parity: {what} has {} columns, expected 4 poly lanes",
            audio.channels
        );
    }
    audio.data.as_chunks::<4>().0.to_vec()
}

/// Upstream `vital::VoiceEvent` values, as used on trigger inputs.
pub mod voice_event {
    pub const INVALID: f32 = 0.0;
    pub const IDLE: f32 = 1.0;
    pub const ON: f32 = 2.0;
    pub const HOLD: f32 = 3.0;
    pub const DECAY: f32 = 4.0;
    pub const OFF: f32 = 5.0;
    pub const KILL: f32 = 6.0;
}

/// One event on a trigger input (`Output::trigger(mask, value, offset)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trigger {
    /// Absolute frame index.
    pub sample: usize,
    /// Trigger value, e.g. [`voice_event::ON`].
    pub value: f32,
    /// Lanes the trigger applies to (voice 0 L/R, voice 1 L/R).
    pub lanes: [bool; 4],
}

/// A single-processor case: its spec, inputs, reference outputs, tolerances.
#[derive(Debug, Clone)]
pub struct ModuleCase {
    resolved: ResolvedCase,
    dir: PathBuf,
}

impl ModuleCase {
    /// Loads `"<area>/<module>/<case>"`, rendering the capture if needed.
    pub fn load(id: &str) -> ModuleCase {
        let id = id.split_once(':').map_or(id, |(case, _)| case);
        let resolved = cases::load_module_cases()
            .unwrap_or_else(|e| panic!("sas-parity: {e}"))
            .into_iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| {
                panic!(
                    "sas-parity: no module case {id:?} in migration/corpus/modules \
                     (ids are \"<area>/<module>/<case>\")"
                )
            });
        let dir = captures::ensure(&resolved);
        ModuleCase { resolved, dir }
    }

    pub fn id(&self) -> &str {
        &self.resolved.id
    }

    /// The case file this case came from, relative to the repo root.
    pub fn file(&self) -> &str {
        self.resolved.json["file"].as_str().unwrap_or_default()
    }

    /// The resolved ref-module spec (module, params, rates, inputs, ...).
    pub fn spec(&self) -> &Value {
        &self.resolved.json["spec"]
    }

    /// ref-module registry name, e.g. `"filter"`.
    pub fn module(&self) -> &str {
        self.spec()["module"].as_str().unwrap_or_default()
    }

    /// The spec's `params` object (`Value::Null` if none).
    pub fn params(&self) -> &Value {
        &self.spec()["params"]
    }

    fn spec_u64(&self, key: &str, default: u64) -> u64 {
        self.spec()
            .get(key)
            .and_then(Value::as_u64)
            .unwrap_or(default)
    }

    /// Base sample rate (before oversampling).
    pub fn sample_rate(&self) -> u32 {
        self.spec_u64("sample_rate", 44100) as u32
    }

    pub fn oversample(&self) -> u32 {
        self.spec_u64("oversample", 1) as u32
    }

    /// Rate the processor runs at: `sample_rate * oversample`. Frame counts,
    /// block sizes and trigger positions are all in this rate.
    pub fn internal_sample_rate(&self) -> u32 {
        self.sample_rate() * self.oversample()
    }

    /// Frames per `process()` call (the last block may be shorter).
    pub fn block_size(&self) -> usize {
        self.spec_u64("block_size", 64) as usize
    }

    pub fn num_samples(&self) -> usize {
        self.spec_u64("num_samples", 0) as usize
    }

    /// The tolerances this case is held to (read-only).
    pub fn tolerances(&self) -> &Tolerances {
        &self.resolved.tolerances
    }

    fn names(&self, prefix: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.strip_prefix(prefix)
                            .and_then(|n| n.strip_suffix(".npy"))
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    /// Names of every input the reference fed the processor.
    pub fn input_names(&self) -> Vec<String> {
        self.names("in_")
    }

    /// Names of the reference outputs.
    pub fn output_names(&self) -> Vec<String> {
        self.names("out_")
    }

    fn single_output_name(&self) -> String {
        let outputs = self.output_names();
        match outputs.as_slice() {
            [only] => only.clone(),
            _ => panic!(
                "sas-parity: {} has outputs {outputs:?}; pick one with \"{}:<output>\"",
                self.id(),
                self.id()
            ),
        }
    }

    fn capture_json(&self) -> Value {
        cases::read_json(&self.dir.join("capture.json"))
            .unwrap_or_else(|e| panic!("sas-parity: {e}"))
    }

    /// Loads `<prefix><name>.npy` and verifies its data: outputs against the
    /// committed hashes, inputs against capture.json.
    fn npy(&self, prefix: &str, name: &str) -> Audio {
        let path = self.dir.join(format!("{prefix}{name}.npy"));
        if !path.is_file() {
            let available = self.names(prefix);
            panic!(
                "sas-parity: {} has no {} {name:?} (available: {available:?})",
                self.id(),
                if prefix == "in_" { "input" } else { "output" }
            );
        }
        let audio =
            audio_diff::audio::load_npy(&path).unwrap_or_else(|e| panic!("sas-parity: {e}"));
        let digest = captures::fnv1a64(&captures::f32_bytes(&audio.data));
        if prefix == "out_" {
            captures::verify_committed_hash(&self.resolved, name, &digest);
        } else {
            let recorded = self.capture_json()["inputs"][name]["fnv1a64"].clone();
            if recorded.as_str() != Some(digest.as_str()) {
                panic!(
                    "sas-parity: {} input {name:?} does not match its capture.json hash; re-render it",
                    self.id()
                );
            }
        }
        audio
    }

    /// The exact per-frame values the reference fed to input `name`
    /// (control-rate inputs are held for each block, as captured).
    pub fn input(&self, name: &str) -> Vec<[f32; 4]> {
        to_frames(
            self.npy("in_", name),
            &format!("{} input {name}", self.id()),
        )
    }

    /// Trigger events on input `name` (empty if it is not a trigger input).
    pub fn triggers(&self, name: &str) -> Vec<Trigger> {
        let Some(events) = self.spec()["inputs"][name]["trigger"].as_array() else {
            return Vec::new();
        };
        events
            .iter()
            .map(|event| {
                let value = match &event["value"] {
                    Value::Null => voice_event::ON,
                    Value::Number(n) => n.as_f64().unwrap() as f32,
                    Value::String(s) => match s.as_str() {
                        "voice_on" => voice_event::ON,
                        "voice_off" => voice_event::OFF,
                        "voice_kill" => voice_event::KILL,
                        "voice_idle" => voice_event::IDLE,
                        "voice_hold" => voice_event::HOLD,
                        "voice_decay" => voice_event::DECAY,
                        other => panic!("sas-parity: unknown trigger value {other:?}"),
                    },
                    other => panic!("sas-parity: bad trigger value {other}"),
                };
                let lanes = match event["voices"].as_str().unwrap_or("all") {
                    "0" => [true, true, false, false],
                    "1" => [false, false, true, true],
                    _ => [true; 4],
                };
                Trigger {
                    sample: event["sample"].as_u64().unwrap_or(0) as usize,
                    value,
                    lanes,
                }
            })
            .collect()
    }

    /// The reference output `name` (processor outputs: 4 poly lanes).
    pub fn reference(&self, name: &str) -> Vec<[f32; 4]> {
        to_frames(
            self.npy("out_", name),
            &format!("{} output {name}", self.id()),
        )
    }

    /// The reference output `name` with whatever channel count it has (e.g.
    /// arrays emitted by an artifact hook).
    pub fn reference_audio(&self, name: &str) -> Audio {
        self.npy("out_", name)
    }

    /// Names of the exact-match text artifacts (`artifact_<name>` files).
    pub fn artifact_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.capture_json()["artifacts"]
            .as_object()
            .map(|a| a.keys().cloned().collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    /// The raw bytes of artifact `name`, verified against the committed hash.
    pub fn artifact_bytes(&self, name: &str) -> Vec<u8> {
        let path = self.dir.join(format!("artifact_{name}"));
        let bytes = fs::read(&path).unwrap_or_else(|_| {
            panic!(
                "sas-parity: {} has no artifact {name:?} (available: {:?})",
                self.id(),
                self.artifact_names()
            )
        });
        captures::verify_committed_hash(
            &self.resolved,
            &format!("artifact/{name}"),
            &captures::fnv1a64(&bytes),
        );
        bytes
    }

    /// Artifact `name` as UTF-8 text.
    pub fn artifact(&self, name: &str) -> String {
        String::from_utf8(self.artifact_bytes(name))
            .unwrap_or_else(|_| panic!("sas-parity: {} artifact {name:?} is not UTF-8", self.id()))
    }

    /// How this case is compared: reference build, artifact modes, JSON
    /// bound and base64-f32 keys (read-only, from the case file).
    pub fn comparison(&self) -> &Comparison {
        &self.resolved.comparison
    }

    /// Asserts that `actual` matches artifact `name`. By default the match is
    /// byte for byte, with no normalisation (whitespace, key order and
    /// number formatting all count). An artifact the case declares `"json"`
    /// is compared under the owner-approved JSON bound instead (see
    /// [`json_bound`]); a `"reference": "nofastmath"` case is always exact.
    #[track_caller]
    pub fn assert_artifact<A: AsRef<[u8]> + ?Sized>(&self, name: &str, actual: &A) {
        let expected = self.artifact_bytes(name);
        let actual = actual.as_ref();
        if expected == actual {
            return;
        }
        let comparison = self.comparison();
        let label = format!("{}:artifact/{name}", self.id());
        let dir = captures::failures_dir().join(label.replace(':', "__"));
        fs::create_dir_all(&dir).ok();
        let dumped = dir.join(format!("port_{name}"));
        fs::write(&dumped, actual).ok();
        fs::write(dir.join(format!("reference_{name}")), &expected).ok();
        if comparison.artifact_mode(name) == ArtifactMode::Json {
            let bound = comparison.json_bound;
            let problems =
                json_bound::problems(&expected, actual, &bound, &comparison.base64_f32_keys);
            if problems.is_empty() {
                fs::remove_dir_all(&dir).ok();
                return;
            }
            let mut text = format!(
                "artifact parity FAILED for {label} (JSON bound: {} f32 ulp or {} absolute; \
                 base64 f32 keys {:?})\n",
                bound.ulp, bound.abs, comparison.base64_f32_keys
            );
            for problem in problems.iter().take(10) {
                text.push_str(&format!("  {problem}\n"));
            }
            if problems.len() > 10 {
                text.push_str(&format!("  ... {} more\n", problems.len() - 10));
            }
            text.push_str(&format!(
                "  port output: {}",
                captures::path_display(&dumped)
            ));
            panic!("{text}");
        }
        panic!("{}", describe_mismatch(&label, &expected, actual, &dumped));
    }

    /// Debugging only: compares without panicking. Parity tests must use
    /// [`ModuleCase::assert_parity`] / [`assert_parity!`].
    #[doc(hidden)]
    pub fn report<O: ParityOutput + ?Sized>(&self, output: &str, data: &O) -> Report {
        let label = format!("{}:{output}", self.id());
        let reference = self.reference_audio(output);
        let test = to_audio(data, &reference, &label);
        let exact = Tolerances {
            max_abs_error: Some(0.0),
            ..Tolerances::default()
        };
        let tolerances = match self.comparison().reference {
            Reference::Official => self.tolerances(),
            Reference::NoFastMath => &exact,
        };
        compare_or_panic(&label, &reference, &test, tolerances)
    }

    /// Asserts parity of output `output`: with the case's tolerances, or
    /// bit for bit against the no-fast-math capture for a
    /// `"reference": "nofastmath"` case.
    #[track_caller]
    pub fn assert_parity<O: ParityOutput + ?Sized>(&self, output: &str, data: &O) {
        let label = format!("{}:{output}", self.id());
        let reference = self.reference_audio(output);
        let test = to_audio(data, &reference, &label);
        if self.comparison().reference == Reference::NoFastMath {
            assert_bit_exact(&label, &reference, &test);
            return;
        }
        let report = compare_or_panic(&label, &reference, &test, self.tolerances());
        if !report.pass {
            fail(&label, self.file(), &test, &report);
        }
    }
}

/// A full-patch render case (preset + MIDI through the whole engine).
#[derive(Debug, Clone)]
pub struct RenderCase {
    resolved: ResolvedCase,
    wav: PathBuf,
}

impl RenderCase {
    /// Loads `"render/<name>"`, rendering the capture if needed.
    pub fn load(id: &str) -> RenderCase {
        let resolved = cases::load_render_cases()
            .unwrap_or_else(|e| panic!("sas-parity: {e}"))
            .into_iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| {
                panic!("sas-parity: no render case {id:?} in migration/corpus/cases.json")
            });
        let dir = captures::ensure(&resolved);
        let name = id.strip_prefix("render/").unwrap();
        RenderCase {
            wav: dir.join(format!("{name}.wav")),
            resolved,
        }
    }

    pub fn id(&self) -> &str {
        &self.resolved.id
    }

    /// The resolved case (preset, midi, sample_rate, block_size, bpm, seed,
    /// tail or length, tolerances).
    pub fn settings(&self) -> &Value {
        &self.resolved.json
    }

    fn corpus_path(&self, key: &str) -> PathBuf {
        cases::corpus_dir().join(self.settings()[key].as_str().unwrap_or_default())
    }

    pub fn preset_path(&self) -> PathBuf {
        self.corpus_path("preset")
    }

    pub fn midi_path(&self) -> PathBuf {
        self.corpus_path("midi")
    }

    pub fn sample_rate(&self) -> u32 {
        self.settings()["sample_rate"].as_u64().unwrap_or(44100) as u32
    }

    pub fn block_size(&self) -> usize {
        self.settings()["block_size"].as_u64().unwrap_or(64) as usize
    }

    pub fn bpm(&self) -> f64 {
        self.settings()["bpm"].as_f64().unwrap_or(120.0)
    }

    pub fn seed(&self) -> Option<i64> {
        self.settings()["seed"].as_i64()
    }

    /// Frames in the reference render (render exactly this many).
    pub fn frames(&self) -> usize {
        self.reference().frames
    }

    /// The tolerances this case is held to (read-only).
    pub fn tolerances(&self) -> &Tolerances {
        &self.resolved.tolerances
    }

    /// The reference render (stereo, interleaved), verified against the
    /// committed hash.
    pub fn reference(&self) -> Audio {
        let audio = audio_diff::load(&self.wav).unwrap_or_else(|e| panic!("sas-parity: {e}"));
        captures::verify_committed_hash(
            &self.resolved,
            "",
            &captures::fnv1a64(&captures::f32_bytes(&audio.data)),
        );
        audio
    }

    /// Debugging only: compares without panicking. Parity tests must use
    /// [`RenderCase::assert_parity`] / [`assert_parity!`].
    #[doc(hidden)]
    pub fn report<O: ParityOutput + ?Sized>(&self, data: &O) -> Report {
        let reference = self.reference();
        let test = to_audio(data, &reference, self.id());
        compare_or_panic(self.id(), &reference, &test, self.tolerances())
    }

    /// Asserts parity with the case's tolerances.
    #[track_caller]
    pub fn assert_parity<O: ParityOutput + ?Sized>(&self, data: &O) {
        let reference = self.reference();
        let test = to_audio(data, &reference, self.id());
        let report = compare_or_panic(self.id(), &reference, &test, self.tolerances());
        if !report.pass {
            fail(self.id(), "migration/corpus/cases.json", &test, &report);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(data: &[f32]) -> Audio {
        Audio::new(2, None, data.to_vec())
    }

    #[test]
    fn nofastmath_comparison_is_bit_exact() {
        let reference = audio(&[0.5, -0.25, 0.0, 1.0]);
        assert_bit_exact("t:a", &reference, &audio(&[0.5, -0.25, 0.0, 1.0]));
        for port in [
            audio(&[0.5, -0.25, -0.0, 1.0]),
            audio(&[0.5, -0.25, 0.0, f32::from_bits(1.0f32.to_bits() + 1)]),
            audio(&[0.5, -0.25]),
        ] {
            let message = *std::panic::catch_unwind(|| assert_bit_exact("t:a", &reference, &port))
                .unwrap_err()
                .downcast::<String>()
                .unwrap();
            assert!(
                message.contains("bit-exact against the REF_FAST_MATH=OFF capture"),
                "{message}"
            );
        }
    }
}
