//! The parasite "virus" logo, ported from `parasite_virus_icon_theme.svg` to
//! direct egui painting so it always adopts the active theme's colours
//! (danger→accent, primary→text, secondary→text_sec, background→panel).

use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};

use super::theme::*;

// Reference frame of the source SVG.
const CX: f32 = 340.0;
const CY: f32 = 200.0;
const REF: f32 = 175.0; // outer reach (spike tips) in SVG units

/// Paint the logo centred at `center` with the given outer `radius` (px).
pub fn paint(painter: &egui::Painter, center: Pos2, radius: f32) {
    let s = radius / REF;
    let tp = |x: f32, y: f32| Pos2::new(center.x + (x - CX) * s, center.y + (y - CY) * s);

    let body = accent();
    let sec  = text_sec();
    let pri  = text_pri();
    let bg   = bg_panel();

    // soft danger glow
    painter.circle_filled(center, 158.0 * s,
        Color32::from_rgba_unmultiplied(body.r(), body.g(), body.b(), 16));

    // 8 radiating spikes (theme primary)
    for k in 0..8 {
        let th = std::f32::consts::FRAC_PI_4 * k as f32;
        let u = Vec2::new(th.sin(), -th.cos());
        painter.line_segment([center + u * (110.0 * s), center + u * (148.0 * s)],
            Stroke::new(14.0 * s, pri));
        painter.circle_filled(center + u * (158.0 * s), 12.0 * s, pri);
    }

    // circuit-style trailing connections (theme secondary)
    let conn = |pts: &[(f32, f32)], dot: (f32, f32)| {
        let line: Vec<Pos2> = pts.iter().map(|(x, y)| tp(*x, *y)).collect();
        painter.add(egui::Shape::line(line, Stroke::new(3.0 * s, sec)));
        painter.circle_filled(tp(dot.0, dot.1), 5.0 * s, sec);
    };
    conn(&[(340.0, 80.0), (340.0, 30.0)], (340.0, 25.0));
    conn(&[(460.0, 150.0), (510.0, 130.0)], (516.0, 127.0));
    conn(&[(460.0, 260.0), (505.0, 290.0), (505.0, 330.0)], (505.0, 337.0));
    conn(&[(220.0, 260.0), (175.0, 290.0), (175.0, 330.0)], (175.0, 337.0));
    conn(&[(220.0, 150.0), (170.0, 130.0)], (164.0, 127.0));
    conn(&[(280.0, 320.0), (260.0, 365.0)], (255.0, 372.0));

    // virus body + hollow core
    painter.circle_filled(center, 110.0 * s, body);
    painter.circle_filled(center, 96.0 * s, bg);

    // infected spots
    for (x, y, r) in [(295.0,155.0,22.0),(385.0,170.0,16.0),(315.0,245.0,24.0),
                      (390.0,240.0,13.0),(355.0,195.0,10.0),(300.0,205.0,8.0)] {
        painter.circle_filled(tp(x, y), r * s, body);
    }
    // highlights punched out of the spots
    for (x, y, r) in [(290.0,150.0,9.0),(382.0,167.0,6.0),(310.0,240.0,10.0),(388.0,237.0,5.0)] {
        painter.circle_filled(tp(x, y), r * s, bg);
    }
}

/// Allocate a square and paint the logo into it; returns the response.
pub fn widget(ui: &mut egui::Ui, radius: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(radius * 2.0), egui::Sense::click());
    paint(ui.painter(), rect.center(), radius);
    resp
}
