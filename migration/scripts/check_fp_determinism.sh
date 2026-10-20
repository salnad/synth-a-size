#!/usr/bin/env bash
# Fails if the reference binaries contain floating-point instructions whose
# results depend on the CPU model or that break agreement with the x86
# reference, or call vendor vector-math libraries:
#   x86_64:  SSE approximate reciprocals (RCPPS/RCPSS/RSQRTPS/RSQRTSS),
#            AVX-512 approximations, FMA (only possible if the ISA pin was
#            overridden), x87 transcendentals;
#   aarch64: reciprocal / rsqrt estimates and steps (FRECPE/FRECPS/FRSQRTE/
#            FRSQRTS) and any fused multiply-add (FMADD/FMSUB/FNMADD/FNMSUB/
#            FMLA/FMLS), which appear when FP contraction is on;
#   imports: glibc libmvec (_ZGV*), and on macOS Accelerate vDSP / vForce
#            (the reference pins JUCE's FFT to its portable fallback).
# See migration/decisions/reference-determinism.md and reference-arm.md.
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

case "$(uname -m)" in
  x86_64|amd64)
    pattern='(rcpps|rcpss|rsqrtps|rsqrtss|vrcp14|vrsqrt14|vrcp28|vrsqrt28|vfmadd|vfmsub|vfnmadd|vfnmsub|fsin|fcos|fsincos|fptan|fpatan|f2xm1|fyl2x|fyl2xp1)' ;;
  aarch64|arm64)
    pattern='(frecpe|frecps|frsqrte|frsqrts|fmadd|fmsub|fnmadd|fnmsub|fmla|fmls|fmlal|fmlsl)' ;;
  *)
    echo "unsupported architecture $(uname -m)" >&2; exit 2 ;;
esac

undefined_symbols() {
  if [[ "$(uname -s)" == "Darwin" ]]; then nm -u "$1"; else nm -D --undefined-only "$1"; fi
}

status=0

for bin in ref-render ref-module; do
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

  vector_math="$(undefined_symbols "$path" | grep -E '(^| )_?_ZGV|(^| )_vDSP_|(^| )_vv[a-z]' || true)"
  if [[ -n "$vector_math" ]]; then
    unexpected+="vector math library imports:"$'\n'"$vector_math"$'\n'
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
