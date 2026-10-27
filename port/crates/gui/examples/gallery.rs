//! Renders the port's editor for every view in
//! `migration/corpus/gui/views.json` to `OUT/<name>.png`, the counterpart of
//! `migration/scripts/gui_reference.py frames`.
//!
//!     cargo run -p sas-gui --example gallery -- OUT [--scale 2]
//!
//! Uses wgpu on whatever adapter the machine has (Mesa's lavapipe in CI).
//!
//! A view with `"midi"` or `"seconds"` runs the engine first, as `ref-gui`
//! does for the upstream frame: the view's preset and MIDI through
//! `sas_engine::synth::render` at 44.1 kHz and 120 BPM, in 64-sample host
//! blocks. The editor then reads that engine's telemetry and held notes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui_kittest::Harness;
use nice_plug::prelude::{FloatParam, FloatRange, IntParam, IntRange, Param};
use sas_engine::edit_bus::PreparedWavetable;
use sas_engine::keyboard::keyboard;
use sas_engine::midi_file::{read_midi_file, TimedMessage};
use sas_engine::sas_oscillators::sas_wavetable::creator::WavetableCreator;
use sas_engine::sas_oscillators::sas_wavetable::Wavetable;
use sas_engine::sas_oscillators::RandomValues;
use sas_engine::synth::{render, RenderSettings, Synth};
use sas_engine::telemetry::Telemetry;
use sas_gui::sections::wavetable_edit::KeyframeRef;
use sas_gui::{EditorModel, Host, HostParam, Interface, Route, Tab};
use sas_preset::registry::{registry, ValueScale};
use sas_preset::{InitContent, PresetState, SampleState};

enum ViewParam {
    Float(FloatParam),
    Int(IntParam),
}

impl ViewParam {
    fn ptr(&self) -> HostParam {
        match self {
            ViewParam::Float(param) => param.as_ptr(),
            ViewParam::Int(param) => param.as_ptr(),
        }
    }
}

/// A preset's controls as host parameters (the init preset's without
/// one), made the way the plugin makes them: indexed controls are
/// integers, the rest linear over the registry range.
struct ViewPreset {
    name: String,
    params: HashMap<String, Box<ViewParam>>,
    state: PresetState,
    /// The preset's wavetables, rendered as the engine renders them.
    wavetables: Vec<Option<Arc<Wavetable>>>,
    /// What the view's engine left, when the view runs one.
    live: Option<Live>,
}

/// `ref-gui`'s sample rate, tempo and block size for a running engine
/// (`renderLive`, `migration/reference/gui/src/gui_main.cpp`).
const LIVE_SAMPLE_RATE: i32 = 44100;
const LIVE_BPM: f64 = 120.0;
const LIVE_BLOCK_SIZE: usize = 64;
/// The authored MIDI a view may play (`gui_reference.py`'s `MIDI_DIR`).
const MIDI_DIR: &str = "migration/corpus/midi/";

/// The engine's state after a view's render: its telemetry snapshot and the
/// notes still held (bit n is note n).
struct Live {
    telemetry: Telemetry,
    held_notes: u128,
}

impl Live {
    /// Renders `seconds` of `events` (default: up to the last event) on a
    /// synth with `state` loaded. None for an empty render.
    fn render(state: &PresetState, events: &[TimedMessage], seconds: Option<f64>) -> Option<Live> {
        let last_event = events.iter().map(|e| e.seconds).fold(0.0f64, f64::max);
        let seconds = seconds.unwrap_or(last_event);
        let frames = (seconds * LIVE_SAMPLE_RATE as f64).ceil() as usize;
        if frames == 0 {
            return None;
        }
        // ref-gui builds one engine per process, and the first engine of a
        // process takes a seed for RandomValues: take it for every view's
        // engine, or each one after the first starts its random sources a
        // seed early.
        RandomValues::reset_process_state();
        let mut synth = Synth::new();
        synth.load_preset(state);
        let settings = RenderSettings {
            sample_rate: LIVE_SAMPLE_RATE,
            block_size: LIVE_BLOCK_SIZE,
            bpm: LIVE_BPM,
            frames,
        };
        render(&mut synth, events, &settings);
        let (mut writer, telemetry) =
            Telemetry::with_equalizer_memory(synth.audio_memory(), synth.equalizer_memory());
        synth.write_telemetry(&mut writer);

        // The plugin's keyboard state (MidiKeyboardState) over the events
        // the render played.
        let (keyboard, mut consumer) = keyboard();
        for event in events {
            let sample = (event.seconds * LIVE_SAMPLE_RATE as f64 + 0.5).floor() as i64;
            if sample >= frames as i64 {
                break;
            }
            consumer.host_message(event.message);
        }
        let held_notes = keyboard.held_notes();
        Some(Live {
            telemetry,
            held_notes,
        })
    }
}

