#!/usr/bin/env bash
# Records the machine and build facts that can change reference output:
# CPU model and features, glibc's view of active CPU features (which picks
# x86 libm variants) or the macOS version (which identifies Apple's libm),
# whether libm has CPU-dispatched (IFUNC) entry points, library and compiler
# versions, and hashes of the reference binaries. Also runs ref-fp-probe
# (libm, approximate-instruction, contraction and toInt result hashes),
# ref-rng-probe (upstream RandomGenerator stream hashes) and ref-complex-probe
# (std::complex<float> division).
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
  for bin in ref-render ref-module ref-fp-probe ref-rng-probe ref-complex-probe; do
    if [[ -x "$build/$bin" ]]; then
      echo "$bin sha256 $(sha256 < "$build/$bin")"
      echo "$bin text sha256 $(text_section_hash "$build/$bin")"
    fi
  done
} > "$out/machine.txt"

if [[ -x "$build/ref-rng-probe" ]]; then
  "$build/ref-rng-probe" > "$out/rng_probe.json"
fi

if [[ "$os" == "Darwin" ]]; then
  # The libc++ std::complex division the reference compiles (A10).
  header="$(xcrun --show-sdk-path)/usr/include/c++/v1/complex"
  {
    echo "## $header"
    grep -n "_LIBCPP_VERSION\|__builtin\|_Complex" "$header" || true
    echo "## operator/"
    grep -n -A45 "operator/(const complex<_Tp>& __z, const complex<_Tp>& __w)" "$header" || true
  } > "$out/libcxx_complex_division.txt" 2>&1
  grep -h "define _LIBCPP_VERSION" "$(xcrun --show-sdk-path)/usr/include/c++/v1/__config" >> "$out/libcxx_complex_division.txt" 2>/dev/null || true
  # Whether the engine calls libSystem's __divsc3 / __mulsc3, and the
  # phase modifier's division as compiled (fused instructions counted).
  render_symbol=__ZN13PhaseModifier21PhaseModifierKeyframe6renderEPN5vital9WaveFrameE
  {
    for bin in ref-render ref-module ref-complex-probe; do
      [[ -x "$build/$bin" ]] || continue
      echo "## $bin imports: $(nm -u "$build/$bin" | grep -E '___(div|mul)[sd]c3' | tr '\n' ' ')"
    done
    if [[ -x "$build/ref-module" ]]; then
      objdump -d --no-show-raw-insn --disassemble-symbols="$render_symbol" "$build/ref-module" > "$out/phase_modifier_render.s"
      echo "## render: $(grep -cE '\sf(n)?m(add|sub|la|ls)\s' "$out/phase_modifier_render.s") fused, $(grep -cE '\sfdiv\s' "$out/phase_modifier_render.s") fdiv, $(grep -cE '\sbl\s' "$out/phase_modifier_render.s") calls: $(grep -oE '\sbl\s.*' "$out/phase_modifier_render.s" | sort | uniq -c | tr '\n' ';')"
    fi
    # A11: every function that calls Apple's merged sin/cos (__sincosf_stret,
    # __sincos_stret), demangled, with call counts.
    for bin in ref-render ref-module; do
      [[ -x "$build/$bin" ]] || continue
      otool -tV "$build/$bin" \
        | awk '/^[^ \t].*:$/ { fn = substr($0, 1, length($0) - 1) } /\tbl\t___sincos(f)?_stret/ { print $NF "\t" fn }' \
        | sort | uniq -c | c++filt > "$out/sincos_call_sites_$bin.txt"
      echo "## $bin __sincos(f)_stret call sites: $(wc -l < "$out/sincos_call_sites_$bin.txt")"
    done
    # The probe's direct __divsc3 call links compiler-rt's builtin statically.
    if [[ -x "$build/ref-complex-probe" ]]; then
      objdump -d --no-show-raw-insn --disassemble-symbols=___divsc3 "$build/ref-complex-probe" > "$out/divsc3.s"
      echo "## ___divsc3 defined in ref-complex-probe: $(nm "$build/ref-complex-probe" | grep -c ' T ___divsc3'), $(grep -cE '\sf(n)?m(add|sub)\s' "$out/divsc3.s") fused, $(grep -cE '\sfdiv\s' "$out/divsc3.s") fdiv"
    fi
  } >> "$out/libcxx_complex_division.txt" 2>&1
fi

if [[ -x "$build/ref-complex-probe" ]]; then
  "$build/ref-complex-probe" > "$out/complex_probe.json"
fi

if [[ -x "$build/ref-fp-probe" ]]; then
  "$build/ref-fp-probe" > "$out/fp_probe.json"
  if [[ "$os" == "Linux" && "$arch" == "x86_64" ]]; then
    GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX2,-FMA "$build/ref-fp-probe" > "$out/fp_probe_no_fma.json"
  fi
fi

cat "$out/machine.txt"
echo "wrote $out"
