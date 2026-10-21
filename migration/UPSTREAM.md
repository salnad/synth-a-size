# Upstream source

`/vital/` is a copy of the open-source upstream synthesizer, used as the
reference implementation for the port. It is a plain copy, not a submodule,
so the reference build never depends on network access.

| | |
|---|---|
| Repository | https://github.com/mtytel/vital |
| Commit | `636ca0ef517a4db087a6a08a6a8a5e704e21f836` (also in [`UPSTREAM_SHA`](UPSTREAM_SHA), read by the build and scripts) |
| Commit date | 2022-04-20 ("Update README.md"), head of `main` when vendored |
| License | GPLv3 (`vital/LICENSE`, unchanged). JUCE 6.0.5 is vendored in `vital/third_party/JUCE` under its GPLv3 option. |
| Vendored | 2026-10-20 by `git archive <sha> \| tar -x` |
| Plugin version at this commit | 1.0.6 (`ProjectInfo::versionString` of the plugin and standalone targets) |

## Exclusions

Every vendored file is byte-identical to upstream. These paths are left
out, and nothing else is:

| Path | Why |
|---|---|
| `third_party/firebase_cpp_sdk/libs/` | Prebuilt Firebase static libraries (`.a` / `.lib`) for login/auth. The repository stores no large binaries, the port drops Firebase/auth entirely, and the reference harness builds with `NO_AUTH=1`, so nothing links them. |
| `third_party/firebase_cpp_sdk/frameworks/` | Prebuilt macOS Firebase frameworks, excluded for the same reason. |

Together they are 318 files and about 137 MB. The Firebase headers
(`third_party/firebase_cpp_sdk/include/`) are source and are kept.

## Verifying

```bash
migration/scripts/verify_upstream.sh            # fetches upstream at the pinned SHA and diffs
migration/scripts/verify_upstream.sh ~/src/vital  # or use an existing local clone
```

CI runs this on every push. Nested upstream `.gitignore` files would hide
some upstream files from `git add`, so the copy was added with
`git add -f vital`. Use `-f` again after any re-vendor.

## Rules

- Never edit anything under `/vital/`. Build glue, configs and harness
  sources belong under `/migration/reference/`.
- To move to a newer upstream commit, re-vendor the whole tree, update
  `UPSTREAM_SHA` and this file, re-run the corpus, and record the change
  and its audio impact in a new `migration/ledger/` entry. Do this in its own PR.
