#!/usr/bin/env bash
# Shows whether the C++ compiler honours -ffp-contract=off under various
# fast-math flag sets, by compiling a tiny a + b * c kernel (scalar and
# vector) and counting fused multiply-add instructions in the assembly.
# Used to explain why Apple clang 15 fused despite -ffp-contract=off
# (migration/decisions/reference-arm.md).
set -euo pipefail

cxx="${CXX:-c++}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cat > "$work/kernel.cpp" <<'EOF'
float scalar(float a, float b, float c) { return a + b * c; }
typedef float v4 __attribute__((vector_size(16)));
v4 vector(v4 a, v4 b, v4 c) { return a + b * c; }
EOF

components="-fno-honor-infinities -fno-honor-nans -fno-math-errno -ffinite-math-only -fassociative-math -freciprocal-math -fno-signed-zeros -fno-trapping-math"
flag_sets=(
  "-O3"
  "-O3 -ffp-contract=off"
  "-O3 -ffast-math"
  "-O3 -ffast-math -ffp-contract=off"
  "-O3 $components -fapprox-func -ffp-contract=off"
  "-O3 $components -ffp-contract=off"
)

"$cxx" --version | head -1
for flags in "${flag_sets[@]}"; do
  # shellcheck disable=SC2086
  count="$("$cxx" $flags -S -o - "$work/kernel.cpp" | grep -cE '\b(fmadd|fmsub|fnmadd|fnmsub|fmla|fmls|vfmadd[0-9a-z]*)\b' || true)"
  # shellcheck disable=SC2086
  unsafe="$("$cxx" $flags -### -c "$work/kernel.cpp" 2>&1 | grep -oE '"-(menable-unsafe-fp-math|funsafe-math-optimizations|ffp-contract=[a-z]+)"' | tr '\n' ' ' || true)"
  printf '%2s fused  [%s]  cc1: %s\n' "$count" "$flags" "$unsafe"
done
