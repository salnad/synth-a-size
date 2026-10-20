#!/usr/bin/env bash
# Builds the C++ reference harness (ref-render, ref-module) into
# migration/reference/build. Extra arguments are passed to CMake configure,
# e.g. -DREF_FAST_MATH=OFF.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
src="$here/../reference"
build="${REF_BUILD_DIR:-$src/build}"
jobs="${JOBS:-$( (nproc || sysctl -n hw.ncpu) 2>/dev/null || echo 4)}"

# Upstream's Linux builds use GCC; prefer it when available.
if [[ -z "${CXX:-}" ]] && command -v g++ >/dev/null 2>&1; then
  export CXX=g++
fi

cmake -S "$src" -B "$build" -DCMAKE_BUILD_TYPE=Release "$@"
cmake --build "$build" -j "$jobs"
echo "built: $build/ref-render $build/ref-module"
