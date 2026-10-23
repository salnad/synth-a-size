## 2026-10-23: LLM follow-up, renaming the patch without a reload

**Why.** L4 (#100) renamed the patch (`set_preset_info`) by reloading the
whole state through the host, because the edit bus had no save-info edit.
That reload also replaced the undo history's baseline and the
outside-change tracking. #104 added `EditHandle::set_info`, a mirror-only
edit, so the live path now uses it, as planned in the L4 ledger.

**Changed**

- `sas-plugin` `live::LiveSession`:
  - `Edit::SetInfo` calls `EditHandle::set_info`. The edit bus's mirror
    (which the host saves and the editor's preset name reads) changes, and
    nothing reaches the engine, which never reads the save info. The
    generation doesn't change, so it is not a replacement.
  - `restore` (undo and A/B) no longer reloads when only the save info
    differs. It applies the target's info with `set_info`, after the other
    edits. A different routing or sample still reloads the whole state.

**Measured**

- `cargo test -p sas-plugin --test live_session`: 15 tests, 1 new. Naming
  the patch sets the name, author and a macro label with no reload and no
  generation change. The undo history stays, and the host's saved state
  carries the new name. Undoing the turn restores the old name and the
  control, still without a reload.
- Clippy `-D warnings` and rustfmt are clean for the workspace and for
  `sas-plugin` on `aarch64-apple-darwin`. The plugin and `sas-llm-control`
  tests pass, apart from the two live-edit parity tests that need
  reference captures (CI renders them).

**Blocked:** nothing.
