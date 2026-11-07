//! Angular constraint maths: swing/twist decomposition and the limit types.
//!
//! Every joint orientation is split, in the bone's *rest frame*, into
//!
//! ```text
//! q = swing * twist
//! ```
//!
//! * **twist** – rotation about the bone's own axis (local +Y),
//! * **swing** – rotation about an axis lying in the local XZ plane, the
//!   plane normal to the bone. It is stored as a *swing vector*
//!   `(x, z) = axis * angle`, so `x` is "how far it bent about local X" and
//!   `z` is "how far it bent about local Z". Because the rest frame names X and
//!   Z, the two in-plane axes can be told apart.
//!
//! The swing limit is an elliptical cone with separate positive and negative
//! reaches on each axis (one quarter-ellipse per quadrant). Setting both
//! reaches of an axis to zero turns it into a hinge.

use glam::{Quat, Vec2, Vec3};

/// Tolerance used to treat lengths and limits as zero.
pub const EPSILON: f32 = 1e-6;

/// A closed interval of angles in radians, `min <= max`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AngleRange {
    pub min: f32,
    pub max: f32,
}

impl AngleRange {
    pub const LOCKED: Self = Self { min: 0.0, max: 0.0 };
    pub const FREE: Self = Self { min: -std::f32::consts::PI, max: std::f32::consts::PI };

    pub fn new(min: f32, max: f32) -> Self {
        Self { min: min.min(max), max: min.max(max) }
    }

    /// `-limit ..= limit`.
    pub fn symmetric(limit: f32) -> Self {
        Self::new(-limit.abs(), limit.abs())
    }

    pub fn clamp(&self, angle: f32) -> f32 {
        angle.clamp(self.min, self.max)
    }

    pub fn contains(&self, angle: f32) -> bool {
        (self.min - EPSILON..=self.max + EPSILON).contains(&angle)
    }

    /// The reach on the side `angle` points to (always `>= 0`).
    fn reach_towards(&self, angle: f32) -> f32 {
        if angle >= 0.0 { self.max.max(0.0) } else { (-self.min).max(0.0) }
    }
}

/// Limits on the swing vector `(x, z)`: how far the bone may bend away from
/// its rest direction, about local X and about local Z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwingLimit {
    /// Allowed bend about local X (tips the bone towards local ±Z).
    pub x: AngleRange,
    /// Allowed bend about local Z (tips the bone towards local ∓X).
    pub z: AngleRange,
}

impl SwingLimit {
    pub const FREE: Self = Self { x: AngleRange::FREE, z: AngleRange::FREE };
    pub const LOCKED: Self = Self { x: AngleRange::LOCKED, z: AngleRange::LOCKED };

    /// Circular cone of the given half-angle.
    pub fn cone(half_angle: f32) -> Self {
        Self { x: AngleRange::symmetric(half_angle), z: AngleRange::symmetric(half_angle) }
    }

    /// Hinge about local X only (a knee).
    pub fn hinge_x(range: AngleRange) -> Self {
        Self { x: range, z: AngleRange::LOCKED }
    }

    /// Hinge about local Z only.
    pub fn hinge_z(range: AngleRange) -> Self {
        Self { x: AngleRange::LOCKED, z: range }
    }

    pub fn contains(&self, swing: Vec2) -> bool {
        (self.clamp(swing) - swing).length() <= 1e-4
    }

    /// Projects a swing vector into the limit.
    ///
    /// Inside each quadrant the boundary is the quarter-ellipse with semi-axes
    /// equal to the reaches on that side. Outside points are pulled radially
    /// towards the origin, which is cheap, continuous and keeps the bend
    /// direction. A zero reach on an axis turns that quadrant into a segment.
    pub fn clamp(&self, swing: Vec2) -> Vec2 {
        let rx = self.x.reach_towards(swing.x);
        let rz = self.z.reach_towards(swing.y);
        match (rx > EPSILON, rz > EPSILON) {
            (false, false) => Vec2::ZERO,
            (true, false) => Vec2::new(self.x.clamp(swing.x), 0.0),
            (false, true) => Vec2::new(0.0, self.z.clamp(swing.y)),
            (true, true) => {
                let e = (swing.x / rx).powi(2) + (swing.y / rz).powi(2);
                if e <= 1.0 { swing } else { swing / e.sqrt() }
            }
        }
    }
}

/// Limits on the twist (roll) of a bone about its own axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwistLimit(pub AngleRange);

