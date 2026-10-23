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
  exactly at every window size. The skin is upstream's
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
- The licences of every crate added (318 in the tree) are MIT, Apache-2.0,
  ISC, Zlib, BSL-1.0 or Unlicense, all GPLv3-compatible. The fonts are
  Apache-2.0 / OFL, as bundled upstream.

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

## Known limits

- egui-baseview does not forward file drag-and-drop to egui yet, and FL
  Studio may keep keyboard focus from text fields. The G1 focus check
  (`--features focus-spike`) tests the second.
- A host-less editor window on Linux (Xvfb) never draws a frame; only the
  embedded (host) path is supported, and it is checked on macOS.
