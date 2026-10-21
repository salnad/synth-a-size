# Ledger entries

The work log is one file per entry, so concurrent PRs never edit the same
file. The history before 2026-10-21 is in [`../LEDGER.md`](../LEDGER.md),
which is frozen.

## Adding an entry

Add a file named `YYYY-MM-DD-<slug>.md`, where the slug uses lowercase
letters, digits and `-` (for example `2026-10-22-digital-svf-parity.md`).
Use one file per PR or work session. It starts with a heading whose date
matches the file name:

```markdown
## 2026-10-22: Digital SVF parity

**Changed** ...
```

Record what changed, the metrics measured, the decisions made and anything
blocked (migration/AGENTS.md).

## Approvals

The resolvers treat `LEDGER.md` plus every entry file as one ledger.

Only **dated** headings (`YYYY-MM-DD: ...`) count as approvals, and each
file is parsed on its own, so a section never runs into the next file.

- **Tolerance exceptions:** a `tolerance_exception`'s `approved_in` must be
  the exact text of a dated heading in the ledger, and that section must
  name the case id. The heading can be in `LEDGER.md` or in an entry file.
- **No-fast-math allowlist:** an id in
  `decisions/nofastmath/<area>/<module>.md` cites an owner approval key
  pinned in both resolvers (`SAS-35`). That key's dated heading must exist
  in the ledger, and its section must name the key. Threads do not add
  owner-approval entries for these ids (see `decisions/nofastmath/README.md`).

Only a human can approve (migration/AGENTS.md, rule 6).

## Reading

```bash
python3 migration/scripts/ledger.py          # everything, in date order
python3 migration/scripts/ledger.py --list   # entry files
python3 migration/scripts/ledger.py --check  # names and headings (CI)
```
