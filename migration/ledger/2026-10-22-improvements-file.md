## 2026-10-22: IMPROVEMENTS.md, parked ideas beyond 1:1 parity

The owner (Salil) asked for a list of potential future improvements beyond
1:1 parity, kept separate from DIVERGENCES.md and the decision records.
`migration/IMPROVEMENTS.md` states at the top that these are parked ideas,
not work: the priority stays close to 1:1. Each entry gives the idea, a
one-line why, the tradeoff (including any parity cost) and its source.

Entries, all taken from existing records (no filler):

- **I1 per-sample host automation.** From #79 / the m8-plugin-sound ledger.
  The default stays matching upstream; the owner approved this in
  `2026-10-22-owner-approval-automation-timing.md`, also noted in
  `decisions/plugin-framework.md`.
- **I2 FMA contraction and vDSP for speed (M11).** From DIVERGENCES D10,
  reference-arm.md and PLAN M11.
- **I3 gentler handling of upstream-undefined input.** From DIVERGENCES
  D4–D7 and D11, and the kbm-validation entry.
- **I4 bypass set from the edit controller while processing (nice-plug).**
  From the validator log in the m8-plugin-sound ledger.
- **I5 shipping and testing CLAP.** From plugin-framework.md, the
  m8-plugin-wrapper ledger and D2.
- **I6 one platform-independent, higher-precision libm.** From
  reference-arm.md, the macOS libm drift list and the frozen LEDGER's
  `libm`-crate note.
- **I7 preparing heavy preset loads off the audio thread.** From the
  m8-plugin-wrapper follow-ups.
- **I8 effects-only plugin.** The owner (Salil) kept upstream's
  `src/synthesis/effects_engine/` out of scope as a parked idea. It has no
  official reference build: the Makefile's `effects/` project folder is
  missing from the open-source release.

migration/AGENTS.md (rule 5) now says ideas beyond parity go in
IMPROVEMENTS.md, not in code. migration/README.md lists the file.
