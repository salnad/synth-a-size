# No-fast-math exact cases (allowlist)

A module case may declare `"reference": "nofastmath"` only if its id is
listed here. Such a case is compared bit for bit against the
`REF_FAST_MATH=OFF` reference build of each target (x86 Linux and macOS
arm64), instead of against the official reference with tolerances. The rule
and its mechanism are described in
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
- `<area>/<module>/<case>` (SAS-35; <targets>): <why the reference amplifies fast-math noise>
```

`<targets>` lists the targets whose official reference fails the case's
normal limits against that target's no-fast-math build: `x86`, `macos`, or
`x86, macos`. A case qualifies if it fails on at least one target.
`compare --justify-nofastmath` proves a declaration on its own target.
- **x86:** `ci.yml` runs it, and it gates the merge.
- **macOS:** macOS CI is advisory (Mac CI policy, `decisions/reference-arm.md`).
  The `Reference ARM (PR)` job runs the justification and writes its result to
  the step summary. A `macos` declaration needs that run, at the PR head, to
  show `justified on macos`; the reviewer cites it before merge. The nightly
  full suite on main (`reference-arm-main.yml`) re-checks every `macos`
  declaration.

A case declared for one target only is still compared exactly against the
no-fast-math build on both.

Ids are exact, with no globs. Any other text is prose. Both resolvers
(`run_corpus.py` and sas-parity) reject the following:

- a malformed or indented list line;
- a missing, unknown or repeated target;
- an id from another module;
- an id listed twice;
- a file not named `<area>/<module>.md`;
- an approval key not pinned in `NOFASTMATH_APPROVALS`;
- a pinned approval whose ledger heading is missing.

Only the owner adds approval keys. SAS-35 covers any case where the C++
reference itself amplifies fast-math noise.

A no-fast-math case must not render a wavetable that contains a Phase Shift
(phase modifier) component. On macOS that modifier's sin/cos pair is merged
in the official build but separate in the no-fast-math build, and the port
follows the official build (A11 in `../reference-arm.md`).

## Re-enabling a parked case

1. **Move the case** from `migration/corpus/modules/<area>/pending/<module>.json`
   into `migration/corpus/modules/<area>/<module>.json`, unchanged, and add
   `"reference": "nofastmath"` to it. The case must not set `tolerances`,
   `tolerance_exception` or `json_bound`, and file-level `tolerances` do not
   apply to it. Delete the pending file once it is empty.
2. **List it** in `migration/decisions/nofastmath/<area>/<module>.md`
   (create the file if needed). Declare the targets where the official
   reference fails (step 4), and give a one-line reason.
3. **Build both x86 references and render:**

   ```bash
   migration/scripts/build_reference.sh
   REF_BUILD_DIR=build/ref-no-fast-math migration/scripts/build_reference.sh -DREF_FAST_MATH=OFF
   python3 migration/scripts/run_corpus.py render --out build/parity
   python3 migration/scripts/run_corpus.py render --out build/proxy \
     --ref-render build/ref-no-fast-math/ref-render --ref-module build/ref-no-fast-math/ref-module
   ```

4. **Justify it on x86:**

   ```bash
   python3 migration/scripts/run_corpus.py compare build/parity build/proxy --justify-nofastmath
   ```

   - A case declared `x86` must print `justified on x86`. If it prints
     `NOT justified on x86`, the x86 official reference meets the limits:
     drop `x86` from its targets.
   - A case declared `macos` only prints `not gated on x86`. Its evidence is
     the `Reference ARM (PR)` job on your PR. That job is advisory, but for a
     `macos` declaration its step summary must show `justified on macos` at
     your PR head, and the reviewer checks it before merge. If the job didn't
     run the justification, dispatch it (`workflow_dispatch`) at the PR head.
   - If neither target fails, the case stays on the official reference, and
     a mismatch is a port bug.

5. **Pin hashes** for both builds.
   - **x86 (required):** write them from your local trees, or from CI's
     `reference-renders` and `parity-captures-nofastmath` artifacts.
   - **macOS (expected, not gating):** the `Reference ARM (PR)` job writes
     the missing entries, new keys only, as its `macos-hashes` artifact.
     Commit them with `macos_hashes.sh`. A missing macOS hash only warns
     on a PR, but the nightly full suite on main treats it as an error.

   ```bash
   python3 migration/scripts/run_corpus.py hashes build/parity --write
   python3 migration/scripts/run_corpus.py hashes build/proxy --write --reference nofastmath
   migration/scripts/macos_hashes.sh <Reference ARM (PR) run id>
   ```

6. **Make the port test exact.** Keep `assert_parity!`/`assert_artifact!`
   for the case; sas-parity compares a `nofastmath` case bit for bit
   against the no-fast-math captures, on x86 locally and in `ci.yml` (gating),
   and on macOS arm64 in the advisory `Reference ARM (PR)` job and the nightly
   full suite. Run it locally with
   `SAS_PARITY_CAPTURES=build/parity SAS_PARITY_CAPTURES_NOFASTMATH=build/proxy
   cargo test --manifest-path port/Cargo.toml -p <crate>`. A port that is
   not bit-exact on either target is a port bug; do not add tolerances.
7. **Check the allowlist change:**
   `python3 migration/scripts/run_corpus.py allowlist-approval --base origin/main`
   must list each id as `added <id> (SAS-35; <targets>)` with 0 problems.
8. **Record it** in a new `migration/ledger/YYYY-MM-DD-<slug>.md`, not in
   `LEDGER.md`. For each id, give the declared targets with the
   official-vs-no-fast-math numbers that justify them, and the port's
   exactness on both targets. Do not write an "owner approval" heading:
   SAS-35 is the approval.

Removing an id needs no approval: delete its line and drop
`"reference": "nofastmath"` from the case, which returns it to the official
reference.
