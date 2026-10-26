# Decision: editor = egui with wgpu, upstream's own layout, skin and fonts

Status: accepted as the working default (2026-10-23). The owner approved
starting the GUI on the defaults recorded in the project context; each
choice below is easy to change.

## Choice

- **Framework:** egui 0.37 through nice-plug's adapter `nice-plug-egui`
  0.6, with its `wgpu` backend (wgpu 30: Metal on macOS, Vulkan or GL
  elsewhere). Pinned with `=` in `port/Cargo.toml`.
- **What is ported:** upstream's interface code is the source of truth for
  layout, skin values, colours, fonts and behaviour. Layout code is
  translated line by line in integer pixels, so rectangles match upstream
  exactly at display scale 1 for every window size, and at higher display
  scales when the window keeps the default aspect ratio. A letterboxed window
  at a display scale above 1 follows the port's consistent layout instead of
  upstream's mixed-unit one ([DIVERGENCES.md](../DIVERGENCES.md) D15). The skin is upstream's
  `vital/default.vitalskin`, read at build time (`Skin::from_json` loads any
  other skin file). The fonts are the five faces in `vital/fonts/`.
- **What is new:** the rendering. Upstream paints backgrounds with JUCE's
  software renderer and widgets with about 40 OpenGL shaders
  (`look_and_feel/shaders.cpp`). The port paints with egui; shader-drawn
  widgets get wgpu primitives through egui paint callbacks (G2 onward).
- **Branding:** upstream's logo is replaced by the product name
  ([DIVERGENCES.md](../DIVERGENCES.md) D14). Authentication, download,
  update-check and expiry sections are not ported (rule 4); factory content
  is never shown (rule 3).

## Why

- `nice-plug-egui` is maintained with nice-plug and published on
  crates.io. vizia has no nice-plug adapter on crates.io and depends on
  Skia binaries; iced's retained model fits hundreds of animated,
  custom-drawn controls less well; a renderer of our own would need text,
  input and popups rebuilt.
- OpenGL is deprecated on macOS; wgpu draws with Metal there.
- Every crate added has a GPLv3-compatible licence. The shipped tree
  (aarch64-apple-darwin, normal and build dependencies) has 193 third-party
  crates, under MIT, Apache-2.0, ISC, Zlib, Unlicense, BSD-2-Clause, 0BSD,
  BlueOak-1.0.0 and Unicode-3.0 terms, alone or as alternatives. `self_cell`
  (via egui) is "Apache-2.0 OR GPL-2.0-only", and we take Apache-2.0.
  The rest of the lock file is dev-only and other-platform crates. It adds
  BSL-1.0, BSD-3-Clause and LGPL-2.1-or-later alternatives, and MPL-2.0 for
  `colored` (dev-only, through `egui_kittest`). None of these are shipped.
- The fonts are OFL 1.1 (Lato, Montserrat) and Apache-2.0 (Droid Sans Mono),
  as bundled upstream. The plugin bundle carries their notices and the OFL
  and Apache texts in `Contents/Resources` (`port/crates/plugin/notices/`).

## Verification

- **Layout:** `migration/reference/gui` builds upstream's interface
  unmodified (ref-gui). `migration/scripts/gui_reference.py layout` writes
  upstream's component bounds at six window sizes to
  `migration/corpus/gui/layout/`; `sas-gui`'s `tests/layout.rs` requires the
  port's rectangles to equal them. CI regenerates the fixtures and fails if
  they are stale.
- **Pixels:** the CI job `GUI reference` renders the views in
  `migration/corpus/gui/views.json` from both editors (Xvfb and Mesa) and
  uploads them side by side as the `gui-gallery` artifact. Upstream's frames
  show its branding, so they are never committed.
- **Interaction:** `egui_kittest` drives the editor with simulated input
  (no GPU needed), in the normal `cargo test`.
- **Host:** FL Studio on the owner's Mac, from the arm64 bundle CI builds.
  REAPER on the same Mac is the interim test host until FL Studio is
  installed; it doesn't replace FL Studio as the target.

### FL Studio checklist (owner's Retina Mac)

