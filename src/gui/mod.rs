//! parasite — an open-source, graph-based OSINT & web-reconnaissance toolkit.
//! A free alternative to Maltego: drop entities on a canvas and expand them with
//! transforms. Ships with a second "Operations" workspace that drives the
//! `parasite` recon engine.

mod app;
mod canvas;
mod engine;
mod install;
mod model;
mod settings;
mod sherlock;
mod theme;
mod transforms;

use eframe::egui::{self, Color32, Margin, RichText, Rounding, Stroke};
use settings::Settings;
use theme::*;

struct Shell {
    graph:         app::GraphPanel,
    settings:      Settings,
    show_settings: bool,
}

impl Shell {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::setup_fonts(&cc.egui_ctx);
        let settings = Settings::load();
        settings.apply(&cc.egui_ctx);
        Self {
            graph: app::GraphPanel::new(),
            show_settings: false,
            settings,
        }
    }
}

impl eframe::App for Shell {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("shell_tabs")
            .frame(egui::Frame::none().fill(bg_sidebar())
                .inner_margin(Margin::symmetric(12.0, 6.0))
                .stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("◈").color(accent()).size(16.0).strong());
                    ui.add_space(3.0);
                    ui.label(RichText::new("parasite").color(text_pri()).strong().size(15.0));
                    ui.label(RichText::new("OSINT graph · open-source Maltego").color(text_mut()).size(12.0));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let gear = ui.add(egui::Button::new(
                            RichText::new("⚙ Settings").color(text_sec()).size(12.0))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, border()))
                            .rounding(Rounding::same(5.0)));
                        if gear.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                        if gear.clicked() { self.show_settings = !self.show_settings; }
                        ui.add_space(8.0);
                        if ui.add(egui::Button::new(RichText::new("? Help").color(text_sec()).size(12.0))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, border()))
                            .rounding(Rounding::same(5.0))).clicked()
                        {
                            self.settings.welcomed = false;
                        }
                    });
                });
            });

        self.graph.ui(ctx);

        self.settings_window(ctx);
        if !self.settings.welcomed {
            self.welcome_window(ctx);
        }
    }
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
            .default_width(360.0)
            .anchor(egui::Align2::RIGHT_TOP, [-16.0, 52.0])
            .frame(egui::Frame::window(&ctx.style()).fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("THEME").color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                egui::ComboBox::from_id_source("theme_combo")
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
                ui.label(RichText::new("ACCENT COLOUR").color(text_mut()).size(10.0).strong());
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

                ui.add_space(12.0);
                ui.label(RichText::new("CANVAS").color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                changed |= ui.add(egui::Slider::new(&mut self.settings.node_radius, 12.0..=40.0)
                    .text("node size")).changed();
                changed |= ui.add(egui::Slider::new(&mut self.settings.font_scale, 0.8..=1.5)
                    .text("font scale")).changed();
                changed |= ui.checkbox(&mut self.settings.show_grid,
                    RichText::new("show grid").color(text_pri())).changed();
                changed |= ui.checkbox(&mut self.settings.edge_labels,
                    RichText::new("edge labels").color(text_pri())).changed();

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("↺ Reset to defaults").color(text_sec())).clicked() {
                        self.settings = Settings { welcomed: true, ..Settings::default() };
                        changed = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("🖥 Reinstall menu entry").color(text_sec()).size(11.0)).clicked() {
                            let _ = install::install(true);
                        }
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
        egui::Area::new(egui::Id::new("welcome_dim"))
            .order(egui::Order::Middle)
            .fixed_pos(egui::Pos2::ZERO)
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();
                ui.painter().rect_filled(screen, Rounding::ZERO,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 150));
            });

        egui::Window::new(RichText::new("Welcome to parasite").color(text_pri()).strong().size(20.0))
            .collapsible(false).resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(560.0)
            .frame(egui::Frame::window(&ctx.style()).fill(bg_panel()).stroke(Stroke::new(1.5, accent_dark())))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("◈  An open-source, graph-based OSINT toolkit — a free Maltego alternative.")
                    .color(text_sec()).size(13.5));
                ui.add_space(12.0);

                ui.label(RichText::new("HOW IT WORKS").color(accent()).size(11.0).strong());
                ui.add_space(6.0);
                for (n, line) in [
                    ("1", "Add an entity from the left palette — a Domain, IP, Email, Username, Hash…"),
                    ("2", "Right-click the node (or use the right panel) to run a transform."),
                    ("3", "Transforms discover related entities and link them automatically."),
                    ("4", "Keep expanding to map out infrastructure, people and accounts."),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!(" {n} ")).color(Color32::WHITE)
                            .background_color(accent()).strong());
                        ui.add_space(6.0);
                        ui.label(RichText::new(line).color(text_pri()).size(13.0));
                    });
                    ui.add_space(4.0);
                }

                ui.add_space(8.0);
                ui.label(RichText::new("TRY THIS").color(accent()).size(11.0).strong());
                ui.add_space(4.0);
                ui.label(RichText::new("• Add a Username (e.g. \"torvalds\") → right-click → Hunt Accounts\n\
                                        • Add a Domain → right-click → Subdomains (crt.sh) / WHOIS / DNS Records")
                    .color(text_sec()).size(12.5));

                ui.add_space(12.0);
                ui.label(RichText::new("⚠  For authorized security testing, research & education only. \
                                        You are responsible for what you target.")
                    .color(c_warn()).size(11.5).italics());

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let go = ui.add_sized([150.0, 34.0], egui::Button::new(
                        RichText::new("▶  Get started").color(Color32::WHITE).strong().size(13.5))
                        .fill(accent()).rounding(Rounding::same(6.0)));
                    if go.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                    if go.clicked() { dismissed = true; }
                    ui.add_space(8.0);
                    ui.label(RichText::new("open the ⚙ Settings to pick a theme & customize")
                        .color(text_mut()).size(11.0));
                });
                ui.add_space(2.0);
            });

        if dismissed || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings.welcomed = true;
            self.settings.save();
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn run() -> eframe::Result<()> {
    // Desktop integration / CLI flags (--install, --uninstall, --no-install).
    if install::handle_cli() {
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("parasite — OSINT graph")
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
    let size: usize = 16;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let cx = x as f32 - 7.5;
            let cy = y as f32 - 7.5;
            let dist = (cx * cx + cy * cy).sqrt();
            let idx = (y * size + x) * 4;
            if dist < 6.5 {
                rgba[idx]     = 217;
                rgba[idx + 1] = 119;
                rgba[idx + 2] = 87;
                rgba[idx + 3] = 255;
            }
        }
    }
    egui::IconData { rgba, width: size as u32, height: size as u32 }
}
