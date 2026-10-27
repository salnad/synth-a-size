// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Upstream's line and fill renderer: a polyline drawn as a triangle strip
//! with anti-aliased edges, an optional fill to a centre line, and per-point
//! "boost" that thickens and brightens it.
//!
//! Ports `OpenGlLineRenderer`
//! (`vital/src/interface/editor_components/open_gl_line_renderer.{h,cpp}`):
//! the vertices are built on the CPU exactly as upstream builds them,
//! including its fast inverse square root, then drawn with `line.wgsl`.

use std::sync::OnceLock;

use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt;

use super::WgpuTarget;
use crate::colour::Colour;
use crate::paint::{PixelPainter, RectF};

const LINE_FLOATS_PER_VERTEX: usize = 3;
const FILL_FLOATS_PER_VERTEX: usize = 4;
const LINE_VERTICES_PER_POINT: usize = 6;
const FILL_VERTICES_PER_POINT: usize = 2;
const LINE_FLOATS_PER_POINT: usize = LINE_VERTICES_PER_POINT * LINE_FLOATS_PER_VERTEX;
const FILL_FLOATS_PER_POINT: usize = FILL_VERTICES_PER_POINT * FILL_FLOATS_PER_VERTEX;
/// `kLoopWidth`: how far (device units) a looping line's copies sit.
const LOOP_WIDTH: f32 = 2.001;
/// `kDefaultLineWidth`.
const DEFAULT_LINE_WIDTH: f32 = 7.0;

/// The fast inverse square root upstream uses (two Newton steps).
fn inverse_sqrt(value: f32) -> f32 {
    const THREE_HALVES: f32 = 1.5;
    let x2 = value * 0.5;
    let i = 0x5f37_59df_i32.wrapping_sub((value.to_bits() as i32) >> 1);
    let mut y = f32::from_bits(i as u32);
    y *= THREE_HALVES - (x2 * y * y);
    y *= THREE_HALVES - (x2 * y * y);
    y
}

fn inverse_magnitude(x: f32, y: f32) -> f32 {
    inverse_sqrt(x * x + y * y)
}

fn normalize(x: f32, y: f32) -> (f32, f32) {
    let inverse = inverse_magnitude(x, y);
    (x * inverse, y * inverse)
}

/// A renderer's boosts and its backward-boost state, kept from one frame
/// to the next by a component that rebuilds its renderer each frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Boosts {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub last_negative: bool,
}

/// One `OpenGlLineRenderer`.
#[derive(Clone, Debug)]
pub struct LineRenderer {
    num_points: usize,
    num_padding: usize,
    looped: bool,
    x: Vec<f32>,
    y: Vec<f32>,
    boost_left: Vec<f32>,
    boost_right: Vec<f32>,
    pub line_width: f32,
    /// The `boost` uniform: how much brighter the middle of the line gets.
    pub boost: f32,
    /// How much a point's boost widens the line.
    pub boost_amount: f32,
    pub fill: bool,
    /// The fill's centre line, in the component's device coordinates.
    pub fill_center: f32,
    pub fill_boost_amount: f32,
    /// `fit_`: shrink slightly so a wide line stays inside the component.
    pub fit: bool,
    pub enable_backward_boost: bool,
    pub color: Colour,
    pub fill_color_from: Colour,
    pub fill_color_to: Colour,
    last_negative_boost: bool,
    any_boost_value: bool,
}

impl LineRenderer {
    pub fn new(num_points: usize, looped: bool) -> LineRenderer {
        let mut line = LineRenderer {
            num_points,
            num_padding: if looped { 2 } else { 1 },
            looped,
            x: vec![0.0; num_points],
            y: vec![0.0; num_points],
            boost_left: vec![0.0; num_points],
            boost_right: vec![0.0; num_points],
            line_width: DEFAULT_LINE_WIDTH,
            boost: 0.0,
            boost_amount: 0.0,
            fill: false,
            fill_center: 0.0,
            fill_boost_amount: 0.0,
            fit: false,
            enable_backward_boost: true,
            color: Colour::TRANSPARENT_BLACK,
            fill_color_from: Colour::TRANSPARENT_BLACK,
            fill_color_to: Colour::TRANSPARENT_BLACK,
            last_negative_boost: false,
            any_boost_value: false,
        };
        for i in 0..num_points {
            line.x[i] = 2.0 * i as f32 / (num_points as f32 - 1.0) - 1.0;
        }
        line
    }

