## 2026-10-27: The live gallery's oscilloscope and Random LFO match ref-gui's engine

**Why.** In the live gallery views (`all_effects_live`, `modulations_live`,
`modulations_matrix_live`), the header oscilloscope showed ref-gui's
waveform at a different phase. The Random 1 LFO (`random_1`) also read
higher than ref-gui's at frame 30, in the Random viewer and on the Matrix
tab. I compared the two engines' telemetry at the end of the same live
render. On the port side I dumped `TelemetrySnapshot`. On the upstream side,
an uncommitted hook in ref-gui dumped `getOscilloscopeMemory()` and every
`getStatusOutput` after `renderLive`. There were two causes, one for each
symptom.

**Changed**

- `sas-engine` `synth`: the oscilloscope's `input_offset` is an `f32`, as
  upstream's `memory_input_offset_` (a `mono_float`) is. Upstream truncates
  it only to index the block (`iclamp`). When a window resets, it adds
  `memory_reset_period_ - memory_index_ * output_inc`, and that fraction
  carries into the next window's start. The port kept an `i32` and truncated
  at every reset. So each window started up to half a sample early, and the
  error built up until the next note reset the window. Middle C at 44.1 kHz,
  for example, has a period of 1348.5 samples.
  - The windowing moved into `OscilloscopeMemory::update`, which takes the
    block as a closure. `update_memory_output` calls it with the engine's
    output.
  - This changes only telemetry. The audio is unaffected.
- `examples/gallery.rs`: every live view's engine calls
  `RandomValues::reset_process_state()` before `Synth::new()`. Upstream's
  `RandomValues` is a function-local static. The first `SynthOscillator` of a
  process builds it and takes one value from the global seed counter. ref-gui
  renders one view per process, so every view's engine pays that seed. In the
  gallery, only the first engine of the process paid it. Every later engine
  seeded its random sources one value earlier, which raised `random_1`.
  - The per-view seed-counter reset in #205 doesn't cover this, because the
    static stays initialized. The two resets together start each view as a
    fresh process does. The engine's parity tests already do the same
    before each engine.

**Measured**

- Oscilloscope memory for `all_effects_live` (2 s of `chord_progression`):
  - Before the fix, the port's memory was ref-gui's shifted by 4 slots
    (8 samples). The largest difference at the same index was 0.024.
  - After it, the memory is bit-identical to ref-gui's (difference 0.0), and
    so is `modulations.vital`'s.
- Status outputs, `modulations.vital` for 1 s, both engines starting from
  seed 0: all 198 are identical. `random_1` is 0.18538436.
  - A second engine in the same process, after resetting only the seed
    counter, read 0.3866458. That is the gallery's value.
  - The gallery now reads 0.18538436 in both modulations views, with #205's
    `views.json` and seed reset applied locally.
- New tests:
  - `synth::tests::oscilloscope_windows_start_a_whole_reset_period_apart`
    feeds a ramp and checks that 90+ successive windows start a whole
    1348.5-sample period apart, within a sample. With the old integer offset,
    it fails at window 4 (4044 samples, not 4045.5).
  - `tests/fresh_process.rs` runs alone in its own binary, because it
    resets process-wide state. On `modulations_live`'s render, it checks
    three things:
    - The first engine's `random_1` is ref-gui's.
    - Resetting only the seed counter changes `random_1`.
    - A second engine after the full reset gives identical status outputs
      and oscilloscope memory.
- `cargo test -p sas-engine` passes. Clippy `-D warnings` and rustfmt are
  clean for the workspace. No parity tolerance changed.

**Merge-rule waiver (coordinator, 2026-10-27).** Every Linux check was
green at `a8c18b7`, rebased on `94a4633`. Then #204 and #199 merged, and
#204 changed `examples/gallery.rs`, so rule (b) ("no commit on `main` since
the base touches the PR's files") failed. The overlap is code-only and
merges without conflict:
- #204's hunks are in `main()`'s view loop (`REF_GUI_FRAMES`, 30 frames for
  live views).
- This PR's hunk is in `Live::render`.

For this PR only, rule (b) is waived for overlaps that are code-only and
merge without conflict, on the same terms as #120 and #158. Rule (a) holds.
Rule (c) passed on the merged tree (`722a6f6`, rebased on `c3d7611`):
- the gallery renders all 26 views
- Python unittests
- `ledger.py --check` and `--check-frozen`
- `allowlist-approval`
- `check_names.sh`
- rustfmt and clippy `-D warnings`
- `cargo test -p sas-engine` and `-p sas-parity`
- x86 render: 2845 hashes checked, 0 problems
- no-fast-math render: 359 hashes checked, 0 problems
