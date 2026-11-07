//! Window, OpenGL context, ImGui and the frame loop.

use std::error::Error;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Instant;

use glow::HasContext;
use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext,
    Version,
};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::DisplayBuilder;
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use super::constants::{GUI_BASE_FONT_SIZE, GUI_SCALE};
use super::camera::{OrbitCamera, OrbitStyle, Projection, ViewPreset};
use super::grid::Grid;
use super::input::{CameraInput, DragAction};
use super::mesh_renderer::{MeshLighting, MeshRenderer, MeshStyle};
use super::skybox::Skybox;
use crate::physics::{Joint, Mesh, SwingLimit};

const TITLE: &str = "Mechanical Spider";
const INITIAL_SIZE: (u32, u32) = (1280, 720);

fn sky_texture_path() -> PathBuf {
    // Prefer the crate's own assets folder so `cargo run` works from anywhere,
    // and fall back to ./assets next to wherever the binary is started.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/sky.png");
    if manifest.exists() { manifest } else { PathBuf::from("assets/sky.png") }
}

/// Opens the window and runs until it is closed.
pub fn run() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    match app.error.take() {
        Some(err) => Err(err.into()),
        None => Ok(()),
    }
}

#[derive(Default)]
struct App {
    display: Option<Display>,
    error: Option<String>,
}

/// Everything that only exists while the window is open.
struct Display {
    // GL objects first so they are dropped before the context.
    skybox: Skybox,
    grid: Grid,
    mesh_renderer: MeshRenderer,
    renderer: imgui_glow_renderer::AutoRenderer,
    platform: WinitPlatform,
    imgui: imgui::Context,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    window: Window,

    camera: OrbitCamera,
    input: CameraInput,
    last_frame: Instant,

    /// Placeholder body until the simulation drives one.
    demo_mesh: Mesh,
    mesh_style: MeshStyle,
    show_mesh: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.display.is_some() {
            return;
        }
        match Display::new(event_loop) {
            Ok(display) => self.display = Some(display),
            Err(err) => {
                self.error = Some(err.to_string());
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(display) = self.display.as_mut() else { return };

        // Camera input first, using ImGui's capture flags from the last frame.
        let io = display.imgui.io();
        display
            .input
            .handle_window_event(&event, io.want_capture_mouse, io.want_capture_keyboard);

        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) => {
                if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                    display.surface.resize(&display.context, w, h);
                }
            }
            WindowEvent::RedrawRequested => display.render_frame(),
            _ => {}
        }

        let wrapped: Event<()> = Event::WindowEvent { window_id, event };
        display.platform.handle_event(display.imgui.io_mut(), &display.window, &wrapped);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(display) = self.display.as_mut() {
            let now = Instant::now();
            display.imgui.io_mut().update_delta_time(now - display.last_frame);
            display.last_frame = now;
            if let Err(err) = display.platform.prepare_frame(display.imgui.io_mut(), &display.window) {
                eprintln!("imgui prepare_frame failed: {err}");
            }
            display.window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(display) = self.display.take() {
            let gl = display.renderer.gl_context();
            display.skybox.destroy(gl);
            display.grid.destroy(gl);
            display.mesh_renderer.destroy(gl);
        }
    }
}

impl Display {
    fn new(event_loop: &ActiveEventLoop) -> Result<Self, Box<dyn Error>> {
        let window_attributes = WindowAttributes::default()
            .with_title(TITLE)
            .with_inner_size(LogicalSize::new(INITIAL_SIZE.0, INITIAL_SIZE.1));

        // 24-bit depth buffer for the scene that will be drawn over the sky.
        let template = ConfigTemplateBuilder::new().with_depth_size(24);
        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(window_attributes))
            .build(event_loop, template, pick_config)?;
        let window = window.ok_or("glutin did not create a window")?;
        let raw_handle = window.window_handle()?.as_raw();

        let gl_display = config.display();
        let core_33 = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .with_profile(GlProfile::Core)
            .build(Some(raw_handle));
        let fallback = ContextAttributesBuilder::new().build(Some(raw_handle));
        let not_current = unsafe {
            gl_display
                .create_context(&config, &core_33)
                .or_else(|_| gl_display.create_context(&config, &fallback))?
        };