    pub fn num_points(&self) -> usize {
        self.num_points
    }

    /// Point `index`'s position, in pixels from the component's top left.
    pub fn set_x_at(&mut self, index: usize, x: f32) {
        self.x[index] = x;
    }

    pub fn set_y_at(&mut self, index: usize, y: f32) {
        self.y[index] = y;
    }

    pub fn x_at(&self, index: usize) -> f32 {
        self.x[index]
    }

    pub fn y_at(&self, index: usize) -> f32 {
        self.y[index]
    }

    pub fn boost_left_at(&self, index: usize) -> f32 {
        self.boost_left[index]
    }

    pub fn any_boost_value(&self) -> bool {
        self.any_boost_value
    }

    /// The boosts, to carry to the next frame's renderer.
    pub fn boosts(&self) -> Boosts {
        Boosts {
            left: self.boost_left.clone(),
            right: self.boost_right.clone(),
            last_negative: self.last_negative_boost,
        }
    }

    /// Takes the boosts a previous frame's renderer left (all zero when
    /// `boosts` is empty or the wrong size).
    pub fn set_boosts(&mut self, boosts: &Boosts) {
        let fits = boosts.left.len() == self.num_points && boosts.right.len() == self.num_points;
        if fits {
            self.boost_left.copy_from_slice(&boosts.left);
            self.boost_right.copy_from_slice(&boosts.right);
        } else {
            self.boost_left.fill(0.0);
            self.boost_right.fill(0.0);
        }
        self.last_negative_boost = boosts.last_negative;
        self.any_boost_value = self
            .boost_left
            .iter()
            .chain(&self.boost_right)
            .any(|&boost| boost != 0.0);
    }

    /// `setBoostLeft` / `setBoostRight` at `index`.
    pub fn set_boost_at(&mut self, left: bool, index: usize, value: f32) {
        if left {
            self.boost_left[index] = value;
        } else {
            self.boost_right[index] = value;
        }
    }

    /// `boostLeftAt` / `boostRightAt`.
    pub fn boost_at(&self, left: bool, index: usize) -> f32 {
        if left {
            self.boost_left[index]
        } else {
            self.boost_right[index]
        }
    }

    /// `setFillColors`.
    pub fn set_fill_colors(&mut self, from: Colour, to: Colour) {
        self.fill_color_from = from;
        self.fill_color_to = to;
    }

    /// `boostLeftRange` / `boostRightRange`.
    pub fn boost_range(
        &mut self,
        left: bool,
        start: f32,
        end: f32,
        buffer_vertices: usize,
        min: f32,
    ) {
        self.any_boost_value = true;
        let num_points_total = self.num_points as i32;
        let active_points = num_points_total - 2 * buffer_vertices as i32;
        let mut start_index = ((start * (active_points - 1) as f32).ceil() as i32).max(0);
        let end_position = end * (active_points - 1) as f32;
        let mut end_index = end_position.ceil().max(0.0) as i32;
        let mut progress = end_position - end_position as i32 as f32;

        start_index %= active_points;
        end_index %= active_points;
        let mut num_points = end_index - start_index;
        let mut direction = 1;
        if self.enable_backward_boost {
            if (num_points < 0 && num_points > -num_points_total / 2)
                || (num_points == 0 && self.last_negative_boost)
            {
                num_points = -num_points;
                direction = -1;
            } else if num_points > num_points_total / 2 {
                num_points -= active_points;
                num_points = -num_points;
                direction = -1;
            }
        }

        self.last_negative_boost = direction < 0;
        if self.last_negative_boost {
            start_index = ((start * (active_points - 1) as f32).floor() as i32).max(0);
            end_index = end_position.floor().max(0.0) as i32;
            start_index %= active_points;
            end_index %= active_points;
            num_points = start_index - end_index;
            progress = 1.0 - progress;
        }

        let delta = (1.0 - min) / num_points as f32;
        let mut value = min;
        let boosts = if left {
            &mut self.boost_left
        } else {
            &mut self.boost_right
        };
        let offset = buffer_vertices as i32;
        let mut i = start_index;
        while i != end_index {
            value += delta;
            value = value.min(1.0);
            let slot = &mut boosts[(i + offset) as usize];
            *slot = slot.max(value);
            i = (i + active_points + direction) % active_points;
        }
        let slot = &mut boosts[(end_index + offset) as usize];
        *slot = slot.max(progress * progress);
    }

