## 2026-10-26: GUI, files dragged onto the editor reach egui (vendored adapter patch)

**Changed.** G1 found that egui-baseview doesn't forward file drops to
egui. baseview 0.4.1 already reports file drags on macOS, X11 and Windows
(`MouseEvent::DragEntered`, `DragMoved`, `DragLeft`, `DragDropped` with
`DropData::Files`). It accepts a drag only when the window handler returns
`EventStatus::AcceptDrop`, and egui-baseview ignored those events, so the
system refused every drop. Patch 3 in
`port/vendor/egui-baseview/PATCHES.md` fixes the adapter:

- `file_drop::handle_drag_event` (new, public) turns the drag events into
  egui's own input. Files over the window go in `RawInput::hovered_files`,
  which egui keeps across frames until the drag leaves or drops. A drop
  goes in `RawInput::dropped_files`, delivered to one frame as
  `file_drop::DroppedPath`, which reads the file on demand. The pointer
  follows the drag in points, and files are answered with
  `AcceptDrop(Copy)`; other drags are refused.
- `EguiWindow::on_event` calls it and returns that answer before its usual
  captured or ignored mapping of mouse events, which would otherwise drop
  it.

Panels read drops through egui as usual (`ctx.input(|i| i.raw.hovered_files
…)`). Uses: G5's wavetable view (upstream's import overlay, then load),
G3b's sample drop, and G7's preset drop.

`decisions/gui-framework.md`:
- New FL Studio checklist item 5: dragging a file from Finder, and from FL
  Studio's browser, into the editor. Whether a host passes such drags
  through to a plugin's view is the host's choice. The REAPER note says a
  REAPER answer only answers for REAPER.
- The vendored-adapter section lists the patches.
- The known limit about file drops is replaced by item 5.

**Verified.**

- `port/crates/gui/tests/file_drop.rs` drives the patch with synthetic
  baseview events (the vendored crate is outside the workspace, so its own
  tests wouldn't run in CI):
  - entering and moving with files accepts as a copy, lists the hovered
    files, and moves the pointer from physical pixels to points;
  - a frame's `take()` keeps the hovered files and clears the events;
  - a drop clears the hovered files, delivers each path once, and reads
    its bytes;
  - leaving clears the hovered files and the pointer;
  - a drag without files is refused, and other events pass through.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, and
  the sas-gui tests pass.

**Host checks.** Checklist item 5 (FL Studio, with a REAPER note).
