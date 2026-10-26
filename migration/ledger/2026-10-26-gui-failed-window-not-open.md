## 2026-10-26: A failed editor window stops counting as open

**Changed.** This is UI Reviewer's non-blocking note on #140. After a panic,
the inert window kept the model's open-editor count
(`EditorModel::set_editor_open(true)`) until the host closed it. While the
count is above zero, the plugin keeps the controls learned MIDI controllers
move for the editor to forward to the host, but the inert window no longer
drains or forwards them.

Each window now holds an `OpenWindow` share of the count (in
`sas_gui::editor`). It is taken at the end of `build` and given back once:
by `Frame::on_failure` when the window fails, or at close, whichever comes
first. The close callback keeps main's order: clear the file dialogs'
parent, give back the share, then end gestures, commit the pending
connection and release the keyboard note.

**Verified.** New unit tests in `sas_gui::editor`. A failure followed by the
close gives back the count once (`[true, false]`). A close before the window
opened gives back nothing, and a second `open` takes nothing more. The
plugin's count handling (`SynthParams::set_editor_open`, saturating at 0)
is already covered by `crates/plugin/tests/midi_learn.rs`. Clippy
`-D warnings` is clean.
