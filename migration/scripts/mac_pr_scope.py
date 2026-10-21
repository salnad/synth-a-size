#!/usr/bin/env python3
"""What the pull-request macOS job runs, from the PR's diff.

    mac_pr_scope.py BASE_REVISION

Prints `key=value` lines (append them to $GITHUB_OUTPUT):

- cargo_args: `--workspace`, `-p <crate> ...` (the port crates the PR touches
  plus their in-workspace dependents) or empty (no parity tests).
  Everything runs when a shared input changes: the corpus, the reference
  harness, sas-parity / sas-common / sas-simd, the workspace manifests or
  toolchain, or this workflow and its tooling.
- justify: `true` when the PR adds or edits a no-fast-math allowlist line
  declaring `macos` (that run is the evidence for the declaration) or
  changes the justification tooling.
- proxy: `true` when the no-fast-math build is needed (parity tests or
  justify).
"""

import json
import os
import re
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

SHARED = (
    "migration/corpus/",
    "migration/reference/",
    "migration/scripts/run_corpus.py",
    "migration/scripts/mac_pr_scope.py",
    ".github/workflows/reference-arm.yml",
    "port/Cargo.toml",
    "port/Cargo.lock",
    "rust-toolchain.toml",
    "port/rust-toolchain.toml",
    "port/crates/parity/",
    "port/crates/common/",
    "port/crates/simd/",
)
JUSTIFY_TOOLING = ("migration/scripts/run_corpus.py", ".github/workflows/reference-arm.yml")
ALLOWLIST_DIR = "migration/decisions/nofastmath/"
MACOS_LINE = re.compile(r"^\+\s*- `[^`]+` \([^;)]*;[^)]*\bmacos\b[^)]*\)")


def git(*args):
    return subprocess.run(["git", "-C", REPO] + list(args), check=True, stdout=subprocess.PIPE,
                          universal_newlines=True).stdout


def workspace_crates():
    """{crate directory under port/crates: (package name, {workspace deps})}."""
    meta = json.loads(subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--manifest-path",
         os.path.join(REPO, "port", "Cargo.toml")], check=True, stdout=subprocess.PIPE,
        universal_newlines=True).stdout)
    names = {p["name"] for p in meta["packages"]}
    crates = {}
    for package in meta["packages"]:
        directory = os.path.relpath(os.path.dirname(package["manifest_path"]), REPO)
        deps = {d["name"] for d in package["dependencies"] if d["name"] in names}
        crates[directory] = (package["name"], deps)
    return crates


def dependents(selected, crates):
    """`selected` plus every workspace crate that depends on one of them."""
    result = set(selected)
    changed = True
    while changed:
        changed = False
        for name, deps in crates.values():
            if name not in result and deps & result:
                result.add(name)
                changed = True
    return result


def main():
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    base = sys.argv[1]
    changed = [f for f in git("diff", "--name-only", base + "...HEAD").splitlines() if f]

    if any(f.startswith(SHARED) for f in changed):
        cargo_args = "--workspace"
    else:
        crates = workspace_crates()
        touched = {name for directory, (name, _) in crates.items()
                   if any(f.startswith(directory + "/") for f in changed)}
        cargo_args = " ".join("-p " + n for n in sorted(dependents(touched, crates)))

    allowlist_diff = git("diff", "-U0", base + "...HEAD", "--", ALLOWLIST_DIR)
    justify = any(MACOS_LINE.match(line) for line in allowlist_diff.splitlines()) or \
        any(f in JUSTIFY_TOOLING for f in changed)

    print("cargo_args=%s" % cargo_args)
    print("justify=%s" % ("true" if justify else "false"))
    print("proxy=%s" % ("true" if cargo_args or justify else "false"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
