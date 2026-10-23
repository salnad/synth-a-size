// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later
//
// Ports the OpenGlMultiQuad shaders of vital/src/interface/look_and_feel/shaders.cpp:
// kPassthroughVertexShader and the fragment shaders it is paired with. One
// instance is one quad. Its per-quad values (upstream's vertex attributes)
// and its uniforms are both instance data, and `kind` picks the fragment
// shader. The arithmetic follows each GLSL shader line by line.

struct Instance {
    // Quad position in normalized device coordinates: x, y (bottom left), w, h.
    @location(0) position: vec4<f32>,
    // `dimensions`: the quad's size in pixels.
    @location(1) dimensions: vec2<f32>,
    @location(2) shader_values: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) alt_color: vec4<f32>,
    @location(5) thumb_color: vec4<f32>,
    @location(6) mod_color: vec4<f32>,
    @location(7) background_color: vec4<f32>,
    // thickness, rounding, thumb_amount, start_pos
    @location(8) params: vec4<f32>,
    // max_arc, alpha_mult, kind, unused
    @location(9) params2: vec4<f32>,
    // `setCoordinates`: the bottom left corner's coordinates and their
    // extent across the quad.
    @location(10) coordinates: vec4<f32>,
};

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) dimensions: vec2<f32>,
    @location(1) coordinates: vec2<f32>,
    @location(2) shader_values: vec4<f32>,
    @location(3) @interpolate(flat) color: vec4<f32>,
    @location(4) @interpolate(flat) alt_color: vec4<f32>,
    @location(5) @interpolate(flat) thumb_color: vec4<f32>,
    @location(6) @interpolate(flat) mod_color: vec4<f32>,
    @location(7) @interpolate(flat) background_color: vec4<f32>,
    @location(8) @interpolate(flat) params: vec4<f32>,
    @location(9) @interpolate(flat) params2: vec4<f32>,
};

const KIND_COLOR: u32 = 0u;
const KIND_FADE_SQUARE: u32 = 1u;
const KIND_CIRCLE: u32 = 2u;
const KIND_RING: u32 = 3u;
const KIND_ROUNDED_CORNER: u32 = 4u;
const KIND_ROUNDED_RECTANGLE: u32 = 5u;
const KIND_DIAMOND: u32 = 6u;
const KIND_ROUNDED_RECTANGLE_BORDER: u32 = 7u;
const KIND_ROTARY_SLIDER: u32 = 8u;
const KIND_ROTARY_MODULATION: u32 = 9u;
const KIND_HORIZONTAL_SLIDER: u32 = 10u;
const KIND_VERTICAL_SLIDER: u32 = 11u;
const KIND_LINEAR_MODULATION: u32 = 12u;
const KIND_MODULATION_KNOB: u32 = 13u;
const KIND_GAIN_METER: u32 = 14u;
const KIND_BAR: u32 = 15u;

const PI: f32 = 3.14159265359;

// Upstream's four vertices: (x, y), (x, y + h), (x + w, y + h), (x + w, y),
// with coordinates from `setCoordinates` (by default (-1, -1) to (1, 1)).
// Drawn here as a triangle strip.
@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, instance: Instance) -> Varyings {
    var corners = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vertex];
    let unit = corner * 0.5 + vec2<f32>(0.5, 0.5);
    var out: Varyings;
    out.position = vec4<f32>(
        instance.position.x + unit.x * instance.position.z,
        instance.position.y + unit.y * instance.position.w,
        0.0,
        1.0,
    );
    out.dimensions = instance.dimensions;
    out.coordinates = instance.coordinates.xy + unit * instance.coordinates.zw;
    out.shader_values = instance.shader_values;
    out.color = instance.color;
    out.alt_color = instance.alt_color;
    out.thumb_color = instance.thumb_color;
    out.mod_color = instance.mod_color;
    out.background_color = instance.background_color;
    out.params = instance.params;
    out.params2 = instance.params2;
    return out;
}

