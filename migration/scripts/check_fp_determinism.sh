#!/usr/bin/env bash
# Fails if the reference binaries contain instructions whose results depend
# on the CPU model: SSE approximate reciprocals (RCPPS/RCPSS/RSQRTPS/RSQRTSS),
# AVX-512 approximations, FMA (only possible if the ISA pin was overridden),
# or x87 transcendentals; or if they call glibc libmvec (whose SSE kernels use
# RCPPS/RSQRTPS internally). See migration/decisions/reference-determinism.md.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
build="${REF_BUILD_DIR:-$here/../reference/build}"

# Functions that contain such an instruction but that the engine never calls.
# Every entry needs a justification.
allowlist=(
  # JUCE FIR filter design; nothing under vital/src references juce::dsp::FIR.
  # GCC emits RSQRTSS for 1/sqrtf whenever reciprocal-math is on (-ffast-math).
  "juce::dsp::FIR::Coefficients<float>::normalise()"
)

pattern='(rcpps|rcpss|rsqrtps|rsqrtss|vrcp14|vrsqrt14|vrcp28|vrsqrt28|vfmadd|vfmsub|vfnmadd|vfnmsub|fsin|fcos|fsincos|fptan|fpatan|f2xm1|fyl2x|fyl2xp1)'
status=0

for bin in ref-render ref-module ref-preset; do
  path="$build/$bin"
  [[ -x "$path" ]] || { echo "missing $path" >&2; exit 2; }
  hits="$(objdump -d -C --no-show-raw-insn "$path" | awk -v pat="^$pattern" '
    /^[0-9a-f]+ <.*>:$/ { fn = substr($0, index($0, "<") + 1); sub(/>:$/, "", fn); next }
    { split($0, cols, "\t"); split(cols[2], words, " "); if (words[1] ~ pat) print fn "\t" words[1] }
  ' | sort | uniq -c)"

  unexpected=""
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    fn="$(echo "$line" | sed -E 's/^ *[0-9]+ //' | cut -f1)"
    allowed=0
    for entry in "${allowlist[@]}"; do
      [[ "$fn" == "$entry" ]] && allowed=1
    done
    [[ $allowed -eq 0 ]] && unexpected+="$line"$'\n'
  done <<< "$hits"

  libmvec="$(nm -D --undefined-only "$path" | grep -E ' _ZGV' || true)"
  if [[ -n "$libmvec" ]]; then
    unexpected+="libmvec imports:"$'\n'"$libmvec"$'\n'
  fi

  if [[ -n "$unexpected" ]]; then
    echo "$bin: CPU-dependent floating-point instructions found:" >&2
    printf '%s' "$unexpected" >&2
    status=1
  else
    echo "$bin: no CPU-dependent floating-point instructions outside the allowlist"
  fi
done
exit $status
