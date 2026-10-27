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

*Merged in #59.*

- **Differs:** what a value typed into a host parameter's text field sets.
- **Upstream:** `ValueBridge::getValueForText` returns
  `unskew(text / display_multiply)`, an engine-domain value. JUCE's VST3
  wrapper hands it to the host as the *normalized* value
  (`juce_VST3_Wrapper.cpp:529`), and `convertToEngineValue` never clamps it.
  - Typing "72" into Filter 1 Cutoff sets the engine cutoff to
    72 × 128 + 8 = 9224.
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
- **Approved:** owner (Salil), 2026-10-21, relayed by the coordinator.
  Recorded in `migration/ledger/2026-10-21-owner-approval-plugin-text-entry.md`
  and `decisions/plugin-framework.md` ("Host text entry"). Implemented in
  #59.

### D2. Note events addressed to every channel or key are dropped

*Merged in #59.*

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

*Merged in #59.*

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

*Merged: preset loading in #7, host state restore in #59, its tuning in #65.*

- **Differs:** loading a corrupted preset, or one from a newer version.
  This applies to `PresetState` loads and to host state restores, including
  a host state whose `"tuning"` is not an object or names a keyboard mapping
  degree past its scale (D6).
- **Upstream:** `LoadSave` applies the steps that ran before the failure,
  so the synth is left partially loaded. For a host state, it reports the
  error and keeps going; a bad `"tuning"` throws in `Tuning::jsonToState`
  after the preset has loaded, so the preset stays. A host state from a
  newer version makes `LoadSave::jsonToState` return `false` without
  throwing, and `setStateInformation` then still applies the state's
  `"tuning"`: the preset is skipped but the tuning changes. Some malformed
  inputs are undefined behaviour upstream: more than 64 modulations, more
  than 3 wavetables or 8 LFOs, a modulation without `source` or
  `destination` (read through a const reference).
- **Port:**
  - A failed load leaves the state unchanged. `PresetState` loads into a
    scratch state first.
  - The undefined inputs are ignored or rejected as `Error::Corrupted`
    (`port/crates/preset/src/state.rs`).
  - In the plugin, `filter_state` empties a saved state whose preset can't
    load, so restoring it changes nothing (`port/crates/plugin/src/state.rs`).
    A `"tuning"` that is not an object, or that `Tuning::json_to_state`
    refuses (D6), counts as a failed load, so the preset doesn't apply
    either. A newer-version state changes neither the preset nor the
    tuning. Missing keys inside a tuning object follow
    `sas_engine::Tuning::json_to_state`, which reads them as defaults.
- **Why:** a partial load gives a state that matches neither preset.
  Undefined behaviour has no defined output to reproduce. Authored and
  corpus presets never hit these paths: every `error_*` preset fails the
  same way in the reference and the port.
- **Approved:** #7 (preset loading,
  `migration/ledger/2026-10-20-m2-preset-registry.md`), #59 (host state)
  and #65 (host state tuning, `migration/ledger/2026-10-21-m8-plugin-tuning-state.md`).

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

### D6. A keyboard map that indexes outside the scale is rejected at load

*Merged in #24 (tuning files); rejection at load in #62.*

- **Differs:** a keyboard mapping that names a scale degree outside the
  scale (≥ the scale's entry count, or negative) at a position
  `Tuning::loadScale` reads. An example is degrees 9–11 on a 7-degree Scala
  scale. It reaches a scale through a `.kbm`, a saved tuning state
  (`jsonToState`) or a new scale under an existing map (`loadScale`).
- **Upstream:** `Tuning::loadScale` reads `scale[note_in_scale]` outside the
  scale vector (undefined behaviour; `vital/src/common/tuning.cpp:340`).
- **Port:** the load is refused with `TuningError::MappingOutsideScale`, and
  nothing is applied: the start note, reference, table, mapping and names
  keep their previous values (`port/crates/engine/src/tuning.rs`,
  `mapping_fits`). Only the positions `loadScale`'s 256-step table walk
  visits are checked, so the unread entries of a map longer than about 128
  are accepted, as upstream. A scale of at most one entry gives a constant
  tuning and reads no mapping, so any mapping is accepted with it.
- **Why:** there is no defined output to reproduce, and the bounds check
  that replaced the read would panic, which a shipped plugin must not do.
  The authored test inputs that hit it were corrected in #24. These are
  invalid inputs, not narrowed coverage.
- **Approved:** #24 (`migration/ledger/2026-10-21-m7-tuning-files.md`) and
  #62 (`migration/ledger/2026-10-21-tuning-kbm-validation.md`), reviewed by
  Graph Reviewer.

### D7. A line generator with `num_points` outside 1–100 is an error

- **Differs:** `LineGenerator::jsonToState` with `num_points` < 1 or > 100.
- **Upstream:** behaviour is undefined. With no points, rendering a looped
  line reads `points_[-1]`.
- **Port:** `LineGenerator::from_json` returns an error and leaves the
  generator unchanged (`port/crates/common/src/line_generator.rs`).
- **Why:** there is no defined output to reproduce.
- **Approved:** #4 (`sas-common`).

### D8. Source formulas instead of the release build's fast-math codegen

- **Differs:** float results wherever upstream's `-ffast-math` release build
  compiles a source formula into something else (reassociation, folded
  checks, inlined approximations).
- **Upstream (release, fast-math):** the codegen differs from the source.
  - `PredefinedWaveFrames::createSaturatedSin`'s `tanh` is reordered.
  - The default saw's `i / 2047.0f` (`wavetable_group.cpp:95`) becomes a
    multiplication by a reciprocal.
  - The JSON dump's `signbit` test is folded, so `-0.0` prints as `0.0`.
  - `std::abs(std::complex<float>)` is inlined as `sqrtf(re² + im²)`.
  - The preset remap curves and wave data are reassociated.
  - Degenerate JSON numbers: `isfinite` is folded, so `1E400` parses as
    infinity, NaN and infinity dump as a garbage double, and the DAZ flag
    makes subnormal doubles dump as `0.0`.
- **Port:** follows the source.
  - `futils::tanh` keeps the source order.
  - The saw divides by 2047.
  - The dump tests `signbit`.
  - `abs` is `hypotf` (`port/crates/wavetable/src/complex.rs`).
  - The remap and wave data follow the source formulas.
  - JSON follows nlohmann's documented behaviour: overflow is a parse
    error, NaN and infinity dump as `null`, and subnormals print normally
    (`sas-common`).
  - The result is bit-identical to the `REF_FAST_MATH=OFF` build. Against
    the official build the difference is rounding-level. For example,
    `keyframe_waves` is 1.2e-7 instead of exact, and preset values differ
    by up to 96 ulp, within 1e-6. All of it is inside the parity
    tolerances and the SAS-36 JSON bound.
- **Exception:** A10, `std::complex<float>` division at the phase
  modifier, follows the official build's plain float formula on every
  target since #54. That is a deliberate return to the release form, so it
  is not a divergence.
- **Why:** the fast-math forms are compiler artefacts with no source to
  port. Mirroring them would mean reproducing GCC's choices call site by
  call site, and they differ between compilers and targets. Authored
  content never hits the degenerate JSON values.
- **Approved:** owner decision 2026-10-21. It is recorded in
  `migration/LEDGER.md` ("Source semantics, owner decisions and the
  remaining cases (2026-10-21)", and "M4a `sas-common`", Decisions, for
  the JSON numbers), in `decisions/owner-decisions-json-and-morphs.md`
  and in `migration/ledger/2026-10-20-m2-preset-registry.md`. Implemented
  in #4, #6, #7 and #8.

### D9. Unsupported `clone()` panics

- **Differs:** `clone()` on `Upsampler`, `Decimator`, `IirHalfbandDecimator`,
  `VoiceHandler`, `SynthVoiceHandler` and `SoundEngine`.
- **Upstream:** `VITAL_ASSERT(false); return nullptr;`. The assert is
  compiled out of release builds, which return `nullptr`. `SoundEngine` has
  no override: it inherits `SynthModule::clone`, which copies it as a plain
  `SynthModule`, slicing off the engine.
- **Port:** `clone` / `clone_box` panics, with a message naming upstream's
  assert.
- **Why:** nothing in the engine clones these processors. A `nullptr`
  processor has no Rust equivalent other than a panic or an `Option`, and
  an unreachable path does not justify an `Option` in every caller.
- **Approved:** #30 (oversampling: `Upsampler`, `Decimator`,
  `IirHalfbandDecimator`), #21 (`VoiceHandler`) and #75 (`SynthVoiceHandler`,
  `SoundEngine`).

