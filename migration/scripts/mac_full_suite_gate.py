#!/usr/bin/env python3
"""Whether reference-arm-main.yml runs its full Apple Silicon suite.

    mac_full_suite_gate.py --event EVENT [--every N] [--before SHA]

Prints `run=true|false` and `reason=...` (append to $GITHUB_OUTPUT).

- schedule and workflow_dispatch: always.
- push to main: when at least N first-parent commits (merges) have landed
  since the head commit of the last run whose macOS 15 job executed (it
  finished, or is queued or running), or when there is no such run; also
  when the push (BEFORE..HEAD) changes this workflow or its scripts, so CI
  changes are exercised on main at once.

Needs `gh` with a token that can read Actions ($GH_TOKEN), and a full clone.
"""

import argparse
import json
import os
import subprocess
import sys

WORKFLOW = "reference-arm-main.yml"
SUITE_JOB = "macOS 15 arm64 full suite"
SUITE_FILES = (
    ".github/workflows/reference-arm-main.yml",
    "migration/scripts/compare_known_drift.py",
    "migration/scripts/mac_full_suite_gate.py",
    "migration/scripts/mac_full_suite_report.py",
    "migration/scripts/mac_hash_backfill.sh",
    "migration/scripts/mac_missing_only.py",
)


def gh_api(path):
    return json.loads(subprocess.run(["gh", "api", path], check=True, stdout=subprocess.PIPE,
                                     universal_newlines=True).stdout)


def last_suite_sha(repo, current_run):
    runs = gh_api("repos/%s/actions/workflows/%s/runs?branch=main&per_page=50" % (repo, WORKFLOW))
    for run in runs["workflow_runs"]:
        if str(run["id"]) == str(current_run):
            continue
        jobs = gh_api("repos/%s/actions/runs/%s/jobs?per_page=100" % (repo, run["id"]))["jobs"]
        if any(j["name"] == SUITE_JOB and j["conclusion"] != "skipped" for j in jobs):
            return run["head_sha"]
    return None


def merges_since(sha):
    if subprocess.run(["git", "merge-base", "--is-ancestor", sha, "HEAD"]).returncode != 0:
        return None
    return int(subprocess.run(["git", "rev-list", "--first-parent", "--count", "%s..HEAD" % sha], check=True,
                              stdout=subprocess.PIPE, universal_newlines=True).stdout)


def suite_files_changed(before):
    if not before or set(before) == {"0"}:
        return []
    diff = subprocess.run(["git", "diff", "--name-only", before, "HEAD"], stdout=subprocess.PIPE,
                          stderr=subprocess.DEVNULL, universal_newlines=True)
    if diff.returncode != 0:
        return []
    return sorted(set(diff.stdout.split()) & set(SUITE_FILES))


def decide(event, every, repo, current_run, before=""):
    if event != "push":
        return True, "%s always runs the full suite" % event
    changed = suite_files_changed(before)
    if changed:
        return True, "this push changes the full suite itself: %s" % ", ".join(changed)
    try:
        sha = last_suite_sha(repo, current_run)
    except subprocess.CalledProcessError as error:
        return True, "could not read earlier runs (%s); running to be safe" % error
    if sha is None:
        return True, "no earlier run executed the full suite"
    count = merges_since(sha)
    if count is None:
        return True, "the last full-suite head %s is not on main's history" % sha[:7]
    if count >= every:
        return True, "%d merges since the last full suite at %s (every %d)" % (count, sha[:7], every)
    return False, "%d of %d merges since the last full suite at %s" % (count, every, sha[:7])


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--event", required=True)
    parser.add_argument("--every", type=int, default=6)
    parser.add_argument("--before", default="", help="the push's previous head (github.event.before)")
    args = parser.parse_args()
    run, reason = decide(args.event, args.every, os.environ["GITHUB_REPOSITORY"], os.environ.get("GITHUB_RUN_ID", ""),
                         args.before)
    print("run=%s" % ("true" if run else "false"))
    print("reason=%s" % reason)
    return 0


if __name__ == "__main__":
    sys.exit(main())
