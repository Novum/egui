// Vertex shader bindings

struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @location(2) @interpolate(flat) vector_origin: vec2<i32>,
    @location(3) vector_coord: vec2<f32>,
    @location(1) color: vec4<f32>, // gamma 0-1
    @builtin(position) position: vec4<f32>,
};

struct Locals {
    screen_size: vec2<f32>,

    /// 1 if dithering is enabled, 0 otherwise
    dithering: u32,

    /// 1 to do manual filtering for more predictable kittest snapshot images.
    /// See also https://github.com/emilk/egui/issues/5295
    predictable_texture_filtering: u32,
    pixel_size: vec4<f32>,
};
@group(0) @binding(0) var<uniform> r_locals: Locals;


// -----------------------------------------------
// Adapted from
// https://www.shadertoy.com/view/llVGzG
// Originally presented in:
// Jimenez 2014, "Next Generation Post-Processing in Call of Duty"
//
// A good overview can be found in
// https://blog.demofox.org/2022/01/01/interleaved-gradient-noise-a-different-kind-of-low-discrepancy-sequence/
// via https://github.com/rerun-io/rerun/
fn interleaved_gradient_noise(n: vec2<f32>) -> f32 {
    let f = 0.06711056 * n.x + 0.00583715 * n.y;
    return fract(52.9829189 * fract(f));
}

fn dither_interleaved(rgb: vec3<f32>, levels: f32, frag_coord: vec4<f32>) -> vec3<f32> {
    var noise = interleaved_gradient_noise(frag_coord.xy);
    // scale down the noise slightly to ensure flat colors aren't getting dithered
    noise = (noise - 0.5) * 0.95;
    return rgb + noise / (levels - 1.0);
}

