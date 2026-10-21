#!/usr/bin/env bash
# Records the machine and build facts that can change reference output:
# CPU model and features, glibc's view of active CPU features (which picks
# x86 libm variants) or the macOS version (which identifies Apple's libm),
# whether libm has CPU-dispatched (IFUNC) entry points, library and compiler
# versions, and hashes of the reference binaries. Also runs ref-fp-probe
# (libm, approximate-instruction, contraction and toInt result hashes) and
# ref-rng-probe (upstream RandomGenerator stream hashes).
# Linux x86_64, Linux aarch64 and macOS. Writes into OUT_DIR (default build/diag).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
build="${REF_BUILD_DIR:-$repo/migration/reference/build}"
out="${1:-$repo/build/diag}"
mkdir -p "$out"

os="$(uname -s)"
arch="$(uname -m)"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum | cut -d' ' -f1; else shasum -a 256 | cut -d' ' -f1; fi
}

text_section_hash() {
  local bin="$1"
  if [[ "$os" == "Darwin" ]]; then
    # __TEXT,__text section contents (otool prints them as hex words).
    otool -t -X "$bin" 2>/dev/null | sha256
  elif command -v objcopy >/dev/null 2>&1; then
    objcopy -O binary --only-section=.text "$bin" /dev/stdout | sha256
  else
    echo "unavailable"
  fi
}

{
  echo "## platform"
  echo "$os $arch"
  echo "## cpu"
  if [[ "$os" == "Darwin" ]]; then
    sysctl -n machdep.cpu.brand_string 2>/dev/null || true
    echo "macOS $(sw_vers -productVersion) ($(sw_vers -buildVersion))"
    echo "ncpu: $(sysctl -n hw.ncpu)"
    echo "## cpu features (hw.optional)"
    sysctl hw.optional 2>/dev/null | grep -E ": 1$" | sed 's/^hw.optional.//; s/: 1$//' | sort | tr '\n' ' '
    echo
  else
    grep -m1 -E "^(model name|CPU implementer)" /proc/cpuinfo || true
    grep -m1 -E "^(cpu family|model|CPU part|CPU variant)\s" /proc/cpuinfo || true
    lscpu 2>/dev/null | grep -E "^(Model name|Vendor ID)" || true
    echo "nproc: $(nproc)"
    echo "## cpu flags"
    grep -m1 -E "^(flags|Features)" /proc/cpuinfo | cut -d: -f2 | tr ' ' '\n' | sed '/^$/d' | sort | tr '\n' ' '
    echo
    echo "## glibc cpu features (ld.so --list-diagnostics)"
    ldso="$(ls /lib64/ld-linux-x86-64.so.2 /lib/ld-linux-aarch64.so.1 2>/dev/null | head -1 || true)"
    if [[ -n "$ldso" ]]; then
      "$ldso" --list-diagnostics 2>/dev/null \
        | grep -E "^(x86|aarch64)\.cpu_features" | grep -vE "preferred|cache|threshold" || echo "unavailable"
    fi
    echo "## libm CPU-dispatched (IFUNC) symbols"
    libm="$(ls /lib/x86_64-linux-gnu/libm.so.6 /lib/aarch64-linux-gnu/libm.so.6 2>/dev/null | head -1 || true)"
    if [[ -n "$libm" ]] && command -v readelf >/dev/null 2>&1; then
      count="$(readelf -W --dyn-syms "$libm" | awk '$4 == "IFUNC"' | wc -l | tr -d ' ')"
      echo "$libm: $count IFUNC symbols"
      readelf -W --dyn-syms "$libm" | awk '$4 == "IFUNC" {print $8}' | sed 's/@.*//' | sort -u | tr '\n' ' '
      echo
    fi
    echo "## packages"
    dpkg-query -W -f='${Package} ${Version}\n' libc6 libstdc++6 gcc-13 g++-13 2>/dev/null || true
  fi
  echo "## compiler"
  if [[ "$os" == "Darwin" ]]; then "${CXX:-c++}" --version | head -1; else "${CXX:-g++}" --version | head -1; fi
  echo "## binaries"
  for bin in ref-render ref-module ref-fp-probe ref-rng-probe; do
    if [[ -x "$build/$bin" ]]; then
      echo "$bin sha256 $(sha256 < "$build/$bin")"
      echo "$bin text sha256 $(text_section_hash "$build/$bin")"
    fi
  done
} > "$out/machine.txt"

if [[ -x "$build/ref-rng-probe" ]]; then
  "$build/ref-rng-probe" > "$out/rng_probe.json"
fi

if [[ -x "$build/ref-fp-probe" ]]; then
  "$build/ref-fp-probe" > "$out/fp_probe.json"
  if [[ "$os" == "Linux" && "$arch" == "x86_64" ]]; then
    GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX2,-FMA "$build/ref-fp-probe" > "$out/fp_probe_no_fma.json"
  fi
fi

cat "$out/machine.txt"
echo "wrote $out"
