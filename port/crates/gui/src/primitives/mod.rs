// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Upstream's shader-drawn quads, drawn with wgpu inside egui.
//!
//! Ports `OpenGlMultiQuad` (`vital/src/interface/editor_components/open_gl_multi_quad.{h,cpp}`)
//! and its fragment shaders (`look_and_feel/shaders.cpp`, in `quad.wgsl`).
//! A [`QuadBatch`] is one egui paint callback holding any number of quads,
//! drawn in one instanced draw at its place in egui's paint order. Each quad
//! carries what upstream sets per quad (position, shader values) and per
//! multi-quad (colours, thickness, rounding, thumb amount, start position,
//! maximum arc, alpha).
//!
//! The pipeline is built for the render target the editor window or the test
//! renderer publishes as [`WgpuTarget`] in egui-wgpu's callback resources.

use std::sync::OnceLock;

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;
pub use nice_plug_egui::WgpuTarget;

use crate::colour::Colour;
use crate::paint::{PixelPainter, RectF};

/// The fragment shader a quad uses (`Shaders::FragmentShader`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum QuadKind {
    Color = 0,
    FadeSquare = 1,
    Circle = 2,
    Ring = 3,
    RoundedCorner = 4,
    RoundedRectangle = 5,
    Diamond = 6,
    RoundedRectangleBorder = 7,
    RotarySlider = 8,
    RotaryModulation = 9,
    HorizontalSlider = 10,
    VerticalSlider = 11,
    LinearModulation = 12,
    ModulationKnob = 13,
}

impl QuadKind {
    /// Whether the shader reads an `alpha_mult` uniform. Upstream folds the
    /// alpha into `color` for the others.
    fn has_alpha_mult(self) -> bool {
        matches!(
            self,
            QuadKind::RoundedRectangleBorder
                | QuadKind::RotaryModulation
                | QuadKind::ModulationKnob
        )
    }
}

/// One quad and the uniforms upstream's `OpenGlMultiQuad` would hold for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub kind: QuadKind,
    /// Where it is, in pixels, relative to the editor.
    pub rect: RectF,
    pub shader_values: [f32; 4],
    pub color: Colour,
    pub alt_color: Colour,
    pub thumb_color: Colour,
    pub mod_color: Colour,
    pub background_color: Colour,
    pub thickness: f32,
    /// `setRounding`'s argument; the shader gets twice it.
    pub rounding: f32,
    pub thumb_amount: f32,
    pub start_pos: f32,
    pub max_arc: f32,
    pub alpha_mult: f32,
}

impl Quad {
    /// `OpenGlMultiQuad`'s defaults: shader value 1, thickness 1, shader
    /// rounding 5, thumb amount 0.5, start position 0, maximum arc 2, alpha
    /// 1, and transparent alternate colours.
    pub fn new(kind: QuadKind, rect: RectF, color: Colour) -> Quad {
        Quad {
            kind,
            rect,
            shader_values: [1.0, 1.0, 1.0, 1.0],
            color,
            alt_color: Colour::TRANSPARENT_BLACK,
            thumb_color: Colour::TRANSPARENT_BLACK,
            mod_color: Colour::TRANSPARENT_BLACK,
            background_color: Colour::TRANSPARENT_BLACK,
            thickness: 1.0,
            rounding: 2.5,
            thumb_amount: 0.5,
            start_pos: 0.0,
            max_arc: 2.0,
            alpha_mult: 1.0,
        }
    }

    /// The instance data, positioned in a viewport `viewport` pixels big.
    /// Colours are set the way `OpenGlMultiQuad::render` sets its uniforms.
    fn instance(&self, viewport: [f32; 2]) -> Instance {
        let x = 2.0 * self.rect.x / viewport[0] - 1.0;
        let w = 2.0 * self.rect.width / viewport[0];
        let h = 2.0 * self.rect.height / viewport[1];
        let y = 1.0 - 2.0 * self.rect.y / viewport[1] - h;
        let mut color = rgba(self.color);
        let mut alpha_mult = self.alpha_mult;
        if !self.kind.has_alpha_mult() {
            color[3] *= alpha_mult;
            alpha_mult = 1.0;
        }
        let or_color = |other: Colour| {
            if other.alpha() == 0 {
                let base = rgba(self.color);
                [base[0], base[1], base[2], 0.0]
            } else {
                rgba(other)
            }
        };
        Instance {
            position: [x, y, w, h],
            dimensions: [self.rect.width, self.rect.height],
            shader_values: self.shader_values,
            color,
            alt_color: or_color(self.alt_color),
            thumb_color: rgba(self.thumb_color),
            mod_color: or_color(self.mod_color),
            background_color: or_color(self.background_color),
            params: [
                self.thickness,
                2.0 * self.rounding,
                self.thumb_amount,
                self.start_pos,
            ],
            params2: [self.max_arc, alpha_mult, self.kind as u32 as f32, 0.0],
        }
    }
}

