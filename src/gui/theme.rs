//! Runtime theming. The palette is a swappable `Palette` value held in a
//! thread-local cell; the rest of the GUI reads colours through the accessor
//! functions (`accent()`, `bg_panel()`, …) so switching themes at runtime just
//! means writing a new `Palette`. Several presets ship built-in, and the accent
//! colour / node size / fonts can be customised on top of any preset.

use eframe::egui::{self, Color32, FontFamily, FontId, Margin, Rounding, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeShape { Circle, Square, Diamond, Triangle, Pentagon, Hexagon, Heptagon, Octagon, Star, Plus, ByType }

impl NodeShape {
    pub const ALL: [NodeShape; 11] = [
        NodeShape::Circle, NodeShape::Square, NodeShape::Diamond, NodeShape::Triangle,
        NodeShape::Pentagon, NodeShape::Hexagon, NodeShape::Heptagon, NodeShape::Octagon,
        NodeShape::Star, NodeShape::Plus, NodeShape::ByType,
    ];
    pub fn label(self) -> &'static str {
        match self {
            NodeShape::Circle=>"Circle", NodeShape::Square=>"Square",
            NodeShape::Diamond=>"Diamond", NodeShape::Triangle=>"Triangle",
            NodeShape::Pentagon=>"Pentagon", NodeShape::Hexagon=>"Hexagon",
            NodeShape::Heptagon=>"Heptagon", NodeShape::Octagon=>"Octagon",
            NodeShape::Star=>"Star", NodeShape::Plus=>"Plus", NodeShape::ByType=>"By type",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStyle { Flat, Material, Neon, Outline }

impl NodeStyle {
    pub const ALL: [NodeStyle; 4] = [NodeStyle::Flat, NodeStyle::Material, NodeStyle::Neon, NodeStyle::Outline];
    pub fn label(self) -> &'static str {
        match self {
            NodeStyle::Flat=>"Flat",
            NodeStyle::Material=>"Material You", NodeStyle::Neon=>"Neon glow", NodeStyle::Outline=>"Outline",
        }
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

/// The overall interface design — a different shape language + chrome, switchable
/// in Settings. `Stock` is the original Parasite look; `Cupertino` is a clean,
/// light, generously-spaced design (Apple-like, but flat — no liquid glass).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Design { Stock, Cupertino, Maltego }

impl Design {
    pub const ALL: [Design; 3] = [Design::Stock, Design::Cupertino, Design::Maltego];
    pub fn label(self) -> &'static str {
        match self {
            Design::Stock => "Stock (Parasite)",
            Design::Cupertino => "Cupertino — clean & light",
            Design::Maltego => "Retro Unix — old-Linux (Motif/CDE)",
        }
    }
    /// Base corner radius for cards/buttons.
    pub fn corner(self) -> f32 {
        match self { Design::Stock => 6.0, Design::Cupertino => 12.0, Design::Maltego => 0.0 }
    }
    /// (item_spacing, button_padding) — kept modest so menus/combos stay compact.
    fn metrics(self) -> (Vec2, Vec2) {
        match self {
            Design::Stock     => (Vec2::new(8.0, 6.0), Vec2::new(10.0, 5.0)),
            Design::Cupertino => (Vec2::new(9.0, 6.0), Vec2::new(11.0, 6.0)),
            Design::Maltego   => (Vec2::new(6.0, 4.0), Vec2::new(8.0, 4.0)),
        }
    }
    /// The retro design pins its own grey palette (no theme entry needed).
    pub fn forces_palette(self) -> Option<Palette> {
        match self { Design::Maltego => Some(maltego()), _ => None }
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
    pub edge_width:  f32,
    pub node_labels: bool,
    pub label_size:  f32,
    pub show_icons:  bool,
    pub color_clusters: bool,
    pub node_style:  NodeStyle,
    pub map_sensitivity: f32,
    pub bg_style:    BgStyle,
    pub variant:     UiVariant,
    pub glow:        bool,
    pub animations:  bool,
    pub anim_speed:  f32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            node_radius: 22.0, show_grid: true, edge_labels: true, font_scale: 1.0,
            node_shape: NodeShape::Circle, edge_curved: false, edge_width: 1.3,
            node_labels: true, label_size: 12.0, show_icons: true, color_clusters: false,
            node_style: NodeStyle::Flat, map_sensitivity: 0.6,
            bg_style: BgStyle::Grid, variant: UiVariant::Standard,
            glow: false, animations: true, anim_speed: 1.0,
        }
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 { Color32::from_rgb(r, g, b) }

/// All built-in themes, in display order. `(name, palette)`.
pub const THEMES: &[(&str, fn() -> Palette)] = &[
    ("Parasite",  parasite),
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
    ("Tokyo Night", tokyo),
    ("Gruvbox",   gruvbox),
    ("Synthwave", synthwave),
    ("Blood",     blood),
    ("Coffee",    coffee),
    ("Light",     light),
    ("Paper",     paper),
    ("Cupertino", cupertino),
];

pub fn theme_by_name(name: &str) -> Palette {
    THEMES.iter().find(|(n, _)| *n == name).map(|(_, f)| f()).unwrap_or_else(anthropic)
}

/// The official parasite theme — deep warm charcoal with the coral brand accent.
pub fn parasite() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(17,15,14), bg_panel: rgb(25,22,20), bg_sidebar: rgb(13,11,10),
        bg_canvas: rgb(15,13,12), bg_item_sel: rgb(44,33,28), bg_item_hov: rgb(31,26,23),
        bg_input: rgb(29,25,22), bg_output: rgb(11,10,9),
        accent: rgb(232,122,84), accent_dark: rgb(150,73,49), accent_hov: rgb(244,148,114),
        text_pri: rgb(243,237,229), text_sec: rgb(166,152,139), text_mut: rgb(99,88,80),
        border: rgb(42,36,32), grid: rgb(27,23,20),
        c_ok: rgb(108,178,122), c_err: rgb(224,98,86), c_warn: rgb(226,168,74), c_info: rgb(122,166,214),
    }
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

pub fn tokyo() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(26,27,38), bg_panel: rgb(31,33,48), bg_sidebar: rgb(22,22,33),
        bg_canvas: rgb(24,25,36), bg_item_sel: rgb(48,52,76), bg_item_hov: rgb(36,40,60),
        bg_input: rgb(34,37,56), bg_output: rgb(18,19,28),
        accent: rgb(122,162,247), accent_dark: rgb(70,98,165), accent_hov: rgb(150,185,255),
        text_pri: rgb(192,202,245), text_sec: rgb(125,134,175), text_mut: rgb(74,82,115),
        border: rgb(41,46,66), grid: rgb(30,33,49),
        c_ok: rgb(158,206,106), c_err: rgb(247,118,142), c_warn: rgb(224,175,104), c_info: rgb(125,207,255),
    }
}

pub fn gruvbox() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(40,40,40), bg_panel: rgb(50,48,47), bg_sidebar: rgb(29,32,33),
        bg_canvas: rgb(35,34,33), bg_item_sel: rgb(80,73,69), bg_item_hov: rgb(60,56,54),
        bg_input: rgb(54,51,49), bg_output: rgb(24,24,24),
        accent: rgb(254,128,25), accent_dark: rgb(175,88,18), accent_hov: rgb(255,160,80),
        text_pri: rgb(235,219,178), text_sec: rgb(168,153,132), text_mut: rgb(102,92,84),
        border: rgb(80,73,69), grid: rgb(50,48,47),
        c_ok: rgb(184,187,38), c_err: rgb(251,73,52), c_warn: rgb(250,189,47), c_info: rgb(131,165,152),
    }
}

