## 2026-10-21: no-fast-math re-enable steps under the Mac CI policy

Follow-up to #63 (Mac CI policy), as noted in its review.
`migration/decisions/nofastmath/README.md` still described the old
required macOS job.

- **`macos` declarations:** the evidence is the advisory
  `Reference ARM (PR, advisory)` job's `justified on macos` result at the PR head. It
  is in the job's step summary, and the reviewer cites it before merge. If
  the job didn't run the justification, the coordinator dispatches it at the PR head (push rights needed). The
  nightly full suite on main re-checks every declaration.
- **x86 declarations:** `ci.yml` still gates them.
- **macOS hashes:** expected but not gating on a PR. Commit the job's
  `macos-hashes` patch with `migration/scripts/macos_hashes.sh <run id>`. On
  main, the nightly suite treats a missing macOS hash as an error.
- **Docs:** the same wording is in the cross-target paragraph of
  `decisions/owner-decisions-json-and-morphs.md`.

Docs only. No code, case or hash changes.
