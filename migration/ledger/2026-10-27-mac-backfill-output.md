## 2026-10-27: Mac full suite — backfill job output, report wording, nightly cancelled

The first automatic backfills ran overnight (runs 40438960207 at dbc72df and
40448236989 at e63a2f0: missing `audio_file/decode` keys from #154). The job
opened #182 directly with `gh pr create`, without needing the compare-link
fallback, and dispatched `ci.yml` on the branch. But the job still failed,
for three reasons:

- **Backfill output:** `mac_hash_backfill.sh`'s stdout goes to
  `$GITHUB_OUTPUT`. `git checkout -B` printed `M <file>` and
  `gh workflow run` printed the run URL there too, so GitHub rejected the
  file ("Invalid format"). The script now sends everything but the final
  `pr=` line to stderr. A regression test runs the script against a local
  origin with a `gh` stub that prints URLs like the real one does.
- **Report wording:** when a run was missing keys only but the backfill
  job reported no PR, the comment on #183 said "Not a missing-keys-only
  failure … real failure". It now says it was missing keys only, names the
  backfill job's result, and gives the `macos_hashes.sh` command.
- **Cancelled nightly:** the 2026-10-27 schedule run (40478644482) was
  cancelled while queued, because a newer push run joined the same
  concurrency group. Push runs now have their own group.