// GLSL's mod(x, y).
fn glsl_mod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

fn fade_square(v: Varyings) -> vec4<f32> {
    let alpha1 = clamp((v.dimensions.x - abs(v.coordinates.x) * v.dimensions.x) * 0.5, 0.0, 1.0);
    let alpha2 = clamp((v.dimensions.y - abs(v.coordinates.y) * v.dimensions.y) * 0.5, 0.0, 1.0);
    var color = v.color;
    color.a = v.color.a * alpha1 * alpha2 * v.shader_values.x;
    return color;
}

fn circle(v: Varyings) -> vec4<f32> {
    let delta_center = length(v.coordinates) * 0.5 * v.dimensions.x;
    let alpha = clamp(v.dimensions.x * 0.5 - delta_center, 0.0, 1.0);
    var color = v.color;
    color.a = v.color.a * alpha;
    return color;
}

fn ring(v: Varyings) -> vec4<f32> {
    let thickness = v.params.x;
    let full_radius = 0.5 * v.dimensions.x;
    let delta_center = length(v.coordinates) * full_radius;
    let alpha_out = clamp(full_radius - delta_center, 0.0, 1.0);
    let alpha_in = clamp(delta_center - full_radius + thickness + 1.0, 0.0, 1.0);
    var color = v.color * alpha_in + (1.0 - alpha_in) * v.alt_color;
    color.a = color.a * alpha_out;
    return color;
}

fn rounded_corner(v: Varyings) -> vec4<f32> {
    let delta_center = length(v.coordinates * v.dimensions);
    let alpha = clamp(delta_center - v.dimensions.x + 0.5, 0.0, 1.0);
    var color = v.color;
    color.a = v.color.a * alpha;
    return color;
}

fn rounded_rectangle(v: Varyings) -> vec4<f32> {
    let rounding = v.params.y;
    let center_offset = abs(v.coordinates) * v.dimensions - v.dimensions;
    let delta_center = length(max(center_offset + vec2<f32>(rounding, rounding), vec2<f32>(0.0, 0.0)));
    let alpha = clamp((rounding - delta_center) * 0.5 + 0.5, 0.0, 1.0);
    var color = v.color;
    color.a = v.color.a * alpha;
    return color;
}

fn diamond(v: Varyings) -> vec4<f32> {
    let thickness = v.params.x;
    let full_radius = 0.5 * v.dimensions.x;
    let delta_center = (abs(v.coordinates.x) + abs(v.coordinates.y)) * full_radius;
    let alpha_out = clamp(full_radius - delta_center, 0.0, 1.0);
    let alpha_in = clamp(delta_center - full_radius + thickness + 1.0, 0.0, 1.0);
    var color = v.color * alpha_in + (1.0 - alpha_in) * v.alt_color;
    color.a = color.a * alpha_out;
    return color;
}

fn rounded_rectangle_border(v: Varyings) -> vec4<f32> {
    let thickness = v.params.x;
    let rounding = v.params.y;
    let alpha_mult = v.params2.y;
    let center_offset = abs(v.coordinates) * v.dimensions - v.dimensions;
    let delta_center = length(max(center_offset + vec2<f32>(rounding, rounding), vec2<f32>(0.0, 0.0)));
    let inside_rounding = rounding + 2.0 * thickness;
    let delta_center_inside =
        length(max(center_offset + vec2<f32>(inside_rounding, inside_rounding), vec2<f32>(0.0, 0.0)));
    let border_delta = (rounding - delta_center) * 0.5;
    let inside_border_delta = (rounding - delta_center_inside) * 0.5;
    let alpha = clamp(border_delta + 0.5, 0.0, 1.0) * clamp(-inside_border_delta + 0.5, 0.0, 1.0);
    var color = v.color;
    color.a = v.color.a * alpha_mult * alpha;
    return color;
}

