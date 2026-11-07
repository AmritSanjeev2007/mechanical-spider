#version 330 core
// One instanced unit sphere per ball of a physics Mesh.

layout(location = 0) in vec3 a_position;      // unit-sphere vertex (doubles as its normal)
layout(location = 1) in vec4 i_center_radius; // per ball: world centre (xyz), radius (w)
layout(location = 2) in vec3 i_color;         // per ball

uniform mat4 u_view_proj;

out vec3 v_world_pos;
out vec3 v_normal;
out vec3 v_color;

void main() {
    vec3 world = i_center_radius.xyz + a_position * i_center_radius.w;
    v_world_pos = world;
    v_normal = a_position;
    v_color = i_color;
    gl_Position = u_view_proj * vec4(world, 1.0);
}
