#ifdef GL_ES
    #if NEW_SHADER_INTERFACE
        precision highp int;
    #endif
    // To avoid weird distortion issues when rendering text etc, we want highp if possible.
    // But apparently some devices don't support it, so we have to check first.
    #if defined(GL_FRAGMENT_PRECISION_HIGH) && GL_FRAGMENT_PRECISION_HIGH == 1
        precision highp float;
    #else
        precision mediump float;
    #endif
#endif

uniform sampler2D u_sampler;
#if NEW_SHADER_INTERFACE
    uniform highp sampler2D u_curves;
    uniform highp usampler2D u_bands;
#endif

#if NEW_SHADER_INTERFACE
    in vec4 v_rgba_in_gamma;
    in vec2 v_tc;
    in vec2 v_vector_origin;
    in vec2 v_vector_coord;
    out vec4 f_color;
    // a dirty hack applied to support webGL2
    #define gl_FragColor f_color
    #define texture2D texture
#else
    varying vec4 v_rgba_in_gamma;
    varying vec2 v_tc;
    varying vec2 v_vector_origin;
    varying vec2 v_vector_coord;
#endif

// -----------------------------------------------
// Adapted from
// https://www.shadertoy.com/view/llVGzG
// Originally presented in:
// Jimenez 2014, "Next Generation Post-Processing in Call of Duty"
//
// A good overview can be found in
// https://blog.demofox.org/2022/01/01/interleaved-gradient-noise-a-different-kind-of-low-discrepancy-sequence/
// via https://github.com/rerun-io/rerun/
float interleaved_gradient_noise(vec2 n) {
    float f = 0.06711056 * n.x + 0.00583715 * n.y;
    return fract(52.9829189 * fract(f));
}

vec3 dither_interleaved(vec3 rgb, float levels) {
    float noise = interleaved_gradient_noise(gl_FragCoord.xy);
    // scale down the noise slightly to ensure flat colors aren't getting dithered
    noise = (noise - 0.5) * 0.95;
    return rgb + noise / (levels - 1.0);
}

#if NEW_SHADER_INTERFACE
uvec4 vector_band(ivec2 origin, uint address) {
    uint width = uint(textureSize(u_bands, 0).x);
    uint index = uint(origin.y) * width + uint(origin.x) + address;
    return texelFetch(u_bands, ivec2(index % width, index / width), 0);
}

vec4 vector_curve(uint address) {
    uint width = uint(textureSize(u_curves, 0).x);
    return texelFetch(u_curves, ivec2(address % width, address / width), 0);
}

vec2 vector_roots(vec2 p0, vec2 p1, vec2 p2) {
    vec2 a = p0 - 2.0 * p1 + p2;
    vec2 b = p0 - p1;
    if (abs(a.y) < 1.0 / 65536.0) {
        float t = p0.y / ((abs(b.y) < 1e-12) ? 1.0 : 2.0 * b.y);
        return vec2((a.x * t - 2.0 * b.x) * t + p0.x);
    }
    float d = sqrt(max(b.y * b.y - a.y * p0.y, 0.0));
    vec2 t = (vec2(b.y) + vec2(-d, d)) / a.y;
    return (a.x * t - vec2(2.0 * b.x)) * t + vec2(p0.x);
}

vec2 vector_ray(ivec2 origin, vec2 p, vec2 width, uint base, bool vertical) {
    uint band = min(uint(clamp(vertical ? p.x : p.y, 0.0, 1.0) * 16.0), 15u);
    uvec4 list = vector_band(origin, base + 1u + (vertical ? 16u : 0u) + band);
    float coverage = 0.0;
    float weight = 0.0;
    for (uint i = 0u; i < list.y; i++) {
        uint address = vector_band(origin, list.x + i / 4u)[i % 4u];
        vec4 points = vector_curve(address);
        vec2 p0 = points.xy - p;
        vec2 p1 = points.zw - p;
        vec2 p2 = vector_curve(address + 1u).xy - p;
        if (vertical) {
            p0 = p0.yx;
            p1 = p1.yx;
            p2 = p2.yx;
        }
        if (max(max(p0.x, p1.x), p2.x) < -0.5 * (vertical ? width.y : width.x)) {
            break;
        }
        // Sign classes select eligible roots without integer bit operations.
        float signs =
            (p0.y < 0.0 ? 1.0 : 0.0) + (p1.y < 0.0 ? 2.0 : 0.0) + (p2.y < 0.0 ? 4.0 : 0.0);
        bool root1 = signs == 2.0 || signs == 4.0 || signs == 5.0 || signs == 6.0;
        bool root2 = signs == 1.0 || signs == 2.0 || signs == 3.0 || signs == 5.0;
        if (root1 || root2) {
            vec2 roots = vector_roots(p0, p1, p2) / (vertical ? width.y : width.x);
            if (root1) {
                coverage += clamp(roots.x + 0.5, 0.0, 1.0);
                weight = max(weight, clamp(1.0 - 2.0 * abs(roots.x), 0.0, 1.0));
            }
            if (root2) {
                coverage -= clamp(roots.y + 0.5, 0.0, 1.0);
                weight = max(weight, clamp(1.0 - 2.0 * abs(roots.y), 0.0, 1.0));
            }
        }
    }
    return vec2(vertical ? -coverage : coverage, weight);
}

