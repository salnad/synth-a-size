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
   does not pass them.

2. `src/window.rs`: `Frame::on_close(callback)`, run when the frame (and so
   the window) is dropped. nice-plug-egui keeps the app across editor
   windows and never tells it a window closed; the editor uses this to end
   host gestures a closing window cut short.

Drop a patch when upstream egui-baseview provides the same. Proposing it
upstream needs a human (migration/AGENTS.md).
