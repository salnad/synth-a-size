## 2026-10-21: ledger entries are one file each

Every merge to main made every other open PR conflict in
`migration/LEDGER.md`, and GitHub then would not run their CI until they
rebased. With 20+ open PRs this was the biggest source of churn.

- **New entries** go in `migration/ledger/YYYY-MM-DD-<slug>.md`, one file
  per PR or session. The format is in `migration/ledger/README.md`.
- **`migration/LEDGER.md` is frozen** as history, with a header pointing
  here. CI's repo checks fail a pull request that changes it once its
  base carries the freeze header (`ledger.py --check-frozen --base
  origin/main`). The check runs on pull requests only, so merging an older
  PR that still appends does not turn main red; move such an entry to a
  file afterwards.
- **One ledger for the resolvers:** `run_corpus.py` and sas-parity read
  `LEDGER.md` followed by the entry files in name order
  (`scripts/ledger.py read_text` / `cases::read_ledger_at`).
  `tolerance_exception.approved_in` can cite a heading in either place.
- **Allowlist approval check** (from #34): an id added to the no-fast-math
  allowlist now needs a *new file* under `migration/ledger/` whose heading
  contains "owner approval" and names the id. An approval appended to
  `LEDGER.md` no longer counts, and neither does an edit to an existing
  entry.
- **Approvals use dated headings, parsed per file** (Parity Reviewer on
  #50). Only `YYYY-MM-DD: ...` headings count, for `approved_in` and for
  the allowlist check. Sections are built per file, so none crosses a file
  boundary. Before this, `"approved_in": "Ledger"` was accepted: the
  `# Ledger` section spans the whole file, so any mention of the case id
  counted, including a worker's own progress note. The hole was already on
  main; joining entry files would have widened it. The allowlist check
  also fails closed if `git ls-tree` errors.
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
  - Both suites: the reviewer's `"approved_in": "Ledger"` repro (on test
    files and on the repository ledger), an undated heading in an entry, and
    a dated level-1 heading that must not reach into the next file are all
    rejected.
- Docs updated: AGENTS.md, migration/AGENTS.md, READMEs, PLAN.md,
  UPSTREAM.md and the decision doc.
