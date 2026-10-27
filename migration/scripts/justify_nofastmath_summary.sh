#!/usr/bin/env bash
# The macOS no-fast-math justification (SAS-35), with its result in the job's
# step summary so reviewers can cite it:
#
#   justify_nofastmath_summary.sh OFFICIAL_TREE NOFASTMATH_TREE
#
# Runs `run_corpus.py compare OFFICIAL NOFASTMATH --justify-nofastmath
# --parked migration/corpus/parked_arm64.json --justify-exceptions
# migration/corpus/macos_justify_exceptions.json` and exits with its status.
set -uo pipefail

if [[ $# -ne 2 ]]; then
  sed -n '2,9p' "$0" >&2
  exit 2
fi
repo=$(cd "$(dirname "$0")/../.." && pwd)
log=$(mktemp)
python3 "$repo/migration/scripts/run_corpus.py" compare "$1" "$2" --justify-nofastmath \
  --parked "$repo/migration/corpus/parked_arm64.json" \
  --justify-exceptions "$repo/migration/corpus/macos_justify_exceptions.json" | tee "$log"
status=${PIPESTATUS[0]}
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo "### No-fast-math justification (SAS-35): $([[ $status -eq 0 ]] && echo pass || echo FAIL)"
    echo
    echo "Commit ${GITHUB_SHA:-unknown}, run ${GITHUB_SERVER_URL:-}/${GITHUB_REPOSITORY:-}/actions/runs/${GITHUB_RUN_ID:-}"
    echo
    echo "Justification lines:"
    echo '```'
    grep -E 'nofastmath|justif|not gated|parked|excepted' "$log" || echo "(no reference \"nofastmath\" cases)"
    echo '```'
    echo
    echo "$(tail -1 "$log")"
  } >> "$GITHUB_STEP_SUMMARY"
fi
rm -f "$log"
exit "$status"
