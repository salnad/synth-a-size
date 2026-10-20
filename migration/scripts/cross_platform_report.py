#!/usr/bin/env python3
"""Compare reference render trees from different platforms or build variants.

Each PAIR is LABEL=REFERENCE_TREE:TEST_TREE (trees as written by
`run_corpus.py render`). For each pair this runs `run_corpus.py compare` with
the corpus tolerances, collects the per-case audio-diff reports, and writes:

  OUT/report.md    markdown table: per pair, pass/fail count and worst metrics
  OUT/report.json  every case's metrics, per pair

The report never changes tolerances. Exit status: 0 unless a pair listed with
--gate fails its tolerances, or a comparison errors.

Example:
  cross_platform_report.py --out build/cross \\
    --pair "aarch64-linux vs x86_64=build/x86:build/arm" --gate "aarch64-linux vs x86_64"
"""

import argparse
import glob
import json
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RUN_CORPUS = os.path.join(HERE, "run_corpus.py")


def as_float(value):
    if isinstance(value, str):
        return {"inf": float("inf"), "-inf": float("-inf")}.get(value, float("nan"))
    return float(value)


def fmt(value, kind):
    if value == float("-inf"):
        return "-inf"
    if kind == "abs":
        return "%.2e" % value
    return "%.4g" % value


def compare_pair(label, reference, test, audio_diff):
    work = os.path.join(test, "reports")
    if os.path.isdir(work):
        shutil.rmtree(work)
    command = [sys.executable, RUN_CORPUS, "compare", reference, test]
    if audio_diff:
        command += ["--audio-diff", audio_diff]
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    cases = {}
    for path in sorted(glob.glob(os.path.join(work, "*.json"))):
        with open(path) as handle:
            report = json.load(handle)
        cases[os.path.basename(path)[:-5]] = {
            "pass": report["pass"],
            "max_abs_error": as_float(report["max_abs_error"]),
            "residual_db": as_float(report["residual_db"]),
            "lsd_db": as_float(report["lsd_db"]),
            "rms_diff_db": as_float(report["rms_diff_db"]),
        }
    return {"label": label, "reference": reference, "test": test, "exit": result.returncode,
            "output": result.stdout, "cases": cases}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", required=True)
    parser.add_argument("--pair", action="append", default=[], help="LABEL=REFERENCE_TREE:TEST_TREE")
    parser.add_argument("--gate", action="append", default=[], help="label of a pair that must pass")
    parser.add_argument("--audio-diff")
    args = parser.parse_args()

    os.makedirs(args.out, exist_ok=True)
    results = []
    for spec in args.pair:
        label, trees = spec.split("=", 1)
        reference, test = trees.split(":", 1)
        if not (os.path.isdir(reference) and os.path.isdir(test)):
            print("skipping %s: tree missing (%s)" % (label, reference if not os.path.isdir(reference) else test))
            if label in args.gate:
                return 2
            continue
        results.append(compare_pair(label, reference, test, args.audio_diff))

    lines = ["| Pair | Cases passing | Worst max abs | Worst residual dB | Worst LSD dB | Worst RMS diff dB | Worst case (residual) |",
             "|---|---|---|---|---|---|---|"]
    failed_gate = False
    for result in results:
        cases = result["cases"]
        if not cases:
            lines.append("| %s | error (exit %d) | | | | | |" % (result["label"], result["exit"]))
            failed_gate = failed_gate or result["label"] in args.gate
            continue
        passing = sum(1 for c in cases.values() if c["pass"])
        worst = lambda key: max(c[key] for c in cases.values())
        worst_case = max(cases, key=lambda name: cases[name]["residual_db"])
        lines.append("| %s | %d/%d | %s | %s | %s | %s | %s |" % (
            result["label"], passing, len(cases), fmt(worst("max_abs_error"), "abs"),
            fmt(worst("residual_db"), "db"), fmt(worst("lsd_db"), "db"), fmt(worst("rms_diff_db"), "db"), worst_case))
        if result["label"] in args.gate and (passing != len(cases) or result["exit"] == 2):
            failed_gate = True

    lines.append("")
    for result in results:
        lines.append("<details><summary>%s</summary>\n" % result["label"])
        lines.append("| Case | Pass | max abs | residual dB | LSD dB | RMS diff dB |\n|---|---|---|---|---|---|")
        for name, c in sorted(result["cases"].items()):
            lines.append("| %s | %s | %s | %s | %s | %s |" % (
                name, "yes" if c["pass"] else "**no**", fmt(c["max_abs_error"], "abs"),
                fmt(c["residual_db"], "db"), fmt(c["lsd_db"], "db"), fmt(c["rms_diff_db"], "db")))
        lines.append("\n</details>\n")

    markdown = "\n".join(lines) + "\n"
    with open(os.path.join(args.out, "report.md"), "w") as handle:
        handle.write(markdown)
    with open(os.path.join(args.out, "report.json"), "w") as handle:
        json.dump(results, handle, indent=2, default=str)
    print(markdown)
    return 1 if failed_gate else 0


if __name__ == "__main__":
    sys.exit(main())
