//! Runtime theming. The palette is a swappable `Palette` value held in a
//! thread-local cell; the rest of the GUI reads colours through the accessor
//! functions (`accent()`, `bg_panel()`, …) so switching themes at runtime just
//! means writing a new `Palette`. Several presets ship built-in, and the accent
//! colour / node size / fonts can be customised on top of any preset.

use eframe::egui::{self, Color32, FontFamily, FontId, Margin, Rounding, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeShape { Circle, Square, Diamond, Hexagon }

impl NodeShape {
    pub const ALL: [NodeShape; 4] = [NodeShape::Circle, NodeShape::Square, NodeShape::Diamond, NodeShape::Hexagon];
    pub fn label(self) -> &'static str {
        match self { NodeShape::Circle=>"Circle", NodeShape::Square=>"Square",
                     NodeShape::Diamond=>"Diamond", NodeShape::Hexagon=>"Hexagon" }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BgStyle { Grid, Dots, Plain }

impl BgStyle {
    pub const ALL: [BgStyle; 3] = [BgStyle::Grid, BgStyle::Dots, BgStyle::Plain];
    pub fn label(self) -> &'static str {
        match self { BgStyle::Grid=>"Grid", BgStyle::Dots=>"Dots", BgStyle::Plain=>"Plain" }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark:        bool,
    pub bg_app:      Color32,
    pub bg_panel:    Color32,
    pub bg_sidebar:  Color32,
    pub bg_canvas:   Color32,
    pub bg_item_sel: Color32,
    pub bg_item_hov: Color32,
    pub bg_input:    Color32,
    pub bg_output:   Color32,
    pub accent:      Color32,
    pub accent_dark: Color32,
    pub accent_hov:  Color32,
    pub text_pri:    Color32,
    pub text_sec:    Color32,
    pub text_mut:    Color32,
    pub border:      Color32,
    pub grid:        Color32,
    pub c_ok:        Color32,
    pub c_err:       Color32,
    pub c_warn:      Color32,
    pub c_info:      Color32,
}

#[derive(Clone, Copy)]
pub struct UiConfig {
    pub node_radius: f32,
    pub show_grid:   bool,
    pub edge_labels: bool,
    pub font_scale:  f32,
    pub node_shape:  NodeShape,
    pub edge_curved: bool,
    pub bg_style:    BgStyle,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            node_radius: 22.0, show_grid: true, edge_labels: true, font_scale: 1.0,
            node_shape: NodeShape::Circle, edge_curved: false, bg_style: BgStyle::Grid,
        }
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 { Color32::from_rgb(r, g, b) }

/// All built-in themes, in display order. `(name, palette)`.
pub const THEMES: &[(&str, fn() -> Palette)] = &[
    ("Anthropic", anthropic),
    ("Midnight",  midnight),
    ("Matrix",    matrix),
    ("Dracula",   dracula),
    ("Nord",      nord),
    ("Solarized", solarized),
    ("Light",     light),
];

pub fn theme_by_name(name: &str) -> Palette {
    THEMES.iter().find(|(n, _)| *n == name).map(|(_, f)| f()).unwrap_or_else(anthropic)
}

pub fn anthropic() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(20,17,15), bg_panel: rgb(26,23,20), bg_sidebar: rgb(13,11,9),
        bg_canvas: rgb(15,13,11), bg_item_sel: rgb(36,30,25), bg_item_hov: rgb(28,24,20),
        bg_input: rgb(30,26,22), bg_output: rgb(12,10,8),
        accent: rgb(217,119,87), accent_dark: rgb(155,82,55), accent_hov: rgb(230,135,100),
        text_pri: rgb(238,230,218), text_sec: rgb(148,136,122), text_mut: rgb(90,80,70),
        border: rgb(40,35,30), grid: rgb(28,24,21),
        c_ok: rgb(95,155,108), c_err: rgb(198,82,72), c_warn: rgb(205,152,60), c_info: rgb(120,155,195),
    }
}

pub fn midnight() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(13,17,28), bg_panel: rgb(18,23,38), bg_sidebar: rgb(10,13,22),
        bg_canvas: rgb(11,15,26), bg_item_sel: rgb(28,38,62), bg_item_hov: rgb(22,29,48),
        bg_input: rgb(22,28,46), bg_output: rgb(8,11,19),
        accent: rgb(96,165,250), accent_dark: rgb(55,98,165), accent_hov: rgb(125,185,255),
        text_pri: rgb(226,232,245), text_sec: rgb(130,145,180), text_mut: rgb(70,82,110),
        border: rgb(34,42,64), grid: rgb(24,30,48),
        c_ok: rgb(74,180,140), c_err: rgb(235,90,95), c_warn: rgb(235,180,70), c_info: rgb(120,170,250),
    }
}

