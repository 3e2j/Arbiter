// Indexed triangles from gui::canvas, each vertex placed in logical pixels and
// its colour interpolated across the triangle. Edges aren't antialiased here.
// A shape that wants a soft edge brings its own fringe of transparent vertices.

struct Viewport {
    // In logical pixels.
    size: vec2<f32>,
    // Physical pixels per logical one.
    scale: f32,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;

struct Vertex {
    @location(0) at: vec2<f32>,
    // sRGB, straight alpha.
    @location(1) color: vec4<f32>,
}

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs(vertex: Vertex) -> Varyings {
    let ndc = vertex.at / viewport.size * 2.0 - 1.0;
    var out: Varyings;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.color = linear(vertex.color);
    return out;
}

fn linear(c: vec4<f32>) -> vec4<f32> {
    let low = c.rgb / 12.92;
    let high = pow((c.rgb + 0.055) / 1.055, vec3<f32>(2.4));
    return vec4<f32>(select(high, low, c.rgb <= vec3<f32>(0.04045)), c.a);
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color.rgb * in.color.a, in.color.a);
}
