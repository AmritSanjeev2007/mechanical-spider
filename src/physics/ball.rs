//! A ball: one point joint of the body.

use glam::{Quat, Vec3};

use super::constraints::{SwingLimit, TwistLimit};

/// One joint of the body, stored as a node of the mesh's ball tree.
///
/// The bone that ends at this ball runs from its parent's position to
/// `position`; that direction is the ball's forward vector (local +Y).
#[derive(Debug, Clone, PartialEq)]
pub struct Ball {
    /// World-space position.
    pub position: Vec3,
    /// Current roll of this bone about its own forward axis, in radians,
    /// measured from its rest frame. The position alone cannot carry this, so
    /// it is the one piece of orientation state a ball keeps. Most spider
    /// joints lock it (see [`Joint::twist`]), which keeps it at zero.
    pub twist: f32,
    /// How this ball may sit relative to its parent. Ignored on the root.
    pub joint: Joint,
}

impl Ball {
    pub fn new(position: Vec3, joint: Joint) -> Self {
        Self { position, twist: 0.0, joint }
    }

    /// A ball with no limits, for the root of the tree.
    pub fn root(position: Vec3) -> Self {
        Self::new(position, Joint::free(0.0))
    }
}

/// The constraints of the bone from a ball's parent to the ball.
///
/// Angles are measured in the bone's **rest frame**: the parent's frame
/// rotated by [`rest`](Self::rest). In that frame +Y is the rest direction of
/// the bone and Z is its reference "up", which is what names the two swing
/// axes and makes them distinguishable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Joint {
    /// Rest orientation of this bone relative to its parent's frame.
    pub rest: Quat,
    /// Bone length (parent to ball). `None` leaves the distance free.
    pub length: Option<f32>,
    /// Allowed bend away from the rest direction, about local X and Z.
    pub swing: SwingLimit,
    /// Allowed roll about the bone.
    pub twist: TwistLimit,
}

impl Joint {
    /// No angular limits and no fixed length beyond `length` (if `> 0`).
    pub fn free(length: f32) -> Self {
        Self {
            rest: Quat::IDENTITY,
            length: (length > 0.0).then_some(length),
            swing: SwingLimit::FREE,
            twist: TwistLimit::FREE,
        }
    }

    /// A bone of fixed `length` whose rest frame is `rest` (relative to the
    /// parent's frame), with no twist allowed.
    pub fn new(rest: Quat, length: f32, swing: SwingLimit) -> Self {
        Self { rest, length: Some(length), swing, twist: TwistLimit::LOCKED }
    }

    /// Rest frame from a rest direction and a reference up vector, both in the
    /// parent's frame. `up` is made perpendicular to `forward`; it becomes the
    /// bone's local Z. Falls back to any perpendicular if they are parallel.
    pub fn rest_from_forward_up(forward: Vec3, up: Vec3) -> Quat {
        let y = forward.normalize();
        let mut z = up - y * up.dot(y);
        if z.length_squared() < 1e-8 {
            z = y.any_orthonormal_vector();
        }
        let z = z.normalize();
        let x = y.cross(z);
        Quat::from_mat3(&glam::Mat3::from_cols(x, y, z))
    }
}
