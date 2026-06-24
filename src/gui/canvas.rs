//! The graph canvas — pan, zoom, drag, multi-select (marquee), plus a
//! force-directed tidy.

use std::collections::HashSet;

use egui::{self, Color32, FontFamily, FontId, Pos2, Rect, Rounding, Sense, Stroke, Vec2};
use egui::epaint::Mesh;

use super::model::Graph;
use super::theme::*;

/// The current selection: a set of node ids plus the "primary" (last-clicked)
/// one used for the details panel.
#[derive(Default)]
pub struct Selection {
    pub set:     HashSet<u64>,
    pub primary: Option<u64>,
    /// A selected edge (index into graph.edges), if any.
    pub edge:    Option<usize>,
}

impl Selection {
    pub fn select_one(&mut self, id: u64) {
        self.set.clear();
        self.set.insert(id);
        self.primary = Some(id);
        self.edge = None;
    }
    pub fn toggle(&mut self, id: u64) {
        self.edge = None;
        if self.set.remove(&id) {
            if self.primary == Some(id) { self.primary = self.set.iter().next().copied(); }
        } else {
            self.set.insert(id);
            self.primary = Some(id);
        }
    }
    pub fn clear(&mut self) { self.set.clear(); self.primary = None; self.edge = None; }
    pub fn contains(&self, id: u64) -> bool { self.set.contains(&id) }
}

pub struct View {
    pub pan:  Vec2,
    pub zoom: f32,
    drag_node:    Option<u64>,
    marquee_from: Option<Pos2>, // screen-space marquee anchor
    linking_from: Option<u64>,  // node we're drawing a new edge from (Ctrl-drag)
}

impl Default for View {
    fn default() -> Self {
        Self { pan: Vec2::ZERO, zoom: 1.0, drag_node: None, marquee_from: None, linking_from: None }
    }
}

/// What the canvas wants the app to do after a frame.
#[derive(Default)]
pub struct CanvasAction {
    pub run_default: Option<u64>,
    /// (entity id, screen position) where a context menu was requested.
    pub context: Option<(u64, Pos2)>,
    /// A manual edge the user drew (Ctrl-drag from one node to another).
    pub new_link: Option<(u64, u64)>,
}

impl View {
    fn w2s(&self, center: Pos2, p: Pos2) -> Pos2 {
        center + (p.to_vec2() * self.zoom) + self.pan
    }
    fn s2w(&self, center: Pos2, s: Pos2) -> Pos2 {
        ((s - center - self.pan) / self.zoom).to_pos2()
    }

    /// Pan/zoom so the whole graph fits the viewport.
    pub fn fit(&mut self, graph: &Graph, rect: Rect) {
        if graph.entities.is_empty() { self.pan = Vec2::ZERO; self.zoom = 1.0; return; }
        let mut min = Pos2::new(f32::MAX, f32::MAX);
        let mut max = Pos2::new(f32::MIN, f32::MIN);
        for e in graph.entities.values() {
            if !e.pos.x.is_finite() || !e.pos.y.is_finite() { continue; }
            min.x = min.x.min(e.pos.x); min.y = min.y.min(e.pos.y);
            max.x = max.x.max(e.pos.x); max.y = max.y.max(e.pos.y);
        }
        if !min.x.is_finite() || !max.x.is_finite() { self.pan = Vec2::ZERO; self.zoom = 1.0; return; }
        let size = (max - min).max(Vec2::new(1.0, 1.0));
        let pad = 120.0;
        let zx = (rect.width()  - pad) / size.x;
        let zy = (rect.height() - pad) / size.y;
        self.zoom = zx.min(zy).clamp(0.15, 2.5);
        let graph_center = (min.to_vec2() + max.to_vec2()) * 0.5;
        self.pan = -graph_center * self.zoom;
    }
}

