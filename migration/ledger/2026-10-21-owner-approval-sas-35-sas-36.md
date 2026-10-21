## 2026-10-21: owner approval — SAS-35 noisy-reference rule and SAS-36 JSON bound

The owner (Salil) approved both decisions on 2026-10-21. The coordinator
relayed them to the Parity Reviewer, who recorded them here.

- **SAS-35, noisy-reference rule: approved.** When the C++ reference itself
  amplifies fast-math noise, those cases are compared for exact match
  against the no-fast-math reference build (`REF_FAST_MATH=OFF`). Salil
  named the 3 pitch-tracking LFO cases, the 8 chaotic Lorenz random-LFO
  cases, the 22 set-aside oscillator cases, and any future cases of the same
  kind. That covers every case parked as SAS-35-pending across threads:
  filters, reverb, distortion and the distortion module, tuning raw values,
  comb, compressor, chorus/flanger, and so on. It includes the five
  wavetable render cases approved earlier
  ("2026-10-21: owner approval — JSON bound and no-fast-math exact cases").
  Each case must still pass `--justify-nofastmath` in CI: the official
  reference fails the case's normal limits against the no-fast-math build,
  and the port is exact against the no-fast-math build.
- **SAS-36, JSON bound: approved as implemented.** JSON state values pass
  within 4 f32 ULP or 1e-6 absolute, whichever is looser (`JSON_BOUND` in
  both resolvers). This confirms the numbers that were pending owner
  confirmation in `decisions/owner-decisions-json-and-morphs.md`.

### How the rule is applied

- The allowlist is one file per module:
  `migration/decisions/nofastmath/<area>/<module>.md`. Each line is
  ``- `<area>/<module>/<case>` (SAS-35; <targets>): <why the reference
  amplifies fast-math noise>``. Ids are exact; globs, other modules' ids,
  unknown targets and malformed list lines are rejected. A thread edits only
  its own modules' files, so parallel PRs do not conflict.
- **Cross-target qualification** (settled with ARM Reference after #2): a
  case qualifies if the official reference fails its normal limits against
  the no-fast-math build on x86 **or** on macOS arm64. `<targets>` declares
  where (`x86`, `macos`, or `x86, macos`).
  - `compare --justify-nofastmath` works out its target from the official
    tree's build identity and gates only the cases declared for it: in
    `ci.yml` on x86, and in `reference-arm.yml` on macOS (that job had no
    gate before).
  - Every listed case is compared exactly against the no-fast-math build
    on both targets.
  - The five wavetable cases are declared `x86`, where CI has always
    justified them.
- `SAS-35` is pinned in both resolvers (`NOFASTMATH_APPROVALS`) with this
  heading. A line may cite only a pinned key, and the heading must exist and
  name it. A later owner decision outside SAS-35 needs a new pinned key, a
  code change in both resolvers.
- CI (`run_corpus.py allowlist-approval`) fails an addition unless it is a
  loaded module case declaring `"reference": "nofastmath"` in the same
  change. Each target's calibration step (`compare --justify-nofastmath`)
  fails any case declared for that target whose official reference already
  meets the limits there.
- Agents do not write owner-approval entries for SAS-35 cases. This entry
  is the approval; the allowlist line cites it.

Checks: `python3 migration/scripts/test_run_corpus.py`, `cargo test -p
sas-parity`, `run_corpus.py allowlist-approval --base origin/main`.