        let size = window.inner_size();
        let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_handle,
            NonZeroU32::new(size.width.max(1)).unwrap(),
            NonZeroU32::new(size.height.max(1)).unwrap(),
        );
        let surface = unsafe { gl_display.create_window_surface(&config, &surface_attributes)? };
        let context = not_current.make_current(&surface)?;
        // V-sync; not fatal if the driver refuses.
        let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|s| gl_display.get_proc_address(s).cast())
        };

        // ImGui
        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(None);
        let mut platform = WinitPlatform::new(&mut imgui);
        platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Rounded);
        // Rasterise the font at its final on-screen size so scaled text stays sharp,
        // then size padding, spacing, rounding etc. to match.
        let hidpi = platform.hidpi_factor() as f32;
        imgui.fonts().add_font(&[imgui::FontSource::DefaultFontData {
            config: Some(imgui::FontConfig {
                size_pixels: GUI_BASE_FONT_SIZE * GUI_SCALE * hidpi,
                ..imgui::FontConfig::default()
            }),
        }]);
        imgui.io_mut().font_global_scale = 1.0 / hidpi;
        imgui.style_mut().scale_all_sizes(GUI_SCALE);

        let skybox = Skybox::new(&gl, &sky_texture_path())?;
        let grid = Grid::new(&gl)?;
        let mesh_renderer = MeshRenderer::new(&gl)?;
        // The renderer takes ownership of the GL context; everything else borrows it back.
        let renderer = imgui_glow_renderer::AutoRenderer::new(gl, &mut imgui)
            .map_err(|e| format!("failed to create ImGui renderer: {e}"))?;

        Ok(Self {
            skybox,
            grid,
            mesh_renderer,
            renderer,
            platform,
            imgui,
            surface,
            context,
            window,
            camera: OrbitCamera::default(),
            input: CameraInput::new(),
            last_frame: Instant::now(),
            demo_mesh: demo_spider(),
            mesh_style: MeshStyle::default(),
            show_mesh: true,
        })
    }

    fn render_frame(&mut self) {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return; // minimised
        }
        let aspect = size.width as f32 / size.height as f32;

        self.input.drive_camera(&mut self.camera, size.height as f32);

        let gl = self.renderer.gl_context().clone();
        unsafe {
            gl.viewport(0, 0, size.width as i32, size.height as i32);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        }

        // 1. Sky, behind everything.
        self.skybox.draw(&gl, &self.camera, aspect);

        // 2. Scene geometry, depth-tested against each other but always in front of the sky.
        let view_proj = self.camera.view_projection(aspect);
        self.grid.draw(&gl, view_proj);
        if self.show_mesh {
            let lighting = MeshLighting {
                view_proj,
                eye: self.camera.eye(),
                light_dir: sun_direction(self.skybox.sun_azimuth),
            };
            self.mesh_renderer.draw(&gl, &self.demo_mesh, &self.mesh_style, &lighting);
        }

        // 3. UI on top.
        let ui = self.imgui.frame();
        draw_camera_panel(ui, &mut self.camera, &mut self.skybox, &mut self.grid, self.input.current_drag());
        draw_mesh_panel(ui, &mut self.show_mesh, &mut self.mesh_style);
        self.platform.prepare_render(ui, &self.window);
        let draw_data = self.imgui.render();
        // imgui 0.12 builds a slice from a null pointer when there are no draw
        // lists (e.g. the very first frame), which debug builds abort on.
        let ui_result = match draw_data.draw_lists_count() {
            0 => Ok(()),
            _ => self.renderer.render(draw_data),
        };
        if let Err(err) = ui_result {
            eprintln!("imgui render failed: {err}");
        }

        if let Err(err) = self.surface.swap_buffers(&self.context) {
            eprintln!("swap_buffers failed: {err}");
        }
    }
}

/// Prefer multisampled configs, then whatever comes first.
fn pick_config(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    configs
        .reduce(|best, c| if c.num_samples() > best.num_samples() { c } else { best })
        .expect("no OpenGL config available")
}

