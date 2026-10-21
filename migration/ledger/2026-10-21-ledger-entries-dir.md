## 2026-10-21: ledger entries are one file each

Every merge to main made every other open PR conflict in
`migration/LEDGER.md`, and GitHub then would not run their CI until they
rebased. With 20+ open PRs this was the biggest source of churn.

- **New entries** go in `migration/ledger/YYYY-MM-DD-<slug>.md`, one file
  per PR or session. The format is in `migration/ledger/README.md`.
- **`migration/LEDGER.md` is frozen** as history, with a header pointing
  here. CI's repo checks fail a pull request that changes it: `ledger.py
  --check-frozen` compares it with a pinned SHA-256. The check runs on
  pull requests only, so merging an older PR does not turn main red. If
  such a merge does append to it, move that entry to a file and re-pin
  `FROZEN_SHA256`.
- **One ledger for the resolvers:** `run_corpus.py` and sas-parity read
  `LEDGER.md` followed by the entry files in name order
  (`scripts/ledger.py read_text` / `cases::read_ledger_at`).
  `tolerance_exception.approved_in` can cite a heading in either place.
- **Allowlist approval check** (from #34): an id added to the no-fast-math
  allowlist now needs a *new file* under `migration/ledger/` whose heading
  contains "owner approval" and names the id. An approval appended to
  `LEDGER.md` no longer counts, and neither does an edit to an existing
  entry.
- **Reading and checks:** `python3 migration/scripts/ledger.py` prints
  everything in date order, `--list` lists the entry files, and `--check`
  validates names and headings (run in CI).
- **Tests:**
  - Python: entry ordering, name and heading checks, a `tolerance_exception`
    approved from an entry file, and `allowlist-approval` against a scratch
    git repo (no entry, an append to `LEDGER.md`, a new entry, an edit to an
    old entry).
  - Rust: ledger assembly order, and an exception resolved from an entry
    file.
- Docs updated: AGENTS.md, migration/AGENTS.md, READMEs, PLAN.md,
  UPSTREAM.md and the decision doc.
