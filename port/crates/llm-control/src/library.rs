//! The user's preset folder, the only place the tools read or write files.
//!
//! Names are plain file stems: no path separators, no leading dot, so a
//! model cannot reach outside the folder.

use std::fs;
use std::path::{Path, PathBuf};

use sas_preset::PresetState;

/// File extension of the upstream preset format, kept for compatibility.
pub const PRESET_EXTENSION: &str = "vital";

/// Longest preset name accepted.
const MAX_NAME: usize = 64;

/// A folder of `.vital` presets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetLibrary {
    root: PathBuf,
}

impl PresetLibrary {
    pub fn new(root: impl Into<PathBuf>) -> PresetLibrary {
        PresetLibrary { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file for preset `name`, if `name` is a valid preset name.
    pub fn path(&self, name: &str) -> Result<PathBuf, String> {
        let valid = !name.is_empty()
            && name.len() <= MAX_NAME
            && !name.starts_with(['.', ' '])
            && !name.ends_with(' ')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.'));
        if !valid {
            return Err(format!(
                "invalid preset name {name:?}: use up to {MAX_NAME} letters, digits, spaces, \
                 '_', '-' or '.', not starting with '.'"
            ));
        }
        Ok(self.root.join(format!("{name}.{PRESET_EXTENSION}")))
    }

    /// Preset names in the folder whose name contains `query` (any case),
    /// sorted, at most `limit`.
    pub fn list(&self, query: &str, limit: usize) -> Result<Vec<String>, String> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("cannot read the preset folder: {error}")),
        };
        let query = query.to_lowercase();
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension()? == PRESET_EXTENSION)
                    .then(|| path.file_stem()?.to_str().map(str::to_string))
                    .flatten()
            })
            .filter(|name| self.path(name).is_ok() && name.to_lowercase().contains(&query))
            .collect();
        names.sort();
        names.truncate(limit);
        Ok(names)
    }

    /// Loads preset `name` on top of `base`, as `SynthBase::loadFromFile`
    /// loads onto the current synth. A failed load leaves nothing changed
    /// (D4).
    pub fn load(&self, name: &str, base: &PresetState) -> Result<PresetState, String> {
        let path = self.path(name)?;
        if !path.is_file() {
            return Err(format!("no preset named {name:?}; use list_presets"));
        }
        let mut state = base.clone();
        state
            .load_file(&path)
            .map_err(|error| format!("cannot load {name:?}: {error}"))?;
        Ok(state)
    }

    /// Writes `state` as preset `name`. An existing file is only replaced
    /// with `overwrite`.
    pub fn save(
        &self,
        name: &str,
        state: &PresetState,
        overwrite: bool,
    ) -> Result<PathBuf, String> {
        let path = self.path(name)?;
        if path.exists() && !overwrite {
            return Err(format!(
                "a preset named {name:?} exists; ask the user before overwriting it"
            ));
        }
        fs::create_dir_all(&self.root)
            .map_err(|error| format!("cannot create the preset folder: {error}"))?;
        fs::write(&path, state.to_json_string())
            .map_err(|error| format!("cannot write {name:?}: {error}"))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_cannot_leave_the_folder() {
        let library = PresetLibrary::new("/presets");
        assert_eq!(
            library.path("Warm Pad").unwrap(),
            Path::new("/presets/Warm Pad.vital")
        );
        for bad in [
            "",
            "../x",
            "a/b",
            "a\\b",
            ".hidden",
            " x",
            "x ",
            &"y".repeat(65),
            "é",
        ] {
            assert!(library.path(bad).is_err(), "{bad:?}");
        }
    }
}
