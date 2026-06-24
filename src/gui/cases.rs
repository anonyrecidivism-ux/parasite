//! Cases mode — manage multiple investigations. Each case is a saved snapshot of
//! the graph (plus free-text notes) on disk; you switch between them, and the
//! active case's graph is loaded into the Graph workspace.

use egui::{self, Color32, Margin, RichText, Rounding, ScrollArea, Stroke};

use super::i18n;
use super::model::GraphData;
use super::theme::*;

pub struct CasesPanel {
    new_name: String,
    status:   String,
    want_save: Option<String>,
    want_open: Option<GraphData>,
    /// notes per case file-stem, lazily loaded
    notes:    std::collections::HashMap<String, String>,
}

fn cases_dir() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("parasite").join("cases")
}

fn list_cases() -> Vec<(String, u64)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(cases_dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("json") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                    out.push((stem.to_string(), size));
                }
            }
        }
    }
    out.sort();
    out
}

impl CasesPanel {
    pub fn new() -> Self {
        Self { new_name: String::new(), status: "save the current graph as a case, or open one".into(),
               want_save: None, want_open: None, notes: std::collections::HashMap::new() }
    }

    /// Consumed by the Shell: it provides the live graph data, which we persist.
    pub fn take_save(&mut self) -> Option<String> { self.want_save.take() }
    pub fn take_open(&mut self) -> Option<GraphData> { self.want_open.take() }

    /// Called by the Shell once it has the graph data for a pending save.
    pub fn write_case(&mut self, name: &str, data: &GraphData) {
        let dir = cases_dir();
        let _ = std::fs::create_dir_all(&dir);
        let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let path = dir.join(format!("{safe}.json"));
        match serde_json::to_string_pretty(data).map_err(|e| e.to_string())
            .and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string()))
        {
            Ok(_)  => self.status = format!("✓ saved case '{safe}' ({} entities)", data.entities.len()),
            Err(e) => self.status = format!("✗ {e}"),
        }
    }

    fn open_case(&mut self, name: &str) {
        let path = cases_dir().join(format!("{name}.json"));
        match std::fs::read_to_string(&path).map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<GraphData>(&s).map_err(|e| e.to_string()))
        {
            Ok(d)  => { self.status = format!("✓ opened case '{name}' ({} entities)", d.entities.len());
                        self.want_open = Some(d); }
            Err(e) => self.status = format!("✗ open failed: {e}"),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("cases_top")
            .frame(egui::Frame::none().fill(bg_sidebar())
                .inner_margin(Margin::symmetric(14.0, 10.0)).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    super::logo::widget(ui, 10.0);
                    ui.add_space(6.0);
                    ui.label(RichText::new(i18n::tr("tab.cases")).color(text_pri()).strong().size(15.0));
                    ui.add_space(14.0);
                    ui.label(RichText::new(i18n::tr("cs.name")).color(text_sec()).size(11.0));
                    ui.add(egui::TextEdit::singleline(&mut self.new_name)
                        .desired_width(180.0).hint_text("investigation-name"));
                    if ui.add(egui::Button::new(RichText::new(i18n::tr("cs.save")).color(Color32::WHITE).strong().size(12.0))
                        .fill(accent()).rounding(Rounding::same(corner()))).clicked()
                    {
                        let n = self.new_name.trim();
                        if n.is_empty() { self.status = "enter a case name".into(); }
                        else { self.want_save = Some(n.to_string()); }
                    }
                });
            });

        egui::TopBottomPanel::bottom("cases_status")
            .frame(egui::Frame::none().fill(bg_sidebar())
                .inner_margin(Margin::symmetric(14.0, 5.0)).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| { ui.label(RichText::new(&self.status).color(text_sec()).size(11.5)); });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_app()).inner_margin(Margin::symmetric(20.0, 16.0)))
            .show(ctx, |ui| {
                let cases = list_cases();
                if cases.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        super::logo::widget(ui, 30.0);
                        ui.add_space(10.0);
                        ui.label(RichText::new(i18n::tr("cs.empty")).color(text_pri()).strong().size(16.0));
                        ui.label(RichText::new(i18n::tr("cs.empty_hint")).color(text_sec()).size(12.0));
                    });
                    return;
                }
                let mut open: Option<String> = None;
                let mut del: Option<String> = None;
                ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    let max = (ui.available_width() - 20.0).min(820.0);
                    for (name, size) in &cases {
                        egui::Frame::none().fill(bg_panel()).rounding(Rounding::same(corner() + 4.0))
                            .stroke(Stroke::new(1.0, border())).inner_margin(Margin::symmetric(16.0, 12.0))
                            .show(ui, |ui| {
                                ui.set_width(max);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("▤ {name}")).color(text_pri()).strong().size(14.0));
                                    ui.label(RichText::new(format!("{:.1} KB", *size as f32 / 1024.0))
                                        .color(text_mut()).size(10.5));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if cbtn(ui, "✗", c_err()).clicked() { del = Some(name.clone()); }
                                        if cbtn(ui, i18n::tr("cs.open"), accent()).clicked() { open = Some(name.clone()); }
                                    });
                                });
                                let note = self.notes.entry(name.clone()).or_default();
                                ui.add(egui::TextEdit::multiline(note).desired_width(f32::INFINITY)
                                    .desired_rows(1).hint_text(i18n::tr("cs.note_ph")));
                            });
                        ui.add_space(8.0);
                    }
                });
                if let Some(n) = open { self.open_case(&n); }
                if let Some(n) = del {
                    let _ = std::fs::remove_file(cases_dir().join(format!("{n}.json")));
                    self.status = format!("⊘ deleted case '{n}'");
                }
            });
    }
}

fn cbtn(ui: &mut egui::Ui, label: &str, col: Color32) -> egui::Response {
    let r = ui.add(egui::Button::new(RichText::new(label).color(col).size(12.0))
        .fill(bg_input()).stroke(Stroke::new(1.0, border())).rounding(Rounding::same(corner())));
    if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    r
}
