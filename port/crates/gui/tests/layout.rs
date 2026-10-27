//! The port's layout against upstream's, component by component.
//!
//! The fixtures in `migration/corpus/gui/layout/` are upstream's own bounds,
//! written by `migration/scripts/gui_reference.py layout` from ref-gui.

use std::path::PathBuf;

use sas_gui::controls::modulation_button::ModulationTabSelector;
use sas_gui::layout::{FullLayout, Rect};
use sas_gui::local::PRESET_STYLE_NAMES;
use sas_gui::scope::Scope;
use sas_gui::sections::bend::{self, BendLayout};
use sas_gui::sections::envelope::{self, EnvelopeLayout};
use sas_gui::sections::extra_mod::ExtraModLayout;
use sas_gui::sections::filter::{self, FilterLayout, Kind};
use sas_gui::sections::lfo::{self, LfoLayout};
use sas_gui::sections::modulation::{ModulationInterface, ModulationLayout};
use sas_gui::sections::oscillator::{self, OscillatorLayout};
use sas_gui::sections::popup_browser::{self, PopupBrowserLayout};
use sas_gui::sections::portamento::{self, PortamentoLayout};
use sas_gui::sections::preset_browser::PresetBrowserLayout;
use sas_gui::sections::random::{self, RandomLayout};
use sas_gui::sections::sample::{self, SampleLayout};
use sas_gui::sections::save_section::SaveLayout;
use sas_gui::sections::synthesis::SynthesisLayout;
use sas_gui::sections::voice::{self, VoiceLayout};
use sas_gui::sections::volume::VolumeLayout;
use sas_gui::sections::wave_source_overlay::OverlayLayout;
use sas_gui::sections::wavetable_edit::{component_rows, WavetableEditLayout};
use sas_gui::skin::{Skin, ValueId};
use sas_preset::{InitContent, PresetState};

/// G6: the Effects tab, laid out with every effect on
/// (`migration/corpus/gui/presets/all_effects.vital`).
mod effects {
    use sas_gui::layout::{FullLayout, Rect};
    use sas_gui::scope::Scope;
    use sas_gui::sections::effects::chorus::ChorusLayout;
    use sas_gui::sections::effects::compressor::CompressorLayout;
    use sas_gui::sections::effects::delay::{DelayLayout, STYLE_MONO};
    use sas_gui::sections::effects::distortion::DistortionLayout;
    use sas_gui::sections::effects::equalizer::EqualizerLayout;
    use sas_gui::sections::effects::flanger::FlangerLayout;
    use sas_gui::sections::effects::order::OrderLayout;
    use sas_gui::sections::effects::phaser::PhaserLayout;
    use sas_gui::sections::effects::reverb::ReverbLayout;
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
            Effect::FilterFx => {
                let f = sas_gui::sections::filter::FilterSection::fx().layout(&scope, bounds);
                match control {
                    "filter_fx_on" => sas_gui::section::power_button_bounds(&scope, bounds),
                    "filter_fx_cutoff" => f.cutoff,
                    "filter_fx_resonance" => f.resonance,
                    "filter_fx_blend" => f.blend,
                    "filter_fx_drive" => f.drive,
                    "filter_fx_mix" => f.mix,
                    "filter_fx_keytrack" => f.keytrack,
                    "label1" => f.drive_label,
                    "label2" => f.keytrack_label,
                    "preset_selector" => f.preset_selector.bounds,
                    "preset_selector/Text" => f.preset_selector.text,
                    "preset_selector/Prev" => f.preset_selector.previous,
                    "preset_selector/Next" => f.preset_selector.next,
                    _ => return None,
                }
            }
            Effect::Eq => {
                let l = EqualizerLayout::new(&scope, bounds);
                match control {
                    "eq_on" => l.on,
                    "eq_low_mode" => l.low_mode,
                    "eq_band_mode" => l.band_mode,
                    "eq_high_mode" => l.high_mode,
                    "eq_low_cutoff" | "eq_band_cutoff" | "eq_high_cutoff" => l.cutoff,
                    "eq_low_resonance" | "eq_band_resonance" | "eq_high_resonance" => l.resonance,
                    "eq_low_gain" | "eq_band_gain" | "eq_high_gain" => l.gain,
                    "selected_band" => l.selected_band,
                    _ => return None,
                }
            }
            Effect::Reverb => {
                let l = ReverbLayout::new(&scope, bounds);
                match control {
                    "reverb_on" => l.on,
                    "selected_band" => l.selected_band,
                    "reverb_pre_low_cutoff" => l.low_pre_cutoff,
                    "reverb_pre_high_cutoff" => l.high_pre_cutoff,
                    "reverb_low_shelf_cutoff" | "reverb_high_shelf_cutoff" => l.cutoff,
                    "reverb_low_shelf_gain" | "reverb_high_shelf_gain" => l.gain,
                    "reverb_chorus_amount" => l.chorus_amount,
                    "reverb_chorus_frequency" => l.chorus_frequency,
                    "reverb_delay" => l.delay,
                    "reverb_size" => l.size,
                    "reverb_dry_wet" => l.dry_wet,
                    "reverb_decay_time" => l.decay_time,
                    _ => return None,
                }
            }
            Effect::Compressor => {
                let l = CompressorLayout::new(&scope, bounds);
                match control {
                    "compressor_on" => l.on,
                    "compressor_enabled_bands" => l.enabled_bands,
                    "compressor_mix" => l.mix,
                    "compressor_attack" => l.attack,
                    "compressor_release" => l.release,
                    "compressor_low_gain" => l.low_gain,
                    "compressor_band_gain" => l.band_gain,
                    "compressor_high_gain" => l.high_gain,
                    _ => return None,
                }
            }
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
        })
    }
}

