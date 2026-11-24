//! Interactive window: free-fly camera over the generated scene with a live day cycle.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::camera::Camera;
use crate::capture::{instance, request_device};
use crate::daylight::{Daylight, DisplayClock};
use crate::renderer::{FrameInput, Renderer};
use crate::scene::Scene;

const WALK_SPEED: f32 = 9.0;
const FAST_MULTIPLIER: f32 = 5.0;
/// Displayed hours per real second while time runs.
const HOURS_PER_SECOND: f64 = 0.5;

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

struct App {
    scene: Scene,
    clock: DisplayClock,
    camera: Camera,
    gpu: Option<Gpu>,
    keys: HashSet<KeyCode>,
    started: Instant,
    last_frame: Instant,
    frame: u32,
    mouse_captured: bool,
    time_running: bool,
    error: Option<String>,
}

pub fn run(scene: Scene, camera: Camera) -> Result<(), String> {
    let event_loop =
        EventLoop::new().map_err(|error| format!("cannot open event loop: {error}"))?;
    let mut app = App {
        clock: DisplayClock::new(scene.day_fraction()),
        scene,
        camera,
        gpu: None,
        keys: HashSet::new(),
        started: Instant::now(),
        last_frame: Instant::now(),
        frame: 0,
        mouse_captured: false,
        time_running: false,
        error: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("event loop failed: {error}"))?;
    app.error.map_or(Ok(()), Err)
}

fn create_gpu(event_loop: &ActiveEventLoop, scene: &Scene) -> Result<Gpu, String> {
    let attributes = Window::default_attributes()
        .with_title("Deep Hearth")
        .with_inner_size(winit::dpi::LogicalSize::new(1600.0, 900.0));
    let window = Arc::new(
        event_loop
            .create_window(attributes)
            .map_err(|error| format!("cannot create window: {error}"))?,
    );
    let instance = instance();
    let surface = instance
        .create_surface(Arc::clone(&window))
        .map_err(|error| format!("cannot create surface: {error}"))?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
        apply_limit_buckets: false,
    }))
    .map_err(|error| format!("no GPU adapter for this window: {error}"))?;
    let (device, queue) = request_device(&adapter)?;
    let size = window.inner_size();
    let mut config = surface
        .get_default_config(&adapter, size.width.max(1), size.height.max(1))
        .ok_or("window surface is not supported by the adapter")?;
    let formats = surface.get_capabilities(&adapter).formats;
    if let Some(format) = formats.iter().copied().find(wgpu::TextureFormat::is_srgb) {
        config.format = format;
    }
    config.present_mode = wgpu::PresentMode::AutoVsync;
    surface.configure(&device, &config);
    let renderer = Renderer::new(
        device,
        queue,
        config.format,
        (config.width, config.height),
        scene,
    )?;
    Ok(Gpu {
        window,
        surface,
        config,
        renderer,
    })
}

impl App {
    fn set_mouse_capture(&mut self, captured: bool) {
        let Some(gpu) = &self.gpu else { return };
        let grabbed = !captured
            || gpu.window.set_cursor_grab(CursorGrabMode::Locked).is_ok()
            || gpu.window.set_cursor_grab(CursorGrabMode::Confined).is_ok();
        if !captured {
            let _ = gpu.window.set_cursor_grab(CursorGrabMode::None);
        }
        self.mouse_captured = captured && grabbed;
        gpu.window.set_cursor_visible(!self.mouse_captured);
    }

    fn key(&mut self, event: &KeyEvent, event_loop: &ActiveEventLoop) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        if event.state == ElementState::Released {
            self.keys.remove(&code);
            return;
        }
        self.keys.insert(code);
        if event.repeat {
            return;
        }
        match code {
            KeyCode::Escape if self.mouse_captured => self.set_mouse_capture(false),
            KeyCode::Escape => event_loop.exit(),
            KeyCode::BracketLeft => self.clock.shift_hours(-0.5),
            KeyCode::BracketRight => self.clock.shift_hours(0.5),
            KeyCode::KeyT => self.time_running = !self.time_running,
            _ => {}
        }
    }

    fn movement(&self) -> Vec3 {
        let (forward, right) = self.camera.ground_axes();
        let bindings = [
            (KeyCode::KeyW, forward),
            (KeyCode::KeyS, -forward),
            (KeyCode::KeyD, right),
            (KeyCode::KeyA, -right),
            (KeyCode::Space, Vec3::Y),
            (KeyCode::KeyC, -Vec3::Y),
        ];
        bindings
            .iter()
            .filter(|(key, _)| self.keys.contains(key))
            .map(|(_, direction)| *direction)
            .sum::<Vec3>()
            .normalize_or_zero()
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let elapsed = (now - self.last_frame).as_secs_f64();
        self.last_frame = now;
        let fast =
            self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
        let speed = WALK_SPEED * if fast { FAST_MULTIPLIER } else { 1.0 };
        self.camera.position += self.movement() * speed * elapsed.min(0.1) as f32;
        if self.time_running {
            self.clock.shift_hours(HOURS_PER_SECOND * elapsed);
        }
        let Some(gpu) = &mut self.gpu else { return };
        let texture = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                gpu.surface.configure(&gpu.renderer.device, &gpu.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return,
        };
        let daylight = Daylight::at(self.clock.day_fraction());
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        gpu.renderer.render(
            &view,
            &FrameInput {
                camera: &self.camera,
                daylight: &daylight,
                time_seconds: (now - self.started).as_secs_f32(),
                frame_index: self.frame,
            },
        );
        gpu.renderer.queue.present(texture);
        self.frame = self.frame.wrapping_add(1);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        match create_gpu(event_loop, &self.scene) {
            Ok(gpu) => {
                self.gpu = Some(gpu);
                self.last_frame = Instant::now();
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu
                    && size.width > 0
                    && size.height > 0
                {
                    gpu.config.width = size.width;
                    gpu.config.height = size.height;
                    gpu.surface.configure(&gpu.renderer.device, &gpu.config);
                    gpu.renderer.resize((size.width, size.height));
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.key(&event, event_loop),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.set_mouse_capture(true),
            WindowEvent::Focused(false) => {
                self.keys.clear();
                self.set_mouse_capture(false);
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.mouse_captured
        {
            self.camera.look(delta.0 as f32, delta.1 as f32);
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }
}