### D10. No FMA contraction, no estimates, and JUCE's fallback FFT

- **Differs:** floating-point codegen and the FFT backend, mainly on the
  shipping target (arm64 macOS).
- **Upstream (release):**
  - The real Mac build contracts multiply-adds. Apple clang defaults to
    `-ffp-contract=on`, and upstream's NEON `vmlaq_f32`/`vmlsq_f32` fuse
    into FMLA.
  - JUCE's FFT uses Accelerate/vDSP (`JUCE_USE_VDSP_FRAMEWORK` defaults to
    1).
  - On x86, `-ffast-math` turns vectorised division and `sqrtf` into
    `RCPPS`/`RSQRTPS` estimates plus a Newton step. It also sends
    auto-vectorised libm calls to glibc's libmvec.
- **Port:**
  - It never contracts (no `mul_add`/`vfmaq_f32` where upstream multiplies
    and adds separately).
  - It uses no reciprocal or rsqrt estimates, and division and sqrt are
    exact.
  - It ports JUCE's portable fallback FFT on every target.
  - Transcendentals go to the platform libm without libmvec.
- **Why:** these are CPU- and compiler-dependent rounding with no source
  to port. Fused contraction and estimate instructions make results
  differ between machines. The pinned reference avoids them too (see
  Verification exceptions).
  - Measured on macOS arm64: contraction −91.2 dB residual, vDSP FFT
    −127.5 dB.
  - Both are far below every tolerance.
- **Approved:**
  - `decisions/reference-arm.md`: "Consequences for the Rust port" 2–3
    and "Residual risks", owner-approved with #2.
  - `decisions/reference-determinism.md`, for x86 `RCPPS` and libmvec.
  - migration/AGENTS.md hard rule 5 and "Reference-specific facts".

### D11. A note outside the tuning table reads the table's nearest end

*Merged in #62.*

- **Differs:** `Tuning::convertMidiNote` for a note more than 128 keys from
  the scale's start note. The start note comes from a `.kbm` header's
  middle note or a saved `scale_start_midi_note`, and can be any int; any
  start note outside 0–128 puts some MIDI notes 0–127 outside the table.
- **Upstream:** `tuning_[kTuningCenter + note - scale_start_midi_note_]` is
  read unchecked, outside the 256-entry table for such notes (undefined
  behaviour; `vital/src/common/tuning.cpp:367`). Notes inside the table read
  it correctly whatever the start note.
- **Port:** any start note loads, as upstream. The index is clamped to the
  table, so a note past either end reads that end's entry. Notes inside the
  table read exactly what upstream reads. The offset is computed in `i64`,
  so no start note or note overflows (`port/crates/engine/src/tuning.rs`,
  `convert_midi_note`).
- **Why:** there is no defined output to reproduce, and the unchecked index
  would panic in the port. Refusing such start notes at load would reject
  files upstream plays correctly for most notes: a middle note of 200
  serves MIDI notes 72–127.
- **Approved:** #62 (`migration/ledger/2026-10-21-tuning-kbm-validation.md`),
  reviewed by Graph Reviewer.

### D12. Retired