fn rotary_slider(v: Varyings) -> vec4<f32> {
    let thickness = v.params.x;
    let thumb_amount = v.params.z;
    let start_pos = v.params.w;
    let max_arc = v.params2.x;
    let rads = atan2(v.coordinates.x, v.coordinates.y);
    let full_radius = 0.5 * v.dimensions.x;
    let delta_center = length(v.coordinates) * full_radius;
    let center_arc = full_radius - thickness * 0.5 - 0.5;
    let delta_arc = delta_center - center_arc;
    let distance_arc = abs(delta_arc);
    let dist_curve_left = max(center_arc * (rads - max_arc), 0.0);
    let dist_curve = max(center_arc * (-rads - max_arc), dist_curve_left);
    let alpha = clamp(thickness * 0.5 - length(vec2<f32>(distance_arc, dist_curve)) + 0.5, 0.0, 1.0);
    let delta_rads = rads - v.shader_values.x;
    let color_step1 = step(0.0, delta_rads);
    let color_step2 = step(0.0, start_pos - rads);
    let color_step = abs(color_step2 - color_step1);
    var color = v.alt_color * color_step + v.color * (1.0 - color_step);
    color.a = color.a * alpha;
    let thumb_length = full_radius * thumb_amount;
    let thumb_x = sin(delta_rads) * delta_center;
    let thumb_y = cos(delta_rads) * delta_center - center_arc;
    let adjusted_thumb_y = min(thumb_y + thumb_length, 0.0);
    let outside_arc_step = step(0.0, thumb_y);
    let thumb_y_distance = thumb_y * outside_arc_step + adjusted_thumb_y * (1.0 - outside_arc_step);
    let thumb_distance = length(vec2<f32>(thumb_x, thumb_y_distance));
    let thumb_alpha = clamp(thickness * 0.5 - thumb_distance + 0.5, 0.0, 1.0);
    return color * (1.0 - thumb_alpha) + v.thumb_color * thumb_alpha;
}

fn rotary_modulation(v: Varyings) -> vec4<f32> {
    let thickness = v.params.x;
    let start_pos = v.params.w;
    let alpha_mult = v.params2.y;
    let full_radius = v.dimensions.x * 0.5;
    let dist = length(v.coordinates) * full_radius;
    let inner_radius = full_radius - thickness;
    let dist_outer_amp = clamp((full_radius - dist) * 0.5 + 0.5, 0.0, 1.0);
    let dist_amp = dist_outer_amp * clamp((dist - inner_radius) * 0.5 + 0.5, 0.0, 1.0);
    let rads = glsl_mod(atan2(v.coordinates.x, v.coordinates.y) + PI + start_pos, 2.0 * PI) - PI;
    let rads_amp_low = clamp(full_radius * 0.5 * (rads - v.shader_values.x) + 1.0, 0.0, 1.0);
    let rads_amp_high = clamp(full_radius * 0.5 * (v.shader_values.y - rads) + 1.0, 0.0, 1.0);
    let rads_amp_low_stereo = clamp(full_radius * 0.5 * (rads - v.shader_values.z) + 0.5, 0.0, 1.0);
    let rads_amp_high_stereo = clamp(full_radius * 0.5 * (v.shader_values.w - rads) + 0.5, 0.0, 1.0);
    let alpha = rads_amp_low * rads_amp_high;
    let alpha_stereo = rads_amp_low_stereo * rads_amp_high_stereo;
    let alpha_center = min(alpha, alpha_stereo);
    let color_center = alpha_center * v.mod_color;
    var out_color = v.color * (1.0 - alpha_stereo) + v.alt_color * alpha_stereo;
    out_color = out_color * (1.0 - alpha_center) + color_center * alpha_center;
    out_color.a = max(alpha, alpha_stereo) * alpha_mult * dist_amp;
    return out_color;
}

