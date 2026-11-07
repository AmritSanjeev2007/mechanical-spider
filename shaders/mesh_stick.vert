#version 330 core
// One instanced unit cylinder per bone (parent ball -> child ball) of a physics Mesh.
// The cylinder runs from y = 0 to y = 1 with radius 1; this shader stretches it
// between the two ball centres.

layout(location = 0) in vec3 a_position;     // unit-cylinder vertex
layout(location = 1) in vec3 a_normal;       // unit-cylinder normal (no y component)
layout(location = 2) in vec4 i_start_radius; // per bone: parent centre (xyz), radius (w)
layout(location = 3) in vec3 i_end;          // per bone: child centre
layout(location = 4) in vec3 i_color;        // per bone

uniform mat4 u_view_proj;

out vec3 v_world_pos;
out vec3 v_normal;
out vec3 v_color;

void main() {
    vec3 axis = i_end - i_start_radius.xyz;
    float len = length(axis);
    vec3 y = len > 1e-6 ? axis / len : vec3(0.0, 0.0, 1.0);
    // Any two unit vectors perpendicular to the bone; the cylinder is round,
    // so which pair does not matter.
    vec3 helper = abs(y.z) < 0.9 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
    vec3 x = normalize(cross(helper, y));
    vec3 z = cross(x, y);

    float r = i_start_radius.w;
    vec3 world = i_start_radius.xyz
               + x * (a_position.x * r)
               + y * (a_position.y * len)
               + z * (a_position.z * r);

    v_world_pos = world;
    v_normal = x * a_normal.x + z * a_normal.z;
    v_color = i_color;
    gl_Position = u_view_proj * vec4(world, 1.0);
}
