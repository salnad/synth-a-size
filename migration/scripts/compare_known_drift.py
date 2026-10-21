#!/usr/bin/env python3
"""Exact comparison of two render trees that tolerates a committed list of
known platform-libm drift cases.

    compare_known_drift.py REFERENCE TEST DRIFT_JSON

Runs `run_corpus.py compare REFERENCE TEST --exact`. A failing case:output
listed under "cases" in DRIFT_JSON is reported with its metrics and does not
fail the run; any other failure or error does. Listed entries that no longer
differ are reported so the list can shrink. Used by the macOS 14 job
(the macOS 15 binary on macOS 14; decisions/reference-arm.md, section B).
"""

import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def main():
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    reference, test, drift_path = sys.argv[1:]
    with open(drift_path) as handle:
        drift = json.load(handle)["cases"]

    result = subprocess.run(
        [sys.executable, os.path.join(HERE, "run_corpus.py"), "compare", reference, test, "--exact"],
        stdout=subprocess.PIPE, universal_newlines=True)
    sys.stdout.write(result.stdout)
    failed = re.findall(r"^FAIL\s+(\S+)", result.stdout, re.M)
    errors = re.findall(r"^ERROR\s+(.+)$", result.stdout, re.M)
    if result.returncode not in (0, 1):
        print("compare exited with status %d" % result.returncode)
        return 1

    unexpected = [case for case in failed if case not in drift]
    print("\nKnown Apple libm drift (%s):" % os.path.relpath(drift_path))
    for case, reason in sorted(drift.items()):
        if case not in failed:
            print("  %s: no longer differs (the entry can be removed)" % case)
            continue
        report = os.path.join(test, "reports", case.replace("/", "__").replace(":", "__") + ".json")
        metrics = ""
        if os.path.exists(report):
            with open(report) as handle:
                data = json.load(handle)
            metrics = "max_abs_error %.3g, residual_db %.1f" % (data["max_abs_error"], data["residual_db"])
        print("  %s: differs (%s) - %s" % (case, metrics or "no report", reason))

    if unexpected or errors:
        print("\nNot on the drift list: %s" % ", ".join(unexpected + errors))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
