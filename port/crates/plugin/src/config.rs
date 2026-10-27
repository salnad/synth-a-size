// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_data_structures, GPL-3.0-only).
// SPDX-License-Identifier: GPL-3.0-or-later AND GPL-3.0-only

//! The user config file: the MIDI learn part of `LoadSave`
//! (`vital/src/common/load_save.cpp`): `getConfigFile`, `getConfigJson`,
//! `saveJsonToConfig`, `saveMidiMapConfig` and the MIDI map in `loadConfig`.
//!
//! The file is one JSON object shared by every instance. Saving the MIDI map
//! rewrites its `"midi_learn"` key and keeps the rest:
//!
//! ```json
//! {"midi_learn":[{"destinations":[{"destination":"filter_1_cutoff","max_range":136.0,"min_range":8.0}],"source":20}]}
//! ```

use std::path::{Path, PathBuf};

use sas_common::json::{self, Value};
use sas_engine::midi_manager::MidiLearnMap;
use sas_preset::lookup;

/// The product name the config folder and file are named after.
const PRODUCT_NAME: &str = "synth-a-size";

/// `getConfigFile()`: JUCE's `PropertiesFile::Options::getDefaultFile` for
/// the product, with the suffix `config`. `None` when the home or
/// application data folder is unknown.
pub fn config_file() -> Option<PathBuf> {
    let file = format!("{PRODUCT_NAME}.config");
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/Application Support")
                .join(PRODUCT_NAME)
                .join(file),
        )
    } else if cfg!(windows) {
        let app_data = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(app_data).join(PRODUCT_NAME).join(file))
    } else {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join(format!(".{PRODUCT_NAME}"))
                .join(file),
        )
    }
}

/// `getConfigJson()`: the file's JSON, or `null` when it is missing or does
/// not parse.
pub fn read_config(path: &Path) -> Value {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| json::parse_bytes(&bytes).ok())
        .unwrap_or(Value::Null)
}

/// `saveJsonToConfig(state)`: creates the file (and its folder) if needed
/// and replaces its text with the compact dump.
pub fn save_json_to_config(path: &Path, state: &Value) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(path, state.dump())
}

/// The `"midi_learn"` value `saveMidiMapConfig` writes: one entry per
/// controller in ascending order, its destinations by name with each
/// parameter's range. A controller whose parameters were all cleared keeps
/// an entry with `null` destinations, and an empty map is `null`.
pub fn midi_learn_to_json(midi_learn_map: &MidiLearnMap) -> Value {
    let mut midi_mapping_data = Value::Null;
    for (source, destinations) in midi_learn_map {
        let mut midi_map_data = Value::Null;
        midi_map_data.set("source", *source);
        let mut destinations_data = Value::Null;
        for (name, details) in destinations {
            let mut destination_data = Value::Null;
            destination_data.set("destination", *name);
            destination_data.set("min_range", details.min);
            destination_data.set("max_range", details.max);
            destinations_data.push(destination_data);
        }
        midi_map_data.set("destinations", destinations_data);
        midi_mapping_data.push(midi_map_data);
    }
    midi_mapping_data
}

/// The MIDI map `loadConfig` reads from config `data`. Entries upstream
/// cannot read (a source that is not a number, a destination that is not a
/// string or not a parameter) are skipped, where upstream throws or has no
/// details to store.
pub fn midi_learn_from_config(data: &Value) -> MidiLearnMap {
    let mut midi_learn_map = MidiLearnMap::new();
    let Ok(midi_mapping_data) = data.get("midi_learn") else {
        return midi_learn_map;
    };
    for midi_map_data in midi_mapping_data.iter_values() {
        let Some(source) = midi_map_data
            .get("source")
            .ok()
            .and_then(|source| source.to_i32().ok())
        else {
            continue;
        };
        let Ok(destinations_data) = midi_map_data.get("destinations") else {
            continue;
        };
        for midi_destination in destinations_data.iter_values() {
            let details = midi_destination
                .get("destination")
                .ok()
                .and_then(|name| name.to_str().ok())
                .and_then(lookup);
            if let Some(details) = details {
                midi_learn_map
                    .entry(source)
                    .or_default()
                    .insert(details.name.as_str(), details);
            }
        }
    }
    midi_learn_map
}

/// `saveMidiMapConfig(midi_manager)`: rewrites the config's `"midi_learn"`
/// key from `midi_learn_map`, keeping every other key.
pub fn save_midi_map_config(path: &Path, midi_learn_map: &MidiLearnMap) -> std::io::Result<()> {
    let mut data = read_config(path);
    if !data.is_null() && !data.is_object() {
        // Upstream's `data["midi_learn"]` throws on a config that is not an
        // object; start a fresh one instead.
        data = Value::Null;
    }
    data.set("midi_learn", midi_learn_to_json(midi_learn_map));
    save_json_to_config(path, &data)
}

