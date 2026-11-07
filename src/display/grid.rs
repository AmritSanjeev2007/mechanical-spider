//! Blender-style floor grid on the XY plane with red X and green Y axes.
//! Gives the camera something to orbit, pan and zoom around until the spider exists.

use glam::Mat4;
use glow::HasContext;

use super::gl_util::compile_program;

const HALF_EXTENT: i32 = 10;

const VERTEX_SHADER: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_color;
uniform mat4 u_view_proj;
out vec3 v_color;
void main() {
    v_color = a_color;
    gl_Position = u_view_proj * vec4(a_pos, 1.0);
}
"#;

const FRAGMENT_SHADER: &str = r#"#version 330 core
in vec3 v_color;
out vec4 frag_color;
void main() { frag_color = vec4(v_color, 1.0); }
"#;

pub struct Grid {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    vertex_count: i32,
    loc_view_proj: Option<glow::UniformLocation>,
    pub visible: bool,
}

impl Grid {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let line = [0.35f32, 0.35, 0.35];
        let x_axis = [0.85f32, 0.25, 0.3];
        let y_axis = [0.45f32, 0.75, 0.2];
        let e = HALF_EXTENT as f32;

        let mut verts: Vec<f32> = Vec::new();
        let mut push = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| {
            verts.extend_from_slice(&a);
            verts.extend_from_slice(&c);
            verts.extend_from_slice(&b);
            verts.extend_from_slice(&c);
        };
        for i in -HALF_EXTENT..=HALF_EXTENT {
            let f = i as f32;
            // Line parallel to X (at y = f); the one at y = 0 is the X axis.
            push([-e, f, 0.0], [e, f, 0.0], if i == 0 { x_axis } else { line });
            // Line parallel to Y (at x = f); the one at x = 0 is the Y axis.
            push([f, -e, 0.0], [f, e, 0.0], if i == 0 { y_axis } else { line });
        }

        unsafe {
            let program = compile_program(gl, VERTEX_SHADER, FRAGMENT_SHADER)?;
            let vao = gl.create_vertex_array()?;
            let vbo = gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let bytes: Vec<u8> = verts.iter().flat_map(|v| v.to_ne_bytes()).collect();
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STATIC_DRAW);
            let stride = 6 * std::mem::size_of::<f32>() as i32;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 3 * 4);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);

            Ok(Self {
                loc_view_proj: gl.get_uniform_location(program, "u_view_proj"),
                program,
                vao,
                vbo,
                vertex_count: (verts.len() / 6) as i32,
                visible: true,
            })
        }
    }

    pub fn draw(&self, gl: &glow::Context, view_proj: Mat4) {
        if !self.visible {
            return;
        }
        unsafe {
            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(self.loc_view_proj.as_ref(), false, &view_proj.to_cols_array());
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::LINES, 0, self.vertex_count);
            gl.bind_vertex_array(None);
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.vbo);
        }
    }
}
