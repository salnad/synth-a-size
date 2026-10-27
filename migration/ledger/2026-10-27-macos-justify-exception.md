## 2026-10-27: macOS justification exception for the A13 seed (owner approval)

Approved: Salil, 2026-10-27. His words: "Go with (i): add just that seed to
the exception list for that check." Relayed by the coordinator.

- **Scope:** the macOS no-fast-math justification step only
  (`justify_nofastmath_summary.sh`), and exactly one value:
  `wavetable/edit/add_sources`, `artifact/saved.json`,
  `$.groups[2].components[0].random_seed`.
- **Why:** only Apple clang's official fast-math build folds `FileSource`'s
  seed (A13), so macOS official (209652480) and no-fast-math (209652560)
  differ by construction, and JSON integers are exact.
- **How:** `migration/corpus/macos_justify_exceptions.json`, read by
  `run_corpus.py compare --justify-exceptions`.
  - The flag is refused without `--justify-nofastmath` or on a non-macOS
    official tree.
  - An entry needs a loaded regular case, a declared json artifact and a
    `$` path.
  - It matches only an integer value change at that exact path. A missing
    key, a type change or any other path still fails.
  - The difference is printed with both values and not gated. An entry
    that no longer differs is reported as unused.
- **Unchanged:** the port's parity stays exact against the official seed
  (#211). The rest of `add_sources` and every other case stay under the
  check. Recorded in DIVERGENCES.md, "Verification exceptions", with no
  precedent for A11 or anything else.
- **Tests (`test_run_corpus.py`):**
  - the excepted path passes, and the same trees fail without the list;
  - a different value at another path in that file fails;
  - another case's difference at the same path fails;
  - a string, float or missing key at the path fails;
  - x86 trees and use without `--justify-nofastmath` are refused;
  - the list format is strict;
  - the committed list holds exactly this entry.
