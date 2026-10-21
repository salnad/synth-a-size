## 2026-10-21: Mac CI policy (owner approval)

The owner (Salil) approved a new policy for the Apple Silicon CI. Over
about 21 hours, 31 of 33 macOS-only failures were missing hashes for new
cases. Only 2 were real, both on bring-up branches, and the cost was mostly
waiting for hosted macOS runners. There is no self-hosted Apple Silicon.

- **macOS does not gate merges.** Merges go on Linux CI (`ci.yml`, with the
  per-module audio parity gate) plus review.
- **Per PR:** `.github/workflows/reference-arm.yml` runs macOS 15 only and is
  advisory (never a required check).
  - It builds the reference, runs the FP guard and the repeat-render
    identity check.
  - It runs the arm64 parity tests for the touched crates plus their
    dependents, or the whole workspace when a shared input changes
    (`migration/scripts/mac_pr_scope.py`).
  - It checks the macOS hashes. A changed output fails; a missing or stale
    hash is a warning (`run_corpus.py hashes --check --missing-ok`,
    `SAS_PARITY_MISSING_HASHES=warn`).
  - It runs the macOS no-fast-math justification whenever it builds the
    no-fast-math tree. That is always the case when the PR adds or edits a
    `macos` allowlist line, so a change that un-justifies a case is caught on
    the PR. The result goes to the step summary for reviewers to cite
    (`migration/scripts/justify_nofastmath_summary.sh`; Parity Reviewer's
    review items).
- **Auto-written macOS hashes:** the PR job writes the missing entries
  (`hashes --write --only-new`: new keys only, never overwriting or
  pruning, so drift is not masked) as the `macos-hashes` patch artifact.
  `migration/scripts/macos_hashes.sh <run-id>` applies it, or writes from the
  run's capture trees. Nothing pushes automatically (no pushes to main by
  CI; contributor branches stay theirs).
- **Main:** `.github/workflows/reference-arm-main.yml` runs the full suite.
  - It runs nightly, on dispatch, and after every 6 merges
    (`migration/scripts/mac_full_suite_gate.py`).
  - It covers macOS 15 with the vDSP, contraction and no-fast-math variants,
    the macOS justification gate with `--parked`, whole-workspace arm64
    parity, and strict `hashes --check --require-known-identity` for pinned
    and no-fast-math. It also runs the macOS 14 same-binary drift check, the
    x86 baseline and the cross-platform report.
  - On a failure, the issue "Mac full suite failing on main" is opened or
    updated (`migration/scripts/mac_full_suite_report.py`), and it closes
    when main is green again.
- **Tests:** `test_run_corpus.py` covers both flags. `--missing-ok` warns on
  a missing hash but still fails on a mismatch. `--only-new` adds the missing
  key without re-pinning a deliberately wrong one, which then still fails.
  `sas-parity` has a unit test for the `SAS_PARITY_MISSING_HASHES` values.
- **Recorded in:** `decisions/reference-arm.md` ("Mac CI policy"),
  `migration/AGENTS.md`, and a pointer in `migration/DIVERGENCES.md`
  ("Verification exceptions").