float vector_coverage(ivec2 origin, vec2 p, vec2 width, uint base) {
    vec2 samples = clamp(floor(width * 16.0 + 1.0), vec2(1.0), vec2(4.0));
    vec2 x = vec2(0.0);
    vec2 y = vec2(0.0);
    for (int i = 0; i < 4; i++) {
        if (float(i) >= samples.y) {
            break;
        }
        float offset = ((float(i) + 0.5) / samples.y - 0.5) * width.y;
        vec2 ray = vector_ray(origin, p + vec2(0.0, offset), width, base, false);
        x.x += ray.x / samples.y;
        x.y = max(x.y, ray.y);
    }
    for (int i = 0; i < 4; i++) {
        if (float(i) >= samples.x) {
            break;
        }
        float offset = ((float(i) + 0.5) / samples.x - 0.5) * width.x;
        vec2 ray = vector_ray(origin, p + vec2(offset, 0.0), width, base, true);
        y.x += ray.x / samples.x;
        y.y = max(y.y, ray.y);
    }
    float weighted = abs(x.x * x.y + y.x * y.y) / max(x.y + y.y, 1.0 / 65536.0);
    float coverage = clamp(max(weighted, min(abs(x.x), abs(y.x))), 0.0, 1.0);
    uint mode = vector_band(origin, base).x;
    if (mode == 1u) {
        float gamma = vector_curve(vector_band(origin, base).y).x;
        coverage = pow(coverage, gamma);
    } else if (mode == 2u) {
        coverage = 2.0 * coverage - coverage * coverage;
    }
    return coverage;
}

vec4 vector_stop(uint address, vec4 foreground) {
    vec4 stop = vector_curve(address);
    vec4 color = vector_curve(address + 1u);
    if (stop.y == 1.0) {
        return foreground * color.a;
    }
    return color * foreground.a;
}

vec2 vector_radial_parameter(vec2 p, vec2 c0, vec2 c1, float r0, float r1) {
    float t = 0.0;
    float dr = r1 - r0;
    vec2 d = c1 - c0;
    vec2 q = p - c0;
    float a = dot(d, d) - dr * dr;
    float b = -2.0 * (dot(q, d) + r0 * dr);
    float c = dot(q, q) - r0 * r0;
    if (abs(a) < 1e-12) {
        if (abs(b) < 1e-12) {
            return vec2(0.0);
        }
        t = -c / b;
        if (r0 + t * dr < 0.0) {
            return vec2(0.0);
        }
    } else {
        float disc = b * b - 4.0 * a * c;
        if (disc < 0.0) {
            return vec2(0.0);
        }
        float root = sqrt(disc);
        float t0 = (-b - root) / (2.0 * a);
        float t1 = (-b + root) / (2.0 * a);
        bool valid0 = r0 + t0 * dr >= 0.0;
        bool valid1 = r0 + t1 * dr >= 0.0;
        if (!valid0 && !valid1) {
            return vec2(0.0);
        }
        t = (valid0 && (!valid1 || t0 > t1)) ? t0 : t1;
    }
    return vec2(t, 1.0);
}

