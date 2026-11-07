//! Reads mouse and keyboard input and drives the [`OrbitCamera`] with
//! Blender's default viewport controls.
//!
//! | Input                         | Action                                   |
//! |-------------------------------|------------------------------------------|
//! | MMB drag                      | Orbit around the pivot                   |
//! | Shift + MMB drag              | Pan                                      |
//! | Ctrl + MMB drag               | Zoom (drag up = in)                      |
//! | Scroll wheel                  | Zoom                                     |
//! | Ctrl + scroll / Shift + scroll| Pan horizontally / vertically            |
//! | Numpad 1 / 3 / 7              | Front / right / top view                 |
//! | Ctrl + Numpad 1 / 3 / 7       | Back / left / bottom view                |
//! | Numpad 2 / 8, 4 / 6           | Orbit down / up, left / right by 15 deg  |
//! | Numpad 9                      | View from the opposite side              |
//! | Numpad 5                      | Toggle perspective / orthographic        |
//! | Numpad + / -                  | Zoom in / out                            |
//! | Numpad .                      | Re-centre the pivot on the origin        |
//! | Home                          | Reset the view                           |
//!
//! Events are collected as they arrive with [`CameraInput::handle_window_event`],
//! then applied once per frame with [`CameraInput::drive_camera`].

use glam::Vec2;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};

use super::camera::{OrbitCamera, ViewPreset};

/// Degrees per numpad orbit step, as in Blender.
const ORBIT_STEP_DEG: f32 = 15.0;
/// Pixels of pan per scroll notch for Ctrl/Shift + wheel.
const SCROLL_PAN_PIXELS: f32 = 40.0;
/// Trackpads report pixels instead of lines; this many pixels count as one notch.
const PIXELS_PER_SCROLL_LINE: f32 = 50.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragAction {
    Orbit,
    Pan,
    Zoom,
}

#[derive(Default)]
pub struct CameraInput {
    cursor: Option<Vec2>,
    /// Cursor movement since the last frame while a drag is active.
    drag_delta: Vec2,
    /// Scroll notches since the last frame (positive = away from the user).
    scroll: f32,
    /// Modifiers captured with the scroll so a quick Ctrl+scroll is not missed.
    scroll_modifiers: ModifiersState,
    modifiers: ModifiersState,
    /// Chosen when the middle button goes down, like Blender.
    drag: Option<DragAction>,
    /// Keys pressed since the last frame, with the modifiers held at the time.
    key_presses: Vec<(KeyCode, ModifiersState)>,
}

impl CameraInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// The drag in progress, if any (shown in the UI).
    pub fn current_drag(&self) -> Option<DragAction> {
        self.drag
    }

    /// Feed every window event through here. `ui_wants_mouse` / `ui_wants_keyboard`
    /// come from ImGui so that clicks and typing in UI panels do not move the camera.
    pub fn handle_window_event(
        &mut self,
        event: &WindowEvent,
        ui_wants_mouse: bool,
        ui_wants_keyboard: bool,
    ) {
        match event {
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),

            WindowEvent::CursorMoved { position, .. } => {
                let pos = Vec2::new(position.x as f32, position.y as f32);
                if let (Some(prev), Some(_)) = (self.cursor, self.drag) {
                    self.drag_delta += pos - prev;
                }
                self.cursor = Some(pos);
            }
            WindowEvent::CursorLeft { .. } => self.cursor = None,

            WindowEvent::MouseInput { state, button: MouseButton::Middle, .. } => match state {
                // Only start a drag outside ImGui windows; always allow it to end.
                ElementState::Pressed if !ui_wants_mouse => {
                    self.drag = Some(if self.modifiers.shift_key() {
                        DragAction::Pan
                    } else if self.modifiers.control_key() {
                        DragAction::Zoom
                    } else {
                        DragAction::Orbit
                    });
                }
                ElementState::Released => self.drag = None,
                _ => {}
            },

            WindowEvent::MouseWheel { delta, .. } if !ui_wants_mouse => {
                self.scroll += match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / PIXELS_PER_SCROLL_LINE,
                };
                self.scroll_modifiers = self.modifiers;
            }

            WindowEvent::KeyboardInput { event, .. }
                if !ui_wants_keyboard && event.state == ElementState::Pressed && !event.repeat =>
            {
                // Physical keys so the numpad works whether Num Lock is on or off.
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.key_presses.push((code, self.modifiers));
                }
            }

            WindowEvent::Focused(false) => {
                self.drag = None;
                self.modifiers = ModifiersState::empty();
            }
            _ => {}
        }
    }

    /// Applies everything gathered since the last call to the camera, then clears it.
    pub fn drive_camera(&mut self, camera: &mut OrbitCamera, viewport_height: f32) {
        let delta = std::mem::take(&mut self.drag_delta);
        if delta != Vec2::ZERO {
            match self.drag {
                Some(DragAction::Orbit) => camera.orbit(delta.x, delta.y),
                Some(DragAction::Pan) => camera.pan(delta.x, delta.y, viewport_height),
                Some(DragAction::Zoom) => camera.zoom_drag(delta.y),
                None => {}
            }
        }

        let scroll = std::mem::take(&mut self.scroll);
        if scroll != 0.0 {
            let m = self.scroll_modifiers;
            if m.control_key() {
                camera.pan(-scroll * SCROLL_PAN_PIXELS, 0.0, viewport_height);
            } else if m.shift_key() {
                camera.pan(0.0, scroll * SCROLL_PAN_PIXELS, viewport_height);
            } else {
                camera.zoom_steps(scroll);
            }
        }

        for (key, mods) in std::mem::take(&mut self.key_presses) {
            apply_key(camera, key, mods.control_key());
        }
    }
}

fn apply_key(camera: &mut OrbitCamera, key: KeyCode, ctrl: bool) {
    let step = ORBIT_STEP_DEG.to_radians();
    match key {
        KeyCode::Numpad1 => camera.set_view(if ctrl { ViewPreset::Back } else { ViewPreset::Front }),
        KeyCode::Numpad3 => camera.set_view(if ctrl { ViewPreset::Left } else { ViewPreset::Right }),
        KeyCode::Numpad7 => camera.set_view(if ctrl { ViewPreset::Bottom } else { ViewPreset::Top }),
        // Same directions as dragging the mouse.
        KeyCode::Numpad8 => camera.rotate_local_x(step),
        KeyCode::Numpad2 => camera.rotate_local_x(-step),
        KeyCode::Numpad4 => camera.rotate_world_z(step),
        KeyCode::Numpad6 => camera.rotate_world_z(-step),
        KeyCode::Numpad9 => camera.flip_view(),
        KeyCode::Numpad5 => camera.toggle_projection(),
        KeyCode::NumpadDecimal => camera.target = glam::Vec3::ZERO,
        KeyCode::Home => camera.reset(),
        KeyCode::NumpadAdd => camera.zoom_steps(1.0),
        KeyCode::NumpadSubtract => camera.zoom_steps(-1.0),
        _ => {}
    }
}
