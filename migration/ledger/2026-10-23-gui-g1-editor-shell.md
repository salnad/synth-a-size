## 2026-10-23: GUI G1, editor shell, skin and fonts, ref-gui reference harness

**Why.** First GUI milestone (M10, the GUI plan's G1), on the owner's
working defaults of 2026-10-22: egui through nice-plug-egui with wgpu,
upstream's default skin and layout, the product name in place of the logo.

**Changed**
- `port/crates/gui` (`sas-gui`), no longer a stub:
  - `skin`: upstream's `Skin` (sections, value and colour ids, overrides,
    `updateJson`, `shouldScaleValue`), reading `vital/default.vitalskin` at
    build time. `colour`: JUCE's `Colour::fromString`.
  - `scope`: `SynthSection::findValue` / `findColour` along the section
    chain, with size-ratio scaling.
  - `fonts`: the five upstream faces as egui families. A JUCE point height
    is the em size, which is egui's font size too.
  - `layout`: `FullInterface::resized`, `HeaderSection::resized` and the
    preset selector's `resized`, in integer pixels with upstream's
    truncations, letterboxing and 2x scaling.
  - `interface`: background, header (product name, `TabSelector::paint`,
    `PresetSelector` with arrows) and the bodies of the Voice, Portamento,
    Bend and Keyboard sections. Clicking or dragging on the tabs selects one.
    Section contents come in G3+.
  - `editor`: the nice-plug egui editor, 1400 × 820, resizable by the host
    down to 350 × 205 at upstream's fixed aspect ratio.
  - `focus_spike` (`--features focus-spike` only): the FL Studio keyboard
    focus check, see below.
  - Tests: `tests/layout.rs` (exact rectangles against upstream at six
    sizes), `tests/interaction.rs` (egui_kittest clicks on the tabs, no GPU),
    unit tests for skin parsing and lookups. `examples/gallery.rs` renders
    the views in `migration/corpus/gui/views.json` to PNG with wgpu.
- `sas-plugin`: `type Editor = sas_gui::Editor`; `editor::PluginModel` gives
  the editor the preset name (document lock taken only when the document
  generation changes). Feature `focus-spike` forwards to `sas-gui`.
- `migration/reference/gui` (new): ref-gui, upstream's interface built
  unmodified with JUCE's GUI modules (`NO_AUTH=1`, no audio device). Renders
  a preset and tab to PNG through its own `renderOpenGL` read-back, or dumps
  every named component's bounds. Separate CMake project, so the parity
  reference stays headless.
- `migration/scripts/gui_reference.py` (new): `build`, `layout [--check]`,
  `frames --out`; runs under `xvfb-run` when there is no display.
  `migration/corpus/gui/`: `layout_paths.txt`, `views.json` and the layout
  fixtures (`layout/<W>x<H>.json`, upstream geometry, no images).
- CI: new `GUI reference` job in `ci.yml` (fixtures fresh, upstream and port
  frames side by side as the `gui-gallery` artifact). `plugin.yml` also
  builds `synth-a-size-vst3-arm64-macos-focus-check`. `bundle.sh` takes
  `SAS_PLUGIN_FEATURES`.
- `check_names.sh` now also fails on string literals in the editor that name
  upstream (vital, vial, tytel), except paths into the vendored tree.
- Records: `decisions/gui-framework.md` (new); DIVERGENCES.md D14 (product
  name instead of the logo) and D15 (letterboxing at display scales above 1
  in one unit, not upstream's mixed units); IMPROVEMENTS.md I17 (custom
  skins, parked); PLAN.md M10 rewritten for the decision.
- The plugin bundle carries `LICENSE.txt` (GPLv3), the font notices
  (`port/crates/plugin/notices/THIRD_PARTY_NOTICES.md`) and the OFL 1.1 and
  Apache-2.0 texts in `Contents/Resources`.

**Measured**
- Layout: all 22 checked upstream components equal the port's rectangles at
  1400×820, 1050×615, 700×410, 2800×1640, 1400×700 and 1000×820 (the last two
  letterbox), all at display scale 1. A 2800×1640 window at scale 1 lays out
  like a 1400×820 window at 2x; letterboxed windows at 2x differ by design
  (D15).
- Pixels: in the side-by-side gallery the tab labels, highlight strip and
  preset selector sit on the same pixels as upstream's. The text is
  rasterised differently (egui vs JUCE/FreeType), so it is not identical.
  The save and menu icons, volume meter and oscilloscope are not drawn yet.
- Licences: all GPLv3-compatible. The shipped tree (aarch64-apple-darwin,
  normal and build dependencies) has 193 third-party crates, under MIT,
  Apache-2.0, ISC, Zlib, Unlicense, BSD-2-Clause, 0BSD, BlueOak-1.0.0 and
  Unicode-3.0 terms, alone or as alternatives. `self_cell` is "Apache-2.0
  OR GPL-2.0-only" (Apache taken). Dev-only and other-platform crates add
  BSL-1.0, BSD-3-Clause, LGPL alternatives and MPL-2.0 (`colored`, via
  `egui_kittest`). The fonts are OFL 1.1 and Apache-2.0.

**Decisions**
- Review of #93 (GUI Reviewer): tabs now select on button press and follow a
  drag, as `TabSelector::mouseDown` / `mouseDrag` do (tests press without
  releasing, drag across, and click at 1x and 2x). A colour a skin omits is
  transparent black, like a default JUCE `Colour`. The layout test also
  requires every path in `layout_paths.txt` in every fixture.
- Upstream frames show its branding, so they live only in CI artifacts (and
  in owner-facing screenshots with the logo masked), never in git. The layout
  fixtures are plain numbers and are committed.
- egui's `CornerRadius` is whole points, so rounded corners are within half a
  point of upstream's until G2's wgpu primitives draw them exactly.

**FL Studio keyboard-focus spike (needs the owner)**
- egui-baseview passes keys to the host whenever no egui text field wants
  them, so host shortcuts (space bar) should keep working with the editor
  focused.
- The `focus-check` bundle adds a panel with a text field, a switch that asks
  the window for keyboard focus when the field is clicked (egui-baseview's
  `keyboard_focus_workaround`, toggled at run time), and readouts of window
  focus and the last keys received.
- To check in FL Studio: type in the field with the switch off, then on; with
  the field unfocused, the space bar should start FL's transport; resize the
  window from FL. Results go in the G2 ledger entry.

**Blocked / follow-ups**
- A host-less editor window on Linux (Xvfb) never draws a frame (egui's `ui`
  is never called), so there is no windowed Linux check; rendering is checked
  offscreen with wgpu (lavapipe) and the host path on the owner's Mac.
- Not yet ported: upstream's saved window-size factor (`LoadSave::loadWindowSize`),
  dropping files on the editor (egui-baseview does not forward them), the
  logo's About section.
- G3+ will read the synth through G0's `EditHandle` and telemetry once G0
  merges.