pub fn draw(
    ui: &mut egui::Ui,
    graph: &mut Graph,
    view: &mut View,
    sel: &mut Selection,
    images: &std::collections::HashMap<u64, egui::TextureHandle>,
    risk: &std::collections::HashMap<u64, u8>,
    filter: &str,
) -> CanvasAction {
    let mut action = CanvasAction::default();

    let rect = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let shift = ui.input(|i| i.modifiers.shift);
    let ctrl  = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);

    painter.rect_filled(rect, Rounding::ZERO, bg_canvas());
    if show_grid() {
        match bg_style() {
            BgStyle::Grid  => draw_grid(&painter, rect, center, view),
            BgStyle::Dots  => draw_dots(&painter, rect, center, view),
            BgStyle::Plain => {}
        }
    }

    // ── Zoom around the cursor ────────────────────────────────────────────────
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            if let Some(ptr) = ui.input(|i| i.pointer.hover_pos()) {
                let before = view.s2w(center, ptr);
                let factor = (scroll * 0.0015).exp();
                view.zoom = (view.zoom * factor).clamp(0.12, 3.0);
                let after = view.s2w(center, ptr);
                view.pan += (after - before) * view.zoom;
            }
        }
    }

    // ── Hit-test nodes (topmost first by id, newest drawn last) ────────────────
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let hit = pointer.and_then(|p| node_at(graph, view, center, p));

    // ── Begin drag ─────────────────────────────────────────────────────────────
    if response.drag_started() {
        if let Some(id) = hit {
            if ctrl {
                // Ctrl-drag from a node → draw a new edge.
                view.linking_from = Some(id);
            } else {
                // Dragging a node moves the whole selection (select it first if new).
                if !sel.contains(id) {
                    if shift { sel.toggle(id); } else { sel.select_one(id); }
                }
                view.drag_node = Some(id);
            }
        } else if shift {
            // Shift + empty drag → marquee select.
            view.marquee_from = pointer;
        }
        // else: plain empty drag pans (handled below).
    }

    // ── Apply drag ─────────────────────────────────────────────────────────────
    if response.dragged() {
        let delta = response.drag_delta();
        if view.linking_from.is_some() {
            // linking: rubber band is drawn below; nothing moves
        } else if let Some(_id) = view.drag_node {
            // move every selected node
            let ids: Vec<u64> = sel.set.iter().copied().collect();
            for id in ids {
                if let Some(e) = graph.entities.get_mut(&id) {
                    e.pos += delta / view.zoom;
                    e.pinned = true;
                    e.vel = Vec2::ZERO;
                }
            }
        } else if view.marquee_from.is_none() {
            view.pan += delta;
        }
    }

    // ── Rubber-band while linking ───────────────────────────────────────────────
    if let (Some(from), Some(cur)) = (view.linking_from, pointer) {
        if let Some(a) = graph.entities.get(&from) {
            let pa = view.w2s(center, a.pos);
            painter.line_segment([pa, cur], Stroke::new(2.0, accent()));
            painter.circle_stroke(cur, 5.0, Stroke::new(1.5, accent()));
        }
    }

    // ── Draw marquee + select on release ───────────────────────────────────────
    if let (Some(from), Some(cur)) = (view.marquee_from, pointer) {
        let mrect = Rect::from_two_pos(from, cur);
        painter.rect_filled(mrect, Rounding::same(2.0),
            Color32::from_rgba_unmultiplied(accent().r(), accent().g(), accent().b(), 30));
        painter.rect_stroke(mrect, Rounding::same(2.0), Stroke::new(1.0, accent()));
    }
    if response.drag_stopped() {
        // finish a manual link
        if let Some(from) = view.linking_from.take() {
            let drop = pointer.or(ui.input(|i| i.pointer.interact_pos()))
                .and_then(|p| node_at(graph, view, center, p));
            if let Some(to) = drop {
                if to != from { action.new_link = Some((from, to)); }
            }
        }
        if let (Some(from), Some(cur)) = (view.marquee_from, pointer.or(ui.input(|i| i.pointer.interact_pos()))) {
            let mrect = Rect::from_two_pos(from, cur);
            if !shift { sel.clear(); }
            for e in graph.entities.values() {
                if mrect.contains(view.w2s(center, e.pos)) {
                    sel.set.insert(e.id);
                    sel.primary = Some(e.id);
                }
            }
        }
        view.drag_node = None;
        view.marquee_from = None;
    }

    // ── Click / double-click ───────────────────────────────────────────────────
    if response.clicked() {
        match hit {
            Some(id) if shift => sel.toggle(id),
            Some(id)          => sel.select_one(id),
            None => {
                // clicked empty space: maybe an edge?
                if let Some(ei) = pointer.and_then(|p| edge_at(graph, view, center, p)) {
                    sel.set.clear(); sel.primary = None; sel.edge = Some(ei);
                } else {
                    sel.clear();
                }
            }
        }
    }
    if response.double_clicked() {
        if let Some(id) = hit { action.run_default = Some(id); }
    }
    if response.secondary_clicked() {
        if let (Some(id), Some(p)) = (hit, pointer) {
            if !sel.contains(id) { sel.select_one(id); }
            action.context = Some((id, p));
        }
    }

    // ── Spawn-animation clock (also drives edge draw-in) ──────────────────────
    // While recording a video the clock is driven by a virtual timestep (one
    // fixed step per captured frame) so the motion is perfectly smooth in the
    // output regardless of how slow the screenshot capture is.
    const SPAWN_DUR: f64 = 0.45;
    let now = render_time().unwrap_or_else(|| ui.input(|i| i.time));
    for e in graph.entities.values_mut() {
        if e.anim_start.is_none() { e.anim_start = Some(now); }
    }
    let node_prog = |id: u64| -> f32 {
        graph.entities.get(&id)
            .map(|e| (((now - e.anim_start.unwrap_or(now)) / SPAWN_DUR).clamp(0.0, 1.0)) as f32)
            .unwrap_or(1.0)
    };

    // ── Draw edges (tinted link, filled arrowhead, label pill) ──────────────────
    let maltego = design() == Design::Maltego;
    let curved = edge_curved() || maltego; // classic Maltego uses organic curved links
    let nr = node_radius() * view.zoom;
    for (ei, edge) in graph.edges.iter().enumerate() {
        let (Some(a), Some(b)) = (graph.entities.get(&edge.from), graph.entities.get(&edge.to)) else { continue };
        if !finite(a.pos) || !finite(b.pos) { continue; }
        let pa = view.w2s(center, a.pos);
        let pb = view.w2s(center, b.pos);
        let selected = sel.edge == Some(ei);
        // Maltego: neutral grey links; otherwise tint by destination kind
        let ecol = if selected { accent() }
                   else if maltego { blend(border(), text_mut(), 0.35) }
                   else { blend(border(), b.kind.color(), 0.5) };
        let ew = if selected { (edge_width() + 1.4).max(2.4) } else { (edge_width() + 0.2).max(1.1) };
        let estroke = Stroke::new(ew, ecol);

        let ep = node_prog(edge.to).max(0.02); // edge grows in as the child appears
        let (mid, dir) = if curved {
            let chord = pb - pa;
            let perp = Vec2::new(-chord.y, chord.x).normalized();
            let ctrl = pa + chord * 0.5 + perp * (chord.length() * 0.16);
            let steps = (16.0 * ep).ceil().max(1.0) as usize;
            let mut pts = Vec::with_capacity(steps + 1);
            for i in 0..=steps {
                let t = (i as f32 / 16.0).min(ep);
                let u = 1.0 - t;
                pts.push((pa.to_vec2() * (u * u) + ctrl.to_vec2() * (2.0 * u * t) + pb.to_vec2() * (t * t)).to_pos2());
            }
            painter.add(egui::Shape::line(pts, estroke));
            (ctrl, (pb - ctrl).normalized())
        } else {
            let pb_a = pa + (pb - pa) * ep;
            painter.line_segment([pa, pb_a], estroke);
            (pa + (pb - pa) * 0.5, (pb - pa).normalized())
        };

        // filled triangular arrowhead near b — only once the edge has drawn in
        if ep > 0.94 && dir.length() > 0.0 {
            let tip = pb - dir * (nr + 2.5);
            let perp = Vec2::new(-dir.y, dir.x);
            let s = 8.0 * view.zoom.clamp(0.65, 1.4);
            let p1 = tip - dir * s + perp * s * 0.5;
            let p2 = tip - dir * s - perp * s * 0.5;
            painter.add(egui::Shape::convex_polygon(vec![tip, p1, p2], ecol, Stroke::NONE));
        }
        if ep > 0.94 && edge_labels() && view.zoom > 0.7 && !edge.label.is_empty() {
            let g = painter.layout_no_wrap(edge.label.clone(),
                FontId::new(9.5, FontFamily::Proportional), text_mut());
            let sz = g.size();
            let lp = Pos2::new(mid.x - sz.x / 2.0, mid.y - sz.y / 2.0);
            let lbg = bg_app();
            painter.rect_filled(Rect::from_min_size(lp - Vec2::new(4.0, 1.5), sz + Vec2::new(8.0, 3.0)),
                Rounding::same(7.0), Color32::from_rgba_unmultiplied(lbg.r(), lbg.g(), lbg.b(), 220));
            painter.galley(lp, g, text_mut());
        }
    }

    // ── Draw nodes ─────────────────────────────────────────────────────────────
    let r = node_radius() * view.zoom;
    let label_font = FontId::new((label_size() * view.zoom).clamp(8.0, 22.0), FontFamily::Proportional);
    // Maltego makes the type icon the focal point → larger glyph
    let icon_px = if design() == Design::Maltego { 24.0 } else { 18.0 };
    let icon_font  = FontId::new((icon_px * view.zoom).clamp(11.0, 30.0), FontFamily::Proportional);
    let icons_on   = show_icons();
    let maltego_nodes = design() == Design::Maltego;

    // optional cluster colouring (by connected component)
    let comp: std::collections::HashMap<u64, usize> = if color_clusters() {
        components(graph)
    } else { std::collections::HashMap::new() };

    // ── Spawn animation: pop-in scale (clock computed above for edges too) ─────
    let mut animating = false;

    // stable draw order
    let mut ids: Vec<u64> = graph.entities.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let e = &graph.entities[&id];
        let p = view.w2s(center, e.pos);
        if !rect.expand(60.0).contains(p) { continue; }

        // spawn-in scale (ease-out-back overshoot) — skipped when animations are off,
        // sped up/slowed by the animation-speed setting
        let prog = if animations() {
            ((now - e.anim_start.unwrap_or(now)) * anim_speed() as f64 / SPAWN_DUR).clamp(0.0, 1.0) as f32
        } else { 1.0 };
        if prog < 1.0 { animating = true; }
        let grow = ease_out_back(prog);
        let r = r * grow;
        let grown = prog > 0.55;

        // graph filter: dim (and skip) nodes that don't match the query
        if !filter.is_empty()
            && !e.value.to_lowercase().contains(filter)
            && !e.kind.label().to_lowercase().contains(filter)
        {
            painter.circle_filled(p, r * 0.5, rgba(e.kind.color(), 24));
            continue;
        }

        let is_sel = sel.contains(id);
        let is_primary = sel.primary == Some(id);
        let is_hov = hit == Some(id);
        let col = if let Some(&c) = comp.get(&id) { cluster_color(c) } else { e.kind.color() };

        // Cupertino & Maltego designs use uniform round nodes; else per config.
        let shape = if matches!(design(), Design::Cupertino | Design::Maltego) {
            NodeShape::Circle
        } else {
            match node_shape() { NodeShape::ByType => shape_for_kind(e.kind), other => other }
        };
        if is_sel {
            fill_shape(&painter, p, r + 5.0, shape, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 40));
            stroke_shape(&painter, p, r + 4.0, shape, Stroke::new(if is_primary { 2.5 } else { 1.5 }, accent()));
        } else if is_hov {
            stroke_shape(&painter, p, r + 3.0, shape, Stroke::new(1.5, accent_dark()));
        }
        // Instinct risk ring (deterministic risk 0-100 → amber→red)
        let rk = risk.get(&id).copied().unwrap_or(0);
        if rk >= 20 && grown {
            stroke_shape(&painter, p, r + 6.5, shape, Stroke::new(2.4, risk_color(rk)));
        }
        // optional soft glow halo behind the node
        if glow() {
            for (rr, a) in [(r * 2.2, 16u8), (r * 1.7, 26), (r * 1.35, 40)] {
                painter.circle_filled(p, rr, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), a));
            }
        }

        // Maltego renders locations as a red map-pin (very recognisable).
        if maltego_nodes && matches!(e.kind, super::model::Kind::Location | super::model::Kind::Coordinate) {
            draw_pin(&painter, p, r);
            // (label + markers below still apply)
        } else
        // an attached image is rendered as a disc on the node face (clipped round);
        // otherwise the usual shaped body + kind glyph.
        if let Some(tex) = images.get(&id).filter(|_| grown) {
            draw_node_body(&painter, p, r, shape, col, node_style());
            let d = Rect::from_center_size(p, Vec2::splat(r * 1.9));
            let mut rs = egui::epaint::RectShape::filled(d, Rounding::same(r), Color32::WHITE);
            rs.fill_texture_id = tex.id();
            rs.uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
            painter.add(rs);
            stroke_shape(&painter, p, r, shape, Stroke::new(1.5, col));
        } else {
            draw_node_body(&painter, p, r, shape, col, node_style());
            if icons_on && grown {
                if maltego_nodes {
                    // exact Maltego-style vector pictogram per entity type
                    maltego_icon(&painter, p, r * 0.6, e.kind, col);
                } else {
                    painter.text(p, egui::Align2::CENTER_CENTER, e.kind.icon(), icon_font.clone(), col);
                }
            }
        }
        // flag badge (top-right)
        if let Some(fc) = super::model::flag_color(e.flag) {
            let bp = p + Vec2::new(r * 0.72, -r * 0.72);
            painter.circle_filled(bp, (r * 0.28).max(3.0), fc);
            painter.circle_stroke(bp, (r * 0.28).max(3.0), Stroke::new(1.0, bg_panel()));
        }
        // Maltego-style note marker (a small yellow page on the right of the icon)
        if maltego_nodes && grown && !e.note.is_empty() {
            let np = p + Vec2::new(r * 0.74, r * 0.2);
            let s = (r * 0.26).max(3.0);
            painter.rect_filled(Rect::from_center_size(np, Vec2::splat(s * 1.4)),
                Rounding::same(1.0), Color32::from_rgb(245, 214, 90));
            painter.rect_stroke(Rect::from_center_size(np, Vec2::splat(s * 1.4)),
                Rounding::same(1.0), Stroke::new(1.0, Color32::from_rgb(150, 120, 20)));
        }

        if grown && node_labels() && view.zoom > 0.45 {
            let label: String = {
                let v = &e.value;
                if v.chars().count() > 28 { format!("{}…", v.chars().take(27).collect::<String>()) }
                else { v.clone() }
            };
            let lcol = if maltego_nodes { text_pri() } else if is_sel { text_pri() } else { text_sec() };
            let galley = painter.layout_no_wrap(label, label_font.clone(), lcol);
            let sz = galley.size();
            let lp = Pos2::new(p.x - sz.x / 2.0, p.y + r + 5.0);
            // Maltego shows plain dark labels (no chip); other designs use a chip for contrast
            if !maltego_nodes {
                let lbg = bg_output();
                painter.rect_filled(
                    Rect::from_min_size(lp - Vec2::new(5.0, 2.0), sz + Vec2::new(10.0, 4.0)),
                    Rounding::same(3.0),
                    Color32::from_rgba_unmultiplied(lbg.r(), lbg.g(), lbg.b(), 210),
                );
            }
            painter.galley(lp, galley, lcol);
        }
    }

    // Maltego-style entity/link count, bottom-right of the canvas
    let count = format!("{} entities · {} links", graph.entities.len(), graph.edges.len());
    let gz = painter.layout_no_wrap(count, FontId::new(11.0, FontFamily::Proportional), text_mut());
    let gp = Pos2::new(rect.max.x - gz.size().x - 12.0, rect.max.y - gz.size().y - 8.0);
    let bg = bg_app();
    painter.rect_filled(Rect::from_min_size(gp - Vec2::new(6.0, 3.0), gz.size() + Vec2::new(12.0, 6.0)),
        Rounding::same(4.0), Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 200));
    painter.galley(gp, gz, text_mut());

    // keep animating the spawn-in
    if animating { ui.ctx().request_repaint(); }

    // hover cursor
    if hit.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    action
}

