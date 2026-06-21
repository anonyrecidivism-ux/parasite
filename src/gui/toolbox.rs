//! Toolbox mode — a grab-bag of offline OSINT utilities: encoders, hashers, a
//! JWT decoder, username/name variant generators, a Google-dork builder and a
//! coordinate converter. Everything runs locally, no network, no keys.

use eframe::egui::{self, Color32, FontId, Margin, RichText, Rounding, ScrollArea, Stroke};

use super::i18n;
use super::theme::*;

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Base64, Url, Hex, Hash, Jwt, UserVariants, Dork, Coord,
}

impl Tool {
    const ALL: [Tool; 8] = [Tool::Base64, Tool::Url, Tool::Hex, Tool::Hash,
        Tool::Jwt, Tool::UserVariants, Tool::Dork, Tool::Coord];
    fn label(self) -> &'static str {
        match self {
            Tool::Base64 => "Base64", Tool::Url => "URL", Tool::Hex => "Hex",
            Tool::Hash => "Hashes", Tool::Jwt => "JWT decode", Tool::UserVariants => "Username variants",
            Tool::Dork => "Dork builder", Tool::Coord => "Coordinates",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Tool::Base64 => "◈", Tool::Url => "❖", Tool::Hex => "⬢", Tool::Hash => "#",
            Tool::Jwt => "❖", Tool::UserVariants => "@", Tool::Dork => "⊙", Tool::Coord => "◎",
        }
    }
}

pub struct ToolboxPanel {
    tool:   Tool,
    input:  String,
    output: String,
}

impl ToolboxPanel {
    pub fn new() -> Self {
        Self { tool: Tool::Base64, input: String::new(), output: String::new() }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("toolbox_tools")
            .resizable(false).default_width(190.0)
            .frame(egui::Frame::none().fill(bg_sidebar()).inner_margin(Margin::same(10.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    super::logo::widget(ui, 8.0);
                    ui.add_space(4.0);
                    ui.label(RichText::new(i18n::tr("tab.toolbox").to_uppercase()).color(text_mut()).size(10.0).strong());
                });
                ui.add_space(8.0);
                for t in Tool::ALL {
                    let active = self.tool == t;
                    let r = ui.add_sized([ui.available_width(), 28.0], egui::Button::new(
                        RichText::new(format!("{}  {}", t.icon(), t.label()))
                            .color(if active { text_pri() } else { text_sec() }).size(12.5))
                        .fill(if active { bg_item_sel() } else { Color32::TRANSPARENT })
                        .stroke(if active { Stroke::new(1.0, accent_dark()) } else { Stroke::NONE })
                        .rounding(Rounding::same(5.0)));
                    if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                    if r.clicked() { self.tool = t; self.output.clear(); }
                    ui.add_space(3.0);
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_app()).inner_margin(Margin::symmetric(20.0, 16.0)))
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("{}  {}", self.tool.icon(), self.tool.label()))
                    .color(text_pri()).strong().size(18.0));
                ui.add_space(2.0);
                ui.label(RichText::new(self.hint()).color(text_sec()).size(11.5));
                ui.add_space(10.0);

                // action buttons (tool-specific)
                ui.horizontal_wrapped(|ui| {
                    for (label, act) in self.actions() {
                        if ui.add(egui::Button::new(RichText::new(label).color(Color32::WHITE).strong().size(12.0))
                            .fill(accent()).rounding(Rounding::same(corner()))).clicked()
                        {
                            self.output = act(self.input.trim());
                        }
                    }
                    if !self.output.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new(i18n::tr("tb.copy")).color(text_sec()).size(11.5))
                                .fill(bg_input()).stroke(Stroke::new(1.0, border())).rounding(Rounding::same(corner()))).clicked()
                            {
                                ui.output_mut(|o| o.copied_text = self.output.clone());
                            }
                        });
                    }
                });
                ui.add_space(10.0);

                ui.label(RichText::new(i18n::tr("tb.input")).color(text_mut()).size(10.0).strong());
                ui.add(egui::TextEdit::multiline(&mut self.input).desired_width(f32::INFINITY)
                    .desired_rows(5).font(FontId::monospace(12.5)));
                ui.add_space(12.0);
                ui.label(RichText::new(i18n::tr("tb.output")).color(text_mut()).size(10.0).strong());
                egui::Frame::none().fill(bg_output()).rounding(Rounding::same(corner()))
                    .inner_margin(Margin::symmetric(10.0, 8.0)).stroke(Stroke::new(1.0, border()))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                            let mut out = self.output.clone();
                            ui.add(egui::TextEdit::multiline(&mut out).desired_width(f32::INFINITY)
                                .font(FontId::monospace(12.5)).text_color(accent()).frame(false)
                                .interactive(true));
                        });
                    });
            });
    }

    fn hint(&self) -> &'static str {
        match self.tool {
            Tool::Base64 => "Encode or decode Base64 text.",
            Tool::Url => "Percent-encode or decode a URL component.",
            Tool::Hex => "Encode text to hex, or decode hex back to text.",
            Tool::Hash => "MD5 / SHA-1 / SHA-256 of the input.",
            Tool::Jwt => "Paste a JWT — header & payload are base64url-decoded.",
            Tool::UserVariants => "Generate username variants from a name (e.g. 'John Doe').",
            Tool::Dork => "Build Google dork queries for a domain or keyword.",
            Tool::Coord => "Convert decimal degrees ⇄ DMS. Input: 'lat, lon'.",
        }
    }

    #[allow(clippy::type_complexity)]
    fn actions(&self) -> Vec<(&'static str, fn(&str) -> String)> {
        match self.tool {
            Tool::Base64 => vec![("Encode", b64_enc), ("Decode", b64_dec)],
            Tool::Url => vec![("Encode", url_enc), ("Decode", url_dec)],
            Tool::Hex => vec![("Encode", hex_enc), ("Decode", hex_dec)],
            Tool::Hash => vec![("Hash all", hash_all)],
            Tool::Jwt => vec![("Decode", jwt_decode)],
            Tool::UserVariants => vec![("Generate", user_variants)],
            Tool::Dork => vec![("Build dorks", dork_build)],
            Tool::Coord => vec![("Convert", coord_convert)],
        }
    }
}