/// `LoadSave::displayHzFrequency()`: whether cutoff sliders show hertz
/// (the config's `"hz_frequency"`, false when missing).
pub fn display_hz_frequency(path: &Path) -> bool {
    read_config(path)
        .get("hz_frequency")
        .ok()
        .and_then(|value| value.to_bool().ok())
        .unwrap_or(false)
}

/// `LoadSave::saveDisplayHzFrequency(hz_frequency)`: rewrites the config's
/// `"hz_frequency"` key, keeping every other key.
pub fn save_display_hz_frequency(path: &Path, hz_frequency: bool) -> std::io::Result<()> {
    let mut data = read_config(path);
    if !data.is_null() && !data.is_object() {
        data = Value::Null;
    }
    data.set("hz_frequency", hz_frequency);
    save_json_to_config(path, &data)
}

/// `LoadSave::getDataDirectory`'s setting: the config's `"data_directory"`,
/// if set.
pub fn data_directory(path: &Path) -> Option<PathBuf> {
    read_config(path)
        .get("data_directory")
        .ok()
        .and_then(|value| value.to_str().ok().map(PathBuf::from))
}

/// `LoadSave::setDataDirectory`: rewrites the config's `"data_directory"`,
/// keeping every other key.
pub fn save_data_directory(path: &Path, folder: &Path) -> std::io::Result<()> {
    let mut data = read_config(path);
    if !data.is_null() && !data.is_object() {
        data = Value::Null;
    }
    data.set("data_directory", folder.to_string_lossy().into_owned());
    save_json_to_config(path, &data)
}

/// `LoadSave::loadWindowSize`'s `kMinWindowSize`.
pub const MIN_WINDOW_SIZE: f32 = 0.25;

/// `LoadSave::loadWindowSize`: the config's `"window_size"`, the editor's
/// width as a share of 1400, at least [`MIN_WINDOW_SIZE`]; 1 if unset.
pub fn window_size(path: &Path) -> f32 {
    match read_config(path).get("window_size").map(Value::to_f64) {
        Ok(Ok(size)) => (size as f32).max(MIN_WINDOW_SIZE),
        _ => 1.0,
    }
}

/// `LoadSave::saveWindowSize`: rewrites the config's `"window_size"`,
/// keeping every other key.
pub fn save_window_size(path: &Path, window_size: f32) -> std::io::Result<()> {
    let mut data = read_config(path);
    if !data.is_null() && !data.is_object() {
        data = Value::Null;
    }
    data.set("window_size", f64::from(window_size));
    save_json_to_config(path, &data)
}

/// `LoadSave::getAuthor`: the config's `"author"`, empty if unset.
pub fn author(path: &Path) -> String {
    read_config(path)
        .get("author")
        .ok()
        .and_then(|value| value.to_str().ok().map(str::to_string))
        .unwrap_or_default()
}

/// `LoadSave::saveAuthor`: rewrites the config's `"author"`, keeping every
/// other key.
pub fn save_author(path: &Path, author: &str) -> std::io::Result<()> {
    let mut data = read_config(path);
    if !data.is_null() && !data.is_object() {
        data = Value::Null;
    }
    data.set("author", author.to_string());
    save_json_to_config(path, &data)
}

/// `LoadSave::kAdditionalWavetableFoldersName` and
/// `kAdditionalSampleFoldersName`: the config keys of the folders added to
/// the wavetable and sample browsers.
pub const WAVETABLE_FOLDERS: &str = "wavetable_folders";
pub const SAMPLE_FOLDERS: &str = "sample_folders";

/// `LoadSave::getAdditionalFolders`: the folders under `name`, in the order
/// they were added.
pub fn additional_folders(path: &Path, name: &str) -> Vec<PathBuf> {
    match read_config(path).get(name) {
        Ok(Value::Array(folders)) => folders
            .iter()
            .filter_map(|folder| folder.to_str().ok().map(PathBuf::from))
            .collect(),
        _ => Vec::new(),
    }
}

/// `LoadSave::saveAdditionalFolders`: rewrites the config's `name` with
/// `folders`, keeping every other key.
pub fn save_additional_folders(
    path: &Path,
    name: &str,
    folders: &[PathBuf],
) -> std::io::Result<()> {
    let mut data = read_config(path);
    if !data.is_null() && !data.is_object() {
        data = Value::Null;
    }
    let folders = folders
        .iter()
        .map(|folder| Value::from(folder.to_string_lossy().into_owned()))
        .collect();
    data.set(name, Value::Array(folders));
    save_json_to_config(path, &data)
}

