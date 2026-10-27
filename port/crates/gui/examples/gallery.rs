//! Renders the port's editor for every view in
//! `migration/corpus/gui/views.json` to `OUT/<name>.png`, the counterpart of
//! `migration/scripts/gui_reference.py frames`.
//!
//!     cargo run -p sas-gui --example gallery -- OUT [--scale 2]
//!
//! Uses wgpu on whatever adapter the machine has (Mesa's lavapipe in CI).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui_kittest::Harness;
use nice_plug::prelude::{FloatParam, FloatRange, IntParam, IntRange, Param};
use sas_engine::edit_bus::PreparedWavetable;
use sas_engine::sas_oscillators::sas_wavetable::Wavetable;
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
}

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
        let model = ViewPreset::new(preset_path.as_deref());
        let width = view.get("width").and_then(|v| v.as_f64()).unwrap_or(1400.0) as f32;
        let height = view.get("height").and_then(|v| v.as_f64()).unwrap_or(820.0) as f32;
        let interface = RefCell::new(Interface::default());
        interface.borrow_mut().set_tab(tab(view
            .get("tab")
            .and_then(|v| v.as_str())
            .unwrap_or("voice")));
        if view.get("overlay").and_then(|v| v.as_str()) == Some("preset_browser") {
            interface.borrow_mut().set_preset_browser_visible(true);
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
        harness.run_steps(2);
        let image = harness
            .render()
            .unwrap_or_else(|e| panic!("{name}: render failed: {e}"));
        let path = out.join(format!("{name}.png"));
        image.save(&path).unwrap();
        println!("wrote {}", path.display());
    }
}