pub fn synthwave() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(22,13,38), bg_panel: rgb(31,18,52), bg_sidebar: rgb(16,9,28),
        bg_canvas: rgb(19,11,33), bg_item_sel: rgb(52,28,82), bg_item_hov: rgb(40,22,66),
        bg_input: rgb(38,20,62), bg_output: rgb(12,7,22),
        accent: rgb(255,113,206), accent_dark: rgb(165,60,130), accent_hov: rgb(255,150,225),
        text_pri: rgb(240,230,255), text_sec: rgb(150,130,200), text_mut: rgb(95,78,135),
        border: rgb(58,34,90), grid: rgb(38,22,62),
        c_ok: rgb(114,253,210), c_err: rgb(255,85,130), c_warn: rgb(255,221,109), c_info: rgb(108,213,255),
    }
}

pub fn blood() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(18,10,10), bg_panel: rgb(28,15,15), bg_sidebar: rgb(13,7,7),
        bg_canvas: rgb(16,9,9), bg_item_sel: rgb(58,24,24), bg_item_hov: rgb(40,18,18),
        bg_input: rgb(36,17,17), bg_output: rgb(10,5,5),
        accent: rgb(229,57,53), accent_dark: rgb(150,34,32), accent_hov: rgb(245,95,90),
        text_pri: rgb(240,224,222), text_sec: rgb(180,140,138), text_mut: rgb(110,76,74),
        border: rgb(56,28,28), grid: rgb(34,18,18),
        c_ok: rgb(150,190,110), c_err: rgb(255,80,70), c_warn: rgb(230,170,80), c_info: rgb(200,130,130),
    }
}

