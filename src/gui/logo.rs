//! The parasite logo (v5) — three nested organic "cell" blobs — ported from
//! `parasite_logo_v5.svg` to direct egui painting so it always adopts the active
//! theme's accent colour, and can be animated (a gentle breathing pulse) for
//! "thinking" indicators.

use egui::{self, Color32, Pos2, Stroke, Vec2};

use super::theme::*;

// Reference frame of the source SVG (blobs are concentric around this point).
const CX: f32 = 355.0;
const CY: f32 = 336.0;
const REF: f32 = 196.0; // outer reach in SVG units (a little margin so nothing clips)

/// Each blob is a closed cubic-bezier path: a start point followed by triples of
/// (control1, control2, end). Outer → inner.
const OUTER: &[(f32, f32)] = &[
    (340.0,158.0), (430.0,152.0),(510.0,200.0),(522.0,290.0),
    (534.0,375.0),(488.0,460.0),(400.0,488.0), (316.0,514.0),(220.0,484.0),(188.0,400.0),
    (156.0,318.0),(190.0,210.0),(260.0,175.0), (288.0,160.0),(314.0,160.0),(340.0,158.0),
];
const MIDDLE: &[(f32, f32)] = &[
    (320.0,198.0), (390.0,190.0),(458.0,234.0),(464.0,306.0),
    (470.0,372.0),(432.0,438.0),(362.0,456.0), (294.0,472.0),(222.0,440.0),(202.0,372.0),
    (184.0,308.0),(220.0,228.0),(280.0,204.0), (296.0,196.0),(308.0,198.0),(320.0,198.0),
];
const INNER: &[(f32, f32)] = &[
    (348.0,248.0), (394.0,244.0),(432.0,276.0),(436.0,322.0),
    (440.0,366.0),(412.0,406.0),(368.0,416.0), (326.0,426.0),(280.0,404.0),(268.0,362.0),
    (256.0,322.0),(278.0,272.0),(316.0,254.0), (328.0,248.0),(338.0,248.0),(348.0,248.0),
];

/// Mix a colour toward another by `t` (0 = a, 1 = b).
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn cubic(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, t: f32) -> Pos2 {
    let u = 1.0 - t;
    let a = u * u * u; let b = 3.0 * u * u * t; let c = 3.0 * u * t * t; let d = t * t * t;
    Pos2::new(a * p0.x + b * p1.x + c * p2.x + d * p3.x,
              a * p0.y + b * p1.y + c * p2.y + d * p3.y)
}

/// Sample a blob path into a filled polygon and paint it, scaled by `pulse`
/// around `center`.
fn blob(painter: &egui::Painter, pts: &[(f32, f32)], color: Color32,
        center: Pos2, s: f32, pulse: f32) {
    let tp = |x: f32, y: f32| {
        let p = Pos2::new(center.x + (x - CX) * s, center.y + (y - CY) * s);
        center + (p - center) * pulse
    };
    let mut poly = Vec::with_capacity(64);
    let mut start = tp(pts[0].0, pts[0].1);
    let mut i = 1;
    while i + 2 < pts.len() {
        let c1 = tp(pts[i].0, pts[i].1);
        let c2 = tp(pts[i + 1].0, pts[i + 1].1);
        let end = tp(pts[i + 2].0, pts[i + 2].1);
        for k in 0..10 { poly.push(cubic(start, c1, c2, end, k as f32 / 10.0)); }
        start = end;
        i += 3;
    }
    painter.add(egui::Shape::convex_polygon(poly, color, Stroke::NONE));
}

