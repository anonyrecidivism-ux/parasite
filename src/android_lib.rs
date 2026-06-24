//! Android entry point for parasite.
//!
//! The desktop build drives the GUI through `eframe`; on Android there is no
//! eframe, so we run egui ourselves on a `winit` event loop with a `wgpu`
//! backend. The whole GUI (`mod gui`) is reused verbatim — only the windowing
//! shell differs. The crate is compiled as a `cdylib` loaded by a NativeActivity
//! (see `[package.metadata.android]` in Cargo.toml).
//!
//! On any non-Android target this file compiles to an empty library.
#![cfg(target_os = "android")]

mod gui;

use std::num::NonZeroU32;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::android::activity::AndroidApp;
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::window::{Window, WindowId};

use egui::ViewportId;

/// Everything that only exists once the activity has a surface.
struct Active {
    window:  Arc<Window>,
    state:   egui_winit::State,
}

struct App {
    egui_ctx: egui::Context,
    shell:    gui::Shell,
    painter:  egui_wgpu::winit::Painter,
    active:   Option<Active>,
    /// A tokio runtime kept alive for the whole session so transforms that need
    /// an async context have one (the GUI spawns work onto it).
    _rt:      tokio::runtime::Runtime,
}

impl App {
    fn new() -> Self {
        let egui_ctx = egui::Context::default();
        let shell = gui::Shell::build(&egui_ctx);

        let painter = egui_wgpu::winit::Painter::new(
            egui_wgpu::WgpuConfiguration::default(),
            1,      // msaa
            None,   // depth
            false,  // transparent backbuffer
            false,  // dithering
        );

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        Self { egui_ctx, shell, painter, active: None, _rt: rt }
    }

    fn redraw(&mut self) {
        let Some(active) = self.active.as_mut() else { return };
        let window = active.window.clone();

        let raw_input = active.state.take_egui_input(&window);
        let full = self.egui_ctx.clone().run(raw_input, |ctx| {
            self.shell.ui(ctx);
        });
        active.state.handle_platform_output(&window, full.platform_output.clone());

        let primitives = self.egui_ctx.tessellate(full.shapes, full.pixels_per_point);
        self.painter.paint_and_update_textures(
            ViewportId::ROOT,
            full.pixels_per_point,
            [0.04, 0.035, 0.03, 1.0],
            &primitives,
            &full.textures_delta,
            false,
        );

        // Drive continuous animation/transform polling.
        window.request_redraw();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title("Parasite OSINT");
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));

        // Attach the surface to the wgpu painter.
        pollster::block_on(self.painter.set_window(ViewportId::ROOT, Some(window.clone())))
            .expect("set_window");

        if let (Some(w), Some(h)) = (
            NonZeroU32::new(window.inner_size().width),
            NonZeroU32::new(window.inner_size().height),
        ) {
            self.painter.on_window_resized(ViewportId::ROOT, w, h);
        }

        let state = egui_winit::State::new(
            self.egui_ctx.clone(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            self.painter.max_texture_side(),
        );

        self.active = Some(Active { window, state });
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Drop the surface; a fresh one is created on the next resume.
        pollster::block_on(self.painter.set_window(ViewportId::ROOT, None)).ok();
        self.active = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(active) = self.active.as_mut() else { return };
        let window = active.window.clone();

        // Let egui consume the event first.
        let response = active.state.on_window_event(&window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    self.painter.on_window_resized(ViewportId::ROOT, w, h);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {
                if response.repaint {
                    window.request_redraw();
                }
            }
        }
    }
}

/// NativeActivity calls this symbol. Builds the winit event loop bound to the
/// Android activity and runs egui until the activity is destroyed.
#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    log::info!("parasite: android_main starting");

    let event_loop = EventLoop::builder()
        .with_android_app(app)
        .build()
        .expect("event loop");

    let mut state = App::new();
    if let Err(e) = event_loop.run_app(&mut state) {
        log::error!("parasite: event loop ended: {e}");
    }
}