/// `LoadSave::getFavoritesFile`: beside the config file, with the
/// `favorites` extension.
pub fn favourites_file(config: &Path) -> PathBuf {
    config.with_extension("favorites")
}

/// `LoadSave::getFavorites`: the favourite files' full paths, the keys of
/// the favourites file's object.
pub fn favourites(path: &Path) -> Vec<PathBuf> {
    match read_config(path) {
        Value::Object(entries) => entries.keys().map(PathBuf::from).collect(),
        _ => Vec::new(),
    }
}

/// `LoadSave::addFavorite` / `removeFavorite`: rewrites the favourites
/// file with `file` in or out (`{"<path>": 1, ...}`).
pub fn save_favourite(path: &Path, file: &Path, favourite: bool) -> std::io::Result<()> {
    let mut data = match read_config(path) {
        Value::Object(entries) => entries,
        _ => Default::default(),
    };
    let key = file.to_string_lossy().into_owned();
    if favourite {
        data.insert(key, Value::from(1));
    } else if data.remove(&key).is_none() {
        return Ok(());
    }
    save_json_to_config(path, &Value::Object(data))
}

/// The MIDI map part of `loadConfig`: the map saved in the config file, empty
/// if there is none.
pub fn load_midi_map_config(path: &Path) -> MidiLearnMap {
    midi_learn_from_config(&read_config(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(i32, &[&str])]) -> MidiLearnMap {
        let mut map = MidiLearnMap::new();
        for (source, names) in entries {
            let destinations = map.entry(*source).or_default();
            for name in *names {
                let details = lookup(name).expect("parameter");
                destinations.insert(details.name.as_str(), details);
            }
        }
        map
    }

    fn temp_config(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sas-config-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join("test.config")
    }

    #[test]
    fn midi_map_json_matches_upstreams_schema() {
        let learned = map(&[
            (20, &["filter_1_cutoff"]),
            (7, &["macro_control_2", "macro_control_1"]),
        ]);
        assert_eq!(
            midi_learn_to_json(&learned).dump(),
            r#"[{"destinations":[{"destination":"macro_control_1","max_range":1.0,"min_range":0.0},{"destination":"macro_control_2","max_range":1.0,"min_range":0.0}],"source":7},{"destinations":[{"destination":"filter_1_cutoff","max_range":136.0,"min_range":8.0}],"source":20}]"#
        );
        assert!(midi_learn_to_json(&MidiLearnMap::new()).is_null());
    }

    #[test]
    fn a_cleared_controller_saves_null_destinations_and_does_not_reload() {
        let mut learned = map(&[(3, &["macro_control_1"]), (4, &["macro_control_2"])]);
        learned.get_mut(&3).unwrap().clear();
        let saved = midi_learn_to_json(&learned);
        assert_eq!(
            saved.dump(),
            r#"[{"destinations":null,"source":3},{"destinations":[{"destination":"macro_control_2","max_range":1.0,"min_range":0.0}],"source":4}]"#
        );
        let mut data = Value::Null;
        data.set("midi_learn", saved);
        assert_eq!(
            midi_learn_from_config(&data),
            map(&[(4, &["macro_control_2"])])
        );
    }

    #[test]
    fn saving_keeps_other_keys_and_round_trips() {
        let path = temp_config("round-trip");
        let mut existing = Value::Null;
        existing.set("author", "someone");
        save_json_to_config(&path, &existing).unwrap();

        let learned = map(&[
            (20, &["filter_1_cutoff", "osc_1_level"]),
            (64, &["macro_control_1"]),
        ]);
        save_midi_map_config(&path, &learned).unwrap();
        let data = read_config(&path);
        assert_eq!(data.get("author").unwrap().to_str().unwrap(), "someone");
        assert_eq!(load_midi_map_config(&path), learned);
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn unreadable_configs_and_entries_load_as_empty_or_skip() {
        let path = temp_config("unreadable");
        assert!(load_midi_map_config(&path).is_empty());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json").unwrap();
        assert!(read_config(&path).is_null());
        assert!(load_midi_map_config(&path).is_empty());

        let text = r#"{"midi_learn":[{"source":"x","destinations":[{"destination":"osc_1_level"}]},
            {"source":5,"destinations":[{"destination":"not_a_parameter"},{"destination":3},{"destination":"osc_1_level"}]},
            {"source":6}]}"#;
        std::fs::write(&path, text).unwrap();
        assert_eq!(load_midi_map_config(&path), map(&[(5, &["osc_1_level"])]));

        // A config that is not an object is replaced rather than thrown on.
        std::fs::write(&path, "[1, 2]").unwrap();
        save_midi_map_config(&path, &map(&[(1, &["osc_1_level"])])).unwrap();
        assert_eq!(load_midi_map_config(&path), map(&[(1, &["osc_1_level"])]));
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn the_config_file_is_named_after_the_product() {
        if let Some(path) = config_file() {
            assert_eq!(path.file_name().unwrap(), "synth-a-size.config");
            let text = path.to_string_lossy().to_lowercase();
            assert!(!text.contains("vital") && !text.contains("vial"), "{text}");
        }
    }

    /// The frequency display setting round-trips through the config file,
    /// false when missing, and saving it keeps the MIDI map beside it.
    #[test]
    fn the_hz_frequency_setting_round_trips() {
        let folder = std::env::temp_dir().join(format!("sas-hz-{}", std::process::id()));
        let path = folder.join("test.config");
        let _ = std::fs::remove_file(&path);
        assert!(!display_hz_frequency(&path));
        save_midi_map_config(&path, &map(&[(7, &["volume"])])).unwrap();
        save_display_hz_frequency(&path, true).unwrap();
        assert!(display_hz_frequency(&path));
        assert_eq!(load_midi_map_config(&path).len(), 1);
        save_display_hz_frequency(&path, false).unwrap();
        assert!(!display_hz_frequency(&path));
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The data directory round-trips through the config file beside the
    /// other settings.
    #[test]
    fn the_data_directory_round_trips() {
        let folder = std::env::temp_dir().join(format!("sas-data-{}", std::process::id()));
        let path = folder.join("test.config");
        let _ = std::fs::remove_file(&path);
        assert_eq!(data_directory(&path), None);
        save_display_hz_frequency(&path, true).unwrap();
        save_data_directory(&path, Path::new("/tmp/my presets")).unwrap();
        assert_eq!(
            data_directory(&path),
            Some(PathBuf::from("/tmp/my presets"))
        );
        assert!(display_hz_frequency(&path));
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// Favourites are the keys of an object in their own file beside the
    /// config, added and removed one at a time.
    #[test]
    fn additional_folders_round_trip() {
        let dir = std::env::temp_dir().join(format!("sas-config-folders-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        save_author(&path, "Someone").unwrap();
        assert!(additional_folders(&path, WAVETABLE_FOLDERS).is_empty());
        let folders = [PathBuf::from("/b/Tables"), PathBuf::from("/a/More")];
        save_additional_folders(&path, WAVETABLE_FOLDERS, &folders).unwrap();
        assert_eq!(additional_folders(&path, WAVETABLE_FOLDERS), folders);
        assert!(additional_folders(&path, SAMPLE_FOLDERS).is_empty());
        assert_eq!(author(&path), "Someone");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn window_size_round_trip() {
        let dir = std::env::temp_dir().join(format!("sas-config-window-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        assert_eq!(window_size(&path), 1.0);
        save_author(&path, "Someone").unwrap();
        save_window_size(&path, 1.5).unwrap();
        assert_eq!(window_size(&path), 1.5);
        assert_eq!(author(&path), "Someone");
        save_window_size(&path, 0.1).unwrap();
        assert_eq!(window_size(&path), MIN_WINDOW_SIZE);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn favourites_round_trip() {
        let folder = std::env::temp_dir().join(format!("sas-favourites-{}", std::process::id()));
        let path = favourites_file(&folder.join("test.config"));
        let _ = std::fs::remove_file(&path);
        assert_eq!(path.extension().unwrap(), "favorites");
        assert!(favourites(&path).is_empty());
        save_favourite(&path, Path::new("/presets/b.vital"), true).unwrap();
        save_favourite(&path, Path::new("/presets/a.vital"), true).unwrap();
        save_favourite(&path, Path::new("/presets/b.vital"), false).unwrap();
        assert_eq!(favourites(&path), [PathBuf::from("/presets/a.vital")]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The author saved with round-trips through the config file.
    #[test]
    fn the_author_round_trips() {
        let folder = std::env::temp_dir().join(format!("sas-author-{}", std::process::id()));
        let path = folder.join("test.config");
        let _ = std::fs::remove_file(&path);
        assert_eq!(author(&path), "");
        save_author(&path, "Someone").unwrap();
        assert_eq!(author(&path), "Someone");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