// 0-1 linear  from  0-1 sRGB gamma
fn linear_from_gamma_rgb(srgb: vec3<f32>) -> vec3<f32> {
    let cutoff = srgb < vec3<f32>(0.04045);
    let lower = srgb / vec3<f32>(12.92);
    let higher = pow((srgb + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(higher, lower, cutoff);
}

// 0-1 sRGB gamma  from  0-1 linear
fn gamma_from_linear_rgb(rgb: vec3<f32>) -> vec3<f32> {
    let cutoff = rgb < vec3<f32>(0.0031308);
    let lower = rgb * vec3<f32>(12.92);
    let higher = vec3<f32>(1.055) * pow(rgb, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(higher, lower, cutoff);
}

// 0-1 sRGBA gamma  from  0-1 linear
fn gamma_from_linear_rgba(linear_rgba: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(gamma_from_linear_rgb(linear_rgba.rgb), linear_rgba.a);
}

// [u8; 4] SRGB as u32 -> [r, g, b, a] in 0.-1
fn unpack_color(color: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(color & 255u),
        f32((color >> 8u) & 255u),
        f32((color >> 16u) & 255u),
        f32((color >> 24u) & 255u),
    ) / 255.0;
}

fn position_from_screen(screen_pos: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(
        2.0 * screen_pos.x / r_locals.screen_size.x - 1.0,
        1.0 - 2.0 * screen_pos.y / r_locals.screen_size.y,
        0.0,
        1.0,
    );
}

@vertex
fn vs_main(
    @location(0) a_pos: vec2<f32>,
    @location(1) a_tex_coord: vec2<f32>,
    @location(2) a_color: u32,
    @location(3) a_glyph_basis_y: vec2<f32>,
    @location(4) a_glyph_origin: vec2<u32>,
) -> VertexOutput {
    var out: VertexOutput;
    var position = a_pos;
    out.tex_coord = a_tex_coord;
    out.vector_origin = vec2<i32>(i32(a_glyph_origin.x) - 1, i32(a_glyph_origin.y & 16383u));
    out.vector_coord = vec2<f32>(f32((a_glyph_origin.y >> 14u) & 1u), f32(a_glyph_origin.y >> 15u));
    let u = a_tex_coord;
    let v = a_glyph_basis_y;
    let det = u.x * v.y - u.y * v.x;
    if a_glyph_origin.x != 0u && abs(det) > 1e-20 {
        // Inverse Jacobian projects a screen pixel onto the two glyph axes.
        let margin = vec2<f32>(abs(v.x) + abs(v.y), abs(u.x) + abs(u.y))
            * (r_locals.pixel_size.x / abs(det));
        let dilation = (2.0 * out.vector_coord - vec2<f32>(1.0)) * margin;
        position += u * dilation.x + v * dilation.y;
        out.vector_coord += dilation;
    }
    out.color = unpack_color(a_color);
    out.position = position_from_screen(position);
    return out;
}

// Fragment shader bindings

@group(1) @binding(0) var r_tex_color: texture_2d<f32>;
@group(1) @binding(1) var r_tex_sampler: sampler;


/// Set in bit 0 of `r_tex_flags` if the sampler uses nearest filtering.
///
/// Must match `TEX_FLAG_NEAREST` in `renderer.rs`.
const TEX_FLAG_NEAREST: u32 = 1u;

/// Wrap modes, stored in bits 1+ of `r_tex_flags`.
///
/// Must match the `WRAP_MODE_*` constants in `renderer.rs`.
const WRAP_MODE_CLAMP_TO_EDGE: u32 = 0u;
const WRAP_MODE_REPEAT: u32 = 1u;
const WRAP_MODE_MIRRORED_REPEAT: u32 = 2u;

/// Texture flags, only read when `predictable_texture_filtering` is on.
///
/// Bit 0: `TEX_FLAG_NEAREST`.
/// Bits 1+: one of the `WRAP_MODE_*` constants.
@group(1) @binding(2) var<uniform> r_tex_flags: vec4<u32>;

/// Map a texel coordinate to a valid texel according to the texture's wrap mode.
fn wrap_texel_coord(coord: vec2<i32>, texture_size: vec2<i32>) -> vec2<i32> {
    let wrap_mode = r_tex_flags[0] >> 1u;
    if wrap_mode == WRAP_MODE_REPEAT {
        return ((coord % texture_size) + texture_size) % texture_size;
    } else if wrap_mode == WRAP_MODE_MIRRORED_REPEAT {
        let period = 2 * texture_size;
        let phase = ((coord % period) + period) % period;
        return min(phase, period - vec2<i32>(1, 1) - phase);
    } else {
        // WRAP_MODE_CLAMP_TO_EDGE
        return clamp(coord, vec2<i32>(0, 0), texture_size - vec2<i32>(1, 1));
    }
}


fn sample_texture(in: VertexOutput) -> vec4<f32> {
    if r_locals.predictable_texture_filtering == 0 {
        // Hardware filtering: fast, but varies across GPUs and drivers.
        return textureSample(r_tex_color, r_tex_sampler, in.tex_coord);
    } else {
        let texture_size = vec2<i32>(textureDimensions(r_tex_color, 0));
        let texture_size_f = vec2<f32>(texture_size);

        if (r_tex_flags[0] & TEX_FLAG_NEAREST) != 0u {
            // Nearest filtering: load the texel under the sample position.
            let texel = wrap_texel_coord(vec2<i32>(floor(in.tex_coord * texture_size_f)), texture_size);
            return textureLoad(r_tex_color, texel, 0);
        }

        // Manual bilinear filtering with four taps at pixel centers using textureLoad
        let pixel_coord = in.tex_coord * texture_size_f - 0.5;
        let pixel_fract = fract(pixel_coord);
        let pixel_floor = vec2<i32>(floor(pixel_coord));

        let p00 = wrap_texel_coord(pixel_floor + vec2<i32>(0, 0), texture_size);
        let p10 = wrap_texel_coord(pixel_floor + vec2<i32>(1, 0), texture_size);
        let p01 = wrap_texel_coord(pixel_floor + vec2<i32>(0, 1), texture_size);
        let p11 = wrap_texel_coord(pixel_floor + vec2<i32>(1, 1), texture_size);

        // Load at pixel centers
        let tl = textureLoad(r_tex_color, p00, 0);
        let tr = textureLoad(r_tex_color, p10, 0);
        let bl = textureLoad(r_tex_color, p01, 0);
        let br = textureLoad(r_tex_color, p11, 0);

        // Manual bilinear interpolation
        let top = mix(tl, tr, pixel_fract.x);
        let bottom = mix(bl, br, pixel_fract.x);
        return mix(top, bottom, pixel_fract.y);
    }
}

@group(2) @binding(0) var r_curves: texture_2d<f32>;
@group(2) @binding(1) var r_bands: texture_2d<u32>;

fn vector_band(origin: vec2<i32>, address: u32) -> vec4<u32> {
    let width = textureDimensions(r_bands).x;
    let index = u32(origin.y) * width + u32(origin.x) + address;
    return textureLoad(r_bands, vec2<i32>(i32(index % width), i32(index / width)), 0);
}

fn vector_curve(address: u32) -> vec4<f32> {
    let width = textureDimensions(r_curves).x;
    return textureLoad(r_curves, vec2<i32>(i32(address % width), i32(address / width)), 0);
}

// Each sign class selects the eligible roots, including shared endpoints.
fn vector_roots(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> vec2<f32> {
    let a = p0 - 2.0 * p1 + p2;
    let b = p0 - p1;
    if abs(a.y) < 1.0 / 65536.0 {
        // Horizontal curves have no eligible roots.
        let t = p0.y / select(2.0 * b.y, 1.0, abs(b.y) < 1e-12);
        let x = (a.x * t - 2.0 * b.x) * t + p0.x;
        return vec2<f32>(x);
    }
    let d = sqrt(max(b.y * b.y - a.y * p0.y, 0.0));
    let t = (vec2<f32>(b.y) + vec2<f32>(-d, d)) / a.y;
    return (a.x * t - vec2<f32>(2.0 * b.x)) * t + vec2<f32>(p0.x);
}

// Returns signed coverage and the confidence of this ray near an edge.
fn vector_ray(origin: vec2<i32>, p: vec2<f32>, width: vec2<f32>, base: u32, axis: u32) -> vec2<f32> {
    let band = min(u32(clamp(p[1u - axis], 0.0, 1.0) * 16.0), 15u);
    let list = vector_band(origin, base + 1u + axis * 16u + band);
    var coverage = 0.0;
    var weight = 0.0;
    for (var i = 0u; i < list.y; i++) {
        let address = vector_band(origin, list.x + i / 4u)[i % 4u];
        let points = vector_curve(address);
        var p0 = points.xy - p;
        var p1 = points.zw - p;
        var p2 = vector_curve(address + 1u).xy - p;
        if axis == 1u {
            p0 = p0.yx;
            p1 = p1.yx;
            p2 = p2.yx;
        }
        if max(max(p0.x, p1.x), p2.x) < -0.5 * width[axis] {
            break;
        }
        let signs = u32(p0.y < 0.0) | (u32(p1.y < 0.0) << 1u) | (u32(p2.y < 0.0) << 2u);
        let code = (0x2e74u >> signs) & 0x101u;
        if code != 0u {
            let roots = vector_roots(p0, p1, p2) / width[axis];
            if (code & 1u) != 0u {
                coverage += clamp(roots.x + 0.5, 0.0, 1.0);
                weight = max(weight, clamp(1.0 - 2.0 * abs(roots.x), 0.0, 1.0));
            }
            if (code & 0x100u) != 0u {
                coverage -= clamp(roots.y + 0.5, 0.0, 1.0);
                weight = max(weight, clamp(1.0 - 2.0 * abs(roots.y), 0.0, 1.0));
            }
        }
    }
    return vec2<f32>(select(coverage, -coverage, axis == 1u), weight);
}

fn vector_coverage(origin: vec2<i32>, p: vec2<f32>, width: vec2<f32>, base: u32) -> f32 {
    // Cross - axis samples integrate coverage over the pixel footprint.
    let samples = clamp(vec2<u32>(width * 16.0 + vec2<f32>(1.0)), vec2<u32>(1u), vec2<u32>(4u));
    var x = vec2<f32>(0.0);
    var y = vec2<f32>(0.0);
    for (var i = 0u; i < samples.y; i++) {
        let offset = ((f32(i) + 0.5) / f32(samples.y) - 0.5) * width.y;
        let ray = vector_ray(origin, p + vec2<f32>(0.0, offset), width, base, 0u);
        x.x += ray.x / f32(samples.y);
        x.y = max(x.y, ray.y);
    }
    for (var i = 0u; i < samples.x; i++) {
        let offset = ((f32(i) + 0.5) / f32(samples.x) - 0.5) * width.x;
        let ray = vector_ray(origin, p + vec2<f32>(offset, 0.0), width, base, 1u);
        y.x += ray.x / f32(samples.x);
        y.y = max(y.y, ray.y);
    }
    let weighted = abs(x.x * x.y + y.x * y.y) / max(x.y + y.y, 1.0 / 65536.0);
    var coverage = clamp(max(weighted, min(abs(x.x), abs(y.x))), 0.0, 1.0);
    let mode = vector_band(origin, base).x;
    if mode == 1u {
        let gamma = vector_curve(vector_band(origin, base).y).x;
        coverage = pow(coverage, gamma);
    } else if mode == 2u {
        coverage = 2.0 * coverage - coverage * coverage;
    }
    return coverage;
}

fn vector_stop(address: u32, foreground: vec4<f32>) -> vec4<f32> {
    let stop = vector_curve(address);
    let color = vector_curve(address + 1u);
    if stop.y == 1.0 {
        return foreground * color.a;
    }
    return color * foreground.a;
}

fn vector_radial_parameter(p: vec2<f32>, c0: vec2<f32>, c1: vec2<f32>, r0: f32, r1: f32) -> vec2<f32> {
    var t = 0.0;
    let dr = r1 - r0;
    let d = c1 - c0;
    let q = p - c0;
    let a = dot(d, d) - dr * dr;
    let b = -2.0 * (dot(q, d) + r0 * dr);
    let c = dot(q, q) - r0 * r0;
    if abs(a) < 1e-12 {
        if abs(b) < 1e-12 {
            return vec2<f32>(0.0);
        }
        t = -c / b;
        if r0 + t * dr < 0.0 {
            return vec2<f32>(0.0);
        }
    } else {
        let disc = b * b - 4.0 * a * c;
        if disc < 0.0 {
            return vec2<f32>(0.0);
        }
        let root = sqrt(disc);
        let t0 = (-b - root) / (2.0 * a);
        let t1 = (-b + root) / (2.0 * a);
        let valid0 = r0 + t0 * dr >= 0.0;
        let valid1 = r0 + t1 * dr >= 0.0;
        if !valid0 && !valid1 {
            return vec2<f32>(0.0);
        }
        t = select(t1, t0, valid0 && (!valid1 || t0 > t1));
    }
    return vec2<f32>(t, 1.0);
}

fn vector_paint(origin: vec2<i32>, base: u32, coord: vec2<f32>, foreground: vec4<f32>) -> vec4<f32> {
    let h = vector_band(origin, base);
    if h.x == 0u {
        let c = vector_curve(h.w);
        if h.y == 1u {
            return foreground * c.a;
        }
        return c * foreground.a;
    }
    let row0 = vector_curve(h.w);
    let row1 = vector_curve(h.w + 1u);
    let point = vec3<f32>(coord, 1.0);
    let p = vec2<f32>(dot(row0.xyz, point), dot(row1.xyz, point));
    let geometry0 = vector_curve(h.w + 2u);
    let geometry1 = vector_curve(h.w + 3u);
    let c0 = geometry0.xy;
    let c1 = geometry1.xy;
    var t = 0.0;
    if h.x == 1u {
        let d = c1 - c0;
        t = dot(p - c0, d) / max(dot(d, d), 1e-20);
    } else if h.x == 2u {
        let r0 = geometry0.z;
        let r1 = geometry1.z;
        let radial = vector_radial_parameter(p, c0, c1, r0, r1);
        if radial.y == 0.0 {
            return vec4<f32>(0.0);
        }
        t = radial.x;
    } else {
        let q = p - c0;
        var angle = atan2(-q.y, q.x) * 57.295779513;
        if angle < 0.0 {
            angle += 360.0;
        }
        t = (angle - c1.x) / (c1.y - c1.x);
    }
    if h.y == 1u {
        t = fract(t);
    }
    if h.y == 2u {
        t = 1.0 - abs(1.0 - (t - 2.0 * floor(t / 2.0)));
    }
    let count = h.z;
    var previous = vector_stop(h.w + 4u, foreground);
    var offset = vector_curve(h.w + 4u).x;
    if t < offset {
        return previous;
    }
    for (var i = 1u; i < count; i++) {
        let address = h.w + 4u + i * 2u;
        let next = vector_stop(address, foreground);
        let end = vector_curve(address).x;
        if t < end {
            return mix(previous, next, clamp((t - offset) / max(end - offset, 1e-20), 0.0, 1.0));
        }
        previous = next;
        offset = end;
    }
    return previous;
}

// W3C Compositing and Blending formulas for the COLR blend modes, on
// unpremultiplied colors. `b` is the backdrop, `s` the source.
fn vector_blend_channel(b: f32, s: f32, mode: u32) -> f32 {
    switch mode {
        case 13u: {
            return b + s - b * s;
        }
        case 14u: {
            if b <= 0.5 {
                return s * 2.0 * b;
            }
            let t = 2.0 * b - 1.0;
            return s + t - s * t;
        }
        case 15u: {
            return min(b, s);
        }
        case 16u: {
            return max(b, s);
        }
        case 17u: {
            if b == 0.0 {
                return 0.0;
            }
            if s >= 1.0 {
                return 1.0;
            }
            return min(1.0, b / (1.0 - s));
        }
        case 18u: {
            if b >= 1.0 {
                return 1.0;
            }
            if s == 0.0 {
                return 0.0;
            }
            return 1.0 - min(1.0, (1.0 - b) / s);
        }
        case 19u: {
            if s <= 0.5 {
                return b * 2.0 * s;
            }
            let t = 2.0 * s - 1.0;
            return b + t - b * t;
        }
        case 20u: {
            if s <= 0.5 {
                return b - (1.0 - 2.0 * s) * b * (1.0 - b);
            }
            var d = sqrt(b);
            if b <= 0.25 {
                d = ((16.0 * b - 12.0) * b + 4.0) * b;
            }
            return b + (2.0 * s - 1.0) * (d - b);
        }
        case 21u: {
            return abs(b - s);
        }
        case 22u: {
            return b + s - 2.0 * b * s;
        }
        case 23u: {
            return b * s;
        }
        default: {
            return s;
        }
    }
}

fn vector_lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.3, 0.59, 0.11));
}