/// `Colour::getFloatRed()` etc.
fn rgba(colour: Colour) -> [f32; 4] {
    [
        colour.red() as f32 / 255.0,
        colour.green() as f32 / 255.0,
        colour.blue() as f32 / 255.0,
        colour.alpha() as f32 / 255.0,
    ]
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Instance {
    position: [f32; 4],
    dimensions: [f32; 2],
    shader_values: [f32; 4],
    color: [f32; 4],
    alt_color: [f32; 4],
    thumb_color: [f32; 4],
    mod_color: [f32; 4],
    background_color: [f32; 4],
    params: [f32; 4],
    params2: [f32; 4],
}

const INSTANCE_FLOATS: usize = 4 + 2 + 4 * 8;

impl Instance {
    fn write(&self, out: &mut Vec<f32>) {
        out.extend_from_slice(&self.position);
        out.extend_from_slice(&self.dimensions);
        for values in [
            &self.shader_values,
            &self.color,
            &self.alt_color,
            &self.thumb_color,
            &self.mod_color,
            &self.background_color,
            &self.params,
            &self.params2,
        ] {
            out.extend_from_slice(values);
        }
    }
}

/// Quads drawn together at one point in egui's paint order.
#[derive(Default)]
pub struct QuadBatch {
    quads: Vec<Quad>,
    additive: bool,
}

impl QuadBatch {
    pub fn new() -> QuadBatch {
        QuadBatch::default()
    }

    /// Quads blended additively (`OpenGlMultiQuad::setAdditive`:
    /// `GL_SRC_ALPHA, GL_ONE`).
    pub fn additive() -> QuadBatch {
        QuadBatch {
            quads: Vec::new(),
            additive: true,
        }
    }

    pub fn push(&mut self, quad: Quad) {
        self.quads.push(quad);
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    /// Adds the batch to `pixels`' painter. Quads are clipped to `clip`
    /// (pixels), as upstream scissors each component.
    pub fn paint(self, pixels: &PixelPainter, clip: RectF) {
        if self.quads.is_empty() {
            return;
        }
        let area = pixels.area();
        let viewport = [
            (area.width() * pixels.pixels_per_point()).round(),
            (area.height() * pixels.pixels_per_point()).round(),
        ];
        let mut data = Vec::with_capacity(self.quads.len() * INSTANCE_FLOATS);
        for quad in &self.quads {
            quad.instance(viewport).write(&mut data);
        }
        let callback = QuadCallback {
            data,
            count: self.quads.len() as u32,
            additive: self.additive,
            buffer: OnceLock::new(),
        };
        let shape = egui_wgpu::Callback::new_paint_callback(area, callback);
        pixels
            .painter()
            .with_clip_rect(pixels.rect(clip))
            .add(shape);
    }
}

struct QuadCallback {
    data: Vec<f32>,
    count: u32,
    additive: bool,
    buffer: OnceLock<wgpu::Buffer>,
}

/// The pipelines (normal and additive blending), built once per render
/// target.
struct QuadPipeline {
    target: WgpuTarget,
    pipeline: wgpu::RenderPipeline,
    additive: wgpu::RenderPipeline,
}

impl QuadPipeline {
    fn new(device: &wgpu::Device, target: WgpuTarget) -> QuadPipeline {
        QuadPipeline {
            target,
            pipeline: Self::build(device, target, wgpu::BlendFactor::OneMinusSrcAlpha),
            additive: Self::build(device, target, wgpu::BlendFactor::One),
        }
    }

    fn build(
        device: &wgpu::Device,
        target: WgpuTarget,
        dst_factor: wgpu::BlendFactor,
    ) -> wgpu::RenderPipeline {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sas-gui quads"),
            source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sas-gui quads"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let float4 = wgpu::VertexFormat::Float32x4;
        let mut attributes = Vec::new();
        let mut offset = 0u64;
        for (location, format) in [
            float4,
            wgpu::VertexFormat::Float32x2,
            float4,
            float4,
            float4,
            float4,
            float4,
            float4,
            float4,
            float4,
        ]
        .into_iter()
        .enumerate()
        {
            attributes.push(wgpu::VertexAttribute {
                format,
                offset,
                shader_location: location as u32,
            });
            offset += format.size();
        }
        // Upstream blends with (SRC_ALPHA, ONE_MINUS_SRC_ALPHA), or
        // (SRC_ALPHA, ONE) when additive. egui's target holds premultiplied
        // colour, so alpha accumulates the same way over it.
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sas-gui quads"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: offset,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target.format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: target.msaa_samples,
                ..Default::default()
            },
            multiview_mask: None,
            cache: None,
        })
    }
}

