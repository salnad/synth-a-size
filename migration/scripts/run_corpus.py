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
same way): case values layer over file-level values over the defaults; every
metric present in the defaults stays checked; a value looser than the default
is an error unless the case or file sets "tolerance_exception" to a non-empty
string citing recorded human approval in migration/LEDGER.md.

Exit status: 0 all pass, 1 any comparison or hash check failed, 2 usage,
case-file or tool error. Python 3.8+ standard library only.
"""

import argparse
import glob
import json
import os
import subprocess
import sys

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CORPUS = os.path.join(REPO, "migration", "corpus")
MODULE_CASES = os.path.join(CORPUS, "modules")
RENDER_HASHES = os.path.join(CORPUS, "reference_hashes.json")
DEFAULT_RENDER = os.path.join(REPO, "migration", "reference", "build", "ref-render")
DEFAULT_MODULE = os.path.join(REPO, "migration", "reference", "build", "ref-module")
DEFAULT_DIFF = os.path.join(REPO, "migration", "tools", "audio-diff", "target", "release", "audio-diff")

METRICS = ("max_abs_error", "rms_diff_db", "residual_db", "lsd_db")
SPEC_KEYS = ("module", "params", "sample_rate", "block_size", "oversample", "num_samples", "seed", "inputs")
CASE_KEYS = SPEC_KEYS + ("description", "tolerances", "tolerance_exception")
FILE_KEYS = CASE_KEYS + ("cases",)
IDENTITY_KEYS = ("upstream_sha", "compiler", "glibc", "fast_math", "opt_flags", "arch", "libm_variants")


class CaseError(Exception):
    pass


def resolve_tolerances(where, defaults, layers):
    """layers: list of dicts (file level, case level) that may hold
    "tolerances" and "tolerance_exception"."""
    tolerances = dict(defaults)
    exception = None
    for layer in layers:
        for key, value in layer.get("tolerances", {}).items():
            if key not in METRICS:
                raise CaseError("%s: unknown tolerance metric %r" % (where, key))
            if not isinstance(value, (int, float)) or isinstance(value, bool):
                raise CaseError("%s: tolerance %s must be a number" % (where, key))
            tolerances[key] = float(value)
        text = layer.get("tolerance_exception")
        if isinstance(text, str) and text.strip():
            exception = text
    looser = [key for key, limit in defaults.items() if tolerances[key] > limit]
    if looser and exception is None:
        raise CaseError("%s: tolerances %s are looser than the defaults %s; that needs a "
                        "tolerance_exception citing recorded human approval (migration/AGENTS.md rule 6)"
                        % (where, {k: tolerances[k] for k in looser}, {k: defaults[k] for k in looser}))
    return tolerances, exception


def load_cases_json():
    with open(os.path.join(CORPUS, "cases.json")) as handle:
        return json.load(handle)


def load_renders():
    cases = load_cases_json()
    defaults = cases["defaults"]
    default_tolerances = {k: float(v) for k, v in defaults["tolerances"].items()}
    renders = []
    for case in cases["renders"]:
        merged = {k: v for k, v in defaults.items() if k != "tolerances"}
        merged.update({k: v for k, v in case.items() if k not in ("tolerances", "tolerance_exception")})
        merged["tolerances"], exception = resolve_tolerances("render/" + case["name"], default_tolerances, [case])
        if exception:
            merged["tolerance_exception"] = exception
        merged["id"] = "render/" + case["name"]
        renders.append(merged)
    return renders


def module_case_files():
    files = []
    for path in sorted(glob.glob(os.path.join(MODULE_CASES, "*", "*.json"))):
        if path.endswith(".hashes.json"):
            continue
        files.append(path)
    return files


def load_modules():
    defaults = load_cases_json()["module_defaults"]
    default_spec = {k: defaults[k] for k in SPEC_KEYS if k in defaults}
    default_tolerances = {k: float(v) for k, v in defaults["tolerances"].items()}
    resolved = []
    for path in module_case_files():
        area = os.path.basename(os.path.dirname(path))
        module = os.path.basename(path)[:-len(".json")]
        rel = os.path.relpath(path, REPO)
        try:
            with open(path) as handle:
                data = json.load(handle)
        except ValueError as error:
            raise CaseError("%s: invalid JSON: %s" % (rel, error))
        unknown = set(data) - set(FILE_KEYS)
        if unknown:
            raise CaseError("%s: unknown keys %s" % (rel, sorted(unknown)))
        if not isinstance(data.get("cases"), dict) or not data["cases"]:
            raise CaseError("%s: needs a non-empty \"cases\" object" % rel)
        file_level = {k: v for k, v in data.items() if k != "cases"}
        for name, case in data["cases"].items():
            case_id = "%s/%s/%s" % (area, module, name)
            unknown = set(case) - set(CASE_KEYS)
            if unknown:
                raise CaseError("%s: unknown keys %s" % (case_id, sorted(unknown)))
            spec = dict(default_spec)
            for layer in (file_level, case):
                spec.update({k: v for k, v in layer.items() if k in SPEC_KEYS})
            if "module" not in spec:
                raise CaseError("%s: no \"module\" (ref-module registry name)" % case_id)
            if "num_samples" not in spec:
                raise CaseError("%s: no \"num_samples\"" % case_id)
            tolerances, exception = resolve_tolerances(case_id, default_tolerances, [file_level, case])
            entry = {"id": case_id, "file": rel, "spec": spec, "tolerances": tolerances}
            if exception:
                entry["tolerance_exception"] = exception
            resolved.append(entry)
    return resolved


def selected(case_id, only):
    return not only or any(token in case_id for token in only)


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
        write_json(os.path.join(args.out, "renders", case["name"] + ".case.json"), case)
        print(("ok   " if code == 0 else "FAIL ") + case["id"])
        if code != 0:
            print(output)
            failures += 1

    for case in modules:
        if not selected(case["id"], args.only):
            continue
        out = os.path.join(args.out, "modules", case["id"])
        os.makedirs(out, exist_ok=True)
        for stale in glob.glob(os.path.join(out, "*.npy")):
            os.remove(stale)
        spec_path = os.path.join(out, "spec.json")
        write_json(spec_path, case["spec"])
        code, output = run([args.ref_module, spec_path, "--out-dir", out])
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
        ref_dir = os.path.join(args.reference, "modules", case["id"])
        outputs = module_outputs(ref_dir)
        if not outputs:
            results.append((case["id"] + " (no reference outputs)", 2))
        for output in outputs:
            label = "%s:%s" % (case["id"], output[len("out_"):-len(".npy")])
            report = os.path.join(reports, label.replace("/", "__").replace(":", "__") + ".json")
            code = diff(args, os.path.join(ref_dir, output), os.path.join(args.test, "modules", case["id"], output),
                        case["tolerances"], report)
            results.append((label, code))

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
            with open(path) as handle:
                build = json.load(handle)["build"]
            return {k: build.get(k) for k in IDENTITY_KEYS}
    for case in modules:
        path = os.path.join(tree, "modules", case["id"], "capture.json")
        if os.path.exists(path):
            with open(path) as handle:
                build = json.load(handle)["build"]
            return {k: build.get(k) for k in IDENTITY_KEYS}
    raise CaseError("no renders or captures found in %s" % tree)


def tree_hashes(tree, renders, modules):
    """Returns {hash_file: {key: hash}} for every case present in the tree."""
    by_file = {}
    for case in renders:
        path = os.path.join(tree, "renders", case["name"] + ".wav.json")
        if os.path.exists(path):
            with open(path) as handle:
                by_file.setdefault(RENDER_HASHES, {})[case["name"]] = json.load(handle)["samples_fnv1a64"]
    for case in modules:
        path = os.path.join(tree, "modules", case["id"], "capture.json")
        if not os.path.exists(path):
            continue
        with open(path) as handle:
            capture = json.load(handle)
        hash_file = os.path.join(REPO, case["file"])[:-len(".json")] + ".hashes.json"
        case_name = case["id"].rsplit("/", 1)[1]
        for output, info in sorted(capture["outputs"].items()):
            by_file.setdefault(hash_file, {})["%s:%s" % (case_name, output)] = info["fnv1a64"]
    return by_file


def expected_keys(renders, modules):
    by_file = {RENDER_HASHES: None}
    for case in modules:
        hash_file = os.path.join(REPO, case["file"])[:-len(".json")] + ".hashes.json"
        by_file.setdefault(hash_file, set()).add(case["id"].rsplit("/", 1)[1])
    return by_file


def read_hash_file(path):
    if not os.path.exists(path):
        return {"entries": []}
    with open(path) as handle:
        return json.load(handle)


def cmd_hashes(args):
    renders, modules = load_renders(), load_modules()
    identity = tree_identity(args.tree, renders, modules)
    current = tree_hashes(args.tree, renders, modules)

    if args.write:
        for path, hashes in sorted(current.items()):
            data = read_hash_file(path)
            entry = next((e for e in data["entries"] if e["identity"] == identity), None)
            if entry is None:
                entry = {"identity": identity, "hashes": {}}
                data["entries"].append(entry)
            entry["hashes"].update(hashes)
            if path != RENDER_HASHES:
                live = {"%s" % c["id"].rsplit("/", 1)[1] for c in modules
                        if os.path.join(REPO, c["file"])[:-len(".json")] + ".hashes.json" == path}
                entry["hashes"] = {k: v for k, v in entry["hashes"].items() if k.split(":")[0] in live}
            else:
                live = {c["name"] for c in renders}
                entry["hashes"] = {k: v for k, v in entry["hashes"].items() if k in live}
            write_json(path, data)
            print("wrote %s (%d hashes)" % (os.path.relpath(path, REPO), len(entry["hashes"])))
        return 0

    problems = []
    checked = 0
    skipped = []
    for path, case_names in sorted(expected_keys(renders, modules).items()):
        rel = os.path.relpath(path, REPO)
        data = read_hash_file(path)
        if not data["entries"] and path != RENDER_HASHES:
            problems.append("%s is missing: run `run_corpus.py hashes <tree> --write`" % rel)
            continue
        entry = next((e for e in data["entries"] if e["identity"] == identity), None)
        if entry is None:
            skipped.append(rel)
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
    if skipped:
        print("no hashes stored for this build identity in: %s (skipped)" % ", ".join(skipped))
        print("  identity:", json.dumps(identity, sort_keys=True))
    print("%d hashes checked, %d problems" % (checked, len(problems)))
    return 1 if problems else 0


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
    compare.add_argument("--only", nargs="*", help="substrings of case ids to run")

    hashes = sub.add_parser("hashes", help="write / check output hashes of a render tree")
    hashes.add_argument("tree")
    mode = hashes.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")

    listing = sub.add_parser("list", help="print resolved cases as JSON")
    listing.add_argument("--only", nargs="*", help="substrings of case ids to include")

    args = parser.parse_args()
    try:
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
