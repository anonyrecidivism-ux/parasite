//! parasite — an open-source, graph-based OSINT & web-reconnaissance toolkit.
//! A free alternative to Maltego: drop entities on a canvas and expand them with
//! transforms. Ships with a second "Operations" workspace that drives the
//! `parasite` recon engine.

mod ai;
mod app;
mod browser;
mod canvas;
mod cases;
mod dossier;
mod engine;
mod export;
mod geoint;
mod i18n;
mod install;
mod keys;
mod lisp;
mod logo;
mod model;
mod monitor;
mod mtgx;
mod net;
mod pivots;
mod settings;
mod theme;
mod toolbox;
mod transforms;
mod watch;

use eframe::egui::{self, Color32, FontFamily, FontId, Margin, RichText, Rounding, Stroke};
use settings::Settings;
use theme::*;

#[derive(Clone, Copy, PartialEq)]
enum AppMode { Graph, Geo, Monitor, Dossier, Cases, Watch, Toolbox, Browser }

struct Shell {
    mode:          AppMode,
    graph:         app::GraphPanel,
    geo:           geoint::GeoPanel,
    monitor:       monitor::MonitorPanel,
    dossier:       dossier::DossierPanel,
    cases:         cases::CasesPanel,
    watch:         watch::WatchPanel,
    toolbox:       toolbox::ToolboxPanel,
    browser:       browser::BrowserPanel,
    embed:         Embed,
    settings:      Settings,
    show_settings: bool,
}

impl Shell {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        theme::apply_font(&cc.egui_ctx, &settings.font_path);
        settings.apply(&cc.egui_ctx);
        Self {
            mode: AppMode::Graph,
            graph: app::GraphPanel::new(),
            geo: geoint::GeoPanel::new(),
            monitor: monitor::MonitorPanel::new(),
            dossier: dossier::DossierPanel::new(),
            cases: cases::CasesPanel::new(),
            watch: watch::WatchPanel::new(),
            toolbox: toolbox::ToolboxPanel::new(),
            browser: browser::BrowserPanel::new(),
            embed: Embed::new(),
            show_settings: false,
            settings,
        }
    }
}

/// Open a URL in ParasiteGoogle. On Hyprland the page loads in the embedded
/// overlay (the Shell switches to the ParasiteGoogle tab and shows the browser);
/// elsewhere it opens a standalone window. The browser only ever appears as a
/// result of an explicit open like this — never on its own.
pub fn app_open(input: &str) {
    // resolve a raw query to a search URL on the chosen engine, and enforce the
    // "block insecure HTTP" policy (returns None → blocked, do nothing).
    let Some(url) = net::resolve_nav(input) else { return };
    nav_write(&url);
    if let Ok(mut g) = pending_open().lock() { *g = Some(url.clone()); }
    if !hypr_available() { launch_browser(&url); }
}

/// A URL waiting to be opened in the overlay (set by `app_open`, consumed by the Shell).
fn pending_open() -> &'static std::sync::Mutex<Option<String>> {
    static P: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();
    P.get_or_init(|| std::sync::Mutex::new(None))
}

/// Path of the parasite↔ParasiteGoogle navigation control file (mirrors the one
/// the `parasitegoogle` binary watches).
fn nav_path() -> std::path::PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
    dir.join("parasitegoogle.nav")
}

/// Write a navigation request; the token forces a re-navigation even to the same URL.
fn nav_write(url: &str) {
    let token = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos()).unwrap_or(0);
    let _ = std::fs::write(nav_path(), format!("{token} {url}"));
}

/// Is the Hyprland compositor available (so we can overlay the browser)?
fn hypr_available() -> bool {
    static A: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *A.get_or_init(|| std::process::Command::new("hyprctl").arg("version")
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .status().map(|s| s.success()).unwrap_or(false))
}

/// Spawn the standalone `parasitegoogle` browser binary on `url`. Tries the copy
/// sitting next to the current executable first, then `$PATH`, and finally falls
/// back to the OS browser. Always non-blocking and detached, so a slow page can
/// never freeze the parasite window.
pub fn launch_browser(url: &str) {
    let sibling = std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|d| d.join("parasitegoogle")));
    if let Some(path) = sibling {
        if path.exists() && spawn_detached(path.as_os_str(), url) { return; }
    }
    if spawn_detached(std::ffi::OsStr::new("parasitegoogle"), url) { return; }
    system_open(url);
}