impl TwistLimit {
    pub const FREE: Self = Self(AngleRange::FREE);
    pub const LOCKED: Self = Self(AngleRange::LOCKED);

    pub fn clamp(&self, twist: f32) -> f32 {
        self.0.clamp(twist)
    }
}

/// Splits `q` into `(swing, twist)` with `q = swing * twist`, where `twist`
/// rotates about the unit `axis` and `swing`'s axis is perpendicular to it.
pub fn swing_twist(q: Quat, axis: Vec3) -> (Quat, Quat) {
    let v = Vec3::new(q.x, q.y, q.z);
    let p = axis * v.dot(axis);
    let t = Quat::from_xyzw(p.x, p.y, p.z, q.w);
    let twist = if t.length_squared() < EPSILON { Quat::IDENTITY } else { t.normalize() };
    (q * twist.conjugate(), twist)
}

/// Twist angle of `q` about local +Y, in `(-π, π]`.
pub fn twist_angle_y(q: Quat) -> f32 {
    let (_, twist) = swing_twist(q, Vec3::Y);
    let angle = 2.0 * twist.y.atan2(twist.w);
    wrap_angle(angle)
}

/// Swing vector `(x, z)` that rotates local +Y onto the unit `dir` (given in
/// the rest frame) by the shortest arc. Undefined direction at `dir = -Y`.
pub fn swing_vector_to(dir: Vec3) -> Vec2 {
    let q = Quat::from_rotation_arc(Vec3::Y, dir.normalize());
    let s = q.to_scaled_axis();
    Vec2::new(s.x, s.z)
}

/// Rotation for a swing vector `(x, z)`; its axis lies in the local XZ plane.
pub fn swing_quat(swing: Vec2) -> Quat {
    Quat::from_scaled_axis(Vec3::new(swing.x, 0.0, swing.y))
}

/// Wraps an angle into `(-π, π]`.
pub fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI { w + TAU } else { w }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn swing_twist_recomposes() {
        let q = Quat::from_euler(glam::EulerRot::XYZ, 0.3, 1.1, -0.4);
        let (swing, twist) = swing_twist(q, Vec3::Y);
        assert!((swing * twist).abs_diff_eq(q, 1e-5));
        // The swing axis lies in the XZ plane.
        let (axis, _) = swing.to_axis_angle();
        assert!(axis.y.abs() < 1e-4);
    }

    #[test]
    fn twist_angle_is_recovered() {
        let q = swing_quat(Vec2::new(0.4, -0.2)) * Quat::from_rotation_y(0.7);
        assert!((twist_angle_y(q) - 0.7).abs() < 1e-4);
    }

    #[test]
    fn swing_vector_round_trips() {
        let s = Vec2::new(0.5, -0.3);
        let dir = swing_quat(s) * Vec3::Y;
        assert!(close(swing_vector_to(dir), s));
    }

    #[test]
    fn swing_axes_are_distinguished() {
        // Bending about X moves the bone towards Z; bending about Z moves it towards -X.
        assert!(close(swing_vector_to(Vec3::new(0.0, 1.0, 1.0)), Vec2::new(FRAC_PI_2 / 2.0, 0.0)));
        assert!(close(swing_vector_to(Vec3::new(-1.0, 1.0, 0.0)), Vec2::new(0.0, FRAC_PI_2 / 2.0)));
    }

    #[test]
    fn hinge_removes_lateral_bend() {
        let hinge = SwingLimit::hinge_x(AngleRange::new(0.0, 2.0));
        assert!(close(hinge.clamp(Vec2::new(1.0, 0.5)), Vec2::new(1.0, 0.0)));
        assert!(close(hinge.clamp(Vec2::new(-1.0, 0.0)), Vec2::ZERO));
        assert!(close(hinge.clamp(Vec2::new(3.0, 0.0)), Vec2::new(2.0, 0.0)));
    }

    #[test]
    fn elliptical_cone_is_asymmetric() {
        let limit = SwingLimit { x: AngleRange::new(-0.2, 1.0), z: AngleRange::symmetric(0.5) };
        assert!(limit.contains(Vec2::new(0.9, 0.0)));
        assert!(!limit.contains(Vec2::new(-0.3, 0.0)));
        let c = limit.clamp(Vec2::new(2.0, 2.0));
        assert!(((c.x / 1.0).powi(2) + (c.y / 0.5).powi(2) - 1.0).abs() < 1e-4);
        assert!((c.x - c.y).abs() < 1e-5); // direction kept
    }
}
