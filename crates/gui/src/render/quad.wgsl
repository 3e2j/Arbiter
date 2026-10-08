// One instance per Quad from gui::canvas, drawn as a 4-vertex strip. Corners
// and borders come from a signed distance to the rounded rect, antialiased
// over one physical pixel. Text and icons scale the fill by the atlas.

struct Viewport {
    // In logical pixels.
    size: vec2<f32>,
    // Physical pixels per logical one.
    scale: f32,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;
// One coverage byte per texel, a layer per page.
@group(1) @binding(0) var atlas: texture_2d_array<f32>;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) fill: vec4<f32>,
    @location(2) border: vec4<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) texels: vec4<f32>,
    @location(5) border_width: f32,
    @location(6) page: u32,
}

struct Varyings {
    @builtin(position) position: vec4<f32>,
    // From the quad's centre, in logical pixels.
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) half: vec2<f32>,
    @location(2) @interpolate(flat) fill: vec4<f32>,
    @location(3) @interpolate(flat) border: vec4<f32>,
    @location(4) @interpolate(flat) radii: vec4<f32>,
    @location(5) @interpolate(flat) border_width: f32,
    @location(6) @interpolate(flat) texels: vec4<f32>,
    @location(7) @interpolate(flat) page: u32,
}

@vertex
fn vs(@builtin(vertex_index) i: u32, quad: Instance) -> Varyings {
    let corner = vec2<f32>(f32(i & 1u), f32(i >> 1u));
    let size = quad.rect.zw;
    let ndc = (quad.rect.xy + corner * size) / viewport.size * 2.0 - 1.0;
    var out: Varyings;
    out.position = vec4<f32>(ndc.x, -ndc.y, 0.0, 1.0);
    out.local = (corner - 0.5) * size;
    out.half = size * 0.5;
    out.fill = quad.fill;
    out.border = quad.border;
    out.radii = quad.radii;
    out.border_width = quad.border_width;
    out.texels = quad.texels;
    out.page = quad.page;
    return out;
}

// Negative inside. Radii are top left, top right, bottom right, bottom left.
fn rounded_rect(p: vec2<f32>, half: vec2<f32>, radii: vec4<f32>) -> f32 {
    let top = select(radii.x, radii.y, p.x > 0.0);
    let bottom = select(radii.w, radii.z, p.x > 0.0);
    let r = min(select(top, bottom, p.y > 0.0), min(half.x, half.y));
    let q = abs(p) - half + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// The atlas texel under this pixel, or full coverage for a quad that samples
// nothing. A sampling quad is its texel area's size in physical pixels, so
// each pixel reads exactly one texel.
fn coverage_under(in: Varyings) -> f32 {
    if in.texels.z <= 0.0 {
        return 1.0;
    }
    let texel = floor((in.local + in.half) * viewport.scale);
    let at = in.texels.xy + clamp(texel, vec2<f32>(0.0), in.texels.zw - 1.0);
    return textureLoad(atlas, vec2<i32>(at), in.page, 0).r;
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
    let d = rounded_rect(in.local, in.half, in.radii);
    let pixel = max(fwidth(d), 1e-4);
    let coverage = clamp(0.5 - d / pixel, 0.0, 1.0);
    let inside_border = clamp(0.5 - (d + in.border_width) / pixel, 0.0, 1.0);
    let t = select(inside_border, 1.0, in.border_width <= 0.0);
    let alpha = in.fill.a * coverage_under(in);
    let fill = vec4<f32>(in.fill.rgb * alpha, alpha);
    let border = vec4<f32>(in.border.rgb * in.border.a, in.border.a);
    return mix(border, fill, t) * coverage;
}
