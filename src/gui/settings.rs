//! User-customisable settings: theme, accent colour, node size, grid, fonts —
//! persisted to `~/.config/parasite/settings.json` so they survive restarts.

use egui::{self, Color32};
use serde::{Deserialize, Serialize};

use super::i18n::{self, Lang};
use super::keys::{self, ApiKeys};
use super::theme::{self, BgStyle, Design, NodeShape, NodeStyle, UiConfig, UiVariant};

fn d_shape() -> NodeShape  { NodeShape::Circle }
fn d_bg()    -> BgStyle    { BgStyle::Grid }
fn d_var()   -> UiVariant  { UiVariant::Standard }
fn d_design()-> Design     { Design::Cupertino }
fn d_style() -> NodeStyle  { NodeStyle::Flat }
fn d_sens()  -> f32        { 0.6 }

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme:       String,
    pub accent:      Option<[u8; 3]>,
    pub node_radius: f32,
    pub show_grid:   bool,
    pub edge_labels: bool,
    pub font_scale:  f32,
    #[serde(default = "d_shape")]
    pub node_shape:  NodeShape,
    #[serde(default)]
    pub edge_curved: bool,
    #[serde(default = "d_bg")]
    pub bg_style:    BgStyle,
    #[serde(default = "d_var")]
    pub variant:     UiVariant,
    #[serde(default = "d_ew")]
    pub edge_width:  f32,
    #[serde(default = "d_true")]
    pub node_labels: bool,
    #[serde(default = "d_ls")]
    pub label_size:  f32,
    #[serde(default = "d_true")]
    pub show_icons:  bool,
    #[serde(default)]
    pub color_clusters: bool,
    #[serde(default = "d_style")]
    pub node_style:  NodeStyle,
    #[serde(default = "d_sens")]
    pub map_sensitivity: f32,
    #[serde(default)]
    pub api:         ApiKeys,
    #[serde(default)]
    pub lang:        Lang,
    #[serde(default)]
    pub glow:        bool,
    #[serde(default = "d_true")]
    pub animations:  bool,
    #[serde(default = "d_one")]
    pub anim_speed:  f32,
    // network policy
    #[serde(default)]
    pub proxy:       String,
    #[serde(default = "d_ddg")]
    pub search_engine: String,
    #[serde(default)]
    pub block_http:  bool,
    /// path to a custom UI font (.ttf/.otf); empty = bundled DejaVu
    #[serde(default)]
    pub font_path:   String,
    #[serde(default = "d_design")]
    pub design:      Design,
    pub welcomed:    bool,
}

fn d_one() -> f32 { 1.0 }
fn d_ddg() -> String { "duckduckgo".into() }

fn d_ew()   -> f32  { 1.3 }
fn d_ls()   -> f32  { 12.0 }
fn d_true() -> bool { true }

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "Parasite".into(),
            accent: None,
            node_radius: 22.0,
            show_grid: true,
            edge_labels: true,
            font_scale: 1.0,
            node_shape: NodeShape::Circle,
            edge_curved: false,
            bg_style: BgStyle::Grid,
            variant: UiVariant::Standard,
            edge_width: 1.3,
            node_labels: true,
            label_size: 12.0,
            show_icons: true,
            color_clusters: false,
            node_style: NodeStyle::Flat,
            map_sensitivity: 0.6,
            api: ApiKeys::default(),
            lang: Lang::default(),
            glow: false,
            animations: true,
            anim_speed: 1.0,
            proxy: String::new(),
            search_engine: "duckduckgo".into(),
            block_http: false,
            font_path: String::new(),
            design: Design::Cupertino,
            welcomed: false,
        }
    }
}

fn config_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
    Some(base.join("parasite").join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = config_path() {
            if let Some(dir) = p.parent() { let _ = std::fs::create_dir_all(dir); }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, json);
            }
        }
    }

    /// Resolve the effective palette (preset + accent override) and push it,
    /// along with the UI config, into the theme module, then repaint visuals.
    pub fn apply(&self, ctx: &egui::Context) {
        theme::set_design(self.design);
        // the retro design pins its own palette; otherwise the theme drives colours
        let mut pal = self.design.forces_palette()
            .unwrap_or_else(|| theme::theme_by_name(&self.theme));
        if let Some([r, g, b]) = self.accent {
            pal.accent = Color32::from_rgb(r, g, b);
            pal.accent_dark = scale(pal.accent, 0.62);
            pal.accent_hov  = lighten(pal.accent, 0.18);
        }
        theme::set_palette(pal);
        theme::set_config(UiConfig {
            node_radius: self.node_radius.clamp(10.0, 48.0),
            show_grid:   self.show_grid,
            edge_labels: self.edge_labels,
            font_scale:  self.font_scale.clamp(0.7, 1.6),
            node_shape:  self.node_shape,
            edge_curved: self.edge_curved,
            edge_width:  self.edge_width.clamp(0.5, 5.0),
            node_labels: self.node_labels,
            label_size:  self.label_size.clamp(8.0, 20.0),
            show_icons:  self.show_icons,
            color_clusters: self.color_clusters,
            node_style:  self.node_style,
            map_sensitivity: self.map_sensitivity.clamp(0.15, 2.5),
            bg_style:    self.bg_style,
            variant:     self.variant,
            glow:        self.glow,
            animations:  self.animations,
            anim_speed:  self.anim_speed.clamp(0.25, 3.0),
        });
        keys::set(self.api.clone());
        i18n::set_lang(self.lang);
        super::net::set(self.proxy.clone(), self.search_engine.clone(), self.block_http);
        theme::apply(ctx);
    }
}

fn scale(c: Color32, f: f32) -> Color32 {
    Color32::from_rgb(
        (c.r() as f32 * f) as u8,
        (c.g() as f32 * f) as u8,
        (c.b() as f32 * f) as u8,
    )
}

fn lighten(c: Color32, f: f32) -> Color32 {
    let l = |v: u8| (v as f32 + (255.0 - v as f32) * f) as u8;
    Color32::from_rgb(l(c.r()), l(c.g()), l(c.b()))
}
