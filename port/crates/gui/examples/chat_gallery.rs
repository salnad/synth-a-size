//! Renders the chat panel over the editor in three states, for review:
//! `chat-off.png` (the chat not yet turned on), `chat-confirm.png` (a
//! conversation with an edit held for confirmation) and
//! `chat-settings.png` (the settings form opened from a working chat), and
//! the conversation in a small window (`chat-small.png`, 350×205) and a
//! letterboxed one (`chat-letterboxed.png`, 1400×700).
//!
//!     cargo run -p sas-gui --example chat_gallery -- OUT [--scale 2]
//!
//! The host is a fake with fixed content in the format the plugin's chat
//! writes. Uses wgpu on whatever adapter the machine has.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use sas_gui::chat::{ChatLine, LineKind};
use sas_gui::{
    ChatHost, ChatPanel, ChatSettingsView, ChatView, EditorModel, FocusMode, Host, Interface,
};

struct InitPreset;

impl EditorModel for InitPreset {
    fn preset_name(&self) -> String {
        String::new()
    }
}

struct FixedHost {
    view: ChatView,
    settings: ChatSettingsView,
}

impl ChatHost for FixedHost {
    fn run_frame(&self) {}
    fn view(&self) -> ChatView {
        self.view.clone()
    }
    fn send(&self, _text: String) {}
    fn stop(&self) {}
    fn undo(&self) {}
    fn compare(&self) {}
    fn confirm(&self, _accept: bool) {}
    fn settings(&self) -> ChatSettingsView {
        self.settings.clone()
    }
    fn save_settings(&self, _: ChatSettingsView, _: Option<String>) -> Result<(), String> {
        Ok(())
    }
}

fn line(kind: LineKind, text: &str) -> ChatLine {
    ChatLine {
        kind,
        text: text.to_string(),
    }
}

fn off() -> FixedHost {
    FixedHost {
        view: ChatView::default(),
        settings: ChatSettingsView {
            provider: "anthropic".into(),
            ..ChatSettingsView::default()
        },
    }
}

fn conversation() -> FixedHost {
    let lines = vec![
        line(LineKind::User, "Make it a soft glassy pad, slow attack."),
        line(
            LineKind::Assistant,
            "Starting from a saw: I'll open the filter a little, slow the amp envelope and \
             add a gentle LFO on the cutoff.",
        ),
        line(
            LineKind::Tool,
            r#"set_params {"changes":[{"name":"env_1_attack","value":1.2},{"name":"env_1_release","value":2.5},{"name":"filter_1_on","value":"On"},{"name":"fi..."#,
        ),
        line(
            LineKind::Tool,
            r#"connect_modulation {"source":"lfo_1","destination":"filter_1_cutoff","amount":0.3,"bipolar":true}"#,
        ),
        line(LineKind::Tool, r#"set_lfo_shape {"lfo":1,"shape":"sine"}"#),
        line(
            LineKind::Assistant,
            "Done: 1.2 s attack, 2.5 s release, cutoff at 70 with a slow bipolar sine sweep.",
        ),
        line(LineKind::User, "Nice. Much louder please."),
        line(
            LineKind::Tool,
            r#"set_params {"changes":[{"name":"volume","value":4}]}"#,
        ),
        line(
            LineKind::Assistant,
            "Raising the master volume to +4 dB goes above 0 dB, so it needs your OK.",
        ),
    ];
    FixedHost {
        view: ChatView {
            enabled: true,
            lines,
            can_undo: true,
            can_compare: true,
            pending: Some("volume would rise to 4.00 dB, above 0 dB".into()),
            ..ChatView::default()
        },
        settings: ChatSettingsView {
            enabled: true,
            provider: "anthropic".into(),
            has_key: true,
            ..ChatSettingsView::default()
        },
    }
}

fn settings() -> FixedHost {
    FixedHost {
        view: ChatView {
            enabled: true,
            ..ChatView::default()
        },
        settings: ChatSettingsView {
            enabled: true,
            provider: "openai".into(),
            model: "qwen2.5:14b".into(),
            base_url: "http://localhost:11434/v1".into(),
            has_key: true,
            focus: FocusMode::GrabOnClick,
        },
    }
}

fn render(
    out: &Path,
    name: &str,
    size: (f32, f32),
    scale: f32,
    host: FixedHost,
    open_settings: bool,
) {
    let interface = RefCell::new(Interface::default());
    let panel = RefCell::new(ChatPanel::default());
    panel.borrow_mut().set_open(true);
    let state = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    sas_gui::primitives::publish_target(&state, 1);
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(scale)
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
            state,
        ))
        .build_ui(|ui| {
            let screen = ui.ctx().content_rect();
            let editor_host = Host::new(&InitPreset, None);
            interface.borrow_mut().show_in(ui, screen, &editor_host);
            let placement = interface
                .borrow()
                .chat_placement(screen, ui.ctx().pixels_per_point());
            panel
                .borrow_mut()
                .show(ui.ctx(), &placement, &host, &mut || {});
        });
    harness.run_steps(2);
    if open_settings {
        harness.get_by_label("Settings").click();
        harness.run_steps(3);
    }
    let image = harness
        .render()
        .unwrap_or_else(|e| panic!("{name}: render failed: {e}"));
    let path = out.join(format!("{name}.png"));
    image.save(&path).unwrap();
    println!("wrote {}", path.display());
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().expect("usage: chat_gallery OUT [--scale S]"));
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
    let full = (1400.0, 820.0);
    render(&out, "chat-off", full, scale, off(), false);
    render(&out, "chat-confirm", full, scale, conversation(), false);
    render(&out, "chat-settings", full, scale, settings(), true);
    render(
        &out,
        "chat-small",
        (350.0, 205.0),
        scale,
        conversation(),
        false,
    );
    render(
        &out,
        "chat-letterboxed",
        (1400.0, 700.0),
        scale,
        conversation(),
        false,
    );
}
