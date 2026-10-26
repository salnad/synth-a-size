//! The port's layout against upstream's, component by component.
//!
//! The fixtures in `migration/corpus/gui/layout/` are upstream's own bounds,
//! written by `migration/scripts/gui_reference.py layout` from ref-gui.

use std::path::PathBuf;

use sas_gui::controls::modulation_button::ModulationTabSelector;
use sas_gui::layout::{FullLayout, Rect};
use sas_gui::scope::Scope;
use sas_gui::sections::bend::{self, BendLayout};
use sas_gui::sections::envelope::{self, EnvelopeLayout};
use sas_gui::sections::lfo::{self, LfoLayout};
use sas_gui::sections::modulation::{ModulationInterface, ModulationLayout};
use sas_gui::sections::oscillator::{self, OscillatorLayout};
use sas_gui::sections::portamento::{self, PortamentoLayout};
use sas_gui::sections::random::{self, RandomLayout};
use sas_gui::sections::sample::{self, SampleLayout};
use sas_gui::sections::synthesis::SynthesisLayout;
use sas_gui::sections::voice::{self, VoiceLayout};
use sas_gui::sections::volume::VolumeLayout;
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

/// What a preset in `migration/corpus/gui/presets/` changes about the
/// layout: which oscillators show the distortion phase slider.
fn phase_visible(preset: Option<&str>) -> [bool; 3] {
    match preset {
        None => [false; 3],
        Some("migration/corpus/gui/presets/osc_3_sync.vital") => [false, false, true],
        Some(preset) => panic!("{preset}: this test does not know the preset"),
    }
}

/// The port's rectangle for each upstream component path it lays out.
fn port_rect(skin: &Skin, layout: &FullLayout, path: &str, preset: Option<&str>) -> Option<Rect> {
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
        _ if path.starts_with("modulation/") => return modulation_rect(skin, layout, path),
        _ => return synthesis_rect(skin, layout, path, phase_visible(preset)),
    })
}

/// The modulation column, with the first source of each kind selected.
fn modulation_rect(skin: &Skin, layout: &FullLayout, path: &str) -> Option<Rect> {
    let top = Scope::top(skin, layout.size_ratio);
    let m = ModulationLayout::new(&top, layout.modulation);
    let rest = path.strip_prefix("modulation/")?;
    let tabs = |selector: &ModulationTabSelector, bounds: Rect, name: &str| {
        let shown = selector.num_shown(|_| false);
        let index = selector.buttons.iter().position(|b| b.name == name)?;
        selector.button_bounds(bounds, shown).get(index).copied()
    };
    let interface = ModulationInterface::default();
    if let Some(name) = rest.strip_prefix("env/") {
        return tabs(interface.tab_selector(0), m.envelope_tabs, name);
    }
    if let Some(name) = rest.strip_prefix("lfo/") {
        return tabs(interface.tab_selector(1), m.lfo_tabs, name);
    }
    if let Some(name) = rest.strip_prefix("random/") {
        return tabs(interface.tab_selector(2), m.random_tabs, name);
    }
    if let Some(name) = rest.strip_prefix("top/") {
        return tabs(interface.tab_selector(3), m.keyboard_top, name);
    }
    if let Some(name) = rest.strip_prefix("bottom/") {
        return tabs(interface.tab_selector(4), m.keyboard_bottom, name);
    }
    if let Some(control) = rest.strip_prefix("ENVELOPE 1") {
        let env = EnvelopeLayout::new(&top.within(envelope::SKIN), m.envelope);
        let knob = |i: usize| Some(env.knobs[i]);
        return match control {
            "" => Some(env.bounds),
            "/env_1_delay" => knob(0),
            "/env_1_attack" => knob(1),
            "/env_1_hold" => knob(2),
            "/env_1_decay" => knob(3),
            "/env_1_sustain" => knob(4),
            "/env_1_release" => knob(5),
            "/env_1" => Some(env.editor),
            "/Magnifying Glass" => Some(env.magnifying_glass),
            _ => None,
        };
    }
    if let Some(control) = rest.strip_prefix("LFO 1") {
        let l = LfoLayout::new(&top.within(lfo::SKIN), m.lfo);
        return Some(match control {
            "" => l.bounds,
            "/lfo_1_tempo" | "/lfo_1_frequency" => l.rate,
            "/lfo_1_keytrack_transpose" => l.keytrack_transpose,
            "/lfo_1_keytrack_tune" => l.keytrack_tune,
            "/lfo_1_sync" => l.sync,
            "/lfo_1_sync_type" => l.sync_type,
            "/lfo_1_phase" => l.phase,
            "/lfo_1_smooth_time" | "/lfo_1_fade_time" => l.fade,
            "/Smooth Mode Text" | "/Smooth Mode" => l.smooth_mode,
            "/lfo_1_delay_time" => l.delay,
            "/lfo_1_stereo" => l.stereo,
            "/paint_pattern" => l.paint_pattern,
            "/grid_size_x" => l.grid_size_x,
            "/grid_size_y" => l.grid_size_y,
            "/paint" => l.paint,
            "/smooth" => l.smooth,
            "/lfo_1" => l.editor,
            "/preset_selector" => l.preset_selector.bounds,
            "/preset_selector/Text" => l.preset_selector.text,
            "/preset_selector/Prev" => l.preset_selector.previous,
            "/preset_selector/Next" => l.preset_selector.next,
            _ => return None,
        });
    }
    if let Some(control) = rest.strip_prefix("RANDOM 1") {
        let r = RandomLayout::new(&top.within(random::SKIN), m.random);
        return Some(match control {
            "" => r.bounds,
            "/random_1_frequency" | "/random_1_tempo" => r.rate,
            "/random_1_keytrack_transpose" => r.keytrack_transpose,
            "/random_1_keytrack_tune" => r.keytrack_tune,
            "/random_1_sync" => r.sync,
            "/random_1_style" => r.style,
            "/random_1" => r.viewer,
            "/random_1_stereo" => r.stereo,
            "/random_1_sync_type" => r.sync_type,
            _ => return None,
        });
    }
    match rest {
        "env" => Some(m.envelope_tabs),
        "lfo" => Some(m.lfo_tabs),
        "random" => Some(m.random_tabs),
        "top" => Some(m.keyboard_top),
        "bottom" => Some(m.keyboard_bottom),
        _ => None,
    }
}

