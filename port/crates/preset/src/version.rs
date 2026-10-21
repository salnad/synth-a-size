// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Version checks of `LoadSave::jsonToState`, on top of
//! [`sas_common::load_save::compare_version_strings`].

use sas_common::load_save::compare_version_strings;

pub use sas_common::load_save::VERSION_STRING as SYNTH_VERSION;

/// `LoadSave::compareFeatureVersionStrings`: compares everything before the
/// last dot ("1.0.6" and "1.0.9" share feature version "1.0").
pub fn compare_feature_version_strings(a: &str, b: &str) -> i32 {
    let feature = |text: &str| -> String {
        text.rsplit_once('.')
            .map_or(text, |(head, _)| head)
            .to_string()
    };
    compare_version_strings(&feature(a), &feature(b))
}

/// `compareVersionStrings(version, other) < 0`.
pub(crate) fn older_than(version: &str, other: &str) -> bool {
    compare_version_strings(version, other) < 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_versions() {
        assert_eq!(compare_feature_version_strings("1.0.9", "1.0.6"), 0);
        assert_eq!(compare_feature_version_strings("1.1.0", "1.0.6"), 1);
        assert_eq!(compare_feature_version_strings("0.9.9", "1.0.6"), -1);
        assert!(older_than("0.2.0", "0.2.1"));
        assert!(!older_than("1.0.6", "0.9.0"));
    }
}
