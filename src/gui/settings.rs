//! User-customisable settings: theme, accent colour, node size, grid, fonts —
//! persisted to `~/.config/parasite/settings.json` so they survive restarts.

use eframe::egui::{self, Color32};
use serde::{Deserialize, Serialize};

use super::theme::{self, UiConfig};

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub theme:       String,
    pub accent:      Option<[u8; 3]>,
    pub node_radius: f32,
    pub show_grid:   bool,
    pub edge_labels: bool,
    pub font_scale:  f32,
    pub welcomed:    bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "Anthropic".into(),
            accent: None,
            node_radius: 22.0,
            show_grid: true,
            edge_labels: true,
            font_scale: 1.0,
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
        let mut pal = theme::theme_by_name(&self.theme);
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
        });
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