fn corpus() -> PathBuf {
    repo().join("migration/corpus/gui")
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
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
        "header_section/Exit" => header.exit_temporary,
        "header_section/Temporary" => header.temporary,
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
        _ if path.starts_with("extra_mod_section/") => return extra_mod_rect(skin, layout, path),
        _ if path.starts_with("master_controls/") => return advanced_rect(skin, layout, path),
        _ if path.starts_with("MODULATION MATRIX/") => return matrix_rect(skin, layout, path),
        _ if path.starts_with("preset_browser") => return browser_rect(skin, layout, path),
        _ if path.starts_with("oscillator 1") => {
            return wavetable_editor_rect(skin, layout, path, preset)
        }
        _ => return synthesis_rect(skin, layout, path, phase_visible(preset)),
    })
}

/// The Matrix tab's list and rows (`ModulationMatrix::resized`,
/// `setRowPositions`, `ModulationMatrixRow::resized`), laid out with the
/// init preset: no connections, so rows in slot order and one shown.
fn matrix_rect(skin: &Skin, layout: &FullLayout, path: &str) -> Option<Rect> {
    use sas_gui::sections::matrix::{self, MatrixLayout, RowLayout};
    let top = Scope::top(skin, layout.size_ratio);
    let scope = top.within(matrix::SKIN);
    let tab = MatrixLayout::new(&scope, layout.main);
    let rest = path.strip_prefix("MODULATION MATRIX/Container")?;
    if rest.is_empty() {
        let viewport = tab.viewport;
        return Some(Rect::new(
            viewport.x,
            viewport.y,
            viewport.width,
            tab.list_height(1),
        ));
    }
    let rest = rest.strip_prefix("/MOD ")?;
    let (index, control) = rest.split_once('/').unwrap_or((rest, ""));
    let slot: usize = index.parse().ok()?;
    let row = RowLayout::new(&scope, tab.row(slot, 0));
    let number = slot + 1;
    let control = control
        .strip_prefix(&format!("modulation_{number}_"))
        .unwrap_or(control);
    Some(match control {
        "" => row.bounds,
        "source" => row.source,
        "destination" => row.destination,
        "amount" => row.amount,
        "power" => row.power,
        "bipolar" => row.bipolar,
        "stereo" => row.stereo,
        "bypass" => row.bypass,
        _ => return None,
    })
}

/// The preset browser's parts (`PresetBrowser::resized`).
fn browser_rect(skin: &Skin, layout: &FullLayout, path: &str) -> Option<Rect> {
    let top = Scope::top(skin, layout.size_ratio);
    let browser = PresetBrowserLayout::new(&top, layout.browser);
    Some(match path {
        "preset_browser" => browser.bounds,
        "preset_browser/Preset List" => browser.list,
        "preset_browser/Selection List" => browser.folders,
        "preset_browser/Preset" => browser.preset_text,
        "preset_browser/Author" => browser.author_text,
        "preset_browser/Search" => browser.search,
        "preset_browser/Comments" => browser.comments,
        _ => {
            let name = path.strip_prefix("preset_browser/")?;
            let index = PRESET_STYLE_NAMES.iter().position(|style| *style == name)?;
            browser.styles[index]
        }
    })
}