D12: retired (temporary, removed in #82).

### D13. LLM control layer: additions with no upstream counterpart

*Started in #91 (L1, tool core); extended by #92 (L2: modulation, LFO,
wavetable and preset tools), #95 (L3: providers, agent loop, `sas-llm`)
#100 (L4: the tools on the running plugin), #101 (L5: the chat panel),
#102 (L6: the listen loop) and L8 (the chat docked in the drawer). L7 (the
MCP server) extends this entry when it lands.*

- **Differs:** the port adds a layer that lets a language model read and
  edit the synth by name: the tools in `port/crates/llm-control`, and later a
  chat panel in the plugin, a model provider client, an offline preview
  renderer and a standalone MCP server.
- **Upstream:** has nothing like it.
- **Port:**
  - Every tool edit is one upstream's GUI can also make: a control value
    clamped to its registry range, and later a modulation connect or
    disconnect, an LFO shape or a wavetable. The engine reacts to it as it
    would to the same edit made by hand, so no engine code path changes and
    no host parameter is added.
  - A modulation connect follows upstream's GUI
    (`SynthGuiInterface::connectModulation` and the drag path in
    `ModulationManager`): a created slot gets a linear remap curve, power
    0, stereo and bypass off, the drag path's default amount, and a
    unipolar route unless the model asks for bipolar (a plain drag, with no
    modifier key).
  - The model client (`port/crates/llm-agent`) talks to a configurable
    provider: Anthropic's Messages API by default, or any OpenAI-compatible
    endpoint. Its HTTPS code is behind the `http` feature, off by default,
    so the default build and CI contain no network code; tests replay
    recorded exchanges. Keys are never stored in recordings, presets or
    plugin state.
  - Wavetables the tools make are generated (basic shapes or harmonic
    spectra in a Wave Source); preset files are read and written only in
    the user's preset folder, and an existing file is replaced only on
    request.
  - Values are given in display units and converted with upstream's
    `ValueBridge` formulas (`sas_preset::display`, shared with the plugin's
    host text entry, D1).
  - The chat is off until the user turns it on (`ChatSettings::enabled`,
    false by default, in a per-user settings file; a missing or damaged
    file leaves it off). With it off, or with no key for a hosted provider,
    the plugin behaves exactly as without the layer and makes no network
    requests. A local OpenAI-compatible server needs no key.
  - On the running plugin (`sas_plugin::live`), a tool edit is a host
    parameter gesture, an edit bus edit, or a whole-state load through the
    host, exactly as the editor makes them. Undo restores the same way.
  - The editor has a chat panel (`sas_gui::chat`). Its toggle is our logo
    section, the product-name area of D14, with a small "chat" caption,
    shown while that section is: not while the wavetable editor replaces
    the header, nor under the save and delete sections, which cover the
    whole interface. The toggle opens a drawer: the window widens by 360 points at the
    editor's scale (G7e), and the panel docks there at full height, beside
    upstream's unchanged layout. If the host keeps the window's width, the
    panel overlays the editor's right side instead, below the header and
    inside the editor's bounds. It is coloured from the skin's Overlay
    section, and placed from the editor's layout, so it scales and
    letterboxes with it (`gui-plan.md` §4).
    The chat runs in the plugin (`sas_plugin::chat`) and lasts as long as
    the plugin instance; it is never saved. Its API key is kept in the
    macOS Keychain (or an environment variable), never in a file. Only the
    macOS plugin has network code.
  - The `audition` tool (`port/crates/llm-listen`) renders a short preview
    of the current patch on a second synth, on a thread of its own and never
    the audio thread, and returns compact analysis features, never audio.
    A preview is at most 6 s of audio at 44.1 kHz, cut short after 5 s of
    wall-clock rendering.
  - The preview thread draws seeds from a counter of its own
    (`sas_common::random::use_thread_seed_counter`, opt-in per thread), so
    building preview synths and loading patches on them never moves the
    process's seed counter: the host's instances and later wavetable loads
    get the same seeds as in a session without auditions. Each audition
    builds a fresh synth from that counter restarted at 0, so the same patch
    always gives the same analysis. Nothing sets the global counter.
  - Working defaults (owner, 2026-10-22, all reversible): edits apply at
    once with undo; raising master volume above 0 dB waits for the user's
    confirmation (`Policy::confirm_above`); chat history is not saved; the
    listen loop sends analysis, never audio; the system prompt does not name
    the upstream synth.
- **Why:** the owner's goal after the port is an LLM chat inside the plugin
  that drives the synth from FL Studio (set controls, route modulations,
  shape LFOs, make wavetables, save presets), with an MCP server for outside
  agents second.
- **Approved:** owner (Salil), 2026-10-22, the LLM plan and its working
  defaults, relayed by the coordinator. Recorded in
  `migration/ledger/2026-10-23-llm-l1-tool-core.md`.

### D14. The editor shows the product name where upstream shows its logo

- **Differs:** the header's logo area (`LogoSection`).
- **Upstream:** draws its logo (`Paths::vitalV`, `Paths::vitalRing`) in the
  logo area, and a click opens its About section.
- **Port:** draws the text "synth-a-size" in the logo area, in the title
  face and the logo section's primary colour (`port/crates/gui/src/interface.rs`,
  `paint_logo`). The area is also the LLM chat's toggle (D13), with a small
  "chat" caption under the name; clicking it opens or closes the chat panel
  instead of an About section. A minimal About of our own is still planned.
- **Why:** migration/AGENTS.md rule 2: UI text and art must not use the
  upstream product names. The placeholder text is the owner's working
  default until a product mark exists.
