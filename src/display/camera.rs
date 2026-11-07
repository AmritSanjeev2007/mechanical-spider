//! Blender-style orbit camera.
//!
//! The camera always looks at a pivot point (`target`) from `distance` away.
//! Its orientation is a quaternion, so it can rotate freely (including over the
//! poles) in both of Blender's orbit styles:
//!
//! * **Turntable** (Blender default): horizontal drag spins around world Z,
//!   vertical drag tilts around the camera's own X axis.
//! * **Trackball**: both drag axes rotate around the camera's own axes, so the
//!   view can roll freely.
//!
//! This module only knows *what* the camera can do. Reading the mouse and
//! keyboard and deciding *when* to do it lives in [`super::input`].

use glam::{Mat3, Mat4, Quat, Vec3};

/// Camera-space axes follow the OpenGL convention: looking down `-Z`, `+Y` up, `+X` right.
const ZOOM_STEP: f32 = 1.2;
const MIN_DISTANCE: f32 = 0.05;
const MAX_DISTANCE: f32 = 5_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection {
    Perspective,
    Orthographic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrbitStyle {
    Turntable,
    Trackball,
}

/// The fixed viewpoints Blender puts on the numpad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewPreset {
    /// Numpad 1: from -Y looking along +Y.
    Front,
    /// Ctrl + Numpad 1.
    Back,
    /// Numpad 3: from +X looking along -X.
    Right,
    /// Ctrl + Numpad 3.
    Left,
    /// Numpad 7: from +Z looking down.
    Top,
    /// Ctrl + Numpad 7.
    Bottom,
}

#[derive(Clone, Debug)]
pub struct OrbitCamera {
    /// Point the camera orbits around and looks at.
    pub target: Vec3,
    /// Distance from `target` to the eye.
    pub distance: f32,
    /// Camera-to-world rotation.
    pub orientation: Quat,
    /// Vertical field of view in radians (also sets the orthographic scale).
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    pub projection: Projection,
    pub orbit_style: OrbitStyle,
    /// Radians of rotation per pixel of mouse movement.
    pub orbit_sensitivity: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        let mut camera = Self {
            target: Vec3::ZERO,
            distance: 8.0,
            orientation: Quat::IDENTITY,
            fov_y: 50f32.to_radians(),
            near: 0.01,
            far: 1_000.0,
            projection: Projection::Perspective,
            orbit_style: OrbitStyle::Turntable,
            orbit_sensitivity: 0.005,
        };
        camera.reset();
        camera
    }
}

impl OrbitCamera {
    /// Back to the start-up view: in front of the origin, slightly raised,
    /// looking towards +Y (where the sun is).
    pub fn reset(&mut self) {
        self.target = Vec3::ZERO;
        self.distance = 8.0;
        self.orientation = look_rotation(Vec3::Y, Vec3::Z);
        self.rotate_local_x(-15f32.to_radians());
        self.projection = Projection::Perspective;
    }

    // ----- derived values -------------------------------------------------

    pub fn forward(&self) -> Vec3 {
        self.orientation * Vec3::NEG_Z
    }

    pub fn right(&self) -> Vec3 {
        self.orientation * Vec3::X
    }

    pub fn up(&self) -> Vec3 {
        self.orientation * Vec3::Y
    }

