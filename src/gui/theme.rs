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
pub enum UiVariant { Standard, Compact, Focus }

impl UiVariant {
    pub const ALL: [UiVariant; 3] = [UiVariant::Standard, UiVariant::Compact, UiVariant::Focus];
    pub fn label(self) -> &'static str {
        match self { UiVariant::Standard=>"Standard", UiVariant::Compact=>"Compact", UiVariant::Focus=>"Focus (no palette)" }
    }
    pub fn palette_width(self) -> f32 { match self { UiVariant::Compact=>166.0, _=>210.0 } }
    pub fn details_width(self) -> f32 { match self { UiVariant::Compact=>234.0, UiVariant::Focus=>280.0, UiVariant::Standard=>290.0 } }
    pub fn show_palette(self) -> bool { self != UiVariant::Focus }
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
    pub variant:     UiVariant,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            node_radius: 22.0, show_grid: true, edge_labels: true, font_scale: 1.0,
            node_shape: NodeShape::Circle, edge_curved: false, bg_style: BgStyle::Grid,
            variant: UiVariant::Standard,
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
    ("Cyberpunk", cyberpunk),
    ("Ocean",     ocean),
    ("Rosé",      rose),
    ("Amber",     amber),
    ("Mono",      mono),
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

pub fn cyberpunk() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(13,11,20), bg_panel: rgb(20,16,32), bg_sidebar: rgb(9,8,15),
        bg_canvas: rgb(11,9,18), bg_item_sel: rgb(40,20,55), bg_item_hov: rgb(28,18,42),
        bg_input: rgb(26,18,40), bg_output: rgb(7,6,12),
        accent: rgb(255,60,172), accent_dark: rgb(150,30,100), accent_hov: rgb(255,110,200),
        text_pri: rgb(230,225,245), text_sec: rgb(120,210,230), text_mut: rgb(90,70,120),
        border: rgb(48,30,70), grid: rgb(30,20,46),
        c_ok: rgb(60,240,180), c_err: rgb(255,70,110), c_warn: rgb(245,220,70), c_info: rgb(90,200,255),
    }
}

pub fn ocean() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(8,22,30), bg_panel: rgb(12,30,40), bg_sidebar: rgb(6,18,25),
        bg_canvas: rgb(9,25,34), bg_item_sel: rgb(20,52,66), bg_item_hov: rgb(15,40,52),
        bg_input: rgb(14,38,50), bg_output: rgb(5,15,21),
        accent: rgb(64,196,200), accent_dark: rgb(36,120,124), accent_hov: rgb(110,220,224),
        text_pri: rgb(220,238,240), text_sec: rgb(120,165,175), text_mut: rgb(64,98,108),
        border: rgb(26,58,70), grid: rgb(16,40,50),
        c_ok: rgb(90,200,150), c_err: rgb(225,95,95), c_warn: rgb(230,190,90), c_info: rgb(96,180,220),
    }
}

pub fn rose() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(26,16,20), bg_panel: rgb(34,21,26), bg_sidebar: rgb(20,12,15),
        bg_canvas: rgb(22,14,18), bg_item_sel: rgb(56,30,38), bg_item_hov: rgb(42,24,30),
        bg_input: rgb(40,24,30), bg_output: rgb(16,10,12),
        accent: rgb(244,114,150), accent_dark: rgb(160,64,90), accent_hov: rgb(250,150,180),
        text_pri: rgb(244,228,234), text_sec: rgb(180,140,152), text_mut: rgb(110,76,86),
        border: rgb(58,34,42), grid: rgb(40,24,30),
        c_ok: rgb(150,200,130), c_err: rgb(230,90,100), c_warn: rgb(235,180,100), c_info: rgb(190,150,220),
    }
}

pub fn amber() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(22,18,10), bg_panel: rgb(30,24,13), bg_sidebar: rgb(16,13,7),
        bg_canvas: rgb(19,15,9), bg_item_sel: rgb(52,40,16), bg_item_hov: rgb(38,30,14),
        bg_input: rgb(36,28,14), bg_output: rgb(13,10,6),
        accent: rgb(245,180,60), accent_dark: rgb(160,116,32), accent_hov: rgb(255,200,100),
        text_pri: rgb(244,236,216), text_sec: rgb(176,158,118), text_mut: rgb(104,90,60),
        border: rgb(58,46,22), grid: rgb(40,32,16),
        c_ok: rgb(150,190,90), c_err: rgb(228,110,70), c_warn: rgb(240,200,80), c_info: rgb(150,180,210),
    }
}

pub fn mono() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(18,18,18), bg_panel: rgb(26,26,26), bg_sidebar: rgb(12,12,12),
        bg_canvas: rgb(15,15,15), bg_item_sel: rgb(46,46,46), bg_item_hov: rgb(34,34,34),
        bg_input: rgb(32,32,32), bg_output: rgb(10,10,10),
        accent: rgb(220,220,220), accent_dark: rgb(120,120,120), accent_hov: rgb(245,245,245),
        text_pri: rgb(235,235,235), text_sec: rgb(150,150,150), text_mut: rgb(90,90,90),
        border: rgb(48,48,48), grid: rgb(32,32,32),
        c_ok: rgb(150,200,150), c_err: rgb(210,110,110), c_warn: rgb(220,200,120), c_info: rgb(140,170,210),
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
#[allow(dead_code)]
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
pub fn variant()     -> UiVariant { config().variant }

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
    let compact = config().variant == UiVariant::Compact;
    style.spacing.item_spacing   = if compact { Vec2::new(6.0, 3.0) } else { Vec2::new(8.0, 6.0) };
    style.spacing.button_padding = if compact { Vec2::new(7.0, 3.0) } else { Vec2::new(10.0, 5.0) };
    style.spacing.window_margin  = Margin::same(0.0);
    ctx.set_style(style);
}

/// Initial setup: fonts + default theme.
#[allow(dead_code)]
pub fn setup(ctx: &egui::Context) {
    setup_fonts(ctx);
    apply(ctx);
}
