//! Renders the port's editor for every view in
//! `migration/corpus/gui/views.json` to `OUT/<name>.png`, the counterpart of
//! `migration/scripts/gui_reference.py frames`.
//!
//!     cargo run -p sas-gui --example gallery -- OUT [--scale 2]
//!
//! Uses wgpu on whatever adapter the machine has (Mesa's lavapipe in CI).

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use sas_gui::{EditorModel, Interface, Tab};

struct InitPreset;

impl EditorModel for InitPreset {
    fn preset_name(&self) -> String {
        String::new()
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
        if view.get("preset").is_some_and(|p| !p.is_null()) {
            panic!("{name}: presets are not supported by the gallery yet");
        }
        let width = view.get("width").and_then(|v| v.as_f64()).unwrap_or(1400.0) as f32;
        let height = view.get("height").and_then(|v| v.as_f64()).unwrap_or(820.0) as f32;
        let interface = RefCell::new(Interface::default());
        interface.borrow_mut().set_tab(tab(view
            .get("tab")
            .and_then(|v| v.as_str())
            .unwrap_or("voice")));

        let mut harness = Harness::builder()
            .with_size((width, height))
            .with_pixels_per_point(scale)
            .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
            .wgpu()
            .build_ui(|ui| {
                let screen = ui.ctx().content_rect();
                interface.borrow_mut().show_in(ui, screen, &InitPreset)
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