pub fn coffee() -> Palette {
    Palette {
        dark: true,
        bg_app: rgb(28,22,18), bg_panel: rgb(38,30,24), bg_sidebar: rgb(22,17,14),
        bg_canvas: rgb(25,20,16), bg_item_sel: rgb(64,50,38), bg_item_hov: rgb(48,38,30),
        bg_input: rgb(44,35,28), bg_output: rgb(18,14,11),
        accent: rgb(198,148,96), accent_dark: rgb(140,100,62), accent_hov: rgb(220,172,120),
        text_pri: rgb(238,226,210), text_sec: rgb(176,156,134), text_mut: rgb(112,96,80),
        border: rgb(58,46,36), grid: rgb(38,30,24),
        c_ok: rgb(150,180,120), c_err: rgb(214,108,88), c_warn: rgb(216,170,96), c_info: rgb(168,160,200),
    }
}

pub fn paper() -> Palette {
    Palette {
        dark: false,
        bg_app: rgb(250,249,246), bg_panel: rgb(255,255,253), bg_sidebar: rgb(243,241,236),
        bg_canvas: rgb(252,251,248), bg_item_sel: rgb(224,232,240), bg_item_hov: rgb(236,238,240),
        bg_input: rgb(255,255,255), bg_output: rgb(248,248,246),
        accent: rgb(56,118,205), accent_dark: rgb(40,88,160), accent_hov: rgb(80,140,225),
        text_pri: rgb(32,36,42), text_sec: rgb(96,104,114), text_mut: rgb(158,164,172),
        border: rgb(220,222,226), grid: rgb(232,234,238),
        c_ok: rgb(46,140,82), c_err: rgb(198,52,48), c_warn: rgb(176,124,18), c_info: rgb(48,108,190),
    }
}

