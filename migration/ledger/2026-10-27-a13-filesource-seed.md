## 2026-10-27: A13, Apple clang folds FileSource's seed

Owner decision (route (a)), relayed by the coordinator: the port produces
the official macOS build's `FileSource` seed on Apple targets, following
the A11 precedent. Recorded as A13 in `migration/decisions/reference-arm.md`
with the disassembly evidence:

- **What Apple clang does:** with `-ffast-math`, Apple clang 17 folds
  `random_generator_.next() * (INT_MAX / kPi)` over
  `uniform_real_distribution(-π, π)` to `(int)((float)u - 2^31)`.
- **Who differs:** x86 GCC and the macOS no-fast-math build evaluate it as
  written.
- **Implementation:** Wavetable Builder implements the port side with a
  test; Parity Reviewer reviews.
- **Open:** the macOS justification step compares official against
  no-fast-math, and JSON integers are exact. So the step fails on
  `wavetable/edit/add_sources:artifact/saved.json` whatever the port does.
  How that step treats a fast-math-only A-entry is for the owner.
