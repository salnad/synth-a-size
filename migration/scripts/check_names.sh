#!/usr/bin/env bash
# Fails if product-facing metadata uses upstream names. The rule (see
# migration/AGENTS.md): our product, crates, binaries, bundles and plugin IDs
# must never be called Vital, Vital Audio, Tytel or Matt Tytel. The vendored
# copy in /vital keeps its original names and is not checked.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
pattern='vital|tytel'

status=0
check() {
  local label="$1"; shift
  local hits
  hits="$(grep -EinH "$pattern" "$@" 2>/dev/null || true)"
  if [[ -n "$hits" ]]; then
    echo "banned upstream name in $label:" >&2
    echo "$hits" >&2
    status=1
  fi
}

# Package metadata of everything we build and ship.
mapfile -t manifests < <(find "$repo/port" "$repo/migration/tools" -name Cargo.toml -not -path '*/target/*')
check "Cargo manifests" "${manifests[@]}"

# The bundle's notices are user visible: they credit third parties without
# naming upstream or its author.
check "bundle notices" "$repo"/port/crates/plugin/notices/*

# Plugin identity (names, vendor, IDs, bundle metadata).
mapfile -t plugin_sources < <(find "$repo/port/crates/plugin" -type f \( -name '*.rs' -o -name '*.toml' \) -not -path '*/target/*' \
  -not -path '*/tests/*')
for file in "${plugin_sources[@]}"; do
  hits="$(grep -EinH "(NAME|VENDOR|CLAP_ID|CLASS_ID|BUNDLE|URL)[^=]*=.*($pattern)" "$file" || true)"
  if [[ -n "$hits" ]]; then
    echo "banned upstream name in plugin identity:" >&2
    echo "$hits" >&2
    status=1
  fi
done

# Editor text: no string literal in the editor's code may carry an upstream
# name (upstream's binary is also called "Vial"). Comments citing the
# upstream files a module ports, and paths into the vendored vital/ tree
# (UPSTREAM_SOURCES, the embedded skin and fonts), are allowed.
mapfile -t gui_sources < <(find "$repo/port/crates/gui/src" "$repo/port/crates/gui/examples" -type f -name '*.rs')
for file in "${gui_sources[@]}"; do
  hits="$(grep -EinH "\"[^\"]*(vital|vial|tytel)[^\"]*\"" "$file" \
    | grep -Ev '^[^:]+:[0-9]+:[[:space:]]*//' \
    | grep -Ev '"(\.\./)*vital/[^"]*"' || true)"
  if [[ -n "$hits" ]]; then
    echo "banned upstream name in editor text:" >&2
    echo "$hits" >&2
    status=1
  fi
done

if [[ $status -eq 0 ]]; then
  echo "name check passed (${#manifests[@]} manifests, ${#plugin_sources[@]} plugin sources, ${#gui_sources[@]} editor sources)"
fi
exit $status