fn vector_set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    let color = c + (l - vector_lum(c));
    let lum = vector_lum(color);
    let low = min(color.r, min(color.g, color.b));
    let high = max(color.r, max(color.g, color.b));
    var clipped = color;
    if low < 0.0 {
        clipped = lum + (clipped - lum) * lum / max(lum - low, 1e-20);
    }
    if high > 1.0 {
        clipped = lum + (clipped - lum) * (1.0 - lum) / max(high - lum, 1e-20);
    }
    return clipped;
}

fn vector_sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn vector_set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let low = min(c.r, min(c.g, c.b));
    let range = vector_sat(c);
    if range <= 0.0 {
        return vec3<f32>(0.0);
    }
    return (c - low) * s / range;
}

fn vector_blend(source: vec4<f32>, destination: vec4<f32>, mode: u32) -> vec4<f32> {
    let s = clamp(source.rgb / max(source.a, 1e-20), vec3<f32>(0.0), vec3<f32>(1.0));
    let b = clamp(destination.rgb / max(destination.a, 1e-20), vec3<f32>(0.0), vec3<f32>(1.0));
    var blended: vec3<f32>;
    switch mode {
        case 24u: {
            blended = vector_set_lum(vector_set_sat(s, vector_sat(b)), vector_lum(b));
        }
        case 25u: {
            blended = vector_set_lum(vector_set_sat(b, vector_sat(s)), vector_lum(b));
        }
        case 26u: {
            blended = vector_set_lum(s, vector_lum(b));
        }
        case 27u: {
            blended = vector_set_lum(b, vector_lum(s));
        }
        default: {
            blended = vec3<f32>(
                vector_blend_channel(b.r, s.r, mode),
                vector_blend_channel(b.g, s.g, mode),
                vector_blend_channel(b.b, s.b, mode),
            );
        }
    }
    let both = source.a * destination.a;
    return vec4<f32>(
        source.rgb * (1.0 - destination.a) + destination.rgb * (1.0 - source.a) + both * blended,
        source.a + destination.a - both,
    );
}

