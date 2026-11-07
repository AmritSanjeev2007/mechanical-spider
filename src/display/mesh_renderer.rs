//! Draws a physics [`Mesh`] as balls and sticks.
//!
//! Every ball is an instanced sphere and every bone (parent ball -> child ball)
//! an instanced cylinder, so a whole mesh costs two draw calls however many
//! balls it has. Instance data is rebuilt from the mesh each frame, which keeps
//! this module free of any knowledge of how the physics moves the balls.
//!
//! Shaders (loaded from `./shaders/` like the skybox's):
//! * `mesh_ball.vert`  – places and scales the unit sphere per ball.
//! * `mesh_stick.vert` – stretches the unit cylinder between two ball centres.
//! * `mesh_lit.frag`   – shared sky/ground ambient + sun lighting.
//!
//! Colours are chosen per role: the root ball (the body), inner joints, and
//! leaf balls (the feet, which are what touch the ground).

use std::f32::consts::{PI, TAU};

use glam::{Mat4, Vec3};
use glow::HasContext;

use super::gl_util::compile_program_from_files;
use crate::physics::Mesh;

const BALL_VERTEX_SHADER: &str = "./shaders/mesh_ball.vert";
const STICK_VERTEX_SHADER: &str = "./shaders/mesh_stick.vert";
const LIT_FRAGMENT_SHADER: &str = "./shaders/mesh_lit.frag";

/// Sphere tessellation (longitude x latitude segments).
const SPHERE_SEGMENTS: (u32, u32) = (24, 16);
/// Cylinder tessellation around its axis.
const CYLINDER_SEGMENTS: u32 = 16;

/// Floats per instance: centre xyz, radius, colour rgb.
const BALL_INSTANCE_FLOATS: usize = 7;
/// Floats per instance: start xyz, radius, end xyz, colour rgb.
const STICK_INSTANCE_FLOATS: usize = 10;

/// How a mesh is drawn. Sizes are world units, colours are display RGB in 0..1.
#[derive(Clone, Debug)]
pub struct MeshStyle {
    pub root_radius: f32,
    pub ball_radius: f32,
    pub foot_radius: f32,
    pub stick_radius: f32,
    pub root_color: [f32; 3],
    pub ball_color: [f32; 3],
    pub foot_color: [f32; 3],
    pub stick_color: [f32; 3],
}

impl Default for MeshStyle {
    fn default() -> Self {
        Self {
            root_radius: 0.22,
            ball_radius: 0.09,
            foot_radius: 0.07,
            stick_radius: 0.04,
            root_color: [0.95, 0.55, 0.20],
            ball_color: [0.90, 0.90, 0.88],
            foot_color: [0.25, 0.75, 0.80],
            stick_color: [0.45, 0.47, 0.52],
        }
    }
}

/// Camera and light inputs for one draw.
pub struct MeshLighting {
    pub view_proj: Mat4,
    /// Camera position, for the highlight.
    pub eye: Vec3,
    /// Unit vector pointing towards the sun.
    pub light_dir: Vec3,
}

struct InstancedShape {
    vao: glow::VertexArray,
    vertex_buffer: glow::Buffer,
    index_buffer: glow::Buffer,
    instance_buffer: glow::Buffer,
    index_count: i32,
}

pub struct MeshRenderer {
    ball_program: glow::Program,
    stick_program: glow::Program,
    sphere: InstancedShape,
    cylinder: InstancedShape,
    ball_uniforms: Uniforms,
    stick_uniforms: Uniforms,
    // Reused every frame to avoid reallocating.
    ball_instances: Vec<f32>,
    stick_instances: Vec<f32>,
}

struct Uniforms {
    view_proj: Option<glow::UniformLocation>,
    light_dir: Option<glow::UniformLocation>,
    eye: Option<glow::UniformLocation>,
}

impl Uniforms {
    fn new(gl: &glow::Context, program: glow::Program) -> Self {
        unsafe {
            Self {
                view_proj: gl.get_uniform_location(program, "u_view_proj"),
                light_dir: gl.get_uniform_location(program, "u_light_dir"),
                eye: gl.get_uniform_location(program, "u_eye"),
            }
        }
    }
}

