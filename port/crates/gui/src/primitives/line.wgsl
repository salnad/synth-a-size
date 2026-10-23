// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later
//
// Ports OpenGlLineRenderer's shaders from vital/src/interface/look_and_feel/shaders.cpp:
// kLineVertexShader with kLineFragmentShader, and kFillVertexShader with
// kFillFragmentShader. Positions arrive already scaled and placed in the
// editor (upstream's `scale` uniform and viewport are applied on the CPU),
// so the fill carries the component-space y its `distance` needs.

struct Uniforms {
    color: vec4<f32>,
    color_from: vec4<f32>,
    color_to: vec4<f32>,
    // line_width, boost, center_position, boost_amount
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct LineOut {
    @builtin(position) position: vec4<f32>,
    @location(0) depth: f32,
};

@vertex
fn line_vs(@location(0) position: vec3<f32>) -> LineOut {
    var out: LineOut;
    out.position = vec4<f32>(position.xy, 0.0, 1.0);
    out.depth = position.z;
    return out;
}

@fragment
fn line_fs(v: LineOut) -> @location(0) vec4<f32> {
    let line_width = u.params.x;
    let boost = u.params.y;
    let dist_from_edge = min(v.depth, 1.0 - v.depth);
    let mult = 1.0 + boost * max(dist_from_edge - 2.0 / line_width, 0.0);
    var result = mult * u.color;
    let scale = line_width * dist_from_edge;
    result.a = result.a * scale / 2.0;
    return result;
}

struct FillOut {
    @builtin(position) position: vec4<f32>,
    @location(0) distance: f32,
    @location(1) boost: f32,
};

// position: editor x, editor y, boost, component y.
@vertex
fn fill_vs(@location(0) position: vec4<f32>) -> FillOut {
    let center_position = u.params.z;
    let boost_amount = u.params.w;
    var out: FillOut;
    out.distance = (position.w - center_position) / (1.0 - center_position);
    out.boost = boost_amount * position.z;
    out.position = vec4<f32>(position.xy, 0.0, 1.0);
    return out;
}

@fragment
fn fill_fs(v: FillOut) -> @location(0) vec4<f32> {
    let delta = abs(v.distance);
    let base_color = u.color_to * delta + u.color_from * (1.0 - delta);
    var color = base_color;
    color.a = (v.boost + 1.0) * base_color.a;
    return color;
}
