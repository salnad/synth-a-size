## 2026-10-27: GUI G7e: the chat drawer slot

The room for the chat beside upstream's layout: a drawer at the window's
right that the editor widens its window for, built to LLM Builder's L8
requirements (`internal/l8-drawer-requirements.md` in the project store). The
chat panel docks into it in L8; until then it still overlays the editor.

**The drawer** (`port/crates/gui/src/drawer.rs`)

- Its width is the chat panel's 360 points at the editor's scale, clamped
  to 0.5–2 like the chat's text, in whole pixels (`width_pixels`). The scale
  comes from the window's height, which the drawer leaves to upstream's
  layout, so widening the window doesn't change it.
- `split` divides a window between upstream's layout and the drawer. The
  drawer docks, full height at the right, when the window is at least
  upstream's aspect-ratio width for its height plus the drawer. Otherwise
  the editor keeps the whole window, and an open drawer means the overlay.
  The edge is on the pixel grid.
- `adjust_size` keeps room for an open drawer in the sizes the host
  proposes (VST3's `checkSizeConstraint`): upstream's aspect ratio and
  minimum size apply to the rest, and the drawer for the resulting height
  is added back. Closing gives back exactly the width it took.
- `DrawerState` holds the open state, per plugin instance and kept while
  the window is closed, like the window's size, and not in the preset.
  `EditorState` (`default_state`) bundles it with the window size.

**The editor**

- `Interface` owns the open state (`drawer_open`, `set_drawer_open`).
  `show_in` and `chat_placement` take the whole window and lay upstream out
  in the part beside a docked drawer, at its usual size. Nothing is drawn
  in the drawer but the skin's plain background.
- `ChatPlacement::mode` is `ChatMode::Docked(rect)` or `ChatMode::Overlay`.
- There is still one toggle: the chat panel's logo toggle. `show_chat`, one
  frame of the chat as `App::ui` runs it, gives the panel the interface's
  open state and takes back what its toggle did.
- When the drawer opens, the editor asks for a window wider by the drawer,
  through egui's `InnerSize` command, which egui-baseview passes to
  baseview's `WindowContext::resize` and on to the host. When it closes
  docked, the editor asks for a window narrower by the same width. The first
  frame of a window asks nothing, so a reopened window keeps the size the
  instance saved.
- **Refusals.** nice-plug 0.4.2 passes VST3's `resizeView` on without its
  result, and on macOS baseview resizes the editor's view before asking.
  So the editor takes the host setting a size (`onSize`, which reaches
  `EditorHandle::set_size`) as the answer. With no size within a second, it
  asks for the old size back, and an open drawer overlays the editor
  (`PendingResize`).
- Focus: the resize keeps egui's focused widget, and opening the drawer
  asks for no keyboard focus. `Frame` stays where `App::ui` uses it for the
  chat's focus requests.

**Hosts.** REAPER honours a VST3 plugin's `resizeView` on its floating FX
windows and answers with `onSize`. How FL Studio, which wraps plugins in its
own window, handles one on macOS is unchecked here, so FL Studio checklist
item 10 (`decisions/gui-framework.md`) covers docking, giving the width back,
reopening, focus and the refusal fallback.

**Offscreen.** `chat_gallery` writes `chat-drawer.png`: the editor with the
drawer open at 1760 × 820, which L8 can extend to the docked panel.

**Tests**

- `drawer` unit tests: the width at each scale, docking in a widened
  window and not otherwise, pixel alignment at 1.5×, sizes keeping room for
  the drawer and giving its width back, and the host's answer to a request.
- `tests/drawer.rs`:
  - the toggle opens the drawer, and the window size the editor asks for
    widens and gives back exactly;
  - the panel overlays until the window widens, then docks with upstream's
    layout at 1400 × 820;
  - the chat field keeps focus across the resize, and opening asks for no
    focus.
- `tests/controls.rs` `a_docked_drawer_leaves_upstream_its_layout_and_input`:
  in the widened window, upstream's controls answer at their usual
  positions and a drag in the drawer reaches none of them. It fails if
  upstream is laid out across the whole window. The two composed chat tests
  pass unchanged.

No audio change.