impl MeshRenderer {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let ball_program =
            compile_program_from_files(gl, BALL_VERTEX_SHADER, LIT_FRAGMENT_SHADER)?;
        let stick_program =
            compile_program_from_files(gl, STICK_VERTEX_SHADER, LIT_FRAGMENT_SHADER)?;

        let (sphere_vertices, sphere_indices) = unit_sphere(SPHERE_SEGMENTS.0, SPHERE_SEGMENTS.1);
        // Sphere: location 0 = position (also the normal).
        let sphere = InstancedShape::new(gl, &sphere_vertices, &sphere_indices, &[3], 1, &[4, 3])?;

        let (cylinder_vertices, cylinder_indices) = unit_cylinder(CYLINDER_SEGMENTS);
        // Cylinder: locations 0, 1 = position, normal.
        let cylinder =
            InstancedShape::new(gl, &cylinder_vertices, &cylinder_indices, &[3, 3], 2, &[4, 3, 3])?;

        Ok(Self {
            ball_uniforms: Uniforms::new(gl, ball_program),
            stick_uniforms: Uniforms::new(gl, stick_program),
            ball_program,
            stick_program,
            sphere,
            cylinder,
            ball_instances: Vec::new(),
            stick_instances: Vec::new(),
        })
    }

    /// Draws `mesh`. Call after the skybox, with depth testing on.
    pub fn draw(&mut self, gl: &glow::Context, mesh: &Mesh, style: &MeshStyle, lighting: &MeshLighting) {
        self.build_instances(mesh, style);

        let ball_count = (self.ball_instances.len() / BALL_INSTANCE_FLOATS) as i32;
        let stick_count = (self.stick_instances.len() / STICK_INSTANCE_FLOATS) as i32;

        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_mask(true);

            draw_shape(gl, self.ball_program, &self.ball_uniforms, &self.sphere, &self.ball_instances, ball_count, lighting);
            draw_shape(gl, self.stick_program, &self.stick_uniforms, &self.cylinder, &self.stick_instances, stick_count, lighting);

            gl.bind_vertex_array(None);
            gl.use_program(None);
        }
    }

    fn build_instances(&mut self, mesh: &Mesh, style: &MeshStyle) {
        self.ball_instances.clear();
        self.stick_instances.clear();
        let root = mesh.root();

        for id in mesh.ids() {
            let position = mesh.ball(id).position;
            let is_leaf = mesh.children(id).next().is_none();
            let (radius, color) = if id == root {
                (style.root_radius, style.root_color)
            } else if is_leaf {
                (style.foot_radius, style.foot_color)
            } else {
                (style.ball_radius, style.ball_color)
            };
            self.ball_instances.extend_from_slice(&[position.x, position.y, position.z, radius]);
            self.ball_instances.extend_from_slice(&color);

            if let Some(parent) = mesh.parent(id) {
                let start = mesh.ball(parent).position;
                self.stick_instances.extend_from_slice(&[start.x, start.y, start.z, style.stick_radius]);
                self.stick_instances.extend_from_slice(&position.to_array());
                self.stick_instances.extend_from_slice(&style.stick_color);
            }
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.ball_program);
            gl.delete_program(self.stick_program);
        }
        self.sphere.destroy(gl);
        self.cylinder.destroy(gl);
    }
}

unsafe fn draw_shape(
    gl: &glow::Context,
    program: glow::Program,
    uniforms: &Uniforms,
    shape: &InstancedShape,
    instances: &[f32],
    count: i32,
    lighting: &MeshLighting,
) {
    if count == 0 {
        return;
    }
    unsafe {
        gl.use_program(Some(program));
        gl.uniform_matrix_4_f32_slice(uniforms.view_proj.as_ref(), false, &lighting.view_proj.to_cols_array());
        gl.uniform_3_f32_slice(uniforms.light_dir.as_ref(), &lighting.light_dir.to_array());
        gl.uniform_3_f32_slice(uniforms.eye.as_ref(), &lighting.eye.to_array());

        gl.bind_buffer(glow::ARRAY_BUFFER, Some(shape.instance_buffer));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(instances), glow::STREAM_DRAW);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);

        gl.bind_vertex_array(Some(shape.vao));
        gl.draw_elements_instanced(glow::TRIANGLES, shape.index_count, glow::UNSIGNED_INT, 0, count);
    }
}

