## 2026-10-22: automatic macOS hash backfill on main

Coordinator request, under the Mac CI policy. Three full suites on main
failed only on missing macOS hash keys from module PRs that merged without
them (#45, #48, #66, #71); hash-only PRs #70, #72 and #74 fixed them by hand.

- **Gate:** `reference-arm-main.yml`'s macOS 15 job keeps its parity and
  hash-check logs, and `migration/scripts/mac_missing_only.py` classifies a
  failure as missing keys only when all of these hold:
  - every other step passed;
  - every hash problem is a missing key or a missing identity entry (no
    MISMATCH, stale or unrendered hash);
  - every parity panic is sas-parity's missing-hash panic;
  - the run's `macos-hashes` patch only adds keys to `.hashes.json` files.
- **Backfill:** the new `backfill` job (`migration/scripts/mac_hash_backfill.sh`)
  applies the patch on the run's commit and pushes it to the bot branch
  `ci/macos-hashes-backfill`, never main. It opens or updates the hash-only
  PR and dispatches `ci.yml` on the branch, because a PR opened with the
  workflow token triggers no workflows. If Actions may not open PRs, the
  issue gets the branch's compare link instead. A human or agent merges the
  PR on Linux CI.
- **Unchanged:** a real mismatch or any other failure still fails, and the
  issue stays open; it links the backfill PR when there is one.
- **Self-check:**
  - The classifier, replayed on nightly run 39750051523's real trees, logs
    and patch, says missing-only. With an injected MISMATCH, or a parity
    comparison failure, it says real.
  - `test_mac_missing_only.py` has 9 cases.
  - The backfill script against a local bare origin with a stubbed `gh`
    covers create, update, the compare-link fallback, and a refused patch
    that touches a non-hash file. Main was untouched.