/// The save section's parts for a preset (`SaveSection::setSaveBounds`),
/// over the whole window (`FullInterface::resized`'s `bounds`).
fn save_rect(layout: &FullLayout, width: i32, height: i32, path: &str) -> Option<Rect> {
    let bounds = Rect::new(0, 0, width, height);
    let save = SaveLayout::new(layout.size_ratio, bounds, true);
    Some(match path.strip_prefix("save_section/")? {
        "Name" => save.name,
        "Author" => save.author,
        "Comments" => save.comments,
        "Save" => save.save,
        "Cancel" => save.cancel,
        "Preset" => save.name_label,
        "Author#2" => save.author_label,
        "Style" => save.style_label,
        "Comments#2" => save.comments_label,
        name => {
            let index = PRESET_STYLE_NAMES.iter().position(|style| *style == name)?;
            save.styles[index]
        }
    })
}

/// Oscillator 1's wavetable editor (`WavetableEditSection::resized`),
/// with the component list's rows for `preset`'s wavetable (the init
/// preset's one Wave Source without one).
fn wavetable_editor_rect(
    skin: &Skin,
    layout: &FullLayout,
    path: &str,
    preset: Option<&str>,
) -> Option<Rect> {
    let top = Scope::top(skin, layout.size_ratio);
    let bounds = Rect::new(
        layout.bounds.x,
        0,
        layout.bounds.width,
        layout.bounds.height,
    );
    let edit = WavetableEditLayout::new(&top, bounds);
    let overlay = OverlayLayout::wave_source(&top, &edit);
    let mut state = PresetState::new(&InitContent { sample_seed: 0 });
    if let Some(preset) = preset {
        state
            .load_file(&repo().join(preset))
            .unwrap_or_else(|e| panic!("{preset}: {e:?}"));
    }
    let rows = component_rows(&state.wavetables[0]);
    // Each group's "Add Modifier" row follows its components, and "Add
    // Source" follows the last group.
    let mut add_rows = Vec::new();
    let mut row = 0;
    for group in &state.wavetables[0].groups {
        row += group.components.len();
        add_rows.push(row);
        row += 1;
    }
    let source_row = row;
    // A repeated name's later instances are "#2", "#3", ... in tree order.
    let (name, instance) = match path.rsplit_once('#') {
        Some((name, n)) => (name, n.parse::<usize>().ok()?),
        None => (path, 1),
    };
    Some(match name {
        "oscillator 1" => edit.bounds,
        "oscillator 1/Wavetable Organizer" => edit.organizer,
        "oscillator 1/component list" => edit.component_list,
        "oscillator 1/component list/Add Source" => edit.add_button(&top, source_row, false),
        "oscillator 1/component list/Add Modifier" => {
            edit.add_button(&top, *add_rows.get(instance - 1)?, true)
        }
        // `plus_icons_`: one per group, then Add Source's (the
        // seventeenth).
        "oscillator 1/component list/plus" if instance == 17 => {
            edit.add_plus(&top, source_row, false)
        }
        "oscillator 1/component list/plus" => {
            edit.add_plus(&top, *add_rows.get(instance - 1)?, true)
        }
        "oscillator 1/Playhead" => edit.playhead,
        "oscillator 1/Exit" => edit.exit,
        "oscillator 1/preset_selector" => edit.preset.bounds,
        "oscillator 1/preset_selector/Text" => edit.preset.text,
        "oscillator 1/preset_selector/Prev" => edit.preset.previous,
        "oscillator 1/preset_selector/Next" => edit.preset.next,
        "oscillator 1/Menu" => edit.menu,
        "oscillator 1/Settings" => edit.settings,
        "oscillator 1/WAVE SOURCE" => overlay.overlay,
        "oscillator 1/WAVE SOURCE/background" => overlay.controls,
        "oscillator 1/WAVE SOURCE/background/text" => *overlay.titles.get(instance - 1)?,
        "oscillator 1/WAVE SOURCE/Interpolation" => overlay.interpolation,
        "oscillator 1/WAVE SOURCE/wave_source_horizontal_grid" => overlay.grid_x,
        "oscillator 1/WAVE SOURCE/wave_source_vertical_grid" => overlay.grid_y,
        // The horizontal grid's increment and decrement, then the
        // vertical's.
        "oscillator 1/WAVE SOURCE/Increment" => *[
            overlay.grid_x_buttons[0],
            overlay.grid_x_buttons[1],
            overlay.grid_y_buttons[0],
            overlay.grid_y_buttons[1],
        ]
        .get(instance - 1)?,
        _ => {
            // `names_[i]` then `menu_buttons_[i]`, both named i.
            let index: usize = name
                .strip_prefix("oscillator 1/component list/")?
                .parse()
                .ok()?;
            let row = rows.get(index)?;
            match instance {
                1 => edit.component_name(row.row, row.component > 0),
                2 => edit.component_menu(row.row),
                _ => return None,
            }
        }
    })
}

