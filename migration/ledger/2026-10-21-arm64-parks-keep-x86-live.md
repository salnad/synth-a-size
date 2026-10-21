## 2026-10-21: macOS-only parks keep the case live on x86

Before this change, only sas-parity honoured `migration/corpus/parked_arm64.json`
(`SAS_PARITY_PARKED`). The macOS `run_corpus.py compare` and hash steps in
`reference-arm.yml` ignored it, so a case noisy only on macOS still failed
there. Threads therefore moved such cases to `pending/` and removed their
x86 hashes, dropping x86 coverage too.

- **`run_corpus.py --parked FILE`** on `compare` and `hashes`:
  - parked cases are skipped and listed as `parked <id> (...): <reason>`;
  - in `hashes`, their committed hashes are kept, neither required nor
    reported as stale, and `--write` leaves them untouched.
  - The option works with every compare mode, including
    `--reference-nofastmath` and `--justify-nofastmath`.
- **Approved, strict, never for `macos` cases** (Parity Reviewer on #56).
  Parks are now resolved identically by `run_corpus.py` and sas-parity
  (`resolve_parked`); sas-parity uses it in `parked_case_ids`.
  - Each entry is `{"approved_in": "<dated ledger heading>", "reason":
    "..."}`. A park switches off every macOS check for the case on the
    shipping target, so like a `tolerance_exception` it needs a dated
    heading (via `approval_sections`) whose section names the case
    (`names_case`).
  - Unknown keys, empty or non-string fields, an unknown top-level key, a
    non-object `cases` and a non-string `description` are rejected.
  - A parked id must be a loaded case.
  - A case whose no-fast-math allowlist targets include `macos` cannot be
    parked: the macOS proxy compare is its only justification, so it
    stays live.
  - Rust's allowlist reader now returns the declared targets
    (`read_allowlist_entries`).
- **Parks are temporary:** they cover a port bug or a macOS-only fix that
  is still pending. Noise in the macOS reference itself belongs under
  SAS-35 `macos` (AGENTS.md and the park file's description say so).
- **`reference-arm.yml`:** the macOS proxy compare (which runs
  `--justify-nofastmath` for cases declared `macos` since #51) and both
  macOS hash checks (official and no-fast-math) pass
  `--parked migration/corpus/parked_arm64.json`. No other job does, so x86
  keeps parked cases live, hashed and gated.
- Documented in migration/AGENTS.md (macOS-only parks) and in the park
  file's description.
- No existing park was moved. The owning threads' SAS-35 re-enable PRs
  bring those cases back from `pending/`.
- **Tests:**
  - Python: compare skips and reports a parked case that would fail, and
    rejects unknown ids and a malformed file; hashes skips a parked case
    whose capture changed and keeps its committed hashes on `--write`.
  - Both suites: an approved park resolves. These are rejected:
    - a missing heading, a heading that does not name the case, and an
      undated heading (including `Ledger`);
    - every malformed entry and file shape, and an unknown id;
    - a macos-declared case (an x86-only one may be parked).
  - Both suites: the repository park file resolves.
