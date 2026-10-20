#!/usr/bin/env python3
"""Render the golden corpus and compare render trees.

  render   Render every case in migration/corpus/cases.json with the reference
           harness (or any CLI with the same interface) into OUT/:
             OUT/renders/<name>.wav (+ .wav.json metadata)
             OUT/modules/<name>/out_*.npy, in_*.npy, capture.json
  compare  Compare two such trees with audio-diff using each case's
           tolerances. --exact requires bit-identical output (determinism).

Examples:
  python3 migration/scripts/run_corpus.py render --out build/ref
  python3 migration/scripts/run_corpus.py render --out build/ref2
  python3 migration/scripts/run_corpus.py compare build/ref build/ref2 --exact
  python3 migration/scripts/run_corpus.py compare build/ref build/port

Exit status: 0 all pass, 1 any comparison failed, 2 usage / tool error.
Python 3.8+ standard library only.
"""

import argparse
import json
import os
import subprocess
import sys

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CORPUS = os.path.join(REPO, "migration", "corpus")
DEFAULT_RENDER = os.path.join(REPO, "migration", "reference", "build", "ref-render")
DEFAULT_MODULE = os.path.join(REPO, "migration", "reference", "build", "ref-module")
DEFAULT_DIFF = os.path.join(REPO, "migration", "tools", "audio-diff", "target", "release", "audio-diff")


def load_cases():
    with open(os.path.join(CORPUS, "cases.json")) as handle:
        cases = json.load(handle)
    defaults = cases["defaults"]
    renders = []
    for case in cases["renders"]:
        merged = dict(defaults)
        merged.update(case)
        merged["tolerances"] = dict(defaults["tolerances"], **case.get("tolerances", {}))
        renders.append(merged)
    return renders, cases.get("modules", [])


def selected(name, only):
    return not only or any(token in name for token in only)


def run(command):
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    return result.returncode, result.stdout


def cmd_render(args):
    renders, modules = load_cases()
    os.makedirs(os.path.join(args.out, "renders"), exist_ok=True)
    os.makedirs(os.path.join(args.out, "modules"), exist_ok=True)
    failures = 0

    for case in renders:
        if not selected(case["name"], args.only):
            continue
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
        print(("ok   " if code == 0 else "FAIL ") + "render " + case["name"])
        if code != 0:
            print(output)
            failures += 1

    for case in modules:
        if not selected(case["name"], args.only):
            continue
        out = os.path.join(args.out, "modules", case["name"])
        code, output = run([args.ref_module, os.path.join(CORPUS, case["spec"]), "--out-dir", out])
        print(("ok   " if code == 0 else "FAIL ") + "module " + case["name"])
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


def cmd_compare(args):
    renders, modules = load_cases()
    reports = os.path.join(args.test, "reports")
    os.makedirs(reports, exist_ok=True)
    results = []

    for case in renders:
        if not selected(case["name"], args.only):
            continue
        name = case["name"] + ".wav"
        code = diff(args, os.path.join(args.reference, "renders", name), os.path.join(args.test, "renders", name),
                    case["tolerances"], os.path.join(reports, case["name"] + ".json"))
        results.append(("render " + case["name"], code))

    for case in modules:
        if not selected(case["name"], args.only):
            continue
        ref_dir = os.path.join(args.reference, "modules", case["name"])
        outputs = sorted(f for f in os.listdir(ref_dir) if f.startswith("out_") and f.endswith(".npy")) \
            if os.path.isdir(ref_dir) else []
        if not outputs:
            results.append(("module " + case["name"] + " (no reference outputs)", 2))
        for output in outputs:
            label = case["name"] + "/" + output
            code = diff(args, os.path.join(ref_dir, output), os.path.join(args.test, "modules", case["name"], output),
                        case.get("tolerances", {}), os.path.join(reports, case["name"] + "__" + output + ".json"))
            results.append(("module " + label, code))

    for label, code in results:
        print({0: "pass ", 1: "FAIL ", 2: "ERROR"}.get(code, "ERROR") + " " + label)
    failed = sum(1 for _, code in results if code == 1)
    errors = sum(1 for _, code in results if code not in (0, 1))
    print("%d compared, %d failed, %d errors (reports in %s)" % (len(results), failed, errors, reports))
    if errors:
        return 2
    return 1 if failed else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    render = sub.add_parser("render", help="render all corpus cases")
    render.add_argument("--out", required=True)
    render.add_argument("--ref-render", default=DEFAULT_RENDER)
    render.add_argument("--ref-module", default=DEFAULT_MODULE)
    render.add_argument("--only", nargs="*", help="substrings of case names to run")

    compare = sub.add_parser("compare", help="compare two render trees")
    compare.add_argument("reference")
    compare.add_argument("test")
    compare.add_argument("--audio-diff", default=DEFAULT_DIFF)
    compare.add_argument("--exact", action="store_true", help="require bit-identical outputs")
    compare.add_argument("--only", nargs="*", help="substrings of case names to run")

    args = parser.parse_args()
    tools = [args.ref_render, args.ref_module] if args.command == "render" else [args.audio_diff]
    for tool in tools:
        if not os.access(tool, os.X_OK):
            print("missing tool: %s (build it first, see migration/README.md)" % tool, file=sys.stderr)
            return 2
    return cmd_render(args) if args.command == "render" else cmd_compare(args)


if __name__ == "__main__":
    sys.exit(main())
