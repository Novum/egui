#if NEW_SHADER_INTERFACE
    #define I in
    #define O out
    #define V(x) x
#else
    #define I attribute
    #define O varying
    #define V(x) vec3(x)
#endif

#ifdef GL_ES
    // To avoid weird distortion issues when rendering text etc, we want highp if possible.
    // But apparently some devices don't support it, so we have to check first.
    #if defined(GL_FRAGMENT_PRECISION_HIGH) && GL_FRAGMENT_PRECISION_HIGH == 1
        precision highp float;
    #else
        precision mediump float;
    #endif
#endif

uniform vec2 u_screen_size;
uniform float u_pixel_size;
I vec2 a_pos;
I vec4 a_srgba; // 0-255 sRGB
I vec2 a_tc;
I vec2 a_glyph_basis_y;
I vec2 a_glyph_origin;
O vec4 v_rgba_in_gamma;
O vec2 v_tc;
O vec2 v_vector_origin;
O vec2 v_vector_coord;

void main() {
    vec2 position = a_pos;
    v_tc = a_tc;
    v_rgba_in_gamma = a_srgba / 255.0;
    v_vector_origin = vec2(-1.0, 0.0);
    v_vector_coord = vec2(0.0);
#if NEW_SHADER_INTERFACE
    uint glyph_y = uint(a_glyph_origin.y);
    v_vector_origin = vec2(a_glyph_origin.x - 1.0, float(glyph_y & 16383u));
    v_vector_coord = vec2(float((glyph_y >> 14u) & 1u), float(glyph_y >> 15u));
    vec2 u = a_tc;
    vec2 v = a_glyph_basis_y;
    float det = u.x * v.y - u.y * v.x;
    if (a_glyph_origin.x != 0.0 && abs(det) > 1e-20) {
        vec2 margin = vec2(abs(v.x) + abs(v.y), abs(u.x) + abs(u.y)) * (u_pixel_size / abs(det));
        vec2 dilation = (2.0 * v_vector_coord - vec2(1.0)) * margin;
        position += u * dilation.x + v * dilation.y;
        v_vector_coord += dilation;
    }
#endif
    gl_Position = vec4(
                      2.0 * position.x / u_screen_size.x - 1.0,
                      1.0 - 2.0 * position.y / u_screen_size.y,
                      0.0,
                      1.0);
}