    /// `decayBoosts`.
    pub fn decay_boosts(&mut self, left_mult: f32, right_mult: f32) {
        let mut any_boost = false;
        for i in 0..self.num_points {
            self.boost_left[i] *= left_mult;
            self.boost_right[i] *= right_mult;
            any_boost = any_boost || self.boost_left[i] != 0.0 || self.boost_right[i] != 0.0;
        }
        self.any_boost_value = any_boost;
    }

    /// `setLineVertices`: the strip in the component's device coordinates,
    /// three floats a vertex (x, y, edge depth).
    pub fn line_vertices(&self, left: bool, width: f32, height: f32) -> Vec<f32> {
        let n = self.num_points;
        let pad = self.num_padding;
        let mut data = vec![0.0f32; LINE_FLOATS_PER_POINT * (n + 2 * pad)];
        let mut i = 0;
        while i < data.len() {
            data[i + 2] = 1.0;
            i += 2 * LINE_FLOATS_PER_VERTEX;
        }
        let boosts = if left {
            &self.boost_left
        } else {
            &self.boost_right
        };

        let mut prev_normalized_delta = (0.0f32, 0.0f32);
        for i in 0..n.saturating_sub(1) {
            if self.x[i] != self.x[i + 1] || self.y[i] != self.y[i + 1] {
                prev_normalized_delta =
                    normalize(self.x[i + 1] - self.x[i], self.y[i + 1] - self.y[i]);
                break;
            }
        }

        let mut prev_delta_normal = (-prev_normalized_delta.1, prev_normalized_delta.0);
        let line_radius = self.line_width / 2.0 + 0.5;
        let mut prev_magnitude = line_radius;
        let x_adjust = 2.0 / width;
        let y_adjust = 2.0 / height;

        for (i, &boost) in boosts.iter().enumerate() {
            let radius = line_radius * (1.0 + self.boost_amount * boost);
            let point = (self.x[i], self.y[i]);
            let next = (i + 1).min(n - 1);
            let mut next_point = (self.x[next], self.y[next]);
            let mut delta = (next_point.0 - point.0, next_point.1 - point.1);
            if point == next_point {
                delta = prev_normalized_delta;
                next_point = (point.0 + delta.0, point.1 + delta.1);
            }
            let _ = next_point;

            let inverse = inverse_magnitude(delta.0, delta.1);
            let magnitude = 1.0 / inverse.max(0.00001);
            let normalized_delta = (delta.0 * inverse, delta.1 * inverse);
            let delta_normal = (-normalized_delta.1, normalized_delta.0);

            let angle_bisect = (
                normalized_delta.0 - prev_normalized_delta.0,
                normalized_delta.1 - prev_normalized_delta.1,
            );
            let straight = angle_bisect.0 < 0.001
                && angle_bisect.0 > -0.001
                && angle_bisect.1 < 0.001
                && angle_bisect.1 > -0.001;
            let bisect_line = if straight {
                delta_normal
            } else {
                normalize(angle_bisect.0, angle_bisect.1)
            };

            let max_inner_radius = radius.max(0.5 * (magnitude + prev_magnitude));
            prev_magnitude = magnitude;

            let bisect_normal_dot = bisect_line.0 * delta_normal.0 + bisect_line.1 * delta_normal.1;
            let inner_mult = 1.0 / bisect_normal_dot.abs().max(0.1);
            let inner_length = (inner_mult * radius).min(max_inner_radius);
            let inner_point = (
                point.0 + inner_length * bisect_line.0,
                point.1 + inner_length * bisect_line.1,
            );
            let outer_point = (
                point.0 - bisect_line.0 * radius,
                point.1 - bisect_line.1 * radius,
            );

            let corners: [(f32, f32); 6] = if bisect_normal_dot < 0.0 {
                let (start, end) = if straight {
                    (outer_point, outer_point)
                } else {
                    (
                        (
                            point.0 + prev_delta_normal.0 * radius,
                            point.1 + prev_delta_normal.1 * radius,
                        ),
                        (
                            point.0 + delta_normal.0 * radius,
                            point.1 + delta_normal.1 * radius,
                        ),
                    )
                };
                [
                    start,
                    inner_point,
                    outer_point,
                    inner_point,
                    end,
                    inner_point,
                ]
            } else {
                let (start, end) = if straight {
                    (outer_point, outer_point)
                } else {
                    (
                        (
                            point.0 - prev_delta_normal.0 * radius,
                            point.1 - prev_delta_normal.1 * radius,
                        ),
                        (
                            point.0 - delta_normal.0 * radius,
                            point.1 - delta_normal.1 * radius,
                        ),
                    )
                };
                [
                    inner_point,
                    start,
                    inner_point,
                    outer_point,
                    inner_point,
                    end,
                ]
            };

            let first = (i + pad) * LINE_FLOATS_PER_POINT;
            for (k, (x, y)) in corners.iter().enumerate() {
                let at = first + k * LINE_FLOATS_PER_VERTEX;
                data[at] = x_adjust * x - 1.0;
                data[at + 1] = 1.0 - y_adjust * y;
            }

            prev_delta_normal = delta_normal;
            prev_normalized_delta = normalized_delta;
        }

        let begin_copy_dest = (pad + n) * LINE_FLOATS_PER_POINT;
        if self.looped && n >= 2 {
            let size = pad * LINE_FLOATS_PER_POINT;
            let begin_copy_source = pad * LINE_FLOATS_PER_POINT;
            let end_copy_source = n * LINE_FLOATS_PER_POINT;
            data.copy_within(end_copy_source..end_copy_source + size, 0);
            data.copy_within(begin_copy_source..begin_copy_source + size, begin_copy_dest);
            for i in 0..pad * LINE_VERTICES_PER_POINT {
                data[i * LINE_FLOATS_PER_VERTEX] -= LOOP_WIDTH;
                data[begin_copy_dest + i * LINE_FLOATS_PER_VERTEX] += LOOP_WIDTH;
            }
        } else if n >= 2 {
            let (sx, sy) = normalize(self.x[0] - self.x[1], self.y[0] - self.y[1]);
            let start = (sx * line_radius, sy * line_radius);
            let (ex, ey) = normalize(self.x[n - 1] - self.x[n - 2], self.y[n - 1] - self.y[n - 2]);
            let end = (ex * line_radius, ey * line_radius);
            for i in 0..LINE_VERTICES_PER_POINT {
                let at = i * LINE_FLOATS_PER_VERTEX;
                data[at] = (self.x[0] + start.0) * x_adjust - 1.0;
                data[at + 1] = 1.0 - (self.y[0] + start.1) * y_adjust;
                data[at + 2] = boosts[0];
                let at = begin_copy_dest + i * LINE_FLOATS_PER_VERTEX;
                data[at] = (self.x[n - 1] + end.0) * x_adjust - 1.0;
                data[at + 1] = 1.0 - (self.y[n - 1] + end.1) * y_adjust;
                data[at + 2] = boosts[n - 1];
            }
        }
        data
    }

