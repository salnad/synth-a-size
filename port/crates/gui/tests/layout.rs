//! The port's layout against upstream's, component by component.
//!
//! The fixtures in `migration/corpus/gui/layout/` are upstream's own bounds,
//! written by `migration/scripts/gui_reference.py layout` from ref-gui.

use std::path::PathBuf;

use sas_gui::layout::{FullLayout, Rect};
use sas_gui::scope::Scope;
use sas_gui::sections::bend::{self, BendLayout};
use sas_gui::sections::portamento::{self, PortamentoLayout};
use sas_gui::sections::voice::{self, VoiceLayout};
use sas_gui::sections::volume::VolumeLayout;
use sas_gui::skin::Skin;

/// G6: the Effects tab, laid out with every effect on
/// (`migration/corpus/gui/presets/all_effects.vital`).
mod effects {
    use sas_gui::layout::{FullLayout, Rect};
    use sas_gui::scope::Scope;
    use sas_gui::sections::effects::chorus::ChorusLayout;
    use sas_gui::sections::effects::delay::{DelayLayout, STYLE_MONO};
    use sas_gui::sections::effects::distortion::DistortionLayout;
    use sas_gui::sections::effects::flanger::FlangerLayout;
    use sas_gui::sections::effects::order::OrderLayout;
    use sas_gui::sections::effects::phaser::PhaserLayout;
    use sas_gui::sections::effects::{self, Effect, EffectsLayout, NUM_EFFECTS};
    use sas_gui::skin::Skin;

    pub const ALL_EFFECTS: &str = "migration/corpus/gui/presets/all_effects.vital";

    pub fn all_effects_rect(skin: &Skin, layout: &FullLayout, path: &str) -> Option<Rect> {
        let top = Scope::top(skin, layout.size_ratio);
        let scope = top.within(effects::SKIN);
        let order: [usize; NUM_EFFECTS] = std::array::from_fn(|i| i);
        let fx = EffectsLayout::new(&scope, layout.effects, order, [true; NUM_EFFECTS]);
        let order_layout = OrderLayout::new(&scope, fx.order, order);
        let section = |effect: Effect| fx.effects[effect as usize].unwrap();
        let in_effect = |effect: Effect| top.within(effect.skin());
        let path = path.strip_prefix("effects/")?;
        if path == "effect_chain_order" {
            return Some(fx.order);
        }
        if path == "container" {
            return Some(fx.container);
        }
        if let Some(rest) = path.strip_prefix("effect_chain_order/") {
            let (name, button) = rest.split_once('/').unwrap_or((rest, ""));
            let effect = Effect::ALL.into_iter().find(|e| e.name() == name)?;
            return Some(if button.is_empty() {
                order_layout.tiles[effect as usize]
            } else {
                order_layout.power_buttons[effect as usize]
            });
        }
        let rest = path.strip_prefix("container/")?;
        let (title, control) = rest.split_once('/').unwrap_or((rest, ""));
        let effect = Effect::ALL.into_iter().find(|e| e.title() == title)?;
        let bounds = section(effect);
        if control.is_empty() {
            return Some(bounds);
        }
        let scope = in_effect(effect);
        Some(match effect {
            Effect::Chorus => {
                let l = ChorusLayout::new(&scope, bounds);
                match control {
                    "chorus_on" => l.on,
                    "chorus_voices" => l.voices,
                    "chorus_frequency" | "chorus_tempo" => l.frequency,
                    "chorus_sync" => l.sync,
                    "chorus_mod_depth" => l.mod_depth,
                    "chorus_delay_1" => l.delay_1,
                    "chorus_delay_2" => l.delay_2,
                    "chorus_feedback" => l.feedback,
                    "chorus_dry_wet" => l.dry_wet,
                    "chorus_cutoff" => l.cutoff,
                    "chorus_spread" => l.spread,
                    _ => return None,
                }
            }
            Effect::Delay => {
                let l = DelayLayout::new(&scope, bounds, STYLE_MONO);
                match control {
                    "delay_on" => l.on,
                    "delay_frequency" | "delay_tempo" => l.frequency,
                    "delay_sync" => l.sync,
                    "delay_aux_frequency" | "delay_aux_tempo" => l.aux_frequency,
                    "delay_aux_sync" => l.aux_sync,
                    "delay_style" => l.style,
                    "delay_feedback" => l.feedback,
                    "delay_dry_wet" => l.dry_wet,
                    "delay_filter_cutoff" => l.filter_cutoff,
                    "delay_filter_spread" => l.filter_spread,
                    _ => return None,
                }
            }
            Effect::Distortion => {
                let l = DistortionLayout::new(&scope, bounds);
                match control {
                    "distortion_on" => l.on,
                    "distortion_type" => l.kind,
                    "distortion_filter_order" => l.filter_order,
                    "distortion_drive" => l.drive,
                    "distortion_mix" => l.mix,
                    "distortion_filter_cutoff" => l.filter_cutoff,
                    "distortion_filter_resonance" => l.filter_resonance,
                    "distortion_filter_blend" => l.filter_blend,
                    _ => return None,
                }
            }
            Effect::Flanger => {
                let l = FlangerLayout::new(&scope, bounds);
                match control {
                    "flanger_on" => l.on,
                    "flanger_frequency" | "flanger_tempo" => l.frequency,
                    "flanger_sync" => l.sync,
                    "flanger_phase_offset" => l.phase_offset,
                    "flanger_feedback" => l.feedback,
                    "flanger_dry_wet" => l.dry_wet,
                    "flanger_center" => l.center,
                    "flanger_mod_depth" => l.mod_depth,
                    _ => return None,
                }
            }
            Effect::Phaser => {
                let l = PhaserLayout::new(&scope, bounds);
                match control {
                    "phaser_on" => l.on,
                    "phaser_frequency" | "phaser_tempo" => l.frequency,
                    "phaser_sync" => l.sync,
                    "phaser_phase_offset" => l.phase_offset,
                    "phaser_blend" => l.blend,
                    "phaser_feedback" => l.feedback,
                    "phaser_dry_wet" => l.dry_wet,
                    "phaser_center" => l.center,
                    "phaser_mod_depth" => l.mod_depth,
                    _ => return None,
                }
            }
            _ => return None,
        })
    }
}

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
    let bend = BendLayout::new(&top.within(bend::SKIN), layout.bend);
    let volume = VolumeLayout::new(header.volume);
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
        "header_section/VOLUME/volume" => volume.volume,
        "BEND/pitch_wheel" => bend.pitch_wheel,
        "BEND/mod_wheel" => bend.mod_wheel,
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
            // Entries laid out with a preset (`@preset` in layout_paths.txt)
            // name it; the port must lay the path out for that preset.
            let actual = match entry.get("preset").map(|preset| preset.as_str().unwrap()) {
                None => port_rect(&skin, &layout, path),
                Some(effects::ALL_EFFECTS) => effects::all_effects_rect(&skin, &layout, path),
                Some(preset) => panic!(
                    "{}: {path} is laid out with {preset}, which this test does not know",
                    file.display()
                ),
            }
            .unwrap_or_else(|| panic!("{}: no port rectangle for {path}", file.display()));
            assert_eq!(actual, expected, "{path} at {width}x{height}");
            checked += 1;
        }
    }
    assert!(checked > 0);
}
