//! Watch mode — keep an eye on targets and get alerted when something changes:
//! a domain's certificate count (new subdomains), a GitHub user's activity, or a
//! Bitcoin address's transaction count. Keyless checks, run on demand or on a
//! timer; changes raise an alert in the feed.

use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use egui::{self, Color32, Margin, RichText, Rounding, ScrollArea, Stroke};

use super::i18n;
use super::theme::*;

#[derive(Clone, Copy, PartialEq)]
enum WatchKind { Domain, Username, Btc }
impl WatchKind {
    const ALL: [WatchKind; 3] = [WatchKind::Domain, WatchKind::Username, WatchKind::Btc];
    fn label(self) -> &'static str {
        match self { WatchKind::Domain => "Domain", WatchKind::Username => "GitHub user", WatchKind::Btc => "BTC address" }
    }
    fn icon(self) -> &'static str {
        match self { WatchKind::Domain => "◈", WatchKind::Username => "@", WatchKind::Btc => "Ƀ" }
    }
}

struct Item {
    id: u64, kind: WatchKind, value: String,
    state: Option<String>, prev: Option<String>,
    checking: bool, last: Option<f64>, changed_at: Option<f64>, checks: u32,
}

enum WMsg { Checked(u64, String) }

pub struct WatchPanel {
    rt: tokio::runtime::Runtime,
    tx: Sender<WMsg>,
    rx: Receiver<WMsg>,
    items: Vec<Item>,
    alerts: Vec<(f64, String)>,
    next_id: u64,
    new_kind: WatchKind,
    new_value: String,
    auto: bool,
    last_auto: f64,
    pending_graph: Option<(super::model::Kind, String)>,
}