impl ViewPreset {
    fn new(preset: Option<&Path>) -> ViewPreset {
        let mut state = PresetState::new(&InitContent { sample_seed: 0 });
        let mut name = String::new();
        if let Some(path) = preset {
            state
                .load_file(path)
                .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
            name = path.file_stem().unwrap().to_string_lossy().to_string();
        }
        let params = state
            .controls()
            .iter()
            .filter_map(|(control, &value)| {
                let info = registry().get(control)?;
                let param = if info.value_scale == ValueScale::Indexed {
                    ViewParam::Int(IntParam::new(
                        control.clone(),
                        value.round() as i32,
                        IntRange::Linear {
                            min: info.min as i32,
                            max: info.max as i32,
                        },
                    ))
                } else {
                    ViewParam::Float(FloatParam::new(
                        control.clone(),
                        value,
                        FloatRange::Linear {
                            min: info.min,
                            max: info.max,
                        },
                    ))
                };
                Some((control.clone(), Box::new(param)))
            })
            .collect();
        let wavetables = state
            .wavetables
            .iter()
            .map(|creator| {
                PreparedWavetable::render(creator.clone())
                    .ok()
                    .map(|prepared| prepared.wavetable().clone())
            })
            .collect();
        ViewPreset {
            name,
            params,
            state,
            wavetables,
            live: None,
        }
    }
}

impl EditorModel for ViewPreset {
    fn preset_name(&self) -> String {
        self.name.clone()
    }

    /// The authored presets ref-gui's views read too
    /// (`gui_reference.py`'s `--data-dir`).
    fn data_directory(&self) -> Option<PathBuf> {
        Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migration/corpus/gui/data"))
    }

    fn modulations(&self) -> Arc<[Route]> {
        self.state
            .connection_order()
            .iter()
            .map(|&slot| {
                let connection = &self.state.modulations()[slot];
                Route {
                    slot,
                    source: connection.source.clone(),
                    destination: connection.destination.clone(),
                }
            })
            .collect()
    }

    fn param(&self, name: &str) -> Option<HostParam> {
        self.params.get(name).map(|param| param.ptr())
    }

    fn wavetable(&self, oscillator: usize) -> Option<Arc<Wavetable>> {
        self.wavetables.get(oscillator).cloned().flatten()
    }

    fn wavetable_creator(&self, oscillator: usize) -> Option<WavetableCreator> {
        self.state.wavetables.get(oscillator).cloned()
    }

    fn wavetable_name(&self, oscillator: usize) -> String {
        self.state
            .wavetables
            .get(oscillator)
            .map(|creator| creator.name.clone())
            .unwrap_or_default()
    }

    fn with_sample(&self, f: &mut dyn FnMut(&SampleState)) {
        f(&self.state.sample);
    }

    fn sample_name(&self) -> String {
        self.state.sample.name.clone()
    }

    fn telemetry(&self) -> Option<Telemetry> {
        self.live.as_ref().map(|live| live.telemetry.clone())
    }

    fn held_notes(&self) -> u128 {
        self.live.as_ref().map_or(0, |live| live.held_notes)
    }
}

/// A view's running engine (`"midi"`, `"seconds"`, `"animate"`), checked as
/// `gui_reference.py`'s `live_args` checks it for the upstream frame.
fn live_view(
    name: &str,
    view: &serde_json::Value,
    repo: &Path,
    state: &PresetState,
) -> Option<Live> {
    let midi = view.get("midi").and_then(|v| v.as_str());
    let seconds = view.get("seconds").map(|v| {
        v.as_f64()
            .filter(|s| *s >= 0.0)
            .unwrap_or_else(|| panic!("{name}: seconds must be a number >= 0"))
    });
    let animate = view
        .get("animate")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if midi.is_none() && seconds.is_none() {
        assert!(!animate, "{name}: animate needs midi or seconds");
        return None;
    }
    let events = match midi {
        Some(midi) => {
            assert!(
                midi.starts_with(MIDI_DIR) && !midi.split('/').any(|part| part == ".."),
                "{name}: MIDI {midi} is not in {MIDI_DIR}"
            );
            let data =
                std::fs::read(repo.join(midi)).unwrap_or_else(|e| panic!("{name}: {midi}: {e}"));
            read_midi_file(&data).unwrap_or_else(|e| panic!("{name}: {midi}: {e}"))
        }
        None => Vec::new(),
    };
    let live = Live::render(state, &events, seconds);
    assert!(
        live.is_some() || !animate,
        "{name}: animate needs a render that isn't empty"
    );
    if live.is_some() && !animate {
        // The editor has no animate switch yet: its animated views always
        // read the telemetry, while ref-gui draws them at rest.
        println!("{name}: no animate switch in the port; animated views read the engine");
    }
    live
}

