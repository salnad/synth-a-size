#!/usr/bin/env python3
"""Render the golden corpus, compare render trees, and check output hashes.

Cases:
  renders  migration/corpus/cases.json "renders": preset + MIDI full-patch cases
           (id "render/<name>").
  modules  migration/corpus/modules/<area>/<module>.json: single-processor
           cases for ref-module (id "<area>/<module>/<case>"). One file per
           module, so parallel work never edits a shared list.

Commands:
  render   Render cases with the reference harness into OUT/:
             OUT/renders/<name>.wav (+ .wav.json metadata, .case.json)
             OUT/modules/<area>/<module>/<case>/{in_*,out_*}.npy,
               capture.json, spec.json, case.json
  compare  Compare two trees with audio-diff using each case's tolerances.
           --exact requires bit-identical output.
  hashes   --write stores output hashes (renders: corpus/reference_hashes.json,
           modules: corpus/modules/<area>/<module>.hashes.json), one entry per
           build identity. --check fails on any mismatch or missing hash for
           the current identity (cross-machine determinism tripwire).
  list     Print the resolved cases (ids, specs, tolerances) as JSON.

Tolerance rules (shared with port/crates/parity, which must resolve them the
same way; keep the two in sync):
- The approved floor is pinned below (RENDER_FLOOR, MODULE_FLOOR, from
  migration/decisions/reference-determinism.md). The defaults in cases.json
  must contain every floor metric at the floor or stricter. Changing the
  floor is a code change in both resolvers and needs recorded human approval.
- Case values layer over file-level values over the defaults. A metric looser
  than the default is an error unless the *case* (never the file) carries
  "tolerance_exception": {"approved_in": "<exact LEDGER.md heading>",
  "tolerances": {<metric>: <approved value>}} where every loosened metric
  equals its approved value, every listed metric is actually loosened, and the
  cited LEDGER.md section names the case id.
- JSON NaN/Infinity and unknown keys are rejected.

Comparison rules (owner decisions in
migration/decisions/owner-decisions-json-and-morphs.md; also mirrored in
port/crates/parity):
- Text artifacts are byte-exact unless the case (or its file) declares them
  "json": "artifacts": {"saved.json": "json"}. A json artifact must match
  structurally (keys, types, strings, integers, array lengths), and each
  float must agree within JSON_BOUND (4 f32 ulp, or 1e-6 absolute, whichever
  is looser); -0.0 equals 0.0. String values under the keys listed in
  "base64_f32_keys" are decoded as little-endian f32 arrays (same length,
  each element under the same bound); other strings, including PCM base64,
  stay exact. "json_bound": {"ulp": n, "abs": x} may tighten the bound,
  never loosen it.
- "reference": "nofastmath" (case-level, only for case ids listed in the
  decision doc's allowlist) compares every array and artifact bit for bit
  against the REF_FAST_MATH=OFF build's captures (compare
  --reference-nofastmath DIR; hashes --reference nofastmath). Such a case
  is only valid while the official reference fails the case's normal limits
  (the module defaults and its declared artifact modes) against the
  no-fast-math build: CI's calibration step checks this with
  `compare <official> <nofastmath> --justify-nofastmath`.

Exit status: 0 all pass, 1 any comparison or hash check failed, 2 usage,
case-file or tool error. Python 3.8+ standard library only.
"""

import argparse
import base64
import binascii
import glob
import json
import math
import os
import re
import struct
import subprocess
import sys

REPO = CORPUS = MODULE_CASES = RENDER_HASHES = LEDGER = DECISION_DOC = None


def set_repo(root):
    """Points every path at a repository root (tests use a scratch copy)."""
    global REPO, CORPUS, MODULE_CASES, RENDER_HASHES, LEDGER, DECISION_DOC
    REPO = os.path.abspath(root)
    CORPUS = os.path.join(REPO, "migration", "corpus")
    MODULE_CASES = os.path.join(CORPUS, "modules")
    RENDER_HASHES = os.path.join(CORPUS, "reference_hashes.json")
    LEDGER = os.path.join(REPO, "migration", "LEDGER.md")
    DECISION_DOC = os.path.join(REPO, "migration", "decisions", "owner-decisions-json-and-morphs.md")


