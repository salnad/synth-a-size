// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity with the reference's audio file readers: every case of module
//! `audio_file/decode` (`migration/corpus/modules/audio_file/decode.json`,
//! captured by `ref-module` with JUCE's `AudioFormatManager` and pinned by
//! hash) decodes in the port to the same first channel, sample for sample,
//! and the same sample rate.

use std::path::Path;

use sas_parity::{all_case_ids, ModuleCase};
use sas_plugin::audio_file::read_audio;

const CASES: &str = "audio_file/decode/";

#[test]
fn authored_audio_files_decode_as_juce_reads_them() {
    let ids: Vec<String> = all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with(CASES))
        .collect();
    assert!(ids.len() >= 11, "{ids:?}");
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migration/corpus");
    for id in ids {
        let case = ModuleCase::load(&id);
        let file = case
            .params()
            .get("file")
            .and_then(|file| file.as_str())
            .unwrap()
            .to_string();
        let path = corpus.join(&file);
        let bytes = std::fs::read(&path).unwrap();
        let extension = path.extension().unwrap().to_str().unwrap();
        let audio =
            read_audio(&bytes, extension).unwrap_or_else(|| panic!("{id}: {file} didn't decode"));
        assert_eq!(
            case.artifact("sample_rate.txt"),
            format!("{}\n", audio.sample_rate),
            "{id}"
        );
        case.assert_parity("first_channel", &audio.first_channel);
    }
}
