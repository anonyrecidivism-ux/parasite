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
    android:  AndroidApp,
    /// Whether the soft keyboard is currently requested (so we only toggle on change).
    kb_open:  bool,
    /// A tokio runtime kept alive for the whole session so transforms that need
    /// an async context have one (the GUI spawns work onto it).
    _rt:      tokio::runtime::Runtime,
}

impl App {
    fn new(android: AndroidApp) -> Self {
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

        Self { egui_ctx, shell, painter, active: None, android, kb_open: false, _rt: rt }
    }

    /// Feed the egui side the current safe-area insets (status bar / nav bar / IME)
    /// so the UI never draws under the system bars.
    fn update_insets(&self) {
        let Some(active) = self.active.as_ref() else { return };
        let cr = self.android.content_rect(); // physical px, area free of system UI
        let size = active.window.inner_size();
        let ppp = self.egui_ctx.pixels_per_point().max(0.5);
        // Only trust a sane, non-empty rect.
        if cr.right > cr.left && cr.bottom > cr.top
            && cr.right as u32 <= size.width && cr.bottom as u32 <= size.height
        {
            let top    = (cr.top.max(0) as f32) / ppp;
            let bottom = ((size.height as i32 - cr.bottom).max(0) as f32) / ppp;
            gui::set_insets(top, bottom);
        } else {
            gui::set_insets(0.0, 0.0);
        }
    }

    fn redraw(&mut self) {
        self.update_insets();
        let Some(active) = self.active.as_mut() else { return };
        let window = active.window.clone();

        let raw_input = active.state.take_egui_input(&window);
        let full = self.egui_ctx.clone().run(raw_input, |ctx| {
            self.shell.ui(ctx);
        });
        active.state.handle_platform_output(&window, full.platform_output.clone());

        // Bring the soft keyboard up/down to match what egui wants (egui-winit
        // does not do this for us on Android).
        let want_kb = self.egui_ctx.wants_keyboard_input();
        if want_kb != self.kb_open {
            if want_kb { self.android.show_soft_input(true); }
            else       { self.android.hide_soft_input(true); }
            self.kb_open = want_kb;
        }

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
        .with_android_app(app.clone())
        .build()
        .expect("event loop");

    let mut state = App::new(app);
    if let Err(e) = event_loop.run_app(&mut state) {
        log::error!("parasite: event loop ended: {e}");
    }
}