fn spawn_detached(program: &std::ffi::OsStr, url: &str) -> bool {
    std::process::Command::new(program)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Hand a URL to the operating-system browser (best-effort, multi-fallback).
pub fn system_open(url: &str) {
    let url = url.to_string();
    #[cfg(target_os = "linux")]
    {
        for (cmd, args) in [("xdg-open", vec![url.as_str()]),
                            ("gio", vec!["open", url.as_str()]),
                            ("firefox", vec![url.as_str()]),
                            ("chromium", vec![url.as_str()]),
                            ("google-chrome", vec![url.as_str()])] {
            if std::process::Command::new(cmd).args(&args)
                .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                .spawn().is_ok() { return; }
        }
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", &url]).spawn();
}

// ── ParasiteGoogle overlay (real WebKit window positioned over the panel) ─────
//
// A live web engine can't share egui's GL/winit surface, so the real browser is
// its own window that the Hyprland compositor floats *exactly over* the browser
// panel and resizes/moves to follow it. From the user's seat it looks embedded.
/// State shared between the UI thread and the overlay worker thread. The UI only
/// ever writes cheap fields here (never calls `hyprctl`), so it can never block.
#[derive(Default)]
struct EmbedShared {
    show:      bool,                       // overlay should be visible & tracking
    open_seq:  u64,                        // bump to (re)spawn the browser
    panel:     Option<(f32, f32, f32, f32)>, // browser-panel rect in egui points
    surf:      (f32, f32),                 // egui surface size in points
    theme:     [String; 7],                // bg,bar,input,accent,text,textsec,border (hex)
    closed:    bool,                       // worker → UI: the window was closed
    quit:      bool,
}

/// Snapshot the active palette into the 7 colours the browser chrome uses.
fn browser_theme() -> [String; 7] {
    [theme::hex(bg_app()), theme::hex(bg_panel()), theme::hex(bg_input()),
     theme::hex(accent()), theme::hex(text_pri()), theme::hex(text_sec()),
     theme::hex(border())]
}

/// The overlay manager. All compositor I/O happens on a dedicated worker thread,
/// so the egui UI thread is never stalled (that stall was the "not responding").
struct Embed {
    avail:  bool,
    active: bool,                          // UI-side: user opened a page
    shared: std::sync::Arc<std::sync::Mutex<EmbedShared>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Embed {
    fn new() -> Self {
        let avail = hypr_available();
        let shared = std::sync::Arc::new(std::sync::Mutex::new(EmbedShared::default()));
        let worker = if avail {
            let sh = shared.clone();
            Some(std::thread::spawn(move || embed_worker(sh)))
        } else { None };
        Self { avail, active: false, shared, worker }
    }

    /// User explicitly opened a page → ask the worker to (re)spawn and show it.
    fn open(&mut self) {
        if !self.avail { return; }
        self.active = true;
        let mut s = self.shared.lock().unwrap();
        s.open_seq += 1;
        s.closed = false;
    }

    /// Called every frame with the current desired visibility + panel geometry.
    /// Pure data hand-off — no blocking. Also notices when the user closed the
    /// browser window (so we drop back to the launcher without respawning).
    fn frame(&mut self, show: bool, panel: Option<egui::Rect>, surf: egui::Vec2) {
        if !self.avail { return; }
        let theme = browser_theme();  // computed on the UI thread (thread-local palette)
        let mut s = self.shared.lock().unwrap();
        if s.closed { self.active = false; s.closed = false; }
        s.show = show && self.active;
        s.panel = panel.map(|r| (r.min.x, r.min.y, r.width(), r.height()));
        s.surf = (surf.x, surf.y);
        s.theme = theme;
    }

    fn shutdown(&mut self) {
        if let Ok(mut s) = self.shared.lock() { s.quit = true; }
        if let Some(h) = self.worker.take() { let _ = h.join(); }
    }
}

impl Drop for Embed {
    fn drop(&mut self) { self.shutdown(); }
}

/// Run a Lua dispatch against Hyprland (0.5x replaced the text protocol with Lua).
fn hypr_lua(code: &str) {
    let _ = std::process::Command::new("hyprctl").args(["dispatch", code])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
}

/// Geometry of the parasite window, straight from the compositor — Wayland won't
/// tell a client its own absolute position, so we ask Hyprland. (x, y, w, h).
fn parasite_geom() -> Option<(i32, i32, i32, i32)> {
    let out = std::process::Command::new("hyprctl").args(["clients", "-j"]).output().ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    for c in v.as_array()? {
        if c.get("class").and_then(|x| x.as_str()) != Some("parasite-gui") { continue; }
        // skip when parasite isn't actually on screen (minimised / other workspace)
        if c.get("mapped").and_then(|x| x.as_bool()) == Some(false) { return None; }
        let at = c.get("at")?.as_array()?;
        let size = c.get("size")?.as_array()?;
        return Some((at[0].as_i64()? as i32, at[1].as_i64()? as i32,
                     size[0].as_i64()? as i32, size[1].as_i64()? as i32));
    }
    None
}

fn spawn_browser(theme: &[String; 7]) -> Option<std::process::Child> {
    let _ = std::process::Command::new("pkill").args(["-x", "parasitegoogle"]).status();
    let prog = std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|d| d.join("parasitegoogle")))
        .filter(|p| p.exists());
    let mut cmd = match prog {
        Some(p) => std::process::Command::new(p),
        None => std::process::Command::new("parasitegoogle"),
    };
    // hand the active parasite theme to the browser chrome
    for (k, v) in ["PG_BG", "PG_BAR", "PG_INPUT", "PG_ACCENT", "PG_TEXT", "PG_TEXTSEC", "PG_BORDER"]
        .iter().zip(theme.iter()) {
        if !v.is_empty() { cmd.env(k, v); }
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok()
}

/// The worker thread: owns the browser process and does every `hyprctl` call, so
/// the UI thread never waits on the compositor. Polls ~every 120 ms.
fn embed_worker(shared: std::sync::Arc<std::sync::Mutex<EmbedShared>>) {
    let mut child: Option<std::process::Child> = None;
    let mut last_open = 0u64;
    let mut last_geom: Option<(i32, i32, i32, i32)> = None;
    let mut hidden = true;

    loop {
        let (show, open_seq, panel, surf, theme, quit) = {
            let s = shared.lock().unwrap();
            (s.show, s.open_seq, s.panel, s.surf, s.theme.clone(), s.quit)
        };
        if quit {
            if let Some(mut c) = child.take() { let _ = c.kill(); }
            let _ = std::process::Command::new("pkill").args(["-x", "parasitegoogle"]).status();
            return;
        }

        // notice the user closing the browser window
        if let Some(c) = &mut child {
            if matches!(c.try_wait(), Ok(Some(_))) {
                child = None;
                shared.lock().unwrap().closed = true;
                last_geom = None;
            }
        }

        // (re)spawn on an explicit open request
        if open_seq != last_open {
            last_open = open_seq;
            if child.is_none() { child = spawn_browser(&theme); last_geom = None; hidden = false; }
        }

        if show && child.is_some() {
            if let (Some(p), Some(win)) = (panel, parasite_geom()) {
                let (wx, wy, ww, wh) = win;
                let fx = if surf.0 > 1.0 { ww as f32 / surf.0 } else { 1.0 };
                let fy = if surf.1 > 1.0 { wh as f32 / surf.1 } else { 1.0 };
                // clamp strictly inside the parasite window so it can't spill onto
                // neighbouring windows
                let mut x = wx + (p.0 * fx).round() as i32;
                let mut y = wy + (p.1 * fy).round() as i32;
                let mut w = (p.2 * fx).round() as i32;
                let mut h = (p.3 * fy).round() as i32;
                x = x.clamp(wx, wx + ww);
                y = y.clamp(wy, wy + wh);
                w = w.clamp(1, wx + ww - x);
                h = h.clamp(1, wy + wh - y);
                let g = (x, y, w, h);
                if last_geom != Some(g) {
                    hypr_lua(&format!(
                        "(function() local g; for _,q in ipairs(hl.get_windows()) do if q.class==\"parasitegoogle\" then g=q end end; \
                         if not g then return nil end; \
                         if not g.floating then hl.dispatch(hl.dsp.window.float({{window=g}})) end; \
                         pcall(function() hl.dispatch(hl.dsp.window.set_prop({{window=g, prop=\"rounding\", value=0}})) end); \
                         hl.dispatch(hl.dsp.window.resize({{window=g, x={w}, y={h}}})); \
                         return hl.dsp.window.move({{window=g, x={x}, y={y}}}) end)()"));
                    last_geom = Some(g);
                    hidden = false;
                }
            }
        } else if !hidden && child.is_some() {
            hypr_lua("(function() local g; for _,q in ipairs(hl.get_windows()) do if q.class==\"parasitegoogle\" then g=q end end; \
                      if not g then return nil end; return hl.dsp.window.move({window=g, x=200000, y=200000}) end)()");
            hidden = true;
            last_geom = None;
        }

        std::thread::sleep(std::time::Duration::from_millis(120));
    }
}

impl eframe::App for Shell {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.embed.shutdown();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // a link/search was triggered → switch to the browser tab and open it
        if let Some(_url) = pending_open().lock().ok().and_then(|mut g| g.take()) {
            if self.embed.avail {
                self.mode = AppMode::Browser;
                self.embed.open();
            }
        }

        egui::TopBottomPanel::top("shell_tabs")
            .frame(egui::Frame::none().fill(bg_sidebar())
                .inner_margin(Margin::symmetric(12.0, 6.0))
                .stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    logo::widget(ui, 12.0);
                    ui.add_space(5.0);
                    ui.label(RichText::new("parasite").color(text_pri()).strong().size(15.0));
                    ui.add_space(14.0);

                    mode_tab(ui, &mut self.mode, AppMode::Graph, &format!("◇ {}", i18n::tr("tab.graph")));
                    mode_tab(ui, &mut self.mode, AppMode::Geo, &format!("◎ {}", i18n::tr("tab.geo")));
                    mode_tab(ui, &mut self.mode, AppMode::Monitor, &format!("◷ {}", i18n::tr("tab.monitor")));
                    mode_tab(ui, &mut self.mode, AppMode::Dossier, &format!("▤ {}", i18n::tr("tab.dossier")));
                    mode_tab(ui, &mut self.mode, AppMode::Cases, &format!("▦ {}", i18n::tr("tab.cases")));
                    mode_tab(ui, &mut self.mode, AppMode::Watch, &format!("⊚ {}", i18n::tr("tab.watch")));
                    mode_tab(ui, &mut self.mode, AppMode::Toolbox, &format!("⊞ {}", i18n::tr("tab.toolbox")));
                    mode_tab(ui, &mut self.mode, AppMode::Browser, "⊕ ParasiteGoogle");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let gear = ui.add(egui::Button::new(
                            RichText::new(format!("⚙ {}", i18n::tr("shell.settings"))).color(text_sec()).size(12.0))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, border()))
                            .rounding(Rounding::same(5.0)));
                        if gear.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                        if gear.clicked() { self.show_settings = !self.show_settings; }
                        ui.add_space(8.0);
                        if ui.add(egui::Button::new(RichText::new(format!("? {}", i18n::tr("shell.help"))).color(text_sec()).size(12.0))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, border()))
                            .rounding(Rounding::same(5.0))).clicked()
                        {
                            self.settings.welcomed = false;
                        }
                    });
                });
            });

        match self.mode {
            AppMode::Graph   => self.graph.ui(ctx),
            AppMode::Geo     => self.geo.ui(ctx),
            AppMode::Monitor => self.monitor.ui(ctx),
            AppMode::Dossier => self.dossier.ui(ctx),
            AppMode::Cases   => self.cases.ui(ctx),
            AppMode::Watch   => self.watch.ui(ctx),
            AppMode::Toolbox => self.toolbox.ui(ctx),
            AppMode::Browser => self.browser.ui(ctx),
        }

        // dossier → graph sync (runs regardless of the active tab)
        if let Some(seed) = self.dossier.take_seed() {
            self.graph.ingest_dossier(seed);
        }
        // cases ⇄ graph
        if let Some(name) = self.cases.take_save() {
            let data = self.graph.export_case();
            self.cases.write_case(&name, &data);
        }
        if let Some(data) = self.cases.take_open() {
            self.graph.import_case(data);
            self.mode = AppMode::Graph;
        }
        // watch → graph
        if let Some((kind, value)) = self.watch.take_graph() {
            self.graph.add_node(kind, value);
        }

        // don't draw settings/welcome over a video recording
        if !self.graph.recording() {
            self.settings_window(ctx);
            if !self.settings.welcomed {
                self.welcome_window(ctx);
            }
        }

        // Hand the overlay worker the desired visibility + panel rect. This is a
        // pure non-blocking data hand-off — all hyprctl I/O happens off-thread, so
        // the UI can never appear "not responding".
        let on_tab = self.mode == AppMode::Browser
            && !self.show_settings && self.settings.welcomed && !self.graph.recording();
        self.embed.frame(on_tab, self.browser.last_rect, ctx.screen_rect().size());
        if on_tab && self.embed.active {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

fn mode_tab(ui: &mut egui::Ui, mode: &mut AppMode, this: AppMode, label: &str) {
    let active = *mode == this;
    let (fill, txt) = if active { (bg_item_sel(), text_pri()) } else { (Color32::TRANSPARENT, text_sec()) };
    let r = ui.add(egui::Button::new(RichText::new(label).color(txt).size(12.5).strong())
        .fill(fill)
        .stroke(if active { Stroke::new(1.0, accent_dark()) } else { Stroke::NONE })
        .rounding(Rounding::same(5.0)));
    if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    if r.clicked() { *mode = this; }
    ui.add_space(4.0);
}

impl Shell {
    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings { return; }
        let mut open = true;
        let mut changed = false;
        egui::Window::new(RichText::new("⚙  Settings & Customization").color(text_pri()).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(390.0)
            .anchor(egui::Align2::RIGHT_TOP, [-16.0, 52.0])
            .frame(egui::Frame::window(&ctx.style()).fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(ctx.screen_rect().height() - 150.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new(i18n::tr("set.language")).color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                egui::ComboBox::from_id_salt("lang_combo")
                    .selected_text(RichText::new(self.settings.lang.label()).color(text_pri()))
                    .width(330.0)
                    .show_ui(ui, |ui| {
                        for l in i18n::Lang::ALL {
                            if ui.selectable_value(&mut self.settings.lang, l,
                                RichText::new(l.label()).color(text_pri())).clicked() { changed = true; }
                        }
                    });

                ui.label(RichText::new("INTERFACE DESIGN").color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                egui::ComboBox::from_id_salt("design_combo")
                    .selected_text(RichText::new(self.settings.design.label()).color(text_pri()))
                    .width(330.0)
                    .show_ui(ui, |ui| {
                        for d in theme::Design::ALL {
                            if ui.selectable_value(&mut self.settings.design, d,
                                RichText::new(d.label()).color(text_pri())).clicked() {
                                changed = true;
                                // jump to the matching palette for a cohesive one-click look
                                if d == theme::Design::Cupertino { self.settings.theme = "Cupertino".into(); }
                                self.settings.accent = None;
                            }
                        }
                    });
                ui.label(RichText::new("Cupertino = clean & light · Retro Unix = old-Linux Motif/CDE grey · Stock = original. Theme below is independent.")
                    .color(text_mut()).size(10.0));

                ui.add_space(10.0);
                ui.label(RichText::new(i18n::tr("set.interface")).color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                egui::ComboBox::from_id_salt("variant_combo")
                    .selected_text(RichText::new(self.settings.variant.label()).color(text_pri()))
                    .width(330.0)
                    .show_ui(ui, |ui| {
                        for v in theme::UiVariant::ALL {
                            if ui.selectable_value(&mut self.settings.variant, v,
                                RichText::new(v.label()).color(text_pri())).clicked() { changed = true; }
                        }
                    });

                let palette_locked = self.settings.design.forces_palette().is_some();
                ui.add_space(10.0);
                if palette_locked {
                    ui.label(RichText::new("⊘ this design uses its own fixed palette — theme & accent don't apply")
                        .color(c_warn()).size(10.5).italics());
                }
                ui.add_enabled_ui(!palette_locked, |ui| {
                ui.label(RichText::new(i18n::tr("set.theme")).color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                egui::ComboBox::from_id_salt("theme_combo")
                    .selected_text(RichText::new(&self.settings.theme).color(text_pri()))
                    .width(330.0)
                    .show_ui(ui, |ui| {
                        for (name, _) in theme::THEMES {
                            if ui.selectable_value(&mut self.settings.theme, name.to_string(),
                                RichText::new(*name).color(text_pri())).clicked()
                            {
                                changed = true;
                            }
                        }
                    });

                ui.add_space(10.0);
                ui.label(RichText::new(i18n::tr("set.accent")).color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let mut rgb = self.settings.accent.unwrap_or_else(|| {
                        let a = theme::theme_by_name(&self.settings.theme).accent;
                        [a.r(), a.g(), a.b()]
                    });
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        self.settings.accent = Some(rgb);
                        changed = true;
                    }
                    ui.add_space(8.0);
                    if ui.button(RichText::new("reset").color(text_sec()).size(11.0)).clicked() {
                        self.settings.accent = None;
                        changed = true;
                    }
                    // quick swatches
                    for sw in [[217u8,119,87],[96,165,250],[74,222,128],[189,147,249],[235,140,90],[136,192,208]] {
                        let (rect, r) = ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::click());
                        ui.painter().rect_filled(rect, Rounding::same(4.0), Color32::from_rgb(sw[0],sw[1],sw[2]));
                        if r.clicked() { self.settings.accent = Some(sw); changed = true; }
                        if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                    }
                });
                });

                ui.add_space(12.0);
                ui.label(RichText::new(i18n::tr("set.canvas")).color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                changed |= ui.add(egui::Slider::new(&mut self.settings.node_radius, 12.0..=40.0)
                    .text("node size")).changed();
                changed |= ui.add(egui::Slider::new(&mut self.settings.font_scale, 0.8..=1.5)
                    .text("font scale")).changed();
                ui.horizontal(|ui| {
                    ui.add_sized([70.0, 18.0], egui::Label::new(RichText::new("UI font").color(text_sec()).size(11.0)));
                    ui.add(egui::TextEdit::singleline(&mut self.settings.font_path)
                        .desired_width(180.0).hint_text("path to a .ttf/.otf  (empty = default)")
                        .font(FontId::new(11.0, FontFamily::Monospace)));
                    if ui.button(RichText::new("↻ apply").color(accent()).size(11.0)).clicked() {
                        theme::apply_font(ctx, &self.settings.font_path);
                        self.settings.save();
                    }
                });
                ui.label(RichText::new("custom font is tried first; point it at an emoji font for coloured icons")
                    .color(text_mut()).size(10.0));

                // node shape/style are driven by the Cupertino/Maltego designs, so hide them there
                let custom_nodes = self.settings.design == theme::Design::Stock;
                if custom_nodes {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("node style").color(text_pri()).size(12.0));
                        egui::ComboBox::from_id_salt("style_combo")
                            .selected_text(RichText::new(self.settings.node_style.label()).color(text_pri()))
                            .show_ui(ui, |ui| {
                                for s in theme::NodeStyle::ALL {
                                    if ui.selectable_value(&mut self.settings.node_style, s,
                                        RichText::new(s.label()).color(text_pri())).clicked() { changed = true; }
                                }
                            });
                    });
                }
                ui.horizontal(|ui| {
                    if custom_nodes {
                        ui.label(RichText::new("node shape").color(text_pri()).size(12.0));
                        egui::ComboBox::from_id_salt("shape_combo")
                            .selected_text(RichText::new(self.settings.node_shape.label()).color(text_pri()))
                            .show_ui(ui, |ui| {
                                for s in theme::NodeShape::ALL {
                                    if ui.selectable_value(&mut self.settings.node_shape, s,
                                        RichText::new(s.label()).color(text_pri())).clicked() { changed = true; }
                                }
                            });
                        ui.add_space(8.0);
                    }
                    ui.label(RichText::new("background").color(text_pri()).size(12.0));
                    egui::ComboBox::from_id_salt("bg_combo")
                        .selected_text(RichText::new(self.settings.bg_style.label()).color(text_pri()))
                        .show_ui(ui, |ui| {
                            for s in theme::BgStyle::ALL {
                                if ui.selectable_value(&mut self.settings.bg_style, s,
                                    RichText::new(s.label()).color(text_pri())).clicked() { changed = true; }
                            }
                        });
                });

                changed |= ui.add(egui::Slider::new(&mut self.settings.edge_width, 0.5..=4.0)
                    .text("edge thickness")).changed();
                changed |= ui.add(egui::Slider::new(&mut self.settings.label_size, 8.0..=18.0)
                    .text("label size")).changed();
                changed |= ui.checkbox(&mut self.settings.edge_curved,
                    RichText::new("curved edges").color(text_pri())).changed();
                changed |= ui.checkbox(&mut self.settings.node_labels,
                    RichText::new("node labels").color(text_pri())).changed();
                changed |= ui.checkbox(&mut self.settings.show_icons,
                    RichText::new("node icons").color(text_pri())).changed();
                changed |= ui.checkbox(&mut self.settings.color_clusters,
                    RichText::new("colour nodes by cluster").color(text_pri())).changed();
                changed |= ui.checkbox(&mut self.settings.glow,
                    RichText::new("node glow").color(text_pri())).changed();
                changed |= ui.checkbox(&mut self.settings.animations,
                    RichText::new("animations").color(text_pri())).changed();
                changed |= ui.add(egui::Slider::new(&mut self.settings.anim_speed, 0.25..=3.0)
                    .text("animation speed")).changed();

                ui.add_space(8.0);
                ui.label(RichText::new("GEOINT MAP").color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                changed |= ui.add(egui::Slider::new(&mut self.settings.map_sensitivity, 0.2..=2.0)
                    .text("zoom sensitivity")).changed();
                changed |= ui.checkbox(&mut self.settings.show_grid,
                    RichText::new("show background pattern").color(text_pri())).changed();
                changed |= ui.checkbox(&mut self.settings.edge_labels,
                    RichText::new("edge labels").color(text_pri())).changed();

                ui.add_space(12.0);
                ui.label(RichText::new("NETWORK").color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_sized([90.0, 18.0], egui::Label::new(RichText::new("proxy").color(text_sec()).size(11.5)));
                    changed |= ui.add(egui::TextEdit::singleline(&mut self.settings.proxy)
                        .desired_width(220.0).hint_text("socks5://127.0.0.1:9050 or http://host:port")
                        .font(FontId::new(11.5, FontFamily::Monospace))).changed();
                });
                ui.label(RichText::new("routes ALL requests through this proxy/VPN — useful if sites block your region")
                    .color(text_mut()).size(10.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("search engine").color(text_sec()).size(11.5));
                    for (id, label) in [("duckduckgo", "DuckDuckGo"), ("google", "Google")] {
                        if ui.selectable_label(self.settings.search_engine == id,
                            RichText::new(label).color(text_pri()).size(12.0)).clicked()
                        { self.settings.search_engine = id.to_string(); changed = true; }
                    }
                });
                changed |= ui.checkbox(&mut self.settings.block_http,
                    RichText::new("block insecure HTTP sites").color(text_pri())).changed();

                ui.add_space(12.0);
                ui.collapsing(RichText::new("❖ API KEYS (optional)").color(text_mut()).size(10.0).strong(), |ui| {
                    ui.label(RichText::new("Enable extra integrations. Keys stay local in your settings file.")
                        .color(text_mut()).size(10.5));
                    ui.add_space(4.0);
                    let field = |ui: &mut egui::Ui, label: &str, val: &mut String, changed: &mut bool| {
                        ui.horizontal(|ui| {
                            ui.add_sized([108.0, 18.0], egui::Label::new(
                                RichText::new(label).color(text_sec()).size(11.0)));
                            *changed |= ui.add(egui::TextEdit::singleline(val)
                                .desired_width(190.0).password(true)
                                .font(FontId::new(11.0, FontFamily::Monospace))).changed();
                        });
                    };
                    field(ui, "Shodan",     &mut self.settings.api.shodan,     &mut changed);
                    field(ui, "VirusTotal", &mut self.settings.api.virustotal, &mut changed);
                    field(ui, "HaveIBeenPwned", &mut self.settings.api.hibp,   &mut changed);
                    field(ui, "Hunter.io",  &mut self.settings.api.hunter,     &mut changed);
                    field(ui, "AbuseIPDB",  &mut self.settings.api.abuseipdb,  &mut changed);
                    ui.add_space(4.0);
                    ui.collapsing(RichText::new("More providers").color(text_sec()).size(10.5).strong(), |ui| {
                        field(ui, "SecurityTrails", &mut self.settings.api.securitytrails, &mut changed);
                        field(ui, "GreyNoise",  &mut self.settings.api.greynoise,  &mut changed);
                        field(ui, "IPinfo",     &mut self.settings.api.ipinfo,     &mut changed);
                        field(ui, "BinaryEdge", &mut self.settings.api.binaryedge, &mut changed);
                        field(ui, "FullHunt",   &mut self.settings.api.fullhunt,   &mut changed);
                        field(ui, "LeakIX",     &mut self.settings.api.leakix,     &mut changed);
                        field(ui, "IntelX",     &mut self.settings.api.intelx,     &mut changed);
                        field(ui, "urlscan.io", &mut self.settings.api.urlscan,    &mut changed);
                        field(ui, "ZoomEye",    &mut self.settings.api.zoomeye,    &mut changed);
                        field(ui, "BuiltWith",  &mut self.settings.api.builtwith,  &mut changed);
                        field(ui, "NumVerify",  &mut self.settings.api.numverify,  &mut changed);
                        field(ui, "WhoisXML",   &mut self.settings.api.whoisxml,   &mut changed);
                        field(ui, "Censys ID",  &mut self.settings.api.censys_id,  &mut changed);
                        field(ui, "Censys secret", &mut self.settings.api.censys_secret, &mut changed);
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new("AI — natural-language graph building").color(text_mut()).size(10.0).strong());
                    // provider + model selector
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("provider").color(text_sec()).size(11.0));
                        let cur = if self.settings.api.ai_provider.is_empty() { "auto".to_string() }
                                  else { self.settings.api.ai_provider.clone() };
                        egui::ComboBox::from_id_salt("ai_provider")
                            .selected_text(RichText::new(cur).color(text_pri()).size(11.5))
                            .show_ui(ui, |ui| {
                                if ui.selectable_value(&mut self.settings.api.ai_provider, String::new(),
                                    "auto (first with a key)").clicked() { changed = true; }
                                for (id, name, _) in ai::PROVIDERS {
                                    if ui.selectable_value(&mut self.settings.api.ai_provider, id.to_string(),
                                        *name).clicked() { changed = true; }
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.add_sized([90.0, 18.0], egui::Label::new(
                            RichText::new("model").color(text_sec()).size(11.0)));
                        let def = ai::PROVIDERS.iter().find(|p| p.0 == self.settings.api.ai_provider)
                            .map(|p| p.2).unwrap_or("provider default");
                        changed |= ui.add(egui::TextEdit::singleline(&mut self.settings.api.ai_model)
                            .desired_width(200.0).hint_text(def)
                            .font(FontId::new(11.5, FontFamily::Monospace))).changed();
                    });
                    field(ui, "Claude (Anthropic)", &mut self.settings.api.claude, &mut changed);
                    field(ui, "Gemini (Google)",    &mut self.settings.api.gemini, &mut changed);
                    field(ui, "OpenAI",     &mut self.settings.api.openai,     &mut changed);
                    field(ui, "Mistral",    &mut self.settings.api.mistral,    &mut changed);
                    field(ui, "DeepSeek",   &mut self.settings.api.deepseek,   &mut changed);
                    field(ui, "Groq",       &mut self.settings.api.groq,       &mut changed);
                    field(ui, "OpenRouter", &mut self.settings.api.openrouter, &mut changed);
                    field(ui, "xAI (Grok)", &mut self.settings.api.xai,        &mut changed);
                });

                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("↺ Reset to defaults").color(text_sec())).clicked() {
                        self.settings = Settings { welcomed: true, ..Settings::default() };
                        changed = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("▣ Reinstall menu entry").color(text_sec()).size(11.0)).clicked() {
                            let _ = install::install(true);
                        }
                    });
                });
                });
            });

        if changed {
            self.settings.apply(ctx);
            self.settings.save();
        }
        if !open { self.show_settings = false; }
    }

    fn welcome_window(&mut self, ctx: &egui::Context) {
        let mut dismissed = false;
        let screen = ctx.screen_rect();

        egui::Area::new(egui::Id::new("welcome"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::Pos2::ZERO)
            .show(ctx, |ui| {
                // full-screen dim that also blocks clicks reaching the app behind
                let block = ui.allocate_rect(screen, egui::Sense::click_and_drag());
                ui.painter().rect_filled(screen, Rounding::ZERO,
                    Color32::from_rgba_unmultiplied(8, 7, 6, 205));

                // centred card
                let cw = 560.0_f32;
                let card = egui::Rect::from_min_size(
                    egui::pos2(screen.center().x - cw / 2.0, screen.center().y - 250.0),
                    egui::Vec2::new(cw, 500.0));
                let p = ui.painter();
                p.rect_filled(card, Rounding::same(18.0), bg_panel());
                p.rect_stroke(card, Rounding::same(18.0), Stroke::new(1.0, border()));
                // accent header strip
                let strip = egui::Rect::from_min_size(card.min, egui::Vec2::new(cw, 5.0));
                p.rect_filled(strip, Rounding { nw: 18.0, ne: 18.0, sw: 0.0, se: 0.0 }, accent());

                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(card.shrink2(egui::Vec2::new(40.0, 30.0))), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        logo::widget(ui, 40.0);
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            let w = ui.available_width();
                            ui.add_space((w - 200.0).max(0.0) / 2.0);
                            ui.label(RichText::new("parasite").color(text_pri()).strong().size(30.0));
                            ui.label(RichText::new(".osint").color(accent()).strong().size(30.0));
                        });
                        ui.add_space(2.0);
                        ui.label(RichText::new(i18n::tr("wel.tagline"))
                            .color(text_sec()).size(12.5));
                        ui.add_space(20.0);
                    });

                    // three mode cards
                    let modes = [
                        ("◇", "Graph", "entities + 110 transforms", accent()),
                        ("◎", "GEOINT", "maps, EXIF GPS, satellite", c_info()),
                        ("◷", "Monitor", "live crypto transactions", c_ok()),
                    ];
                    ui.columns(3, |cols| {
                        for (i, (icon, name, desc, col)) in modes.iter().enumerate() {
                            cols[i].vertical_centered(|ui| {
                                egui::Frame::none().fill(bg_item_hov()).rounding(Rounding::same(8.0))
                                    .inner_margin(Margin::symmetric(8.0, 12.0)).stroke(Stroke::new(1.0, border()))
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.vertical_centered(|ui| {
                                            ui.label(RichText::new(*icon).color(*col).size(22.0));
                                            ui.add_space(3.0);
                                            ui.label(RichText::new(*name).color(text_pri()).strong().size(13.0));
                                            ui.label(RichText::new(*desc).color(text_mut()).size(10.0));
                                        });
                                    });
                            });
                        }
                    });

                    ui.add_space(16.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(i18n::tr("wel.try"))
                            .color(accent()).size(10.5).strong());
                        ui.add_space(4.0);
                        ui.label(RichText::new(i18n::tr("wel.try_hint"))
                            .color(text_sec()).size(12.0));
                        ui.add_space(18.0);

                        let go = ui.add_sized([200.0, 38.0], egui::Button::new(
                            RichText::new(format!("▶  {}", i18n::tr("wel.start"))).color(Color32::WHITE).strong().size(14.0))
                            .fill(accent()).rounding(Rounding::same(8.0)));
                        if go.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                        if go.clicked() { dismissed = true; }
                        ui.add_space(12.0);
                        ui.label(RichText::new(format!("⚠  {}", i18n::tr("wel.warn")))
                            .color(text_mut()).size(10.5).italics());
                    });
                });
                let _ = block;
            });

        if dismissed || ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter)) {
            self.settings.welcomed = true;
            self.settings.save();
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Ensure user-installed CLI tools (holehe, sherlock, maigret, subfinder…) are
/// findable even when launched from a desktop menu with a minimal PATH.
fn augment_path() {
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy().into_owned();
        let cur = std::env::var("PATH").unwrap_or_default();
        let mut extra = vec![
            format!("{home}/.local/bin"),
            "/usr/local/bin".to_string(),
            format!("{home}/go/bin"),
        ];
        extra.retain(|p| !cur.split(':').any(|c| c == p));
        if !extra.is_empty() {
            std::env::set_var("PATH", format!("{}:{}", extra.join(":"), cur));
        }
    }
}

pub fn run() -> eframe::Result<()> {
    install::print_banner();

    // Make sure user-installed CLI tools are on PATH (desktop-menu launches often
    // have a minimal PATH that omits ~/.local/bin).
    augment_path();

    // Desktop integration / CLI flags (--install, --uninstall, --no-install, --setup).
    if install::handle_cli() {
        return Ok(());
    }

    // Drop any stale browser nav request from a previous session, and make sure no
    // orphaned browser window is lingering, so nothing pops up on its own.
    let _ = std::fs::remove_file(nav_path());
    let _ = std::process::Command::new("pkill").args(["-x", "parasitegoogle"]).status();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("parasite — OSINT graph")
            .with_app_id("parasite-gui")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([960.0, 640.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "parasite — OSINT graph",
        options,
        Box::new(|cc| Ok(Box::new(Shell::new(cc)))),
    )
}

fn load_icon() -> egui::IconData {
    // Rasterise the v5 "cell" logo (three nested blobs) into a 128×128 icon.
    let size: usize = 128;
    egui::IconData { rgba: logo::icon_rgba(size), width: size as u32, height: size as u32 }
}