impl WatchPanel {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2)
            .enable_all().build().expect("tokio runtime");
        let (tx, rx) = std::sync::mpsc::channel();
        Self { rt, tx, rx, items: Vec::new(), alerts: Vec::new(), next_id: 1,
               new_kind: WatchKind::Domain, new_value: String::new(), auto: false, last_auto: 0.0,
               pending_graph: None }
    }

    /// Consumed by the Shell to drop a watched value onto the graph.
    pub fn take_graph(&mut self) -> Option<(super::model::Kind, String)> { self.pending_graph.take() }

    fn add_kv(&mut self, kind: WatchKind, value: String) {
        if value.trim().is_empty() { return; }
        let id = self.next_id; self.next_id += 1;
        self.items.push(Item { id, kind, value: value.trim().to_string(),
            state: None, prev: None, checking: false, last: None, changed_at: None, checks: 0 });
        self.check_one(id);
    }

    fn add(&mut self) {
        let v = self.new_value.trim().to_string();
        self.new_value.clear();
        self.add_kv(self.new_kind, v);
    }

    fn check_one(&mut self, id: u64) {
        let Some(it) = self.items.iter_mut().find(|i| i.id == id) else { return };
        it.checking = true;
        let (kind, value) = (it.kind, it.value.clone());
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let state = check(kind, &value).await;
            let _ = tx.send(WMsg::Checked(id, state));
        });
    }

    fn check_all(&mut self) {
        let ids: Vec<u64> = self.items.iter().map(|i| i.id).collect();
        for id in ids { self.check_one(id); }
    }

    fn drain(&mut self, now: f64) {
        while let Ok(WMsg::Checked(id, state)) = self.rx.try_recv() {
            if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
                it.checking = false;
                it.last = Some(now);
                it.checks += 1;
                if let Some(prev) = &it.state {
                    if *prev != state && !state.starts_with('✗') {
                        it.prev = Some(prev.clone());
                        it.changed_at = Some(now);
                        self.alerts.insert(0, (now,
                            format!("{} {} changed:  {prev}  →  {state}", it.kind.icon(), it.value)));
                        if self.alerts.len() > 100 { self.alerts.truncate(100); }
                    }
                }
                it.state = Some(state);
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.drain(now);
        // auto-check every ~5 minutes
        if self.auto && now - self.last_auto > 300.0 && !self.items.is_empty() {
            self.last_auto = now;
            self.check_all();
        }
        if self.items.iter().any(|i| i.checking) { ctx.request_repaint_after(Duration::from_millis(200)); }

        egui::TopBottomPanel::top("watch_top")
            .frame(egui::Frame::none().fill(bg_sidebar())
                .inner_margin(Margin::symmetric(14.0, 10.0)).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    super::logo::widget(ui, 10.0);
                    ui.add_space(6.0);
                    ui.label(RichText::new(i18n::tr("tab.watch")).color(text_pri()).strong().size(15.0));
                    ui.add_space(12.0);
                    egui::ComboBox::from_id_salt("watch_kind")
                        .selected_text(RichText::new(self.new_kind.label()).color(text_pri()).size(12.0))
                        .show_ui(ui, |ui| {
                            for k in WatchKind::ALL {
                                ui.selectable_value(&mut self.new_kind, k, k.label());
                            }
                        });
                    let r = ui.add(egui::TextEdit::singleline(&mut self.new_value)
                        .desired_width(220.0).hint_text(i18n::tr("wt.value")));
                    let go = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.add(egui::Button::new(RichText::new(i18n::tr("wt.add")).color(Color32::WHITE).strong().size(12.0))
                        .fill(accent()).rounding(Rounding::same(corner()))).clicked() || go { self.add(); }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(&mut self.auto, RichText::new(i18n::tr("wt.auto")).color(text_sec()).size(11.0));
                        if ui.add(egui::Button::new(RichText::new(i18n::tr("wt.checkall")).color(text_pri()).size(12.0))
                            .fill(bg_item_hov()).stroke(Stroke::new(1.0, border())).rounding(Rounding::same(corner()))).clicked()
                        { self.check_all(); }
                    });
                });
            });

        // alerts feed at the bottom
        egui::TopBottomPanel::bottom("watch_alerts")
            .resizable(true).default_height(160.0)
            .frame(egui::Frame::none().fill(bg_output())
                .inner_margin(Margin::symmetric(12.0, 8.0)).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("⚑ {}", i18n::tr("wt.alerts"))).color(text_mut()).size(10.0).strong());
                    if !self.alerts.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(RichText::new(i18n::tr("wt.clear")).color(text_sec()).size(10.5)).clicked() {
                                self.alerts.clear();
                            }
                        });
                    }
                });
                ui.add_space(2.0);
                ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    if self.alerts.is_empty() {
                        ui.label(RichText::new(i18n::tr("wt.nochanges")).color(text_mut()).size(11.0).italics());
                    }
                    for (_, msg) in &self.alerts {
                        ui.label(RichText::new(msg).color(c_warn()).size(12.0));
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_app()).inner_margin(Margin::symmetric(16.0, 12.0)))
            .show(ctx, |ui| {
                if self.items.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        super::logo::widget_anim(ui, 26.0);
                        ui.add_space(8.0);
                        ui.label(RichText::new(i18n::tr("wt.empty")).color(text_pri()).strong().size(16.0));
                        ui.label(RichText::new(i18n::tr("wt.empty_hint")).color(text_sec()).size(12.0));
                        ui.add_space(14.0);
                        ui.label(RichText::new(i18n::tr("wt.try")).color(accent()).size(10.0).strong());
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let w = ui.available_width();
                            ui.add_space((w - 520.0).max(0.0) / 2.0);
                            for (k, v) in [(WatchKind::Domain, "tesla.com"),
                                           (WatchKind::Username, "torvalds"),
                                           (WatchKind::Btc, "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")] {
                                if ui.add(egui::Button::new(RichText::new(format!("{} {}", k.icon(), v))
                                    .color(text_pri()).size(11.5)).fill(bg_item_hov())
                                    .stroke(Stroke::new(1.0, border())).rounding(Rounding::same(14.0))).clicked()
                                { self.add_kv(k, v.to_string()); }
                            }
                        });
                    });
                    return;
                }
                let mut check: Option<u64> = None;
                let mut remove: Option<u64> = None;
                let mut to_graph: Option<(WatchKind, String)> = None;
                ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    for it in &self.items {
                        // status colour: grey unknown / amber checking / red error / green ok
                        let dot = if it.checking { c_warn() }
                                  else if it.state.as_deref().map(|s| s.starts_with('✗')).unwrap_or(false) { c_err() }
                                  else if it.state.is_some() { c_ok() } else { text_mut() };
                        let recently_changed = it.changed_at.map(|t| now - t < 30.0).unwrap_or(false);
                        let stroke = if recently_changed { Stroke::new(1.6, c_warn()) } else { Stroke::new(1.0, border()) };
                        egui::Frame::none().fill(bg_panel()).rounding(Rounding::same(corner() + 4.0))
                            .stroke(stroke).inner_margin(Margin::symmetric(14.0, 11.0))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    // pulsing dot while checking
                                    let (rc, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                                    let r = if it.checking { 5.0 + 1.5 * (0.5 + 0.5 * ((now * 5.0) as f32).sin()) } else { 5.0 };
                                    ui.painter().circle_filled(rc.center(), r, dot);
                                    ui.label(RichText::new(it.kind.icon()).color(accent()).size(15.0));
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(short(&it.value, 40)).color(text_pri()).strong().size(13.0));
                                            if recently_changed {
                                                egui::Frame::none().fill(c_warn()).rounding(Rounding::same(8.0))
                                                    .inner_margin(Margin::symmetric(6.0, 1.0)).show(ui, |ui| {
                                                        ui.label(RichText::new("Δ changed").color(Color32::BLACK).size(9.5).strong());
                                                    });
                                            }
                                        });
                                        let st = if it.checking { "checking…".to_string() }
                                                 else { it.state.clone().unwrap_or_else(|| "not checked".into()) };
                                        let col = if st.starts_with('✗') { c_err() } else { text_sec() };
                                        ui.label(RichText::new(&st).color(col).size(11.5).monospace());
                                        if let Some(p) = &it.prev {
                                            ui.label(RichText::new(format!("was: {p}")).color(text_mut()).size(10.0).monospace());
                                        }
                                    });
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if wbtn(ui, "✗", c_err()).clicked() { remove = Some(it.id); }
                                        if wbtn(ui, "⟳", text_sec()).clicked() { check = Some(it.id); }
                                        if wbtn(ui, "◇", accent()).on_hover_text("add to graph").clicked() {
                                            to_graph = Some((it.kind, it.value.clone()));
                                        }
                                        let ago = it.last.map(|t| ago_str(now - t)).unwrap_or_else(|| "—".into());
                                        ui.label(RichText::new(format!("⟳{} · {ago}", it.checks)).color(text_mut()).size(10.0));
                                    });
                                });
                            });
                        ui.add_space(7.0);
                    }
                });
                if let Some(id) = check { self.check_one(id); }
                if let Some(id) = remove { self.items.retain(|i| i.id != id); }
                if let Some((k, v)) = to_graph {
                    let kind = match k { WatchKind::Domain => super::model::Kind::Domain,
                        WatchKind::Username => super::model::Kind::Username,
                        WatchKind::Btc => super::model::Kind::BtcAddress };
                    self.pending_graph = Some((kind, v));
                }
            });
    }
}

