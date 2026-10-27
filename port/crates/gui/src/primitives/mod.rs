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
//! If the device rejects a pipeline, or reports any error outside an error
//! scope ([`WgpuErrors`]), the shader-drawn primitives stop drawing and the
//! rest of the editor carries on.

pub mod line;

use std::future::Future;
use std::sync::OnceLock;
use std::task::{Context, Poll, Waker};

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;
pub use nice_plug_egui::{WgpuErrors, WgpuTarget};

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
    /// `kGainMeterFragment` (with `kGainMeterVertex`): `color` to
    /// `alt_color` across the component.
    GainMeter = 14,
    /// `kBarFragment` (`BarRenderer`'s bars): `color`, fading out towards
    /// each edge. `shader_values[0]` and `[1]` are the bar vertex shaders'
    /// `size` in interface pixels along x and y: the bar's full thickness
    /// across it, and half its length along it (`BarRenderer::setBarSizes`;
    /// its `display_scale_` is always 1), so the fade spans one pixel across
    /// and two along. Additive bars go in
    /// [`QuadBatch::additive`].
    Bar = 15,
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
    /// `setCoordinates`: x, y, width, height of the shader coordinates
    /// across the quad, from its bottom left corner.
    pub coordinates: [f32; 4],
    /// `setRotatedCoordinates`: the coordinates turned a quarter turn, x
    /// running from the bottom edge to the top and y from the right edge to
    /// the left.
    pub rotated: bool,
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
            coordinates: [-1.0, -1.0, 2.0, 2.0],
            rotated: false,
        }
    }

    /// One of a vertical `BarRenderer`'s bars in `component` (pixels): `x`,
    /// `bottom` and `top` are the bar's position in the component's -1 to 1
    /// coordinates, `width_percent` its width in them (the vertex shader's
    /// `width_percent` uniform). Sizes are in interface pixels at any
    /// display scale, as upstream's are.
    pub fn vertical_bar(
        component: RectF,
        x: f32,
        bottom: f32,
        top: f32,
        width_percent: f32,
        colour: Colour,
    ) -> Quad {
        let (w, h) = (component.width, component.height);
        let rect = RectF::new(
            component.x + (x + 1.0) * 0.5 * w,
            component.y + (1.0 - top) * 0.5 * h,
            width_percent * 0.5 * w,
            (top - bottom) * 0.5 * h,
        );
        let mut quad = Quad::new(QuadKind::Bar, rect, colour);
        // kBarVerticalVertex: size.x = width_percent * dimensions.x / 2,
        // size.y = position.z * dimensions.y / 2 (setBarSizes' position.z).
        let length = (top - bottom).abs() * 0.5;
        quad.shader_values = [width_percent * w / 2.0, length * h / 2.0, 0.0, 0.0];
        quad
    }

    /// `OpenGlCorners::setCorners`: four quads that paint `colour` outside
    /// rounded corners of radius `rounding` (pixels) on `bounds`.
    pub fn corners(bounds: RectF, rounding: f32, colour: Colour) -> [Quad; 4] {
        let r = rounding;
        let (left, top) = (bounds.x, bounds.y);
        let (right, bottom) = (bounds.x + bounds.width - r, bounds.y + bounds.height - r);
        let corner = |x: f32, y: f32, coordinates: [f32; 4]| {
            let mut quad = Quad::new(QuadKind::RoundedCorner, RectF::new(x, y, r, r), colour);
            quad.coordinates = coordinates;
            quad
        };
        [
            corner(left, bottom, [1.0, 1.0, -1.0, -1.0]),
            corner(left, top, [1.0, 0.0, -1.0, 1.0]),
            corner(right, top, [0.0, 0.0, 1.0, 1.0]),
            corner(right, bottom, [0.0, 1.0, 1.0, -1.0]),
        ]
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
            params2: [
                self.max_arc,
                alpha_mult,
                self.kind as u32 as f32,
                if self.rotated { 1.0 } else { 0.0 },
            ],
            coordinates: self.coordinates,
            rect: [
                self.rect.x,
                self.rect.y + self.rect.height,
                self.rect.width,
                self.rect.height,
            ],
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
    coordinates: [f32; 4],
    rect: [f32; 4],
}

