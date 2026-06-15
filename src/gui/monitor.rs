//! Crypto monitoring — a **live transaction feed**. No addresses to enter: it
//! streams recent BTC & ETH transactions straight into a scrolling list, shows
//! their value in coin and ≈USD, and lets you filter to only the big ones
//! ("whale" moves). All via free, key-less public APIs.

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use eframe::egui::{self, Color32, FontFamily, FontId, Margin, RichText, Rounding,
                   ScrollArea, Stroke};

use super::theme::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Chain { Btc, Eth, Ton }
impl Chain {
    fn sym(self) -> &'static str { match self { Chain::Btc => "BTC", Chain::Eth => "ETH", Chain::Ton => "TON" } }
    fn icon(self) -> &'static str { match self { Chain::Btc => "Ƀ", Chain::Eth => "Ξ", Chain::Ton => "◈" } }
    fn color(self) -> Color32 {
        match self {
            Chain::Btc => Color32::from_rgb(242, 169, 0),
            Chain::Eth => Color32::from_rgb(130, 130, 230),
            Chain::Ton => Color32::from_rgb(70, 175, 235),
        }
    }
    fn explorer(self, hash: &str) -> String {
        match self {
            Chain::Btc => format!("https://blockchair.com/bitcoin/transaction/{hash}"),
            Chain::Eth => format!("https://etherscan.io/tx/{hash}"),
            Chain::Ton => format!("https://tonviewer.com/transaction/{hash}"),
        }
    }
    fn price(self, m: &MonitorPanel) -> f64 {
        match self { Chain::Btc => m.price_btc, Chain::Eth => m.price_eth, Chain::Ton => m.price_ton }
    }
}

struct Tx { chain: Chain, hash: String, native: f64, fresh: bool }

/// A tracked currency for the price-movers strip (rises & falls).
#[derive(Clone)]
struct Mover { sym: String, price: f64, change: f64 }

enum FeedMsg { Txs(Vec<(Chain, String, f64)>), Movers(Vec<Mover>) }

pub struct MonitorPanel {
    feed:     Vec<Tx>,
    seen:     HashSet<String>,
    rt:       tokio::runtime::Runtime,
    tx:       Sender<FeedMsg>,
    rx:       Receiver<FeedMsg>,
    client:   reqwest::Client,
    show_btc: bool,
    show_eth: bool,
    show_ton: bool,
    min_usd:  f64,
    price_btc: f64,
    price_eth: f64,
    price_ton: f64,
    movers:   Vec<Mover>,
    paused:   bool,
    interval: f64,
    last_poll: f64,
    last_price: f64,
    fetching: usize,
}

