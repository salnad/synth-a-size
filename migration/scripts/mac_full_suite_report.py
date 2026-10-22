#!/usr/bin/env python3
"""Report a reference-arm-main.yml run through one tracking issue.

Reads the workflow's `needs` context from $NEEDS (JSON). If a suite job
failed, opens the issue "Mac full suite failing on main", or comments on it
when it is already open, with the failing jobs and steps and what to do. When
every job passed, an open issue gets a closing comment and is closed.

Needs `gh` with a token that can write issues and read Actions ($GH_TOKEN).
"""

import json
import os
import subprocess
import sys
import tempfile

TITLE = "Mac full suite failing on main"
HASH_STEPS = {"Hashes match the committed macOS arm64 identity",
              "arm64 parity tests on the whole workspace (NEON path)"}


def gh(*args):
    return subprocess.run(["gh"] + list(args), check=True, stdout=subprocess.PIPE,
                          universal_newlines=True).stdout


def open_issue(repo):
    found = json.loads(gh("issue", "list", "--repo", repo, "--state", "open", "--search",
                          'in:title "%s"' % TITLE, "--json", "number,title"))
    return next((i["number"] for i in found if i["title"] == TITLE), None)


def failed_steps(repo, run_id):
    jobs = json.loads(gh("api", "repos/%s/actions/runs/%s/jobs?per_page=100" % (repo, run_id)))["jobs"]
    return {j["name"]: [s["name"] for s in j.get("steps", []) if s.get("conclusion") == "failure"]
            for j in jobs if j.get("conclusion") == "failure"}


def artifact_names(repo, run_id):
    data = json.loads(gh("api", "repos/%s/actions/runs/%s/artifacts?per_page=100" % (repo, run_id)))
    return {a["name"] for a in data["artifacts"]}


def body_for_failure(repo, run_id, url, sha, results, steps, artifacts):
    lines = ["The Apple Silicon full suite failed on main at %s ([run %s](%s))." % (sha[:12], run_id, url), "",
             "| Job | Result |", "|---|---|"]
    lines += ["| %s | %s |" % (job, result) for job, result in sorted(results.items())]
    if steps:
        lines += ["", "Failed steps:"]
        for job, names in sorted(steps.items()):
            lines.append("- %s: %s" % (job, ", ".join(names) or "(no step reported)"))
    macos_steps = set().union(*steps.values()) if steps else set()
    lines.append("")
    backfill = os.environ.get("BACKFILL_PR", "")
    reason = os.environ.get("MISSING_ONLY_REASON", "")
    if backfill:
        lines += ["Missing macOS hash keys only (%s). The backfill PR applies this run's patch, new keys only: "
                  "%s. Merge it on Linux CI. This issue closes on the next green full suite." % (reason, backfill)]
        return "\n".join(lines) + "\n"
    if reason:
        lines += ["Not a missing-keys-only failure: %s." % reason, ""]
    if "macos-hashes" in artifacts:
        lines += ["New cases on main have no macOS hashes. The run wrote them (new keys only); commit them "
                  "in a fix PR:", "", "```bash", "migration/scripts/macos_hashes.sh %s" % run_id, "```", ""]
        if macos_steps and macos_steps <= HASH_STEPS:
            lines.append("Only the parity and hash steps failed, so this may be just the missing hashes. "
                         "Check the parity log for other failures before assuming so.")
        else:
            lines.append("Other steps failed too: that is a real failure, open a fix PR.")
    else:
        lines.append("This is a real failure (not missing hashes): open a fix PR. Policy: "
                     "migration/decisions/reference-arm.md, \"Mac CI policy\".")
    return "\n".join(lines) + "\n"


def main():
    repo = os.environ["GITHUB_REPOSITORY"]
    run_id = os.environ["GITHUB_RUN_ID"]
    sha = os.environ["GITHUB_SHA"]
    url = "%s/%s/actions/runs/%s" % (os.environ.get("GITHUB_SERVER_URL", "https://github.com"), repo, run_id)
    needs = json.loads(os.environ["NEEDS"])
    results = {job: data["result"] for job, data in needs.items() if job != "gate"}
    failed = [job for job, result in results.items() if result in ("failure", "cancelled")]
    issue = open_issue(repo)

    with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as handle:
        if failed:
            body = body_for_failure(repo, run_id, url, sha, results, failed_steps(repo, run_id),
                                    artifact_names(repo, run_id))
        else:
            body = "The Apple Silicon full suite passes on main again at %s ([run %s](%s)).\n" % (sha[:12], run_id, url)
        handle.write(body)
        path = handle.name

    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as handle:
            handle.write(body)
    if failed:
        if issue:
            gh("issue", "comment", str(issue), "--repo", repo, "--body-file", path)
            print("commented on #%s" % issue)
        else:
            print(gh("issue", "create", "--repo", repo, "--title", TITLE, "--body-file", path).strip())
    elif issue:
        gh("issue", "comment", str(issue), "--repo", repo, "--body-file", path)
        gh("issue", "close", str(issue), "--repo", repo)
        print("closed #%s" % issue)
    else:
        print("all jobs passed; no open issue")
    return 0


if __name__ == "__main__":
    sys.exit(main())