/// The popup browser's parts, open for oscillator 1's wavetables
/// (`OscillatorSection::textMouseDown`, `PopupBrowser::resized`), over the
/// whole window.
fn popup_browser_rect(
    skin: &Skin,
    layout: &FullLayout,
    width: i32,
    height: i32,
    path: &str,
) -> Option<Rect> {
    let top = Scope::top(skin, layout.size_ratio);
    let synthesis = SynthesisLayout::new(&top, layout.main);
    let osc = OscillatorLayout::new(
        &top.within(oscillator::SKIN),
        synthesis.oscillators[0],
        false,
    );
    let ratio = layout.size_ratio;
    let browser = Rect::new(
        osc.unison_voices.x,
        osc.preset_selector.bounds.y,
        (600.0 * ratio) as i32,
        (400.0 * ratio) as i32,
    );
    let scope = top.within(popup_browser::SKIN);
    let parts = PopupBrowserLayout::new(ratio, scope.value(ValueId::Padding), browser);
    let areas = popup_browser::closing_areas(Rect::new(0, 0, width, height), osc.bounds);
    Some(match path.strip_prefix("Popup Browser/")? {
        "Ignore Area" => areas[0],
        "Ignore Area#2" => areas[1],
        "Ignore Area#3" => areas[2],
        "Ignore Area#4" => areas[3],
        "Exit" => parts.exit,
        // The download button, where the search box goes.
        "Login" => parts.search,
        "Selection List" => parts.folders,
        "Selection List#2" => parts.files,
        _ => return None,
    })
}

/// The macro and wheel source column.
fn extra_mod_rect(skin: &Skin, layout: &FullLayout, path: &str) -> Option<Rect> {
    let top = Scope::top(skin, layout.size_ratio);
    let e = ExtraModLayout::new(&top, layout.extra_mod);
    let rest = path.strip_prefix("extra_mod_section/")?;
    let wheels =
        ModulationTabSelector::numbered(&["pitch_wheel", "mod_wheel"]).button_bounds(e.other, 2);
    Some(match rest {
        "OTHER" => e.other,
        "OTHER/pitch_wheel" => wheels[0],
        "OTHER/mod_wheel" => wheels[1],
        "MACRO" => e.macros_area,
        _ => {
            let rest = rest.strip_prefix("MACRO/MACRO")?;
            let (index, control) = rest.split_at(1);
            let m = e.macros[index.parse::<usize>().ok()?];
            let control = control.strip_prefix('/');
            match control {
                None => m.bounds,
                // The knob and the source button share the macro's name.
                Some(c) if c.starts_with("macro_control_") && c.ends_with("#2") => m.source,
                Some(c) if c.starts_with("macro_control_") => m.knob,
                Some(c) if c.starts_with("Macro Label ") => m.label,
                Some(c) if c.starts_with("Edit ") => m.edit,
                _ => return None,
            }
        }
    })
}

