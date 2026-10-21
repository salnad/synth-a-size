# No-fast-math exact cases (allowlist)

A module case may declare `"reference": "nofastmath"` only if its id is
listed here. Such a case is compared bit for bit against the
`REF_FAST_MATH=OFF` reference build instead of against the official
reference with tolerances. The rule and its mechanism are described in
[`../owner-decisions-json-and-morphs.md`](../owner-decisions-json-and-morphs.md).
The owner's approval is SAS-35, recorded in
`migration/ledger/2026-10-21-owner-approval-sas-35-sas-36.md`.

## Files

There is one file per module, mirroring the corpus:
`<area>/<module>.md` lists cases of
`migration/corpus/modules/<area>/<module>.json` only. Each thread edits only
its own modules' files, so parallel PRs do not conflict.

Every line that starts with a list marker must be exactly:

```markdown
- `<area>/<module>/<case>` (SAS-35): <why the reference amplifies fast-math noise>
```

Ids are exact, with no globs. Any other text is prose. Both resolvers
(`run_corpus.py` and sas-parity) reject the following:

- a malformed or indented list line;
- an id from another module;
- an id listed twice;
- a file not named `<area>/<module>.md`;
- an approval key not pinned in `NOFASTMATH_APPROVALS`;
- a pinned approval whose ledger heading is missing.

Only the owner adds approval keys. SAS-35 covers any case where the C++
reference itself amplifies fast-math noise.

## Re-enabling a parked case

1. **Move the case** from `migration/corpus/modules/<area>/pending/<module>.json`
   into `migration/corpus/modules/<area>/<module>.json`, unchanged, and add
   `"reference": "nofastmath"` to it. The case must not set `tolerances`,
   `tolerance_exception` or `json_bound`, and file-level `tolerances` do not
   apply to it. Delete the pending file once it is empty.
2. **List it** in `migration/decisions/nofastmath/<area>/<module>.md`
   (create the file if needed) with a one-line reason.
3. **Build both references and render:**

   ```bash
   migration/scripts/build_reference.sh
   REF_BUILD_DIR=build/ref-no-fast-math migration/scripts/build_reference.sh -DREF_FAST_MATH=OFF
   python3 migration/scripts/run_corpus.py render --out build/parity
   python3 migration/scripts/run_corpus.py render --out build/proxy \
     --ref-render build/ref-no-fast-math/ref-render --ref-module build/ref-no-fast-math/ref-module
   ```

4. **Justify it.** Every listed case must print `reference "nofastmath"
   justified`. If one says `NOT justified`, the official reference already
   meets the normal limits: keep that case on the official reference and
   fix the port instead.

   ```bash
   python3 migration/scripts/run_corpus.py compare build/parity build/proxy --justify-nofastmath
   ```

5. **Pin hashes** for both builds:

   ```bash
   python3 migration/scripts/run_corpus.py hashes build/parity --write
   python3 migration/scripts/run_corpus.py hashes build/proxy --write --reference nofastmath
   python3 migration/scripts/run_corpus.py hashes build/parity --check --require-known-identity
   python3 migration/scripts/run_corpus.py hashes build/proxy --check --require-known-identity --reference nofastmath
   ```

6. **Make the port test exact.** Keep `assert_parity!`/`assert_artifact!`
   for the case; sas-parity compares a `nofastmath` case bit for bit
   against the no-fast-math captures. Run it with
   `SAS_PARITY_CAPTURES=build/parity SAS_PARITY_CAPTURES_NOFASTMATH=build/proxy
   cargo test --manifest-path port/Cargo.toml -p <crate>`. A port that is
   not bit-exact is a port bug; do not add tolerances.
7. **Check the allowlist change:**
   `python3 migration/scripts/run_corpus.py allowlist-approval --base origin/main`
   must list each id as `added <id> (SAS-35)` with 0 problems.
8. **Record it** in a new `migration/ledger/YYYY-MM-DD-<slug>.md`, not in
   `LEDGER.md`. Give each id with the official-vs-no-fast-math numbers that
   justify it and the port's exactness. Do not write an "owner approval"
   heading: SAS-35 is the approval.

Removing an id needs no approval: delete its line and drop
`"reference": "nofastmath"` from the case, which returns it to the official
reference.
