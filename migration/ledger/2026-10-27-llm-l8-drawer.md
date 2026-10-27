## 2026-10-27: LLM L8, the chat docked in the drawer

**Why.** L8 of the LLM plan: the chat panel moves from its overlay into the
right-side drawer that G7e (#167) adds. Stacked on #167, which was built to
L8's requirements (one open state, sized at the chat's text scale, an
overlay fallback, focus kept). Until this lands, an open drawer is an empty
strip.

**Changed**

- `sas-gui` `chat`:
  - `ChatPlacement::panel()` is the drawer's rectangle when the placement is
    `ChatMode::Docked`: the window's full height at its right, beside
    upstream's unchanged layout. It is the overlay rectangle as before when
    the host kept the window's width (`ChatMode::Overlay`).
  - A docked panel is flush with the window's edges (no rounded corners).
    Its text and spacing still follow the chat's text scale, which matches
    the drawer's width.
  - The module docs describe the toggle opening the drawer, docked or
    overlaid.
- `examples/chat_gallery.rs` draws the chat through `show_chat`, as the
  editor does, with the drawer open in every view. So `chat-drawer.png`
  (1760×820) shows the docked conversation, and the other views keep the
  overlay their window allows. The per-view drawer flag is gone; the
  window's width decides.
- `migration/DIVERGENCES.md` D13 and `gui-plan.md` §4 describe the drawer,
  with the overlay as the fallback.

**Measured**

- `cargo test -p sas-gui --test controls`: 90 tests, 1 new.
  - #167's docked-drawer test now also checks that the panel fills the
    drawer, clear of upstream's layout, and that its title, Undo and
    message field draw there.
  - The new test: with the drawer open in an unwidened 1400×820 window,
    the placement is the overlay, inside the editor and below the header,
    360 points wide.
  - Removing the docked branch from `panel()` fails the docked test.
- `chat-drawer.png` and the overlay views render as expected at 2×.
- Clippy `-D warnings` and rustfmt are clean for the workspace and for
  `sas-plugin` on `aarch64-apple-darwin`. The `sas-gui` and `sas-plugin`
  tests pass.

**Blocked:** merges after #167.