set_repo(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
DEFAULT_RENDER = os.path.join(REPO, "migration", "reference", "build", "ref-render")
DEFAULT_MODULE = os.path.join(REPO, "migration", "reference", "build", "ref-module")
DEFAULT_DIFF = os.path.join(REPO, "migration", "tools", "audio-diff", "target", "release", "audio-diff")

METRICS = ("max_abs_error", "rms_diff_db", "residual_db", "lsd_db")
# Approved floors (migration/decisions/reference-determinism.md, owner approval
# in LEDGER.md 2026-10-20). Mirrored in port/crates/parity/src/cases.rs.
RENDER_FLOOR = {"residual_db": -50.0, "lsd_db": 0.1, "rms_diff_db": 0.01}
MODULE_FLOOR = {"max_abs_error": 1e-4, "residual_db": -80.0}

# Owner decision (migration/decisions/owner-decisions-json-and-morphs.md,
# LEDGER.md 2026-10-21). Mirrored in port/crates/parity/src/cases.rs.
JSON_BOUND = {"ulp": 4, "abs": 1e-6}
# The only keys a case may declare as base64 little-endian f32 data. Any
# other string (e.g. PCM int16 "audio_file") stays exact; extending this set
# is a code change in both resolvers plus an owner decision.
BASE64_F32_KEYS = ("wave_data",)
# The no-fast-math reference build's opt_flags are the CI build's minus this.
FAST_MATH_FLAGS = " -ffast-math -U__FAST_MATH__"
ARTIFACT_MODES = ("exact", "json")
REFERENCES = ("official", "nofastmath")
ALLOWLIST_HEADING = "No-fast-math exact cases (allowlist)"
ARTIFACT_NAME = re.compile(r"[A-Za-z0-9_.-]+")

SPEC_KEYS = ("module", "params", "sample_rate", "block_size", "oversample", "num_samples", "seed", "inputs")
COMPARISON_KEYS = ("artifacts", "base64_f32_keys", "json_bound")
CASE_KEYS = SPEC_KEYS + COMPARISON_KEYS + ("description", "tolerances", "tolerance_exception", "reference")
FILE_KEYS = SPEC_KEYS + COMPARISON_KEYS + ("description", "tolerances", "cases")
MODULE_DEFAULT_KEYS = ("comment", "sample_rate", "block_size", "oversample", "tolerances")
RENDER_SETTING_KEYS = ("sample_rate", "block_size", "tail", "length", "bpm", "seed")
RENDER_DEFAULT_KEYS = RENDER_SETTING_KEYS + ("tolerances",)
RENDER_CASE_KEYS = RENDER_SETTING_KEYS + ("name", "preset", "midi", "description", "tolerances",
                                          "tolerance_exception")
EXCEPTION_KEYS = ("approved_in", "tolerances")
IDENTITY_KEYS = ("upstream_sha", "compiler", "glibc", "fast_math", "opt_flags", "arch", "libm_variants")


class CaseError(Exception):
    pass


def _reject_constant(name):
    raise ValueError("%s is not allowed in corpus JSON" % name)


def loads_strict(text, where):
    try:
        return json.loads(text, parse_constant=_reject_constant)
    except ValueError as error:
        raise CaseError("%s: invalid JSON: %s" % (where, error))


def read_json_strict(path):
    with open(path) as handle:
        return loads_strict(handle.read(), os.path.relpath(path, REPO))


def read_ledger():
    with open(LEDGER) as handle:
        return handle.read()


def ledger_sections(text):
    """[(heading, body)] for every markdown heading outside code fences; the
    body runs to the next heading of the same or a higher level."""
    headings = []
    lines = text.splitlines()
    in_fence = False
    for index, line in enumerate(lines):
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence or not line.startswith("#"):
            continue
        level = len(line) - len(line.lstrip("#"))
        title = line[level:].strip()
        if title and line[level:level + 1] in (" ", "\t"):
            headings.append((index, level, title))
    sections = []
    for position, (index, level, title) in enumerate(headings):
        end = len(lines)
        for later_index, later_level, _ in headings[position + 1:]:
            if later_level <= level:
                end = later_index
                break
        sections.append((title, "\n".join(lines[index + 1:end])))
    return sections


ID_CHARS = re.compile(r"[A-Za-z0-9_/-]")
NAME = re.compile(r"[A-Za-z0-9_-]+")


def check_name(where, kind, name):
    """Area, module, case and render names are [A-Za-z0-9_-]+, so a case id
    can never extend another id across a token boundary (e.g. "a.b" or "a:x"
    containing an approval for "a"). Mirrored in sas-parity (cases.rs)."""
    if not isinstance(name, str) or not NAME.fullmatch(name):
        raise CaseError("%s: %s name %r must match [A-Za-z0-9_-]+" % (where, kind, name))


def names_case(text, case_id):
    """True if case_id appears in text as a whole token: the characters
    around it are not id characters (letters, digits, _ - /). So an approval
    for "filters/x/a" is not satisfied by a section naming "filters/x/ab".
    Mirrored in sas-parity (cases.rs names_case)."""
    start = text.find(case_id)
    while start != -1:
        end = start + len(case_id)
        before = text[start - 1] if start > 0 else ""
        after = text[end] if end < len(text) else ""
        if not (before and ID_CHARS.match(before)) and not (after and ID_CHARS.match(after)):
            return True
        start = text.find(case_id, end)
    return False


def is_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def check_tolerance_map(where, tolerances):
    if not isinstance(tolerances, dict):
        raise CaseError("%s: tolerances must be an object" % where)
    for key, value in tolerances.items():
        if key not in METRICS:
            raise CaseError("%s: unknown tolerance metric %r" % (where, key))
        if not is_number(value):
            raise CaseError("%s: tolerance %s must be a number" % (where, key))
    return {k: float(v) for k, v in tolerances.items()}


def check_defaults(where, tolerances, floor):
    """Defaults must hold every floor metric, at the floor or stricter."""
    tolerances = check_tolerance_map(where, tolerances)
    for metric, limit in floor.items():
        if metric not in tolerances:
            raise CaseError("%s: missing %s; the approved floor requires it (%s)" % (where, metric, limit))
        if tolerances[metric] > limit:
            raise CaseError("%s: %s = %s is looser than the approved floor %s" % (where, metric,
                                                                                  tolerances[metric], limit))
    return tolerances


def resolve_tolerances(case_id, defaults, file_level, case, ledger_text):
    if "tolerance_exception" in file_level:
        raise CaseError("%s: tolerance_exception is only allowed on individual cases" % case_id)
    tolerances = dict(defaults)
    for where, layer in (("file", file_level), ("case", case)):
        if "tolerances" in layer:
            tolerances.update(check_tolerance_map("%s (%s level)" % (case_id, where), layer["tolerances"]))
    looser = sorted(k for k, limit in defaults.items() if tolerances[k] > limit)

    exception = case.get("tolerance_exception")
    if exception is None:
        if looser:
            raise CaseError("%s: tolerances %s are looser than the defaults %s; that needs a case-level "
                            "tolerance_exception approved in LEDGER.md (migration/AGENTS.md rule 6)"
                            % (case_id, {k: tolerances[k] for k in looser}, {k: defaults[k] for k in looser}))
        return tolerances, None

    if not isinstance(exception, dict) or set(exception) != set(EXCEPTION_KEYS):
        raise CaseError("%s: tolerance_exception must be {\"approved_in\": \"<LEDGER.md heading>\", "
                        "\"tolerances\": {...}}" % case_id)
    approved_in = exception["approved_in"]
    if not isinstance(approved_in, str) or not approved_in.strip():
        raise CaseError("%s: tolerance_exception.approved_in must name a LEDGER.md heading" % case_id)
    approved = check_tolerance_map(case_id + " tolerance_exception", exception["tolerances"])
    if not approved:
        raise CaseError("%s: tolerance_exception.tolerances is empty" % case_id)
    for metric, value in sorted(approved.items()):
        if metric not in looser:
            raise CaseError("%s: tolerance_exception approves %s, but the case does not loosen it" % (case_id, metric))
        if tolerances[metric] != value:
            raise CaseError("%s: %s = %s differs from the approved value %s; changing it needs a new approval"
                            % (case_id, metric, tolerances[metric], value))
    unapproved = [k for k in looser if k not in approved]
    if unapproved:
        raise CaseError("%s: %s loosened without approval in tolerance_exception" % (case_id, unapproved))
    bodies = [body for title, body in ledger_sections(ledger_text) if title == approved_in.strip()]
    if not bodies:
        raise CaseError("%s: tolerance_exception cites LEDGER.md heading %r, which does not exist"
                        % (case_id, approved_in))
    if not any(names_case(body, case_id) for body in bodies):
        raise CaseError("%s: LEDGER.md section %r does not name the case" % (case_id, approved_in))
    return tolerances, exception


def allowlist_from(text):
    """Case ids listed as "- `<id>`" under the decision doc's allowlist
    heading. The doc is the only place the list lives, so extending it is a
    decision-doc edit. Mirrored in sas-parity (cases.rs allowlist_from)."""
    ids = []
    for title, body in ledger_sections(text):
        if title == ALLOWLIST_HEADING:
            for line in body.splitlines():
                match = re.match(r"- `([^`]+)`", line)
                if match:
                    ids.append(match.group(1))
    return ids


def read_allowlist():
    if not os.path.exists(DECISION_DOC):
        return []
    with open(DECISION_DOC) as handle:
        return allowlist_from(handle.read())


def resolve_comparison(case_id, file_level, case, allowlist):
    """Artifact modes, the JSON bound, base64-f32 keys and the reference of a
    module case. Returns only the keys that differ from the defaults (exact
    artifacts, official reference), so existing resolutions are unchanged."""
    modes = {}
    for where, layer in (("file", file_level), ("case", case)):
        if "artifacts" not in layer:
            continue
        artifacts = layer["artifacts"]
        if not isinstance(artifacts, dict):
            raise CaseError("%s (%s level): artifacts must be an object" % (case_id, where))
        for name, mode in artifacts.items():
            if not ARTIFACT_NAME.fullmatch(name):
                raise CaseError("%s: artifact name %r must match [A-Za-z0-9_.-]+" % (case_id, name))
            if mode not in ARTIFACT_MODES:
                raise CaseError("%s: artifact %s mode must be one of %s, not %r"
                                % (case_id, name, list(ARTIFACT_MODES), mode))
            modes[name] = mode

    keys = None
    for where, layer in (("file", file_level), ("case", case)):
        if "base64_f32_keys" not in layer:
            continue
        value = layer["base64_f32_keys"]
        if not isinstance(value, list) or not all(isinstance(k, str) and k for k in value):
            raise CaseError("%s (%s level): base64_f32_keys must be a list of key names" % (case_id, where))
        for key in value:
            if key not in BASE64_F32_KEYS:
                raise CaseError("%s: %r cannot be declared in base64_f32_keys; only %s hold base64 f32 data "
                                "(pinned in both resolvers)" % (case_id, key, list(BASE64_F32_KEYS)))
        keys = sorted(set(value))

    bound = dict(JSON_BOUND)
    for where, layer in (("file", file_level), ("case", case)):
        if "json_bound" not in layer:
            continue
        value = layer["json_bound"]
        if not isinstance(value, dict) or not value or set(value) - set(JSON_BOUND):
            raise CaseError("%s (%s level): json_bound must be {\"ulp\": <int>, \"abs\": <number>}"
                            % (case_id, where))
        if "ulp" in value and not (isinstance(value["ulp"], int) and not isinstance(value["ulp"], bool)
                                   and value["ulp"] >= 0):
            raise CaseError("%s: json_bound.ulp must be a non-negative integer" % case_id)
        if "abs" in value and not (is_number(value["abs"]) and value["abs"] >= 0):
            raise CaseError("%s: json_bound.abs must be a non-negative number" % case_id)
        for metric, limit in sorted(JSON_BOUND.items()):
            if metric in value and value[metric] > limit:
                raise CaseError("%s: json_bound %s = %s is looser than the approved JSON bound %s; the "
                                "bound is pinned in both resolvers and only tightens"
                                % (case_id, metric, value[metric], limit))
        bound.update(value)
    bound["abs"] = float(bound["abs"])

    reference = case.get("reference", "official")
    if reference not in REFERENCES:
        raise CaseError("%s: reference must be one of %s, not %r" % (case_id, list(REFERENCES), reference))
    extra = {}
    if reference == "nofastmath":
        # The JSON bound is part of the normal limits the calibration
        # justification uses, so it is always the pinned one here.
        bound = dict(JSON_BOUND)
        if "json_bound" in case:
            raise CaseError("%s: a reference \"nofastmath\" case cannot set json_bound" % case_id)
        if case_id not in allowlist:
            raise CaseError("%s: reference \"nofastmath\" is only allowed for case ids listed under \"%s\" "
                            "in %s" % (case_id, ALLOWLIST_HEADING, os.path.relpath(DECISION_DOC, REPO)))
        for key in ("tolerances", "tolerance_exception"):
            if key in case:
                raise CaseError("%s: a reference \"nofastmath\" case cannot set %s: it is compared exactly, "
                                "and its normal limits (the module defaults) decide in calibration whether "
                                "it qualifies" % (case_id, key))
        # Compared bit for bit; the declared artifact modes still define the
        # normal limits the calibration justification uses.
        extra["reference"] = "nofastmath"

    if modes:
        extra["artifacts"] = modes
    if "json" in modes.values():
        extra["json_bound"] = bound
        if keys:
            extra["base64_f32_keys"] = keys
    return extra


def resolve_renders(cases_json, ledger_text):
    if not isinstance(cases_json, dict):
        raise CaseError("cases.json must be an object")
    defaults = cases_json.get("defaults")
    if not isinstance(defaults, dict):
        raise CaseError("cases.json: missing defaults")
    unknown = set(defaults) - set(RENDER_DEFAULT_KEYS)
    if unknown:
        raise CaseError("cases.json defaults: unknown keys %s" % sorted(unknown))
    default_tolerances = check_defaults("cases.json defaults.tolerances", defaults.get("tolerances", {}),
                                        RENDER_FLOOR)
    renders = []
    for case in cases_json.get("renders", []):
        if not isinstance(case, dict) or not isinstance(case.get("name"), str):
            raise CaseError("cases.json: every render case needs a name")
        check_name("cases.json", "render case", case["name"])
        case_id = "render/" + case["name"]
        unknown = set(case) - set(RENDER_CASE_KEYS)
        if unknown:
            raise CaseError("%s: unknown keys %s" % (case_id, sorted(unknown)))
        for key in ("preset", "midi"):
            if not isinstance(case.get(key), str):
                raise CaseError("%s: needs %r" % (case_id, key))
        merged = {k: v for k, v in defaults.items() if k != "tolerances"}
        merged.update({k: v for k, v in case.items() if k not in ("tolerances", "tolerance_exception")})
        merged["tolerances"], exception = resolve_tolerances(case_id, default_tolerances, {}, case, ledger_text)
        if exception:
            merged["tolerance_exception"] = exception
        merged["id"] = case_id
        renders.append(merged)
    return renders


def resolve_module_file(area, module, rel, data, module_defaults, ledger_text, allowlist=()):
    check_name(rel, "area", area)
    check_name(rel, "module", module)
    if not isinstance(data, dict):
        raise CaseError("%s: must be an object" % rel)
    unknown = set(data) - set(FILE_KEYS)
    if unknown:
        hint = " (tolerance_exception is case-level only)" if "tolerance_exception" in unknown else ""
        raise CaseError("%s: unknown keys %s%s" % (rel, sorted(unknown), hint))
    if not isinstance(data.get("cases"), dict) or not data["cases"]:
        raise CaseError("%s: needs a non-empty \"cases\" object" % rel)
    default_spec = {k: module_defaults[k] for k in SPEC_KEYS if k in module_defaults}
    default_tolerances = check_defaults("cases.json module_defaults.tolerances",
                                        module_defaults.get("tolerances", {}), MODULE_FLOOR)
    file_level = {k: v for k, v in data.items() if k != "cases"}
    resolved = []
    for name, case in data["cases"].items():
        check_name(rel, "case", name)
        case_id = "%s/%s/%s" % (area, module, name)
        if not isinstance(case, dict):
            raise CaseError("%s: must be an object" % case_id)
        unknown = set(case) - set(CASE_KEYS)
        if unknown:
            raise CaseError("%s: unknown keys %s" % (case_id, sorted(unknown)))
        spec = dict(default_spec)
        for layer in (file_level, case):
            spec.update({k: v for k, v in layer.items() if k in SPEC_KEYS})
        if "module" not in spec:
            raise CaseError("%s: no \"module\" (ref-module registry name)" % case_id)
        comparison = resolve_comparison(case_id, file_level, case, allowlist)
        if comparison.get("reference") == "nofastmath":
            # Normal limits are the module defaults: file-level tightening
            # must not make the official reference "fail" on purpose.
            tolerances, exception = dict(default_tolerances), None
        else:
            tolerances, exception = resolve_tolerances(case_id, default_tolerances, file_level, case, ledger_text)
        entry = {"id": case_id, "file": rel, "spec": spec, "tolerances": tolerances}
        if exception:
            entry["tolerance_exception"] = exception
        entry.update(comparison)
        resolved.append(entry)
    return resolved


CASES_JSON_KEYS = ("comment", "defaults", "renders", "module_defaults")


def load_cases_json():
    data = read_json_strict(os.path.join(CORPUS, "cases.json"))
    unknown = set(data) - set(CASES_JSON_KEYS) if isinstance(data, dict) else set()
    if unknown:
        raise CaseError("cases.json: unknown keys %s" % sorted(unknown))
    return data


def load_renders():
    return resolve_renders(load_cases_json(), read_ledger())


def module_case_files():
    files = []
    for path in sorted(glob.glob(os.path.join(MODULE_CASES, "*", "*.json"))):
        if path.endswith(".hashes.json"):
            continue
        files.append(path)
    return files


def load_modules():
    cases_json = load_cases_json()
    module_defaults = cases_json.get("module_defaults")
    if not isinstance(module_defaults, dict):
        raise CaseError("cases.json: missing module_defaults")
    unknown = set(module_defaults) - set(MODULE_DEFAULT_KEYS)
    if unknown:
        raise CaseError("cases.json module_defaults: unknown keys %s" % sorted(unknown))
    ledger_text = read_ledger()
    allowlist = read_allowlist()
    resolved = []
    for path in module_case_files():
        area = os.path.basename(os.path.dirname(path))
        module = os.path.basename(path)[:-len(".json")]
        rel = os.path.relpath(path, REPO)
        resolved.extend(resolve_module_file(area, module, rel, read_json_strict(path), module_defaults,
                                            ledger_text, allowlist))
    return resolved


def selected(case_id, only):
    """--only tokens match a case id exactly or as a path prefix
    ("filters/digital_svf" matches "filters/digital_svf/<case>")."""
    if not only:
        return True
    for token in only:
        token = token.rstrip("/")
        if case_id == token or case_id.startswith(token + "/"):
            return True
    return False


def run(command):
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    return result.returncode, result.stdout


def write_json(path, data):
    with open(path, "w") as handle:
        json.dump(data, handle, indent=2, sort_keys=True)
        handle.write("\n")


def cmd_render(args):
    renders, modules = load_renders(), load_modules()
    failures = 0

    for case in renders:
        if not selected(case["id"], args.only):
            continue
        os.makedirs(os.path.join(args.out, "renders"), exist_ok=True)
        out = os.path.join(args.out, "renders", case["name"] + ".wav")
        case_json = os.path.join(args.out, "renders", case["name"] + ".case.json")
        # A failed render must never leave older output that looks fresh.
        for stale in (case_json, out, out + ".json"):
            if os.path.exists(stale):
                os.remove(stale)
        command = [
            args.ref_render,
            os.path.join(CORPUS, case["preset"]),
            os.path.join(CORPUS, case["midi"]),
            "-o", out,
            "--sample-rate", str(case["sample_rate"]),
            "--block-size", str(case["block_size"]),
            "--bpm", str(case["bpm"]),
            "--seed", str(case["seed"]),
        ]
        command += ["--length", str(case["length"])] if "length" in case else ["--tail", str(case["tail"])]
        code, output = run(command)
        if code == 0:
            write_json(case_json, case)
        print(("ok   " if code == 0 else "FAIL ") + case["id"])
        if code != 0:
            print(output)
            failures += 1

    for case in modules:
        if not selected(case["id"], args.only):
            continue
        out = os.path.join(args.out, "modules", case["id"])
        os.makedirs(out, exist_ok=True)
        # Remove everything from earlier renders (case.json first) so a
        # failure can never leave stale data that looks fresh.
        for stale in [os.path.join(out, "case.json")] + sorted(os.listdir(out)):
            path = os.path.join(out, stale)
            if os.path.isfile(path):
                os.remove(path)
        spec_path = os.path.join(out, "spec.json")
        write_json(spec_path, case["spec"])
        code, output = run([args.ref_module, spec_path, "--out-dir", out, "--corpus-dir", CORPUS])
        if code == 0:
            write_json(os.path.join(out, "case.json"), case)
        print(("ok   " if code == 0 else "FAIL ") + case["id"])
        if code != 0:
            print(output)
            failures += 1

    return 2 if failures else 0


def diff(args, reference, test, tolerances, report_path):
    command = [args.audio_diff, reference, test, "--quiet", "--json", report_path]
    if args.exact:
        command += ["--max-abs-error", "0"]
    else:
        flag = {"max_abs_error": "--max-abs-error", "rms_diff_db": "--rms-diff-db",
                "residual_db": "--residual-db", "lsd_db": "--lsd-db"}
        for key, value in tolerances.items():
            command += [flag[key], str(value)]
    code, output = run(command)
    if code == 2:
        print(output.strip())
    return code


def module_outputs(directory):
    if not os.path.isdir(directory):
        return []
    return sorted(f for f in os.listdir(directory) if f.startswith("out_") and f.endswith(".npy"))


def module_artifacts(directory):
    if not os.path.isdir(directory):
        return []
    return sorted(f for f in os.listdir(directory) if f.startswith("artifact_"))


def f32_order(bits):
    """Maps f32 bits to integers whose difference is the ulp distance
    (-0.0 and 0.0 both map to 0)."""
    return -(bits & 0x7FFFFFFF) if bits & 0x80000000 else bits


def f32_close(a, b, bits_a, bits_b, bound):
    if bits_a == bits_b or a == b:
        return True
    if not (math.isfinite(a) and math.isfinite(b)):
        return False
    return abs(a - b) <= bound["abs"] or abs(f32_order(bits_a) - f32_order(bits_b)) <= bound["ulp"]


def f32_bits(value):
    try:
        return struct.unpack("<I", struct.pack("<f", value))[0]
    except OverflowError:
        return None


def number_close(a, b, bound):
    """JSON floats (parsed as doubles): equal, within bound["abs"], or within
    bound["ulp"] after rounding both to f32. Mirrored in sas-parity."""
    if a == b:
        return True
    if not (math.isfinite(a) and math.isfinite(b)):
        return False
    if abs(a - b) <= bound["abs"]:
        return True
    bits_a, bits_b = f32_bits(a), f32_bits(b)
    if bits_a is None or bits_b is None:
        return False
    fa, fb = struct.unpack("<2f", struct.pack("<2I", bits_a, bits_b))
    if not (math.isfinite(fa) and math.isfinite(fb)):
        return False
    return abs(f32_order(bits_a) - f32_order(bits_b)) <= bound["ulp"]


def json_kind(value):
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, int):
        return "integer"
    if isinstance(value, float):
        return "float"
    if isinstance(value, str):
        return "string"
    return "array" if isinstance(value, list) else "object"


