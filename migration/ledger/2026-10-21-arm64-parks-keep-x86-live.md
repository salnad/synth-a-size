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
- **Strict:** the file must be `{"description": ..., "cases": {<id>:
  <reason>}}`, and every parked id must be a loaded case, in
  `run_corpus.py` (`load_parked`) and sas-parity (`all_case_ids` via
  `check_parked`).
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
  - Rust: `check_parked` rejects ids that are not loaded cases.