impl InstancedShape {
    /// Uploads a static shape and sets up its instance buffer.
    ///
    /// `vertex_layout` gives the float count of each per-vertex attribute
    /// (locations 0..), `instance_layout` the float count of each per-instance
    /// attribute (locations `first_instance_location`..).
    fn new(
        gl: &glow::Context,
        vertices: &[f32],
        indices: &[u32],
        vertex_layout: &[i32],
        first_instance_location: u32,
        instance_layout: &[i32],
    ) -> Result<Self, String> {
        unsafe {
            let vao = gl.create_vertex_array()?;
            let vertex_buffer = gl.create_buffer()?;
            let index_buffer = gl.create_buffer()?;
            let instance_buffer = gl.create_buffer()?;

            gl.bind_vertex_array(Some(vao));

            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, as_bytes(vertices), glow::STATIC_DRAW);
            set_attributes(gl, 0, vertex_layout, 0);

            gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_buffer));
            set_attributes(gl, first_instance_location, instance_layout, 1);

            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(index_buffer));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, as_bytes(indices), glow::STATIC_DRAW);

            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);

            Ok(Self { vao, vertex_buffer, index_buffer, instance_buffer, index_count: indices.len() as i32 })
        }
    }

    fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.vertex_buffer);
            gl.delete_buffer(self.index_buffer);
            gl.delete_buffer(self.instance_buffer);
        }
    }
}

/// Interleaved float attributes on the bound ARRAY_BUFFER.
unsafe fn set_attributes(gl: &glow::Context, first_location: u32, layout: &[i32], divisor: u32) {
    let stride = layout.iter().sum::<i32>() * 4;
    let mut offset = 0;
    for (i, &size) in layout.iter().enumerate() {
        let location = first_location + i as u32;
        unsafe {
            gl.enable_vertex_attrib_array(location);
            gl.vertex_attrib_pointer_f32(location, size, glow::FLOAT, false, stride, offset);
            gl.vertex_attrib_divisor(location, divisor);
        }
        offset += size * 4;
    }
}

fn as_bytes<T: Copy>(data: &[T]) -> &[u8] {
    // f32 and u32 have no padding or invalid bit patterns.
    unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), std::mem::size_of_val(data)) }
}

/// Unit sphere: positions only (each is also its own normal).
fn unit_sphere(longitude: u32, latitude: u32) -> (Vec<f32>, Vec<u32>) {
    let mut vertices = Vec::new();
    for j in 0..=latitude {
        let theta = PI * j as f32 / latitude as f32; // 0 at +Z
        for i in 0..=longitude {
            let phi = TAU * i as f32 / longitude as f32;
            vertices.extend_from_slice(&[theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos()]);
        }
    }
    let row = longitude + 1;
    let mut indices = Vec::new();
    for j in 0..latitude {
        for i in 0..longitude {
            let a = j * row + i;
            let b = a + row;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (vertices, indices)
}

/// Open unit cylinder from y = 0 to y = 1, radius 1: position + normal per vertex.
/// The ends need no caps because they sit inside the balls.
fn unit_cylinder(segments: u32) -> (Vec<f32>, Vec<u32>) {
    let mut vertices = Vec::new();
    for i in 0..=segments {
        let phi = TAU * i as f32 / segments as f32;
        let (x, z) = (phi.cos(), phi.sin());
        for y in [0.0, 1.0] {
            vertices.extend_from_slice(&[x, y, z, x, 0.0, z]);
        }
    }
    let mut indices = Vec::new();
    for i in 0..segments {
        let a = i * 2;
        indices.extend_from_slice(&[a, a + 1, a + 2, a + 2, a + 1, a + 3]);
    }
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_indices_stay_in_range() {
        let (v, i) = unit_sphere(8, 6);
        let n = (v.len() / 3) as u32;
        assert!(i.iter().all(|&k| k < n));
        assert!(v.chunks(3).all(|p| (Vec3::from_slice(p).length() - 1.0).abs() < 1e-5));
    }

    #[test]
    fn cylinder_indices_stay_in_range() {
        let (v, i) = unit_cylinder(8);
        let n = (v.len() / 6) as u32;
        assert!(i.iter().all(|&k| k < n));
    }
}
