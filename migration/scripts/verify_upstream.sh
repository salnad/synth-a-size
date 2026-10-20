#!/usr/bin/env bash
# Verifies that /vital is an unmodified copy of upstream at the commit in
# migration/UPSTREAM_SHA, apart from the documented exclusions in
# migration/UPSTREAM.md.
#
#   migration/scripts/verify_upstream.sh                 # clones upstream
#   migration/scripts/verify_upstream.sh /path/to/clone  # uses a local clone
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
sha="$(tr -d '[:space:]' < "$repo/migration/UPSTREAM_SHA")"
upstream_url="https://github.com/mtytel/vital.git"

# Keep in sync with the "Exclusions" table in migration/UPSTREAM.md.
excluded=(
  "third_party/firebase_cpp_sdk/libs"
  "third_party/firebase_cpp_sdk/frameworks"
)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

clone="${1:-}"
if [[ -z "$clone" ]]; then
  clone="$work/clone"
  git init -q "$clone"
  git -C "$clone" remote add origin "$upstream_url"
  git -C "$clone" fetch -q --depth 1 origin "$sha"
fi

mkdir -p "$work/expected"
git -C "$clone" archive "$sha" | tar -x -C "$work/expected"
for path in "${excluded[@]}"; do
  rm -rf "${work:?}/expected/$path"
done

if diff -r --no-dereference "$work/expected" "$repo/vital" > "$work/diff.txt"; then
  count="$(find "$repo/vital" -type f | wc -l | tr -d ' ')"
  echo "vital/ matches upstream $sha ($count files, exclusions: ${excluded[*]})"
else
  echo "vital/ differs from upstream $sha:" >&2
  head -50 "$work/diff.txt" >&2
  exit 1
fi