    /// `setFillVertices`: four floats a vertex (x, y, boost, unused).
    pub fn fill_vertices(&self, left: bool, width: f32, height: f32) -> Vec<f32> {
        let n = self.num_points;
        let pad = self.num_padding;
        let mut data = vec![0.0f32; FILL_FLOATS_PER_POINT * (n + 2 * pad)];
        let boosts = if left {
            &self.boost_left
        } else {
            &self.boost_right
        };
        let x_adjust = 2.0 / width;
        let y_adjust = 2.0 / height;
        for (i, &boost) in boosts.iter().enumerate() {
            let top = (i + pad) * FILL_FLOATS_PER_POINT;
            let bottom = top + FILL_FLOATS_PER_VERTEX;
            let x = x_adjust * self.x[i] - 1.0;
            let y = 1.0 - y_adjust * self.y[i];
            data[top] = x;
            data[top + 1] = y;
            data[top + 2] = boost;
            data[bottom] = x;
            data[bottom + 1] = self.fill_center;
            data[bottom + 2] = boost;
        }

        let size = pad * FILL_FLOATS_PER_POINT;
        let begin_copy_source = pad * FILL_FLOATS_PER_POINT;
        let end_copy_source = n * FILL_FLOATS_PER_POINT;
        let end_copy_dest = (pad + n) * FILL_FLOATS_PER_POINT;
        if self.looped && n >= 2 {
            data.copy_within(end_copy_source..end_copy_source + size, 0);
            data.copy_within(begin_copy_source..begin_copy_source + size, end_copy_dest);
            for i in 0..pad {
                data[i * FILL_FLOATS_PER_POINT] -= LOOP_WIDTH;
                data[i * FILL_FLOATS_PER_POINT + FILL_FLOATS_PER_VERTEX] -= LOOP_WIDTH;
                data[end_copy_dest + i * FILL_FLOATS_PER_POINT] += LOOP_WIDTH;
                data[end_copy_dest + i * FILL_FLOATS_PER_POINT + FILL_FLOATS_PER_VERTEX] +=
                    LOOP_WIDTH;
            }
        } else {
            data.copy_within(end_copy_source..end_copy_source + size, end_copy_dest);
            data.copy_within(begin_copy_source..begin_copy_source + size, 0);
        }
        data
    }