/// Ease-out-back: overshoots slightly past 1.0 then settles — a "pop".
fn ease_out_back(t: f32) -> f32 {
    if t >= 1.0 { return 1.0; }
    let c1 = 1.70158_f32;
    let c3 = c1 + 1.0;
    let x = t - 1.0;
    1.0 + c3 * x * x * x + c1 * x * x
}

/// Hit-test edges: returns the index of the edge whose segment is closest to
/// `p` within a small threshold.
fn edge_at(graph: &Graph, view: &View, center: Pos2, p: Pos2) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, e) in graph.edges.iter().enumerate() {
        let (Some(a), Some(b)) = (graph.entities.get(&e.from), graph.entities.get(&e.to)) else { continue };
        if !finite(a.pos) || !finite(b.pos) { continue; }
        let pa = view.w2s(center, a.pos);
        let pb = view.w2s(center, b.pos);
        let d = dist_to_segment(p, pa, pb);
        if d <= 6.0 && best.map_or(true, |(_, bd)| d < bd) { best = Some((i, d)); }
    }
    best.map(|(i, _)| i)
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 <= 1e-6 { return p.distance(a); }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    let proj = a + ab * t;
    p.distance(proj)
}

fn node_at(graph: &Graph, view: &View, center: Pos2, p: Pos2) -> Option<u64> {
    let r = node_radius() * view.zoom;
    let mut best: Option<(u64, f32)> = None;
    for e in graph.entities.values() {
        let sp = view.w2s(center, e.pos);
        let d = sp.distance(p);
        if d <= r + 2.0 {
            if best.map_or(true, |(_, bd)| d < bd) { best = Some((e.id, d)); }
        }
    }
    best.map(|(id, _)| id)
}