fn horizontal_slider(v: Varyings) -> vec4<f32> {
    let thickness = v.params.x;
    let rounding = v.params.y;
    let thumb_amount = v.params.z;
    let start_pos = v.params.w;
    let position = v.coordinates * v.dimensions;
    let center_offset = abs(position) - vec2<f32>(v.dimensions.x, thickness);
    let delta_center = length(max(center_offset + vec2<f32>(rounding, rounding), vec2<f32>(0.0, 0.0)));
    let alpha = clamp((rounding - delta_center) * 0.5 + 0.5, 0.0, 1.0);
    let adjusted_value = v.shader_values.x * 2.0 - 1.0;
    let delta_pos = v.coordinates.x - adjusted_value;
    let color_step1 = step(0.001, delta_pos);
    let color_step2 = step(0.001, start_pos - v.coordinates.x);
    let color_step = abs(color_step2 - color_step1);
    var color = v.alt_color * color_step + v.color * (1.0 - color_step);
    color.a = color.a * alpha;
    let thumb_center_offset =
        abs(position - vec2<f32>(adjusted_value * v.dimensions.x, 0.0)) - vec2<f32>(thumb_amount, thickness);
    let thumb_delta_center = length(max(thumb_center_offset + vec2<f32>(rounding, rounding), vec2<f32>(0.0, 0.0)));
    let thumb_alpha = clamp((rounding - thumb_delta_center) * 0.5 + 0.5, 0.0, 1.0) * alpha;
    return color * (1.0 - thumb_alpha) + v.thumb_color * thumb_alpha;
}

fn vertical_slider(v: Varyings) -> vec4<f32> {
    let thickness = v.params.x;
    let rounding = v.params.y;
    let thumb_amount = v.params.z;
    let start_pos = v.params.w;
    let position = v.coordinates * v.dimensions;
    let center_offset = abs(position) - vec2<f32>(thickness, v.dimensions.y);
    let delta_center = length(max(center_offset + vec2<f32>(rounding, rounding), vec2<f32>(0.0, 0.0)));
    let alpha = clamp((rounding - delta_center) * 0.5 + 0.5, 0.0, 1.0);
    let adjusted_value = v.shader_values.x * 2.0 - 1.0;
    let delta_pos = v.coordinates.y - adjusted_value;
    let color_step1 = step(0.001, delta_pos);
    let color_step2 = step(0.001, start_pos - v.coordinates.y);
    let color_step = abs(color_step2 - color_step1);
    var color = v.color * color_step + v.alt_color * (1.0 - color_step);
    color.a = color.a * alpha;
    let thumb_center_offset =
        abs(position - vec2<f32>(0.0, adjusted_value * v.dimensions.y)) - vec2<f32>(thickness, thumb_amount);
    let thumb_delta_center = length(max(thumb_center_offset + vec2<f32>(rounding, rounding), vec2<f32>(0.0, 0.0)));
    let thumb_alpha = clamp((rounding - thumb_delta_center) * 0.5 + 0.5, 0.0, 1.0) * alpha;
    return color * (1.0 - thumb_alpha) + v.thumb_color * thumb_alpha;
}

fn linear_modulation(v: Varyings) -> vec4<f32> {
    let position = v.coordinates.x * 0.5 + 0.5;
    let dist1 = clamp(200.0 * (position - v.shader_values.x), 0.0, 1.0);
    let dist2 = clamp(200.0 * (v.shader_values.y - position), 0.0, 1.0);
    let stereo_dist1 = clamp(200.0 * (position - v.shader_values.z), 0.0, 1.0);
    let stereo_dist2 = clamp(200.0 * (v.shader_values.w - position), 0.0, 1.0);
    let alpha = dist1 * dist2;
    let alpha_stereo = stereo_dist1 * stereo_dist2;
    let alpha_center = min(alpha, alpha_stereo);
    let color_left = (alpha - alpha_center) * v.color;
    let color_right = (alpha_stereo - alpha_center) * v.alt_color;
    let color_center = alpha_center * v.mod_color;
    var color = color_left + color_right + color_center;
    color.a = max(alpha, alpha_stereo);
    return color;
}

