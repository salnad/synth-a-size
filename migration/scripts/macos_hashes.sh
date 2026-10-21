#!/usr/bin/env bash
# Commit-ready macOS hash entries for new cases, from a Reference ARM run.
#
#   migration/scripts/macos_hashes.sh <run-id>
#
# Applies the run's `macos-hashes` patch (new keys only, written by the job).
# If the run has no patch or it no longer applies, writes the entries from the
# run's ref-arm64-macos-15 capture trees instead, with
# `run_corpus.py hashes --write --only-new` (pinned and no-fast-math). Either
# way existing hashes are never changed. Review `git diff` and commit.
set -euo pipefail

if [[ $# -ne 1 ]]; then
  sed -n '2,10p' "$0" >&2
  exit 2
fi
run=$1
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

if gh run download "$run" -n macos-hashes -D "$tmp/patch" 2>/dev/null \
    && git apply --check "$tmp/patch/macos-hashes.patch" 2>/dev/null; then
  git apply "$tmp/patch/macos-hashes.patch"
  echo "applied the macos-hashes patch from run $run"
else
  echo "no applicable macos-hashes patch in run $run; writing from its capture trees"
  gh run download "$run" -n ref-arm64-macos-15 -D "$tmp/arm"
  parked=migration/corpus/parked_arm64.json
  python3 migration/scripts/run_corpus.py hashes "$tmp/arm/build/platform/pinned" --write --only-new --parked "$parked"
  if [[ -d "$tmp/arm/build/platform/proxy" ]]; then
    python3 migration/scripts/run_corpus.py hashes "$tmp/arm/build/platform/proxy" --write --only-new \
      --reference nofastmath --parked "$parked"
  fi
fi
git status --short -- migration/corpus
