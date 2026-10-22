#!/usr/bin/env bash
# Open or update the hash-only PR that backfills missing macOS hash keys on
# main (reference-arm-main.yml, `backfill` job; Mac CI policy).
#
#   mac_hash_backfill.sh PATCH
#
# Applies PATCH (the run's macos-hashes patch: `hashes --write --only-new`,
# new keys only) on top of the checked-out commit, commits it to the bot
# branch ci/macos-hashes-backfill (never main), opens the PR or updates the
# open one, and dispatches Linux CI on the branch (a PR opened with the
# workflow token triggers no workflows). Prints `pr=<url>`. If the repository
# does not allow Actions to open PRs, prints the branch's compare URL instead
# for a human or agent to open. Needs GH_TOKEN with contents, pull-requests
# and actions write, and RUN_ID / GITHUB_REPOSITORY / GITHUB_SERVER_URL.
set -euo pipefail

patch=$1
branch=ci/macos-hashes-backfill
title="macOS hashes backfill for main (hash-only, new keys)"
run_url="${GITHUB_SERVER_URL}/${GITHUB_REPOSITORY}/actions/runs/${RUN_ID}"

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git apply --check "$patch"
git apply "$patch"
if git diff --name-only | grep -qv '\.hashes\.json$'; then
  echo "refusing: the patch touches files other than .hashes.json" >&2
  exit 1
fi
git checkout -B "$branch"
git commit -qam "macOS arm64 hashes for new cases on main (new keys only, Reference ARM run ${RUN_ID})"
# The bot branch is rebuilt from the run's commit each time; main is never pushed.
git push --force origin "$branch"

body=$(mktemp)
{
  echo "Automated hash-only backfill (Mac CI policy, \`decisions/reference-arm.md\`)."
  echo
  echo "The full suite on main ([run ${RUN_ID}](${run_url})) failed only on missing macOS hash keys, with no"
  echo "mismatches (\`migration/scripts/mac_missing_only.py\`). This PR applies that run's \`macos-hashes\` patch"
  echo "(\`hashes --write --only-new\`: new keys only, existing hashes unchanged):"
  echo
  echo '```'
  git show --stat --format= HEAD
  echo '```'
  echo
  echo "Merge it on Linux CI (dispatched on this branch). A later run that finds more missing keys"
  echo "rebuilds this branch from its own commit and updates this PR."
} > "$body"

existing=$(gh pr list --head "$branch" --state open --json url --jq '.[0].url // empty')
if [[ -n "$existing" ]]; then
  gh pr edit "$existing" --title "$title" --body-file "$body" > /dev/null
  url=$existing
elif ! url=$(gh pr create --base main --head "$branch" --title "$title" --body-file "$body" 2>/tmp/pr-error); then
  echo "could not open the PR ($(cat /tmp/pr-error)); the branch is ready to open by hand" >&2
  url="${GITHUB_SERVER_URL}/${GITHUB_REPOSITORY}/compare/main...${branch}?expand=1"
fi
gh workflow run ci.yml --ref "$branch" || echo "could not dispatch ci.yml on ${branch}" >&2
echo "pr=${url}"
