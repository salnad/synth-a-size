# Golden test corpus

Every preset and MIDI file here was written for this project by
[`generate.py`](generate.py), a standard-library Python script. **No
factory or third-party presets, banks, wavetables or samples are included,
and none may be added.** CI fails if the committed files differ from what
`generate.py` produces.

```bash
python3 migration/corpus/generate.py          # regenerate after editing definitions
python3 migration/corpus/generate.py --check  # what CI runs
```

## Presets (`presets/*.vital`)

Upstream preset JSON (`synth_version` 1.0.6). Controls that a preset
leaves out load as their parameter defaults; that is upstream
`LoadSave::loadControls` behaviour. A preset without `wavetables` or
`lfos` keeps the engine's init wavetable and LFO shapes. Every preset
embeds a tiny self-generated 2048-sample tone as its `sample`, because
the loader rejects presets without one.

| Preset | Exercises |
|---|---|
| `init_saw` | Engine defaults: init wavetable, one oscillator, default envelope. Baseline. |
| `analog_filter_lfo` | Analog (Sallen-Key) low-pass with drive, LFO 1 → cutoff, fixed oscillator phase |
| `unison_pad_fx` | Two oscillators with unison and stereo spread; chorus, delay and reverb |
| `line_wavetable_fold` | Authored Line Source wavetable + Wave Folder modifier; envelope 2 → wave frame; hard-clip distortion |
| `mono_legato_glide` | Mono legato with portamento, ladder filter, mod wheel → cutoff, velocity → level |

## MIDI (`midi/*.mid`)

Format 0, 480 PPQ, 120 BPM.

| File | Content |
|---|---|
| `single_note_c3` | One note, MIDI 48 (C3 in the upstream naming), velocity 100, 1 s |
| `chord_progression` | Four triads, 0.75 s apart |
| `legato_bend_modwheel` | Overlapping legato line, rising mod wheel (CC1), pitch-bend swell |
| `velocity_ramp` | Eight repeated MIDI-60 notes, velocity 16 → 121 |

## Cases

- **Renders** ([`cases.json`](cases.json) `renders`, ids `render/<name>`):
  preset + MIDI pairs with sample rate, block size, BPM, seed and tail.
  Settings not given in a case come from `defaults`.
- **Module cases** ([`modules/<area>/<module>.json`](modules/), ids
  `<area>/<module>/<case>`): single-processor specs for `ref-module`. There
  is one file per module, so parallel threads never edit a shared list.
  Settings layer as `cases.json` `module_defaults`, then the file's
  top-level keys, then each case's keys. Expected output hashes sit next to
  each file in `<module>.hashes.json`.
- `python3 migration/scripts/run_corpus.py list` prints every resolved case
  (spec and tolerances).

Each case carries the parity tolerances that `audio-diff` enforces. The
render defaults (`residual_db` −50, `lsd_db` 0.1, `rms_diff_db` 0.01, no
`max_abs_error` gate) and their margins are explained in
[`../decisions/reference-determinism.md`](../decisions/reference-determinism.md).
Module tolerances default to `module_defaults` and may only be made
stricter, unless a case cites recorded human approval in
`tolerance_exception`; `run_corpus.py` and `sas-parity` both enforce this.
[`reference_hashes.json`](reference_hashes.json) (renders) and the
per-module `*.hashes.json` files hold the expected output hashes, one entry
per build identity. CI checks it, so the
reference stays bit-identical across machines.
Render and compare everything with
[`../scripts/run_corpus.py`](../scripts/run_corpus.py):

```bash
python3 migration/scripts/run_corpus.py render --out build/ref
python3 migration/scripts/run_corpus.py compare build/ref build/port
```

Renders are not committed (`*.wav` and `*.npy` are git-ignored). CI
uploads the reference renders as the `reference-renders` artifact, and the
full capture tree as `parity-captures` for the port's parity tests
(`port/crates/parity`).

## Adding a case

Module cases: follow "How to port a piece of the engine" in
`migration/AGENTS.md`. You create the module's case file and its hashes
file and edit nothing shared.

Render cases:

1. Add the preset or MIDI definition to `generate.py`, run it, and commit
   the output.
2. Add the case to `cases.json` with tolerances at least as strict as the
   defaults. CI's `tolerance-calibration` job must still pass, which means
   the no-fast-math proxy meets them.
3. Render the corpus and refresh the hashes with
   `python3 migration/scripts/run_corpus.py hashes <tree> --write`.
4. Note the addition in `migration/LEDGER.md`.