- **Approved:** owner working default, 2026-10-22 (project context, "GUI
  and LLM build: working defaults"); `decisions/gui-framework.md`.

### D15. A letterboxed editor on a high-DPI screen keeps one unit throughout

- **Differs:** the editor's layout when the window's aspect ratio differs
  from 1400:820 *and* the display scale is above 1 (for example a Retina
  screen and a host that ignores the editor's aspect-ratio hint).
- **Upstream:** `FullInterface::resized`
  (`vital/src/interface/editor_sections/full_interface.cpp:411-424`) works
  in physical pixels, `width = ceil(getWidth() * display_scale_)`, and scales
  the size ratio by the display scale in its normal branch. Its two
  letterbox branches don't:
  - they set `ratio = height_ratio` or `width_ratio` without
    `display_scale_`, so every skin value is half size at 2x;
  - they compute `left = (getWidth() - width) / 2` and `top` from the logical
    window size minus a physical width or height, so the interface is
    off-centre, with a negative `left` at 2x.
- **Port:** `FullLayout::new` (`port/crates/gui/src/layout.rs`) letterboxes
  in physical pixels throughout. The result is what upstream computes at
  display scale 1 for a window of the same physical size. At display scale 1,
  and at any scale when the aspect ratio is the default, the port and
  upstream agree exactly; the layout fixtures check this at six sizes.
- **Why:** upstream's result is a units bug with no meaningful layout to
  reproduce: half-size sections, off-centre. This follows D1's precedent of
  not replicating upstream bugs. The editor asks hosts to keep the aspect
  ratio, as upstream's constrainer does, so the case needs a host that
  ignores that.
- **Approved:** coordinator relaying the review of #93 (GUI Reviewer,
  2026-10-23); `decisions/gui-framework.md`.
- **Open, related:** whether upstream's coordinates below the interface are
  also physical pixels on Retina, which would make its knob drag length
  (200 per turn) half as many points there. The port drags in points. The
  FL Studio checklist in `decisions/gui-framework.md` (item 2) compares
  the two on the owner's Mac; record the answer here.

### D16. With the editor closed, the host is not told when a learned MIDI controller moves a parameter

- **Differs:** what the host sees when a MIDI-learned controller sets a
  parameter.
- **Upstream:** `SynthBase::valueChangedThroughMidi` sets the control, then
  `setValueNotifyHost` calls JUCE's `setValueNotifyingHost` on the
  parameter's `ValueBridge`, from the audio thread. The host is notified of
  the edit, and can record it as automation, whether or not the editor is
  open.
- **Port:** the engine sets the control the same way, and the plugin stores
  the value on its own host parameter (`SynthParams::set_from_midi`,
  `port/crates/plugin/src/params.rs`). The editor shows it, saved states
  keep it, and like upstream it is not sent back to the engine as a host
  edit.
  - **Editor open:** the audio thread marks the moved parameter (a
    lock-free bit), and on its next frame the editor sends the host a
    begin, set and end gesture with the value the parameter holds then,
    through nice-plug's `GuiContext` (`Host::forward_midi_moves`,
    `port/crates/gui/src/host.rs`). A parameter the user is dragging gets
    only the value, inside the drag's gesture. The host hears of the move
    a frame later than upstream's notification, once per frame, as a
    gesture where upstream sends a bare edit per change.
  - **The host's echo is guarded:** during playback nice-plug only passes
    the edit to the host (`performEdit`), and the host sends it back into a
    later `process` as a parameter change. While a learned controller keeps
    moving, that value can be up to about a frame older than the newest
    MIDI value. Upstream's echo carries the current value (its notification
    comes from the audio thread, and JUCE drops an echo equal to the current
    value), so the control never steps back. The port matches that: the
    editor records each value it forwards (as nice-plug's normalized bits),
    and a host change equal to one of them, within 0.5 s of the control's
    last MIDI move and while the last MIDI value differs, is the stale
    echo. It does not reach the engine; the parameter takes the last MIDI
    value back and the next frame forwards it
    (`SynthParams::take_stale_echo`, `port/crates/plugin/src/params.rs`).
    Any other host change is automation and applies as before, including
    automation the host recorded from forwarded values, played back once
    the knob is idle. The FL
    Studio checklist (`decisions/gui-framework.md`, item 3) asks the owner
    to confirm there's no stepping and to check automation recording and
    undo with one gesture per frame.
  - **Editor closed:** the host is not sent a parameter edit: it sees the
    new value only when it next reads the parameter, and records no
    automation. Moves while the window is closed are not replayed when it
    opens.
- **Why:** nice-plug offers no parameter edits from the audio thread; its
  begin/set/end calls are on the editor's `GuiContext`, which exists only
  while the editor is open.
- **Approved:** Salil, 2026-10-23: accepted as is (option a). Telling the
  host through the editor while it is open is option b (the MIDI learn
  host-gestures PR); doing it from the audio thread is parked as
  `IMPROVEMENTS.md` I19 (option c). Reviewed by Graph Reviewer and Parity
  Reviewer on #99.

### D17. The MIDI learn config loads without upstream's install check

- **Differs:** when the saved MIDI learn map is loaded, and config entries
  upstream cannot read.
- **Upstream:** `Startup::doStartupChecks` calls `LoadSave::loadConfig`
  (which restores the MIDI map) only when `LoadSave::isInstalled()`, that
  is, when its data directory of presets and factory content exists. A
  config entry whose `source` is not a number, or whose `destination` is not
  a string, throws; one that names an unknown parameter reads a details
  entry that does not exist. A config file that is not a JSON object throws
  when the map is saved.
- **Port:** the map loads whenever the config file has one
  (`port/crates/plugin/src/config.rs`). Entries it cannot read are skipped,
  and a config that is not an object is replaced by a new one on save. Of
  `doStartupChecks` the port runs only `loadConfig`'s MIDI map part: it
  does not stamp `synth_version` into the config (`saveVersionConfig`
  after `wasUpgraded`), and reads none of `loadConfig`'s other items (the
  computer keyboard layout).
- **Why:** the port ships no factory content and has no data directory
  (migration/AGENTS.md rule 3), so the install check would never pass and a
  learned map would never come back. Skipping unreadable entries replaces
  a crash or undefined behaviour.
- **Approved:** Salil, 2026-10-23. Reviewed by Graph Reviewer and Parity
  Reviewer on #99.

### D18. One user action is one host gesture, where upstream sets parameters with bare edits

