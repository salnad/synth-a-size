#!/usr/bin/env bash
# Opens the bundle's editor in a real window with vst3-editor-smoke and checks
# the plugin's log: the editor must present a frame, and nothing may panic
# (caught or not) or raise a wgpu error. On Linux run it under xvfb-run.
#
#   port/crates/plugin/scripts/editor_smoke.sh <vst3-editor-smoke> <bundle.vst3> <log>
set -euo pipefail

tool="$1"
bundle="$2"
log="$3"

# nice-plug's logger (the plugin's `tracing-subscriber` feature) writes the
# editor's log lines and any panic to STDERR.
NICE_LOG=stderr "$tool" "$bundle" --seconds 5 2>&1 | tee "$log"

if ! grep -q "presented the first frame" "$log"; then
  echo "editor smoke: the editor never presented a frame" >&2
  exit 1
fi
if grep -E "panicked|wgpu error|is invalid" "$log"; then
  echo "editor smoke: the editor panicked or hit a wgpu error (lines above)" >&2
  exit 1
fi
echo "editor smoke: passed"