    /// `drawLines`: the fill (if on) then the line, for the left or right
    /// channel's boosts, in `bounds` (pixels), clipped to it.
    pub fn paint(&self, pixels: &PixelPainter, bounds: RectF, left: bool) {
        if bounds.width <= 0.0 || bounds.height <= 0.0 || self.num_points < 2 {
            return;
        }
        let area = pixels.area();
        let viewport = (
            (area.width() * pixels.pixels_per_point()).round(),
            (area.height() * pixels.pixels_per_point()).round(),
        );
        let (x_shrink, y_shrink) = if self.fit {
            (
                1.0 - 0.33 * self.line_width / bounds.width,
                1.0 - 0.33 * self.line_width / bounds.height,
            )
        } else {
            (1.0, 1.0)
        };
        // Component device coordinates, scaled, to editor device coordinates.
        let place = |x: f32, y: f32| {
            let pixel_x = bounds.x + (x * x_shrink + 1.0) * 0.5 * bounds.width;
            let pixel_y = bounds.y + (1.0 - y * y_shrink) * 0.5 * bounds.height;
            (
                2.0 * pixel_x / viewport.0 - 1.0,
                1.0 - 2.0 * pixel_y / viewport.1,
            )
        };

        let mut line = self.line_vertices(left, bounds.width, bounds.height);
        for vertex in line.as_chunks_mut::<LINE_FLOATS_PER_VERTEX>().0 {
            let (x, y) = place(vertex[0], vertex[1]);
            vertex[0] = x;
            vertex[1] = y;
        }
        let fill = self.fill.then(|| {
            let mut fill = self.fill_vertices(left, bounds.width, bounds.height);
            for vertex in fill.as_chunks_mut::<FILL_FLOATS_PER_VERTEX>().0 {
                let component_y = vertex[1];
                let (x, y) = place(vertex[0], vertex[1]);
                vertex[0] = x;
                vertex[1] = y;
                vertex[3] = component_y;
            }
            fill
        });

        let uniforms = [
            rgba(self.color),
            rgba(self.fill_color_from),
            rgba(self.fill_color_to),
            [
                self.line_width,
                self.boost,
                self.fill_center,
                self.fill_boost_amount,
            ],
        ];
        let callback = LineCallback {
            line,
            fill,
            uniforms,
            buffers: OnceLock::new(),
        };
        let shape = egui_wgpu::Callback::new_paint_callback(area, callback);
        pixels
            .painter()
            .with_clip_rect(pixels.rect(bounds))
            .add(shape);
    }
}