/// `ref-gui`'s default `--frames`.
const REF_GUI_FRAMES: usize = 30;

fn tab(name: &str) -> Tab {
    match name {
        "voice" => Tab::Voice,
        "effects" => Tab::Effects,
        "matrix" => Tab::Matrix,
        "advanced" => Tab::Advanced,
        _ => panic!("unknown tab {name}"),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("usage: gallery OUT [--scale S]"));
    let mut scale = 1.0f32;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scale" => {
                scale = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .expect("--scale needs a number")
            }
            _ => panic!("unknown argument {arg}"),
        }
    }
    std::fs::create_dir_all(&out).unwrap();

    let views_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migration/corpus/gui/views.json");
    let views: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&views_path).unwrap()).unwrap();
    for view in views["views"].as_array().unwrap() {
        let name = view["name"].as_str().unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let preset_path = view
            .get("preset")
            .and_then(|p| p.as_str())
            .map(|p| repo.join(p));
        let mut model = ViewPreset::new(preset_path.as_deref());
        model.live = live_view(name, view, &repo, &model.state);
        let live = model.live.is_some();
        let width = view.get("width").and_then(|v| v.as_f64()).unwrap_or(1400.0) as f32;
        let height = view.get("height").and_then(|v| v.as_f64()).unwrap_or(820.0) as f32;
        let interface = RefCell::new(Interface::default());
        interface.borrow_mut().set_tab(tab(view
            .get("tab")
            .and_then(|v| v.as_str())
            .unwrap_or("voice")));
        match view.get("overlay").and_then(|v| v.as_str()) {
            Some("preset_browser") => interface.borrow_mut().set_preset_browser_visible(true),
            Some("save_section") => interface.borrow_mut().request_save_preset(),
            Some("wavetable_editor") => interface.borrow_mut().show_wavetable_editor(Some(0)),
            Some("popup_browser") => interface.borrow_mut().request_wavetable_browser(0),
            _ => {}
        }
        if let Some(select) = view.get("wavetable_select").and_then(|v| v.as_str()) {
            interface
                .borrow_mut()
                .select_wavetable_keyframe(0, wavetable_select(select));
        }
        if let Some(scroll) = view.get("effects_scroll").and_then(|v| v.as_f64()) {
            interface
                .borrow_mut()
                .set_effects_scroll(scroll as f32 * scale);
        }

        let state = egui_kittest::wgpu::create_render_state(
            egui_kittest::wgpu::default_wgpu_setup(),
            egui_wgpu::RendererOptions::PREDICTABLE,
        );
        sas_gui::primitives::publish_target(&state, 1);
        let mut harness = Harness::builder()
            .with_size((width, height))
            .with_pixels_per_point(scale)
            .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
            .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
                state,
            ))
            .build_ui(|ui| {
                let screen = ui.ctx().content_rect();
                let host = Host::new(&model, None);
                interface.borrow_mut().show_in(ui, screen, &host)
            });
        // ref-gui draws 30 frames (`--frames`) before it reads one back, so
        // an animated view's decays have run as long in both.
        harness.run_steps(if live { REF_GUI_FRAMES } else { 2 });
        let image = harness
            .render()
            .unwrap_or_else(|e| panic!("{name}: render failed: {e}"));
        let path = out.join(format!("{name}.png"));
        image.save(&path).unwrap();
        println!("wrote {}", path.display());
    }
}

/// A view's `wavetable_select`, "group:component[:keyframe]".
fn wavetable_select(text: &str) -> KeyframeRef {
    let parts: Vec<usize> = text
        .split(':')
        .map(|part| part.parse().expect("wavetable_select takes whole numbers"))
        .collect();
    assert!(
        parts.len() == 2 || parts.len() == 3,
        "wavetable_select takes group:component[:keyframe]"
    );
    KeyframeRef {
        group: parts[0],
        component: parts[1],
        keyframe: parts.get(2).copied().unwrap_or(0),
    }
}
