## 2026-10-22: M7 render coverage 4, tuning files in full renders

Voice Engine. Fourth feature-stacking render case (engine-completeness
audit, "full-render coverage"). Stacked on render coverage 3. Touches the
harness, so it needs Parity Reviewer as well as Graph Reviewer.

**What changed**

- `ref-render --tuning PATH` (a `.scl` or `.tun` file) and
  `--keyboard-map PATH` (a `.kbm` file): after the preset, each goes
  through `SynthBase::loadTuningFile`, as the plugin's tuning menu applies
  a file, the scale first and the keyboard map on top of it. The render's
  meta JSON records them only when given, so existing renders' metadata is
  unchanged. No upstream file changes.
- Render cases take optional `"tuning"` and `"keyboard_map"` corpus paths,
  in `run_corpus.py` (passed to `ref-render`, validated as strings) and
  `sas-parity` (`RenderCase::tuning_path`, `keyboard_map_path`), with a unit
  test on each side. Upstream picks the loader by file extension, whichever
  key names the file, so both resolvers require `tuning` to be a `.scl` or
  `.tun` file and `keyboard_map` a `.kbm` file (a keyboard map alone, on the
  default scale, stays valid). The engine render test asserts each file
  exists, as `ref-render` does, instead of loading an empty scale.
- `engine/tests/render.rs`: a case's tuning file, then its keyboard map, are
  loaded into one `Tuning` and given to the synth after the preset.
- `generate.py` writes `migration/corpus/tunings/`:
  - `corpus_septimal.scl`: an authored seven-note septimal just scale, five
    degrees as ratios and two in cents (one with trailing text), so both
    Scala pitch forms are read;
  - `corpus_septimal.kbm`: twelve keys per period with five unmapped (`x`)
    keys, the scale's middle at MIDI 62 and A4 (69) at 432 Hz.
- `render/tuned_glide__septimal_run` (preset `tuned_glide`, MIDI
  `scale_run`), with its x86 CI-identity hash and a `preset/state` case:
  two oscillators gliding (`portamento_scale` on) through a chromatic run
  over mapped and unmapped keys, then a chord; the filter (keytrack 0.6)
  and a keytracked LFO 3 follow the tuned pitch, and `note` drives the
  cutoff through a bipolar custom line map.

**Results.** Passes against the official reference; the port is
bit-identical to the `REF_FAST_MATH=OFF` reference. Rendering the preset
with no tuning, with the scale alone and with scale and keyboard map gives
three different outputs, so both files reach the audio.

**Mutation checks** (every render case run per mutation; only this case
fails):

| Mutation | Fails |
|---|---|
| keyboard map ignored | `tuned_glide__septimal_run` |
| unmapped (`x`) keys map to degree 0 instead of repeating the last degree | `tuned_glide__septimal_run` |

**Not changed.** The plugin's `sas-render` CLI has no tuning flag; its
render test covers the four init-preset cases only.

**macOS.** No aarch64 hash yet; the advisory job's `macos-hashes` patch adds
it.
