## 2026-10-26: Copyright and credit audit

Port files translate upstream and JUCE code nearly line by line, so they
keep the original copyright notices (rule 1). This audit checked all 313
Rust files under `port/crates` (not `port/vendor`). In the Rust sources it
changed headers only, never code.

**Upstream.** 145 files already started with the upstream credit line.
Seventeen more get it:

- 13 credited upstream only inside their module docs: `common`'s
  `line_generator`, `load_save`, `random` and `scalar`, `wavetable`'s
  `wave_frame`, `wavetable` and `fourier_transform`, and the six
  `wavetable::creator` modules. That credit stays where it is.
- The `common` and `wavetable` crate roots, as the other ported crates'
  roots already have it.
- Two test tables copied from `synth_parameters.cpp`
  (`effects/tests/common/params.rs`,
  `modulation/tests/common/parameters.rs`).

`engine/src/midi_file.rs` loses the upstream line it had. It mirrors the
harness's `ref-render` and JUCE's `MidiFile` and ports no upstream code
(Chain Reviewer).

The other files without the line are our own code: the LLM crates, the
harness and parity crates, plugin glue, egui rendering, and tests that read
or only cite upstream (`params_table.rs` parses the upstream file and checks
it against the copy).

**JUCE.** JUCE 6.0.5's file headers give two licences:

- ISC for `juce_core`, `juce_audio_basics`, `juce_audio_devices` and
  `juce_events`;
- commercial or "the GPL v3" for every other module. With no "or later",
  that is GPL-3.0-only (GPLv3 section 14; Parity Reviewer).

Sixteen files reproduce JUCE functions and now carry a
`Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: <module>, <licence>)`
line:

- **GPL-3.0-only:**
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

The seven files with GPL-3.0-only portions say
`SPDX-License-Identifier: GPL-3.0-or-later AND GPL-3.0-only`. Their crates
(`sas-gui`, `sas-plugin`, `sas-preset`, `sas-wavetable`) set
`license = "GPL-3.0-or-later AND GPL-3.0-only"` in `Cargo.toml` instead of
inheriting the workspace's `GPL-3.0-or-later`. ISC portions change neither.

Not credited, because they aren't translations:

- `gui/src/paint.rs`, `fonts.rs` and `layout.rs` draw what JUCE's
  `Graphics` would, with egui;
- the automation timing in `plugin/src/lib.rs` matches one behaviour of
  JUCE's VST3 wrapper;
- `common/src/random.rs` and `wavetable/src/complex.rs` emulate C++
  standard-library results, not library code.

`controls/path.rs` (`Path`, `PathStrokeType`) merged with #116 without
the JUCE line. It gets the same `Portions` line, `juce_graphics,
GPL-3.0-only`, and the compound SPDX line, identical to Tabs Builder's G6a
branch, so the two merge cleanly. `controls/icon.rs` (its own rasteriser)
and `controls/shape_button.rs` (upstream's button, with JUCE's behaviour
only) aren't JUCE translations.

**The plugin as a whole is GPLv3 only.** With GPL-3.0-only portions, the
combined plugin can be distributed under GPL version 3 only. Statements
that said "or later" for the whole now say so:

- rule 1 in `migration/AGENTS.md`;
- the README;
- the opening of the bundle notices;
- the bundle's `Info.plist` copyright string (`GPL-3.0-only`).

`UPSTREAM.md` now gives JUCE's split: ISC modules, and the GPL option as
version 3 only. `PLAN.md` makes no licence statement. Upstream's own code
stays GPL-3.0-or-later, as its headers say.

**Notices.** The bundle's `THIRD_PARTY_NOTICES.md` gains "Code translated
from C++ libraries":

- JUCE, with each module's licence ("version 3 only" for the GPL ones) and
  the ISC permission text;
- nlohmann/json 3.1.2, whose Grisu2 double formatting
  `common/src/json/dtoa.rs` translates line by line (MIT, notice already in
  that file). `json.rs` itself only matches nlohmann's semantics with its own
  code.

The notices don't name upstream or its author (rule 2), and
`check_names.sh` now scans the notices folder to enforce that.

**Policy.** Rule 1 in `migration/AGENTS.md` now gives:

- the header lines, and when each applies;
- the JUCE modules under each licence;
- the compound SPDX and Cargo licence that a GPL-3.0-only portion
  requires;
- when a library goes in the notices.

**Owner approval (Salil, 2026-10-26):** rule 1's licence statement
changes from "Everything stays GPL-3.0-or-later" to:

- our code is GPL-3.0-or-later;
- files with JUCE GPL portions are also GPL-3.0-only;
- the shipped plugin is GPLv3 only.

He approved #120 as written. The alternative, keeping the whole plugin "or
later", would have meant replacing the seven GPL-3.0-only portions with
code not derived from JUCE. The FFT fallback is among them, and the
reference pins it for parity.

Checks: `check_names.sh` passes (and fails on an injected name in the
notices), `cargo fmt --check` passes, `cargo metadata` accepts the compound
licences, and `ledger.py --check` passes.