    pub fn eye(&self) -> Vec3 {
        self.target - self.forward() * self.distance
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.orientation, self.eye()).inverse()
    }

    /// Half the visible height at the pivot distance, used for both the
    /// orthographic box and for converting pixels into world units.
    pub fn half_height_at_target(&self) -> f32 {
        self.distance * (self.fov_y * 0.5).tan()
    }

    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        match self.projection {
            Projection::Perspective => {
                Mat4::perspective_rh_gl(self.fov_y, aspect, self.near, self.far)
            }
            Projection::Orthographic => {
                let hh = self.half_height_at_target();
                let hw = hh * aspect;
                // Symmetric depth range so geometry behind the pivot is not clipped.
                Mat4::orthographic_rh_gl(-hw, hw, -hh, hh, -self.far, self.far)
            }
        }
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }

    // ----- actions --------------------------------------------------------

    /// Orbit around the pivot by a mouse delta in pixels (MMB drag).
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        let yaw = -dx * self.orbit_sensitivity;
        let pitch = -dy * self.orbit_sensitivity;
        match self.orbit_style {
            OrbitStyle::Turntable => {
                // World Z for left/right, camera X for up/down.
                self.orientation = (Quat::from_rotation_z(yaw)
                    * self.orientation
                    * Quat::from_rotation_x(pitch))
                .normalize();
            }
            OrbitStyle::Trackball => {
                self.orientation = (self.orientation
                    * Quat::from_rotation_y(yaw)
                    * Quat::from_rotation_x(pitch))
                .normalize();
            }
        }
    }

    /// Turntable step around world Z (numpad 4 / 6).
    pub fn rotate_world_z(&mut self, angle: f32) {
        self.orientation = (Quat::from_rotation_z(angle) * self.orientation).normalize();
    }

    /// Tilt step around the camera's X axis (numpad 2 / 8).
    pub fn rotate_local_x(&mut self, angle: f32) {
        self.orientation = (self.orientation * Quat::from_rotation_x(angle)).normalize();
    }

    /// Move the pivot so the scene follows the cursor (Shift + MMB drag).
    /// `dx`, `dy` are pixels; `viewport_height` is in pixels.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        if viewport_height <= 0.0 {
            return;
        }
        let world_per_pixel = 2.0 * self.half_height_at_target() / viewport_height;
        self.target += (-self.right() * dx + self.up() * dy) * world_per_pixel;
    }

    /// Zoom by scroll steps: positive zooms in.
    pub fn zoom_steps(&mut self, steps: f32) {
        self.distance = (self.distance * ZOOM_STEP.powf(-steps)).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Continuous zoom from a vertical drag (Ctrl + MMB): dragging up zooms in.
    pub fn zoom_drag(&mut self, dy: f32) {
        self.zoom_steps(-dy * 0.02);
    }

    pub fn set_view(&mut self, preset: ViewPreset) {
        let (forward, up) = match preset {
            ViewPreset::Front => (Vec3::Y, Vec3::Z),
            ViewPreset::Back => (Vec3::NEG_Y, Vec3::Z),
            ViewPreset::Right => (Vec3::NEG_X, Vec3::Z),
            ViewPreset::Left => (Vec3::X, Vec3::Z),
            ViewPreset::Top => (Vec3::NEG_Z, Vec3::Y),
            ViewPreset::Bottom => (Vec3::Z, Vec3::NEG_Y),
        };
        self.orientation = look_rotation(forward, up);
    }

    /// Look at the pivot from the opposite side (numpad 9).
    pub fn flip_view(&mut self) {
        self.orientation = (self.orientation * Quat::from_rotation_y(std::f32::consts::PI)).normalize();
    }

    pub fn toggle_projection(&mut self) {
        self.projection = match self.projection {
            Projection::Perspective => Projection::Orthographic,
            Projection::Orthographic => Projection::Perspective,
        };
    }
}

/// Rotation whose camera looks along `forward` with `up` roughly upward.
fn look_rotation(forward: Vec3, up: Vec3) -> Quat {
    let f = forward.normalize();
    let r = f.cross(up).normalize();
    let u = r.cross(f);
    Quat::from_mat3(&Mat3::from_cols(r, u, -f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn front_view_matches_blender() {
        let mut cam = OrbitCamera::default();
        cam.set_view(ViewPreset::Front);
        assert!(close(cam.forward(), Vec3::Y));
        assert!(close(cam.right(), Vec3::X));
        assert!(close(cam.up(), Vec3::Z));
        assert!(close(cam.eye(), Vec3::new(0.0, -cam.distance, 0.0)));
    }

    #[test]
    fn top_view_has_y_up_on_screen() {
        let mut cam = OrbitCamera::default();
        cam.set_view(ViewPreset::Top);
        assert!(close(cam.forward(), Vec3::NEG_Z));
        assert!(close(cam.up(), Vec3::Y));
        assert!(close(cam.right(), Vec3::X));
    }

    #[test]
    fn dragging_down_raises_the_eye() {
        let mut cam = OrbitCamera::default();
        cam.set_view(ViewPreset::Front);
        cam.orbit(0.0, 50.0);
        assert!(cam.eye().z > 0.0);
    }

    #[test]
    fn pan_moves_scene_with_cursor() {
        let mut cam = OrbitCamera::default();
        cam.set_view(ViewPreset::Front);
        cam.pan(100.0, 0.0, 800.0);
        // Dragging right moves the scene right, so the pivot moves left (-X).
        assert!(cam.target.x < 0.0);
    }
}
