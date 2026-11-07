//! Equirectangular skybox.
//!
//! `assets/sky.png` is a 2:1 latitude/longitude panorama: the horizon runs
//! along the middle row, the top row is the zenith and the bottom row the
//! nadir, and the image wraps horizontally through 360 degrees. The sun sits at
//! the centre of the image, i.e. on the horizon at the middle column.
//!
//! Mapping chosen (world is Z-up, see [`super`]):
//!
//! ```text
//! azimuth   = atan2(d.x, d.y)        0 along +Y, +90 deg along +X
//! elevation = asin(d.z)
//! u = 0.5 + azimuth / (2 pi)          texture centre (u = 0.5) -> +Y
//! v = 0.5 - elevation / pi            texture centre (v = 0.5) -> horizon
//! ```
//!
//! So the texture centre, and with it the sun, lands on world **+Y**, which is
//! exactly what the Blender front view (numpad 1) looks at. Moving right in the
//! image turns towards +X, which is also screen-right in the front view, so the
//! panorama is not mirrored. `sun_azimuth` spins the whole sky around Z if the
//! sun needs to come from somewhere else.
//!
//! The sky is drawn first, as a single full-screen triangle at the far plane
//! with depth writes off, so anything drawn afterwards always covers it.

use std::path::Path;

use glam::Mat3;
use glow::HasContext;

use super::camera::OrbitCamera;
use super::gl_util::compile_program;

const VERTEX_SHADER: &str = "./shaders/skybox.vert";
const FRAGMENT_SHADER: &str = "./shaders/skybox.frag";

pub struct Skybox {
    program: glow::Program,
    vao: glow::VertexArray,
    texture: glow::Texture,
    loc_right: Option<glow::UniformLocation>,
    loc_up: Option<glow::UniformLocation>,
    loc_forward: Option<glow::UniformLocation>,
    loc_rotation: Option<glow::UniformLocation>,
    loc_sampler: Option<glow::UniformLocation>,
    /// Extra rotation of the sky around world Z, in radians. 0 puts the sun at +Y.
    pub sun_azimuth: f32,
}

impl Skybox {
    pub fn new(gl: &glow::Context, texture_path: &Path) -> Result<Self, String> {
        let image = image::open(texture_path)
            .map_err(|e| format!("failed to load {}: {e}", texture_path.display()))?
            .to_rgb8();
        let (width, height) = image.dimensions();

        let vert_src = std::fs::read_to_string(VERTEX_SHADER)
            .map_err(|e| format!("Failed to load skybox vertex shader due to {e:?}"))?;
        let frag_src = std::fs::read_to_string(FRAGMENT_SHADER)
            .map_err(|e| format!("Failed to load skybox fragment shader due to {e:?}"))?;

        unsafe {
            let program = compile_program(gl, &vert_src, &frag_src)?;
            // Core profile needs a bound VAO even though the triangle has no attributes.
            let vao = gl.create_vertex_array()?;

            let texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            // Rows go up top-first, so texture v = 0 is the top (zenith) of the image.
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGB8 as i32,
                width as i32,
                height as i32,
                0,
                glow::RGB,
                glow::UNSIGNED_BYTE,
                Some(image.as_raw()),
            );
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.generate_mipmap(glow::TEXTURE_2D);
            // Wraps around horizontally, clamps at the poles.
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR_MIPMAP_LINEAR as i32,
            );
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.bind_texture(glow::TEXTURE_2D, None);

            Ok(Self {
                loc_right: gl.get_uniform_location(program, "u_right"),
                loc_up: gl.get_uniform_location(program, "u_up"),
                loc_forward: gl.get_uniform_location(program, "u_forward"),
                loc_rotation: gl.get_uniform_location(program, "u_sky_rotation"),
                loc_sampler: gl.get_uniform_location(program, "u_sky"),
                program,
                vao,
                texture,
                sun_azimuth: 0.0,
            })
        }
    }

    /// Draws the sky. Call this first each frame, before any scene geometry.
    pub fn draw(&self, gl: &glow::Context, camera: &OrbitCamera, aspect: f32) {
        // The sky always uses a perspective ray fan, even when the camera is
        // orthographic, otherwise every pixel would see the same direction.
        let tan_half = (camera.fov_y * 0.5).tan();
        let right = camera.right() * tan_half * aspect;
        let up = camera.up() * tan_half;
        let forward = camera.forward();
        // World -> sky: undo the sun's spin.
        let sky_rotation = Mat3::from_rotation_z(-self.sun_azimuth);

        unsafe {
            gl.depth_mask(false);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);

            gl.use_program(Some(self.program));
            gl.uniform_3_f32_slice(self.loc_right.as_ref(), &right.to_array());
            gl.uniform_3_f32_slice(self.loc_up.as_ref(), &up.to_array());
            gl.uniform_3_f32_slice(self.loc_forward.as_ref(), &forward.to_array());
            gl.uniform_matrix_3_f32_slice(
                self.loc_rotation.as_ref(),
                false,
                &sky_rotation.to_cols_array(),
            );
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.uniform_1_i32(self.loc_sampler.as_ref(), 0);

            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);

            // Leave depth state ready for the scene drawn on top.
            gl.depth_mask(true);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vao);
            gl.delete_texture(self.texture);
        }
    }
}