/// G6c: the Advanced tab's settings panels.
fn advanced_rect(skin: &Skin, layout: &FullLayout, path: &str) -> Option<Rect> {
    use sas_gui::sections::advanced::{
        self, AdvancedLayout, DisplaySettingsLayout, VoiceSettingsLayout,
    };
    let top = Scope::top(skin, layout.size_ratio);
    let tab = AdvancedLayout::new(&top, layout.main);
    let scope = top.within(advanced::SKIN);
    let voice = VoiceSettingsLayout::new(&scope, tab.voice);
    let display = DisplaySettingsLayout::new(&scope, tab.display);
    let rest = path.strip_prefix("master_controls/")?;
    if let Some(rest) = rest.strip_prefix("OSC ") {
        use sas_gui::sections::oscillator_advanced::OscillatorAdvancedLayout;
        let (number, rest) = rest.split_at(1);
        let index = number.parse::<usize>().ok()? - 1;
        let l = OscillatorAdvancedLayout::new(&scope, tab.oscillators[index]);
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        let (section, control) = rest.split_once('/').unwrap_or((rest, ""));
        let prefix = format!("osc_{number}_");
        return Some(
            match (section, control.strip_prefix(&prefix).unwrap_or(control)) {
                ("", _) => l.bounds,
                (s, "") if s.ends_with("OPTIONS") => l.options,
                (s, "") if s.ends_with("UNISON") => l.unison,
                (_, "midi_track") => l.midi_track,
                (_, "smooth_interpolation") => l.smooth_interpolation,
                (_, "stack_style") => l.stack_style,
                (_, "unison_blend") => l.unison_blend,
                (_, "detune_range") => l.detune_range,
                (_, "frame_spread") => l.frame_spread,
                (_, "distortion_spread") => l.distortion_spread,
                (_, "spectral_morph_spread") => l.spectral_morph_spread,
                (_, "stereo_spread") => l.stereo_spread,
                _ => return None,
            },
        );
    }
    Some(match rest {
        "VOICE" => tab.voice,
        "VOICE/mpe_enabled" => voice.mpe_enabled,
        "VOICE/voice_priority" => voice.voice_priority,
        "VOICE/voice_override" => voice.voice_override,
        "VOICE/tuning" => voice.tuning,
        "VOICE/voice_tune" => voice.voice_tune,
        "VOICE/voice_transpose" => voice.voice_transpose,
        "OVERSAMPLING" => tab.oversample,
        "DISPLAY" => tab.display,
        "DISPLAY/frequency_display" => display.frequency_display,
        "DISPLAY/skin" => display.skin,
        "ANALYSIS" => tab.output,
        _ => return None,
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
    if let Some(rest) = rest.strip_prefix("FILTER ") {
        let (number, control) = rest.split_at(1);
        let n: usize = number.parse().ok()?;
        let control = control.strip_prefix('/')?;
        let bounds = [synthesis.filter_1, synthesis.filter_2][n - 1];
        let f = FilterLayout::new(&top.within(filter::SKIN), bounds, Kind::Voice(n));
        let inputs = f.inputs?;
        let prefix = format!("filter_{n}_");
        return Some(match control.strip_prefix(&prefix).unwrap_or(control) {
            "cutoff" => f.cutoff,
            "resonance" => f.resonance,
            "drive" => f.drive,
            "mix" => f.mix,
            "blend" => f.blend,
            "keytrack" => f.keytrack,
            "on" => sas_gui::section::power_button_bounds(&top.within(filter::SKIN), bounds),
            "filter_input" => inputs[4],
            "label1" => f.drive_label,
            "label2" => f.keytrack_label,
            "preset_selector" => f.preset_selector.bounds,
            "preset_selector/Text" => f.preset_selector.text,
            "preset_selector/Prev" => f.preset_selector.previous,
            "preset_selector/Next" => f.preset_selector.next,
            "OSC1" => inputs[0],
            "OSC2" => inputs[1],
            "OSC3" => inputs[2],
            "SMP" => inputs[3],
            _ => return None,
        });
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
            let actual = match preset {
                Some(effects::ALL_EFFECTS) => effects::all_effects_rect(&skin, &layout, path),
                _ if path.starts_with("save_section/") => save_rect(&layout, width, height, path),
                _ if entry.get("overlay").and_then(|o| o.as_str()) == Some("popup_browser") => {
                    popup_browser_rect(&skin, &layout, width, height, path)
                }
                _ => port_rect(&skin, &layout, path, preset),
            }
            .unwrap_or_else(|| panic!("{}: no port rectangle for {path}", file.display()));
            assert_eq!(actual, expected, "{path} at {width}x{height}");
            checked += 1;
        }
    }
    assert!(checked > 0);
}
