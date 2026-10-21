## 2026-10-20: M2 parameter registry and preset load/save (`sas-preset`)

Built on `sas-common` (PR #4: nlohmann-exact JSON, JUCE base64, RNG,
`LineGenerator`, version comparison and PCM helpers) and the
`sas-wavetable` state model (PR #6), both merged.

**Changed**

- `port/crates/preset`: the parameter registry (`synth_parameters.cpp`:
  every list, group expansion, default overrides, upstream index order),
  the display string tables, engine facts (which parameters are controls,
  which accept modulation, the modulation sources), the sample model, the
  whole `updateFromOldVersion` chain, and `PresetState` with
  load/save/init and by-name access (`lookup`, `set_value`,
  `add_modulation`, `remove_modulation`, `to_json_string`). It depends
  only on `sas-common` and the DSP-free wavetable state model in
  `sas-wavetable`.
- Reference captures as exact-match artifacts (the `sas-parity` mechanism
  from PR #3): `migration/reference/src/modules/preset/preset.cpp`
  registers the artifact-only `ref-module` modules `preset_registry`
  (registry, string tables, engine tables, init sample seed) and
  `preset_state` (the state after `loadFromFile`/`loadInitPreset` on a
  fresh synth, `load_error.txt` when the load fails, and `roundtrip.json`
  after a second load→save). The cases are in
  `migration/corpus/modules/preset/{registry,state}.json`, one per
  corpus preset, preset test and the init preset, with their hashes in
  `*.hashes.json`. CI renders them in the reference job; the tests compare
  with `assert_artifact` (hash-verified; saved states under the JSON
  bound, the registry and load errors exact). Nothing else is committed.
- `generate.py` writes 15 authored presets to
  `migration/corpus/preset_tests/`: six old-version presets that together
  run every upgrade step, edge cases for controls, metadata, LFO shapes
  and modulation slots, and five presets the loader must reject.

**Measured**

- The artifacts are byte-identical to the goldens of the earlier
  `ref-preset` tool, which they replace (28 files, 1.7 MB no longer
  committed).
- Registry: 794 parameters. Every name, index, range, default, scale,
  display field and string table matches. The engine has 772 controls (22
  legacy parameters are never instantiated or saved), 32 modulation
  sources and 428 modulation destinations.
- `saveToJson().dump()` after loading matches the reference for the init
  preset, the 5 corpus presets and the 10 loading test presets: bit for
  bit against the no-fast-math reference, and within the JSON bound
  against the default reference (table under Findings). All 5 rejected
  presets fail in both with the same error class.
- Second load→save round trip: matches the reference's for init and all
  corpus presets. Everything except the sample is a fixed point after the
  first save.

**Findings / decisions**

- Upstream's `Sample::stateToJson` encodes the padded buffer from its
  start, so every save delays the sample by 4 frames and drops its last 4,
  and requantizes it (int16 → float → truncate). The port does the same,
  so the sample is not idempotent across round trips, upstream included.
- The reference build's `-ffast-math` changes six upgrade-chain
  expressions. The owner decided that JSON state parity is ULP-bounded
  (within 4 f32 ulp or 1e-6 absolute, whichever is looser; keys, types and
  strings exact), so the port uses the formulas as written in the source.
  The table records what the x86 reference computes instead, in case a
  future comparison needs it (the functions named were deleted):

  | Port function | Upstream expression (`vital/src/common/load_save.cpp`) | What GCC emits in the x86 reference |
  |---|---|---|
  | `pow_two_thirds` | `std::pow(settings[x], 1.0f / 1.5f)`, 0.2.1 envelope times (327-329) | `cbrtf(x)` squared |
  | `pow_three_quarters` | `std::pow(settings[x], 3.0f / 4.0f)`, 0.2.5 envelope times (358-360) | `s = sqrtf(x); s * sqrtf(s)` |
  | `adjusted_level` | `sqrtf(adjustment * level * level)`, 0.2.7 levels (382-390) | `sqrtf((level * level) * adjustment)` (reassociated) |
  | `pow_half` | `std::pow(x, 1.0f / 2.0f)`, 0.4.7 FM amounts and targets (606, 627-628, 652, 673-674) | `sqrtss` |
  | `fm_remap_points` | `t = i / (kRemapResolution - 1.0f)`, `interpolate(min, max, t)`, `y = 1.0f - (pow(old, 1/2) - min_target) / new_amount`, 0.4.7 remap (636-641, 682-687) | `t = i * (1/31)`, `1/new_amount` hoisted out of the loop, `y = (min_target - sqrtf(old)) * (1/new_amount) + 1` |
  | `sub_wave_frame` | `sub_waveform * 257.0f / 6.0f`, 0.5.0 step (713) | `sub_waveform * (257.0f * (1.0f / 6.0f))` (folded reciprocal) |

  The `preset/state` cases declare `state.json` and `roundtrip.json` as
  `"json"` artifacts with `"base64_f32_keys": ["wave_data"]` (PR #34);
  `load_error.txt` and the registry stay exact. Measured state.json, port
  vs reference (wave_data compared as decoded f32):

  | Case | vs default (fast-math) reference | vs no-fast-math reference |
  |---|---|---|
  | `init`, the 5 corpus presets and 7 test presets | ≤ 12 wave_data samples, ≤ 4 ulp, ≤ 1.2e-7 abs | identical |
  | `upgrade_from_0_1_0` | 15 values: max 52 ulp, 7.7e-7 abs; wave_data ≤ 4 ulp | identical |
  | `upgrade_from_0_4_6_fm` | 18 values: max 96 ulp, 3.6e-7 abs; wave_data ≤ 4 ulp | identical |
  | `upgrade_from_0_4_9_sub` | 1 value: 1 ulp (3.8e-6 abs); 372 wave_data samples ≤ 3 ulp, 1.8e-7 abs | identical |
  | `upgrade_from_0_4_9_sub_direct` | 1 value: 1 ulp (1.5e-5 abs); 380 wave_data samples ≤ 4 ulp, 1.8e-7 abs | identical |

  Every difference is within the bound. The wave_data ones come from
  sas-wavetable's default saw and predefined waves, which now also use the
  source formulas. The 52/96 ulp values are remap-curve points near zero,
  within 1e-6 absolute (1.3x margin). Bit-identical results against the
  no-fast-math reference confirm the differences are fast-math codegen
  only. The proxy calibration (`compare build/ref build/proxy`) passes all
  708 comparisons.
- nlohmann 3.1.2 implicit numeric conversions (`float x = j`) accept
  booleans. `sas-common` now does too (PR #4), and the preset crate uses
  its conversions directly. No preset field is enum-typed (every read in
  `load_save.cpp` and `Sample::jsonToState` goes into an `int`, `float`,
  `std::string`, `bool` or `json`), so `Value::to_enum` has no use here.
- Upstream's 0.4.7 FM remap skips its slot counter on zero amounts
  (`continue` before `index++`) and reads `modulation_N_bipolar` by
  conversion (a missing key fails the load). Both are reproduced and
  covered by `upgrade_from_0_4_6_fm`.
- `SynthBase::isInvalidConnection` never fires in this engine (no slot owns
  a destination). The port has no such rule, and a test checks the
  reference still reports none.
- A failed load leaves the port's state unchanged. Upstream keeps the
  steps that ran before the failure. Undefined behaviour upstream (more
  than 64 modulations, more than 3 wavetables or 8 LFOs, a modulation
  without `source`/`destination`) becomes ignore-or-error in the port.

**Open / blocked**

- None. The macOS arm64 hash entries for `preset/*` are in (from #7's
  macos-15 artifact, Reference ARM run 39622672382, Apple clang 17): the
  registry hash equals x86_64's; `state.json` differs from x86_64 in 4 of
  27 hashes (`controls_and_metadata`, `upgrade_from_0_1_0`,
  `upgrade_from_0_4_9_sub`, `upgrade_from_0_4_9_sub_direct`), and the five
  `sas-preset` reference tests pass against the macOS captures under the
  owner's JSON bound.