vec4 vector_paint(ivec2 origin, uint base, vec2 coord, vec4 foreground) {
    uvec4 h = vector_band(origin, base);
    if (h.x == 0u) {
        vec4 c = vector_curve(h.w);
        if (h.y == 1u) {
            return foreground * c.a;
        }
        return c * foreground.a;
    }
    vec4 row0 = vector_curve(h.w);
    vec4 row1 = vector_curve(h.w + 1u);
    vec3 point = vec3(coord, 1.0);
    vec2 p = vec2(dot(row0.xyz, point), dot(row1.xyz, point));
    vec4 geometry0 = vector_curve(h.w + 2u);
    vec4 geometry1 = vector_curve(h.w + 3u);
    vec2 c0 = geometry0.xy;
    vec2 c1 = geometry1.xy;
    float t = 0.0;
    if (h.x == 1u) {
        vec2 d = c1 - c0;
        t = dot(p - c0, d) / max(dot(d, d), 1e-20);
    } else if (h.x == 2u) {
        float r0 = geometry0.z;
        float r1 = geometry1.z;
        vec2 radial = vector_radial_parameter(p, c0, c1, r0, r1);
        if (radial.y == 0.0) {
            return vec4(0.0);
        }
        t = radial.x;
    } else {
        vec2 q = p - c0;
        float angle = atan(-q.y, q.x) * 57.295779513;
        if (angle < 0.0) {
            angle += 360.0;
        }
        t = (angle - c1.x) / (c1.y - c1.x);
    }
    if (h.y == 1u) {
        t = fract(t);
    }
    if (h.y == 2u) {
        t = 1.0 - abs(1.0 - (t - 2.0 * floor(t / 2.0)));
    }
    uint count = h.z;
    vec4 previous = vector_stop(h.w + 4u, foreground);
    float offset = vector_curve(h.w + 4u).x;
    if (t < offset) {
        return previous;
    }
    for (uint i = 1u; i < count; i++) {
        uint address = h.w + 4u + i * 2u;
        vec4 next = vector_stop(address, foreground);
        float end = vector_curve(address).x;
        if (t < end) {
            return mix(previous, next, clamp((t - offset) / max(end - offset, 1e-20), 0.0, 1.0));
        }
        previous = next;
        offset = end;
    }
    return previous;
}

// W3C Compositing and Blending formulas for the COLR blend modes, on
// unpremultiplied colors. `b` is the backdrop, `s` the source.
float vector_blend_channel(float b, float s, uint mode) {
    switch (mode) {
        case 13u:
            return b + s - b * s;
        case 14u:
            if (b <= 0.5) {
                return s * 2.0 * b;
            }
            return s + (2.0 * b - 1.0) - s * (2.0 * b - 1.0);
        case 15u:
            return min(b, s);
        case 16u:
            return max(b, s);
        case 17u:
            if (b == 0.0) {
                return 0.0;
            }
            if (s >= 1.0) {
                return 1.0;
            }
            return min(1.0, b / (1.0 - s));
        case 18u:
            if (b >= 1.0) {
                return 1.0;
            }
            if (s == 0.0) {
                return 0.0;
            }
            return 1.0 - min(1.0, (1.0 - b) / s);
        case 19u:
            if (s <= 0.5) {
                return b * 2.0 * s;
            }
            return b + (2.0 * s - 1.0) - b * (2.0 * s - 1.0);
        case 20u: {
            if (s <= 0.5) {
                return b - (1.0 - 2.0 * s) * b * (1.0 - b);
            }
            float d = b <= 0.25 ? ((16.0 * b - 12.0) * b + 4.0) * b : sqrt(b);
            return b + (2.0 * s - 1.0) * (d - b);
        }
        case 21u:
            return abs(b - s);
        case 22u:
            return b + s - 2.0 * b * s;
        case 23u:
            return b * s;
        default:
            return s;
    }
}

float vector_lum(vec3 c) {
    return dot(c, vec3(0.3, 0.59, 0.11));
}

vec3 vector_set_lum(vec3 c, float l) {
    vec3 color = c + (l - vector_lum(c));
    float lum = vector_lum(color);
    float low = min(color.r, min(color.g, color.b));
    float high = max(color.r, max(color.g, color.b));
    if (low < 0.0) {
        color = lum + (color - lum) * lum / max(lum - low, 1e-20);
    }
    if (high > 1.0) {
        color = lum + (color - lum) * (1.0 - lum) / max(high - lum, 1e-20);
    }
    return color;
}