const INSTANCE_FLOATS: usize = 4 + 2 + 4 * 10;

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
            &self.coordinates,
            &self.rect,
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
    ///
    /// `pixels`' area must start on a whole physical pixel. egui rounds a
    /// callback's viewport to whole pixels, and the quad shader rebuilds
    /// coordinates from pixel centres measured from that viewport; an area
    /// off the grid would move every quad by the rounding.
    pub fn paint(self, pixels: &PixelPainter, clip: RectF) {
        if self.quads.is_empty() {
            return;
        }
        let area = pixels.area();
        debug_assert!(
            [area.min.x, area.min.y].iter().all(|&point| {
                let pixel = point * pixels.pixels_per_point();
                (pixel - pixel.round()).abs() < 1e-3
            }),
            "quads painted in an area off the pixel grid: {area:?}"
        );
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
/// target; `None` if the device rejected them.
struct QuadPipeline {
    target: WgpuTarget,
    pipelines: Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>,
}

impl QuadPipeline {
    fn new(device: &wgpu::Device, target: WgpuTarget) -> QuadPipeline {
        QuadPipeline {
            target,
            pipelines: validated(device, "quad", || {
                (
                    Self::build(device, target, wgpu::BlendFactor::OneMinusSrcAlpha),
                    Self::build(device, target, wgpu::BlendFactor::One),
                )
            }),
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
        let blend = straight_alpha_blend(dst_factor);
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
        let Some(target) = target(resources) else {
            return Vec::new();
        };
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
        if gpu_failed(resources) {
            return;
        }
        let (
            Some(QuadPipeline {
                pipelines: Some((pipeline, additive)),
                ..
            }),
            Some(buffer),
        ) = (resources.get::<QuadPipeline>(), self.buffer.get())
        else {
            return;
        };
        render_pass.set_pipeline(if self.additive { additive } else { pipeline });
        render_pass.set_vertex_buffer(0, buffer.slice(..));
        render_pass.draw(0..4, 0..self.count);
    }
}

/// The render target the window or test renderer published, or `None` if
/// there is none or custom drawing is off ([`gpu_failed`]).
pub(crate) fn target(resources: &egui_wgpu::CallbackResources) -> Option<WgpuTarget> {
    if gpu_failed(resources) {
        return None;
    }
    let Some(target) = resources.get::<WgpuTarget>() else {
        nice_plug::nice_error!("no WgpuTarget published; shader-drawn primitives are off");
        return None;
    };
    Some(WgpuTarget::new(target.format, target.msaa_samples))
}

/// Whether the device has reported an error outside an error scope, after
/// which the custom pipelines draw nothing.
pub(crate) fn gpu_failed(resources: &egui_wgpu::CallbackResources) -> bool {
    resources
        .get::<WgpuErrors>()
        .is_some_and(|errors| errors.count() > 0)
}

/// What `build` creates, or `None` (logged) if the device reports a
/// validation error while creating it.
pub(crate) fn validated<T>(
    device: &wgpu::Device,
    what: &str,
    build: impl FnOnce() -> T,
) -> Option<T> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let built = build();
    let mut error = std::pin::pin!(scope.pop());
    // Native wgpu resolves the scope at once.
    match error.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(Some(error)) => {
            nice_plug::nice_error!("the {what} pipeline is invalid, so it draws nothing: {error}");
            None
        }
        Poll::Ready(None) => Some(built),
        Poll::Pending => {
            nice_plug::nice_warn!(
                "the {what} pipeline's validation is still pending; drawing with it unchecked"
            );
            Some(built)
        }
    }
}

/// Upstream's `glBlendFunc(GL_SRC_ALPHA, dst)`. egui's target holds
/// premultiplied colour, so alpha accumulates the same way over it.
pub(crate) fn straight_alpha_blend(dst_factor: wgpu::BlendFactor) -> wgpu::BlendState {
    wgpu::BlendState {
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
    }
}

/// Whether the quad and line pipelines are built and valid for `state`'s
/// published target, with custom drawing on. For tests that render the
/// editor.
pub fn custom_drawing_ready(state: &egui_wgpu::RenderState) -> bool {
    let renderer = state.renderer.read();
    let resources = &renderer.callback_resources;
    let Some(target) = target(resources) else {
        return false;
    };
    resources
        .get::<QuadPipeline>()
        .is_some_and(|quads| quads.target == target && quads.pipelines.is_some())
        && line::pipelines_ready(resources, target)
}

/// Publishes `state`'s target for the quads, as the patched egui-baseview
/// does for the editor window. For offscreen renderers (tests, the gallery).
pub fn publish_target(state: &egui_wgpu::RenderState, msaa_samples: u32) {
    let target = WgpuTarget::new(state.target_format, msaa_samples);
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
        assert_eq!(instance.rect, [100.0, 70.0, 40.0, 20.0]);
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
    fn the_shaders_parse() {
        for source in [include_str!("quad.wgsl"), include_str!("line.wgsl")] {
            let module = naga::front::wgsl::parse_str(source).expect("valid WGSL");
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .expect("a valid shader module");
        }
    }
}
