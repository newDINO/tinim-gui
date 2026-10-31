// Vertex positions are in physical pixels; the viewport maps them onto NDC with y flipped.
struct Globals {
    screen_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

// The bound texture supplies color and coverage. Untextured geometry samples the renderer's
// opaque white texel (TextureId::WHITE).
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var tex_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let ndc = vec2<f32>(
        in.position.x / globals.screen_size.x * 2.0 - 1.0,
        1.0 - in.position.y / globals.screen_size.y * 2.0,
    );
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(tex, tex_sampler, in.uv);
    let alpha = in.color.a * texel.a;
    // Premultiplied alpha: color is already multiplied by alpha.
    let rgb = srgb_to_linear(in.color.rgb) * texel.rgb * alpha;
    return vec4<f32>(rgb, alpha);
}