float vector_sat(vec3 c) {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

vec3 vector_set_sat(vec3 c, float s) {
    float low = min(c.r, min(c.g, c.b));
    float range = vector_sat(c);
    if (range <= 0.0) {
        return vec3(0.0);
    }
    return (c - low) * s / range;
}

vec4 vector_blend(vec4 source, vec4 destination, uint mode) {
    vec3 s = clamp(source.rgb / max(source.a, 1e-20), 0.0, 1.0);
    vec3 b = clamp(destination.rgb / max(destination.a, 1e-20), 0.0, 1.0);
    vec3 blended;
    if (mode == 24u) {
        blended = vector_set_lum(vector_set_sat(s, vector_sat(b)), vector_lum(b));
    } else if (mode == 25u) {
        blended = vector_set_lum(vector_set_sat(b, vector_sat(s)), vector_lum(b));
    } else if (mode == 26u) {
        blended = vector_set_lum(s, vector_lum(b));
    } else if (mode == 27u) {
        blended = vector_set_lum(b, vector_lum(s));
    } else {
        blended = vec3(
            vector_blend_channel(b.r, s.r, mode),
            vector_blend_channel(b.g, s.g, mode),
            vector_blend_channel(b.b, s.b, mode)
        );
    }
    float both = source.a * destination.a;
    return vec4(
        source.rgb * (1.0 - destination.a) + destination.rgb * (1.0 - source.a) + both * blended,
        source.a + destination.a - both
    );
}

vec4 vector_composite(vec4 source, vec4 destination, uint mode) {
    switch (mode) {
        case 0u:
            return vec4(0.0);
        case 1u:
            return source;
        case 2u:
            return destination;
        case 3u:
            return source + destination * (1.0 - source.a);
        case 4u:
            return destination + source * (1.0 - destination.a);
        case 5u:
            return source * destination.a;
        case 6u:
            return destination * source.a;
        case 7u:
            return source * (1.0 - destination.a);
        case 8u:
            return destination * (1.0 - source.a);
        case 9u:
            return source * destination.a + destination * (1.0 - source.a);
        case 10u:
            return destination * source.a + source * (1.0 - destination.a);
        case 11u:
            return source * (1.0 - destination.a) + destination * (1.0 - source.a);
        case 12u:
            return min(source + destination, vec4(1.0));
        default:
            return vector_blend(source, destination, mode);
    }
}

vec4 vector_color(ivec2 origin, vec2 coord, vec2 width, vec4 foreground) {
    uvec4 header = vector_band(origin, 0u);
    if (header.x != 3u) {
        return foreground * vector_coverage(origin, coord, width, 0u);
    }
    vec4 color = vec4(0.0);
    vec4 colors[16];
    uint modes[16];
    uint depth = 0u;
    for (uint i = 0u; i < header.y; i++) {
        uint base = vector_band(origin, 1u + i).x;
        uvec4 layer = vector_band(origin, base);
        if (layer.x == 0xffffffffu) {
            colors[depth] = color;
            modes[depth] = layer.y;
            depth++;
            color = vec4(0.0);
            continue;
        }
        if (layer.x == 0xfffffffeu) {
            depth--;
            color = vector_composite(color, colors[depth], modes[depth]);
            continue;
        }
        uint clip_count = layer.x;
        float coverage = 1.0;
        for (uint j = 0u; j < clip_count; j++) {
            uint clip = vector_band(origin, base + 1u + j).x;
            coverage *= vector_coverage(origin, coord, width, clip);
        }
        uint paint = vector_band(origin, base).y;
        vec4 source = vector_paint(origin, paint, coord, foreground) * coverage;
        color = source + color * (1.0 - source.a);
    }
    return color;
}

#endif

void main() {
#if NEW_SHADER_INTERFACE
    vec2 width = max(fwidth(v_vector_coord), vec2(1e-8));
#endif
    vec4 texture_in_gamma = texture2D(u_sampler, v_tc);
    vec4 frag_color_gamma = v_rgba_in_gamma * texture_in_gamma;
#if NEW_SHADER_INTERFACE
    if (v_vector_origin.x >= 0.0) {
        frag_color_gamma = vector_color(ivec2(v_vector_origin), v_vector_coord, width, v_rgba_in_gamma);
    }
#endif

    // Dither the float color down to eight bits to reduce banding.
    // This step is optional for egui backends.
#if DITHERING
    frag_color_gamma.rgb = dither_interleaved(frag_color_gamma.rgb, 256.);
#endif
    gl_FragColor = frag_color_gamma;
}