def decode_f32(text):
    try:
        raw = base64.b64decode(text, validate=True)
    except (binascii.Error, ValueError):
        return None
    if len(raw) % 4:
        return None
    count = len(raw) // 4
    return struct.unpack("<%df" % count, raw), struct.unpack("<%dI" % count, raw)


def base64_f32_diff(ref, test, bound, path, problems):
    decoded_ref, decoded_test = decode_f32(ref), decode_f32(test)
    if decoded_ref is None or decoded_test is None:
        which = "reference" if decoded_ref is None else "port"
        problems.append("%s: %s value is not base64 of little-endian f32" % (path, which))
        return
    (ref_values, ref_bits), (test_values, test_bits) = decoded_ref, decoded_test
    if len(ref_values) != len(test_values):
        problems.append("%s: %d f32 values in the reference, %d in the port"
                        % (path, len(ref_values), len(test_values)))
        return
    for index, (a, b) in enumerate(zip(ref_values, test_values)):
        if not f32_close(a, b, ref_bits[index], test_bits[index], bound):
            problems.append("%s[%d]: reference %r, port %r (%d ulp, abs %.3g)"
                            % (path, index, a, b, abs(f32_order(ref_bits[index]) - f32_order(test_bits[index])),
                               abs(a - b) if math.isfinite(a - b) else float("inf")))


