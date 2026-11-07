#version 330 core
// Shared lighting for the mesh's balls and sticks: a sky/ground hemisphere
// ambient, a diffuse sun and a soft highlight.

in vec3 v_world_pos;
in vec3 v_normal;
in vec3 v_color;

out vec4 frag_color;

uniform vec3 u_light_dir;   // unit vector towards the sun
uniform vec3 u_eye;         // camera position (for the highlight)

const vec3 SKY_AMBIENT    = vec3(0.42, 0.48, 0.62);
const vec3 GROUND_AMBIENT = vec3(0.20, 0.18, 0.17);
const vec3 SUN_COLOR      = vec3(1.00, 0.88, 0.72);

void main() {
    vec3 n = normalize(v_normal);
    vec3 l = normalize(u_light_dir);
    vec3 v = normalize(u_eye - v_world_pos);
    vec3 h = normalize(l + v);

    // World is Z up: blend ambient from the ground below to the sky above.
    vec3 ambient = mix(GROUND_AMBIENT, SKY_AMBIENT, n.z * 0.5 + 0.5);
    float diffuse = max(dot(n, l), 0.0);
    float highlight = pow(max(dot(n, h), 0.0), 48.0) * 0.35;

    vec3 color = v_color * (ambient + SUN_COLOR * diffuse) + SUN_COLOR * highlight;
    frag_color = vec4(color, 1.0);
}
