# Local patches

This is nice-plug-egui 0.6.0 from crates.io (ISC, authors Billy Messenger
and Robbert van der Helm, https://codeberg.org/RustAudio/nice-plug at
faa0cd61d249bf7c5103f3072be0c65cf72e9f4e, `crates/nice-plug-egui`),
vendored by synth-a-size and used through `[patch.crates-io]` in
`port/Cargo.toml`. Its `Cargo.toml` is the one crates.io publishes.
Removed from the published crate: `Cargo.lock`, `Cargo.toml.orig` (which
inherits from nice-plug's workspace) and Cargo's `.cargo-ok` and
`.cargo_vcs_info.json`.

The published crate carries no licence file. `LICENSE` is the ISC licence
of nih-plug (https://github.com/robbert-vdh/nih-plug), which nice-plug is a
fork of and whose `nih_plug_egui` this crate continues; nice-plug's own
repository couldn't be reached to copy its file.

Changes to the code, each marked "synth-a-size patch":

1. `src/lib.rs`: `EguiEditorState::set_size`, which stores the size the
   next window opens at. nice-plug builds a plugin's editor once and opens
   each window at the state's size, which only the window itself changed;
   the editor sets it from the size saved in the config before each window
   opens, as upstream's editor does every time it is built
   (`LoadSave::loadWindowSize`).
