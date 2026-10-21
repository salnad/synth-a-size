## 2026-10-21: M7 PR 2, `sas-engine` tuning files (Scala, keyboard map, AnaMark)

Follows M7 PR 1 (#21, merged). Moved here from `LEDGER.md`, which is frozen
(migration/ledger/README.md).

**What changed**

- `port/crates/engine/src/tuning.rs` now ports all of `common/tuning.cpp`:
  - Scala `.scl` loading (`loadScalaFile`, cents and ratio degrees) and
    keyboard maps `.kbm` (header, `x` entries, reference note and
    frequency);
  - AnaMark `.tun` loading (`[Tuning]` / `[Exact Tuning]`, `basefreq`,
    `note N`);
  - `noteToMidiKey`, and `stateToJson` / `jsonToState` (serde_json).
- `juce_text` reproduces the JUCE string semantics the parser depends on:
  - `File::readLines` (a trailing empty line), `String::trim` and
    `addTokens`, which keeps empty tokens, so `note  5` reads note 0 as
    upstream does;
  - `getIntValue` (no `+`, unsigned wrap);
  - `getDoubleValue`: the ≤ 18-significant-digit buffer passed to `strtod`;
  - the `size_t` comparisons, so a negative scale length or map size never
    stops the read.
- The voice handler's `scala_tuning` case (tuned `note`/`last_note`
  triggers) is added back here, now that Scala loading exists.

**Reference capture**

- `migration/reference/src/modules/engine/tuning.cpp`, artifact-only. It
  writes `params.files` (authored text) to a temporary directory, loads each
  file with `Tuning::loadFile`, and emits:
  - `convertMidiNote(0..127)` raw (`converted`) and as SynthVoiceHandler's
    `note` source, `note * (1.0f / (kMidiSize - 1))`
    (`converted_note_percent`);
  - the same two after a `stateToJson` → `jsonToState` round trip;
  - `scale_start_midi_note`, `reference_midi_note` and the stored scale;
  - `getName()` as an exact-match text artifact.
- 13 cases in `migration/corpus/modules/engine/tuning.json`: default; Scala
  in cents, ratios, 19-EDO and a stretched period with blank and annotated
  lines; Scala + kbm; kbm alone; a short kbm; `.tun` with comments, odd
  spacing, an out-of-range note and a stray section; `.tun` + kbm; reference
  ratio with a start note; reference frequency; and `tun_octave_then_kbm`, a
  one-octave `.tun` + kbm whose values stay within ±256 semitones, so the
  raw `converted` and `roundtrip` are captured for the `.tun` + `.kbm`
  combination (large-scale rule: a raw case over the band that passes).

**Metrics** (x86_64; the committed hashes equal those written from CI run
39522994531's artifact, except `tun_octave_then_kbm`, which is new and gets
its hashes from this PR's next CI run). Every comparison passes under
`module_defaults`: 88 numeric comparisons (13 cases, `state` and `scale`
included), 43 bit-identical, and 13 exact `name.txt` artifacts:

macOS arm64 identity: `engine/tuning` (all 13 cases) and the voice handler's
`scala_tuning` entries were written from Reference ARM run 39622565746's
`ref-arm64-macos-15` artifact (`build/platform/pinned`). No case here uses
`reference: nofastmath`, so no proxy entries are needed.

| Output | Worst max abs error | Worst residual |
|---|---|---|
| `*_note_percent` | 9.5e-7 (`tun_then_kbm`) | −138.4 dB |
| raw `converted` / `roundtrip` (captured cases) | 1.5e-5 | −138.4 dB |
| raw `reference_midi_note` (captured cases) | 6.0e-8 | −141.1 dB |

The voice handler with `scala_tuning` (21 cases) stays bit-identical.

**Raw comparisons parked, pending the owner's noisy-reference decision
(SAS-35).** Two raw comparisons fail only because the fast-math x86
reference is noisy against its own `REF_FAST_MATH=OFF` build; the port
matches the no-fast-math build on both (Graph Reviewer verified with
`skip_raw` removed). No tolerance changed:

- `tun_then_kbm`, raw `converted` and `roundtrip`: the `.tun` period is
  ~127 semitones and the kbm repeats it, so values reach ±697 semitones.
  The official reference differs from no-fast-math by 1.22e-4 at |696.85|
  (2 ULP), against the 1e-4 gate; the residual is −138.5 dB.
- `scl_reference_frequency`, raw `reference_midi_note`:
  `setReferenceNoteFrequency(69, 440 Hz)` cancels to about 0. The source
  formula `12 * logf(f / k) * kInvLogOf2` gives exactly 69.0 in f32, and the
  no-fast-math reference and the port give 0. The fast-math reference
  reassociates it to `logf(...) * (12 * kInvLogOf2)` and gives −7.63e-6, so
  the residual against a near-zero reference is 0 dB. The port keeps the
  source formula.

They sit, inputs unchanged and with every raw output captured, as
`tun_then_kbm_raw` and `scl_reference_frequency_raw` in
`migration/corpus/modules/engine/pending/tuning.json`. The loaders read only
`modules/<area>/*.json`, so they are not rendered or tested until they move
back. The active cases keep `skip_raw` and the normalized outputs:

- `tun_then_kbm`'s `converted_note_percent` passes at 9.5e-7. That gate
  resolves about 0.0127 semitone on this path (1e-4 × 127): +0.02 semitone
  on the large values fails it, +0.01 passes (Graph Reviewer's mutations).
  `tun_octave_then_kbm` now covers `.tun` + kbm at the raw gate's resolution.
- `scl_reference_frequency`'s reference note is carried by `converted`,
  where it is added to every note: +2e-4 on `reference_midi_note` fails
  `converted`.

- Two authored inputs were fixed, because upstream would either read past
  the end of the scale vector (a kbm mapping degrees 9–11 onto a 7-degree
  scale) or produce −∞ (a blank Scala ratio line reads as `logf(0)`). These
  are invalid inputs, not narrowed coverage. The blank line is still
  exercised, as the description line. The −∞ itself is upstream's
  deterministic behaviour (`ratioToMidiTranspose(0)` is `logf(0)`), which
  the parity metrics cannot gate. A port unit test pins it
  (`tuning::tests::blank_scala_degree_is_negative_infinity`).
- `tun_octave_then_kbm`: max abs error 7.6e-6 on raw `converted` and
  `roundtrip`, residual −155.9 dB; `reference_midi_note` bit-identical.
