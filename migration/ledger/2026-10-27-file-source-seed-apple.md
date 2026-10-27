## 2026-10-27: A File Source's default seed on Apple targets (A13)

Main's macOS full suite failed on
`wavetable/edit/add_sources:artifact/saved.json`: the macOS official
reference saves a File Source `random_seed` of 209652480, where the
no-fast-math build saves 209652560. x86 agrees with the no-fast-math build.

**Cause** (ARM Reference's disassembly, A13 in
`decisions/reference-arm.md`): `FileSource::FileSource()` sets
`random_seed_ = random_generator_.next() * (INT_MAX / kPi)` with a
`RandomGenerator(-kPi, kPi)`. Apple clang's fast-math folds the expression
to `(int)((float)u + -2^31f)`, where `u` is the raw mt19937 draw (`ucvtf`;
`movi #0xcf, lsl 24`; `fadd`; `fcvtzs`). The no-fast-math build and x86 GCC
round each step (`canonical * 2π - π`, then `* (2^31 / π)`), which can land
a few units away in the seed's low bits.

**Decision** (coordinator, route (a), as for A11): mirror the compiler, as
the port does for Apple's merged `sincosf`. The case stays on the official
reference.

**Port** (`sas-wavetable` `creator/component.rs`, `sas-common` `random.rs`)

- `default_random_seed(draw)`: on `target_vendor = "apple"`,
  `folded_random_seed` (`(draw as f32 + -2147483648.0) as i32`, saturating
  as `fcvtzs` does); elsewhere `generic_random_seed`, the expression as
  before, from the same draw.
- `RandomGenerator::next_draw`: one raw mt19937 draw, which `FileSource::new`
  now takes in place of `next()`. It consumes the generator exactly as
  `next()` did, so the generator's later draws (the vocode phases) are
  unchanged.

**Tests**

- `creator::tests::file_source_seeds_follow_their_targets_compiler`: for
  draw 2357136128, behind the failing case's seed, the generic path gives
  209652560 and the folded path 209652480. Each target's
  `default_random_seed` takes its own path. The fold saturates at both
  ends. For 1,000 draws, the generic path equals the old
  `next() * (INT_MAX / kPi)` bit for bit, so x86 and every other
  non-Apple target are unchanged.