/// The Cupertino design palette — a calm, light, elevated surface set (white
/// cards on soft gray, subtle hairline borders, deep parasite-green accent).
pub fn cupertino() -> Palette {
    Palette {
        dark: false,
        bg_app: rgb(236,236,239), bg_panel: rgb(255,255,255), bg_sidebar: rgb(245,245,247),
        bg_canvas: rgb(242,242,245), bg_item_sel: rgb(223,240,231), bg_item_hov: rgb(237,237,240),
        bg_input: rgb(255,255,255), bg_output: rgb(247,247,249),
        accent: rgb(28,158,98), accent_dark: rgb(20,118,73), accent_hov: rgb(38,184,118),
        text_pri: rgb(29,29,31), text_sec: rgb(99,99,104), text_mut: rgb(160,160,166),
        border: rgb(210,210,215), grid: rgb(223,223,228),
        c_ok: rgb(40,168,98), c_err: rgb(214,78,72), c_warn: rgb(206,142,32), c_info: rgb(70,128,210),
    }
}

/// Retro old-Linux / Unix workstation palette (Motif / CDE) — classic #c6c6c6
/// grey, square sunken/raised panels, Motif blue selection, black text.
pub fn maltego() -> Palette {
    Palette {
        dark: false,
        bg_app: rgb(198,198,198), bg_panel: rgb(206,206,206), bg_sidebar: rgb(190,190,190),
        bg_canvas: rgb(223,223,219), bg_item_sel: rgb(95,120,180), bg_item_hov: rgb(214,214,214),
        bg_input: rgb(240,240,237), bg_output: rgb(228,228,224),
        accent: rgb(48,86,150), accent_dark: rgb(30,58,110), accent_hov: rgb(70,112,180),
        text_pri: rgb(20,20,20), text_sec: rgb(62,62,62), text_mut: rgb(104,104,104),
        border: rgb(120,120,120), grid: rgb(176,176,176),
        c_ok: rgb(38,118,58), c_err: rgb(172,42,38), c_warn: rgb(150,110,20), c_info: rgb(48,86,150),
    }
}

thread_local! {
    static CUR:  Cell<Palette>  = Cell::new(anthropic());
    static CONF: Cell<UiConfig> = Cell::new(UiConfig::default());
    static DSGN: Cell<Design>   = const { Cell::new(Design::Cupertino) };
}

pub fn current() -> Palette { CUR.with(|c| c.get()) }
pub fn design() -> Design { DSGN.with(|c| c.get()) }
pub fn set_design(d: Design) { DSGN.with(|c| c.set(d)); }
/// Fixed corner radius used by custom cards/buttons (design-aware).
pub fn corner() -> f32 { design().corner() }

/// `#rrggbb` for a colour — used to theme the external ParasiteGoogle browser.
pub fn hex(c: Color32) -> String { format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b()) }
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
pub fn edge_width()  -> f32       { config().edge_width }
pub fn node_labels() -> bool      { config().node_labels }
pub fn label_size()  -> f32       { config().label_size }
pub fn show_icons()  -> bool      { config().show_icons }
pub fn color_clusters() -> bool   { config().color_clusters }
pub fn node_style()  -> NodeStyle { config().node_style }
pub fn map_sensitivity() -> f32   { config().map_sensitivity }
pub fn bg_style()    -> BgStyle   { config().bg_style }
pub fn variant()     -> UiVariant { config().variant }
pub fn glow()        -> bool      { config().glow }
pub fn animations()  -> bool      { config().animations }
pub fn anim_speed()  -> f32       { config().anim_speed }

/// Install fonts once (idempotent enough to call again is fine).
pub fn setup_fonts(ctx: &egui::Context) { apply_font(ctx, ""); }

