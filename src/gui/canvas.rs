//! The graph canvas — pan, zoom, drag, multi-select (marquee), plus a
//! force-directed tidy.

use std::collections::HashSet;

use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, Rounding, Sense, Stroke, Vec2};

use super::model::Graph;
use super::theme::*;

/// The current selection: a set of node ids plus the "primary" (last-clicked)
/// one used for the details panel.
#[derive(Default)]
pub struct Selection {
    pub set:     HashSet<u64>,
    pub primary: Option<u64>,
}

impl Selection {
    pub fn select_one(&mut self, id: u64) {
        self.set.clear();
        self.set.insert(id);
        self.primary = Some(id);
    }
    pub fn toggle(&mut self, id: u64) {
        if self.set.remove(&id) {
            if self.primary == Some(id) { self.primary = self.set.iter().next().copied(); }
        } else {
            self.set.insert(id);
            self.primary = Some(id);
        }
    }
    pub fn clear(&mut self) { self.set.clear(); self.primary = None; }
    pub fn contains(&self, id: u64) -> bool { self.set.contains(&id) }
}

pub struct View {
    pub pan:  Vec2,
    pub zoom: f32,
    drag_node:    Option<u64>,
    marquee_from: Option<Pos2>, // screen-space marquee anchor
}

impl Default for View {
    fn default() -> Self {
        Self { pan: Vec2::ZERO, zoom: 1.0, drag_node: None, marquee_from: None }
    }
}

/// What the canvas wants the app to do after a frame.
#[derive(Default)]
pub struct CanvasAction {
    pub run_default: Option<u64>,
    /// (entity id, screen position) where a context menu was requested.
    pub context: Option<(u64, Pos2)>,
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
            min.x = min.x.min(e.pos.x); min.y = min.y.min(e.pos.y);
            max.x = max.x.max(e.pos.x); max.y = max.y.max(e.pos.y);
        }
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
) -> CanvasAction {
    let mut action = CanvasAction::default();

    let rect = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let shift = ui.input(|i| i.modifiers.shift);

    painter.rect_filled(rect, Rounding::ZERO, bg_canvas());
    if show_grid() { draw_grid(&painter, rect, center, view); }

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
            // Dragging a node moves the whole selection (select it first if new).
            if !sel.contains(id) {
                if shift { sel.toggle(id); } else { sel.select_one(id); }
            }
            view.drag_node = Some(id);
        } else if shift {
            // Shift + empty drag → marquee select.
            view.marquee_from = pointer;
        }
        // else: plain empty drag pans (handled below).
    }

    // ── Apply drag ─────────────────────────────────────────────────────────────
    if response.dragged() {
        let delta = response.drag_delta();
        if let Some(_id) = view.drag_node {
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

    // ── Draw marquee + select on release ───────────────────────────────────────
    if let (Some(from), Some(cur)) = (view.marquee_from, pointer) {
        let mrect = Rect::from_two_pos(from, cur);
        painter.rect_filled(mrect, Rounding::same(2.0),
            Color32::from_rgba_unmultiplied(accent().r(), accent().g(), accent().b(), 30));
        painter.rect_stroke(mrect, Rounding::same(2.0), Stroke::new(1.0, accent()));
    }
    if response.drag_stopped() {
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
            None              => sel.clear(),
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

    // ── Draw edges ─────────────────────────────────────────────────────────────
    for edge in &graph.edges {
        let (Some(a), Some(b)) = (graph.entities.get(&edge.from), graph.entities.get(&edge.to)) else { continue };
        let pa = view.w2s(center, a.pos);
        let pb = view.w2s(center, b.pos);
        painter.line_segment([pa, pb], Stroke::new(1.3, border()));
        // arrow head
        let dir = (pb - pa).normalized();
        let tip = pb - dir * (node_radius() * view.zoom + 2.0);
        let perp = Vec2::new(-dir.y, dir.x);
        let s = 6.0 * view.zoom.clamp(0.6, 1.4);
        painter.line_segment([tip, tip - dir * s + perp * s * 0.6], Stroke::new(1.3, border()));
        painter.line_segment([tip, tip - dir * s - perp * s * 0.6], Stroke::new(1.3, border()));
        if edge_labels() && view.zoom > 0.7 && !edge.label.is_empty() {
            let mid = pa + (pb - pa) * 0.5;
            painter.text(mid, egui::Align2::CENTER_CENTER, &edge.label,
                FontId::new(9.5, FontFamily::Proportional), text_mut());
        }
    }

    // ── Draw nodes ─────────────────────────────────────────────────────────────
    let r = node_radius() * view.zoom;
    let label_font = FontId::new((12.0 * view.zoom).clamp(9.0, 15.0), FontFamily::Proportional);
    let icon_font  = FontId::new((18.0 * view.zoom).clamp(11.0, 24.0), FontFamily::Proportional);

    // stable draw order
    let mut ids: Vec<u64> = graph.entities.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let e = &graph.entities[&id];
        let p = view.w2s(center, e.pos);
        if !rect.expand(60.0).contains(p) { continue; }

        let is_sel = sel.contains(id);
        let is_primary = sel.primary == Some(id);
        let is_hov = hit == Some(id);
        let col = e.kind.color();

        if is_sel {
            painter.circle_filled(p, r + 5.0, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 40));
            painter.circle_stroke(p, r + 4.0, Stroke::new(if is_primary { 2.5 } else { 1.5 }, accent()));
        } else if is_hov {
            painter.circle_stroke(p, r + 3.0, Stroke::new(1.5, accent_dark()));
        }
        painter.circle_filled(p, r, bg_panel());
        painter.circle_stroke(p, r, Stroke::new(2.0, col));
        painter.circle_filled(p, r * 0.62, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 55));
        painter.text(p, egui::Align2::CENTER_CENTER, e.kind.icon(), icon_font.clone(), col);

        if view.zoom > 0.45 {
            let label: String = {
                let v = &e.value;
                if v.chars().count() > 28 { format!("{}…", v.chars().take(27).collect::<String>()) }
                else { v.clone() }
            };
            let galley = painter.layout_no_wrap(label, label_font.clone(),
                if is_sel { text_pri() } else { text_sec() });
            let sz = galley.size();
            let lp = Pos2::new(p.x - sz.x / 2.0, p.y + r + 5.0);
            let lbg = bg_output();
            painter.rect_filled(
                Rect::from_min_size(lp - Vec2::new(5.0, 2.0), sz + Vec2::new(10.0, 4.0)),
                Rounding::same(3.0),
                Color32::from_rgba_unmultiplied(lbg.r(), lbg.g(), lbg.b(), 210),
            );
            painter.galley(lp, galley, text_sec());
        }
    }

    // hover cursor
    if hit.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    action
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

