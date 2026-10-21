# Decision: plugin framework = nice-plug (the maintained nih-plug fork)

Status: accepted (M0, 2026-10-20). Revisit at M8 if the VST3 build fails in
FL Studio on macOS.

## Options

| | **nice-plug** (BillyDM fork) | **nih-plug** (robbert-vdh, upstream) |
|---|---|---|
| Where | [codeberg.org/RustAudio/nice-plug](https://codeberg.org/RustAudio/nice-plug), mirrored at [github.com/BillyDM/nih-plug](https://github.com/BillyDM/nih-plug) | [github.com/robbert-vdh/nih-plug](https://github.com/robbert-vdh/nih-plug) |
| Status | Active community project under the RustAudio org. Releases 0.4.0 to 0.4.2 in 2026; last push 2026-10-18 (baseview 0.4.1, egui 0.37). | Upstream README now says the framework is "in maintenance mode" and links to the community fork. Last push 2026-09-03, mostly CI and house-keeping commits. 123 open issues. |
| Distribution | Published on crates.io (`nice-plug = "0.4.2"`), semver releases | Git dependency only, not on crates.io |
| License | ISC | ISC |
| VST3 bindings | `vst3` crate (MIT/Apache-2.0) | `vst3-sys` (GPLv3) |
| Formats | VST3, CLAP, standalone | VST3, CLAP, standalone |
| MSRV | Rust 1.88 (stable) | Stable, some features need nightly |
| GUI | egui 0.37, vizia and wgpu examples, baseview 0.4 | egui / vizia / iced adapters on older versions |

## Decision

Use **nice-plug 0.4.2** from crates.io, pinned with `=0.4.2` in
`port/Cargo.toml`, with `default-features = false, features = ["vst3"]`
for now. CLAP export is always available and is also enabled.

Reasons:

1. It is the maintained line. Upstream's own author points framework users
   to it, and it already tracks current egui and baseview, which M10 needs.
2. Versioned crates.io releases give reproducible, auditable builds; a git
   dependency on a repo in maintenance mode does not.
3. Same ISC license and a near-identical API (`nice_plug::prelude`,
   `nice_export_vst3!`), so nih-plug examples and knowledge still apply.
4. Both are GPLv3-compatible, so licensing does not decide this.

## Verified in M0

- `sas-plugin` builds as a `cdylib` on Linux with no system GUI
  libraries (editor feature off) and exports `GetPluginFactory`,
  `ModuleEntry` (VST3) and `clap_entry` (CLAP).

## Open items for M8

- Bundle the macOS `.vst3` (nice-plug's bundler/xtask or a small script),
  sign ad hoc, and load it in FL Studio on macOS (arm64 and x86_64).
  *Status (M8 PR 1, `migration/ledger/2026-10-21-m8-plugin-wrapper.md`):*
  bundled by `port/crates/plugin/scripts/bundle.sh`, arm64 only (owner
  decision: Apple Silicon is the only shipping target), signed ad hoc and
  validated with Steinberg's VST3 validator in `.github/workflows/plugin.yml`.
  The FL Studio load still needs the owner.
- *Host text entry (owner decision, 2026-10-21, #59):* a typed value is read
  as the displayed value, clamped to the parameter's range, and option names
  select their option. Upstream's out-of-range typing (an engine value handed
  to the host as a normalized value) is not replicated. Recorded in
  `migration/ledger/2026-10-21-owner-approval-plugin-text-entry.md`.
- Turn the `editor` feature on when the GUI crate lands (M10).
- The RustAudio community has an AI-usage policy
  (<https://rust.audio/community/ai/>). It covers contributions *to* their
  repositories, not use as a dependency. Agents must not open issues or
  PRs upstream without a human.