/// Paint the logo centred at `center` with the given outer `radius` (px). `t` is
/// an animation phase (0 = static); pass `ui.input(|i| i.time)` to breathe.
pub fn paint_t(painter: &egui::Painter, center: Pos2, radius: f32, t: f64) {
    let s = radius / REF;
    let accent = accent();
    let bg = bg_panel();
    let outer = accent;
    let middle = mix(accent, bg, 0.42);
    let inner = mix(accent, bg, 0.70);

    let anim = t != 0.0;

    // expanding "sonar" rings — a living-cell ripple, not moving dots (kept inside
    // the widget box so nothing is clipped)
    if anim {
        for k in 0..3 {
            let tt = ((t * 0.85 + k as f64 * 0.34).rem_euclid(1.0)) as f32;
            let rr = radius * (0.5 + tt * 0.55);
            let al = ((1.0 - tt).powf(1.4) * 90.0) as u8;
            painter.circle_stroke(center, rr,
                Stroke::new(2.0, Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), al)));
        }
    }

    // breathing pulse — a single coherent heartbeat across the nested blobs
    let beat = if anim {
        // ease-y double-throb heartbeat in [0,1]
        let x = (t * 1.4).rem_euclid(1.0) as f32;
        let h = (-(x * 9.0 - 1.0).powi(2)).exp() + 0.5 * (-(x * 9.0 - 3.2).powi(2)).exp();
        h
    } else { 0.0 };
    blob(painter, OUTER,  outer,  center, s, 1.0 + 0.025 * beat);
    blob(painter, MIDDLE, middle, center, s, 1.0 + 0.06 * beat);
    blob(painter, INNER,  inner,  center, s, 1.0 + 0.11 * beat);

    // tiny highlight glint on the inner blob
    let glint = mix(accent, Color32::WHITE, 0.5);
    painter.circle_filled(center + Vec2::new(-radius * 0.18, -radius * 0.18), radius * 0.07, glint);
}

/// Static paint (back-compat).
pub fn paint(painter: &egui::Painter, center: Pos2, radius: f32) {
    paint_t(painter, center, radius, 0.0);
}

/// Sample a blob path into a polygon in SVG coordinates.
fn blob_poly(pts: &[(f32, f32)]) -> Vec<Pos2> {
    let mut poly = Vec::new();
    let mut start = Pos2::new(pts[0].0, pts[0].1);
    let mut i = 1;
    while i + 2 < pts.len() {
        let c1 = Pos2::new(pts[i].0, pts[i].1);
        let c2 = Pos2::new(pts[i + 1].0, pts[i + 1].1);
        let end = Pos2::new(pts[i + 2].0, pts[i + 2].1);
        for k in 0..12 { poly.push(cubic(start, c1, c2, end, k as f32 / 12.0)); }
        start = end; i += 3;
    }
    poly
}

fn inside(poly: &[Pos2], p: Pos2) -> bool {
    let mut c = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if ((a.y > p.y) != (b.y > p.y))
            && (p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x) { c = !c; }
        j = i;
    }
    c
}

/// Rasterise the logo into an RGBA buffer (transparent background) for the window
/// / taskbar icon — three nested accent-tinted blobs, matching the painted look.
pub fn icon_rgba(size: usize) -> Vec<u8> {
    let accent = accent();
    let bg = bg_panel();
    let layers = [
        (blob_poly(OUTER),  accent),
        (blob_poly(MIDDLE), mix(accent, bg, 0.42)),
        (blob_poly(INNER),  mix(accent, bg, 0.70)),
    ];
    let half = size as f32 / 2.0;
    let s = (half * 0.94) / REF;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let p = Pos2::new(CX + (x as f32 - half) / s, CY + (y as f32 - half) / s);
            let mut col: Option<Color32> = None;
            for (poly, c) in &layers { if inside(poly, p) { col = Some(*c); } }
            if let Some(c) = col {
                let i = (y * size + x) * 4;
                rgba[i] = c.r(); rgba[i + 1] = c.g(); rgba[i + 2] = c.b(); rgba[i + 3] = 255;
            }
        }
    }
    rgba
}

/// Allocate a square and paint the static logo; returns the response.
pub fn widget(ui: &mut egui::Ui, radius: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(radius * 2.0), egui::Sense::click());
    paint(ui.painter(), rect.center(), radius);
    resp
}

/// Allocate a square and paint the *animated* (breathing + orbiting) logo; keeps
/// repainting. The box is a little larger than 2·radius so the orbit isn't clipped.
pub fn widget_anim(ui: &mut egui::Ui, radius: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(radius * 2.4), egui::Sense::hover());
    paint_t(ui.painter(), rect.center(), radius, ui.input(|i| i.time));
    ui.ctx().request_repaint();
    resp
}
