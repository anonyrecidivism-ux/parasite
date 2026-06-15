//! ParasiteGoogle launcher tab.
//!
//! The actual browser is a **real WebKitGTK browser** shipped as a separate
//! `parasitegoogle` binary (see `src/parasitegoogle.rs`) — a real web engine can't
//! share egui's event loop, and running it out-of-process is what stops the
//! browser from ever freezing or crashing the main window. This tab is just a
//! branded home screen that opens that browser; every "open in browser" action in
//! parasite launches it too.

use eframe::egui::{self, FontFamily, FontId, Margin, RichText, Rounding, Stroke, TextEdit};

use super::theme::*;

pub struct BrowserPanel {
    bar: String,
    /// The panel area in egui points — the Shell uses it to place the real
    /// WebKitGTK overlay window exactly over this region.
    pub last_rect: Option<egui::Rect>,
}

impl BrowserPanel {
    pub fn new() -> Self { Self { bar: String::new(), last_rect: None } }

    fn open(&self) {
        let q = self.bar.trim();
        super::app_open(if q.is_empty() { "https://duckduckgo.com/" } else { q });
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_canvas()))
            .show(ctx, |ui| {
                self.last_rect = Some(ui.max_rect());
                ui.add_space(ui.available_height() * 0.22);
                ui.vertical_centered(|ui| {
                    super::logo::widget(ui, 52.0);
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let w = ui.available_width();
                        ui.add_space((w - 280.0).max(0.0) / 2.0);
                        ui.label(RichText::new("Parasite").color(text_pri()).strong().size(38.0));
                        ui.label(RichText::new("Google").color(accent()).strong().size(38.0));
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new("a real WebKitGTK browser — opens in its own window")
                        .color(text_mut()).size(12.0));
                    ui.add_space(20.0);

                    // search / URL bar
                    ui.horizontal(|ui| {
                        let w = ui.available_width();
                        let field = 540.0_f32.min(w - 40.0);
                        ui.add_space((w - field - 90.0).max(0.0) / 2.0);
                        egui::Frame::none().fill(bg_input()).rounding(Rounding::same(22.0))
                            .stroke(Stroke::new(1.0, border())).inner_margin(Margin::symmetric(14.0, 8.0))
                            .show(ui, |ui| {
                                super::logo::widget(ui, 9.0);
                                ui.add_space(6.0);
                                let te = ui.add(TextEdit::singleline(&mut self.bar)
                                    .hint_text("search ParasiteGoogle or type a URL")
                                    .frame(false).desired_width(field - 40.0)
                                    .font(FontId::new(14.0, FontFamily::Proportional)));
                                if te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    self.open();
                                }
                            });
                        ui.add_space(8.0);
                        if pgbtn(ui, "Open").clicked() { self.open(); }
                    });

                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        let w = ui.available_width();
                        ui.add_space((w - 360.0).max(0.0) / 2.0);
                        for (label, url) in [
                            ("Google",     "https://www.google.com/"),
                            ("DuckDuckGo", "https://duckduckgo.com/"),
                            ("GitHub",     "https://github.com/"),
                            ("Shodan",     "https://www.shodan.io/"),
                        ] {
                            if pgbtn(ui, label).clicked() { super::app_open(url); }
                            ui.add_space(6.0);
                        }
                    });
                });
            });
    }
}

fn pgbtn(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let r = ui.add(egui::Button::new(RichText::new(label).color(text_sec()).size(13.0))
        .fill(bg_input()).stroke(Stroke::new(1.0, border())).rounding(Rounding::same(7.0)));
    if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    r
}