def json_diff(ref, test, bound, base64_keys, path, problems):
    kind = json_kind(ref)
    if kind != json_kind(test):
        problems.append("%s: reference is %s, port is %s" % (path, kind, json_kind(test)))
    elif kind == "object":
        for key in sorted(set(ref) | set(test)):
            sub = "%s.%s" % (path, key)
            if key not in test:
                problems.append("%s: missing in the port" % sub)
            elif key not in ref:
                problems.append("%s: not in the reference" % sub)
            elif key in base64_keys and json_kind(ref[key]) == "string" == json_kind(test[key]):
                base64_f32_diff(ref[key], test[key], bound, sub, problems)
            else:
                json_diff(ref[key], test[key], bound, base64_keys, sub, problems)
    elif kind == "array":
        if len(ref) != len(test):
            problems.append("%s: array length %d in the reference, %d in the port" % (path, len(ref), len(test)))
        else:
            for index, (a, b) in enumerate(zip(ref, test)):
                json_diff(a, b, bound, base64_keys, "%s[%d]" % (path, index), problems)
    elif kind == "float":
        if not number_close(ref, test, bound):
            problems.append("%s: reference %r, port %r" % (path, ref, test))
    elif ref != test:
        problems.append("%s: reference %s, port %s" % (path, json.dumps(ref), json.dumps(test)))


