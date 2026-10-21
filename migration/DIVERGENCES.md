# Divergences from upstream

Every intentional difference between the Rust port's behaviour and upstream
(the vendored source in `/vital`, as its release build behaves) is listed
here. The owner asked for this file (Salil, 2026-10-21). It is kept separate
from the work log in `migration/ledger/`, so the complete list is in one
place.

Each entry gives:

- what differs;
- upstream's behaviour;
- the port's behaviour;
- why;
- the approving decision or PR.

Any PR that introduces an intentional behavioural divergence adds an entry
here, and reviewers check for it (migration/AGENTS.md).

What is **not** listed:

- **Per-target emulation that matches upstream on each target.** Examples
  are the A1–A12 sites in `decisions/reference-arm.md` (`toInt`,
  float-to-int conversion, the libc++/libstdc++ RNG and `expf` columns) and
  A10's official-build complex division. The port reproduces upstream
  there, so these are not divergences.
- **Test-harness policies.** These are listed under
  [Verification exceptions](#verification-exceptions) because they are not
  port behaviour.

## Port divergences

### D1. Host text entry reads the displayed value, clamped

- **Differs:** what a value typed into a host parameter's text field sets.
- **Upstream:** `ValueBridge::getValueForText` returns
  `unskew(text / display_multiply)`, an engine-domain value. JUCE's VST3
  wrapper hands it to the host as the *normalized* value
  (`juce_VST3_Wrapper.cpp:529`), and `convertToEngineValue` never clamps it.
  - Typing "72" into Filter 1 Cutoff sets the engine cutoff to
    72 × 128 + 8 = 9224 (about 9224).
  - An option name parses as 0, so it selects option 0.
- **Port:** the typed value is read as the displayed value: `post_offset` is
  removed, the value is divided by `display_multiply`, unskewed and clamped
  to the parameter's range, with stepped controls rounded to a step. An
  option name (case-insensitive) selects the matching option. Implemented
  in `port/crates/plugin/src/params.rs` (`value_from_text`).
- **Why:** upstream's result is an out-of-range bug. nice-plug clamps text
  input to [0, 1] anyway, so the closest reproduction would make every typed
  number above 1 select the maximum. Only the host's parameter text entry
  is affected; engine values, automation and preset state are not.
- **Approved:** owner (Salil), 2026-10-21. Implemented in #59. The decision
  is recorded in `migration/ledger/2026-10-21-owner-approval-plugin-text-entry.md`
  and `decisions/plugin-framework.md`.

### D2. Note events addressed to every channel or key are dropped

- **Differs:** note-on, note-off and poly-pressure events that address all
  channels or all keys.
- **Upstream:** they cannot occur. JUCE's VST3 wrapper always delivers
  concrete MIDI messages with a channel and a key.
- **Port:** such events are dropped (`processor.rs`, `midi_message`). Only
  CLAP can send them, and VST3 always names both.
- **Why:** a wildcard has no 3-byte MIDI equivalent for the engine's
  `MidiManager`. CLAP is exported but neither tested nor shipped.
- **Approved:** #59.

### D3. A host tempo of zero or less is ignored

- **Differs:** a block whose host transport reports a tempo ≤ 0.
- **Upstream:** `SynthPlugin::processBlock` skips `setBpm` when the tempo
  is 0 but, while playing, still computes the song position as
  `ppqPosition / (bpm / 60)`. A zero tempo therefore makes the clock
  infinite or NaN, and a negative one is passed to `setBpm`
  (`vital/src/plugin/synth_plugin.cpp:151-157`).
- **Port:** a tempo ≤ 0 is ignored. Neither the tempo nor the song position
  changes for that block (`processor.rs`:
  `transport.tempo.filter(|bpm| *bpm > 0.0)`).
- **Why:** there is no meaningful result to reproduce, and an infinite or
  negative clock would corrupt every tempo-synced module.
- **Approved:** #59.

### D4. A preset or host state that fails to load changes nothing

- **Differs:** loading a corrupted preset, or one from a newer version.
  This applies to `PresetState` loads and to host state restores.
- **Upstream:** `LoadSave` applies the steps that ran before the failure,
  so the synth is left partially loaded. For a host state, it reports the
  error and keeps going. Some malformed inputs are undefined behaviour
  upstream: more than 64 modulations, more than 3 wavetables or 8 LFOs, a
  modulation without `source` or `destination` (read through a const
  reference).
- **Port:**
  - A failed load leaves the state unchanged. `PresetState` loads into a
    scratch state first.
  - The undefined inputs are ignored or rejected as `Error::Corrupted`
    (`port/crates/preset/src/state.rs`).
  - In the plugin, `filter_state` empties a saved state whose preset can't
    load, so restoring it changes nothing (`port/crates/plugin/src/state.rs`).
- **Why:** a partial load gives a state that matches neither preset.
  Undefined behaviour has no defined output to reproduce. Authored and
  corpus presets never hit these paths: every `error_*` preset fails the
  same way in the reference and the port.
- **Approved:** #7 (preset loading,
  `migration/ledger/2026-10-20-m2-preset-registry.md`) and #59 (host state).