fn modulation_knob(v: Varyings) -> vec4<f32> {
    let thickness = v.params.x;
    let alpha_mult = v.params2.y;
    let rads = atan2(v.coordinates.x, -v.coordinates.y);
    let full_radius = 0.5 * v.dimensions.x;
    let delta_center = length(v.coordinates) * full_radius;
    let circle_alpha = clamp(full_radius - delta_center, 0.0, 1.0);
    let delta_rads = rads - v.shader_values.x;
    let color_amount = clamp(delta_rads * max(delta_center, 1.0) * 1.6, 0.0, 1.0);
    var color = v.alt_color * color_amount + v.color * (1.0 - color_amount);
    color.a = color.a * circle_alpha;
    let center_arc = full_radius - thickness * 0.5 - 0.5;
    let delta_arc = delta_center - center_arc;
    let distance_arc = abs(delta_arc);
    let thumb_alpha = clamp(thickness * 0.5 - distance_arc + 0.5, 0.0, 1.0);
    color = color * (1.0 - thumb_alpha) + v.thumb_color * thumb_alpha;
    let mod_alpha1 = clamp(full_radius * 0.48 - delta_center, 0.0, 1.0) * v.mod_color.a;
    let mod_alpha2 = clamp(full_radius * 0.35 - delta_center, 0.0, 1.0) * v.mod_color.a;
    color = color * (1.0 - mod_alpha1) + v.background_color * mod_alpha1;
    color = color * (1.0 - mod_alpha2) + v.mod_color * mod_alpha2;
    color.a = color.a * alpha_mult;
    return color;
}

// kGainMeterFragmentShader: a gradient across the component, whose device
// x arrives as the coordinates.
fn gain_meter(v: Varyings) -> vec4<f32> {
    let t = (v.coordinates.x + 1.0) / 2.0;
    return v.alt_color * t + v.color * (1.0 - t);
}

// kBarFragmentShader (`BarRenderer`): the colour, fading out towards each
// edge. `corner` runs 0 to 1 across the bar; `size` (shader values x and y)
// is the bar vertex shaders' varying, in pixels.
fn bar(v: Varyings) -> vec4<f32> {
    let corner = v.coordinates * 0.5 + vec2<f32>(0.5, 0.5);
    let size = v.shader_values.xy;
    let alpha_x = min(corner.x * size.x, (1.0 - corner.x) * size.x);
    let alpha_y = min(corner.y * size.y, (1.0 - corner.y) * size.y);
    var color = v.color;
    color.a = v.color.a * min(1.0, min(alpha_x, alpha_y));
    return color;
}

@fragment
fn fs_main(v: Varyings) -> @location(0) vec4<f32> {
    let kind = u32(v.params2.z);
    switch kind {
        case KIND_FADE_SQUARE: { return fade_square(v); }
        case KIND_CIRCLE: { return circle(v); }
        case KIND_RING: { return ring(v); }
        case KIND_ROUNDED_CORNER: { return rounded_corner(v); }
        case KIND_ROUNDED_RECTANGLE: { return rounded_rectangle(v); }
        case KIND_DIAMOND: { return diamond(v); }
        case KIND_ROUNDED_RECTANGLE_BORDER: { return rounded_rectangle_border(v); }
        case KIND_ROTARY_SLIDER: { return rotary_slider(v); }
        case KIND_ROTARY_MODULATION: { return rotary_modulation(v); }
        case KIND_HORIZONTAL_SLIDER: { return horizontal_slider(v); }
        case KIND_VERTICAL_SLIDER: { return vertical_slider(v); }
        case KIND_LINEAR_MODULATION: { return linear_modulation(v); }
        case KIND_MODULATION_KNOB: { return modulation_knob(v); }
        case KIND_GAIN_METER: { return gain_meter(v); }
        case KIND_BAR: { return bar(v); }
        default: { return v.color; }
    }
}
