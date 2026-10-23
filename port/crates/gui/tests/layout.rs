//! The port's layout against upstream's, component by component.
//!
//! The fixtures in `migration/corpus/gui/layout/` are upstream's own bounds,
//! written by `migration/scripts/gui_reference.py layout` from ref-gui.

use std::path::PathBuf;

use sas_gui::layout::{FullLayout, Rect};
use sas_gui::skin::Skin;

fn fixtures() -> Vec<PathBuf> {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../migration/corpus/gui/layout");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no layout fixtures in {}", dir.display());
    files
}

/// The port's rectangle for each upstream component path it lays out.
fn port_rect(layout: &FullLayout, path: &str) -> Option<Rect> {
    let header = &layout.header;
    Some(match path {
        "header_section" => header.bounds,
        "header_section/logo_section" => header.logo,
        "header_section/tab_selector" => header.tabs,
        "header_section/preset_selector" => header.preset_selector,
        "header_section/preset_selector/preset_selector" => header.preset.bounds,
        "header_section/preset_selector/preset_selector/Text" => header.preset.text,
        "header_section/preset_selector/preset_selector/Prev" => header.preset.previous,
        "header_section/preset_selector/preset_selector/Next" => header.preset.next,
        "header_section/preset_selector/Save" => header.save_button,
        "header_section/preset_selector/Menu" => header.menu_button,
        "header_section/VOLUME" => header.volume,
        "header_section/view_spectrogram" => header.oscilloscope,
        "synthesis" | "master_controls" | "MODULATION MATRIX" => layout.main,
        "effects" => layout.effects,
        "modulation" => layout.modulation,
        "extra_mod_section" => layout.extra_mod,
        "keyboard" => layout.keyboard,
        "BEND" => layout.bend,
        "PORTAMENTO" => layout.portamento,
        "VOICE" => layout.voice,
        _ => return None,
    })
}

#[test]
fn layout_matches_upstream_at_every_size() {
    let skin = Skin::default();
    let mut checked = 0;
    for file in fixtures() {
        let text = std::fs::read_to_string(&file).unwrap();
        let data: serde_json::Value = serde_json::from_str(&text).unwrap();
        let width = data["width"].as_i64().unwrap() as i32;
        let height = data["height"].as_i64().unwrap() as i32;
        let layout = FullLayout::new(&skin, width, height);
        for (path, entry) in data["components"].as_object().unwrap() {
            let bounds: Vec<i32> = entry["bounds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap() as i32)
                .collect();
            let expected = Rect::new(bounds[0], bounds[1], bounds[2], bounds[3]);
            let actual = port_rect(&layout, path)
                .unwrap_or_else(|| panic!("{}: no port rectangle for {path}", file.display()));
            assert_eq!(actual, expected, "{path} at {width}x{height}");
            checked += 1;
        }
    }
    assert!(checked > 0);
}
