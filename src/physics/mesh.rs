//! The mesh: a body's actual pose plus the tree of balls it owns.

use std::collections::HashMap;

use glam::{Quat, Vec2, Vec3};
use indextree::{Arena, NodeId};

use super::ball::{Ball, Joint};
use super::constraints::{swing_quat, swing_vector_to, EPSILON};

/// Handle to a ball inside a [`Mesh`].
pub type BallId = NodeId;

/// A body made of balls joined in a tree.
///
/// `position` and `orientation` are the body's actual pose in the world. The
/// root ball sits at `position`, and `orientation` is the root's frame, which
/// every other ball's frame is propagated from. Ball positions are stored in
/// world space so contacts and integration need no conversion.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub position: Vec3,
    pub orientation: Quat,
    balls: Arena<Ball>,
    root: BallId,
}

/// A ball's pose relative to its rest frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalPose {
    /// Swing vector `(x, z)`: bend about local X and local Z, in radians.
    pub swing: Vec2,
    /// Roll about the bone, in radians.
    pub twist: f32,
    /// Current distance from the parent.
    pub length: f32,
}

impl Mesh {
    /// A mesh with a single root ball at `position`.
    pub fn new(position: Vec3, orientation: Quat) -> Self {
        let mut balls = Arena::new();
        let root = balls.new_node(Ball::root(position));
        Self { position, orientation: orientation.normalize(), balls, root }
    }

    pub fn root(&self) -> BallId {
        self.root
    }

    pub fn len(&self) -> usize {
        self.balls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn ball(&self, id: BallId) -> &Ball {
        self.balls[id].get()
    }

    pub fn ball_mut(&mut self, id: BallId) -> &mut Ball {
        self.balls[id].get_mut()
    }

    pub fn parent(&self, id: BallId) -> Option<BallId> {
        id.ancestors(&self.balls).nth(1)
    }

    pub fn children(&self, id: BallId) -> impl Iterator<Item = BallId> + '_ {
        id.children(&self.balls)
    }