pub fn matrix() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(6,10,7), bg_panel: rgb(10,16,11), bg_sidebar: rgb(4,7,5),
        bg_canvas: rgb(5,9,6), bg_item_sel: rgb(16,32,18), bg_item_hov: rgb(12,22,13),
        bg_input: rgb(10,18,11), bg_output: rgb(3,6,4),
        accent: rgb(74,222,128), accent_dark: rgb(40,120,68), accent_hov: rgb(120,250,160),
        text_pri: rgb(190,240,200), text_sec: rgb(90,150,105), text_mut: rgb(45,80,52),
        border: rgb(24,46,28), grid: rgb(14,28,17),
        c_ok: rgb(90,235,130), c_err: rgb(235,95,90), c_warn: rgb(220,210,80), c_info: rgb(90,200,180),
    }
}

pub fn dracula() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(33,34,44), bg_panel: rgb(40,42,54), bg_sidebar: rgb(28,29,38),
        bg_canvas: rgb(30,31,41), bg_item_sel: rgb(60,62,82), bg_item_hov: rgb(48,50,66),
        bg_input: rgb(45,47,62), bg_output: rgb(24,25,33),
        accent: rgb(189,147,249), accent_dark: rgb(120,90,165), accent_hov: rgb(210,175,255),
        text_pri: rgb(248,248,242), text_sec: rgb(150,152,170), text_mut: rgb(98,100,120),
        border: rgb(58,60,78), grid: rgb(44,46,60),
        c_ok: rgb(80,250,123), c_err: rgb(255,85,85), c_warn: rgb(241,250,140), c_info: rgb(139,233,253),
    }
}

pub fn nord() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(46,52,64), bg_panel: rgb(59,66,82), bg_sidebar: rgb(40,45,56),
        bg_canvas: rgb(44,50,62), bg_item_sel: rgb(76,86,106), bg_item_hov: rgb(67,76,94),
        bg_input: rgb(64,72,89), bg_output: rgb(36,41,51),
        accent: rgb(136,192,208), accent_dark: rgb(76,120,138), accent_hov: rgb(160,215,230),
        text_pri: rgb(236,239,244), text_sec: rgb(160,170,186), text_mut: rgb(106,116,134),
        border: rgb(76,86,106), grid: rgb(54,61,76),
        c_ok: rgb(163,190,140), c_err: rgb(191,97,106), c_warn: rgb(235,203,139), c_info: rgb(129,161,193),
    }
}

pub fn solarized() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(0,43,54), bg_panel: rgb(7,54,66), bg_sidebar: rgb(0,36,46),
        bg_canvas: rgb(2,40,50), bg_item_sel: rgb(20,72,84), bg_item_hov: rgb(12,60,72),
        bg_input: rgb(10,58,70), bg_output: rgb(0,30,39),
        accent: rgb(38,139,210), accent_dark: rgb(28,98,148), accent_hov: rgb(80,170,225),
        text_pri: rgb(220,228,222), text_sec: rgb(131,148,150), text_mut: rgb(78,98,102),
        border: rgb(20,68,80), grid: rgb(8,52,63),
        c_ok: rgb(133,153,0), c_err: rgb(220,50,47), c_warn: rgb(181,137,0), c_info: rgb(42,161,152),
    }
}

pub fn light() -> Palette {
    Palette {
        dark: false,
        bg_app: rgb(245,242,237), bg_panel: rgb(252,250,246), bg_sidebar: rgb(238,234,227),
        bg_canvas: rgb(248,245,240), bg_item_sel: rgb(232,222,210), bg_item_hov: rgb(240,234,225),
        bg_input: rgb(255,253,249), bg_output: rgb(250,248,244),
        accent: rgb(200,95,60), accent_dark: rgb(160,72,45), accent_hov: rgb(220,115,80),
        text_pri: rgb(40,34,28), text_sec: rgb(110,100,88), text_mut: rgb(165,156,144),
        border: rgb(214,206,194), grid: rgb(228,221,210),
        c_ok: rgb(60,135,80), c_err: rgb(190,55,48), c_warn: rgb(170,120,20), c_info: rgb(60,110,170),
    }
}

thread_local! {
    static CUR:  Cell<Palette>  = Cell::new(anthropic());
    static CONF: Cell<UiConfig> = Cell::new(UiConfig::default());
}

