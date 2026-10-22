## 2026-10-22: M7 render coverage 2, effect chain, modulation and tempo

Voice Engine. Second set of feature-stacking render cases (engine-completeness
audit, "full-render coverage"). Stacked on render coverage 1.

**What changed**

- `generate.py`: `preset()` takes per-slot modulation extras (the slot's
  `bipolar`, `power` and `stereo` controls and its `line_mapping`) and LFO
  shapes (`lfos`); `effect_order()` encodes an effect order as
  `effect_chain_order` (`encodeOrderToFloat`); tables for the effects,
  random LFO styles and LFO sync types; two authored presets and the MIDI
  `held_pad` (two held chords and a long high note).
- Render cases, with x86 CI-identity hashes and `preset/state` cases:
  - `fx_chain_reordered__chords_140bpm`: every effect on, in a custom order
    (EQ, filter FX, distortion, multiband compressor, flanger, phaser,
    chorus, delay, reverb). The EQ uses its high-pass and low-pass modes,
    the filter FX the digital notch style, the compressor all three bands.
    Flanger, phaser, chorus and delay are tempo-synced (tempo, dotted,
    triplet, tempo), and a tempo-synced LFO 1 moves the phaser centre.
    Rendered at 140 BPM with 4x oversampling.
  - `random_macro_shapes__pad_93bpm`: the four random LFO styles (Perlin
    free, sample-and-hold tempo-synced, sine-interpolated stereo, Lorenz);
    two authored LFO shapes (LFO 1 tempo-synced with powers, LFO 2 smooth,
    dotted, sync mode, stereo); LFO 3 keytracked; macros 1–3; bipolar,
    power, stereo and custom line-mapped connections. Oscillators: ring mod
    by the sample + skew, ring mod by oscillator B + form scale, vocode, and
    the sample oscillator direct out. Rendered at 93 BPM.

**Coverage.** Filter FX, compressor (multiband), EQ, flanger, phaser and a
non-default effect order; non-120 BPM tempo sync of LFOs, random LFOs and
the effects; 4x oversampling; random LFOs, custom LFO shapes, macros,
remapped modulation (bipolar, power, stereo, custom line maps); the last
three oscillator distortions (ring mod by sample and by oscillator B) and
spectral morphs (skew, form scale).

**Results.** Both pass against the official reference, and the port is
bit-identical to the `REF_FAST_MATH=OFF` reference on both.

**Content tuned to the reference's own noise.** The first draft of
`random_macro_shapes` gave oscillator 3 FM by oscillator A; the official
reference then missed its own no-fast-math build by far (residual -32.9 dB,
max error 0.12). Removing one feature at a time put it all on that FM: the
modulator is oscillator 1, heavily modulated, and FM turns its fast-math
phase noise into output (still -39.7 dB at FM amount 0.1). Oscillator 3 has
no distortion here; FM by oscillator A moves to the MPE case, where its
modulator is plain and the reference meets its limits (-58.4 dB). The
random LFOs, including the Lorenz attractor, did not amplify the noise.

**Mutation checks** (every render case run per mutation; only the named
cases fail):

| Mutation | Fails |
|---|---|
| effect order ignored (`decodeFloatToOrder` of 0) | `fx_chain_reordered__chords_140bpm` |
| tempo fixed at 120 BPM | `fx_chain_reordered__chords_140bpm`, `random_macro_shapes__pad_93bpm` |
| `checkOversampling` fixed at 2x | `fx_chain_reordered__chords_140bpm` (and the two 1x/8x cases of PR 1) |
| LFO shapes not loaded | `random_macro_shapes__pad_93bpm` |
| Lorenz random LFO runs as sample-and-hold | `random_macro_shapes__pad_93bpm` |
| modulation line maps not loaded | `random_macro_shapes__pad_93bpm` |
| preset sample not loaded | `random_macro_shapes__pad_93bpm` (and `osc_stack_serial__arpeggio`) |

**macOS.** No aarch64 hashes yet; the advisory job's `macos-hashes` patch
adds them.
