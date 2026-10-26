## 2026-10-26: Editor panic guards, review follow-ups to #138

**Changed.** Graph Reviewer's three non-blocking items and UI Reviewer's two
nits on the editor crash fix (#138):

- *Window teardown:* `EguiWindow` keeps its inner state (app, egui context,
  frame, renderer) in a `ManuallyDrop` and drops it under `catch_unwind`.
  baseview drops the handler from AppKit callbacks or the host's
  `removed()` on macOS, and on Linux from the X11 window thread, whose panic
  baseview re-raises in the host's thread when it joins (patch 4 in
  `PATCHES.md`).
- *Editor API:* `sas_gui::Editor` is now a thin wrapper around
  nice-plug-egui's editor rather than an alias for it. Every method nice-plug
  calls from the host's plugin API runs under `catch_unwind`:
  - on the editor: `spawn` (nice-plug-egui's setup and baseview's window
    creation before `build`), `size`, `resize_hint` and
    `track_info_updated`;
  - on its `EditorHandle`: `show`, `set_size`, the parameter callbacks and
    the rest.

  Its window type, `EditorWindow`, drops the baseview window (closing it)
  under `catch_unwind` too. A panic logs and falls back: `spawn` fails, the
  default size is used, and a call becomes a no-op or returns an error.
- *Gestures:* egui-baseview's new `Frame::on_failure` runs when a window
  callback panics. The editor then ends its open host gestures and releases
  the on-screen keyboard's note at once, instead of waiting for the host to
  close the dead window.
- *Nits:* `editor_smoke.sh` greps for `pipeline is invalid` instead of
  `is invalid`. `validated` logs a warning on the `Pending` branch, which
  native wgpu never takes.

**Verified locally (Linux, lavapipe, Xvfb).** `cargo test -p sas-gui
--features gpu-tests` passes; clippy `-D warnings` is clean; the editor
smoke on the dev bundle passes through the wrapper. In a probe build (not
committed) with a panic injected in the editor's `ui` and another in a close
callback, the host exited 0. The log showed the draw callback's panic
caught, the failure hook run, and the close-time panic caught.
