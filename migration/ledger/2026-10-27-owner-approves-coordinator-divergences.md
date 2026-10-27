## 2026-10-27: Salil approves the divergences the coordinator had approved

**Decision.** Salil reviewed the divergences the coordinator had approved
on its own authority. In his words: "Keep all of them as they are". No
behaviour changes.

**Changed** (docs only)

- `migration/DIVERGENCES.md`: these entries' approval line now reads
  "Approved: Salil, 2026-10-27". The earlier approval and its review
  history follow it in brackets.
  - D25, D27, D28, D29, D32, D33, D34, D35, D36, D37.
  - Each entry was checked first. All are complete, with what differs,
    upstream's behaviour, the port's behaviour, the reason and an approval.
- `migration/decisions/reference-arm.md`: A13, the folded `FileSource` seed
  on Apple targets, now reads route (a) approved by Salil, 2026-10-27, after
  the coordinator's decision earlier that day.

**Scope.** This approves only the entries listed above. It isn't a standing
rule: a divergence the coordinator approves in future still needs its own
owner approval.

Left unchanged:
- D26, D30, D31 and every other entry.
- The separate one-seed justification exception in #212, which already
  carries Salil's approval.

**Checks.** `ledger.py --check` and `--check-frozen`, and `check_names.sh`,
pass.
