## 2026-10-21: M3 PR 3, `sas-modulation` random LFO family; Lorenz cases parked

- **Ported** (`port/crates/modulation`):
  - `random_lfo.rs`: `vital::RandomLfo`, every style (perlin, sample and
    hold, sine interpolation, Lorenz attractor), mono and stereo, free
    and tempo-synced (shared state);
  - `trigger_random.rs`: `vital::TriggerRandom`;
  - `random_lfo_module.rs`: `vital::RandomLfoModule`, with registry
    controls and the tempo-sync switch.
  - Values come from `sas_common::RandomGenerator`, seeded from the
    global counter. The random kernels' `Clone` takes a new seed, as
    upstream's `RandomGenerator` copy constructor does.
- **Captures:**
  - `random_lfo.cpp`, `trigger_random.cpp` and `random_lfo_module.cpp`.
  - Every case sets the spec `seed`, so the mt19937 streams are compared
    exactly.
  - `clock_start_seconds` calls `correctToTime(start + frames / rate)`
    before each block, as the engine does, so tempo sync advances. The
    Rust tests mirror it with a test-only wrapper processor.
  - `use_clone` captures a `clone()`, which pins the extra seed draw.
  - Case counts:
    - `random_lfo.json`: 31 live cases (every style at both rates, mono
      and stereo, frequency sweep 2^-7 to 2^9 Hz, maximum frequency,
      tempo sync, style rounding and clamp, clone, blocks of 37 and 128,
      48 and 96 kHz, oversample 2);
    - `trigger_random.json`: 3 cases;
    - `random_lfo_module.json`: 14 live cases.
  - All use the `module_defaults` tolerances.
- **Metrics (x86_64 reference):**
  - `random_lfo`: 31 outputs, 12 bit-identical. Worst is 7.63e-6 at
    −126.4 dB (`block_128_48k:value`).
  - `random_lfo_module`: 14 outputs, 5 bit-identical. Worst
    max_abs_error is 3.82e-6 (`defaults:value`). Worst residual is
    −130.0 dB (`free_frequency_min:value`).
  - `trigger_random`: 3 outputs, all bit-identical.
  - The no-fast-math proxy passes all 48 live outputs.
- **Parked: Lorenz attractor at audio-relevant frequencies.** Eight cases
  are in `migration/corpus/modules/modulation/pending/random_lfo.json`
  and `pending/random_lfo_module.json`, which the harness does not load:
  - `modulation/random_lfo/lorenz_mono_audio`, `lorenz_mono_control`,
    `lorenz_stereo_audio`, `lorenz_stereo_control`,
    `lorenz_frequency_sweep` and `lorenz_max_frequency`;
  - `modulation/random_lfo_module/free_lorenz` and `tempo_extremes`
    (the Lorenz style).
  - The port is **bit-identical to the `REF_FAST_MATH=OFF` reference**
    on them, checked on `lorenz_frequency_sweep` and `free_lorenz`. The
    proxy fails all eight against the official reference.
  - The official reference's first ulp difference is at frame 877 and
    333 respectively. The attractor is chaotic, so it grows to full scale
    (residual about −6 dB).
  - This is the same pattern as the high-note keytrack cases in the LFO
    entry above. The coordinator has escalated it to the owner as one
    rule: exact comparison against no-fast-math, allowed only where CI
    shows the official reference fails the limits against its own
    no-fast-math build. These cases qualify. No phase-blind metrics, and
    no tolerance change.
  - The live band: Lorenz at 2^-7 Hz (`lorenz_low_frequency_*`,
    `free_lorenz_low_frequency`) is exact or within 6e-8, and the
    tempo-synced `lorenz_tempo_sync` is within 1.8e-7.
- **macOS arm64 (after #2):**
  - Hashes for `random_lfo`, `random_lfo_module`, `trigger_random` come from this PR's `Reference ARM` run 39622663846
    (`ref-arm64-macos-15`).
  - All four M3 macOS artifacts agree with the committed hashes.
  - The macOS no-fast-math proxy passes all 48 of these outputs.
  - The port built with `aarch64-semantics` (A1 truncating `toInt`, the
    A9 libc++ RNG column) passes every case against the macOS captures.
  - The macOS job runs the same parity tests natively.