fn vector_composite(source: vec4<f32>, destination: vec4<f32>, mode: u32) -> vec4<f32> {
    switch mode {
        case 0u: {
            return vec4<f32>(0.0);
        }
        case 1u: {
            return source;
        }
        case 2u: {
            return destination;
        }
        case 3u: {
            return source + destination * (1.0 - source.a);
        }
        case 4u: {
            return destination + source * (1.0 - destination.a);
        }
        case 5u: {
            return source * destination.a;
        }
        case 6u: {
            return destination * source.a;
        }
        case 7u: {
            return source * (1.0 - destination.a);
        }
        case 8u: {
            return destination * (1.0 - source.a);
        }
        case 9u: {
            return source * destination.a + destination * (1.0 - source.a);
        }
        case 10u: {
            return destination * source.a + source * (1.0 - destination.a);
        }
        case 11u: {
            return source * (1.0 - destination.a) + destination * (1.0 - source.a);
        }
        case 12u: {
            return min(source + destination, vec4<f32>(1.0));
        }
        default: {
            return vector_blend(source, destination, mode);
        }
    }
}

fn vector_color(origin: vec2<i32>, coord: vec2<f32>, width: vec2<f32>, foreground: vec4<f32>) -> vec4<f32> {
    let header = vector_band(origin, 0u);
    if header.x != 3u {
        return foreground * vector_coverage(origin, coord, width, 0u);
    }
    var color = vec4<f32>(0.0);
    var colors: array<vec4<f32>, 16>;
    var modes: array<u32, 16>;
    var depth = 0u;
    for (var i = 0u; i < header.y; i++) {
        let base = vector_band(origin, 1u + i).x;
        let layer = vector_band(origin, base);
        if layer.x == 0xffffffffu {
            colors[depth] = color;
            modes[depth] = layer.y;
            depth++;
            color = vec4<f32>(0.0);
            continue;
        }
        if layer.x == 0xfffffffeu {
            depth--;
            color = vector_composite(color, colors[depth], modes[depth]);
            continue;
        }
        let clip_count = layer.x;
        var coverage = 1.0;
        for (var j = 0u; j < clip_count; j++) {
            let clip = vector_band(origin, base + 1u + j).x;
            coverage *= vector_coverage(origin, coord, width, clip);
        }
        let paint = vector_band(origin, base).y;
        let source = vector_paint(origin, paint, coord, foreground) * coverage;
        color = source + color * (1.0 - source.a);
    }
    return color;
}

