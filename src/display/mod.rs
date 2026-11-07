//! OpenGL + Dear ImGui display for the mechanical spider.
//!
//! World convention (matches Blender): right-handed, **Z up**, `-Y` is "front"
//! (so the front view, numpad 1, looks along `+Y`).
//!
//! * [`app`]    – window, GL context, ImGui and the frame loop.
//! * [`skybox`] – equirectangular sky drawn behind everything else.
//! * [`camera`] – Blender-style orbit camera (pure maths, no input handling).
//! * [`grid`]   – floor grid with X/Y axes, a placeholder scene.
//! * [`mesh_renderer`] – draws a physics `Mesh` as balls and sticks.
//! * [`input`]  – collects mouse/keyboard events and turns them into camera actions.
//! * [`constants`] – tunable settings such as the GUI size.
//! * [`gl_util`] – small shader helpers.

mod app;
pub mod camera;
pub mod constants;
mod gl_util;
pub mod grid;
pub mod input;
pub mod mesh_renderer;
pub mod skybox;

pub use app::run;
