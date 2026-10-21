## 2026-10-21: M7 tuning raw cases re-enabled under the no-fast-math reference (SAS-35)

Re-enables the two raw comparisons that M7 PR 2 parked in
`migration/corpus/modules/engine/pending/tuning.json`, following
`migration/decisions/nofastmath/README.md`.

**Changed**

- `tun_then_kbm_raw` and `scl_reference_frequency_raw` move, unchanged, into
  `migration/corpus/modules/engine/tuning.json` with
  `"reference": "nofastmath"`. They set no tolerances, so every output, raw
  included, is compared bit for bit against the `REF_FAST_MATH=OFF` build of
  each target. The pending file and `engine/pending/` are deleted.
- `migration/decisions/nofastmath/engine/tuning.md` lists both ids
  (SAS-35; x86). `allowlist-approval --base origin/main` lists both as
  added, with 0 problems.
- The active `tun_then_kbm` and `scl_reference_frequency` cases are
  unchanged. They keep `skip_raw` and the normalized outputs on the official
  reference.

**Justification (x86).** `compare --justify-nofastmath` prints
`justified on x86` for both:

| Case | Official vs no-fast-math (x86) |
|---|---|
| `tun_then_kbm_raw` | `converted` and `roundtrip` fail: 1.22e-4 at \|696.85\| semitones (2 ULP), residual −138.5 dB |
| `scl_reference_frequency_raw` | `reference_midi_note` fails: −7.63e-6 against exactly 0 (residual 0 dB) |

**Port exactness.** On x86 the port is bit-exact against the no-fast-math
captures on every output of both cases
(`SAS_PARITY_CAPTURES_NOFASTMATH=build/proxy cargo test -p sas-engine`).
The macOS arm64 result comes from this PR's `Reference ARM` macOS 15 job.

**Hashes.** x86 official and no-fast-math entries come from local
CI-identity trees. macOS official and no-fast-math entries come from this
PR's `ref-arm64-macos-15` artifact.