pub fn current() -> Palette { CUR.with(|c| c.get()) }
pub fn set_palette(p: Palette) { CUR.with(|c| c.set(p)); }
pub fn config() -> UiConfig { CONF.with(|c| c.get()) }
pub fn set_config(c: UiConfig) { CONF.with(|cell| cell.set(c)); }

// ── Accessors (used everywhere instead of constants) ───────────────────────────
pub fn bg_app()      -> Color32 { current().bg_app }
pub fn bg_panel()    -> Color32 { current().bg_panel }
pub fn bg_sidebar()  -> Color32 { current().bg_sidebar }
pub fn bg_canvas()   -> Color32 { current().bg_canvas }
pub fn bg_item_sel() -> Color32 { current().bg_item_sel }
pub fn bg_item_hov() -> Color32 { current().bg_item_hov }
pub fn bg_input()    -> Color32 { current().bg_input }
pub fn bg_output()   -> Color32 { current().bg_output }
pub fn accent()      -> Color32 { current().accent }
pub fn accent_dark() -> Color32 { current().accent_dark }
pub fn accent_hov()  -> Color32 { current().accent_hov }
pub fn text_pri()    -> Color32 { current().text_pri }
pub fn text_sec()    -> Color32 { current().text_sec }
pub fn text_mut()    -> Color32 { current().text_mut }
pub fn border()      -> Color32 { current().border }
pub fn grid()        -> Color32 { current().grid }
pub fn c_ok()        -> Color32 { current().c_ok }
pub fn c_err()       -> Color32 { current().c_err }
pub fn c_warn()      -> Color32 { current().c_warn }
pub fn c_info()      -> Color32 { current().c_info }

pub fn node_radius() -> f32       { config().node_radius }
pub fn show_grid()   -> bool      { config().show_grid }
pub fn edge_labels() -> bool      { config().edge_labels }
pub fn node_shape()  -> NodeShape { config().node_shape }
pub fn edge_curved() -> bool      { config().edge_curved }
pub fn bg_style()    -> BgStyle   { config().bg_style }

/// Install fonts once (idempotent enough to call again is fine).
pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("dejavu".to_owned(),
        egui::FontData::from_static(include_bytes!("fonts/DejaVuSans.ttf")));
    fonts.font_data.insert("dejavu_mono".to_owned(),
        egui::FontData::from_static(include_bytes!("fonts/DejaVuSansMono.ttf")));
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "dejavu".to_owned());
    fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "dejavu_mono".to_owned());
    ctx.set_fonts(fonts);
}

/// Apply the current palette + config to egui's visuals and text styles.
pub fn apply(ctx: &egui::Context) {
    let p = current();
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill        = p.bg_app;
    v.window_fill       = p.bg_panel;
    v.extreme_bg_color  = p.bg_output;
    v.faint_bg_color    = p.bg_sidebar;
    v.code_bg_color     = p.bg_input;
    v.window_stroke     = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.bg_fill   = p.bg_panel;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.bg_fill   = p.bg_input;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text_sec);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.hovered.bg_fill    = p.bg_item_hov;
    v.widgets.hovered.fg_stroke  = Stroke::new(1.0, p.accent_dark);
    v.widgets.hovered.bg_stroke  = Stroke::new(1.0, p.accent_dark);
    v.widgets.active.bg_fill     = p.accent_dark;
    v.widgets.active.fg_stroke   = Stroke::new(1.0, p.accent);
    v.selection.bg_fill          = p.accent_dark;
    v.selection.stroke           = Stroke::new(1.0, p.accent);
    v.hyperlink_color            = p.accent;
    v.override_text_color        = Some(p.text_pri);
    v.window_rounding            = Rounding::same(6.0);
    ctx.set_visuals(v);

    let s = config().font_scale;
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (egui::TextStyle::Heading,   FontId::new(18.0 * s, FontFamily::Proportional)),
        (egui::TextStyle::Body,      FontId::new(13.5 * s, FontFamily::Proportional)),
        (egui::TextStyle::Small,     FontId::new(11.0 * s, FontFamily::Proportional)),
        (egui::TextStyle::Button,    FontId::new(13.5 * s, FontFamily::Proportional)),
        (egui::TextStyle::Monospace, FontId::new(12.5 * s, FontFamily::Monospace)),
    ].into();
    style.spacing.item_spacing   = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.window_margin  = Margin::same(0.0);
    ctx.set_style(style);
}

/// Initial setup: fonts + default theme.
pub fn setup(ctx: &egui::Context) {
    setup_fonts(ctx);
    apply(ctx);
}