def json_bound_problems(ref_bytes, test_bytes, bound, base64_keys=()):
    """Differences between two JSON documents under the owner-approved bound
    ([] if they match). Mirrored in sas-parity (json_bound.rs)."""
    documents = []
    for name, data in (("reference", ref_bytes), ("port", test_bytes)):
        try:
            documents.append(json.loads(data.decode("utf-8"), parse_constant=_reject_constant))
        except ValueError as error:
            return ["%s is not valid JSON: %s" % (name, error)]
    problems = []
    json_diff(documents[0], documents[1], bound, set(base64_keys), "$", problems)
    return problems


def compare_module_case(args, case, ref_dir, test_dir, reports, bit_exact):
    """[(label, code)] for one module case. bit_exact: every array and
    artifact must be byte-identical (reference "nofastmath" cases)."""
    results = []
    outputs = module_outputs(ref_dir)
    artifacts = module_artifacts(ref_dir)
    # A case may emit arrays, text artifacts, or both (artifact-only
    # modules have no out_*.npy at all).
    if not outputs and not artifacts:
        results.append((case["id"] + " (no reference outputs or artifacts)", 2))
    for output in outputs:
        label = "%s:%s" % (case["id"], output[len("out_"):-len(".npy")])
        test_path = os.path.join(test_dir, output)
        if bit_exact:
            if not os.path.exists(test_path):
                results.append((label + " (missing in test tree)", 2))
                continue
            with open(os.path.join(ref_dir, output), "rb") as a, open(test_path, "rb") as b:
                results.append((label + " (nofastmath, bit-exact)", 0 if a.read() == b.read() else 1))
            continue
        report = os.path.join(reports, label.replace("/", "__").replace(":", "__") + ".json")
        results.append((label, diff(args, os.path.join(ref_dir, output), test_path, case["tolerances"], report)))
    for output in sorted(set(module_outputs(test_dir)) - set(outputs)):
        label = "%s:%s" % (case["id"], output[len("out_"):-len(".npy")])
        results.append((label + " (missing in reference tree)", 2))
    modes = case.get("artifacts", {})
    if outputs or artifacts:
        for name in sorted(set(modes) - {a[len("artifact_"):] for a in artifacts}):
            results.append(("%s:artifact/%s (declared, but not in the reference tree)" % (case["id"], name), 2))
    for artifact in artifacts:
        name = artifact[len("artifact_"):]
        label = "%s:artifact/%s" % (case["id"], name)
        test_path = os.path.join(test_dir, artifact)
        if not os.path.exists(test_path):
            results.append((label + " (missing in test tree)", 2))
            continue
        with open(os.path.join(ref_dir, artifact), "rb") as a, open(test_path, "rb") as b:
            ref_bytes, test_bytes = a.read(), b.read()
        # Text artifacts are byte-exact unless declared "json"; --exact
        # (determinism) and nofastmath comparisons are always byte-exact.
        if modes.get(name) == "json" and not (args.exact or bit_exact):
            problems = json_bound_problems(ref_bytes, test_bytes, case["json_bound"],
                                           case.get("base64_f32_keys", ()))
            for problem in problems[:10]:
                print("  %s: %s" % (label, problem))
            if len(problems) > 10:
                print("  %s: ... %d more" % (label, len(problems) - 10))
            results.append((label + " (json bound)", 1 if problems else 0))
        else:
            kind = "nofastmath, exact" if bit_exact else "exact"
            results.append((label + " (%s)" % kind, 0 if ref_bytes == test_bytes else 1))
    for artifact in sorted(set(module_artifacts(test_dir)) - set(artifacts)):
        label = "%s:artifact/%s" % (case["id"], artifact[len("artifact_"):])
        results.append((label + " (missing in reference tree)", 2))
    return results


