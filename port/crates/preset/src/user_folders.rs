// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_core, ISC).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's folders for presets, wavetables, samples and LFOs, shared by
//! the editor's browsers and the chat's preset tools. Upstream keeps them
//! under its data directory (`LoadSave::getDataDirectory`, which the config's
//! `"data_directory"` can move); the port's default root is
//! `~/Music/synth-a-size`, with one folder per kind directly inside it.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// The preset file extension (upstream's `kPresetExtension`), kept for
/// compatibility with existing files.
pub const PRESET_EXTENSION: &str = "vital";
/// `kWavetableExtension`.
pub const WAVETABLE_EXTENSION: &str = "vitaltable";
/// `kLfoExtension`.
pub const LFO_EXTENSION: &str = "vitallfo";
/// `kBankExtension`.
pub const BANK_EXTENSION: &str = "vitalbank";

/// The folder under the user's music folder that holds the rest.
pub const PRODUCT_FOLDER: &str = "synth-a-size";

/// What a user folder holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Presets,
    Wavetables,
    Samples,
    Lfos,
}

impl Kind {
    /// `kPresetFolderName` and the like.
    pub fn folder_name(self) -> &'static str {
        match self {
            Kind::Presets => "Presets",
            Kind::Wavetables => "Wavetables",
            Kind::Samples => "Samples",
            Kind::Lfos => "LFOs",
        }
    }
}

/// The default root: `~/Music/synth-a-size` (the user profile's on
/// Windows). `None` without a home folder.
pub fn default_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join("Music").join(PRODUCT_FOLDER))
}

/// `LoadSave::getDataDirectory`: the configured root if it is an existing
/// folder, else the default.
pub fn root(configured: Option<&Path>) -> Option<PathBuf> {
    match configured {
        Some(folder) if folder.is_dir() => Some(folder.to_path_buf()),
        _ => default_root(),
    }
}

/// The folder for `kind` under `root`.
pub fn folder(root: &Path, kind: Kind) -> PathBuf {
    root.join(kind.folder_name())
}

/// `LoadSave::getDirectories`: the folder for `kind` under `root` and under
/// each folder directly inside it (where imported banks go), those that
/// exist. The root's comes first, then the rest in natural order (upstream
/// takes them in the file system's order).
pub fn directories(root: &Path, kind: Kind) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut sub_folders: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    sub_folders.sort_by(|a, b| compare_paths(a, b));
    std::iter::once(root.to_path_buf())
        .chain(sub_folders)
        .map(|folder| folder.join(kind.folder_name()))
        .filter(|folder| folder.is_dir())
        .collect()
}

/// `LoadSave::FileSorterAscending`: the lower-cased full paths in natural
/// order (runs of digits compare as numbers).
pub fn compare_paths(a: &Path, b: &Path) -> Ordering {
    compare_natural(
        &a.to_string_lossy().to_lowercase(),
        &b.to_string_lossy().to_lowercase(),
    )
}

/// JUCE's `String::compareNatural`, for the paths' characters: digit runs
/// compare by value (then by length, so leading zeros sort first), other
/// characters by code point.
pub fn compare_natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let run = |chars: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(&c) = chars.peek().filter(|c| c.is_ascii_digit()) {
                        digits.push(c);
                        chars.next();
                    }
                    digits
                };
                let (x, y) = (run(&mut a), run(&mut b));
                let (tx, ty) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = tx
                    .len()
                    .cmp(&ty.len())
                    .then_with(|| tx.cmp(ty))
                    .then_with(|| y.len().cmp(&x.len()));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

/// The files directly in `folder` with `extension` (case-insensitive),
/// sorted as [`compare_paths`] does. Empty when the folder can't be read.
pub fn files_in(folder: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case(extension))
        })
        .collect();
    files.sort_by(|a, b| compare_paths(a, b));
    files
}

/// `PresetBrowser::jumpToPreset(indices)`: the file `by` places from
/// `current` among the files beside it with the same extension, wrapping
/// round. `None` when the folder has no such files.
pub fn step(current: &Path, by: i32) -> Option<PathBuf> {
    let folder = current.parent()?;
    let extension = current.extension()?.to_string_lossy().into_owned();
    let files = files_in(folder, &extension);
    if files.is_empty() {
        return None;
    }
    // indexOf is -1 for a file no longer there, as upstream's is.
    let index = files
        .iter()
        .position(|file| file == current)
        .map_or(-1, |i| i as i32);
    let count = files.len() as i32;
    let next = (index + by + count).rem_euclid(count);
    Some(files[next as usize].clone())
}

