#version 330 core
uniform vec3 u_right;     // camera basis, pre-scaled by tan(fov/2) * aspect
uniform vec3 u_up;        // camera basis, pre-scaled by tan(fov/2)
uniform vec3 u_forward;

out vec3 v_dir;

void main() {
    // Full-screen triangle from the vertex id: (-1,-1), (3,-1), (-1,3).
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2)) * 2.0 - 1.0;
    // View ray through this pixel. Linear in screen space, so interpolation is exact.
    v_dir = u_forward + p.x * u_right + p.y * u_up;
    // z == w puts the sky exactly on the far plane (depth 1.0).
    gl_Position = vec4(p, 1.0, 1.0);
}
