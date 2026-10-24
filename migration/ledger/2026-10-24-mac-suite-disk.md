## 2026-10-24: Mac full suite — out of disk on the parity build

The 2026-10-24 nightly (run 40077324174 at d7d5586, issue #113) failed in
"arm64 parity tests on the whole workspace": linking `sas-plugin`'s debug
test binaries hit `ld: write() failed, errno=28` (ENOSPC). Nothing compared
differently: both strict hash checks passed (2775 pinned and 359
no-fast-math hashes, 0 problems), and so did the justification and the
variant comparisons. The NEON parity tests did not get to run. The LLM
crates merged since the last green suite (f9587d6) grew the workspace's
debug build past the runner's free disk.

- **Free disk:** both Mac workflows delete the preinstalled Android SDK
  first, and print the free disk before and after.
- **Smaller test build:** both Mac jobs set
  `CARGO_PROFILE_DEV_DEBUG=line-tables-only` (job-level, so rust-cache keys
  on it). Debug-info level does not change generated code; backtraces keep
  file and line. On Linux, `sas-simd`'s test build drops from 471 MB to
  305 MB.
- **Diagnosis:** the main job logs free disk and the sizes of `port/target`
  and `build/platform` after the parity build.
