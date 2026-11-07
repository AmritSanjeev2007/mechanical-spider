#version 330 core
in vec3 v_dir;
out vec4 frag_color;

uniform sampler2D u_sky;
uniform mat3 u_sky_rotation;   // world -> sky frame (spins the sun around Z)

const float PI = 3.14159265358979;

void main() {
    vec3 d = normalize(u_sky_rotation * v_dir);

    float azimuth   = atan(d.x, d.y);               // 0 at +Y (sun), +pi/2 at +X
    float elevation = asin(clamp(d.z, -1.0, 1.0));  // +pi/2 at zenith

    float u = 0.5 + azimuth / (2.0 * PI);
    float v = 0.5 - elevation / PI;                 // image row 0 is the zenith

    // atan() jumps from +pi to -pi along -Y (straight away from the sun). Left
    // alone, that jump makes the mip level explode along a one-pixel seam. Pick whichever of two
    // parameterisations is continuous across this pixel quad (Tarini 2012).
    float u_a = fract(u);
    float u_b = fract(u + 0.5) - 0.5;
    u = fwidth(u_a) <= fwidth(u_b) + 1e-5 ? u_a : u_b;

    // The texture is stored as plain RGB8 (not sRGB8) and the framebuffer does
    // no conversion, so the panorama's colours reach the screen unchanged.
    frag_color = vec4(texture(u_sky, vec2(u, v)).rgb, 1.0);
}