### D5. `create_filter(Comb)` returns `None`

- **Differs:** the generic filter factory for the comb model.
- **Upstream:** `SynthFilter::createFilter(kComb)` builds `CombFilter(1)`.
  Its memory rounds to one sample, and processing reads past the 2-float
  buffer (undefined behaviour).
- **Port:** `synth_filter::create_filter(FilterModel::Comb)` returns `None`
  instead of building a filter that could panic or diverge in `process`.
- **Why:** there is no defined output to reproduce. Nothing in the engine
  uses that path: `FilterModule` builds its comb through `CombModule`, with
  full memory.
- **Approved:** M5a follow-up (Character Filters / Filter Interface),
  recorded in `migration/LEDGER.md` ("M5a PR 1 follow-up,
  `create_filter(Comb)` returns `None`").

### D6. A keyboard map that indexes past the scale panics

*Pending: in #24 (tuning files), open.*

- **Differs:** a `.kbm` whose mapping names a scale degree beyond the loaded
  scale. An example is degrees 9–11 on a 7-degree Scala scale.
- **Upstream:** `Tuning::loadScale` reads `scale[note_in_scale]` past the end
  of the scale vector (undefined behaviour; `vital/src/common/tuning.cpp:340`).
- **Port:** the index is bounds-checked, so loading such a mapping panics
  (`tuning.rs`, `scale[note_in_scale as usize]`). The authored test inputs
  that hit it were corrected in #24. These are invalid inputs, not narrowed
  coverage.
- **Why:** there is no defined output to reproduce.
- **Approved:** #24. Open follow-up: a panic is not acceptable in a shipped
  plugin, so the loader should reject such a map with an error instead.
  Update this entry when that lands.

### D7. A line generator with `num_points` outside 1–100 is an error

- **Differs:** `LineGenerator::jsonToState` with `num_points` < 1 or > 100.
- **Upstream:** behaviour is undefined. With no points, rendering a looped
  line reads `points_[-1]`.
- **Port:** `LineGenerator::from_json` returns an error and leaves the
  generator unchanged (`port/crates/common/src/line_generator.rs`).
- **Why:** there is no defined output to reproduce.
- **Approved:** #4 (`sas-common`).

### D8. JSON numbers out of double range, NaN, infinity and subnormals

- **Differs:** parsing `1E400`, and dumping NaN, infinity or subnormal
  doubles, in the JSON library.
- **Upstream (release, fast-math):** `isfinite` is folded away. So `1E400`
  parses as infinity, NaN and infinity dump as a garbage double, and the DAZ
  flag makes subnormal doubles dump as `0.0`.
- **Port:** follows nlohmann's documented, no-fast-math behaviour. Overflow
  is a parse error, NaN and infinity dump as `null`, and subnormals print
  normally (`sas-common` JSON).
- **Why:** upstream's results are fast-math artefacts of degenerate input.
  Authored content never contains these values.
- **Approved:** #4. Recorded in `migration/LEDGER.md` ("M4a `sas-common`",
  Decisions).

### D9. Unsupported `clone()` panics

- **Differs:** `clone()` on `Upsampler`, `Decimator`, `IirHalfbandDecimator`
  and `VoiceHandler`.
- **Upstream:** `VITAL_ASSERT(false); return nullptr;`. The assert is
  compiled out of release builds, which return `nullptr`.
- **Port:** `clone` / `clone_box` panics, with a message naming upstream's
  assert.
- **Why:** nothing in the engine clones these processors. A `nullptr`
  processor has no Rust equivalent other than a panic or an `Option`, and
  an unreachable path does not justify an `Option` in every caller.
- **Approved:** #30 (oversampling: `Upsampler`, `Decimator`,
  `IirHalfbandDecimator`) and #21 (`VoiceHandler`).

## Verification exceptions

These are policies of the parity harness, not port behaviour. They change
**what the port is compared against or how**, never what the port does. Each
has its own decision record.

- **Bounded JSON state (SAS-36).** Declared JSON artifacts compare
  structurally, with floats within 4 f32 ulp or 1e-6. See
  `decisions/owner-decisions-json-and-morphs.md`.
- **No-fast-math exact comparisons (SAS-35).** Allowlisted noisy-reference
  cases compare bit for bit against the `REF_FAST_MATH=OFF` build, gated
  per target by `--justify-nofastmath`. See `decisions/nofastmath/` and
  `migration/ledger/2026-10-21-owner-approval-sas-35-sas-36.md`.
- **macOS libm drift list.** Cases where Apple libm differs between macOS 14
  and 15 are reported with their metrics instead of failing the same-binary
  check. See `migration/corpus/macos_libm_drift.json` and
  `decisions/reference-arm.md`.
- **macOS arm64 parks.** These are temporary, owner-approved skips of a case
  on the macOS reference only; x86 keeps the case live. See
  `migration/corpus/parked_arm64.json` and migration/AGENTS.md.
- **Parked cases in `pending/`.** Case files under
  `migration/corpus/modules/<area>/pending/` are not loaded until their
  decision lands. Each says why in its description.