/// Force-directed relaxation (Fruchterman–Reingold style). Pinned nodes stay put.
pub fn auto_layout(graph: &mut Graph) {
    let ids: Vec<u64> = graph.entities.keys().copied().collect();
    let n = ids.len();
    if n < 2 { return; }

    let k = 150.0_f32; // ideal edge length
    for _ in 0..240 {
        let mut disp: std::collections::HashMap<u64, Vec2> = ids.iter().map(|&i| (i, Vec2::ZERO)).collect();

        // repulsion
        for ai in 0..n {
            for bi in (ai + 1)..n {
                let a = ids[ai]; let b = ids[bi];
                let pa = graph.entities[&a].pos;
                let pb = graph.entities[&b].pos;
                let mut d = pa - pb;
                let mut dist = d.length();
                if dist < 0.01 { d = Vec2::new(0.5, 0.3); dist = 0.6; }
                let force = (k * k) / dist;
                let push = d.normalized() * force;
                *disp.get_mut(&a).unwrap() += push;
                *disp.get_mut(&b).unwrap() -= push;
            }
        }
        // attraction along edges
        for e in &graph.edges {
            let (Some(a), Some(b)) = (graph.entities.get(&e.from), graph.entities.get(&e.to)) else { continue };
            let d = a.pos - b.pos;
            let dist = d.length().max(0.01);
            let force = (dist * dist) / k;
            let pull = d.normalized() * force;
            *disp.get_mut(&e.from).unwrap() -= pull;
            *disp.get_mut(&e.to).unwrap()   += pull;
        }
        // integrate
        for &id in &ids {
            let e = graph.entities.get_mut(&id).unwrap();
            if e.pinned { continue; }
            let mv = disp[&id];
            let len = mv.length().min(20.0);
            if len > 0.0 { e.pos += mv.normalized() * len; }
        }
    }
}