- **Differs:** what the host is told when the editor changes a parameter
  in code rather than through the slider's own drag: upstream sends a bare
  edit per change, the port wraps the user's action in one host gesture.
- **Upstream:** these sites call `setValue` on a slider (or set the control
  through a listener) from their own mouse or menu handling. A
  programmatic `setValue` starts no gesture, so the host gets a bare
  parameter edit per change, with no begin or end around the action.
- **Port:** each such action is wrapped in one host gesture, as the port's
  sliders do: begin on the first change, set for each change, end when the
  action ends. A drag begins on its first movement, so a click that doesn't
  move sends nothing. A one-shot choice (a menu item, an arrow button) is
  one begin, set and end (`Host::set_once`). The values are the same as
  upstream's.
- **Why:** hosts group edits by gesture. With one gesture per action, FL
  Studio records one automation pass, which it can record in touch or latch
  mode, and keeps one undo step for the action, where bare edits can leave
  one step per change. The action is a single user action, as a slider drag
  is. REAPER shows the same with the track's automation mode in Touch or
  Latch.
- **Known sites:**
  - G5: dragging an oscillator's wavetable view moves its wave frame
    (`Wavetable3d::mouseDown` / `mouseDrag`;
    `port/crates/gui/src/controls/wavetable_3d.rs`, #127).
  - G6a: the effect displays' drags and the effects order drag (#124).
  - G3b: the oscillator and sample sections' menus and prev/next arrows
    (spectral morph, distortion and destination types;
    `port/crates/gui/src/sections/oscillator.rs` and `sample.rs`, #118).
  - G4b: dragging a modulation source onto a control
    (`ModulationManager::modulationDraggedToComponent` /
    `setTemporaryModulationBipolar` / `endModulationMap`;
    `port/crates/gui/src/modulation.rs`). Upstream tells the host nothing
    at all here: the slot's values go to the synth with `valueChanged` and
    the connection through the engine's change queue. The port's slot
    values are host parameters, so the connection left at the release gets
    one gesture per slot parameter (power, amount, bipolar, stereo,
    bypass) then; the connections the drag passed over reach the engine
    through the edit bus only and never the host, and keep the drag's
    values in their freed slot as upstream's do.
  Later sites cite D18 rather than adding entries.
- **Approved:** UI Reviewer on #127; the coordinator asked for the
  port-wide rule.

### D19. "Set Start Point" on an LFO grid cell sets the phase

*Proposed in #128.*

- **Differs:** what the LFO editor's right-click "Set Start Point" does when
  it was opened over a grid cell in paint mode (no point or power under the
  pointer).
- **Upstream:** `LfoEditor::mouseDown` captures the hovered point and power
  for the menu's callback but not the grid cell; `respondToCallback`'s
  `kSetPhaseToGrid` reads `getActiveGridSection()` when the choice arrives
  (`vital/src/interface/editor_components/lfo_editor.cpp:123`). By then the
  pointer has moved onto the popup, `LineEditor::mouseExit` has run
  `clearActiveMouseActions()` and the section is -1, so the choice does
  nothing.
- **Port:** `LineEditor` (`port/crates/gui/src/controls/line_editor.rs`)
  captures the cell with the point and power when the menu opens
  (`LineAction::grid_section`), so the choice sets the LFO's phase to the
  cell's start, as the menu item says.
- **Why:** upstream offers the item and then can't carry it out; there is no
  meaningful behaviour to reproduce. This follows D1's and D15's precedent
  of not replicating upstream bugs.
- **Approved:** coordinator relaying the review of #128 (GUI Reviewer,
  Voice tab and modulation, 2026-10-26).

### D20. Text to Wavetable is absent

- **Differs:** a feature upstream has and the port doesn't: generating a
  wavetable from typed text.
- **Upstream:** the wavetable view's right-click menu always has "Text to
  Wavetable" (or "Text to Wavetable - Log in" when signed out;
  `Wavetable3d::mouseDown`), and the oscillator section has the
  text-to-wavetable field it opens (`OscillatorSection::textToWavetable`,
  `ttwt_`, `ttwt_settings_`). The text goes to upstream's online service,
  which needs the user signed in.
- **Port:** neither the menu item nor the field exists
  (`port/crates/gui/src/controls/wavetable_3d.rs`,
  `port/crates/gui/src/sections/oscillator.rs`).
- **Why:** it depends on sign-in and an online account, which the port
  drops (migration/AGENTS.md rule 4). A local or offline alternative is
  parked as I20.
- **Approved:** coordinator, relaying UI Reviewer's review of #137.

### D21. Wavetable JSON that can't be read, pasted or loaded from a file, leaves the oscillator unchanged

- **Differs:** what pasting wavetable JSON onto an oscillator does when the
  text passes the validity check but can't be read, and what loading a
  `.vitaltable` file that can't be read does.
- **Upstream:** the paste checks `WavetableCreator::isValidJson` (an object
  with `version`, `name` and an array of `groups`, or a line shape) and then
  calls `WavetableEditSection::loadWavetable`. That clears the creator, and
  `jsonToState` clears it again, sets the name and author, and reads the
  groups. If a group can't be read (for example `"groups": [7]`), the
  exception is caught and nothing re-renders, so the creator is left
  cleared and partly loaded while the oscillator keeps playing its old
  table. Loading a `.vitaltable` (the popup browser, the oscillator's
  arrows, a drop) goes through `WavetableEditSection::loadFile`, which
  clears the creator first and, if the file doesn't parse or a group can't
  be read, returns before re-rendering: the same cleared creator over the
  old sound.
- **Port:** `EditorModel::load_wavetable_json` reads the JSON into a copy of
  the oscillator's creator and sets it only if reading succeeds
  (`PluginModel`, `port/crates/plugin/src/editor.rs`). Unreadable JSON
  returns false and leaves the creator, the table and the edit bus revision
  unchanged. A `.vitaltable` file goes through the same call
  (`popup_browser::load_wavetable_file`), and one that doesn't parse returns
  false before it, so it too leaves the oscillator unchanged.