// ── tool implementations ──────────────────────────────────────────────────────

use base64::Engine;

fn b64_enc(s: &str) -> String { base64::engine::general_purpose::STANDARD.encode(s.as_bytes()) }
fn b64_dec(s: &str) -> String {
    match base64::engine::general_purpose::STANDARD.decode(s.trim()) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => format!("✗ not valid Base64: {e}"),
    }
}

fn url_enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}
fn url_dec(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) { out.push(b); i += 3; continue; }
        }
        if bytes[i] == b'+' { out.push(b' '); } else { out.push(bytes[i]); }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_enc(s: &str) -> String { s.bytes().map(|b| format!("{b:02x}")).collect() }
fn hex_dec(s: &str) -> String {
    let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if clean.len() % 2 != 0 { return "✗ odd number of hex digits".into(); }
    let bytes: Vec<u8> = (0..clean.len()).step_by(2)
        .filter_map(|i| u8::from_str_radix(&clean[i..i + 2], 16).ok()).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn hash_all(s: &str) -> String {
    use md5::{Md5, Digest};
    use sha1::Sha1;
    use sha2::Sha256;
    let md5 = { let mut h = Md5::new(); h.update(s.as_bytes()); hexs(&h.finalize()) };
    let sha1 = { let mut h = Sha1::new(); h.update(s.as_bytes()); hexs(&h.finalize()) };
    let sha256 = { let mut h = Sha256::new(); h.update(s.as_bytes()); hexs(&h.finalize()) };
    format!("MD5    {md5}\nSHA1   {sha1}\nSHA256 {sha256}")
}
fn hexs(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }

fn jwt_decode(s: &str) -> String {
    let parts: Vec<&str> = s.trim().split('.').collect();
    if parts.len() < 2 { return "✗ not a JWT (need header.payload.signature)".into(); }
    let dec = |p: &str| match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(p) {
        Ok(b) => {
            let txt = String::from_utf8_lossy(&b);
            serde_json::from_str::<serde_json::Value>(&txt)
                .ok().and_then(|v| serde_json::to_string_pretty(&v).ok())
                .unwrap_or_else(|| txt.into_owned())
        }
        Err(_) => "(could not decode)".into(),
    };
    format!("── HEADER ──\n{}\n\n── PAYLOAD ──\n{}", dec(parts[0]), dec(parts[1]))
}

fn user_variants(s: &str) -> String {
    let parts: Vec<String> = s.split_whitespace().map(|w| w.to_lowercase()).collect();
    if parts.is_empty() { return String::new(); }
    let mut v = std::collections::BTreeSet::new();
    if parts.len() == 1 {
        let a = &parts[0];
        v.insert(a.clone());
        v.insert(format!("{a}1")); v.insert(format!("{a}123")); v.insert(format!("_{a}_"));
        v.insert(format!("{a}.official")); v.insert(format!("real{a}")); v.insert(format!("{a}_"));
    } else {
        let (f, l) = (&parts[0], &parts[parts.len() - 1]);
        let fi = f.chars().next().unwrap_or(' ');
        let li = l.chars().next().unwrap_or(' ');
        for s in [format!("{f}{l}"), format!("{f}.{l}"), format!("{f}_{l}"), format!("{f}-{l}"),
                  format!("{fi}{l}"), format!("{f}{li}"), format!("{l}{f}"), format!("{l}.{f}"),
                  format!("{f}{l}1"), format!("{f}.{l}.official"), format!("the{f}{l}"),
                  format!("{fi}.{l}"), format!("{f}{li}{}", l.len())] {
            v.insert(s);
        }
    }
    v.into_iter().collect::<Vec<_>>().join("\n")
}

fn dork_build(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() { return "enter a domain or keyword".into(); }
    let is_domain = t.contains('.') && !t.contains(' ');
    if is_domain {
        [
            format!("site:{t}"),
            format!("site:{t} -www"),
            format!("site:{t} ext:pdf OR ext:doc OR ext:xls"),
            format!("site:{t} intitle:index.of"),
            format!("site:{t} inurl:admin OR inurl:login"),
            format!("site:{t} \"password\" OR \"api_key\" OR \"secret\""),
            format!("site:pastebin.com {t}"),
            format!("site:github.com {t}"),
            format!("\"@{t}\" email"),
        ].join("\n")
    } else {
        [
            format!("intext:\"{t}\""),
            format!("intitle:\"{t}\""),
            format!("\"{t}\" site:linkedin.com"),
            format!("\"{t}\" site:github.com"),
            format!("\"{t}\" filetype:pdf"),
            format!("\"{t}\" (email OR phone OR address)"),
        ].join("\n")
    }
}

fn coord_convert(s: &str) -> String {
    // try decimal "lat, lon"
    let nums: Vec<f64> = s.split([',', ' ', ';']).filter_map(|p| p.trim().parse::<f64>().ok()).collect();
    if nums.len() == 2 {
        let (lat, lon) = (nums[0], nums[1]);
        return format!("Decimal: {lat:.6}, {lon:.6}\nDMS:     {}  {}\nMaps:    https://maps.google.com/?q={lat},{lon}",
            dms(lat, true), dms(lon, false));
    }
    "✗ enter coordinates as 'lat, lon' (decimal degrees)".into()
}
fn dms(deg: f64, is_lat: bool) -> String {
    let hemi = if is_lat { if deg >= 0.0 { 'N' } else { 'S' } } else if deg >= 0.0 { 'E' } else { 'W' };
    let a = deg.abs();
    let d = a.trunc() as i64;
    let m = ((a - d as f64) * 60.0).trunc() as i64;
    let s = (a - d as f64 - m as f64 / 60.0) * 3600.0;
    format!("{d}°{m}'{s:.2}\"{hemi}")
}
