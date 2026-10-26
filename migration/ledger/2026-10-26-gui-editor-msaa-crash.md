## 2026-10-26: Editor crash on opening (MSAA sample count 0), host safety, editor-open CI

**Problem.** Salil's local build of main crashed REAPER 7.82 (macOS 26.6.2,
M4 Max) as soon as the editor opened:
`Device::create_render_pipeline, label = 'sas-gui lines': Invalid sample
count 0`, then `libc++abi: terminating due to uncaught foreign exception`.

**Root cause.** egui-wgpu 0.37's `RendererOptions::default()` has
`msaa_samples: 0`, meaning no multisampling. egui-wgpu clamps it with
`.max(1)` wherever it builds a pipeline or texture, and egui-baseview's
render pass is single-sampled for 0. Our egui-baseview patch 1 published the
raw value in `WgpuTarget`, so with nice-plug-egui's default
`GraphicsConfig` sas-gui's custom pipelines (`sas-gui quads`, which also
draw the bars, and `sas-gui lines`, which draw every line including the
wavetable views) were built with a sample count of 0. wgpu's default
uncaptured-error handler panics. baseview calls the window handler from
`extern "C-unwind"` AppKit methods, so the panic unwound into AppKit as a
foreign exception, and libc++abi terminated the host.

It fails on every real device, not only an M4. CI missed it because no path
ever built the window's render state: the GPU tests, the gallery and the
kittest renderers all called `sas_gui::primitives::publish_target(state, 1)`
with `RendererOptions::PREDICTABLE` (whose `msaa_samples` is 1), and no CI
step created the editor view. The macOS VST3 job only bundled the plugin,
ran the validator and played a note, and the decision doc's "checked on
macOS" for the embedded editor was not backed by any check. Building the
Linux dev bundle without the clamp reproduces the exact error under Xvfb
with lavapipe.

**Changed.**
- *Fix:* `WgpuTarget::new` clamps the sample count to at least 1, as do the
  egui-baseview renderer, `publish_target` and sas-gui's `target()`. The
  window's state comes from the new public
  `egui_baseview::create_render_state`, which `Renderer::new` calls and
  tests can call without a window (patch 1 in `PATCHES.md`).
- *Host safety (patch 4):* `create_render_state` installs a device
  `on_uncaptured_error` handler that logs the first eight errors and counts
  them all (`WgpuErrors`), in place of wgpu's panicking default. sas-gui builds
  its pipelines in a validation error scope. A rejected pipeline is logged
  and draws nothing, and once the device reports any error the custom
  primitives stop drawing while the rest of egui carries on. Every
  `WindowHandler` callback, window creation (renderer and `App::build`) and
  each `Frame::on_close` callback runs under `catch_unwind`. After a panic
  the window logs it and stops drawing and handling input; the host keeps
  running, and reopening the editor starts over. The renderer's remaining
  panics (`expect`/`unwrap`/`unreachable!` on surface errors) log and skip
  the frame. `Plugin::editor()` (inside the host's `createView`) is guarded
  too and returns no editor on a panic.
- *Logging:* the workspace built nice-plug without default features, so its
  logger was off and every `nice_*!` and tracing line was dropped.
  `sas-plugin` now enables nice-plug's `tracing-subscriber` feature: logs and
  panics go to STDERR, or wherever `NICE_LOG` points. This adds `tracing-log` and
  `valuable` (both MIT) to the lockfile.
- *Tests:* `port/crates/gui/tests/editor_gpu.rs` (`gpu-tests`) builds the
  render state through `create_render_state` with `GraphicsConfig::default()`
  and checks three things. The published target is single-sampled. Every tab
  of the real `Interface` draws in a validation error scope with no device
  errors, and the quad and line pipelines are built and valid. A deliberate
  sample-count-0 pipeline is counted and logged instead of panicking. With
  the clamp removed, the first two tests fail.
- *Editor-open CI:* `vst3-editor-smoke` (in `port/crates/plugin/vst3-smoke`)
  is a minimal VST3 host that creates the editor view, attaches it to a real
  window (an NSWindow on macOS, an X11 window on Linux), lets it draw for
  5 s, and removes it. `scripts/editor_smoke.sh` requires the renderer's
  "presented the first frame" log line and fails on `panicked`, `wgpu error`
  or `is invalid`. It runs in plugin.yml's macOS VST3 job (Metal) and its
  gating Linux dev job (under Xvfb, with lavapipe). The macOS job also runs
  `editor_gpu` on Metal; the GUI reference job already runs every
  `gpu-tests` test on lavapipe.
- *Checklist:* `decisions/gui-framework.md` gains item 7: open the editor in
  REAPER and in FL Studio, and how to capture the log if anything is
  missing. Its known limits now name the CI checks.

**Verified locally (Linux, lavapipe).** `cargo test -p sas-gui --features
gpu-tests` and `cargo test -p sas-plugin` pass; clippy `-D warnings` is
clean. `editor_gpu` fails with the clamp removed. Under Xvfb, the editor
smoke on the dev bundle passes, logging `presented the first frame
(1400x820 px, Bgra8Unorm, 1 sample(s), 0 device error(s))`. With the clamp
removed, the host survives, the log shows `Invalid sample count 0` for
`sas-gui lines` and `sas-gui quads`, and the check fails. macOS (Metal) is
checked by this PR's plugin.yml run.

**Open.** Salil: open the editor in REAPER on the M4 Max with this PR's
bundle (checklist item 7), and in FL Studio once it is installed. The
`gpu.rs` pixel test still runs only on lavapipe.