def justify_nofastmath(args, case, nofastmath_dir, reports):
    """Calibration check for a reference "nofastmath" case (the positional
    reference is the official tree, the test tree the REF_FAST_MATH=OFF
    tree): the case is valid only if the official reference fails the
    case's normal limits against the no-fast-math build."""
    official_dir = os.path.join(args.reference, "modules", case["id"])
    normal = compare_module_case(args, case, official_dir, nofastmath_dir, reports, bit_exact=False)
    errors = [(label, code) for label, code in normal if code not in (0, 1)]
    if errors:
        return errors
    failing = [label for label, code in normal if code == 1]
    if failing:
        return [("%s (reference \"nofastmath\" justified: the official reference fails its normal limits "
                 "against the no-fast-math build in %s)" % (case["id"], ", ".join(failing)), 0)]
    return [("%s (reference \"nofastmath\" NOT justified: the official reference passes the normal limits "
             "against the no-fast-math build, so this case must use the official reference)" % case["id"], 2)]


def cmd_compare(args):
    renders, modules = load_renders(), load_modules()
    reports = os.path.join(args.test, "reports")
    os.makedirs(reports, exist_ok=True)
    results = []

    for case in renders:
        if not selected(case["id"], args.only):
            continue
        name = case["name"] + ".wav"
        code = diff(args, os.path.join(args.reference, "renders", name), os.path.join(args.test, "renders", name),
                    case["tolerances"], os.path.join(reports, case["name"] + ".json"))
        results.append((case["id"], code))

    for case in modules:
        if not selected(case["id"], args.only):
            continue
        test_dir = os.path.join(args.test, "modules", case["id"])
        if case.get("reference") != "nofastmath":
            ref_dir = os.path.join(args.reference, "modules", case["id"])
            results.extend(compare_module_case(args, case, ref_dir, test_dir, reports, bit_exact=False))
        elif args.justify_nofastmath:
            results.extend(justify_nofastmath(args, case, test_dir, reports))
        elif args.reference_nofastmath or args.exact:
            ref_dir = os.path.join(args.reference_nofastmath or args.reference, "modules", case["id"])
            results.extend(compare_module_case(args, case, ref_dir, test_dir, reports, bit_exact=True))
        else:
            results.append((case["id"] + " (reference \"nofastmath\": pass --reference-nofastmath DIR)", 2))

    for label, code in results:
        print({0: "pass ", 1: "FAIL ", 2: "ERROR"}.get(code, "ERROR") + " " + label)
    failed = sum(1 for _, code in results if code == 1)
    errors = sum(1 for _, code in results if code not in (0, 1))
    print("%d compared, %d failed, %d errors (reports in %s)" % (len(results), failed, errors, reports))
    if errors:
        return 2
    return 1 if failed else 0


def tree_identity(tree, renders, modules):
    for case in renders:
        path = os.path.join(tree, "renders", case["name"] + ".wav.json")
        if os.path.exists(path):
            build = read_json_strict(path)["build"]
            return {k: build.get(k) for k in IDENTITY_KEYS}
    for case in modules:
        path = os.path.join(tree, "modules", case["id"], "capture.json")
        if os.path.exists(path):
            build = read_json_strict(path)["build"]
            return {k: build.get(k) for k in IDENTITY_KEYS}
    raise CaseError("no renders or captures found in %s" % tree)


FNV_OFFSET = 1469598103934665603
FNV_PRIME = 1099511628211
MASK64 = (1 << 64) - 1


def fnv1a64(data):
    """The harness hash, identical to harness_io.cpp and sas-parity: FNV-1a 64
    with offset basis 1469598103934665603 (not the standard basis
    14695981039346656037; kept so committed hashes stay valid)."""
    value = FNV_OFFSET
    for byte in data:
        value = ((value ^ byte) * FNV_PRIME) & MASK64
    return "%016x" % value