impl egui_wgpu::CallbackTrait for QuadCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let target = *resources
            .get::<WgpuTarget>()
            .expect("the renderer publishes its WgpuTarget (see sas_gui::primitives)");
        if resources.get::<QuadPipeline>().map(|p| p.target) != Some(target) {
            resources.insert(QuadPipeline::new(device, target));
        }
        let bytes: Vec<u8> = self
            .data
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        let _ = self.buffer.set(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sas-gui quad instances"),
                contents: &bytes,
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let (Some(pipeline), Some(buffer)) = (resources.get::<QuadPipeline>(), self.buffer.get())
        else {
            return;
        };
        render_pass.set_pipeline(if self.additive {
            &pipeline.additive
        } else {
            &pipeline.pipeline
        });
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..4, 0..self.count);
    }
}

/// Publishes `state`'s target for the quads, as the patched egui-baseview
/// does for the editor window. For offscreen renderers (tests, the gallery).
pub fn publish_target(state: &egui_wgpu::RenderState, msaa_samples: u32) {
    let target = WgpuTarget {
        format: state.target_format,
        msaa_samples,
    };
    state.renderer.write().callback_resources.insert(target);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quads_map_pixels_to_device_coordinates() {
        let quad = Quad::new(
            QuadKind::Circle,
            RectF::new(100.0, 50.0, 40.0, 20.0),
            Colour(0xffff0000),
        );
        let instance = quad.instance([400.0, 200.0]);
        let expected = [-0.5, 0.3, 0.2, 0.2];
        for (actual, expected) in instance.position.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-6, "{:?}", instance.position);
        }
        assert_eq!(instance.dimensions, [40.0, 20.0]);
    }

    #[test]
    fn uniforms_follow_open_gl_multi_quad_render() {
        let mut quad = Quad::new(
            QuadKind::RoundedRectangle,
            RectF::new(0.0, 0.0, 10.0, 10.0),
            Colour(0xff336699),
        );
        quad.alpha_mult = 0.5;
        quad.rounding = 4.0;
        let instance = quad.instance([10.0, 10.0]);
        // No alpha_mult uniform: the alpha goes into the colour.
        assert_eq!(instance.color[3], 0.5);
        assert_eq!(instance.params2[1], 1.0);
        // setRounding doubles.
        assert_eq!(instance.params[1], 8.0);
        // A transparent alternate colour is the main colour at zero alpha.
        assert_eq!(instance.alt_color, [0.2, 0.4, 0.6, 0.0]);

        let mut border = Quad::new(
            QuadKind::RoundedRectangleBorder,
            RectF::new(0.0, 0.0, 10.0, 10.0),
            Colour(0xff336699),
        );
        border.alpha_mult = 0.5;
        let instance = border.instance([10.0, 10.0]);
        assert_eq!(instance.color[3], 1.0);
        assert_eq!(instance.params2[1], 0.5);
    }

    #[test]
    fn the_shader_parses() {
        let module = naga::front::wgsl::parse_str(include_str!("quad.wgsl")).expect("valid WGSL");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("a valid shader module");
    }
}
