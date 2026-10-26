// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor drawn on a real wgpu device with the render state the plugin
//! window creates: egui-baseview's `create_render_state` (what its
//! `Renderer::new` calls) with nice-plug-egui's default `GraphicsConfig`,
//! whose `msaa_samples` is 0. Every tab is drawn inside a validation error
//! scope, and the shader-drawn primitives' pipelines must be built. Built
//! with the `gpu-tests` feature: Mesa's lavapipe in CI's GUI reference job,
//! Metal in the macOS VST3 job.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use egui_kittest::Harness;
use egui_wgpu::wgpu;
use nice_plug::prelude::{FloatParam, FloatRange, Param};
use nice_plug_egui::{create_render_state, GraphicsConfig, WgpuErrors, WgpuTarget};
use sas_gui::{EditorModel, Host, HostParam, Interface, Tab};
use sas_preset::registry::registry;

/// Every control at its default, linear over its registry range.
struct Synth {
    params: BTreeMap<String, FloatParam>,
}

impl Synth {
    fn new() -> Synth {
        let params = registry()
            .parameters()
            .iter()
            .map(|info| {
                let range = FloatRange::Linear {
                    min: info.min,
                    max: info.max,
                };
                let param = FloatParam::new(info.name.clone(), info.default_value, range);
                (info.name.clone(), param)
            })
            .collect();
        Synth { params }
    }
}

impl EditorModel for Synth {
    fn preset_name(&self) -> String {
        "Init".to_owned()
    }

    fn param(&self, name: &str) -> Option<HostParam> {
        self.params.get(name).map(|param| param.as_ptr())
    }
}

/// The plugin window's render state, without the window's surface.
fn window_render_state() -> egui_wgpu::RenderState {
    let config = GraphicsConfig::default();
    let instance = pollster::block_on(config.wgpu_options.wgpu_setup.new_instance());
    pollster::block_on(create_render_state(&config, &instance, None))
        .expect("a wgpu adapter and device")
}

fn device_errors(state: &egui_wgpu::RenderState) -> usize {
    state
        .renderer
        .read()
        .callback_resources
        .get::<WgpuErrors>()
        .expect("create_render_state publishes WgpuErrors")
        .count()
}

#[test]
fn the_window_target_is_single_sampled_with_the_default_config() {
    assert_eq!(GraphicsConfig::default().renderer_options.msaa_samples, 0);
    let state = window_render_state();
    let target = *state
        .renderer
        .read()
        .callback_resources
        .get::<WgpuTarget>()
        .expect("create_render_state publishes WgpuTarget");
    assert_eq!(target, WgpuTarget::new(state.target_format, 1));
}

#[test]
fn every_tab_draws_without_device_errors() {
    let state = window_render_state();
    let info = state.adapter.get_info();
    println!("adapter: {} ({:?})", info.name, info.backend);

    let model = Synth::new();
    let interface = RefCell::new(Interface::default());
    let tab = Cell::new(Tab::Voice);
    let mut harness = Harness::builder()
        .with_size((1400.0, 820.0))
        .with_pixels_per_point(2.0)
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
            state.clone(),
        ))
        .build_ui(|ui| {
            let host = Host::new(&model, None);
            let mut interface = interface.borrow_mut();
            interface.set_tab(tab.get());
            interface.show(ui, &host);
        });

    let scope = state.device.push_error_scope(wgpu::ErrorFilter::Validation);
    for each in [Tab::Voice, Tab::Effects, Tab::Matrix, Tab::Advanced] {
        tab.set(each);
        harness.run_steps(2);
        harness
            .render()
            .unwrap_or_else(|e| panic!("{each:?}: render failed: {e}"));
    }
    let error = pollster::block_on(scope.pop());
    assert!(error.is_none(), "validation error: {}", error.unwrap());
    assert_eq!(device_errors(&state), 0);
    assert!(
        sas_gui::primitives::custom_drawing_ready(&state),
        "the quad and line pipelines are built and valid for the window's target"
    );
}

/// A device error outside any scope, here the original sample-count bug,
/// is logged and counted instead of panicking (wgpu's default), and the
/// shader-drawn primitives stop drawing.
#[test]
fn a_device_error_is_counted_and_turns_custom_drawing_off() {
    let state = window_render_state();
    let shader = state
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("invalid on purpose"),
            source: wgpu::ShaderSource::Wgsl(
                "@vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0); }
                 @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0); }"
                    .into(),
            ),
        });
    let _ = state
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("invalid on purpose"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(state.target_format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 0,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        });
    assert!(device_errors(&state) > 0);
    assert!(!sas_gui::primitives::custom_drawing_ready(&state));
}