/// A small overview map in the bottom-right corner of `area`, showing all nodes
/// and the current viewport rectangle.
/// Draw a Maltego-style vector pictogram for an entity kind, centred at `c`,
/// half-size `s`, in colour `col`. Replaces glyphs in the Retro/Maltego design so
/// every entity type has a recognisable figure.
fn maltego_icon(painter: &egui::Painter, c: Pos2, s: f32, kind: super::model::Kind, col: Color32) {
    use super::model::Kind::*;
    let w = (s * 0.16).max(1.5);
    let st = Stroke::new(w, col);
    let pt = |x: f32, y: f32| Pos2::new(c.x + x * s, c.y + y * s);
    let line = |a: Pos2, b: Pos2| painter.line_segment([a, b], st);
    let poly = |pts: Vec<Pos2>| { painter.add(egui::Shape::closed_line(pts, st)); };

    let globe = || {
        painter.circle_stroke(c, s, st);
        line(pt(-1.0, 0.0), pt(1.0, 0.0));
        // two meridians (thin vertical ellipses approximated by polylines)
        for sx in [0.5_f32, -0.5] {
            let pts: Vec<Pos2> = (0..=10).map(|i| {
                let t = i as f32 / 10.0; let a = std::f32::consts::PI * (t - 0.5);
                pt(sx * a.cos(), (a).sin())
            }).collect();
            painter.add(egui::Shape::line(pts, Stroke::new(w * 0.8, col)));
        }
        line(pt(0.0, -1.0), pt(0.0, 1.0));
    };
    let monitor = || {
        poly(vec![pt(-1.0, -0.7), pt(1.0, -0.7), pt(1.0, 0.45), pt(-1.0, 0.45)]);
        line(pt(-0.35, 0.45), pt(-0.5, 0.9)); line(pt(0.35, 0.45), pt(0.5, 0.9));
        line(pt(-0.55, 0.9), pt(0.55, 0.9));
    };
    let person = || {
        painter.circle_stroke(pt(0.0, -0.45), s * 0.42, st);
        let pts: Vec<Pos2> = (0..=12).map(|i| {
            let t = i as f32 / 12.0; let a = std::f32::consts::PI * t;
            pt(0.85 * a.cos(), 0.95 - 0.55 * a.sin())
        }).collect();
        painter.add(egui::Shape::line(pts, st));
        ()
    };
    let page = |fold: bool| {
        poly(vec![pt(-0.7, -1.0), pt(0.4, -1.0), pt(0.7, -0.6), pt(0.7, 1.0), pt(-0.7, 1.0)]);
        if fold { line(pt(0.4, -1.0), pt(0.4, -0.6)); line(pt(0.4, -0.6), pt(0.7, -0.6)); }
        for y in [-0.35_f32, 0.05, 0.45] { line(pt(-0.45, y), pt(0.45, y)); }
    };

    match kind {
        Domain | Website => globe(),
        Ip | OperatingSystem | Netblock => monitor(),
        Email => { poly(vec![pt(-1.0,-0.6), pt(1.0,-0.6), pt(1.0,0.6), pt(-1.0,0.6)]);
                   line(pt(-1.0,-0.6), pt(0.0,0.15)); line(pt(1.0,-0.6), pt(0.0,0.15)); }
        Phone => { poly(vec![pt(-0.5,-1.0), pt(0.5,-1.0), pt(0.5,1.0), pt(-0.5,1.0)]);
                   line(pt(-0.25,-0.75), pt(0.25,-0.75)); painter.circle_filled(pt(0.0,0.72), w, col); }
        Person | Username | Social => person(),
        Organization => { poly(vec![pt(-0.8,-1.0), pt(0.8,-1.0), pt(0.8,1.0), pt(-0.8,1.0)]);
                          for yy in [-0.6_f32,-0.1,0.4] { for xx in [-0.4_f32,0.4] {
                              painter.rect_stroke(Rect::from_center_size(pt(xx,yy), egui::vec2(s*0.28,s*0.28)), Rounding::ZERO, Stroke::new(w*0.7,col)); } } }
        Location | Coordinate => { painter.circle_filled(c, s*0.5, col); }
        EthAddress => poly(vec![pt(0.0,-1.0), pt(0.7,0.0), pt(0.0,1.0), pt(-0.7,0.0)]),
        BtcAddress => {
            painter.circle_stroke(c, s, st); line(pt(0.0,-0.6), pt(0.0,0.6));
            line(pt(-0.2,-0.6), pt(0.25,-0.6)); line(pt(-0.2,0.6), pt(0.25,0.6)); line(pt(-0.2,0.0), pt(0.25,0.0)); }
        Transaction => { line(pt(-0.8,-0.3), pt(0.8,-0.3)); line(pt(0.8,-0.3), pt(0.5,-0.55));
                         line(pt(0.8,0.3), pt(-0.8,0.3)); line(pt(-0.8,0.3), pt(-0.5,0.05)); }
        Cve => { poly(vec![pt(0.0,-1.0), pt(0.85,-0.55), pt(0.6,0.7), pt(0.0,1.0), pt(-0.6,0.7), pt(-0.85,-0.55)]);
                 line(pt(0.0,-0.45), pt(0.0,0.3)); painter.circle_filled(pt(0.0,0.6), w, col); }
        Service => { painter.circle_stroke(c, s*0.55, st);
                     for k in 0..8 { let a = std::f32::consts::TAU*k as f32/8.0;
                         line(pt(0.55*a.cos(),0.55*a.sin()), pt(a.cos(),a.sin())); } }
        Document | File => page(matches!(kind, Document)),
        MacAddress | Port => { painter.rect_stroke(Rect::from_center_size(c, egui::vec2(s*1.2,s*1.2)), Rounding::ZERO, st);
                               for k in 0..3 { let x = -0.4 + k as f32*0.4;
                                   line(pt(x,-0.6), pt(x,-1.0)); line(pt(x,0.6), pt(x,1.0)); } }
        Hash => { line(pt(-0.35,-0.9), pt(-0.55,0.9)); line(pt(0.55,-0.9), pt(0.35,0.9));
                  line(pt(-0.9,-0.3), pt(0.9,-0.3)); line(pt(-0.9,0.3), pt(0.9,0.3)); }
        Asn => poly((0..6).map(|k| { let a = std::f32::consts::TAU*k as f32/6.0 - std::f32::consts::FRAC_PI_2;
                                     pt(a.cos(), a.sin()) }).collect()),
        Phrase => { poly(vec![pt(-1.0,-0.8), pt(1.0,-0.8), pt(1.0,0.5), pt(-0.3,0.5), pt(-0.6,1.0), pt(-0.6,0.5), pt(-1.0,0.5)]);
                    for y in [-0.4_f32,0.0] { line(pt(-0.6,y), pt(0.6,y)); } }
    }
}