impl MonitorPanel {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_all().build()
            .expect("mon runtime");
        let (tx, rx) = std::sync::mpsc::channel();
        let client = reqwest::Client::builder().user_agent("parasite-monitor/1.0")
            .timeout(Duration::from_secs(15)).build().unwrap();
        Self {
            feed: Vec::new(), seen: HashSet::new(), rt, tx, rx, client,
            show_btc: true, show_eth: true, show_ton: true, min_usd: 50_000.0,
            price_btc: 0.0, price_eth: 0.0, price_ton: 0.0, movers: Vec::new(), paused: false,
            interval: 7.0, last_poll: 0.0, last_price: 0.0, fetching: 0,
        }
    }

    fn poll(&mut self, now: f64) {
        self.last_poll = now;
        if self.show_btc {
            let (tx, c) = (self.tx.clone(), self.client.clone());
            self.fetching += 1;
            self.rt.spawn(async move { let _ = tx.send(FeedMsg::Txs(fetch_btc(&c).await)); });
        }
        if self.show_eth {
            let (tx, c) = (self.tx.clone(), self.client.clone());
            self.fetching += 1;
            self.rt.spawn(async move { let _ = tx.send(FeedMsg::Txs(fetch_eth(&c).await)); });
        }
        if self.show_ton {
            let (tx, c) = (self.tx.clone(), self.client.clone());
            self.fetching += 1;
            self.rt.spawn(async move { let _ = tx.send(FeedMsg::Txs(fetch_ton(&c).await)); });
        }
    }

    fn poll_prices(&mut self, now: f64) {
        self.last_price = now;
        let (tx, c) = (self.tx.clone(), self.client.clone());
        self.rt.spawn(async move { let _ = tx.send(FeedMsg::Movers(fetch_movers(&c).await)); });
    }

    fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                FeedMsg::Movers(m) => {
                    for x in &m {
                        match x.sym.as_str() {
                            "BTC" => self.price_btc = x.price,
                            "ETH" => self.price_eth = x.price,
                            "TON" => self.price_ton = x.price,
                            _ => {}
                        }
                    }
                    if !m.is_empty() { self.movers = m; }
                }
                FeedMsg::Txs(txs) => {
                    self.fetching = self.fetching.saturating_sub(1);
                    if self.paused { continue; }
                    for t in &mut self.feed { t.fresh = false; }
                    let mut added = 0;
                    for (chain, hash, native) in txs {
                        if self.seen.contains(&hash) { continue; }
                        self.seen.insert(hash.clone());
                        self.feed.insert(0, Tx { chain, hash, native, fresh: true });
                        added += 1;
                    }
                    let _ = added;
                    if self.feed.len() > 400 {
                        for t in self.feed.drain(400..) { self.seen.remove(&t.hash); }
                    }
                }
            }
        }
    }

    fn usd(&self, t: &Tx) -> f64 {
        t.native * t.chain.price(self)
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        self.drain();
        let now = ctx.input(|i| i.time);
        if !self.paused && self.fetching == 0 && (self.last_poll == 0.0 || now - self.last_poll >= self.interval) {
            self.poll(now);
        }
        if self.last_price == 0.0 || now - self.last_price >= 60.0 { self.poll_prices(now); }
        if !self.paused { ctx.request_repaint_after(Duration::from_millis(700)); }

        self.toolbar(ctx);
        self.movers_strip(ctx);
        self.feed_list(ctx);
    }

    /// Live price strip — rises (green ▲) and falls (red ▼) across tracked coins.
    fn movers_strip(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("mon_movers")
            .frame(egui::Frame::none().fill(bg_sidebar()).inner_margin(Margin::symmetric(12.0, 6.0))
                .stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                if self.movers.is_empty() {
                    ui.label(RichText::new("⟳ fetching prices…").color(text_mut()).size(11.0));
                    return;
                }
                ScrollArea::horizontal().auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("PRICES · 24h").color(text_mut()).strong().size(10.5));
                        ui.add_space(6.0);
                        for m in &self.movers {
                            let up = m.change >= 0.0;
                            let col = if up { c_ok() } else { c_err() };
                            let arrow = if up { "▲" } else { "▼" };
                            egui::Frame::none().fill(bg_panel()).rounding(Rounding::same(6.0))
                                .inner_margin(Margin::symmetric(9.0, 5.0))
                                .stroke(Stroke::new(1.0, border())).show(ui, |ui| {
                                    ui.label(RichText::new(&m.sym).color(text_pri()).strong().size(12.0));
                                    ui.add_space(4.0);
                                    ui.label(RichText::new(format!("${}", fmt_price(m.price)))
                                        .color(text_sec()).size(11.5)
                                        .font(FontId::new(11.5, FontFamily::Monospace)));
                                    ui.add_space(4.0);
                                    ui.label(RichText::new(format!("{arrow} {:.2}%", m.change.abs()))
                                        .color(col).strong().size(11.5));
                                });
                            ui.add_space(5.0);
                        }
                    });
                });
            });
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("mon_toolbar")
            .frame(egui::Frame::none().fill(bg_panel()).inner_margin(Margin::symmetric(12.0, 7.0))
                .stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⏱ LIVE FEED").color(accent()).strong().size(13.0));
                    ui.add_space(8.0);
                    ui.checkbox(&mut self.show_btc, RichText::new("Ƀ BTC").color(Chain::Btc.color()).size(12.0));
                    ui.checkbox(&mut self.show_eth, RichText::new("Ξ ETH").color(Chain::Eth.color()).size(12.0));
                    ui.checkbox(&mut self.show_ton, RichText::new("◈ TON").color(Chain::Ton.color()).size(12.0));
                    ui.add_space(10.0);
                    ui.label(RichText::new("min").color(text_mut()).size(11.0));
                    ui.add(egui::Slider::new(&mut self.min_usd, 0.0..=1_000_000.0)
                        .logarithmic(true).custom_formatter(|n, _| format!("${}", fmt_usd(n)))
                        .clamping(egui::SliderClamping::Always));
                    let plabel = if self.paused { "▶ resume" } else { "⏸ pause" };
                    if ui.add(egui::Button::new(RichText::new(plabel).color(text_sec()).size(12.0))
                        .fill(bg_input()).stroke(Stroke::new(1.0, border())).rounding(Rounding::same(5.0))).clicked() {
                        self.paused = !self.paused;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let status = if self.fetching > 0 { "⟳ live" } else { "● live" };
                        ui.label(RichText::new(status).color(if self.paused { c_warn() } else { c_ok() }).size(11.0));
                    });
                });
            });
    }

    fn feed_list(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::none().fill(bg_canvas()).inner_margin(Margin::same(12.0)))
            .show(ctx, |ui| {
                // collect visible rows first (avoid borrow issues with open links)
                let rows: Vec<(Chain, String, f64, f64, bool)> = self.feed.iter()
                    .filter(|t| t.native > 0.0 && match t.chain {
                        Chain::Btc => self.show_btc, Chain::Eth => self.show_eth, Chain::Ton => self.show_ton })
                    .map(|t| (t.chain, t.hash.clone(), t.native, self.usd(t), t.fresh))
                    .filter(|(_, _, _, usd, _)| self.min_usd <= 0.0 || *usd == 0.0 || *usd >= self.min_usd)
                    .take(200)
                    .collect();

                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} transactions", rows.len())).color(text_mut()).size(11.0));
                    if self.paused { ui.label(RichText::new("⏸ paused").color(c_warn()).size(11.0)); }
                });
                ui.add_space(6.0);

                if rows.is_empty() {
                    ui.label(RichText::new("Waiting for transactions above the threshold…")
                        .color(text_mut()).italics().size(12.5));
                    return;
                }

                let mut open: Option<(Chain, String)> = None;
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (chain, hash, native, usd, fresh) in &rows {
                        let bg = if *fresh { bg_item_sel() } else { bg_panel() };
                        let r = egui::Frame::none().fill(bg).rounding(Rounding::same(6.0))
                            .inner_margin(Margin::symmetric(12.0, 8.0))
                            .stroke(Stroke::new(1.0, if *fresh { accent_dark() } else { border() }))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(chain.icon()).color(chain.color()).strong().size(17.0));
                                    ui.add_space(6.0);
                                    ui.label(RichText::new(format!("{:.4} {}", native, chain.sym()))
                                        .color(text_pri()).strong().size(13.5)
                                        .font(FontId::new(13.5, FontFamily::Monospace)));
                                    if *usd > 0.0 {
                                        ui.add_space(8.0);
                                        ui.label(RichText::new(format!("≈ ${}", fmt_usd(*usd)))
                                            .color(if *usd >= 100_000.0 { c_ok() } else { text_sec() }).size(12.5));
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(format!("{}…", hash.chars().take(16).collect::<String>()))
                                            .color(text_mut()).size(11.0).font(FontId::new(11.0, FontFamily::Monospace)));
                                    });
                                });
                            }).response.interact(egui::Sense::click());
                        if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                        if r.clicked() { open = Some((*chain, hash.clone())); }
                        ui.add_space(4.0);
                    }
                });
                if let Some((chain, hash)) = open { super::app_open(&chain.explorer(&hash)); }
            });
    }
}