fn rgba(colour: Colour) -> [f32; 4] {
    [
        colour.red() as f32 / 255.0,
        colour.green() as f32 / 255.0,
        colour.blue() as f32 / 255.0,
        colour.alpha() as f32 / 255.0,
    ]
}

struct LineCallback {
    line: Vec<f32>,
    fill: Option<Vec<f32>>,
    uniforms: [[f32; 4]; 4],
    buffers: OnceLock<LineBuffers>,
}

struct LineBuffers {
    line: wgpu::Buffer,
    fill: Option<wgpu::Buffer>,
    bind_group: wgpu::BindGroup,
}

struct LinePipelines {
    target: WgpuTarget,
    /// `None` if the device rejected them.
    built: Option<BuiltLinePipelines>,
}

struct BuiltLinePipelines {
    layout: wgpu::BindGroupLayout,
    line: wgpu::RenderPipeline,
    fill: wgpu::RenderPipeline,
}

impl LinePipelines {
    fn new(device: &wgpu::Device, target: WgpuTarget) -> LinePipelines {
        LinePipelines {
            target,
            built: super::validated(device, "line", || Self::build(device, target)),
        }
    }

    fn build(device: &wgpu::Device, target: WgpuTarget) -> BuiltLinePipelines {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sas-gui lines"),
            source: wgpu::ShaderSource::Wgsl(include_str!("line.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sas-gui lines"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sas-gui lines"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let build = |entry: (&str, &str), format: wgpu::VertexFormat| {
            let attributes = [wgpu::VertexAttribute {
                format,
                offset: 0,
                shader_location: 0,
            }];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sas-gui lines"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(entry.0),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: format.size(),
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry.1),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target.format,
                        blend: Some(super::straight_alpha_blend(
                            wgpu::BlendFactor::OneMinusSrcAlpha,
                        )),
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
        };
        BuiltLinePipelines {
            line: build(("line_vs", "line_fs"), wgpu::VertexFormat::Float32x3),
            fill: build(("fill_vs", "fill_fs"), wgpu::VertexFormat::Float32x4),
            layout,
        }
    }
}

pub(super) fn pipelines_ready(
    resources: &egui_wgpu::CallbackResources,
    target: WgpuTarget,
) -> bool {
    resources
        .get::<LinePipelines>()
        .is_some_and(|lines| lines.target == target && lines.built.is_some())
}

fn bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