def npy_payload(path):
    with open(path, "rb") as handle:
        raw = handle.read()
    if raw[:6] != b"\x93NUMPY":
        raise CaseError("%s is not a .npy file" % path)
    if raw[6] == 1:
        start = 10 + int.from_bytes(raw[8:10], "little")
    else:
        start = 12 + int.from_bytes(raw[8:12], "little")
    return raw[start:]


def wav_payload(path):
    with open(path, "rb") as handle:
        raw = handle.read()
    position = 12
    while position + 8 <= len(raw):
        chunk, size = raw[position:position + 4], int.from_bytes(raw[position + 4:position + 8], "little")
        if chunk == b"data":
            return raw[position + 8:position + 8 + size]
        position += 8 + size + (size & 1)
    raise CaseError("%s has no data chunk" % path)


def module_hash_file(case):
    return os.path.join(REPO, case["file"])[:-len(".json")] + ".hashes.json"


def tree_hashes(tree, renders, modules):
    """{hash_file: {key: hash}} for every case present in the tree, hashed
    from the files themselves, plus a list of problems where the files do
    not match their own metadata (edited or truncated captures)."""
    by_file = {}
    problems = []
    for case in renders:
        wav = os.path.join(tree, "renders", case["name"] + ".wav")
        meta = wav + ".json"
        if not (os.path.exists(wav) and os.path.exists(meta)):
            continue
        digest = fnv1a64(wav_payload(wav))
        if digest != read_json_strict(meta)["samples_fnv1a64"]:
            problems.append("%s: WAV data does not match its metadata hash" % case["id"])
        by_file.setdefault(RENDER_HASHES, {})[case["name"]] = digest
    for case in modules:
        directory = os.path.join(tree, "modules", case["id"])
        capture_path = os.path.join(directory, "capture.json")
        if not os.path.exists(capture_path):
            continue
        capture = read_json_strict(capture_path)
        case_name = case["id"].rsplit("/", 1)[1]
        hashes = by_file.setdefault(module_hash_file(case), {})
        for output, info in sorted(capture.get("outputs", {}).items()):
            path = os.path.join(directory, info["file"])
            digest = fnv1a64(npy_payload(path)) if os.path.exists(path) else "missing"
            if digest != info["fnv1a64"]:
                problems.append("%s:%s: %s does not match capture.json" % (case["id"], output, info["file"]))
            hashes["%s:%s" % (case_name, output)] = digest
        for name, info in sorted(capture.get("artifacts", {}).items()):
            path = os.path.join(directory, info["file"])
            digest = fnv1a64(open(path, "rb").read()) if os.path.exists(path) else "missing"
            if digest != info["fnv1a64"]:
                problems.append("%s: artifact %s does not match capture.json" % (case["id"], name))
            hashes["%s:artifact/%s" % (case_name, name)] = digest
    return by_file, problems


def expected_keys(renders, modules):
    by_file = {RENDER_HASHES: None} if renders else {}
    for case in modules:
        by_file.setdefault(module_hash_file(case), set()).add(case["id"].rsplit("/", 1)[1])
    return by_file


def read_hash_file(path):
    if not os.path.exists(path):
        return {"entries": []}
    return read_json_strict(path)


def known_identities():
    """Identities with committed render hashes, i.e. the CI build identities."""
    return [entry["identity"] for entry in read_hash_file(RENDER_HASHES)["entries"]]


def nofastmath_counterpart(known):
    """The identity a REF_FAST_MATH=OFF build of CI identity `known` has:
    fast_math false and opt_flags without FAST_MATH_FLAGS, everything else
    equal. None if `known` is not a fast-math identity."""
    flags = known.get("opt_flags")
    if known.get("fast_math") is not True or not isinstance(flags, str) or FAST_MATH_FLAGS not in flags:
        return None
    return dict(known, fast_math=False, opt_flags=flags.replace(FAST_MATH_FLAGS, "", 1))


def nofastmath_identity_known(identity):
    """A no-fast-math identity is known when it is exactly the no-fast-math
    counterpart of a CI identity. Mirrored in sas-parity (captures.rs)."""
    return any(nofastmath_counterpart(known) == identity for known in known_identities())


def cmd_hashes(args):
    renders, modules = load_renders(), load_modules()
    if args.reference == "nofastmath":
        # Only reference "nofastmath" cases are consumed from the no-fast-math
        # tree, so only they are pinned for its identity.
        renders = []
        modules = [c for c in modules if c.get("reference") == "nofastmath"]
        if not modules:
            print("no reference \"nofastmath\" cases")
            return 0
    identity = tree_identity(args.tree, renders, modules)
    current, problems = tree_hashes(args.tree, renders, modules)
    if args.reference == "nofastmath":
        if identity.get("fast_math") is not False:
            print("%s was not rendered by a REF_FAST_MATH=OFF build (fast_math: %s)"
                  % (args.tree, identity.get("fast_math")))
            return 2
        known = nofastmath_identity_known(identity)
    else:
        known = identity in known_identities()

    if args.write:
        if problems:
            for problem in problems:
                print(problem)
            print("refusing to write hashes from a tree whose files do not match their metadata")
            return 1
        if not known and not args.new_identity:
            print("refusing to write hashes: this tree's build identity is not a known CI identity:")
            print("  " + json.dumps(identity, sort_keys=True))
            print("Write hashes from a CI-identity tree (the parity-captures or reference-renders artifact),")
            print("or pass --new-identity to add a new build identity (needs a recorded decision).")
            return 2
        for path, hashes in sorted(current.items()):
            data = read_hash_file(path)
            entry = next((e for e in data["entries"] if e["identity"] == identity), None)
            if entry is None:
                entry = {"identity": identity, "hashes": {}}
                data["entries"].append(entry)
            entry["hashes"].update(hashes)
            if path != RENDER_HASHES:
                live = {c["id"].rsplit("/", 1)[1] for c in modules if module_hash_file(c) == path}
                entry["hashes"] = {k: v for k, v in entry["hashes"].items() if k.split(":")[0] in live}
            else:
                live = {c["name"] for c in renders}
                entry["hashes"] = {k: v for k, v in entry["hashes"].items() if k in live}
            write_json(path, data)
            print("wrote %s (%d hashes)" % (os.path.relpath(path, REPO), len(entry["hashes"])))
        return 0

    if not known:
        print("build identity of %s has no committed hashes:" % args.tree)
        print("  " + json.dumps(identity, sort_keys=True))
        if args.require_known_identity:
            print("FAIL: --require-known-identity (CI must run a known build identity; a runner image or")
            print("toolchain change needs a recorded decision and new hashes)")
            return 1
        print("skipped (not a CI build identity)")
        return 0

    checked = 0
    for path, case_names in sorted(expected_keys(renders, modules).items()):
        rel = os.path.relpath(path, REPO)
        data = read_hash_file(path)
        entry = next((e for e in data["entries"] if e["identity"] == identity), None)
        if entry is None:
            problems.append("%s has no hashes for the CI build identity: write them from a CI-identity tree "
                            "(`run_corpus.py hashes <parity-captures> --write`)" % rel)
            continue
        got = current.get(path, {})
        expected = entry["hashes"]
        for key in sorted(set(expected) | set(got)):
            name = key.split(":")[0]
            if case_names is not None and name not in case_names:
                problems.append("%s: stale hash for removed case %s" % (rel, key))
            elif key not in got:
                problems.append("%s: %s has a stored hash but was not rendered" % (rel, key))
            elif key not in expected:
                problems.append("%s: no stored hash for %s (run hashes --write)" % (rel, key))
            elif expected[key] != got[key]:
                problems.append("MISMATCH %s %s expected %s got %s" % (rel, key, expected[key], got[key]))
            checked += 1
        if case_names is not None:
            for name in sorted(case_names - {k.split(":")[0] for k in got}):
                problems.append("%s: case %s was not rendered" % (rel, name))

    for problem in problems:
        print(problem)
    print("%d hashes checked, %d problems" % (checked, len(problems)))
    return 1 if problems else 0


