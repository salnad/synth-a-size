## 2026-10-22: owner approval — host automation timing matches upstream

The owner (Salil) decided on 2026-10-22, relayed by the coordinator, to
resolve the open automation-timing question by matching upstream.

- **Host automation is applied at block rate, as upstream does it.**
  Upstream's JUCE VST3 wrapper applies only the last point of each
  parameter queue, once, before `processBlock` (`processParameterChanges`),
  and the engine smooths from there.
  - The port keeps nice-plug's `SAMPLE_ACCURATE_AUTOMATION = false`, as
    merged in #79. nice-plug then applies every queued point before
    `process`, leaving the last value, and never splits the block.
  - Test: `automation_applies_once_per_block_like_upstream`
    (`port/crates/plugin/src/lib.rs`).
- **Per-sample automation is not adopted.** It would track fast automation
  more tightly, but it splits blocks at every point, changing the engine's
  chunking and the values inside the block. Heavily automated projects
  would then no longer render bit-exactly like upstream. It is parked as an
  idea in `migration/IMPROVEMENTS.md` (I1).
- Also recorded in `migration/decisions/plugin-framework.md` (open items for
  M8). This is not a divergence: it matches upstream.
