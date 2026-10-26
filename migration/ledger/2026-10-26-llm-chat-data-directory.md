## 2026-10-26: LLM follow-up, the chat's preset tools follow the data directory

**Why.** #148 (G7a) put the user folders in `sas_preset::user_folders` and
let the config's `data_directory` move them, as upstream's
`LoadSave::getDataDirectory` does. The editor's preset browser reads that
setting, but the chat's preset tools fell back to the default root only.
With a data directory set, the two used different Presets folders, against
D24's single shared location. Raised on #148; the coordinator asked for
this follow-up.

**Changed**

- `sas-plugin` `chat::PluginChat`:
  - The preset tools' folder is the chat settings' `preset_dir`, else the
    Presets folder under `user_folders::root(data_directory)`. That is the
    config's `data_directory` if it is an existing folder, else
    `~/Music/synth-a-size`, the folder the editor's browser uses.
  - `data_directory` is read from the plugin's config file
    (`config::config_file()`) when a worker starts, so a changed data
    directory applies from the next worker.
  - `PluginChat::new` uses the plugin's config file. `with_config_file`
    supplies one, for tests. `in_memory` and `with` have none, so they use
    the default root as before.

**Measured**

- `cargo test -p sas-plugin --test chat`: 19 tests, 2 new.
  - With a configured data directory, the model's `save_preset` lands in
    `<data_directory>/Presets`. After the config moves to another folder
    and a settings change starts a new worker, the next save lands in the
    new folder and not the old one.
  - The chat's own `preset_dir` wins over the data directory. A configured
    folder that no longer exists falls back to the default root, as the
    browser does.
  - Reading the folder from the chat settings alone, the old behaviour,
    fails the first test.
- Clippy `-D warnings` and rustfmt are clean for the workspace and for
  `sas-plugin` on `aarch64-apple-darwin`. All `sas-plugin` tests pass.

**Blocked:** nothing.
