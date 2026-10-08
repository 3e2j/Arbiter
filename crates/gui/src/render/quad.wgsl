// One instance per Quad from gui::canvas, drawn as a 4-vertex strip. Corners
// and borders come from a signed distance to the rounded rect, antialiased
// over one physical pixel. Text and icons scale the fill by the coverage
// atlas, and colour glyphs replace it with the colour atlas.

struct Viewport {
    // In logical pixels.
    size: vec2<f32>,
    // Physical pixels per logical one.
    scale: f32,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;
// A layer per page. Coverage in red, and colour with straight alpha.
@group(1) @binding(0) var coverage_atlas: texture_2d_array<f32>;
@group(1) @binding(1) var color_atlas: texture_2d_array<f32>;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) texels: vec4<u32>,
    // In sixteenths of a logical pixel.
    @location(2) radii: vec4<u32>,
    // sRGB, straight alpha.
    @location(3) fill: vec4<f32>,
    @location(4) border: vec4<f32>,
    @location(5) border_width: f32,
    // The page, then 0 for the coverage atlas or 1 for the colour one.
    @location(6) atlas: vec2<u32>,
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
    @location(7) @interpolate(flat) atlas: vec2<u32>,
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
    out.fill = linear(quad.fill);
    out.border = linear(quad.border);
    out.radii = vec4<f32>(quad.radii) / 16.0;
    out.border_width = quad.border_width;
    out.texels = vec4<f32>(quad.texels);
    out.atlas = quad.atlas;
    return out;
}

fn linear(c: vec4<f32>) -> vec4<f32> {
    let low = c.rgb / 12.92;
    let high = pow((c.rgb + 0.055) / 1.055, vec3<f32>(2.4));
    return vec4<f32>(select(high, low, c.rgb <= vec3<f32>(0.04045)), c.a);
}

// Negative inside. Radii are top left, top right, bottom right, bottom left.
fn rounded_rect(p: vec2<f32>, half: vec2<f32>, radii: vec4<f32>) -> f32 {
    let top = select(radii.x, radii.y, p.x > 0.0);
    let bottom = select(radii.w, radii.z, p.x > 0.0);
    let r = min(select(top, bottom, p.y > 0.0), min(half.x, half.y));
    let q = abs(p) - half + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// The fill, premultiplied, scaled by the coverage texel under this pixel or
// replaced by the colour one. A sampling quad is its texel area's size in
// physical pixels, so each pixel reads exactly one texel.
fn fill_under(in: Varyings) -> vec4<f32> {
    if in.texels.z <= 0.0 {
        return vec4<f32>(in.fill.rgb * in.fill.a, in.fill.a);
    }
    let texel = floor((in.local + in.half) * viewport.scale);
    let at = vec2<i32>(in.texels.xy + clamp(texel, vec2<f32>(0.0), in.texels.zw - 1.0));
    if in.atlas.y == 0u {
        let alpha = in.fill.a * textureLoad(coverage_atlas, at, in.atlas.x, 0).r;
        return vec4<f32>(in.fill.rgb * alpha, alpha);
    }
    let color = textureLoad(color_atlas, at, in.atlas.x, 0);
    let alpha = in.fill.a * color.a;
    return vec4<f32>(color.rgb * alpha, alpha);
}

@fragment
fn fs(in: Varyings) -> @location(0) vec4<f32> {
    let d = rounded_rect(in.local, in.half, in.radii);
    let pixel = max(fwidth(d), 1e-4);
    let coverage = clamp(0.5 - d / pixel, 0.0, 1.0);
    let inside_border = clamp(0.5 - (d + in.border_width) / pixel, 0.0, 1.0);
    let t = select(inside_border, 1.0, in.border_width <= 0.0);
    let fill = fill_under(in);
    let border = vec4<f32>(in.border.rgb * in.border.a, in.border.a);
    return mix(border, fill, t) * coverage;
}