def unapproved_allowlist_additions(base_doc, head_doc, base_ledger, head_ledger):
    """Ids added to the no-fast-math allowlist without a LEDGER.md section
    that is new in this change, has "owner approval" in its heading and
    names the id. Removals need no approval (the case returns to the
    official reference)."""
    before = set(allowlist_from(base_doc))
    added = [case_id for case_id in allowlist_from(head_doc) if case_id not in before]
    old_titles = {title for title, _ in ledger_sections(base_ledger)}
    approvals = [body for title, body in ledger_sections(head_ledger)
                 if title not in old_titles and "owner approval" in title.lower()]
    return [case_id for case_id in added if not any(names_case(body, case_id) for body in approvals)]


def git_show(rev, path):
    code, output = run(["git", "-C", REPO, "show", "%s:%s" % (rev, os.path.relpath(path, REPO))])
    return output if code == 0 else ""


def cmd_allowlist_approval(args):
    if not args.base or set(args.base) == {"0"}:
        print("no base revision; allowlist approval check skipped")
        return 0
    code, output = run(["git", "-C", REPO, "rev-parse", "--verify", args.base + "^{commit}"])
    if code != 0:
        print("base revision %s not found: %s" % (args.base, output.strip()))
        return 2
    with open(DECISION_DOC) as handle:
        head_doc = handle.read()
    missing = unapproved_allowlist_additions(git_show(args.base, DECISION_DOC), head_doc,
                                             git_show(args.base, LEDGER), read_ledger())
    for case_id in missing:
        print("%s was added to the no-fast-math allowlist without a new LEDGER.md section whose heading "
              "contains \"owner approval\" and which names the case" % case_id)
    print("%d allowlist additions without a recorded owner approval" % len(missing))
    return 1 if missing else 0


def cmd_list(args):
    data = {"renders": load_renders(), "modules": load_modules()}
    if args.only:
        data = {k: [c for c in v if selected(c["id"], args.only)] for k, v in data.items()}
    print(json.dumps(data, indent=2, sort_keys=True))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    render = sub.add_parser("render", help="render corpus cases")
    render.add_argument("--out", required=True)
    render.add_argument("--ref-render", default=DEFAULT_RENDER)
    render.add_argument("--ref-module", default=DEFAULT_MODULE)
    render.add_argument("--only", nargs="*", help="substrings of case ids to run")

    compare = sub.add_parser("compare", help="compare two render trees")
    compare.add_argument("reference")
    compare.add_argument("test")
    compare.add_argument("--audio-diff", default=DEFAULT_DIFF)
    compare.add_argument("--exact", action="store_true", help="require bit-identical outputs")
    compare.add_argument("--reference-nofastmath", metavar="DIR",
                         help="REF_FAST_MATH=OFF tree; reference \"nofastmath\" cases are compared bit for bit "
                              "against it")
    compare.add_argument("--justify-nofastmath", action="store_true",
                         help="calibration: REFERENCE is the official tree and TEST the REF_FAST_MATH=OFF tree; "
                              "each reference \"nofastmath\" case must fail its normal limits there")
    compare.add_argument("--only", nargs="*", help="substrings of case ids to run")

    hashes = sub.add_parser("hashes", help="write / check output hashes of a render tree")
    hashes.add_argument("tree")
    mode = hashes.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    hashes.add_argument("--require-known-identity", action="store_true",
                        help="--check fails (instead of skipping) when the tree's build identity has no hashes")
    hashes.add_argument("--new-identity", action="store_true",
                        help="--write may add hashes for a build identity that has none yet")
    hashes.add_argument("--reference", choices=REFERENCES, default="official",
                        help="nofastmath: TREE is a REF_FAST_MATH=OFF tree; pin only reference \"nofastmath\" cases")

    listing = sub.add_parser("list", help="print resolved cases as JSON")
    listing.add_argument("--only", nargs="*", help="substrings of case ids to include")

    approval = sub.add_parser("allowlist-approval",
                              help="fail if no-fast-math allowlist additions since --base lack an owner approval "
                                   "entry in LEDGER.md")
    approval.add_argument("--base", default="", help="git revision to compare against (e.g. origin/main)")

    args = parser.parse_args()
    try:
        if args.command == "allowlist-approval":
            return cmd_allowlist_approval(args)
        if args.command == "hashes":
            return cmd_hashes(args)
        if args.command == "list":
            return cmd_list(args)
        tools = [args.ref_render, args.ref_module] if args.command == "render" else [args.audio_diff]
        for tool in tools:
            if not os.access(tool, os.X_OK):
                print("missing tool: %s (build it first, see migration/README.md)" % tool, file=sys.stderr)
                return 2
        return cmd_render(args) if args.command == "render" else cmd_compare(args)
    except CaseError as error:
        print("case error: %s" % error, file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
