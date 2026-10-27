// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the preset browser reads from preset files without loading them:
//! every preset under the user folders, and each file's author, style and
//! creation date. Ports `LoadSave::getAllPresets`, `getAuthorFromFile` and
//! `getStyleFromFile` (`vital/src/common/load_save.cpp`) and the dates
//! `PresetList` shows (`File::getCreationTime`).

use std::io::Read;
use std::path::{Path, PathBuf};

use sas_common::json;

use crate::user_folders::{self, compare_paths, Kind};

/// `LoadSave::getAllFilesOfTypeInDirectories`: every file with `extension`
/// in the kind's folders (`user_folders::directories`) and the folders
/// inside them, in natural path order (upstream takes the file system's).
pub fn all_files(root: &Path, kind: Kind, extension: &str) -> Vec<PathBuf> {
    fn walk(folder: &Path, extension: &str, files: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        for path in entries.filter_map(|entry| entry.ok().map(|entry| entry.path())) {
            if path.is_dir() {
                walk(&path, extension, files);
            } else if path.extension().is_some_and(|ext| ext == extension) {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    for folder in user_folders::directories(root, kind) {
        walk(&folder, extension, &mut files);
    }
    files.sort_by(|a, b| compare_paths(a, b));
    files
}

/// `LoadSave::getAuthorFromFile`: a file that starts `{"author":"` gives
/// the next 40 bytes up to a quote; any other is parsed whole for its
/// `"author"`. Files under 60 bytes have none.
pub fn author_from_file(path: &Path) -> String {
    const MAX_CHARACTERS: usize = 40;
    const MIN_SIZE: usize = 60;
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    if bytes.len() < MIN_SIZE {
        return String::new();
    }
    if &bytes[2..8] == b"author" && &bytes[8..11] == b"\":\"" {
        let name = &bytes[11..(11 + MAX_CHARACTERS).min(bytes.len())];
        let name = String::from_utf8_lossy(name);
        return name.split('"').next().unwrap_or_default().to_string();
    }
    json::parse_bytes(&bytes)
        .ok()
        .and_then(|data| data.get("author").ok()?.to_str().ok().map(str::to_string))
        .unwrap_or_default()
}

/// `LoadSave::getStyleFromFile`: the text after the `preset_style` key in
/// the file's first 5000 bytes, splitting at quotes. Smaller files have
/// none.
pub fn style_from_file(path: &Path) -> String {
    const MIN_SIZE: usize = 5000;
    let Ok(file) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut start = Vec::with_capacity(MIN_SIZE);
    if file.take(MIN_SIZE as u64).read_to_end(&mut start).is_err() || start.len() < MIN_SIZE {
        return String::new();
    }
    let text = String::from_utf8_lossy(&start);
    let mut found_style = false;
    for token in text.split('"') {
        if found_style && token.trim() != ":" {
            return token.to_string();
        }
        if token == "preset_style" {
            found_style = true;
        }
    }
    String::new()
}

/// A file's creation time in seconds since 1970 (`File::getCreationTime`:
/// the birth time on macOS, the status change time on Linux, as JUCE reads
/// them).
pub fn creation_time(path: &Path) -> Option<i64> {
    let metadata = std::fs::metadata(path).ok()?;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.ctime())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let created = metadata.created().ok()?;
        let since = created.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(since.as_secs() as i64)
    }
}

/// `Time::toString(true, false, false)`: the local date as "26 Oct 2026".
pub fn date_string(seconds: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (year, month, day) = local_date(seconds);
    format!("{day} {} {year}", MONTHS[(month - 1) as usize])
}

/// The local calendar date at `seconds` since 1970.
#[cfg(unix)]
fn local_date(seconds: i64) -> (i64, u32, u32) {
    let time = seconds as libc::time_t;
    // SAFETY: `localtime_r` writes only the `tm` it is given.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&time, &mut tm) }.is_null() {
        return utc_date(seconds);
    }
    (
        tm.tm_year as i64 + 1900,
        tm.tm_mon as u32 + 1,
        tm.tm_mday as u32,
    )
}

#[cfg(not(unix))]
fn local_date(seconds: i64) -> (i64, u32, u32) {
    utc_date(seconds)
}

/// The UTC calendar date at `seconds` since 1970 (days to civil date).
fn utc_date(seconds: i64) -> (i64, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("sas-files-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn utc_dates() {
        assert_eq!(utc_date(0), (1970, 1, 1));
        assert_eq!(utc_date(951_782_400), (2000, 2, 29));
        assert_eq!(utc_date(1_792_972_800), (2026, 10, 26));
    }

    /// The fast path reads the author straight after `{"author":"`, up to a
    /// quote or 40 bytes; anything else is parsed.
    #[test]
    fn authors() {
        let folder = scratch("authors");
        let padding = " ".repeat(60);
        let fast = folder.join("fast");
        std::fs::write(
            &fast,
            format!("{{\"author\":\"Someone\",\"x\":\"{padding}\"}}"),
        )
        .unwrap();
        assert_eq!(author_from_file(&fast), "Someone");
        let long = folder.join("long");
        let name = "N".repeat(50);
        std::fs::write(
            &long,
            format!("{{\"author\":\"{name}\",\"x\":\"{padding}\"}}"),
        )
        .unwrap();
        assert_eq!(author_from_file(&long), "N".repeat(40));
        let spaced = folder.join("spaced");
        std::fs::write(
            &spaced,
            format!("{{\n  \"author\": \"Spaced\", \"x\": \"{padding}\"\n}}"),
        )
        .unwrap();
        assert_eq!(author_from_file(&spaced), "Spaced");
        let small = folder.join("small");
        std::fs::write(&small, "{\"author\":\"Tiny\"}").unwrap();
        assert_eq!(author_from_file(&small), "");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// The style comes from the first 5000 bytes of files that size or
    /// larger.
    #[test]
    fn styles() {
        let folder = scratch("styles");
        let padding = " ".repeat(5000);
        let styled = folder.join("styled");
        std::fs::write(
            &styled,
            format!("{{\"preset_style\": \"Bass\", \"x\": \"{padding}\"}}"),
        )
        .unwrap();
        assert_eq!(style_from_file(&styled), "Bass");
        let empty = folder.join("empty");
        std::fs::write(
            &empty,
            format!("{{\"preset_style\": \"\", \"x\": \"{padding}\"}}"),
        )
        .unwrap();
        assert_eq!(style_from_file(&empty), "");
        let small = folder.join("small");
        std::fs::write(&small, "{\"preset_style\": \"Bass\"}").unwrap();
        assert_eq!(style_from_file(&small), "");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// Presets come from the Presets folders under the root and its banks,
    /// including folders inside them.
    #[test]
    fn all_presets() {
        let root = scratch("all");
        for file in [
            "Presets/b.vital",
            "Presets/Sub/a.vital",
            "Bank/Presets/c.vital",
            "Presets/n.txt",
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        let names: Vec<_> = all_files(&root, Kind::Presets, "vital")
            .iter()
            .map(|path| path.strip_prefix(&root).unwrap().to_path_buf())
            .collect();
        assert_eq!(
            names,
            [
                PathBuf::from("Bank/Presets/c.vital"),
                PathBuf::from("Presets/b.vital"),
                PathBuf::from("Presets/Sub/a.vital"),
            ]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
