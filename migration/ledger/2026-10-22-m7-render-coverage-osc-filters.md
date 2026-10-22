## 2026-10-22: M7 render coverage 1, oscillators and filters

Voice Engine. First of the feature-stacking render cases for the audit's
"never in a full render" list (Parity Reviewer's engine-completeness audit,
"full-render coverage"). Follows #78.

**What changed**

- `generate.py`: tables of the enumerated control values the new presets
  use (`DISTORTION`, `MORPH`, `FILTER_MODEL`, `DESTINATION`, `LFO_SYNC`), an
  `oscillator()` helper, three authored presets and the MIDI
  `wide_arpeggio` (twelve overlapping notes from C1 to C6, varied
  velocities).
- Render cases, each with its x86 CI-identity hash, and a
  `modules/preset/state.json` load/save case per preset:
  - `osc_stack_serial__arpeggio`: oscillator 1 sync + vocode (3 unison
    voices), oscillator 2 sample FM + inharmonic scale, oscillator 3 ring
    mod by oscillator A + harmonic scale, the sample oscillator (keytracked)
    into filter 2; the dirty filter feeds the digital filter (24 dB band) in
    series (`filter_2_filter_input`). Envelope 2, LFO 1 and velocity
    modulate morph, cutoff and distortion amounts.
  - `osc_formant_comb__chords`: formant + smear, quantize + random
    amplitudes, bend + low-pass oscillators; the formant filter feeds the
    comb filter in series the other way (`filter_1_filter_input`); envelope
    3 moves the formant X; 8x oversampling.
  - `osc_diode_phase__overlap`: squeeze + high-pass (into both filters),
    pulse width + phase disperse, FM by oscillator B + Shepard tone; the
    diode and phaser filters in parallel; envelopes 1 and 3 shaped; `note`
    → cutoff; 1x oversampling.
- `engine/tests/render.rs`: the three cases.

**Coverage.** Oscillator 3, the sample oscillator, 9 of the 12 oscillator
distortions and 9 of the 11 spectral morphs (the rest, plus FM by oscillator
A, come with the next PRs), filter 2, both serial directions and parallel
dual filters, the dirty, digital, diode, formant, comb and phaser filter
models, envelopes 1 and 3 beyond the defaults, 1x and 8x oversampling.

**Results.** All three pass against the official reference. The port is
bit-identical to the `REF_FAST_MATH=OFF` reference on all three (and on
every earlier render case), through `Synth` and the plugin's `sas-render`.

**Content tuned to the reference's own noise.** The first draft of
`osc_diode_phase` failed the official reference against its own no-fast-math
build (residual -45.7 dB against the -50 dB limit). Removing one feature at a
time showed FM of oscillator 3 by oscillator B as the amplifier (it turns the
modulator's fast-math phase noise into output); at FM amount 0.1 the
reference meets its limits (-56.8 dB). `osc_formant_comb` peaked at 1.35;
`voice_amplitude` 0.6 keeps it at 0.37, away from the output clamp.

**Mutation checks** (each run over every render case; only the named case
fails, and no earlier case catches any of them):

| Mutation | Fails |
|---|---|
| `FiltersModule` serial forward runs parallel | `osc_stack_serial__arpeggio` |
| `FiltersModule` serial backward runs parallel | `osc_formant_comb__chords` |
| `checkOversampling` fixed at 2x | `osc_formant_comb__chords`, `osc_diode_phase__overlap` |
| preset sample not loaded | `osc_stack_serial__arpeggio` |

**macOS.** The three cases have no aarch64 hashes yet; the advisory job's
`macos-hashes` patch adds them.