/// A Maltego-style red map-pin (teardrop + white dot) centred on `p`.
fn draw_pin(painter: &egui::Painter, p: Pos2, r: f32) {
    let red = Color32::from_rgb(213, 64, 58);
    let top = p - Vec2::new(0.0, r * 0.35);
    // round head
    painter.circle_filled(top, r * 0.78, red);
    // pointed tail (triangle down to the anchor point)
    let tip = p + Vec2::new(0.0, r * 1.05);
    painter.add(egui::Shape::convex_polygon(
        vec![tip, top + Vec2::new(-r * 0.62, r * 0.18), top + Vec2::new(r * 0.62, r * 0.18)],
        red, Stroke::NONE));
    painter.circle_stroke(top, r * 0.78, Stroke::new(1.2, Color32::from_rgb(150, 35, 30)));
    // white centre hole
    painter.circle_filled(top, r * 0.30, Color32::from_rgb(250, 250, 250));
}

/// Risk 0-100 → ring colour (amber at 20 → deep red at 100).
fn risk_color(rk: u8) -> Color32 {
    let t = ((rk as f32 - 20.0) / 80.0).clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32| (a + (b - a) * t) as u8;
    Color32::from_rgb(lerp(228.0, 210.0), lerp(168.0, 56.0), lerp(46.0, 52.0))
}

pub fn draw_minimap(painter: &egui::Painter, graph: &Graph, view: &View, area: Rect) {
    if graph.entities.len() < 2 { return; }
    let mm_size = Vec2::new(168.0, 116.0);
    let mm = Rect::from_min_size(area.right_bottom() - mm_size - Vec2::splat(12.0), mm_size);
    let bg = bg_panel();
    painter.rect_filled(mm, Rounding::same(4.0),
        Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 220));
    painter.rect_stroke(mm, Rounding::same(4.0), Stroke::new(1.0, border()));

    let (mut min, mut max) = (Pos2::new(f32::MAX, f32::MAX), Pos2::new(f32::MIN, f32::MIN));
    for e in graph.entities.values() {
        if !finite(e.pos) { continue; }
        min.x = min.x.min(e.pos.x); min.y = min.y.min(e.pos.y);
        max.x = max.x.max(e.pos.x); max.y = max.y.max(e.pos.y);
    }
    if !min.x.is_finite() || !max.x.is_finite() { return; }
    let wsize = (max - min).max(Vec2::new(1.0, 1.0));
    let pad = 8.0;
    let scale = ((mm.width() - pad * 2.0) / wsize.x).min((mm.height() - pad * 2.0) / wsize.y);
    let used = wsize * scale;
    let off = mm.min + Vec2::splat(pad) + (Vec2::new(mm.width(), mm.height()) - Vec2::splat(pad * 2.0) - used) * 0.5;
    let map = |p: Pos2| off + (p - min) * scale;

    for e in graph.entities.values() {
        if !finite(e.pos) { continue; }
        painter.circle_filled(map(e.pos), 1.6, e.kind.color());
    }
    // viewport rectangle (visible world region)
    let cw = (-view.pan / view.zoom).to_pos2();
    let half = Vec2::new(area.width(), area.height()) * 0.5 / view.zoom;
    let vr = Rect::from_min_max(map(cw - half), map(cw + half)).intersect(mm);
    painter.rect_stroke(vr, Rounding::ZERO, Stroke::new(1.0, accent()));
}

/// Arrange all nodes on a circle.
pub fn circle_layout(graph: &mut Graph) {
    let mut ids: Vec<u64> = graph.entities.keys().copied().collect();
    ids.sort_unstable();
    let n = ids.len();
    if n == 0 { return; }
    let radius = (n as f32 * 26.0).max(160.0);
    for (i, id) in ids.iter().enumerate() {
        let a = std::f32::consts::TAU * i as f32 / n as f32;
        if let Some(e) = graph.entities.get_mut(id) {
            e.pos = Pos2::new(radius * a.cos(), radius * a.sin());
            e.pinned = false;
        }
    }
}

/// Arrange all nodes on a square grid.
pub fn grid_layout(graph: &mut Graph) {
    let mut ids: Vec<u64> = graph.entities.keys().copied().collect();
    ids.sort_unstable();
    let n = ids.len();
    if n == 0 { return; }
    let cols = (n as f32).sqrt().ceil() as usize;
    let step = 130.0;
    let off = (cols as f32 - 1.0) * step * 0.5;
    for (i, id) in ids.iter().enumerate() {
        let (cx, cy) = ((i % cols) as f32, (i / cols) as f32);
        if let Some(e) = graph.entities.get_mut(id) {
            e.pos = Pos2::new(cx * step - off, cy * step - off);
            e.pinned = false;
        }
    }
}

thread_local! {
    /// When `Some`, draw uses this virtual time instead of the wall clock — set
    /// during video recording for deterministic, smooth animation.
    static RENDER_T: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
}
/// Set (or clear) the virtual render clock used while recording a video.
pub fn set_render_time(t: Option<f64>) { RENDER_T.with(|c| c.set(t)); }
fn render_time() -> Option<f64> { RENDER_T.with(|c| c.get()) }

/// Archimedean spiral — compact and orderly.
pub fn spiral_layout(graph: &mut Graph) {
    let mut ids: Vec<u64> = graph.entities.keys().copied().collect();
    ids.sort_unstable();
    for (i, id) in ids.iter().enumerate() {
        let a = i as f32 * 0.5;
        let r = 36.0 + a * 15.0;
        if let Some(e) = graph.entities.get_mut(id) {
            e.pos = Pos2::new(r * a.cos(), r * a.sin());
            e.pinned = false;
        }
    }
}

/// Deterministic scatter — pseudo-random but stable per node id.
pub fn scatter_layout(graph: &mut Graph) {
    let ids: Vec<u64> = graph.entities.keys().copied().collect();
    let span = (ids.len() as f32).sqrt().max(2.0) * 95.0;
    for id in &ids {
        let hx = id.wrapping_mul(2654435761) % 100000;
        let hy = id.wrapping_mul(40503).wrapping_add(7) % 100000;
        if let Some(e) = graph.entities.get_mut(id) {
            e.pos = Pos2::new(hx as f32 / 100000.0 * span - span / 2.0,
                              hy as f32 / 100000.0 * span - span / 2.0);
            e.pinned = false;
        }
    }
}

/// Columns grouped by entity kind (Maltego "block"-style ordering).
pub fn columns_layout(graph: &mut Graph) {
    let mut kinds: Vec<super::model::Kind> = Vec::new();
    let mut ids: Vec<u64> = graph.entities.keys().copied().collect();
    ids.sort_unstable();
    for &id in &ids {
        let k = graph.entities[&id].kind;
        if !kinds.contains(&k) { kinds.push(k); }
    }
    let colw = 210.0;
    let rowh = 92.0;
    let xoff = (kinds.len() as f32 - 1.0) * colw * 0.5;
    for (ci, &k) in kinds.iter().enumerate() {
        let col_ids: Vec<u64> = ids.iter().copied().filter(|i| graph.entities[i].kind == k).collect();
        let yoff = (col_ids.len() as f32 - 1.0) * rowh * 0.5;
        for (ri, id) in col_ids.iter().enumerate() {
            if let Some(e) = graph.entities.get_mut(id) {
                e.pos = Pos2::new(ci as f32 * colw - xoff, ri as f32 * rowh - yoff);
                e.pinned = false;
            }
        }
    }
}