    /// Every ball, parents before children (pre-order from the root).
    pub fn ids(&self) -> impl Iterator<Item = BallId> + '_ {
        self.root.descendants(&self.balls)
    }

    /// The underlying generic tree, for traversals not wrapped here.
    pub fn tree(&self) -> &Arena<Ball> {
        &self.balls
    }

    /// Adds `ball` as a child of `parent` exactly as given.
    pub fn add_ball(&mut self, parent: BallId, ball: Ball) -> BallId {
        parent.append_value(ball, &mut self.balls)
    }

    /// Adds a child of `parent` placed at its rest pose: along its rest
    /// direction at its rest length (unit length if the joint has none).
    pub fn attach(&mut self, parent: BallId, joint: Joint) -> BallId {
        let rest_frame = self.frame(parent) * joint.rest;
        let position = self.ball(parent).position + rest_frame * Vec3::Y * joint.length.unwrap_or(1.0);
        self.add_ball(parent, Ball::new(position, joint))
    }

    /// World frame of one ball (local +Y along its bone). Walks up to the root.
    pub fn frame(&self, id: BallId) -> Quat {
        match self.parent(id) {
            None => self.orientation,
            Some(parent) => self.child_frame(self.frame(parent), parent, id),
        }
    }

    /// World frames of every ball in one pass.
    pub fn frames(&self) -> HashMap<BallId, Quat> {
        let mut frames = HashMap::with_capacity(self.len());
        frames.insert(self.root, self.orientation);
        for id in self.ids().skip(1) {
            let parent = self.parent(id).expect("non-root ball has a parent");
            let frame = self.child_frame(frames[&parent], parent, id);
            frames.insert(id, frame);
        }
        frames
    }

    /// Pose of a non-root ball relative to its rest frame; `None` for the root.
    pub fn local_pose(&self, id: BallId) -> Option<LocalPose> {
        let parent = self.parent(id)?;
        let rest_frame = self.frame(parent) * self.ball(id).joint.rest;
        let (swing, length) = self.swing_in(rest_frame, parent, id);
        Some(LocalPose { swing, twist: self.ball(id).twist, length })
    }

    /// Whether every joint is within its limits (and lengths within `tolerance`).
    pub fn satisfies_constraints(&self, tolerance: f32) -> bool {
        let frames = self.frames();
        self.ids().skip(1).all(|id| {
            let parent = self.parent(id).unwrap();
            let ball = self.ball(id);
            let rest_frame = frames[&parent] * ball.joint.rest;
            let (swing, length) = self.swing_in(rest_frame, parent, id);
            ball.joint.swing.contains(swing)
                && ball.joint.twist.0.contains(ball.twist)
                && ball.joint.length.is_none_or(|l| (length - l).abs() <= tolerance)
        })
    }

    /// Projects every ball onto its constraints, root first.
    ///
    /// The root is pinned to `position`. Then each ball's bend is clamped by
    /// its swing limit, its twist by its twist limit and its distance set to
    /// its length. When a ball moves, its whole subtree moves with it so
    /// descendant bone lengths are kept.
    pub fn enforce_constraints(&mut self) {
        let root_delta = self.position - self.ball(self.root).position;
        self.translate_subtree(self.root, root_delta);

        let order: Vec<BallId> = self.ids().collect();
        let mut frames = HashMap::with_capacity(order.len());
        frames.insert(self.root, self.orientation);
        for &id in &order[1..] {
            let parent = self.parent(id).unwrap();
            let joint = self.ball(id).joint;
            let rest_frame = frames[&parent] * joint.rest;
            let (swing, length) = self.swing_in(rest_frame, parent, id);
            let swing = joint.swing.clamp(swing);
            let twist = joint.twist.clamp(self.ball(id).twist);
            let length = joint.length.unwrap_or(length);

            let bone_frame = rest_frame * swing_quat(swing);
            let target = self.ball(parent).position + bone_frame * Vec3::Y * length;
            let delta = target - self.ball(id).position;
            self.translate_subtree(id, delta);
            self.ball_mut(id).twist = twist;
            frames.insert(id, bone_frame * Quat::from_rotation_y(twist));
        }
    }

    /// Moves the whole body (pose and every ball) by `delta`.
    pub fn translate(&mut self, delta: Vec3) {
        self.position += delta;
        self.translate_subtree(self.root, delta);
    }

    /// Rigidly moves the body to a new pose, carrying every ball along.
    pub fn set_pose(&mut self, position: Vec3, orientation: Quat) {
        let orientation = orientation.normalize();
        let turn = orientation * self.orientation.inverse();
        let old = self.position;
        for node in self.balls.iter_mut() {
            let ball = node.get_mut();
            ball.position = position + turn * (ball.position - old);
        }
        self.position = position;
        self.orientation = orientation;
    }

    /// Mean position of all balls.
    pub fn centroid(&self) -> Vec3 {
        let sum: Vec3 = self.ids().map(|id| self.ball(id).position).sum();
        sum / self.len() as f32
    }

    fn translate_subtree(&mut self, id: BallId, delta: Vec3) {
        if delta.length_squared() == 0.0 {
            return;
        }
        let ids: Vec<BallId> = id.descendants(&self.balls).collect();
        for id in ids {
            self.ball_mut(id).position += delta;
        }
    }

    /// Frame of `id` given its parent's world frame.
    fn child_frame(&self, parent_frame: Quat, parent: BallId, id: BallId) -> Quat {
        let ball = self.ball(id);
        let rest_frame = parent_frame * ball.joint.rest;
        let (swing, _) = self.swing_in(rest_frame, parent, id);
        rest_frame * swing_quat(swing) * Quat::from_rotation_y(ball.twist)
    }

    /// Swing vector of the bone parent -> id in `rest_frame`, and its length.
    fn swing_in(&self, rest_frame: Quat, parent: BallId, id: BallId) -> (Vec2, f32) {
        let bone = self.ball(id).position - self.ball(parent).position;
        let length = bone.length();
        if length < EPSILON {
            return (Vec2::ZERO, length);
        }
        (swing_vector_to(rest_frame.inverse() * bone / length), length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::constraints::{AngleRange, SwingLimit, TwistLimit};
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

    /// Body at the origin facing -Y (Blender front), with one leg pointing +X:
    /// a coxa that may yaw, then a femur that only hinges up and down.
    fn one_leg() -> (Mesh, BallId, BallId) {
        let mut mesh = Mesh::new(Vec3::ZERO, Quat::IDENTITY);
        let coxa_rest = Joint::rest_from_forward_up(Vec3::X, Vec3::Z);
        let coxa = mesh.attach(
            mesh.root(),
            Joint::new(coxa_rest, 1.0, SwingLimit::hinge_z(AngleRange::symmetric(FRAC_PI_4))),
        );
        let femur = mesh.attach(
            coxa,
            Joint::new(Quat::IDENTITY, 2.0, SwingLimit::hinge_x(AngleRange::new(-FRAC_PI_2, FRAC_PI_4))),
        );
        (mesh, coxa, femur)
    }

    #[test]
    fn attach_places_balls_at_rest() {
        let (mesh, coxa, femur) = one_leg();
        assert!(mesh.ball(coxa).position.abs_diff_eq(Vec3::X, 1e-5));
        assert!(mesh.ball(femur).position.abs_diff_eq(Vec3::X * 3.0, 1e-5));
        assert!(mesh.satisfies_constraints(1e-4));
        assert_eq!(mesh.parent(femur), Some(coxa));
        assert_eq!(mesh.ids().count(), 3);
    }

    #[test]
    fn bends_in_the_normal_plane_are_told_apart() {
        let (mut mesh, coxa, femur) = one_leg();
        // Femur lifted up (+Z) vs swept forward (-Y): same angle, different axes.
        mesh.ball_mut(femur).position = Vec3::X + Vec3::new(1.0, 0.0, 1.0).normalize() * 2.0;
        let up = mesh.local_pose(femur).unwrap().swing;
        mesh.ball_mut(femur).position = Vec3::X + Vec3::new(1.0, -1.0, 0.0).normalize() * 2.0;
        let sweep = mesh.local_pose(femur).unwrap().swing;
        assert!(up.y.abs() < 1e-4 && (up.x - FRAC_PI_4).abs() < 1e-4);
        assert!(sweep.x.abs() < 1e-4 && sweep.y.abs() > 0.7);
        let _ = coxa;
    }

    #[test]
    fn enforce_projects_onto_the_hinge() {
        let (mut mesh, _, femur) = one_leg();
        // Sweep the femur sideways and stretch it: the hinge forbids both.
        mesh.ball_mut(femur).position = Vec3::new(2.0, -1.5, 0.3);
        assert!(!mesh.satisfies_constraints(1e-4));
        mesh.enforce_constraints();
        assert!(mesh.satisfies_constraints(1e-4));
        let p = mesh.ball(femur).position;
        assert!(p.y.abs() < 1e-4, "lateral sweep removed: {p}");
        assert!(((p - Vec3::X).length() - 2.0).abs() < 1e-4);
    }

    #[test]
    fn subtree_follows_its_parent() {
        let (mut mesh, coxa, femur) = one_leg();
        // Yaw the coxa past its limit; the femur should come along rigidly.
        mesh.ball_mut(coxa).position = Vec3::new(0.0, 1.0, 0.0);
        mesh.enforce_constraints();
        assert!(mesh.satisfies_constraints(1e-4));
        let coxa_dir = mesh.ball(coxa).position.normalize();
        assert!((coxa_dir.angle_between(Vec3::X) - FRAC_PI_4).abs() < 1e-4);
        let femur_dir = (mesh.ball(femur).position - mesh.ball(coxa).position).normalize();
        assert!(femur_dir.abs_diff_eq(coxa_dir, 1e-3));
    }

    #[test]
    fn twist_rotates_child_axes() {
        let (mut mesh, coxa, femur) = one_leg();
        mesh.ball_mut(coxa).joint.twist = TwistLimit::FREE;
        mesh.ball_mut(coxa).twist = FRAC_PI_2;
        // With the coxa rolled 90°, the femur's hinge now bends towards ±Y instead of ±Z.
        mesh.ball_mut(femur).position = Vec3::X + Vec3::new(1.0, 0.0, 1.0).normalize() * 2.0;
        mesh.enforce_constraints();
        let p = mesh.ball(femur).position;
        assert!(p.z.abs() < 1e-4, "bend about the rolled axis only: {p}");
    }

    #[test]
    fn set_pose_moves_everything() {
        let (mut mesh, _, femur) = one_leg();
        mesh.set_pose(Vec3::new(0.0, 0.0, 5.0), Quat::from_rotation_z(FRAC_PI_2));
        assert!(mesh.ball(femur).position.abs_diff_eq(Vec3::new(0.0, 3.0, 5.0), 1e-4));
        assert!(mesh.satisfies_constraints(1e-4));
        mesh.translate(Vec3::X);
        assert!(mesh.ball(mesh.root()).position.abs_diff_eq(mesh.position, 1e-6));
    }
}
