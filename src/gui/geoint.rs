//! GEOINT mode — a standalone geospatial workspace: a slippy OpenStreetMap tile
//! map with markers, EXIF-GPS extraction from images, reverse geocoding, distance
//! measuring and one-click links into Google Maps / Earth / Street View.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::sync::mpsc::{Receiver, Sender};

use eframe::egui::{self, Color32, FontFamily, FontId, Margin, Pos2, Rect, RichText,
                   Rounding, ScrollArea, Sense, Stroke, TextEdit, Vec2};

use super::theme::*;

const TILE: f64 = 256.0;

pub struct GeoPoint {
    pub lat:   f64,
    pub lon:   f64,
    pub label: String,
    pub props: Vec<(String, String)>,
}

enum GeoMsg {
    Tile { key: (u8, u32, u32), img: egui::ColorImage },
    Address { idx: usize, text: String },
}

pub struct GeoPanel {
    center_lat: f64,
    center_lon: f64,
    zoom:       f32,
    points:     Vec<GeoPoint>,
    selected:   Option<usize>,
    measure:    Option<usize>,

    tiles:    HashMap<(u8, u32, u32), egui::TextureHandle>,
    inflight: HashSet<(u8, u32, u32)>,
    rt:       tokio::runtime::Runtime,
    tx:       Sender<GeoMsg>,
    rx:       Receiver<GeoMsg>,
    client:   reqwest::Client,

    satellite: bool,
    add_lat:   String,
    add_lon:   String,
    add_label: String,
    exif_path: String,
    map_rect:  Rect,
    log:       Vec<String>,
}

// ── Web-Mercator projection ─────────────────────────────────────────────────────
fn world_px(lat: f64, lon: f64, z: u8) -> (f64, f64) {
    let n = (1u64 << z) as f64 * TILE;
    let x = (lon + 180.0) / 360.0 * n;
    let latr = lat.to_radians();
    let y = (1.0 - (latr.tan() + 1.0 / latr.cos()).ln() / PI) / 2.0 * n;
    (x, y)
}

fn px_to_lonlat(x: f64, y: f64, z: u8) -> (f64, f64) {
    let n = (1u64 << z) as f64 * TILE;
    let lon = x / n * 360.0 - 180.0;
    let yy = y / n;
    let lat = (PI * (1.0 - 2.0 * yy)).sinh().atan().to_degrees();
    (lat, lon)
}

/// Haversine distance in kilometres.
pub fn haversine(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = 6371.0;
    let (dlat, dlon) = ((b.0 - a.0).to_radians(), (b.1 - a.1).to_radians());
    let h = (dlat / 2.0).sin().powi(2)
        + a.0.to_radians().cos() * b.0.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * h.sqrt().asin()
}

fn to_dms(deg: f64, pos: char, neg: char) -> String {
    let hemi = if deg >= 0.0 { pos } else { neg };
    let d = deg.abs();
    let dd = d.floor();
    let m = (d - dd) * 60.0;
    let mm = m.floor();
    let ss = (m - mm) * 60.0;
    format!("{dd:.0}°{mm:.0}'{ss:.1}\"{hemi}")
}