impl egui_wgpu::CallbackTrait for LineCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(target) = super::target(resources) else {
            return Vec::new();
        };
        if resources.get::<LinePipelines>().map(|p| p.target) != Some(target) {
            resources.insert(LinePipelines::new(device, target));
        }
        let Some(LinePipelines {
            built: Some(pipelines),
            ..
        }) = resources.get::<LinePipelines>()
        else {
            return Vec::new();
        };
        let vertex_buffer = |data: &[f32]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sas-gui line vertices"),
                contents: &bytes(data),
                usage: wgpu::BufferUsages::VERTEX,
            })
        };
        let uniforms: Vec<f32> = self.uniforms.iter().flatten().copied().collect();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sas-gui line uniforms"),
            contents: &bytes(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sas-gui lines"),
            layout: &pipelines.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let _ = self.buffers.set(LineBuffers {
            line: vertex_buffer(&self.line),
            fill: self.fill.as_deref().map(vertex_buffer),
            bind_group,
        });
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if super::gpu_failed(resources) {
            return;
        }
        let (
            Some(LinePipelines {
                built: Some(pipelines),
                ..
            }),
            Some(buffers),
        ) = (resources.get::<LinePipelines>(), self.buffers.get())
        else {
            return;
        };
        render_pass.set_bind_group(0, &buffers.bind_group, &[]);
        if let (Some(fill), Some(data)) = (&buffers.fill, &self.fill) {
            render_pass.set_pipeline(&pipelines.fill);
            render_pass.set_vertex_buffer(0, fill.slice(..));
            render_pass.draw(0..(data.len() / FILL_FLOATS_PER_VERTEX) as u32, 0..1);
        }
        render_pass.set_pipeline(&pipelines.line);
        render_pass.set_vertex_buffer(0, buffers.line.slice(..));
        render_pass.draw(0..(self.line.len() / LINE_FLOATS_PER_VERTEX) as u32, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_inverse_sqrt_is_upstreams() {
        // Two Newton steps from the 0x5f3759df guess, bit for bit (computed
        // with the C algorithm in single precision).
        assert_eq!(inverse_sqrt(4.0).to_bits(), 0x3eff_ffb7);
        assert_eq!(inverse_sqrt(2.0).to_bits(), 0x3f35_04f1);
    }

    #[test]
    fn a_flat_line_is_a_band_with_soft_edges() {
        let mut line = LineRenderer::new(3, false);
        line.line_width = 2.0;
        for i in 0..3 {
            line.set_x_at(i, 10.0 + 10.0 * i as f32);
            line.set_y_at(i, 50.0);
        }
        let data = line.line_vertices(true, 100.0, 100.0);
        assert_eq!(data.len(), LINE_FLOATS_PER_POINT * 5);
        // The middle point: radius 1.5 pixels above and below y = 50.
        let first = 2 * LINE_FLOATS_PER_POINT;
        let ys: Vec<f32> = (0..6).map(|k| data[first + k * 3 + 1]).collect();
        let depths: Vec<f32> = (0..6).map(|k| data[first + k * 3 + 2]).collect();
        let up = 1.0 - 2.0 * 48.5 / 100.0;
        let down = 1.0 - 2.0 * 51.5 / 100.0;
        for (k, y) in ys.iter().enumerate() {
            let expected = if k % 2 == 0 { down } else { up };
            assert!((y - expected).abs() < 1e-4, "{ys:?}");
        }
        assert_eq!(depths, [1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
        // The end caps' padding vertices carry the end boost as depth (0),
        // so they fade out, as upstream's do.
        assert_eq!(data[2], 0.0);
    }

    #[test]
    fn fills_reach_the_centre_line() {
        let mut line = LineRenderer::new(2, false);
        line.fill_center = -1.0;
        line.set_x_at(0, 0.0);
        line.set_x_at(1, 100.0);
        line.set_y_at(0, 25.0);
        line.set_y_at(1, 25.0);
        let data = line.fill_vertices(true, 100.0, 100.0);
        let top = FILL_FLOATS_PER_POINT;
        assert_eq!(&data[top..top + 2], &[-1.0, 0.5]);
        assert_eq!(&data[top + 4..top + 6], &[-1.0, -1.0]);
        // Padding copies the end points.
        assert_eq!(&data[0..2], &[-1.0, 0.5]);
    }

    #[test]
    fn boosts_ramp_up_to_the_end() {
        let mut line = LineRenderer::new(11, false);
        // Halfway along: five steps of 0.2, then the end point gets the
        // square of how far into it the range reaches (none here).
        line.boost_range(true, 0.0, 0.5, 0, 0.0);
        assert!((line.boost_left_at(0) - 0.2).abs() < 1e-6);
        assert!((line.boost_left_at(1) - 0.4).abs() < 1e-6);
        assert!((line.boost_left_at(4) - 1.0).abs() < 1e-6);
        assert_eq!(line.boost_left_at(5), 0.0);
        line.decay_boosts(0.5, 0.5);
        assert!((line.boost_left_at(4) - 0.5).abs() < 1e-6);
        assert!(line.any_boost_value());
    }
}
