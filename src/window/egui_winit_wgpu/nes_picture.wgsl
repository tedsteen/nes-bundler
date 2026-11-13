// Draws the NES picture with sharp bilinear filtering: nearest-neighbour within each NES pixel,
// and a blend only across the physical pixel that a pixel edge falls inside. With a whole number
// of physical pixels per NES pixel (the rows, see integer_scaling.rs) it's exactly nearest; the
// columns, at the NES's own pixel shape, aren't whole, and get the blend at their edges rather
// than some being a pixel wider than others.

struct Uniforms {
    texture_size: vec2<f32>,
    // The size the picture is drawn at, in physical pixels.
    output_size: vec2<f32>,
    // Multiplied in, in gamma space as egui does (the menu darkens the game behind it).
    tint: vec4<f32>,
    // x: 1 when the target is sRGB, so the gamma-encoded result has to be made linear first.
    flags: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var picture: texture_2d<f32>;
@group(0) @binding(2) var picture_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle covering the viewport, which egui sets to the picture's rectangle.
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOutput;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn linear_from_gamma(srgb: vec3<f32>) -> vec3<f32> {
    let cutoff = srgb < vec3<f32>(0.04045);
    let lower = srgb / vec3<f32>(12.92);
    let higher = pow((srgb + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(higher, lower, cutoff);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = in.uv * uniforms.texture_size;
    // Whole physical pixels per NES pixel, per direction.
    let scale = max(floor(uniforms.output_size / uniforms.texture_size), vec2<f32>(1.0));
    // Within this distance of a pixel's center, sample its center; past it, blend towards the
    // neighbour over one physical pixel.
    let region = vec2<f32>(0.5) - vec2<f32>(0.5) / scale;
    let from_center = fract(texel) - vec2<f32>(0.5);
    let offset = (from_center - clamp(from_center, -region, region)) * scale + vec2<f32>(0.5);
    let sample_at = (floor(texel) + offset) / uniforms.texture_size;

    var color = textureSample(picture, picture_sampler, sample_at) * uniforms.tint;
    if uniforms.flags.x > 0.5 {
        color = vec4<f32>(linear_from_gamma(color.rgb), color.a);
    }
    return vec4<f32>(color.rgb, 1.0);
}