fn wbtn(ui: &mut egui::Ui, label: &str, col: Color32) -> egui::Response {
    let r = ui.add(egui::Button::new(RichText::new(label).color(col).size(12.0))
        .fill(bg_input()).stroke(Stroke::new(1.0, border())).rounding(Rounding::same(corner())));
    if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    r
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() > n { format!("{}…", s.chars().take(n - 1).collect::<String>()) } else { s.to_string() }
}

fn ago_str(secs: f64) -> String {
    if secs < 60.0 { format!("{:.0}s ago", secs) }
    else if secs < 3600.0 { format!("{:.0}m ago", secs / 60.0) }
    else { format!("{:.0}h ago", secs / 3600.0) }
}

fn http() -> reqwest::Client {
    super::net::builder().user_agent("parasite-watch/1.0")
        .timeout(Duration::from_secs(20)).build().expect("client")
}

/// Compute the current "state" string for a watch item.
async fn check(kind: WatchKind, value: &str) -> String {
    match kind {
        WatchKind::Domain => {
            let url = format!("https://crt.sh/?q=%25.{}&output=json", value.trim());
            match http().get(&url).send().await {
                Ok(r) => match r.json::<serde_json::Value>().await {
                    Ok(j) => format!("{} cert record(s)", j.as_array().map(|a| a.len()).unwrap_or(0)),
                    Err(_) => "✗ parse error".into(),
                },
                Err(e) => format!("✗ {e}"),
            }
        }
        WatchKind::Username => {
            let url = format!("https://api.github.com/users/{}", value.trim());
            match http().get(&url).send().await {
                Ok(r) if r.status().as_u16() == 404 => "not found".into(),
                Ok(r) => match r.json::<serde_json::Value>().await {
                    Ok(j) => format!("repos:{} followers:{} gists:{}",
                        j["public_repos"].as_u64().unwrap_or(0),
                        j["followers"].as_u64().unwrap_or(0),
                        j["public_gists"].as_u64().unwrap_or(0)),
                    Err(_) => "✗ parse error".into(),
                },
                Err(e) => format!("✗ {e}"),
            }
        }
        WatchKind::Btc => {
            let url = format!("https://blockchain.info/rawaddr/{}?limit=0", value.trim());
            match http().get(&url).send().await {
                Ok(r) => match r.json::<serde_json::Value>().await {
                    Ok(j) => format!("tx:{} balance:{} sat",
                        j["n_tx"].as_u64().unwrap_or(0), j["final_balance"].as_u64().unwrap_or(0)),
                    Err(_) => "✗ parse / not found".into(),
                },
                Err(e) => format!("✗ {e}"),
            }
        }
    }
}