Checks only the host can answer. Record each answer in the ledger of the
PR that settles it. FL Studio stays the host that settles each check. Until
it is installed, the REAPER notes say which checks can also run in REAPER
and what a REAPER answer settles. Record a REAPER answer as REAPER's, and
repeat the check in FL Studio where the note says so.

1. **Keyboard focus (G1).** Install the `...-focus-check` artifact in place
   of the normal bundle and follow its panel: does the text field receive
   keys, with and without the window asking for focus?
   - *REAPER:* runs there too, but only answers for REAPER: each host
     routes keys to plugins its own way. Try it with the FX window's
     "Send all keyboard input to plug-in" option (its + menu) off, then
     on. FL Studio still needs checking.
2. **Knob drag speed (G2).** Open upstream and the port side by side at
   their default size. In each, start Voice's VEL TRK at its default (0%,
   the middle of its range), drag straight up 50 points (macOS's screenshot
   crosshair, Cmd-Shift-4, reads in points) and note the value the popup
   shows. The port reaches 50% (a quarter of the 200 points a full sweep
   takes). If upstream reaches 100%, its 200 is in physical pixels on
   Retina and the port should halve its drag length at display scale 2.
   Record the answer next to D15 in `DIVERGENCES.md`.
   - *REAPER:* settles it, if both editors run at display scale 2. The
     question is how upstream measures drags against the window's backing
     scale (`getDisplayScale()`), not anything the host does, so a
     side-by-side in REAPER answers D15 (upstream's VST3 is needed there
     too). Precondition: REAPER is not set to "Open in Low Resolution"
     (Finder, Get Info), and both editors' text is crisp, not blurred. In
     low resolution the backing scale is 1, both sides measure in points,
     and the check doesn't settle D15.
3. **A learned knob with the editor open (D16).** MIDI-learn a knob
   (right-click a control, learn, turn a hardware knob), keep the editor
   open, start playback and turn the hardware knob steadily for a few
   seconds:
   - Listen for stepping or zipper noise as the knob moves. The host
     echoes each forwarded value back into the plugin, up to about a frame
     older than the newest MIDI value.
   - Record automation in touch or latch mode while turning: is the curve
     smooth, or broken into segments at the editor's frame rate (one
     gesture per frame)?
   - Undo after turning: one step, or many?
   Record the answers next to D16 in `DIVERGENCES.md`.
   - *REAPER:* gives an interim answer. Arm the track with the hardware
     controller as MIDI input and monitoring on; set the track's automation
     mode to Touch or Latch to record. The zipper question is mostly the
     plugin's, but the echo, the automation curve and undo steps are the
     host's, so repeat all three in FL Studio.
4. **The chat's Keychain prompt (L5).** Turn the chat on, store an API key
   in its settings, quit FL Studio and reopen the project. Does macOS ask
   for Keychain access when the chat reads the key, and does it ask again
   after installing a rebuilt bundle? Ad-hoc signed builds may prompt after
   each rebuild; note whether "Always Allow" sticks.
   - *REAPER:* runs there, but only answers for REAPER. Keychain grants
     access to the app that asks, which is the host, so an "Always Allow"
     given to REAPER says nothing about FL Studio's prompt. Repeat it in FL
     Studio.
5. **Dragging a file into the editor (G5).** Drag an audio file from
   Finder onto oscillator 1's wavetable view. While it hovers, the view
   shows the import overlay (WAVETABLE, VOCODE, PITCH SPLICE) and the
   pointer shows a copy badge; dropping loads it. Then drag from FL
   Studio's browser too. Does FL Studio pass the drag through to the
   plugin's view, or take it itself (for example, to load a sampler)?
   - *REAPER:* runs there too, but only answers for REAPER: each host
     decides whether a drag over a plugin window reaches the plugin. Try a
     drag from Finder and one from REAPER's media explorer. FL Studio still
     needs checking.
