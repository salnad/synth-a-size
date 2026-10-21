## 2026-10-21: A10 complex division follows the official build (owner decision)

The owner decided that `std::complex<float>` division, at the phase
modifier's `1.0f / phase_shift` sites (`phase_modifier.cpp:53, 72`), uses
the official build's plain float textbook formula on every target. It
replaces the no-fast-math `__divsc3` columns from #52.

- **Why:** both official references are `-ffast-math`, so they use
  limited-range complex arithmetic and inline
  `((ac + bd) / (c² + d²), (bc − ad) / (c² + d²))` in float. That means
  no scaling, no FMAs and no `__divsc3` call; ARM Reference's probe found
  them equal on both targets. The no-fast-math forms differed between the
  targets (libgcc in double, compiler-rt fused).
  - The shipping plugin follows the official build.
  - No exact case could exercise the `__divsc3` columns, because a
    `reference: nofastmath` render case may not contain a phase modifier
    (A11 guard).
- **Port:**
  - `Complex / Complex` is that formula, with the same operation order
    and no `mul_add`.
  - `div_libgcc`, `div_compiler_rt` and their `logb`/`scalbnf` helpers
    are removed, and with them the per-target A10 split.
  - `AARCH64_SEMANTICS` is still used by A9 and A12. CI's
    `aarch64-semantics` step for `sas-wavetable` is renamed, since it no
    longer covers division.
- **Checks:**
  - GCC 13 with the reference flags (`-ffast-math -ffp-contract=off
    -U__FAST_MATH__`) compiles complex division inline, with no
    `__divsc3`. It equals the plain formula on 2M random inputs.
  - The unit test pins 8 quotients, including 4 where the `__divsc3`
    forms differ.
  - Against the official x86 reference, the phase-modifier render cases
    are at most 4.8e-7 in `wave` and 8.9e-8 in the coefficient arrays.
    Reference captures and hashes are unchanged.
