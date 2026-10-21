## 2026-10-21: DIVERGENCES.md, the list of intentional differences from upstream

Owner instruction (Salil, 2026-10-21): record every intentional behavioural
difference between the Rust port and upstream in a dedicated file,
`migration/DIVERGENCES.md`, kept separate from the decisions ledger.

- **New file.** Each entry gives what differs, upstream's behaviour, the
  port's behaviour, why, and the approving decision or PR. It starts with
  the owner-approved host text entry from #59 (D1).
- **Survey.** I searched `migration/LEDGER.md`, `migration/ledger/`,
  `migration/decisions/`, the port sources (comments naming upstream
  undefined behaviour, asserts or departures) and the merged and open PRs.
  Entries D2–D9:
  - #59's other departures: wildcard note events, tempo ≤ 0, and a failed
    host state restore. The last is merged with #7's failed-load and
    corrupted-input handling (D4).
  - `create_filter(Comb)` (D5).
  - #24's kbm overrun (D6). #24 merged while this PR was in review; the
    port panics there, and the entry flags rejecting such a map as a
    follow-up.
  - Line generator `num_points` (D7).
  - Degenerate JSON numbers in `sas-common` (D8).
  - Unsupported `clone()` (D9).
- **Corrected while checking upstream:** for a zero host tempo, upstream
  already skips `setBpm`, but while playing it still divides the song
  position by it. A negative tempo passes straight through.
- **Not listed as divergences:**
  - per-target emulation that matches upstream on each target (A1–A12,
    A10's official-build division);
  - behaviours the port mirrors (VST3 note expressions dropped as in JUCE,
    the `setOversampleAmount` no-op, sample padding on save).
- **Parity Reviewer changes on #60:**
  - D8 is now the whole class "source formulas instead of the release
    build's fast-math codegen": `tanh` order, saw /2047, `signbit`,
    `abs` via `hypotf`, the preset remap and wave data, and the degenerate
    JSON numbers. It is rounding-level against the official build, and A10
    is the exception since #54.
  - New D10: no FMA contraction, no estimates and JUCE's fallback FFT,
    where upstream's real Mac build fuses and uses vDSP (−91 / −127 dB on
    the shipping target; `decisions/reference-arm.md`).
  - D1–D3 and D4's host-state half are marked pending (#59 is open), and
    D1 says its approval ledger file arrives with #59.
  - Rule 5 says what reviewers look for.
- **Verification exceptions.** These are harness policies, not port
  behaviour, and sit in their own section:
  - the pinned reference build;
  - tolerance floors and `tolerance_exception`;
  - normalized-form captures;
  - SAS-36 bounded JSON;
  - SAS-35 no-fast-math comparisons, with the A11 no-phase-modifier
    constraint;
  - the macOS libm drift list;
  - arm64 parks;
  - `pending/` case files.
- **Rule** in migration/AGENTS.md, under hard rule 5: any PR that
  introduces an intentional behavioural divergence adds a DIVERGENCES.md
  entry, and reviewers check for it. migration/README.md lists the file.