- **Why:** upstream's result is a creator that no longer matches the sound,
  and saving the preset then would store the partial state. Rejecting the
  paste keeps the two consistent. Valid JSON loads exactly as upstream's.
- **Approved:** coordinator, relaying UI Reviewer's and Graph Reviewer's
  reviews of #137; extended to files, relaying UI Reviewer's review of
  #172.

### D22. "Resynthesize Preset to Wavetable" renders a fresh synth on a worker thread

- **Differs:** which engine renders the patch for the wavetable view's
  "Resynthesize Preset to Wavetable", and on which thread.
- **Upstream:** `SynthBase::renderAudioForResynthesis`
  (`vital/src/common/synth_base.cpp`) takes the audio lock and renders on
  the GUI thread with the playing engine. `allSoundsOff` kills the voices
  and hard-resets the effect chain and decimator, then 1 s of running in
  (`correctToTime` from -1 s, 64-sample blocks), note 16 at velocity 0.7
  for 4 s, the left channel normalized to its peak (at least 0.01), and
  `allSoundsOff` again. The engine keeps everything else it had:
  - its random generators where the session left them;
  - its host tempo for tempo-synced modulators and effects;
  - the state of global modulators.

  The GUI and the audio both stop until the render is done.
- **Port:** the same render, step for step
  (`port/crates/plugin/src/resynthesis.rs`), on a fresh synth built from
  the current patch and its live controls, on a thread of its own with its
  own seed counter restarted at 0. It runs at the host's tempo from the
  last block that reported one (120 BPM before any did), so tempo-synced
  patches render as upstream's would. The result loads
  through the edit bus like any wavetable edit, so the editor and the
  audio keep running and the table changes when the render is done (about
  half a second). The creator step is upstream's: pitched audio with wave
  blending, named "Resynthesize", window one period of the note.
- **Why:** the audio thread's engine can't be locked or rendered from the
  GUI thread here, and rendering on it would stop the host's audio. A
  fresh synth from rest with fixed seeds also makes the result depend
  only on the patch.
- **Audible differences:** patches with randomness (random LFOs, random
  unison phases, the noise sample's position) get the fresh synth's draws
  rather than the session's. Other patches render as upstream's do. Before
  the host has reported a tempo, the render uses 120 BPM, upstream's
  default.
- **Approved:** coordinator (render off the GUI thread, seed isolation,
  record the differences); Graph Reviewer reviews on the resynthesis PR.

### D23. Audio files dropped on a wavetable view: CAF and rarer CoreAudio types aren't read, and lossy decoders can differ

- **Differs:** which audio files a wavetable view takes when one is dropped
  on it, and the samples lossy files decode to.
- **Upstream:** `AudioFileDropSource` takes one file in any format JUCE's
  `AudioFormatManager::registerBasicFormats()` reads. On macOS that is
  WAV, AIFF, FLAC, Ogg Vorbis (JUCE's own readers) and CoreAudio's formats:
  MP3, AAC/M4A, CAF and rarer types (AC3, AMR, 3GP and others).
- **Port:** the view takes `.wav`, `.bwf`, `.aif`, `.aiff`, `.flac`,
  `.ogg`, `.mp3`, `.m4a` and `.aac` (`port/crates/plugin/src/audio_file.rs`).
  WAV is read by the port's own reader. The others decode with Symphonia
  0.6.1 (MPL-2.0; see the bundle notices). Other files get no copy badge,
  and the drop is refused.
  - **Lossless formats match JUCE sample for sample.** The authored WAV,
    AIFF and FLAC files in `migration/corpus/audio` decode exactly as the
    reference's JUCE readers do (module `audio_file/decode`, compared at
    zero tolerance).
  - **Lossy formats** (Ogg Vorbis, MP3, AAC/M4A) aren't covered by our
    fixture tests. The project can't author encoded files (`generate.py`
    has no encoders, and hard rule 3 allows no outside content), so they
    rely on Symphonia's own codec tests. Their decoded samples can differ
    slightly from JUCE's Ogg reader and CoreAudio's MP3 and AAC decoders
    (rounding, and the encoder delay and padding they trim). An empty or
    corrupt file of each type is refused without changing anything.
  - **CAF and the rarer CoreAudio types** aren't read.
  - **WAV data lengths:** JUCE reads the frames a `data` chunk declares
    and zero-fills what the file lacks, sizing its buffer from the header.
    The port does the same only within bounds. A declared length of
    `0xFFFFFFFF` or 0, the "unknown length" markers streamed WAVs (ffmpeg
    or sox writing to a pipe) carry, reads the rest of the file, unless
    a 0 is followed by what looks like a chunk header (a printable label
    and a length the file can hold): that is an empty chunk, read as JUCE
    reads it. A declared length more than double the bytes present, or
    past 2^24 frames, reads only the bytes present. Buffers are reserved
    fallibly, so no file can make the plugin abort the host, where JUCE
    would try to allocate gigabytes for such a header.
- **Why:** parity first. The port matches upstream's macOS formats with an
  open-source decoder, since CoreAudio isn't available to it everywhere
  and JUCE's readers aren't part of the port. CAF and the rarer types are
  left out as seldom used for wavetable sources.
- **Approved:** coordinator, 2026-10-26 (the formats, the decoder and the
  lossy-fixture policy); Graph Reviewer reviews the decoder and Parity
  Reviewer the reference case and corpus.

### D24. The user data folder: one default root, and a menu item to choose it

- **Differs:** where the user's presets, wavetables, samples and LFOs live
  by default, how the user moves them, and the export dialog's starting
  name.
