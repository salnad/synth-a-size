## 2026-10-26: Copyright and credit audit

Port files translate upstream and JUCE code nearly line by line, so they
keep the original copyright notices (rule 1). This audit checked all 313
Rust files under `port/crates` (not `port/vendor`) and changed headers only:
65 added lines at the top of 31 files, no code.

**Upstream.** 145 files already started with the upstream credit line.
Seventeen more get it:

- 13 credited upstream only inside their module docs: `common`'s
  `line_generator`, `load_save`, `random` and `scalar`, `wavetable`'s `wave_frame`,
  `wavetable` and `fourier_transform`, and the six `wavetable::creator`
  modules. That credit stays where it is.
- The `common` and `wavetable` crate roots, as the other ported crates'
  roots already have it.
- Two test tables copied from `synth_parameters.cpp`
  (`effects/tests/common/params.rs`,
  `modulation/tests/common/parameters.rs`).

The other files without the line are our own code: the LLM crates, the
harness and parity crates, plugin glue, egui rendering, and tests that read
or only cite upstream (`params_table.rs` parses the upstream file and checks
it against the copy).

**JUCE.** JUCE 6.0.5's file headers give two licences: ISC for
`juce_core` and `juce_audio_basics`, and commercial or GPLv3 for the
rest. Sixteen files reproduce JUCE functions and now carry a
`Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: <module>, <licence>)`
line:

- **GPLv3 option:**
  - `juce_dsp`: `wavetable/src/fourier_transform.rs` (`FFTFallback`);
  - `juce_gui_basics`: `gui/src/controls/slider.rs` (`Slider`,
    `handleAbsoluteDrag`, the multiple-click rules),
    `gui/src/controls/button.rs`, `gui/src/scope.rs` (`findColour`), and,
    together with `juce_core` (ISC), `preset/src/state.rs`
    (`valueToProportionOfLength`, `String::trim`,
    `getFileNameWithoutExtension`);
  - `juce_graphics`: `gui/src/colour.rs` (`Colour`);
  - `juce_data_structures`: `plugin/src/config.rs`
    (`PropertiesFile::Options::getDefaultFile`).
- **ISC:**
  - `juce_core`: `common/src/base64.rs`, `common/src/load_save.rs`
    (`getIntValue`), `engine/src/tuning.rs` (`juce_text`),
    `gui/src/controls/format.rs` (`String(double)`, `getDoubleValue`);
  - `juce_audio_basics`: `engine/src/keyboard.rs` (`MidiKeyboardState`),
    `engine/src/midi_file.rs` (`MidiFile`), `engine/src/midi_manager.rs`
    (`MPEZoneLayout`), `dsp-core/src/denormals.rs` (`ScopedNoDenormals`),
    `plugin/src/processor.rs` (`MidiBuffer::addEvent` ordering).

Not credited, because they aren't translations:

- `gui/src/paint.rs`, `fonts.rs` and `layout.rs` draw what JUCE's
  `Graphics` would, with egui;
- the automation timing in `plugin/src/lib.rs` matches one behaviour of
  JUCE's VST3 wrapper;
- `common/src/random.rs` and `wavetable/src/complex.rs` emulate C++
  standard-library results, not library code.

`controls/path.rs` (`Path`, `PathStrokeType`, `juce_graphics`) is only on
#116's branch. It needs the same `Portions` line, with `juce_graphics,
GPL-3.0 option`.

**Notices.** The bundle's `THIRD_PARTY_NOTICES.md` gains "Code translated
from C++ libraries":

- JUCE, with each module's licence and the ISC permission text;
- nlohmann/json 3.1.2, whose Grisu2 double formatting
  `common/src/json/dtoa.rs` translates line by line (MIT, notice already in
  that file). `json.rs` itself only matches nlohmann's semantics with its own
  code.

The notices don't name upstream or its author (rule 2), and
`check_names.sh` now scans the notices folder to enforce that.

**Policy.** Rule 1 in `migration/AGENTS.md` now gives the header lines,
when each applies, and when a library goes in the notices.

Checks: `check_names.sh` passes (and fails on an injected name in the
notices), `cargo fmt --check` passes, and `ledger.py --check` passes.
