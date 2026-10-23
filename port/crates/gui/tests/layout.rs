//! The port's layout against upstream's, component by component.
//!
//! The fixtures in `migration/corpus/gui/layout/` are upstream's own bounds,
//! written by `migration/scripts/gui_reference.py layout` from ref-gui.

use std::path::PathBuf;

use sas_gui::layout::{FullLayout, Rect};
use sas_gui::scope::Scope;
use sas_gui::sections::portamento::{self, PortamentoLayout};
use sas_gui::sections::voice::{self, VoiceLayout};
use sas_gui::skin::Skin;

fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../migration/corpus/gui")
}

/// `migration/corpus/gui/layout_paths.txt`: every component the port lays out.
fn checked_paths() -> Vec<String> {
    std::fs::read_to_string(corpus().join("layout_paths.txt"))
        .unwrap()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('@'))
        .map(str::to_string)
        .collect()
}

fn fixtures() -> Vec<PathBuf> {
    let dir = corpus().join("layout");
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
fn port_rect(skin: &Skin, layout: &FullLayout, path: &str) -> Option<Rect> {
    let header = &layout.header;
    let top = Scope::top(skin, layout.size_ratio);
    let voice = VoiceLayout::new(&top.within(voice::SKIN), layout.voice);
    let portamento = PortamentoLayout::new(&top.within(portamento::SKIN), layout.portamento);
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
        "VOICE/polyphony" => voice.polyphony,
        "VOICE/pitch_bend_range" => voice.pitch_bend_range,
        "VOICE/velocity_track" => voice.velocity_track,
        "VOICE/stereo_routing" => voice.stereo_routing,
        "VOICE/Stereo Mode" | "VOICE/Stereo Mode Text" => voice.stereo_mode,
        "PORTAMENTO/portamento_time" => portamento.portamento_time,
        "PORTAMENTO/portamento_slope" => portamento.portamento_slope,
        "PORTAMENTO/portamento_force" => portamento.portamento_force,
        "PORTAMENTO/portamento_scale" => portamento.portamento_scale,
        "PORTAMENTO/legato" => portamento.legato,
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
        let components = data["components"].as_object().unwrap();
        for path in checked_paths() {
            assert!(
                components.contains_key(&path),
                "{}: no entry for {path}; regenerate the fixtures",
                file.display()
            );
        }
        for (path, entry) in components {
            let bounds: Vec<i32> = entry["bounds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap() as i32)
                .collect();
            let expected = Rect::new(bounds[0], bounds[1], bounds[2], bounds[3]);
            let actual = port_rect(&skin, &layout, path)
                .unwrap_or_else(|| panic!("{}: no port rectangle for {path}", file.display()));
            assert_eq!(actual, expected, "{path} at {width}x{height}");
            checked += 1;
        }
    }
    assert!(checked > 0);
}