- **Upstream:** `LoadSave::getDataDirectory` defaults to a product-named
  folder: in `~/Music` on macOS, `$XDG_DATA_HOME` (or `~/.local/share`)
  on Linux, and `Documents` on Windows. The config's `data_directory`
  overrides it, but only the factory-content installer's install-location
  choice (`download_section.cpp`) and Import Bank write that key. The
  preset menu (`SynthPresetSelector::showPopupMenu`) has no item for it.
  Export Preset's dialog starts with no file name
  (`FileChooser("Export Preset", File(), …)`).
- **Port:** the default root is `~/Music/synth-a-size` on every platform
  (`USERPROFILE\Music\synth-a-size` on Windows;
  `sas_preset::user_folders::default_root`). The config's `data_directory`
  overrides it as upstream's does, falling back to the default when the
  folder doesn't exist. The preset menu ends with "Choose Data Folder...",
  which picks a folder and saves it as `data_directory`
  (`port/crates/gui/src/sections/preset_selector.rs`). Export Preset's
  dialog starts with `<preset name>.vital`. Presets, wavetables, samples
  and LFOs live directly under the root (`<root>/Presets`,
  `<root>/Wavetables`, `<root>/Samples`, `<root>/LFOs`), where upstream
  keeps the user's own under a `User` folder (`LoadSave::getUserDirectory`)
  beside its factory folders; the save section and the chat's preset tools
  write there and share these folders. A data folder from upstream, whose
  content sits in `User/…`, still shows up: `user_folders::directories`
  reads each folder directly inside the root as a bank, `User` included.
- **Why:**
  - The folder name follows the product name (migration/AGENTS.md rule 2).
  - One location keeps the editor's folders and the chat's preset tools on
    the same path. macOS is the shipped target and keeps upstream's
    `~/Music` location. The Linux build is for development only, and
    Windows isn't targeted.
  - The installer that let upstream's users choose the folder isn't ported
    (rule 3), so the menu item takes its place.
  - The pre-filled export name saves typing; the result is the same file.
  - The port ships no factory folders, so a `User` folder would have
    nothing to sit beside.