fn draw_camera_panel(
    ui: &imgui::Ui,
    camera: &mut OrbitCamera,
    skybox: &mut Skybox,
    grid: &mut Grid,
    drag: Option<DragAction>,
) {
    ui.window("Camera")
        .position([10.0 * GUI_SCALE, 10.0 * GUI_SCALE], imgui::Condition::FirstUseEver)
        .size([300.0 * GUI_SCALE, 0.0], imgui::Condition::FirstUseEver)
        .build(|| {
            ui.text(format!("{:.0} FPS", ui.io().framerate));
            ui.text(match drag {
                Some(DragAction::Orbit) => "Orbiting",
                Some(DragAction::Pan) => "Panning",
                Some(DragAction::Zoom) => "Zooming",
                None => " ",
            });
            ui.separator();

            let t = camera.target;
            ui.text(format!("Pivot  {:.2}, {:.2}, {:.2}", t.x, t.y, t.z));
            ui.text(format!("Distance {:.2}", camera.distance));

            let mut perspective = camera.projection == Projection::Perspective;
            if ui.checkbox("Perspective (Numpad 5)", &mut perspective) {
                camera.toggle_projection();
            }
            let mut trackball = camera.orbit_style == OrbitStyle::Trackball;
            if ui.checkbox("Trackball orbit", &mut trackball) {
                camera.orbit_style = if trackball { OrbitStyle::Trackball } else { OrbitStyle::Turntable };
            }
            let mut fov_deg = camera.fov_y.to_degrees();
            if ui.slider("Field of view", 10.0, 120.0, &mut fov_deg) {
                camera.fov_y = fov_deg.to_radians();
            }
            let mut sun_deg = skybox.sun_azimuth.to_degrees();
            if ui.slider("Sun azimuth", -180.0, 180.0, &mut sun_deg) {
                skybox.sun_azimuth = sun_deg.to_radians();
            }
            ui.checkbox("Floor grid", &mut grid.visible);

            ui.separator();
            ui.text("Views");
            for (label, preset) in [
                ("Front", ViewPreset::Front),
                ("Right", ViewPreset::Right),
                ("Top", ViewPreset::Top),
            ] {
                if ui.button(label) {
                    camera.set_view(preset);
                }
                ui.same_line();
            }
            if ui.button("Reset") {
                camera.reset();
            }

            if ui.collapsing_header("Controls", imgui::TreeNodeFlags::empty()) {
                ui.text_wrapped(
                    "MMB drag: orbit\n\
                     Shift+MMB: pan\n\
                     Ctrl+MMB / wheel: zoom\n\
                     Ctrl/Shift+wheel: pan\n\
                     Numpad 1/3/7: front/right/top (Ctrl: opposite)\n\
                     Numpad 2/4/6/8: orbit 15 deg\n\
                     Numpad 9: flip, 5: ortho, .: centre\n\
                     Home: reset view",
                );
            }
        });
}

/// Light direction for the mesh: towards the sky texture's sun (world +Y,
/// spun by the sky's azimuth), raised a little so tops of balls catch light
/// even though the painted sun sits on the horizon.
fn sun_direction(sun_azimuth: f32) -> glam::Vec3 {
    glam::Quat::from_rotation_z(sun_azimuth) * glam::Vec3::new(0.0, 1.0, 0.6).normalize()
}

/// A rough eight-legged body: a root ball with coxa -> femur -> tibia legs,
/// four down each side, front legs pointing towards -Y (Blender front).
fn demo_spider() -> Mesh {
    use glam::{Quat, Vec3};
    let mut mesh = Mesh::new(Vec3::new(0.0, 0.0, 0.6), Quat::IDENTITY);
    let root = mesh.root();
    for side in [1.0f32, -1.0] {
        for k in 0..4 {
            // -60 deg (forward) to +60 deg (back) around the body.
            let a = (-60.0 + 40.0 * k as f32).to_radians();
            let out = Vec3::new(side * a.cos(), a.sin(), 0.0);
            let coxa = mesh.attach(root, Joint::new(Joint::rest_from_forward_up(out, Vec3::Z), 0.35, SwingLimit::FREE));
            // Femur rises (+X rotation tilts the bone towards its local +Z, i.e. up)...
            let femur = mesh.attach(coxa, Joint::new(Quat::from_rotation_x(0.7), 0.8, SwingLimit::FREE));
            // ...and the tibia comes back down to the ground.
            mesh.attach(femur, Joint::new(Quat::from_rotation_x(-1.9), 1.0, SwingLimit::FREE));
        }
    }
    mesh
}

fn draw_mesh_panel(ui: &imgui::Ui, show: &mut bool, style: &mut MeshStyle) {
    ui.window("Mesh")
        .position([10.0 * GUI_SCALE, 330.0 * GUI_SCALE], imgui::Condition::FirstUseEver)
        .size([300.0 * GUI_SCALE, 0.0], imgui::Condition::FirstUseEver)
        .build(|| {
            ui.checkbox("Show demo mesh", show);
            ui.slider("Root radius", 0.02, 0.6, &mut style.root_radius);
            ui.slider("Ball radius", 0.02, 0.3, &mut style.ball_radius);
            ui.slider("Foot radius", 0.02, 0.3, &mut style.foot_radius);
            ui.slider("Stick radius", 0.005, 0.2, &mut style.stick_radius);
            ui.color_edit3("Root", &mut style.root_color);
            ui.color_edit3("Joints", &mut style.ball_color);
            ui.color_edit3("Feet", &mut style.foot_color);
            ui.color_edit3("Sticks", &mut style.stick_color);
        });
}