// ── feed fetchers (free, key-less) ──────────────────────────────────────────────
async fn json(c: &reqwest::Client, url: &str) -> Result<serde_json::Value, String> {
    let resp = c.get(url).send().await.map_err(|e| e.to_string())?;
    let t = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&t).map_err(|e| e.to_string())
}

async fn fetch_btc(c: &reqwest::Client) -> Vec<(Chain, String, f64)> {
    let mut out = Vec::new();
    if let Ok(j) = json(c, "https://blockchain.info/unconfirmed-transactions?format=json").await {
        if let Some(txs) = j.get("txs").and_then(|v| v.as_array()) {
            for t in txs.iter().take(100) {
                let hash = t.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let sat: f64 = t.get("out").and_then(|v| v.as_array())
                    .map(|outs| outs.iter().filter_map(|o| o.get("value").and_then(|v| v.as_f64())).sum())
                    .unwrap_or(0.0);
                if !hash.is_empty() { out.push((Chain::Btc, hash, sat / 1e8)); }
            }
        }
    }
    out
}

async fn fetch_eth(c: &reqwest::Client) -> Vec<(Chain, String, f64)> {
    let mut out = Vec::new();
    if let Ok(j) = json(c, "https://eth.blockscout.com/api/v2/transactions?filter=validated").await {
        if let Some(items) = j.get("items").and_then(|v| v.as_array()) {
            for t in items.iter().take(50) {
                let hash = t.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let wei = t.get("value").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                if !hash.is_empty() { out.push((Chain::Eth, hash, wei / 1e18)); }
            }
        }
    }
    out
}