/// The Voice tab's synthesis column, with the distortion phase slider
/// shown on the oscillators `phase_visible` says.
fn synthesis_rect(
    skin: &Skin,
    layout: &FullLayout,
    path: &str,
    phase_visible: [bool; 3],
) -> Option<Rect> {
    let top = Scope::top(skin, layout.size_ratio);
    let synthesis = SynthesisLayout::new(&top, layout.main);
    let rest = path.strip_prefix("synthesis/")?;
    if let Some(rest) = rest.strip_prefix("SMP") {
        let smp = SampleLayout::new(&top.within(sample::SKIN), synthesis.sample);
        return Some(match rest {
            "" => smp.bounds,
            "/sample_on" => {
                sas_gui::section::power_button_bounds(&top.within(sample::SKIN), smp.bounds)
            }
            "/Transpose Quantize" => smp.transpose_quantize,
            "/sample_transpose" => smp.transpose,
            "/sample_tune" => smp.tune,
            "/sample_level" => smp.level,
            "/sample_pan" => smp.pan,
            "/preset_selector" => smp.preset_selector.bounds,
            "/preset_selector/Text" => smp.preset_selector.text,
            "/preset_selector/Prev" => smp.preset_selector.previous,
            "/preset_selector/Next" => smp.preset_selector.next,
            "/Destination Text" | "/Destination" => smp.destination,
            "/Prev Destination" => smp.prev_destination,
            "/Next Destination" => smp.next_destination,
            "/sample_keytrack" => smp.keytrack,
            "/sample_random_phase" => smp.random_phase,
            "/sample_loop" => smp.loop_button,
            "/sample_bounce" => smp.bounce,
            _ => return None,
        });
    }
    match rest {
        "FILTER 1" => return Some(synthesis.filter_1),
        "FILTER 2" => return Some(synthesis.filter_2),
        _ => {}
    }
    let rest = rest.strip_prefix("OSC ")?;
    let (number, rest) = rest.split_at(1);
    let n: usize = number.parse().ok()?;
    let scope = top.within(oscillator::SKIN);
    let osc = OscillatorLayout::new(&scope, synthesis.oscillators[n - 1], phase_visible[n - 1]);
    if rest.is_empty() {
        return Some(osc.bounds);
    }
    let control = rest.strip_prefix('/')?;
    let prefix = format!("osc_{n}_");
    Some(match control.strip_prefix(&prefix).unwrap_or(control) {
        "on" => sas_gui::section::power_button_bounds(&scope, osc.bounds),
        "transpose" => osc.transpose,
        "tune" => osc.tune,
        "unison_voices" => osc.unison_voices,
        "unison_detune" => osc.unison_detune,
        "detune_power" => osc.detune_power,
        "phase" => osc.phase,
        "random_phase" => osc.random_phase,
        "level" => osc.level,
        "pan" => osc.pan,
        "spectral_morph_amount" => osc.spectral_morph_amount,
        "distortion_amount" => osc.distortion_amount,
        "wave_frame" => osc.wave_frame,
        "dimension" => osc.dimension,
        "distortion_phase" => osc.distortion_phase?,
        "Transpose Quantize" => osc.transpose_quantize,
        "Frequency Morph Text" => osc.spectral_morph_text,
        "Distortion Text" => osc.distortion_text,
        "Spectral Morph" => osc.spectral_morph_selector,
        "Distortion" => osc.distortion_selector,
        "Prev Spectral" => osc.prev_spectral,
        "Next Spectral" => osc.next_spectral,
        "Prev Distortion" => osc.prev_distortion,
        "Next Distortion" => osc.next_distortion,
        "preset_selector" => osc.preset_selector.bounds,
        "preset_selector/Text" => osc.preset_selector.text,
        "preset_selector/Prev" => osc.preset_selector.previous,
        "preset_selector/Next" => osc.preset_selector.next,
        "edit" => osc.edit,
        "Destination Text" | "Destination" => osc.destination,
        "Prev Destination" => osc.prev_destination,
        "Next Destination" => osc.next_destination,
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
            let preset = entry.get("preset").map(|preset| preset.as_str().unwrap());
            let actual = port_rect(&skin, &layout, path, preset)
                .unwrap_or_else(|| panic!("{}: no port rectangle for {path}", file.display()));
            assert_eq!(actual, expected, "{path} at {width}x{height}");
            checked += 1;
        }
    }
    assert!(checked > 0);
}