impl GeoPanel {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(3).enable_all().build()
            .expect("geo runtime");
        let (tx, rx) = std::sync::mpsc::channel();
        let client = reqwest::Client::builder()
            .user_agent("parasite-geoint/1.0 (OSINT tool)")
            .build().unwrap();
        Self {
            center_lat: 48.8584, center_lon: 2.2945, zoom: 12.0, // Eiffel Tower
            points: Vec::new(), selected: None, measure: None,
            tiles: HashMap::new(), inflight: HashSet::new(), rt, tx, rx, client,
            satellite: false,
            add_lat: String::new(), add_lon: String::new(), add_label: String::new(),
            exif_path: String::new(), map_rect: Rect::NOTHING,
            log: vec!["◦  add a point, import an image's EXIF, or pan the map".into()],
        }
    }

    fn log(&mut self, m: impl Into<String>) {
        self.log.push(m.into());
        if self.log.len() > 200 { self.log.remove(0); }
    }

    fn request_tile(&mut self, key: (u8, u32, u32)) {
        if self.tiles.contains_key(&key) || self.inflight.contains(&key) { return; }
        self.inflight.insert(key);
        let url = if self.satellite {
            // Esri World Imagery uses z/y/x ordering
            format!("https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{}/{}/{}", key.0, key.2, key.1)
        } else {
            format!("https://tile.openstreetmap.org/{}/{}/{}.png", key.0, key.1, key.2)
        };
        let tx = self.tx.clone();
        let client = self.client.clone();
        self.rt.spawn(async move {
            if let Ok(resp) = client.get(&url).send().await {
                if let Ok(bytes) = resp.bytes().await {
                    if let Ok(img) = image::load_from_memory(&bytes) {
                        let rgba = img.to_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let ci = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                        let _ = tx.send(GeoMsg::Tile { key, img: ci });
                    }
                }
            }
        });
    }

    fn drain(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                GeoMsg::Tile { key, img } => {
                    let tex = ctx.load_texture(format!("t{}_{}_{}", key.0, key.1, key.2), img,
                        egui::TextureOptions::LINEAR);
                    self.tiles.insert(key, tex);
                    self.inflight.remove(&key);
                }
                GeoMsg::Address { idx, text } => {
                    if let Some(p) = self.points.get_mut(idx) {
                        p.props.retain(|(k, _)| k != "address");
                        p.props.push(("address".into(), text.clone()));
                    }
                    self.log(format!("✓  {text}"));
                }
            }
        }
        if !self.inflight.is_empty() { ctx.request_repaint(); }
    }

    fn reverse_geocode(&mut self, idx: usize) {
        let Some(p) = self.points.get(idx) else { return };
        let (lat, lon) = (p.lat, p.lon);
        let tx = self.tx.clone();
        let client = self.client.clone();
        self.log("◦  reverse geocoding…");
        self.rt.spawn(async move {
            let url = format!("https://nominatim.openstreetmap.org/reverse?format=json&lat={lat}&lon={lon}");
            if let Ok(resp) = client.get(&url).send().await {
                if let Ok(t) = resp.text().await {
                    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&t) {
                        if let Some(name) = j.get("display_name").and_then(|v| v.as_str()) {
                            let _ = tx.send(GeoMsg::Address { idx, text: name.to_string() });
                        }
                    }
                }
            }
        });
    }

    fn add_point(&mut self, lat: f64, lon: f64, label: String, props: Vec<(String, String)>) {
        self.points.push(GeoPoint { lat, lon, label, props });
        self.selected = Some(self.points.len() - 1);
        self.center_lat = lat;
        self.center_lon = lon;
    }

    fn import_exif(&mut self, path: &str) {
        let file = match std::fs::File::open(path.trim()) {
            Ok(f) => f, Err(e) => { self.log(format!("✗  open failed: {e}")); return; }
        };
        let mut buf = std::io::BufReader::new(&file);
        let exif = match exif::Reader::new().read_from_container(&mut buf) {
            Ok(e) => e, Err(e) => { self.log(format!("✗  no EXIF: {e}")); return; }
        };
        let mut props: Vec<(String, String)> = Vec::new();
        let get = |tag: exif::Tag, name: &str, props: &mut Vec<(String, String)>| {
            if let Some(f) = exif.get_field(tag, exif::In::PRIMARY) {
                props.push((name.into(), f.display_value().with_unit(&exif).to_string()));
            }
        };
        get(exif::Tag::Make, "camera make", &mut props);
        get(exif::Tag::Model, "camera model", &mut props);
        get(exif::Tag::DateTimeOriginal, "taken", &mut props);
        get(exif::Tag::Software, "software", &mut props);

        let lat = gps(&exif, exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef);
        let lon = gps(&exif, exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef);
        match (lat, lon) {
            (Some(la), Some(lo)) => {
                let name = std::path::Path::new(path).file_name()
                    .map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
                self.log(format!("✓  GPS {la:.6}, {lo:.6} — {} EXIF field(s)", props.len()));
                self.add_point(la, lo, name, props);
            }
            _ => {
                self.log(format!("◦  image has EXIF ({} field(s)) but no GPS coordinates", props.len()));
            }
        }
    }

    // ── UI ──────────────────────────────────────────────────────────────────────
    pub fn ui(&mut self, ctx: &egui::Context) {
        self.drain(ctx);
        self.toolbar(ctx);
        self.sidebar(ctx);
        self.details(ctx);
        self.map(ctx);
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("geo_toolbar")
            .frame(egui::Frame::none().fill(bg_panel()).inner_margin(Margin::symmetric(12.0, 7.0))
                .stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("◎ GEOINT").color(accent()).strong().size(13.0));
                    ui.label(RichText::new("⚠ experimental / WIP").color(c_warn()).size(10.5).italics());
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("{:.5}, {:.5}  ·  z{:.1}",
                        self.center_lat, self.center_lon, self.zoom)).color(text_sec()).size(12.0));
                    ui.add_space(10.0);
                    if geobtn(ui, if self.satellite { "🛰 Satellite" } else { "🗺 Street" }).clicked() {
                        self.satellite = !self.satellite;
                        self.tiles.clear();
                        self.inflight.clear();
                        self.log(if self.satellite { "◦  satellite imagery (Esri)" } else { "◦  street map (OSM)" });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if geobtn(ui, "Earth view").clicked() {
                            super::app_open(&format!("https://earth.google.com/web/@{},{},1000a",
                                self.center_lat, self.center_lon));
                        }
                        if geobtn(ui, "Google Maps").clicked() {
                            super::app_open(&format!("https://www.google.com/maps/@{},{},{}z",
                                self.center_lat, self.center_lon, self.zoom.round() as i32));
                        }
                    });
                });
            });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("geo_side").resizable(true).default_width(230.0).width_range(180.0..=340.0)
            .frame(egui::Frame::none().fill(bg_sidebar()).inner_margin(Margin::symmetric(12.0, 10.0)))
            .show(ctx, |ui| {
                ui.label(RichText::new("ADD POINT").color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(&mut self.add_lat).hint_text("lat").desired_width(70.0));
                    ui.add(TextEdit::singleline(&mut self.add_lon).hint_text("lon").desired_width(70.0));
                });
                ui.add(TextEdit::singleline(&mut self.add_label).hint_text("label").desired_width(f32::INFINITY));
                if ui.add_sized([ui.available_width(), 26.0], egui::Button::new(
                    RichText::new("＋ Add").color(Color32::WHITE).strong())
                    .fill(accent()).rounding(Rounding::same(5.0))).clicked()
                {
                    if let (Ok(la), Ok(lo)) = (self.add_lat.trim().parse(), self.add_lon.trim().parse()) {
                        let lbl = if self.add_label.trim().is_empty() { "point".into() } else { self.add_label.trim().to_string() };
                        self.add_point(la, lo, lbl, vec![]);
                        self.add_lat.clear(); self.add_lon.clear(); self.add_label.clear();
                    } else { self.log("✗  invalid lat/lon"); }
                }

                ui.add_space(10.0);
                ui.label(RichText::new("IMPORT EXIF (image)").color(text_mut()).size(10.0).strong());
                ui.add_space(4.0);
                ui.add(TextEdit::singleline(&mut self.exif_path).hint_text("/path/to/photo.jpg")
                    .desired_width(f32::INFINITY).font(FontId::new(12.0, FontFamily::Monospace)));
                if ui.add_sized([ui.available_width(), 26.0], egui::Button::new(
                    RichText::new("⛏ Extract GPS & metadata").color(text_pri()).size(12.0))
                    .stroke(Stroke::new(1.0, border())).rounding(Rounding::same(5.0))).clicked()
                {
                    let p = self.exif_path.clone();
                    self.import_exif(&p);
                }

                ui.add_space(10.0); ui.separator(); ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("POINTS ({})", self.points.len())).color(text_mut()).size(10.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("clear").color(text_mut()).size(10.0))
                            .fill(Color32::TRANSPARENT)).clicked() { self.points.clear(); self.selected = None; }
                    });
                });
                let mut focus: Option<usize> = None;
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (i, p) in self.points.iter().enumerate() {
                        let sel = self.selected == Some(i);
                        let bg = if sel { bg_item_sel() } else { Color32::TRANSPARENT };
                        let r = egui::Frame::none().fill(bg).rounding(Rounding::same(4.0))
                            .inner_margin(Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(RichText::new(format!("◉ {}", p.label)).color(if sel { text_pri() } else { text_sec() }).size(11.5));
                                ui.label(RichText::new(format!("{:.5}, {:.5}", p.lat, p.lon)).color(text_mut()).size(10.5));
                            }).response.interact(Sense::click());
                        if r.clicked() { focus = Some(i); }
                    }
                });
                if let Some(i) = focus {
                    self.selected = Some(i);
                    self.center_lat = self.points[i].lat;
                    self.center_lon = self.points[i].lon;
                }
            });
    }

    fn details(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("geo_details").resizable(true).default_width(270.0).width_range(200.0..=420.0)
            .frame(egui::Frame::none().fill(bg_panel()).inner_margin(Margin::symmetric(14.0, 12.0)))
            .show(ctx, |ui| {
                let Some(i) = self.selected else {
                    ui.label(RichText::new("Select a point").color(text_mut()).italics().size(12.0));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Drag to pan · scroll to zoom · click the map to drop a point.")
                        .color(text_mut()).size(11.5));
                    return;
                };
                let Some(p) = self.points.get(i) else { self.selected = None; return; };
                let (lat, lon, label) = (p.lat, p.lon, p.label.clone());
                let props = p.props.clone();
                ui.label(RichText::new(format!("◉ {label}")).color(text_pri()).strong().size(14.0));
                ui.add_space(4.0);
                ui.label(RichText::new(format!("{lat:.6}, {lon:.6}")).color(text_sec())
                    .font(FontId::new(12.0, FontFamily::Monospace)));
                ui.label(RichText::new(format!("{}  {}", to_dms(lat, 'N', 'S'), to_dms(lon, 'E', 'W')))
                    .color(text_mut()).size(11.0));

                ui.add_space(8.0);
                egui::Grid::new("geo_links").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
                    if geobtn(ui, "Google Maps").clicked() { super::app_open(&format!("https://www.google.com/maps?q={lat},{lon}")); }
                    if geobtn(ui, "Street View").clicked() { super::app_open(&format!("https://www.google.com/maps?q&layer=c&cbll={lat},{lon}")); }
                    ui.end_row();
                    if geobtn(ui, "Google Earth").clicked() { super::app_open(&format!("https://earth.google.com/web/@{lat},{lon},1000a")); }
                    if geobtn(ui, "OpenStreetMap").clicked() { super::app_open(&format!("https://www.openstreetmap.org/?mlat={lat}&mlon={lon}#map=16/{lat}/{lon}")); }
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if geobtn(ui, "↻ Reverse geocode").clicked() { self.reverse_geocode(i); }
                    let measuring = self.measure == Some(i);
                    if geobtn(ui, if measuring { "● measuring…" } else { "↔ Measure" }).clicked() {
                        if let Some(m) = self.measure.take() {
                            if m != i {
                                let d = haversine((self.points[m].lat, self.points[m].lon), (lat, lon));
                                self.log(format!("↔  {:.2} km between '{}' and '{label}'", d, self.points[m].label));
                            }
                        } else { self.measure = Some(i); }
                    }
                });

                if !props.is_empty() {
                    ui.add_space(8.0); ui.separator(); ui.add_space(4.0);
                    egui::Grid::new("geo_props").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                        for (k, v) in &props {
                            ui.label(RichText::new(k).color(text_sec()).size(11.0));
                            ui.add(egui::Label::new(RichText::new(v).color(text_pri()).size(11.0)
                                .font(FontId::new(11.0, FontFamily::Monospace))).wrap());
                            ui.end_row();
                        }
                    });
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if ui.add(egui::Button::new(RichText::new("⊘ Delete point").color(c_err()).size(11.0))
                        .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, border())).rounding(Rounding::same(4.0))).clicked()
                    {
                        self.points.remove(i);
                        self.selected = None;
                    }
                    // log
                    ui.add_space(4.0);
                    ScrollArea::vertical().max_height(120.0).auto_shrink([false, false]).stick_to_bottom(true)
                        .show(ui, |ui| {
                            for l in &self.log {
                                ui.label(RichText::new(l).color(text_mut()).size(10.5)
                                    .font(FontId::new(10.5, FontFamily::Monospace)));
                            }
                        });
                });
            });
    }

    fn map(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::none().fill(bg_app()).inner_margin(Margin::same(10.0)))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                self.map_rect = rect;
                let round = 14.0;
                let resp = ui.allocate_rect(rect, Sense::click_and_drag());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, Rounding::same(round), bg_canvas());

                // scroll → smooth continuous zoom (fractional; tiles are scaled)
                if resp.hovered() {
                    let sc = ui.input(|i| i.smooth_scroll_delta.y);
                    if sc != 0.0 {
                        self.zoom = (self.zoom + sc * map_sensitivity() * 0.01).clamp(2.0, 19.0);
                        ctx.request_repaint();
                    }
                }
                let iz = self.zoom.floor().clamp(2.0, 19.0) as u8;
                let scale = 2f64.powf((self.zoom - iz as f32) as f64); // 1.0 .. 2.0

                let (cwx, cwy) = world_px(self.center_lat, self.center_lon, iz);
                // drag → pan (delta in screen px, divided by the current scale)
                if resp.dragged() {
                    let d = resp.drag_delta();
                    let (nlat, nlon) = px_to_lonlat(cwx - d.x as f64 / scale, cwy - d.y as f64 / scale, iz);
                    self.center_lat = nlat.clamp(-85.0, 85.0);
                    self.center_lon = ((nlon + 180.0).rem_euclid(360.0)) - 180.0;
                }
                let (cwx, cwy) = world_px(self.center_lat, self.center_lon, iz);

                let w2s = |wx: f64, wy: f64| Pos2::new(
                    rect.center().x + ((wx - cwx) * scale) as f32,
                    rect.center().y + ((wy - cwy) * scale) as f32);
                let n = 1u64 << iz;
                let tsize = (TILE * scale) as f32;
                let halfw = rect.width() as f64 / (2.0 * scale);
                let halfh = rect.height() as f64 / (2.0 * scale);
                let x0 = ((cwx - halfw) / TILE).floor() as i64;
                let x1 = ((cwx + halfw) / TILE).ceil() as i64;
                let y0 = ((cwy - halfh) / TILE).floor() as i64;
                let y1 = ((cwy + halfh) / TILE).ceil() as i64;

                let mut wanted: Vec<(u8, u32, u32)> = Vec::new();
                for ty in y0..=y1 {
                    if ty < 0 || ty >= n as i64 { continue; }
                    for tx in x0..=x1 {
                        let txm = tx.rem_euclid(n as i64) as u32;
                        let key = (iz, txm, ty as u32);
                        let sp = w2s(tx as f64 * TILE, ty as f64 * TILE);
                        let trect = Rect::from_min_size(sp, Vec2::splat(tsize + 1.0));
                        if let Some(tex) = self.tiles.get(&key) {
                            painter.image(tex.id(), trect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                        } else {
                            painter.rect_filled(trect, Rounding::ZERO, bg_panel());
                            wanted.push(key);
                        }
                    }
                }
                for k in wanted { self.request_tile(k); }

                // click → drop point
                if resp.clicked() {
                    if let Some(pp) = ui.input(|i| i.pointer.interact_pos()) {
                        let wx = cwx + (pp.x - rect.center().x) as f64 / scale;
                        let wy = cwy + (pp.y - rect.center().y) as f64 / scale;
                        let (la, lo) = px_to_lonlat(wx, wy, iz);
                        self.add_point(la, lo, format!("pin {}", self.points.len() + 1), vec![]);
                    }
                }

                // markers
                for (i, p) in self.points.iter().enumerate() {
                    let (wx, wy) = world_px(p.lat, p.lon, iz);
                    let sp = w2s(wx, wy);
                    if !rect.expand(20.0).contains(sp) { continue; }
                    let selc = self.selected == Some(i);
                    let col = if selc { accent() } else { c_err() };
                    let rr = if selc { 7.0 } else { 5.0 };
                    painter.add(egui::Shape::convex_polygon(
                        vec![sp, sp + Vec2::new(-rr, -rr * 1.8), sp + Vec2::new(rr, -rr * 1.8)], col, Stroke::NONE));
                    painter.circle_filled(sp + Vec2::new(0.0, -rr * 1.8), rr, col);
                    painter.circle_filled(sp + Vec2::new(0.0, -rr * 1.8), rr * 0.4, bg_panel());
                    if iz >= 10 {
                        painter.text(sp + Vec2::new(8.0, -rr * 1.8), egui::Align2::LEFT_CENTER, &p.label,
                            FontId::new(11.0, FontFamily::Proportional), text_pri());
                    }
                }

                // attribution
                painter.text(rect.left_bottom() + Vec2::new(8.0, -6.0), egui::Align2::LEFT_BOTTOM,
                    "© OpenStreetMap", FontId::new(10.0, FontFamily::Proportional), text_mut());

                // round the corners by carving the outside with the panel colour
                let full = ui.painter();
                let bg = bg_app();
                carve_corner(full, rect.min, Pos2::new(rect.min.x + round, rect.min.y + round), -90.0, -180.0, round, bg);
                carve_corner(full, rect.right_top(), Pos2::new(rect.max.x - round, rect.min.y + round), -90.0, 0.0, round, bg);
                carve_corner(full, rect.left_bottom(), Pos2::new(rect.min.x + round, rect.max.y - round), 90.0, 180.0, round, bg);
                carve_corner(full, rect.max, Pos2::new(rect.max.x - round, rect.max.y - round), 90.0, 0.0, round, bg);
                full.rect_stroke(rect, Rounding::same(round), Stroke::new(1.0, border()));

                if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::Grab); }
            });
    }
}

