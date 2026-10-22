## 2026-10-22: Mac full suite — slim macOS 14 artifact, infrastructure reporting

Coordinator request, under the Mac CI policy. Full run 39775970688 (bdb9b5d)
passed the whole macOS 15 job, strict hashes included, but the macOS 14 job
could not download the macOS 15 artifact ("failed after 5 retries"), so it
compared nothing. The report job still called it a real failure on #69.

- **Slim artifact:** the macOS 15 job also uploads `ref-arm64-macos-15-binary`
  (the pinned tree and the reference binaries). The macOS 14 job downloads
  that instead of `ref-arm64-macos-15`, which carries every variant tree.
  The full artifact is unchanged for the cross-platform report and
  `macos_hashes.sh`.
- **Reporting:** `mac_full_suite_report.py` says "Infrastructure — re-run"
  when every failed job failed only in `actions/download-artifact` or
  `actions/upload-artifact` steps. Anything else is reported as before.
- **Gate:** `mac_full_suite_gate.py --before` runs the full suite on a push
  that changes `reference-arm-main.yml` or its scripts, so CI changes are
  exercised on main at once (this merge re-runs the suite for #69).
- **Self-check:**
  - `test_mac_full_suite.py` has 8 cases.
  - Run 39775970688's real step list classifies as infrastructure.
  - The gate on this branch against main says run, because this push
    changes the full suite itself.
