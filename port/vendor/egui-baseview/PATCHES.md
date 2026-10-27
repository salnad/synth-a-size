# Local patches

This is egui-baseview 0.9.0 from crates.io (MIT OR Apache-2.0, see
`LICENSE-MIT` and `LICENSE-APACHE`), vendored by synth-a-size and used through
`[patch.crates-io]` in `port/Cargo.toml`. Removed from the published crate:
`examples/`, `.github/`, `Cargo.lock`, `screenshot.png` and the `[[example]]`
entries. Changes to the code, each marked "synth-a-size patch":

1. `src/renderer/wgpu/renderer.rs`: a public `WgpuTarget { format,
   msaa_samples }`, inserted into the egui-wgpu renderer's
   `callback_resources` when the renderer is created, and re-exported from
   the crate root. egui paint callbacks need the target's format and sample
   count to build matching render pipelines, and egui-wgpu's callback API
   does not pass them. `WgpuTarget::new` clamps the sample count to at
   least 1: `RendererOptions::default()` has `msaa_samples: 0`, which
   egui-wgpu and the render pass read as 1, but a pipeline built with 0
   fails validation (the 2026-10-26 crash on opening the editor). The state
   is built by the public `create_render_state`, which `Renderer::new` calls
   and tests call without a window.

2. `src/window.rs`: `Frame::on_close(callback)`, run when the frame (and so
   the window) is dropped. nice-plug-egui keeps the app across editor
   windows and never tells it a window closed; the editor uses this to end
   host gestures a closing window cut short.

3. `src/file_drop.rs` (new, public) and `src/window.rs`: files dragged
   onto the window reach egui. baseview already reports file drags
   (`MouseEvent::DragEntered`, `DragMoved`, `DragLeft`, `DragDropped` with
   `DropData::Files`) and accepts one only when the handler returns
   `EventStatus::AcceptDrop`; egui-baseview ignored them, so drops were
   refused. `file_drop::handle_drag_event` turns them into egui's
   `RawInput::hovered_files` (while files are over the window) and
   `RawInput::dropped_files` (on the drop, as `file_drop::DroppedPath`,
   read on demand), moves the pointer with the drag, and answers
   `AcceptDrop(Copy)` for files. `on_event` calls it and returns that
   answer before its usual captured/ignored mapping of mouse events.
   Tested from `port/crates/gui/tests/file_drop.rs`, since this crate is
   outside the workspace.

4. Host safety, in `src/renderer/wgpu/renderer.rs` and `src/window.rs`:
   - `create_render_state` installs a device `on_uncaptured_error` handler
     that logs (the first few) and counts errors in a public `WgpuErrors`,
     inserted into `callback_resources`, instead of wgpu's default handler,
     which panics. Paint callbacks read the count to stop drawing.
   - The renderer's other panics (an unsupported surface, a surface that
     fails validation or cannot be recreated) log and skip the frame.
   - Every `WindowHandler` callback (`draw`, `on_event`, `resized`, `poll`,
     `on_timer`) runs under `catch_unwind`. baseview calls them from
     platform callbacks (`extern "C-unwind"` AppKit methods), so a panic
     would unwind into the host and abort it. After a panic the window
     logs it and stops drawing and handling input. Creating the window
     (the renderer and `App::build`) is guarded the same way and fails the
     window instead, and so is each `Frame::on_close` callback.
   - `Frame::on_failure(callback)` runs once when a callback panics, so
     the app can end what the dead window leaves open (host gestures).
   - The window's inner state (app, egui context, frame and renderer) is
     a `ManuallyDrop` dropped under `catch_unwind` in `EguiWindow`'s
     `Drop`: baseview drops the handler from AppKit callbacks or the
     host's `removed()` on macOS, and on the X11 window thread, whose
     panic the host's thread re-raises when it joins.
   - The renderer logs once, at info level, when it presents its first
     frame; CI's editor-open check (`scripts/editor_smoke.sh` in
     `sas-plugin`) looks for that line.
   Tested from `port/crates/gui/tests/editor_gpu.rs` and the
   `vst3-editor-smoke` host in CI.

Drop a patch when upstream egui-baseview provides the same. Proposing it
upstream needs a human (migration/AGENTS.md).