6. **Effects tab drags (G6a, G6b).** Turn every effect on (the effect
   order's power buttons), then:
   - drag across the delay's echoes (feedback), the distortion's curve
     (drive) and the filter responses (chorus, delay, distortion, flanger,
     phaser: cutoff or centre across, spread, resonance or feedback up and
     down);
   - drag the compressor's thresholds and ratios, and the EQ's and the
     reverb's band handles;
   - drag an effect in the order list to another place.

   Each drag should be one gesture: one undo step, and with automation
   recording, a smooth curve rather than steps at the editor's frame rate.
   After the reorder, the sections should follow the new order, and saving
   and reopening the project should keep it.
   - *REAPER:* gives an interim answer. Gestures, undo steps and the
     automation curve are the host's, so repeat the check in FL Studio.
7. **The editor opens (check first, after every new bundle).** Insert the
   plugin, open its editor, and check the shader-drawn parts draw (knob
   arcs, the envelope and LFO lines, the oscilloscope); close and reopen it
   and resize the window. A crash on opening was the 2026-10-26 sample-count
   bug. If the editor opens but parts are missing, or anything looks wrong,
   start the host from Terminal with the plugin's log on, for example
   `NICE_LOG=$HOME/Desktop/sas.log /Applications/REAPER.app/Contents/MacOS/REAPER`,
   and send the log: lines with `wgpu error`, `is invalid` or `panicked` are
   errors the editor caught instead of crashing the host.
   - *REAPER:* runs there too, but only answers for REAPER: each host
     embeds the editor's view its own way. Repeat it in FL Studio.

   Also scroll the effects column with the mouse wheel and the trackpad, the
   port and upstream side by side: does a notch, or a short swipe, move it
   as far in both?
   - *REAPER:* settles it if both editors run there. The scroll distance
     comes from the operating system's events and each editor's handling of
     them (JUCE's `Viewport`, egui's smooth scrolling), not from the host.
8. **The file dialog (G6c).** On the Advanced tab, choose TUNING >
   "Load Tuning File..." and pick a `.scl` file (any Scala scale), then open
   it again and cancel.
   - The dialog should open as a sheet on the plugin's window, not the
     host's main window, and the editor should keep drawing behind it (the
     oscilloscope moves while a note plays).
   - Choosing the file should change the tuning's name in the selector and
     the pitch of held notes; cancelling should change nothing.
   - Closing the plugin window while the dialog is open should neither
     crash nor leave the sheet behind.
   - Removing the plugin from the track, or closing the project, while the
     dialog is open should not crash. `rfd` can't cancel the sheet, and its
     completion handler is code in the plugin binary.
   - *REAPER:* settles the sheet and drawing behaviour, which come from
     AppKit and the plugin window rather than the host. FL Studio wraps
     plugins in its own windows, so repeat the parenting check there.

## Vendored egui-baseview

egui paint callbacks (how upstream's shader-drawn quads are drawn, from G2)
need the render target's texture format and sample count to build a
pipeline, and neither egui-wgpu's callback API nor egui-baseview exposes
them. `port/vendor/egui-baseview` is egui-baseview 0.9.0, used through
`[patch.crates-io]`, with small patches listed in its `PATCHES.md`: the
render target in the renderer's callback resources (its sample count at
least 1), a window-close hook, file drag-and-drop (patch 3, which hands
baseview's file drags to egui as hovered and dropped files), and host
safety (patch 4: a wgpu error handler that logs instead of panicking, and
`catch_unwind` around every window callback, so an editor failure never
unwinds into the host). It is excluded from the workspace, so its
code is not held to the port's lints, and patch 3's tests live in
`port/crates/gui/tests/file_drop.rs`. Proposing the patches upstream needs
a human.

## Known limits

- FL Studio may keep keyboard focus from text fields. The G1 focus check
  (`--features focus-spike`) tests it.
- Whether a host passes Finder drags through to the plugin's view is the
  host's choice; checklist item 5 checks it.
- A host-less editor window on Linux (Xvfb) never draws a frame; only the
  embedded (host) path is supported. CI opens it in a minimal VST3 host
  (`vst3-editor-smoke`), in an NSWindow on macOS and an X11 window under
  Xvfb on Linux, and draws every tab on a real device through the window's
  render-state path (`sas-gui`'s `editor_gpu` test, lavapipe and Metal).
