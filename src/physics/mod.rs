//! Physics body for the mechanical spider.
//!
//! A [`Mesh`] is one rigid-ish body: it tracks its actual world pose and owns
//! a tree of [`Ball`]s (an [`indextree::Arena`], so every ball is wrapped in a
//! generic tree node). Each ball is a point joint: it knows its world position
//! and the [`Joint`] limits that say how it may sit relative to its parent.
//!
//! Conventions (same as the display, and as Blender bones):
//!
//! * World is right-handed and **Z up**.
//! * A bone's local frame has **+Y along the bone** (parent -> ball, the
//!   "forward" vector). Local **X** and **Z** are the two swing axes, and local
//!   Z doubles as the bone's reference "up" (its roll).
//!
//! Why the frames matter: the forward vector alone cannot tell the two swing
//! axes apart (any rotation about forward gives an equally valid X/Z pair). We
//! fix that by propagating a full frame down from the mesh's own orientation;
//! see `constraints.typ` next to this file for the methods considered and why
//! this one was picked.
//!
//! * [`ball`]        – [`Ball`] and its [`Joint`] limits.
//! * [`constraints`] – swing/twist maths and the limit types.
//! * [`mesh`]        – [`Mesh`], the ball tree, frame propagation and projection.

pub mod ball;
pub mod constraints;
pub mod mesh;

pub use ball::{Ball, Joint};
pub use constraints::{AngleRange, SwingLimit, TwistLimit};
pub use mesh::{BallId, Mesh};