fn sample_color(in: VertexOutput) -> vec4<f32> {
    let width = max(fwidth(in.vector_coord), vec2<f32>(1e-8));
    let texture_color = sample_texture(in);
    if in.vector_origin.x >= 0 {
        return vector_color(in.vector_origin, in.vector_coord, width, in.color);
    }
    return in.color * texture_color;
}

@fragment
fn fs_main_linear_framebuffer(in: VertexOutput) -> @location(0) vec4<f32> {
    // We expect "normal" textures that are NOT sRGB-aware.
    var out_color_gamma = sample_color(in);
    // Dither the float color down to eight bits to reduce banding.
    // This step is optional for egui backends.
    // Note that dithering is performed on the gamma encoded values,
    // because this function is used together with a srgb converting target.
    if r_locals.dithering == 1 {
        let out_color_gamma_rgb = dither_interleaved(out_color_gamma.rgb, 256.0, in.position);
        out_color_gamma = vec4<f32>(out_color_gamma_rgb, out_color_gamma.a);
    }
    let out_color_linear = linear_from_gamma_rgb(out_color_gamma.rgb);
    return vec4<f32>(out_color_linear, out_color_gamma.a);
}

@fragment
fn fs_main_gamma_framebuffer(in: VertexOutput) -> @location(0) vec4<f32> {
    // We expect "normal" textures that are NOT sRGB-aware.
    var out_color_gamma = sample_color(in);
    // Dither the float color down to eight bits to reduce banding.
    // This step is optional for egui backends.
    if r_locals.dithering == 1 {
        let out_color_gamma_rgb = dither_interleaved(out_color_gamma.rgb, 256.0, in.position);
        out_color_gamma = vec4<f32>(out_color_gamma_rgb, out_color_gamma.a);
    }
    return out_color_gamma;
}