/// Fill the area between a square corner and its rounding arc, hiding the square.
fn carve_corner(painter: &egui::Painter, corner: Pos2, center: Pos2, a0: f32, a1: f32, rad: f32, fill: Color32) {
    let mut pts = vec![corner];
    for i in 0..=8 {
        let a = (a0 + (a1 - a0) * i as f32 / 8.0).to_radians();
        pts.push(center + Vec2::new(rad * a.cos(), rad * a.sin()));
    }
    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
}

fn gps(exif: &exif::Exif, tag: exif::Tag, reftag: exif::Tag) -> Option<f64> {
    let f = exif.get_field(tag, exif::In::PRIMARY)?;
    if let exif::Value::Rational(r) = &f.value {
        if r.len() >= 3 {
            let mut d = r[0].to_f64() + r[1].to_f64() / 60.0 + r[2].to_f64() / 3600.0;
            if let Some(rf) = exif.get_field(reftag, exif::In::PRIMARY) {
                let s = rf.display_value().to_string();
                if s.contains('S') || s.contains('W') { d = -d; }
            }
            return Some(d);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_roundtrips() {
        for &(lat, lon, z) in &[(48.8584, 2.2945, 12u8), (-33.8688, 151.2093, 9), (40.7128, -74.0060, 14)] {
            let (x, y) = world_px(lat, lon, z);
            let (la, lo) = px_to_lonlat(x, y, z);
            assert!((la - lat).abs() < 1e-6, "lat roundtrip");
            assert!((lo - lon).abs() < 1e-6, "lon roundtrip");
        }
    }
    #[test]
    fn haversine_paris_london() {
        let d = haversine((48.8566, 2.3522), (51.5074, -0.1278));
        assert!((d - 343.5).abs() < 5.0, "Paris–London ~344 km, got {d}");
    }
}

fn geobtn(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let r = ui.add(egui::Button::new(RichText::new(label).color(text_sec()).size(11.5))
        .fill(bg_input()).stroke(Stroke::new(1.0, border())).rounding(Rounding::same(5.0)));
    if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    r
}
