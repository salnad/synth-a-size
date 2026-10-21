## 2026-10-21: M3 PR 4, `sas-modulation` modulation matrix (ModulationConnectionProcessor, LineMap)

- **Ported** (`port/crates/modulation`):
  - `modulation_connection_processor.rs`:
    `vital::ModulationConnectionProcessor`. It covers the control-rate
    path and the four audio-rate paths (linear, morphed, remapped,
    remapped + morphed), bypass, bipolar, stereo, destination scale and
    its change reset. The remap line and the destination scale are shared
    with clones.
  - `line_map.rs`: `vital::LineMap` (polynomial interpolation).
- **Mono vs poly destinations:** `polyphonic_` is only a flag here. The
  engine (`sound_engine.cpp`) picks the destination and drops the
  processor to audio rate when both ends are audio-rate. The captures
  drive both rates directly; the routing is M7's.
- **Captures:**
  - New `modulation_connection.cpp` and `line_map.cpp`. `line_shape.h`
    now also applies a shape to an existing generator (the remap line).
    The LFO capture hashes are unchanged by that.
  - `modulation_connection.json`: 32 cases. They cover:
    - every path at both rates;
    - bipolar and stereo;
    - power ±10, and power crossing zero (so the path switches mid-run);
    - remap and smooth remap;
    - bypass, slot index 5;
    - out-of-range source and amount;
    - destination scales 0.25, 48 and 128;
    - per-voice reset smoothing;
    - a control-rate source into an audio-rate processor;
    - blocks of 1, 37 and 128; 48 and 96 kHz.
  - `line_map.json`: 6 cases (five shapes over phase −0.25..1.25, plus
    stereo noise at block 1).
  - All use the `module_defaults` tolerances.
- **Metrics (x86_64 reference):**
  - `modulation_connection`: 96 outputs, 68 bit-identical. Worst is
    3.8e-5 at −123.3 dB, on `power_min_audio` (power −10, bipolar). The
    no-fast-math proxy shows exactly the same numbers there, so it is the
    reference's own fast-math noise.
  - `line_map`: 12 outputs, 6 bit-identical. Worst is 1.8e-7 at
    −141.5 dB.
  - The no-fast-math proxy passes all 108 outputs.
- **M3 status after this PR:**
  - All of `modulators/*` and the envelope, LFO, random LFO and
    modulation-connection modules are ported.
  - The macro, mod wheel, pitch wheel, velocity, aftertouch and note
    sources are created in `synth_voice_handler.cpp` (M7).
  - Parked under the owner's pending no-fast-math rule: three high-note
    keytrack cases (LFO family) and eight Lorenz cases (random LFO
    family).
- Graph Reviewer's non-blocking note on #33, addressed here so #33 stays
  as approved: `RandomLfo`'s `int random_type_int = std::round(...)` (A3)
  goes through `conversions::float_to_i32`. It is in range (0..3), so the
  captures are unchanged.
- **Graph Reviewer's changes requested on #37 at f066544, addressed:**
  - **The destination-scale reset is exercised.** Before, it could never
    matter: the scale was set once before the first block, and the amount
    starts at 0.
    - `modulation_connection.cpp` gained `destination_scale_changes:
      [{block, scale}]`, applied before the given block. The Rust test
      mirrors it with the harness's per-block hook. The old `Clocked`
      wrapper is now a thin layer over that hook.
    - Four cases were added: `destination_scale_changes_linear_{audio,control}`,
      `_morphed_audio` and `_remapped_audio`, with scale 1 → 3 → 0.5 → 48
      over blocks 60, 150 and 151.
    - A reset that never fires now fails parity, on
      `destination_scale_changes_linear_audio:modulation_output`.
    - `modulation_connection` is now 36 cases and 108 outputs. Worst is
      still 3.8e-5 (`power_min_audio`). The no-fast-math proxy passes all
      108.
- **Open / next (for Voice Engine, M7):** mono vs poly modulation routing
  is not covered here. `sound_engine.cpp` connects each modulation to
  `poly_destination` or the mono destination, sets `polyphonic_`, and
  drops the processor and its source to audio rate when both ends are
  audio-rate. The engine thread must port that and test it in full-patch
  renders.
- **macOS arm64 (after #2):**
  - Hashes for `modulation_connection`, `line_map` come from this PR's `Reference ARM` run 39622667208
    (`ref-arm64-macos-15`).
  - All four M3 macOS artifacts agree with the committed hashes.
  - The macOS no-fast-math proxy passes all 120 of these outputs.
  - The port built with `aarch64-semantics` (A1 truncating `toInt`, the
    A9 libc++ RNG column) passes every case against the macOS captures.
  - The macOS job runs the same parity tests natively.
