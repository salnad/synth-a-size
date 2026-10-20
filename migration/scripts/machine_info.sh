#!/usr/bin/env bash
# Records the machine and build facts that can change reference output:
# CPU model and flags, glibc's view of active CPU features (which picks libm
# variants), library and compiler versions, and hashes of the reference
# binaries. Also runs ref-libm-probe. Writes into OUT_DIR (default build/diag).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
build="${REF_BUILD_DIR:-$repo/migration/reference/build}"
out="${1:-$repo/build/diag}"
mkdir -p "$out"

{
  echo "## cpu"
  grep -m1 "model name" /proc/cpuinfo || true
  grep -m1 -E "^(cpu family|model)\s" /proc/cpuinfo || true
  echo "nproc: $(nproc)"
  echo "## cpu flags"
  grep -m1 "^flags" /proc/cpuinfo | cut -d: -f2 | tr ' ' '\n' | sed '/^$/d' | sort | tr '\n' ' '
  echo
  echo "## glibc cpu features (ld.so --list-diagnostics)"
  /lib64/ld-linux-x86-64.so.2 --list-diagnostics 2>/dev/null \
    | grep -E "^x86\.cpu_features\.(basic|features\.active|isa_1)" || echo "unavailable"
  echo "## packages"
  dpkg-query -W -f='${Package} ${Version}\n' libc6 libstdc++6 gcc-13 g++-13 2>/dev/null || true
  echo "## compiler"
  "${CXX:-g++}" --version | head -1
  echo "## binaries"
  for bin in ref-render ref-module ref-libm-probe; do
    if [[ -x "$build/$bin" ]]; then
      echo "$bin sha256 $(sha256sum "$build/$bin" | cut -d' ' -f1)"
      echo "$bin .text sha256 $(objcopy -O binary --only-section=.text "$build/$bin" /dev/stdout | sha256sum | cut -d' ' -f1)"
    fi
  done
} > "$out/machine.txt"

if [[ -x "$build/ref-libm-probe" ]]; then
  "$build/ref-libm-probe" > "$out/libm_probe.json"
  GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX2,-FMA "$build/ref-libm-probe" > "$out/libm_probe_no_fma.json"
fi

cat "$out/machine.txt"
echo "wrote $out"