- **Approved:** coordinator (G7 plan's data_directory decision), on UI
  Reviewer's review of #148, 2026-10-26.

### D25. The preset arrows do nothing in an external preset's empty folder

- **Differs:** the header's previous and next arrows after an external
  preset (Open External, Export) when its folder still exists but holds no
  presets, for example because the file was deleted or renamed away.
- **Upstream:** `PresetBrowser::jumpToPreset` lists the folder's presets and
  takes `(index + indices + presets.size()) % presets.size()`, a modulo by
  zero for an empty folder (undefined behaviour), then reads `presets[index]`.
- **Port:** `user_folders::step` returns `None` for a folder with no presets,
  and the arrow does nothing
  (`port/crates/gui/src/sections/preset_browser.rs`, `jump_to_preset`).
- **Why:** there is no defined result to reproduce. Doing nothing leaves the
  current preset playing, which is what a folder with nothing to step to
  suggests.
- **Approved:** coordinator, relaying Voice Reviewer's review of #159,
  2026-10-26.

### D26. The popup browser always has its search box, and no store button

- **Differs:** the top of the popup browser that sections open to choose a
  wavetable, sample or LFO file.
- **Upstream:** `PopupBrowser::checkNoContent` shows a "Download content"
  button in place of the search box until the factory content is installed
  (`LoadSave::hasDataDirectory` needs the installer's `packs.json` in the
  data folder); it starts the factory download (`startDownload`).
  `checkStoreButton` shows a "Get more … by <author>" button, opening
  upstream's store, for the loaded file's author when that author has packs
  on sale (`getAvailablePacks`).
- **Port:** the search box always shows, where the download button would
  be, and there is no store button
  (`port/crates/gui/src/sections/popup_browser.rs`).
- **Why:** the port ships no factory content or installer (rule 3), and the
  store needs upstream's accounts and servers (rule 4). Without them, the
  search box is what upstream shows once content is installed.
- **Approved:** coordinator, 2026-10-26.

### D27. A cycle of modulation amounts reads at a share instead of hanging

- **Differs:** amount knobs whose connections modulate each other's amounts
  in a loop, which a preset can hold. For example, slot 3 on
  `modulation_4_amount` and slot 4 on `modulation_3_amount`.
- **Upstream:** only a connection to its own slot's amount is rejected
  (`isInvalidConnection`). `ModulationManager::getAuxMultiplier`, and the
  walks in `sliderValueChanged`, `setModulationSliderValues` and
  `setModulationSliderScale`, follow `aux_connections_to_from_` or
  `aux_connections_from_to_` in a `while` loop. Around a cycle that loop
  never ends, so dragging or loading such a knob hangs the interface.
- **Port:** `AuxLinks::driven` and `AuxLinks::display_end` stop before the
  first slot they would revisit (`port/crates/gui/src/modulation.rs`,
  `walk`). In the example, each knob drives the other slot's amount at half
  its shown value, as a one-step chain does.
- **Why:** there is no defined result to reproduce. Stopping where the chain
  turns back on itself gives each knob the share of the longest chain
  without repeats, so it shows and moves a sensible amount.
- **Approved:** coordinator, relaying Voice Reviewer's review of #170,
  2026-10-26.

### D28. "Add Folder" refuses a folder that links back into itself

- **Differs:** what the popup browser's "Add Folder" does with a folder
  holding a symbolic link to itself or to a folder above it.
- **Upstream:** `isAcceptableRoot` walks the folder with
  `findChildFiles(File::findDirectories, false)`, linked folders followed,
  and refuses it once more than `kMaxRootFiles` (8,000) files are counted.
  A loop is walked round and round, each pass deeper in the path. With
  files in it, the count soon passes 8,000 and the folder is refused. With
  none, the walk goes on until the system refuses the path, and with two
  such links it doubles at every level, so the host hangs.
- **Port:** `popup_browser::is_acceptable_root` walks and counts the same
  way, linked folders followed, so a folder without a loop gets upstream's
  answer. A folder whose walk reaches one of the folders above it is
  refused at once, with the same "too many files" message.
- **Why:** no folder a user picks may hang the host. A looped folder with
  files is refused by upstream too, and the browser's listings follow
  links as upstream's do, so an accepted loop would only move the hang to
  the listing.
- **Approved:** coordinator, 2026-10-26.

### D29. No "Open File Location" menu on the All, Favorites and Add Folder rows

- **Differs:** a right-click on the All, Favorites or Add Folder row of the
  preset browser's or popup browser's folder list.
- **Upstream:** `SelectionList::menuClick` offers "Open File Location" on
  any hovered row. For these three rows the row is a made-up file under
  `cwd/_` that doesn't exist, so `revealToUser` does nothing.
- **Port:** the menu is offered only on rows that are real files or
  folders (`selection_list.rs`); a right-click on these three rows opens
  nothing.
- **Why:** a menu item that does nothing is left out rather than copied.
- **Approved:** coordinator, 2026-10-26.

### D30. The saved window size isn't kept within the screen

- **Differs:** a saved window size larger than the screen the window opens
  on, and a corner drag past the screen's edge.
- **Upstream:** `SynthEditor`'s constructor, run each time the window
  opens, reads the config's `window_size` and keeps it within the desktop's
  total bounds less 50 pixels of height
  (`Desktop::getInstance().getDisplays().getTotalBounds`) before opening at
  1400 × 820 times it.
- **Port:** each window opens at 1400 × 820 times the saved size (at least
  0.25), as upstream's does, without the screen bound. Upstream also keeps
  a size dragged with the corner resizer within the display
  (`BorderBoundsConstrainer::checkBounds`); the port's corner doesn't.
- **Why:** baseview, which opens the window, reports no screen or display
  size, so there is nothing to bound it by before the window opens. A size
  saved on a large screen can open larger than a smaller one; the host can
  still shrink the window.
- **Approved:** coordinator, 2026-10-26.

### D31. No factory wavetables

- **Differs:** the wavetables the oscillator's and the wavetable editor's
  browsers and arrows offer.
- **Upstream:** the factory content installer puts upstream's wavetables in
  a factory folder inside the data folder, beside the user's; the browsers
  list every bank's Wavetables folder (`LoadSave::getWavetableDirectories`),
  so the factory tables are listed and stepped through with the user's.
- **Port:** no factory wavetables ship, and there is no installer. The
  browsers and arrows list the user's wavetables, any bank folders in the
  data folder (an upstream data folder with its factory content included),
  and added folders.
- **Why:** the port ships no factory content (migration/AGENTS.md rule 3),
  and the downloader isn't ported (D26).
- **Approved:** coordinator, 2026-10-27.

### D32. The wavetable editor loads any audio file, not only `.wav`

- **Differs:** a `.flac`, or a `.wav` with its extension in upper case,
  loaded from the wavetable editor's browser, arrows or Import.
- **Upstream:** `WavetableEditSection::loadFile` takes a file as audio
  only when its extension is exactly `.wav`, and parses everything else as
  a `.vitaltable`. The browser lists `.flac` and any case of `.wav`, but
  they fail to parse and load nothing. The oscillator's `loadFile` loads
  them as audio.
- **Port:** the editor loads everything that isn't a `.vitaltable` as
  audio, spliced, as the oscillator does
  (`popup_browser::editor_load_wavetable_file`).
- **Why:** a file the browser offers should load, as in D1 and D19.
- **Approved:** coordinator, 2026-10-27.

## Verification exceptions

These are policies of the parity harness, not port behaviour. They change
**what the port is compared against or how**, never what the port does. Each
has its own decision record.

- **The pinned reference build.** Parity is measured against a pinned C++
  build of `/vital`, not upstream's shipped binaries.
  - Flags: `-ffp-contract=off`, `-mno-recip` and `-U__FAST_MATH__` (no
    libmvec).
  - glibc is re-executed with `GLIBC_TUNABLES` pinned to its baseline
    variants.
  - `JUCE_USE_VDSP_FRAMEWORK=0` (JUCE's fallback FFT).
  - The macOS runner image is fixed; macOS 15 is the primary reference.
  - See `decisions/reference-determinism.md` and
    `decisions/reference-arm.md`. D10 is the port-side counterpart.
- **Tolerance floors and `tolerance_exception`.** Outside SAS-35, parity is
  within tolerances, not exact.
  - The floors `RENDER_FLOOR` and `MODULE_FLOOR` were owner-approved on
    2026-10-20 and are pinned in `run_corpus.py` and sas-parity.
  - A case may only loosen with a structured `tolerance_exception` citing
    a dated ledger heading that names it.
  - See `decisions/reference-determinism.md` and migration/AGENTS.md
    rule 6.
- **Normalized-form captures.** Large-scale quantities (Hz, ms, samples)
  may be captured in the form their upstream consumer uses, computed the
  same way in the ref-module and the parity test. For example, a keytrack
  frequency is captured as `frequency * (1.0f / sample_rate)`, with a raw
  case over the band that passes. See migration/AGENTS.md.
- **Bounded JSON state (SAS-36).** Declared JSON artifacts compare
  structurally, with floats within 4 f32 ulp or 1e-6. See
  `decisions/owner-decisions-json-and-morphs.md`.
- **No-fast-math exact comparisons (SAS-35).** Allowlisted noisy-reference
  cases compare bit for bit against the `REF_FAST_MATH=OFF` build, gated
  per target by `--justify-nofastmath`. See `decisions/nofastmath/` and
  `migration/ledger/2026-10-21-owner-approval-sas-35-sas-36.md`.
  - **A11 constraint:** the port follows the official build's merged
    sin/cos in the phase modifier (A11). So no `reference: nofastmath` case
    may render a phase modifier, whose no-fast-math form would differ. See
    `decisions/nofastmath/README.md`.
- **macOS CI is advisory (Mac CI policy).** Merges are gated on Linux CI and review; macOS parity runs per PR as advisory and in full on main. See `decisions/reference-arm.md` ("Mac CI policy").
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
