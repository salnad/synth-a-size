#!/usr/bin/env python3
"""Did a reference-arm-main.yml macOS run fail only on missing macOS hash keys?

    mac_missing_only.py --steps-json JSON --hash-log FILE [--hash-log FILE]
                        --parity-log FILE --patch FILE

Prints `missing_only=true|false` and `reason=...` (append to $GITHUB_OUTPUT).
It is true only when all of these hold:
- every step with an id passed, except the parity tests (`parity`) and the
  hash checks (`hashes`), and at least one of those two failed;
- every hash-check problem is a missing key or a hash file without this
  build identity. A MISMATCH, a stored hash whose case was not rendered, a
  stale hash or a tree problem is a real failure;
- every parity panic is sas-parity's missing-hash panic (no `parity FAILED`,
  no other panic, no build error);
- the run wrote a non-empty macos-hashes patch that only adds keys to
  `.hashes.json` files.
"""

import argparse
import json
import re
import sys

ALLOWED_FAILED_STEPS = {"parity", "hashes"}
MISSING_HASH = re.compile(r"has no hashes for the CI build identity|no stored hash for ")
HASH_SUMMARY = re.compile(r"^\d+ hashes checked, \d+ problems")
PARKED = re.compile(r"^parked ")
PANIC_MISSING = re.compile(r"sas-parity: .*(has no committed hash for|has no hashes for the CI build identity)")


def hash_log_ok(text):
    """None if every problem line in a `hashes --check` log is a missing hash."""
    for line in text.splitlines():
        line = line.strip()
        if not line or HASH_SUMMARY.match(line) or PARKED.match(line) or line.startswith("warning: "):
            continue
        if line.startswith("::"):
            continue
        if not MISSING_HASH.search(line):
            return "hash check: " + line
    return None


def parity_log_ok(text):
    """None if every failure in a `cargo test` log is the missing-hash panic."""
    if re.search(r"^error(\[E\d+\])?: (could not compile|aborting)", text, re.M):
        return "parity: build error"
    if "parity FAILED" in text:
        return "parity: a comparison failed"
    lines = text.splitlines()
    panics = 0
    for i, line in enumerate(lines):
        if "panicked at" in line:
            panics += 1
            message = lines[i + 1] if i + 1 < len(lines) else ""
            if not PANIC_MISSING.search(message):
                return "parity: " + (message.strip() or line.strip())
    failed = sum(int(n) for n in re.findall(r"^test result: FAILED\. \d+ passed; (\d+) failed", text, re.M))
    if failed and not panics:
        return "parity: %d failed tests without a panic message" % failed
    return None


def patch_ok(text):
    """None if the patch only adds keys to .hashes.json files."""
    if not text.strip():
        return "no macos-hashes patch was written"
    files = re.findall(r"^\+\+\+ b/(\S+)", text, re.M)
    if not files or any(not f.endswith(".hashes.json") for f in files):
        return "the patch touches files other than .hashes.json: %s" % ", ".join(files)
    added = {l[1:] for l in text.splitlines() if l.startswith("+") and not l.startswith("+++")}
    for line in text.splitlines():
        if line.startswith("-") and not line.startswith("---"):
            # --write --only-new only appends; a removed line may only come
            # back unchanged with a trailing comma.
            if line[1:].rstrip() + "," not in added:
                return "the patch changes an existing line: " + line.strip()
    return None


def classify(steps, hash_logs, parity_log, patch):
    failed = sorted(i for i, s in steps.items() if s.get("outcome") not in ("success", "skipped"))
    if not failed:
        return False, "no step failed"
    other = [i for i in failed if i not in ALLOWED_FAILED_STEPS]
    if other:
        return False, "other steps failed: " + ", ".join(other)
    for text in hash_logs:
        problem = hash_log_ok(text)
        if problem:
            return False, problem
    problem = parity_log_ok(parity_log) or patch_ok(patch)
    if problem:
        return False, problem
    return True, "only missing macOS hash keys (failed: %s)" % ", ".join(failed)


def read(path):
    try:
        with open(path) as handle:
            return handle.read()
    except OSError:
        return ""


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--steps-json", required=True, help="the job's steps context, as JSON")
    parser.add_argument("--hash-log", action="append", default=[])
    parser.add_argument("--parity-log", required=True)
    parser.add_argument("--patch", required=True)
    args = parser.parse_args()
    steps = json.loads(args.steps_json)
    ok, reason = classify(steps, [read(p) for p in args.hash_log], read(args.parity_log), read(args.patch))
    print("missing_only=%s" % ("true" if ok else "false"))
    print("reason=%s" % reason.replace("\n", " "))
    return 0


if __name__ == "__main__":
    sys.exit(main())