/// Assign a connected-component index to every node (BFS over undirected edges).
pub fn components(graph: &Graph) -> std::collections::HashMap<u64, usize> {
    let adj = undirected_adj(graph);
    let mut comp = std::collections::HashMap::new();
    let mut next = 0usize;
    let mut ids: Vec<u64> = adj.keys().copied().collect();
    ids.sort_unstable();
    for start in ids {
        if comp.contains_key(&start) { continue; }
        let mut q = std::collections::VecDeque::from([start]);
        comp.insert(start, next);
        while let Some(x) = q.pop_front() {
            for &y in &adj[&x] {
                if !comp.contains_key(&y) { comp.insert(y, next); q.push_back(y); }
            }
        }
        next += 1;
    }
    comp
}

/// A distinct colour per cluster index.
pub fn cluster_color(i: usize) -> Color32 {
    const PAL: [Color32; 10] = [
        Color32::from_rgb(217,119,87), Color32::from_rgb(96,165,250), Color32::from_rgb(74,222,128),
        Color32::from_rgb(189,147,249), Color32::from_rgb(235,180,90), Color32::from_rgb(120,210,230),
        Color32::from_rgb(244,114,150), Color32::from_rgb(163,190,140), Color32::from_rgb(255,140,90),
        Color32::from_rgb(150,160,210),
    ];
    PAL[i % PAL.len()]
}

/// Betweenness centrality (Brandes' algorithm) for an undirected graph.
pub fn betweenness(graph: &Graph) -> std::collections::HashMap<u64, f64> {
    use std::collections::{HashMap, VecDeque};
    let adj = undirected_adj(graph);
    let nodes: Vec<u64> = adj.keys().copied().collect();
    let mut bc: HashMap<u64, f64> = nodes.iter().map(|&n| (n, 0.0)).collect();

    for &s in &nodes {
        let mut stack: Vec<u64> = Vec::new();
        let mut pred: HashMap<u64, Vec<u64>> = nodes.iter().map(|&n| (n, Vec::new())).collect();
        let mut sigma: HashMap<u64, f64> = nodes.iter().map(|&n| (n, 0.0)).collect();
        let mut dist: HashMap<u64, i64> = nodes.iter().map(|&n| (n, -1)).collect();
        sigma.insert(s, 1.0);
        dist.insert(s, 0);
        let mut q = VecDeque::from([s]);
        while let Some(v) = q.pop_front() {
            stack.push(v);
            for &w in &adj[&v] {
                if dist[&w] < 0 { dist.insert(w, dist[&v] + 1); q.push_back(w); }
                if dist[&w] == dist[&v] + 1 {
                    *sigma.get_mut(&w).unwrap() += sigma[&v];
                    pred.get_mut(&w).unwrap().push(v);
                }
            }
        }
        let mut delta: HashMap<u64, f64> = nodes.iter().map(|&n| (n, 0.0)).collect();
        while let Some(w) = stack.pop() {
            for &v in &pred[&w] {
                let c = (sigma[&v] / sigma[&w]) * (1.0 + delta[&w]);
                *delta.get_mut(&v).unwrap() += c;
            }
            if w != s { *bc.get_mut(&w).unwrap() += delta[&w]; }
        }
    }
    for v in bc.values_mut() { *v /= 2.0; }
    bc
}

fn undirected_adj(graph: &Graph) -> std::collections::HashMap<u64, Vec<u64>> {
    let mut adj: std::collections::HashMap<u64, Vec<u64>> =
        graph.entities.keys().map(|&k| (k, Vec::new())).collect();
    for e in &graph.edges {
        if adj.contains_key(&e.from) && adj.contains_key(&e.to) {
            adj.get_mut(&e.from).unwrap().push(e.to);
            adj.get_mut(&e.to).unwrap().push(e.from);
        }
    }
    adj
}

/// BFS levels from the highest-degree node → a top-down hierarchy.
pub fn tree_layout(graph: &mut Graph) {
    if graph.entities.len() < 2 { return; }
    let adj = undirected_adj(graph);
    let root = *adj.iter().max_by_key(|(_, v)| v.len()).map(|(k, _)| k).unwrap();

    let mut level: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    let mut queue = std::collections::VecDeque::from([root]);
    level.insert(root, 0);
    while let Some(n) = queue.pop_front() {
        let d = level[&n];
        for &m in &adj[&n] {
            if !level.contains_key(&m) { level.insert(m, d + 1); queue.push_back(m); }
        }
    }
    // place disconnected nodes on a trailing level
    let maxlvl = level.values().copied().max().unwrap_or(0);
    for id in graph.entities.keys().copied().collect::<Vec<_>>() {
        level.entry(id).or_insert(maxlvl + 1);
    }
    // group by level and spread on x
    let mut by_level: std::collections::BTreeMap<usize, Vec<u64>> = std::collections::BTreeMap::new();
    for (id, l) in &level { by_level.entry(*l).or_default().push(*id); }
    let (vstep, hstep) = (130.0, 150.0);
    for (l, ids) in by_level {
        let mut ids = ids; ids.sort_unstable();
        let off = (ids.len() as f32 - 1.0) * hstep * 0.5;
        for (i, id) in ids.iter().enumerate() {
            if let Some(e) = graph.entities.get_mut(id) {
                e.pos = Pos2::new(i as f32 * hstep - off, l as f32 * vstep);
                e.pinned = false;
            }
        }
    }
}