/// `LoadSave::getAllFilesOfTypeInDirectories`: the files inside each of
/// `directories` and their folders (`findChildFiles`, recursive) whose
/// extension is one of `extensions` (lowercase, without the dot), matched
/// ignoring case as on macOS. Each folder's files come in natural order.
pub fn files_of_type(directories: &[PathBuf], extensions: &[&str]) -> Vec<PathBuf> {
    fn walk(folder: &Path, extensions: &[&str], files: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        let mut entries: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        entries.sort_by(|a, b| compare_paths(a, b));
        for path in entries {
            if path.is_dir() {
                walk(&path, extensions, files);
            } else if path.extension().is_some_and(|extension| {
                let extension = extension.to_string_lossy().to_lowercase();
                extensions.contains(&extension.as_str())
            }) {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    for directory in directories {
        walk(directory, extensions, &mut files);
    }
    files
}

/// `LoadSave::getShiftedFile`: the file `shift` places from `current`
/// among `files` sorted by full path, wrapping round; the first file when
/// `current` isn't among them, and `None` when there are none.
pub fn shifted_file(
    mut files: Vec<PathBuf>,
    current: Option<&Path>,
    shift: i32,
) -> Option<PathBuf> {
    if files.is_empty() {
        return None;
    }
    files.sort_by(|a, b| compare_paths(a, b));
    let Some(index) = current.and_then(|current| files.iter().position(|file| file == current))
    else {
        return Some(files[0].clone());
    };
    let count = files.len() as i32;
    Some(files[(index as i32 + shift).rem_euclid(count) as usize].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_compares_numbers_by_value() {
        let mut names = ["b10", "B2", "a", "b1", "b02"];
        names.sort_by(|a, b| compare_paths(Path::new(a), Path::new(b)));
        assert_eq!(names, ["a", "b1", "b02", "B2", "b10"]);
    }

    /// Stepping wraps round the folder's presets in natural order and skips
    /// other files.
    #[test]
    fn stepping_wraps_round_the_folder() {
        let folder = std::env::temp_dir().join(format!("sas-step-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["Bass 10.vital", "bass 2.vital", "Lead.vital", "notes.txt"] {
            std::fs::write(folder.join(name), "").unwrap();
        }
        let bass_2 = folder.join("bass 2.vital");
        assert_eq!(step(&bass_2, 1), Some(folder.join("Bass 10.vital")));
        assert_eq!(step(&bass_2, -1), Some(folder.join("Lead.vital")));
        assert_eq!(step(&folder.join("Lead.vital"), 1), Some(bass_2.clone()));
        assert_eq!(files_in(&folder, PRESET_EXTENSION).len(), 3);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// The kind's folder under the root comes first, then those under the
    /// root's own folders (banks); folders without one are skipped.
    #[test]
    fn directories_include_banks() {
        let root = std::env::temp_dir().join(format!("sas-dirs-{}", std::process::id()));
        for folder in [
            "Presets",
            "Bank 10/Presets",
            "Bank 2/Presets",
            "Bank 3/Wavetables",
        ] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        assert_eq!(
            directories(&root, Kind::Presets),
            [
                root.join("Presets"),
                root.join("Bank 2/Presets"),
                root.join("Bank 10/Presets"),
            ]
        );
        assert_eq!(
            directories(&root, Kind::Wavetables),
            [root.join("Bank 3/Wavetables")]
        );
        std::fs::remove_dir_all(&root).unwrap();
        assert!(directories(&root, Kind::Presets).is_empty());
    }

    /// An upstream data folder keeps the user's files under `User`; that
    /// folder sits directly inside the root, so its kinds are found like a
    /// bank's (D24).
    #[test]
    fn an_upstream_user_folder_is_found_like_a_bank() {
        let root = std::env::temp_dir().join(format!("sas-dirs-user-{}", std::process::id()));
        for folder in [
            "User/Presets",
            "User/Wavetables",
            "User/Samples",
            "User/LFOs",
        ] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        for kind in [Kind::Presets, Kind::Wavetables, Kind::Samples, Kind::Lfos] {
            assert_eq!(
                directories(&root, kind),
                [root.join("User").join(kind.folder_name())]
            );
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Files of the given types under the folders and theirs, ignoring the
    /// extension's case; stepping wraps round them in full-path order, and
    /// starts at the first for a file not among them.
    #[test]
    fn files_of_a_type_step_round_every_folder() {
        let root = std::env::temp_dir().join(format!("sas-shifted-{}", std::process::id()));
        for file in [
            "Wavetables/b.vitaltable",
            "Wavetables/A.WAV",
            "Wavetables/Sub/c.flac",
            "Wavetables/notes.txt",
            "Bank/Wavetables/d.vitaltable",
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        let directories = directories(&root, Kind::Wavetables);
        let files = files_of_type(&directories, &["vitaltable", "wav", "flac"]);
        assert_eq!(
            files,
            [
                root.join("Wavetables/A.WAV"),
                root.join("Wavetables/b.vitaltable"),
                root.join("Wavetables/Sub/c.flac"),
                root.join("Bank/Wavetables/d.vitaltable"),
            ]
        );
        let b = root.join("Wavetables/b.vitaltable");
        assert_eq!(
            shifted_file(files.clone(), Some(&b), 1),
            Some(root.join("Wavetables/Sub/c.flac"))
        );
        // Sorted by full path, the bank's comes first.
        let d = root.join("Bank/Wavetables/d.vitaltable");
        assert_eq!(
            shifted_file(files.clone(), Some(&d), -1),
            Some(root.join("Wavetables/Sub/c.flac"))
        );
        assert_eq!(shifted_file(files.clone(), None, 1), Some(d.clone()));
        assert_eq!(shifted_file(Vec::new(), Some(&b), 1), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_missing_configured_root_falls_back_to_the_default() {
        assert_eq!(root(Some(Path::new("/no/such/folder"))), default_root());
        let here = std::env::temp_dir();
        assert_eq!(root(Some(&here)), Some(here.clone()));
        assert_eq!(folder(&here, Kind::Lfos), here.join("LFOs"));
    }
}