/// Build the font set: bundled DejaVu always present; an optional user font
/// (a `.ttf`/`.otf` path) is loaded and tried *first* for proportional + mono —
/// e.g. point this at a Noto Emoji / Symbols font to get coloured-symbol glyphs.
pub fn apply_font(ctx: &egui::Context, custom_path: &str) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("dejavu".to_owned(),
        egui::FontData::from_static(include_bytes!("fonts/DejaVuSans.ttf")));
    fonts.font_data.insert("dejavu_mono".to_owned(),
        egui::FontData::from_static(include_bytes!("fonts/DejaVuSansMono.ttf")));
    let prop = fonts.families.get_mut(&FontFamily::Proportional).unwrap();
    prop.insert(0, "dejavu".to_owned());
    let mono = fonts.families.get_mut(&FontFamily::Monospace).unwrap();
    mono.insert(0, "dejavu_mono".to_owned());

    let p = custom_path.trim();
    if !p.is_empty() {
        if let Ok(bytes) = std::fs::read(p) {
            fonts.font_data.insert("custom".to_owned(), egui::FontData::from_owned(bytes));
            fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "custom".to_owned());
            fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "custom".to_owned());
        }
    }
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
    // design shape language: corner radius + window chrome + soft shadow
    let dsg = design();
    let rad = Rounding::same(dsg.corner());
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered,
              &mut v.widgets.active, &mut v.widgets.open] {
        w.rounding = rad;
    }
    v.window_rounding = Rounding::same(dsg.corner() + 4.0);
    v.menu_rounding   = rad;
    if dsg == Design::Cupertino {
        // soft elevation instead of glass; hairline borders
        let sh = egui::epaint::Shadow {
            offset: Vec2::new(0.0, 8.0), blur: 24.0, spread: 0.0,
            color: Color32::from_black_alpha(40),
        };
        v.window_shadow = sh;
        v.popup_shadow  = egui::epaint::Shadow {
            offset: Vec2::new(0.0, 6.0), blur: 18.0, spread: 0.0,
            color: Color32::from_black_alpha(35),
        };
        v.window_stroke = Stroke::new(0.6, p.border);
    } else {
        v.window_shadow = egui::epaint::Shadow::NONE;
        v.popup_shadow  = egui::epaint::Shadow::NONE;
    }
    // Retro Unix (Motif/CDE): square chunky widgets with a visible bevel-grey
    // outline on every control + window.
    if dsg == Design::Maltego {
        let bevel = rgb(96, 96, 96);
        let light = rgb(238, 238, 238);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, bevel);
        v.widgets.inactive.bg_stroke = Stroke::new(1.6, bevel);
        v.widgets.inactive.bg_fill   = p.bg_panel;
        v.widgets.hovered.bg_stroke  = Stroke::new(1.6, p.text_pri);
        v.widgets.active.bg_stroke   = Stroke::new(1.6, light);
        v.window_stroke = Stroke::new(1.6, bevel);
        v.selection.bg_fill = p.bg_item_sel;
        v.selection.stroke  = Stroke::new(1.0, p.accent_dark);
    }
    ctx.set_visuals(v);

    let s = config().font_scale;
    let mut style = (*ctx.style()).clone();
    // Retro Unix uses a monospace UI font everywhere — that 90s terminal feel.
    let ui_fam = if dsg == Design::Maltego { FontFamily::Monospace } else { FontFamily::Proportional };
    style.text_styles = [
        (egui::TextStyle::Heading,   FontId::new(18.0 * s, ui_fam.clone())),
        (egui::TextStyle::Body,      FontId::new(13.0 * s, ui_fam.clone())),
        (egui::TextStyle::Small,     FontId::new(11.0 * s, ui_fam.clone())),
        (egui::TextStyle::Button,    FontId::new(13.0 * s, ui_fam)),
        (egui::TextStyle::Monospace, FontId::new(12.5 * s, FontFamily::Monospace)),
    ].into();
    let compact = config().variant == UiVariant::Compact;
    let (isp, bpad) = dsg.metrics();
    style.spacing.item_spacing   = if compact { Vec2::new(6.0, 3.0) } else { isp };
    style.spacing.button_padding = if compact { Vec2::new(7.0, 3.0) } else { bpad };
    style.spacing.window_margin  = Margin::same(0.0);
    ctx.set_style(style);
}

/// Initial setup: fonts + default theme.
#[allow(dead_code)]
pub fn setup(ctx: &egui::Context) {
    setup_fonts(ctx);
    apply(ctx);
}