/// Concentric rings by BFS distance from the highest-degree node.
pub fn radial_layout(graph: &mut Graph) {
    if graph.entities.is_empty() { return; }
    let adj = undirected_adj(graph);
    let root = *adj.iter().max_by_key(|(_, v)| v.len()).map(|(k, _)| k).unwrap();
    let mut dist: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    let mut queue = std::collections::VecDeque::from([root]);
    dist.insert(root, 0);
    while let Some(n) = queue.pop_front() {
        let d = dist[&n];
        for &m in &adj[&n] {
            if !dist.contains_key(&m) { dist.insert(m, d + 1); queue.push_back(m); }
        }
    }
    let maxd = dist.values().copied().max().unwrap_or(0);
    for id in graph.entities.keys().copied().collect::<Vec<_>>() {
        dist.entry(id).or_insert(maxd + 1);
    }
    let mut by_ring: std::collections::BTreeMap<usize, Vec<u64>> = std::collections::BTreeMap::new();
    for (id, d) in &dist { by_ring.entry(*d).or_default().push(*id); }
    for (d, ids) in by_ring {
        let mut ids = ids; ids.sort_unstable();
        if d == 0 {
            if let Some(e) = graph.entities.get_mut(&ids[0]) { e.pos = Pos2::ZERO; e.pinned = false; }
            continue;
        }
        let radius = d as f32 * 170.0;
        let n = ids.len();
        for (i, id) in ids.iter().enumerate() {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            if let Some(e) = graph.entities.get_mut(id) {
                e.pos = Pos2::new(radius * a.cos(), radius * a.sin());
                e.pinned = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::auto_layout;
    use super::super::model::{Graph, Kind};
    use egui::Pos2;

    #[test]
    fn layout_stays_finite_with_coincident_nodes() {
        let mut g = Graph::new();
        // many nodes all stacked on the exact same point — used to produce NaN
        let mut ids = Vec::new();
        for i in 0..50 {
            ids.push(g.add(if i % 2 == 0 { Kind::Domain } else { Kind::Ip }, format!("n{i}"), Pos2::ZERO));
        }
        for w in ids.windows(2) { g.link(w[0], w[1], "x"); }
        auto_layout(&mut g);
        for e in g.entities.values() {
            assert!(e.pos.x.is_finite() && e.pos.y.is_finite(), "non-finite position after layout");
        }
    }
}

fn regular(c: Pos2, r: f32, sides: usize) -> Vec<Pos2> {
    (0..sides).map(|i| {
        let a = std::f32::consts::TAU * i as f32 / sides as f32 - std::f32::consts::FRAC_PI_2;
        egui::pos2(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect()
}

/// Resolve the concrete shape for a node, expanding `ByType` to a per-kind shape.
pub fn shape_for_kind(kind: super::model::Kind) -> NodeShape {
    use super::model::Kind::*;
    match kind {
        Domain | Website | Phrase            => NodeShape::Circle,
        Ip | Netblock | MacAddress           => NodeShape::Square,
        Email | Phone                        => NodeShape::Diamond,
        Person | Username | Social           => NodeShape::Hexagon,
        Organization                         => NodeShape::Pentagon,
        Location | Coordinate                => NodeShape::Triangle,
        Cve | Service | OperatingSystem | Port => NodeShape::Octagon,
        BtcAddress | EthAddress              => NodeShape::Star,
        Transaction                          => NodeShape::Plus,
        Asn | File | Document | Hash         => NodeShape::Square,
    }
}

fn rgba(c: Color32, a: u8) -> Color32 { Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a) }
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}
fn lerp4(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

fn ring_pts(p: Pos2, r: f32, shape: NodeShape) -> Vec<Pos2> {
    if shape == NodeShape::Circle {
        (0..30).map(|i| {
            let a = std::f32::consts::TAU * i as f32 / 30.0;
            Pos2::new(p.x + r * a.cos(), p.y + r * a.sin())
        }).collect()
    } else {
        shape_pts(p, r, shape)
    }
}

/// Fill a node shape with a top→bottom vertical gradient (via a triangle mesh).
fn gradient_fill(painter: &egui::Painter, p: Pos2, r: f32, shape: NodeShape, top: Color32, bottom: Color32) {
    let ring = ring_pts(p, r, shape);
    if ring.len() < 3 { fill_shape(painter, p, r, shape, top); return; }
    let ytop = p.y - r;
    let cat = |y: f32| lerp4(top, bottom, ((y - ytop) / (2.0 * r)).clamp(0.0, 1.0));
    let mut m = Mesh::default();
    m.colored_vertex(p, cat(p.y));
    for &pt in &ring { m.colored_vertex(pt, cat(pt.y)); }
    let n = ring.len() as u32;
    for i in 0..n { m.add_triangle(0, 1 + i, 1 + (i + 1) % n); }
    painter.add(egui::Shape::mesh(m));
}

/// Render a node's body in the chosen visual style.
fn draw_node_body(painter: &egui::Painter, p: Pos2, r: f32, shape: NodeShape, col: Color32, style: NodeStyle) {
    let white = Color32::WHITE;
    // Cupertino design overrides node styling entirely: a soft, elevated pastel
    // disc with a hairline tint border — the saturated icon/label sits on top.
    if design() == Design::Cupertino {
        for k in 1..=5 {
            painter.circle_filled(p + Vec2::new(0.0, k as f32 * 1.1 + 1.0), r,
                rgba(Color32::BLACK, 9));
        }
        fill_shape(painter, p, r, shape, blend(white, col, 0.22));
        fill_shape(painter, p, r * 0.95, shape, blend(white, col, 0.34));
        stroke_shape(painter, p, r, shape, Stroke::new(1.2, rgba(col, 130)));
        return;
    }
    // Maltego Classic: a light category-tinted disc with a thin coloured ring; the
    // large type icon (drawn by the caller in the type colour) is the focal point.
    if design() == Design::Maltego {
        painter.circle_filled(p + Vec2::new(0.0, 2.4), r, rgba(Color32::BLACK, 20));
        painter.circle_filled(p + Vec2::new(0.0, 1.2), r, rgba(Color32::BLACK, 12));
        fill_shape(painter, p, r, shape, blend(white, col, 0.22));     // light blue-ish tint
        stroke_shape(painter, p, r, shape, Stroke::new(1.0, rgba(Color32::BLACK, 35)));
        stroke_shape(painter, p, r - 1.4, shape, Stroke::new(2.2, col));
        return;
    }
    match style {
        NodeStyle::Flat => {
            // soft drop shadow → depth (the biggest "professional" upgrade)
            for k in 1..=3 {
                fill_shape(painter, p + Vec2::new(0.0, k as f32 * 1.1 + 0.5), r, shape, rgba(Color32::BLACK, 11));
            }
            fill_shape(painter, p, r, shape, bg_panel());
            stroke_shape(painter, p, r, shape, Stroke::new(2.2, col));
            stroke_shape(painter, p, r - 2.6, shape, Stroke::new(1.0, rgba(col, 55)));
        }
        NodeStyle::Material => {
            // soft elevation shadow (stacked translucent layers)
            for k in 0..4 {
                fill_shape(painter, p + Vec2::new(0.0, r * 0.10 + k as f32 * 1.6),
                    r + 1.5, shape, rgba(Color32::BLACK, 22));
            }
            // tonal surface with a gentle gradient (Material You "surface tint")
            let surf = blend(bg_panel(), col, 0.30);
            gradient_fill(painter, p, r, shape, blend(surf, white, 0.14), surf);
            stroke_shape(painter, p, r, shape, Stroke::new(1.0, rgba(white, 36)));
        }
        NodeStyle::Neon => {
            // glow halo: several rings of decreasing alpha
            for k in (1..=5).rev() {
                let a = (40 / k).min(34) as u8;
                stroke_shape(painter, p, r + k as f32 * 2.4, shape, Stroke::new(3.0, rgba(col, a)));
            }
            fill_shape(painter, p, r, shape, blend(bg_app(), col, 0.06));
            stroke_shape(painter, p, r, shape, Stroke::new(2.2, col));
            stroke_shape(painter, p, r - 3.0, shape, Stroke::new(1.0, rgba(col, 150)));
        }
        NodeStyle::Outline => {
            fill_shape(painter, p, r, shape, rgba(bg_canvas(), 230));
            stroke_shape(painter, p, r, shape, Stroke::new(2.0, col));
        }
    }
}

fn shape_pts(c: Pos2, r: f32, shape: NodeShape) -> Vec<Pos2> {
    match shape {
        NodeShape::Square => {
            let k = r * 0.9;
            vec![egui::pos2(c.x - k, c.y - k), egui::pos2(c.x + k, c.y - k),
                 egui::pos2(c.x + k, c.y + k), egui::pos2(c.x - k, c.y + k)]
        }
        NodeShape::Diamond  => vec![
            egui::pos2(c.x, c.y - r), egui::pos2(c.x + r, c.y),
            egui::pos2(c.x, c.y + r), egui::pos2(c.x - r, c.y)],
        NodeShape::Triangle => regular(c, r, 3),
        NodeShape::Pentagon => regular(c, r, 5),
        NodeShape::Hexagon  => regular(c, r, 6),
        NodeShape::Heptagon => regular(c, r, 7),
        NodeShape::Octagon  => regular(c, r, 8),
        NodeShape::Star => (0..10).map(|i| {
            let rad = if i % 2 == 0 { r } else { r * 0.42 };
            let a = std::f32::consts::PI / 5.0 * i as f32 - std::f32::consts::FRAC_PI_2;
            egui::pos2(c.x + rad * a.cos(), c.y + rad * a.sin())
        }).collect(),
        NodeShape::Plus => {
            let (o, n) = (r * 0.36, r * 0.95); // arm half-width, length
            vec![
                egui::pos2(c.x - o, c.y - n), egui::pos2(c.x + o, c.y - n),
                egui::pos2(c.x + o, c.y - o), egui::pos2(c.x + n, c.y - o),
                egui::pos2(c.x + n, c.y + o), egui::pos2(c.x + o, c.y + o),
                egui::pos2(c.x + o, c.y + n), egui::pos2(c.x - o, c.y + n),
                egui::pos2(c.x - o, c.y + o), egui::pos2(c.x - n, c.y + o),
                egui::pos2(c.x - n, c.y - o), egui::pos2(c.x - o, c.y - o),
            ]
        }
        NodeShape::Circle | NodeShape::ByType => Vec::new(),
    }
}

/// Solid fill of a shape. Uses a centre-fan mesh so concave shapes (Star, Plus)
/// fill correctly too (convex_polygon would only fill the convex hull).
fn fill_shape(painter: &egui::Painter, c: Pos2, r: f32, shape: NodeShape, fill: Color32) {
    if shape == NodeShape::Circle {
        painter.circle_filled(c, r, fill);
        return;
    }
    let ring = shape_pts(c, r, shape);
    if ring.len() < 3 { return; }
    let mut m = Mesh::default();
    m.colored_vertex(c, fill);
    for &pt in &ring { m.colored_vertex(pt, fill); }
    let n = ring.len() as u32;
    for i in 0..n { m.add_triangle(0, 1 + i, 1 + (i + 1) % n); }
    painter.add(egui::Shape::mesh(m));
}

fn stroke_shape(painter: &egui::Painter, c: Pos2, r: f32, shape: NodeShape, stroke: Stroke) {
    if shape == NodeShape::Circle {
        painter.circle_stroke(c, r, stroke);
    } else {
        let mut pts = shape_pts(c, r, shape);
        if let Some(&first) = pts.first() { pts.push(first); }
        painter.add(egui::Shape::line(pts, stroke));
    }
}

fn draw_dots(painter: &egui::Painter, rect: Rect, center: Pos2, view: &View) {
    let step = 48.0 * view.zoom;
    if step < 10.0 { return; }
    let origin = center + view.pan;
    let mut x = origin.x % step;
    while x < rect.right() {
        let mut y = origin.y % step;
        while y < rect.bottom() {
            if x >= rect.left() && y >= rect.top() {
                painter.circle_filled(Pos2::new(x, y), 1.2, grid());
            }
            y += step;
        }
        x += step;
    }
}

fn draw_grid(painter: &egui::Painter, rect: Rect, center: Pos2, view: &View) {
    let step = 48.0 * view.zoom;
    if step < 8.0 { return; }
    let origin = center + view.pan;
    let stroke = Stroke::new(1.0, grid());

    let mut x = origin.x % step;
    while x < rect.right() {
        if x >= rect.left() {
            painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], stroke);
        }
        x += step;
    }
    let mut y = origin.y % step;
    while y < rect.bottom() {
        if y >= rect.top() {
            painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], stroke);
        }
        y += step;
    }
}

/// A non-zero unit vector even when the input is (near) zero — never returns NaN.
fn safe_dir(v: Vec2, seed: u64) -> (Vec2, f32) {
    let len = v.length();
    if len > 1e-4 && len.is_finite() {
        (v / len, len)
    } else {
        // deterministic tiny offset so coincident nodes still separate
        let a = (seed as f32 * 2.399_963) % std::f32::consts::TAU;
        (Vec2::new(a.cos(), a.sin()), 0.5)
    }
}

fn finite(p: Pos2) -> bool { p.x.is_finite() && p.y.is_finite() }

/// Force-directed relaxation (Fruchterman–Reingold style). Pinned nodes stay put.
/// Hardened against NaN/Inf (coincident nodes used to produce non-finite
/// positions, which then crashed the renderer).
pub fn auto_layout(graph: &mut Graph) {
    let ids: Vec<u64> = graph.entities.keys().copied().collect();
    let n = ids.len();
    if n < 2 { return; }

    // Repair any non-finite positions before we start.
    for (i, &id) in ids.iter().enumerate() {
        if let Some(e) = graph.entities.get_mut(&id) {
            if !finite(e.pos) {
                e.pos = Pos2::new((i as f32 * 37.0) % 600.0 - 300.0, (i as f32 * 53.0) % 600.0 - 300.0);
            }
        }
    }

    // Fewer iterations as the graph grows, to keep the UI responsive.
    let iters = match n { 0..=60 => 220, 61..=150 => 110, 151..=400 => 55, _ => 25 };
    let k = 150.0_f32; // ideal edge length

    for _ in 0..iters {
        let mut disp: std::collections::HashMap<u64, Vec2> = ids.iter().map(|&i| (i, Vec2::ZERO)).collect();

        // repulsion
        for ai in 0..n {
            for bi in (ai + 1)..n {
                let a = ids[ai]; let b = ids[bi];
                let pa = graph.entities[&a].pos;
                let pb = graph.entities[&b].pos;
                let (dir, dist) = safe_dir(pa - pb, a.wrapping_add(b));
                let force = ((k * k) / dist.max(0.5)).min(20000.0);
                let push = dir * force;
                *disp.get_mut(&a).unwrap() += push;
                *disp.get_mut(&b).unwrap() -= push;
            }
        }
        // attraction along edges
        for e in &graph.edges {
            let (Some(a), Some(b)) = (graph.entities.get(&e.from), graph.entities.get(&e.to)) else { continue };
            let (dir, dist) = safe_dir(a.pos - b.pos, e.from.wrapping_add(e.to));
            let force = ((dist * dist) / k).min(20000.0);
            let pull = dir * force;
            *disp.get_mut(&e.from).unwrap() -= pull;
            *disp.get_mut(&e.to).unwrap()   += pull;
        }
        // integrate
        for &id in &ids {
            let e = graph.entities.get_mut(&id).unwrap();
            if e.pinned { continue; }
            let mv = disp[&id];
            let (dir, len) = safe_dir(mv, id);
            if len > 1e-3 {
                e.pos += dir * len.min(20.0);
            }
            if !finite(e.pos) { e.pos = Pos2::ZERO; }
        }
    }
}