async fn fetch_ton(c: &reqwest::Client) -> Vec<(Chain, String, f64)> {
    let mut out = Vec::new();
    if let Ok(j) = json(c, "https://toncenter.com/api/v3/transactions?limit=40&sort=desc").await {
        if let Some(items) = j.get("transactions").and_then(|v| v.as_array()) {
            for t in items {
                let hash = t.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
                // value moved = incoming message value + sum of outgoing message values
                let inv = t.pointer("/in_msg/value").and_then(val_nano).unwrap_or(0.0);
                let outv: f64 = t.get("out_msgs").and_then(|v| v.as_array())
                    .map(|ms| ms.iter().filter_map(|m| m.get("value").and_then(val_nano)).sum())
                    .unwrap_or(0.0);
                let nano = inv.max(outv);
                if !hash.is_empty() && nano > 0.0 { out.push((Chain::Ton, hash, nano / 1e9)); }
            }
        }
    }
    out
}

fn val_nano(v: &serde_json::Value) -> Option<f64> {
    v.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| v.as_f64())
}

/// Top tracked coins with their 24h change — feeds the rises/falls strip.
async fn fetch_movers(c: &reqwest::Client) -> Vec<Mover> {
    let url = "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&ids=bitcoin,ethereum,the-open-network,tether,solana,ripple,binancecoin,dogecoin&price_change_percentage=24h";
    let mut out = Vec::new();
    if let Ok(j) = json(c, url).await {
        if let Some(arr) = j.as_array() {
            for c in arr {
                let id = c.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let sym = match id {
                    "bitcoin" => "BTC", "ethereum" => "ETH", "the-open-network" => "TON",
                    "tether" => "USDT", "solana" => "SOL", "ripple" => "XRP",
                    "binancecoin" => "BNB", "dogecoin" => "DOGE",
                    _ => continue,
                };
                let price = c.get("current_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let change = c.get("price_change_percentage_24h").and_then(|v| v.as_f64()).unwrap_or(0.0);
                out.push(Mover { sym: sym.to_string(), price, change });
            }
        }
    }
    // keep a stable, meaningful order
    let order = ["BTC", "ETH", "TON", "USDT", "SOL", "XRP", "BNB", "DOGE"];
    out.sort_by_key(|m| order.iter().position(|s| *s == m.sym).unwrap_or(99));
    out
}

fn fmt_price(n: f64) -> String {
    if n >= 1000.0 { format!("{:.0}", n) }
    else if n >= 1.0 { format!("{:.2}", n) }
    else { format!("{:.4}", n) }
}

fn fmt_usd(n: f64) -> String {
    if n >= 1_000_000.0 { format!("{:.2}M", n / 1e6) }
    else if n >= 1_000.0 { format!("{:.1}k", n / 1e3) }
    else { format!("{n:.0}") }
}
