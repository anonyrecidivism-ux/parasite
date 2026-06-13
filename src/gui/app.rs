//! The graph workspace — the Maltego-style heart of the app. Wires the entity
//! palette, the canvas, the details/transform panel and the async transform
//! runner together.

use eframe::egui::{self, Color32, FontFamily, FontId, Margin, Pos2, RichText,
                   Rounding, ScrollArea, Stroke, TextEdit};
use std::sync::mpsc::{Receiver, Sender};

use super::canvas::{self, View};
use super::engine;
use super::model::{Graph, Kind};
use super::theme::*;
use super::transforms;

/// A "machine" — a named pipeline of transforms run in waves. Wave 0 runs on the
/// starting entity; each later wave runs on the entities the previous wave
/// produced (filtered to the transforms that apply to their kind).
struct Machine {
    name:  &'static str,
    desc:  &'static str,
    root:  Kind,
    waves: Vec<Vec<&'static str>>,
}

fn machines() -> Vec<Machine> {
    vec![
        Machine { name: "Domain Footprint", root: Kind::Domain,
            desc: "resolve, crt.sh, DNS, WHOIS, emails → then expand IPs & sites",
            waves: vec![
                vec!["dom_resolve", "dom_crtsh", "dom_dns", "dom_whois", "dom_emails"],
                vec!["ip_ptr", "ip_geo", "web_headers", "web_fetch"],
            ] },
        Machine { name: "Website Recon", root: Kind::Website,
            desc: "fingerprint, links, emails, exposed files, headers, robots",
            waves: vec![
                vec!["web_fetch", "web_links", "web_emails", "web_files", "web_headers", "web_robots"],
                vec!["dom_resolve"],
            ] },
        Machine { name: "Username Recon", root: Kind::Username,
            desc: "hunt accounts across social networks, then fingerprint each",
            waves: vec![
                vec!["user_hunt"],
                vec!["social_fetch"],
            ] },
        Machine { name: "Email → Identity", root: Kind::Email,
            desc: "username + domain + gravatar, then hunt the username",
            waves: vec![
                vec!["mail_user", "mail_domain", "mail_gravatar"],
                vec!["user_hunt", "dom_resolve"],
            ] },
        Machine { name: "Person → Accounts", root: Kind::Person,
            desc: "guess usernames, then hunt each across social networks",
            waves: vec![
                vec!["person_user"],
                vec!["user_hunt"],
            ] },
        Machine { name: "IP Profile", root: Kind::Ip,
            desc: "reverse DNS, geo/ASN and a common-port scan",
            waves: vec![
                vec!["ip_ptr", "ip_geo", "ip_ports"],
            ] },
    ]
}

/// Live state of a running machine.
struct MachineRun {
    name:     &'static str,
    waves:    Vec<Vec<&'static str>>,
    idx:      usize,
    targets:  Vec<u64>,
    snapshot: std::collections::HashSet<u64>,
    started:  bool,
}

/// A message sent from a transform/operation worker back to the UI thread.
enum Msg {
    Log(String),
    Result {
        source_id: u64,
        transform: String,
        outcome:   transforms::Outcome,
    },
    Done,
}

pub struct GraphPanel {
    graph:    Graph,
    view:     View,
    sel:      canvas::Selection,
    needs_fit: bool,

    rt:      tokio::runtime::Runtime,
    tx:      Sender<Msg>,
    rx:      Receiver<Msg>,
    running: usize,

    log:    Vec<(String, Color32)>,
    status: String,

    // palette / editor state
    new_kind:  Kind,
    new_value: String,
    save_path: String,
    filter:    String,
    menu:      Option<(u64, egui::Pos2)>,
    machine:   Option<MachineRun>,
    canvas_rect: egui::Rect,
    pending_shot: Option<ExportFmt>,
}

#[derive(Clone, Copy)]
enum ExportFmt { Png, Pdf }

impl GraphPanel {
    pub fn new() -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("tokio runtime");
        let (tx, rx) = std::sync::mpsc::channel();

        let mut s = Self {
            graph: Graph::new(),
            view: View::default(),
            sel: canvas::Selection::default(),
            needs_fit: false,
            rt, tx, rx,
            running: 0,
            log: Vec::new(),
            status: "ready".into(),
            new_kind: Kind::Domain,
            new_value: String::new(),
            save_path: "graph.json".into(),
            filter: String::new(),
            menu: None,
            machine: None,
            canvas_rect: egui::Rect::NOTHING,
            pending_shot: None,
        };
        s.log("◦  add an entity from the palette, then double-click it to run a transform");
        s
    }

    fn log(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        let c = if m.contains('✗') { c_err() }
            else if m.contains('⚠') { c_warn() }
            else if m.contains('✓') { c_ok() }
            else { text_sec() };
        self.log.push((m, c));
        if self.log.len() > 500 { self.log.drain(0..self.log.len() - 500); }
    }

    /// Place a new entity in the centre of the current view.
    fn add_entity(&mut self, kind: Kind, value: String) -> u64 {
        let pos = self.view_center_world();
        let jitter = self.graph.entities.len() as f32;
        let pos = Pos2::new(pos.x + (jitter * 17.0) % 60.0 - 30.0,
                            pos.y + (jitter * 29.0) % 60.0 - 30.0);
        let id = self.graph.add(kind, value, pos);
        self.sel.select_one(id);
        id
    }

    fn view_center_world(&self) -> Pos2 {
        // Inverse of canvas world→screen at the screen centre.
        (-self.view.pan / self.view.zoom).to_pos2()
    }

    /// Spawn an async transform against entity `source_id`.
    fn run_transform(&mut self, source_id: u64, transform_id: &str) {
        let Some(e) = self.graph.entities.get(&source_id) else { return };
        let value = e.value.clone();
        let tid = transform_id.to_string();
        let tx = self.tx.clone();

        self.running += 1;
        self.status = format!("running {} transform(s)…", self.running);
        self.log(format!("▸  {tid}  ←  {value}"));

        self.rt.spawn(async move {
            let outcome = transforms::run(&tid, &value).await;
            let _ = tx.send(Msg::Result { source_id, transform: tid, outcome });
            let _ = tx.send(Msg::Done);
        });
    }

    /// Run the first (default) transform for the selected entity's kind.
    fn run_default(&mut self, id: u64) {
        let kind = match self.graph.entities.get(&id) { Some(e) => e.kind, None => return };
        if let Some(t) = transforms::for_kind(kind).first() {
            let tid = t.id.to_string();
            self.run_transform(id, &tid);
        }
    }

    /// Run a transform OR an engine operation, by id, on `id`. `op_*` ids route
    /// to the streaming engine runner; everything else runs in-process.
    fn dispatch(&mut self, id: u64, tid: &str) {
        if let Some(rest) = tid.strip_prefix("op_") {
            if let Ok(op) = rest.parse::<u32>() { self.run_engine_op(id, op); return; }
        }
        self.run_transform(id, tid);
    }

    fn transform_applies(tid: &str, kind: Kind) -> bool {
        if let Some(o) = engine::by_tid(tid) { return o.applies == kind; }
        transforms::TRANSFORMS.iter().any(|t| t.id == tid && t.applies == kind)
    }

    fn start_machine(&mut self, m: &Machine, root: u64) {
        if self.machine.is_some() {
            self.log("⚠  a machine is already running");
            return;
        }
        self.log(format!("⚙  machine '{}' started", m.name));
        self.machine = Some(MachineRun {
            name: m.name,
            waves: m.waves.clone(),
            idx: 0,
            targets: vec![root],
            snapshot: std::collections::HashSet::new(),
            started: false,
        });
    }

    /// Advance the running machine: launch the next wave once the current one is
    /// done. Called every frame; does nothing unless a machine is active and idle.
    fn machine_tick(&mut self) {
        if self.running > 0 { return; }
        let Some(mut m) = self.machine.take() else { return };

        if m.started {
            // previous wave finished — its new entities become the next targets
            let new_ids: Vec<u64> = self.graph.entities.keys().copied()
                .filter(|id| !m.snapshot.contains(id)).collect();
            m.idx += 1;
            if m.idx >= m.waves.len() {
                self.log(format!("✓  machine '{}' finished", m.name));
                return;
            }
            m.targets = new_ids;
        }

        if m.targets.is_empty() {
            self.log(format!("◦  machine '{}' — nothing more to expand", m.name));
            return;
        }

        m.snapshot = self.graph.entities.keys().copied().collect();
        let tids = m.waves[m.idx].clone();
        let targets = m.targets.clone();
        let mut launched = 0;
        for tid in &tids {
            for &t in &targets {
                if let Some(kind) = self.graph.entities.get(&t).map(|e| e.kind) {
                    if Self::transform_applies(tid, kind) {
                        self.dispatch(t, tid);
                        launched += 1;
                    }
                }
            }
        }
        self.log(format!("▸  machine '{}' wave {}/{} — {launched} transform(s)",
            m.name, m.idx + 1, m.waves.len()));
        m.started = true;
        self.machine = Some(m);
    }

    /// The combined transform + operation menu for a kind: (id, name, desc, engine?).
    fn menu_items(kind: Kind) -> Vec<(String, &'static str, &'static str, bool)> {
        let mut v: Vec<(String, &'static str, &'static str, bool)> =
            transforms::for_kind(kind).iter().map(|t| (t.id.to_string(), t.name, t.desc, false)).collect();
        for o in engine::for_kind(kind) {
            v.push((o.tid.to_string(), o.name, o.desc, true));
        }
        v
    }

    /// Spawn an engine operation as a streaming "transform".
    fn run_engine_op(&mut self, source_id: u64, op_id: u32) {
        let value = match self.graph.entities.get(&source_id) { Some(e) => e.value.clone(), None => return };
        let name = engine::OPS.iter().find(|o| o.id == op_id).map(|o| o.name).unwrap_or("operation");
        let bin = engine::find_engine();
        let stdin = engine::full_stdin(op_id, &value);
        let tx = self.tx.clone();

        self.running += 1;
        self.status = format!("running {} …", self.running);
        self.log(format!("▸  {name}  ←  {value}"));

        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader, Write};
            use std::process::{Command, Stdio};

            let mut child = match Command::new(&bin).arg("--gui")
                .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Msg::Log(format!("✗  cannot launch engine '{bin}': {e}")));
                    let _ = tx.send(Msg::Log("   build it with: cargo build --release".into()));
                    let _ = tx.send(Msg::Done);
                    return;
                }
            };
            if let Some(mut si) = child.stdin.take() { let _ = si.write_all(stdin.as_bytes()); }

            let mut text = String::new();
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    let clean = engine::strip_ansi(&line);
                    if engine::keep_line(&clean) {
                        let t = clean.trim().to_string();
                        text.push('\n'); text.push_str(&t);
                        let _ = tx.send(Msg::Log(t));
                    }
                }
            }
            let _ = child.wait();

            let items = harvest(&text);
            let outcome = transforms::Outcome { items, props: Vec::new(), log: Vec::new() };
            let _ = tx.send(Msg::Result { source_id, transform: format!("op_{op_id}"), outcome });
            let _ = tx.send(Msg::Done);
        });
    }

    fn drain_messages(&mut self, ctx: &egui::Context) {
        let mut got = false;
        while let Ok(msg) = self.rx.try_recv() {
            got = true;
            match msg {
                Msg::Log(s) => self.log(s),
                Msg::Done => {
                    self.running = self.running.saturating_sub(1);
                    if self.running == 0 { self.status = "done".into(); }
                }
                Msg::Result { source_id, transform, outcome } => {
                    self.apply_outcome(source_id, &transform, outcome);
                }
            }
        }
        if got { self.needs_fit = false; }
        if self.running > 0 { ctx.request_repaint(); }
    }

    fn apply_outcome(&mut self, source_id: u64, _transform: &str, outcome: transforms::Outcome) {
        for line in outcome.log { self.log(line); }
        if !outcome.props.is_empty() {
            self.graph.merge_props(source_id, &outcome.props);
        }

        let origin = self.graph.entities.get(&source_id).map(|e| e.pos).unwrap_or_default();
        let base_deg = self.graph.degree(source_id);
        let total = outcome.items.len().max(1);

        for (i, item) in outcome.items.into_iter().enumerate() {
            // Fan children out on a ring around the source.
            let ang = std::f32::consts::TAU * ((base_deg + i) as f32) / (total as f32 + base_deg as f32 + 1.0);
            let radius = 150.0 + (i as f32 % 3.0) * 26.0;
            let pos = Pos2::new(origin.x + radius * ang.cos(), origin.y + radius * ang.sin());

            let (child, created) = self.graph.upsert(item.kind, &item.value, pos);
            if created && !item.props.is_empty() {
                self.graph.merge_props(child, &item.props);
            }
            self.graph.link(source_id, child, item.edge);
        }
    }

    fn delete_selected(&mut self) {
        if self.sel.set.is_empty() { return; }
        let ids: Vec<u64> = self.sel.set.iter().copied().collect();
        for id in &ids { self.graph.remove(*id); }
        let n = ids.len();
        self.sel.clear();
        self.log(format!("⊘  removed {n} entit{}", if n == 1 { "y" } else { "ies" }));
    }

    fn save_graph(&mut self) {
        let path = self.save_path.trim().to_string();
        match serde_json::to_string_pretty(&self.graph.to_data()) {
            Ok(json) => match std::fs::write(&path, json) {
                Ok(_)  => self.log(format!("✓  saved {} entities → {path}", self.graph.entities.len())),
                Err(e) => self.log(format!("✗  save failed: {e}")),
            },
            Err(e) => self.log(format!("✗  serialise failed: {e}")),
        }
    }

    fn load_graph(&mut self) {
        let path = self.save_path.trim().to_string();
        match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(data) => {
                    self.graph = Graph::from_data(data);
                    self.sel.clear();
                    self.needs_fit = true;
                    self.log(format!("✓  loaded {} entities ← {path}", self.graph.entities.len()));
                }
                Err(e) => self.log(format!("✗  parse failed: {e}")),
            },
            Err(e) => self.log(format!("✗  load failed: {e}")),
        }
    }

    // ── Rendering ──────────────────────────────────────────────────────────────

    pub fn ui(&mut self, ctx: &egui::Context) {
        self.drain_messages(ctx);
        self.machine_tick();
        if self.machine.is_some() { ctx.request_repaint(); }

        // Pick up a requested screenshot once the backend delivers it.
        if self.pending_shot.is_some() {
            let shot = ctx.input(|i| i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            }));
            if let Some(img) = shot {
                if let Some(fmt) = self.pending_shot.take() {
                    self.save_shot(img, fmt, ctx.pixels_per_point());
                }
            }
        }

        // Global shortcuts — but never while the user is typing in a text field
        // (otherwise Backspace/Delete would nuke the selected node).
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                if !self.sel.set.is_empty() { self.delete_selected(); }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F)) { self.needs_fit = true; }
            if ctx.input(|i| i.key_pressed(egui::Key::L)) {
                canvas::auto_layout(&mut self.graph);
                self.needs_fit = true;
            }
        }

        self.toolbar(ctx);
        self.palette(ctx);
        self.details_panel(ctx);
        self.log_panel(ctx);
        self.canvas_panel(ctx);
        self.context_menu(ctx);
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("graph_toolbar")
            .frame(egui::Frame::none().fill(bg_panel())
                .inner_margin(Margin::symmetric(12.0, 7.0))
                .stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let count = self.graph.entities.len();
                    let edges = self.graph.edges.len();
                    super::logo::widget(ui, 8.0);
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{count} entities · {edges} links"))
                        .color(text_sec()).size(12.0));
                    ui.add_space(12.0);

                    if toolbtn(ui, "⊹ Layout").clicked() {
                        canvas::auto_layout(&mut self.graph);
                        self.needs_fit = true;
                    }
                    if toolbtn(ui, "⤢ Fit").clicked() {
                        self.needs_fit = true;
                    }
                    if toolbtn(ui, "✗ Clear").clicked() {
                        self.graph.clear();
                        self.sel.clear();
                        self.log("⊘  graph cleared");
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    if toolbtn(ui, "▼ Load").clicked() { self.load_graph(); }
                    if toolbtn(ui, "▲ Save").clicked() { self.save_graph(); }
                    if toolbtn(ui, "⇩ CSV").clicked()  { self.export_csv(); }
                    if toolbtn(ui, "⛶ PNG").clicked()  { self.request_shot(ctx, ExportFmt::Png); }
                    if toolbtn(ui, "⛶ PDF").clicked()  { self.request_shot(ctx, ExportFmt::Pdf); }
                    ui.add(TextEdit::singleline(&mut self.save_path)
                        .desired_width(120.0)
                        .font(FontId::new(12.0, FontFamily::Monospace))
                        .text_color(text_sec()));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (txt, col) = if self.running > 0 {
                            (format!("↻  {}", self.status), accent())
                        } else {
                            (format!("◦  {}", self.status), text_mut())
                        };
                        ui.label(RichText::new(txt).color(col).size(12.0));
                    });
                });
            });
    }

    fn palette(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("graph_palette")
            .exact_width(210.0)
            .frame(egui::Frame::none().fill(bg_sidebar()).inner_margin(Margin::same(0.0)))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .inner_margin(Margin::symmetric(12.0, 10.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("NEW ENTITY").color(text_mut()).size(10.0).strong());
                        ui.add_space(8.0);

                        // Kind selector
                        egui::ComboBox::from_id_source("kind_combo")
                            .selected_text(RichText::new(format!("{} {}",
                                self.new_kind.icon(), self.new_kind.label())).color(text_pri()))
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for k in Kind::ALL {
                                    ui.selectable_value(&mut self.new_kind, k,
                                        RichText::new(format!("{}  {}", k.icon(), k.label()))
                                            .color(k.color()));
                                }
                            });
                        ui.add_space(6.0);

                        let te = ui.add(TextEdit::singleline(&mut self.new_value)
                            .hint_text(self.new_kind.default_value())
                            .desired_width(f32::INFINITY)
                            .font(FontId::new(12.5, FontFamily::Proportional)));
                        ui.add_space(6.0);

                        let add = ui.add_sized([ui.available_width(), 30.0],
                            egui::Button::new(RichText::new("＋  Add to graph")
                                .color(Color32::WHITE).strong().size(12.5))
                                .fill(accent()).rounding(Rounding::same(5.0)));
                        let submit = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if add.clicked() || submit {
                            let v = if self.new_value.trim().is_empty() {
                                self.new_kind.default_value().to_string()
                            } else { self.new_value.trim().to_string() };
                            self.add_entity(self.new_kind, v);
                            self.new_value.clear();
                        }

                        ui.add_space(8.0);
                        ui.label(RichText::new("Quick-add").color(text_mut()).size(10.0).strong());
                        ui.add_space(4.0);
                    });

                // Quick-add chips for each kind
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut to_add: Option<Kind> = None;
                    egui::Frame::none().inner_margin(Margin::symmetric(10.0, 0.0)).show(ui, |ui| {
                        for k in Kind::ALL {
                            let r = egui::Frame::none()
                                .fill(bg_item_hov())
                                .rounding(Rounding::same(4.0))
                                .inner_margin(Margin::symmetric(10.0, 6.0))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(k.icon()).color(k.color()).size(13.0));
                                        ui.add_space(4.0);
                                        ui.label(RichText::new(k.label()).color(text_sec()).size(12.0));
                                    });
                                }).response.interact(egui::Sense::click());
                            if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                            if r.clicked() { to_add = Some(k); }
                            ui.add_space(4.0);
                        }
                    });
                    if let Some(k) = to_add {
                        self.add_entity(k, k.default_value().to_string());
                    }

                    // ── Entities on the graph (searchable) ──────────────────
                    ui.add_space(10.0);
                    egui::Frame::none().inner_margin(Margin::symmetric(10.0, 0.0)).show(ui, |ui| {
                        ui.separator();
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("ENTITIES").color(text_mut()).size(10.0).strong());
                            ui.label(RichText::new(format!("{}", self.graph.entities.len()))
                                .color(text_mut()).size(10.0));
                        });
                        ui.add_space(4.0);
                        ui.add(TextEdit::singleline(&mut self.filter)
                            .hint_text("filter…")
                            .desired_width(f32::INFINITY)
                            .font(FontId::new(12.0, FontFamily::Proportional))
                            .text_color(text_sec()));
                        ui.add_space(4.0);
                    });

                    let q = self.filter.to_lowercase();
                    let mut list: Vec<(u64, Kind, String)> = self.graph.entities.values()
                        .filter(|e| q.is_empty() || e.value.to_lowercase().contains(&q))
                        .map(|e| (e.id, e.kind, e.value.clone()))
                        .collect();
                    list.sort_by(|a, b| a.2.to_lowercase().cmp(&b.2.to_lowercase()));

                    let mut focus_id: Option<u64> = None;
                    egui::Frame::none().inner_margin(Margin::symmetric(10.0, 0.0)).show(ui, |ui| {
                        for (id, kind, value) in list.iter().take(300) {
                            let sel = self.sel.contains(*id);
                            let bg = if sel { bg_item_sel() } else { Color32::TRANSPARENT };
                            let r = egui::Frame::none().fill(bg).rounding(Rounding::same(4.0))
                                .inner_margin(Margin::symmetric(8.0, 4.0))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(kind.icon()).color(kind.color()).size(12.0));
                                        ui.add_space(3.0);
                                        ui.label(RichText::new(truncate(value, 22))
                                            .color(if sel { text_pri() } else { text_sec() }).size(11.5));
                                    });
                                }).response.interact(egui::Sense::click());
                            if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                            if r.clicked() { focus_id = Some(*id); }
                        }
                    });
                    if let Some(id) = focus_id { self.focus(id); }
                });
            });
    }

    fn details_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("graph_details")
            .exact_width(290.0)
            .frame(egui::Frame::none().fill(bg_panel()).inner_margin(Margin::same(0.0)))
            .show(ctx, |ui| {
                let Some(id) = self.sel.primary else {
                    egui::Frame::none().inner_margin(Margin::symmetric(16.0, 16.0)).show(ui, |ui| {
                        ui.label(RichText::new("No entity selected").color(text_mut()).italics().size(12.5));
                        ui.add_space(6.0);
                        ui.label(RichText::new("Click a node to inspect it and run transforms.")
                            .color(text_mut()).size(12.0));
                    });
                    return;
                };
                let Some(e) = self.graph.entities.get(&id) else { self.sel.clear(); return; };
                let kind = e.kind;
                let mut value = e.value.clone();
                let props = e.props.clone();

                egui::Frame::none().inner_margin(Margin::symmetric(16.0, 14.0))
                    .stroke(Stroke::new(1.0, border())).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(kind.icon()).color(kind.color()).size(18.0));
                        ui.add_space(4.0);
                        ui.label(RichText::new(kind.label()).color(text_pri()).strong().size(15.0));
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("VALUE").color(text_mut()).size(10.0).strong());
                    ui.add_space(3.0);
                    if ui.add(TextEdit::singleline(&mut value)
                        .desired_width(f32::INFINITY)
                        .font(FontId::new(12.5, FontFamily::Monospace))
                        .text_color(text_pri())).changed()
                    {
                        if let Some(e) = self.graph.entities.get_mut(&id) { e.value = value.clone(); }
                    }
                    let lo = value.to_lowercase();
                    if lo.starts_with("http://") || lo.starts_with("https://")
                        || matches!(kind, Kind::Domain | Kind::Website | Kind::Social)
                    {
                        ui.add_space(6.0);
                        if ui.add(egui::Button::new(RichText::new("↗  Open in browser").color(accent()).size(12.0))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, accent_dark()))
                            .rounding(Rounding::same(5.0))).clicked()
                        {
                            let url = if lo.starts_with("http") { value.clone() } else { format!("https://{value}") };
                            open_url(&url);
                        }
                    }
                });

                // Properties
                if !props.is_empty() {
                    egui::Frame::none().inner_margin(Margin::symmetric(16.0, 10.0)).show(ui, |ui| {
                        ui.label(RichText::new("PROPERTIES").color(text_mut()).size(10.0).strong());
                        ui.add_space(4.0);
                        egui::Grid::new("props_grid").num_columns(2).spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                for (k, v) in &props {
                                    ui.label(RichText::new(k).color(text_sec()).size(11.5));
                                    ui.label(RichText::new(v).color(text_pri()).size(11.5)
                                        .font(FontId::new(11.5, FontFamily::Monospace)));
                                    ui.end_row();
                                }
                            });
                    });
                }

                ui.separator();

                // Machines (transform pipelines) for this kind
                let avail: Vec<(usize, &'static str, &'static str)> = machines().into_iter().enumerate()
                    .filter(|(_, m)| m.root == kind)
                    .map(|(i, m)| (i, m.name, m.desc))
                    .collect();
                if !avail.is_empty() {
                    let mut start: Option<usize> = None;
                    egui::Frame::none().inner_margin(Margin::symmetric(16.0, 8.0)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("⚙ MACHINES").color(accent()).size(10.0).strong());
                            if self.machine.is_some() {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add(egui::Button::new(RichText::new("◼ stop").color(c_err()).size(11.0))
                                        .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, border()))
                                        .rounding(Rounding::same(4.0))).clicked()
                                    {
                                        self.machine = None;
                                        self.log("◼  machine stopped");
                                    }
                                });
                            }
                        });
                        ui.add_space(4.0);
                        for (mi, name, desc) in &avail {
                            let r = egui::Frame::none()
                                .fill(bg_item_sel()).rounding(Rounding::same(5.0))
                                .inner_margin(Margin::symmetric(11.0, 7.0))
                                .stroke(Stroke::new(1.0, accent_dark()))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(format!("▶  {name}")).color(text_pri()).strong().size(12.5));
                                        ui.label(RichText::new(*desc).color(text_sec()).size(10.5));
                                    });
                                }).response.interact(egui::Sense::click());
                            if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                            if r.clicked() { start = Some(*mi); }
                            ui.add_space(5.0);
                        }
                    });
                    if let Some(mi) = start {
                        let ms = machines();
                        self.start_machine(&ms[mi], id);
                    }
                    ui.separator();
                }

                // Transforms for this kind
                egui::Frame::none().inner_margin(Margin::symmetric(16.0, 10.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("TRANSFORMS").color(text_mut()).size(10.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("⊘ Delete").color(c_err()).size(11.0))
                                .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, border()))
                                .rounding(Rounding::same(4.0))).clicked()
                            {
                                self.delete_selected();
                            }
                        });
                    });
                });

                let items = Self::menu_items(kind);
                let mut to_run: Option<String> = None;
                let mut run_all = false;

                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Frame::none().inner_margin(Margin::symmetric(14.0, 0.0)).show(ui, |ui| {
                        if items.is_empty() {
                            ui.label(RichText::new("No transforms for this entity type yet.")
                                .color(text_mut()).italics().size(12.0));
                        }
                        for (tid, name, desc, is_engine) in &items {
                            let r = egui::Frame::none()
                                .fill(bg_item_hov()).rounding(Rounding::same(5.0))
                                .inner_margin(Margin::symmetric(11.0, 8.0))
                                .stroke(Stroke::new(1.0, border()))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            let (g, c) = if *is_engine { ("⚙", c_info()) } else { ("▸", accent()) };
                                            ui.label(RichText::new(g).color(c).size(12.0));
                                            ui.add_space(2.0);
                                            ui.label(RichText::new(*name).color(text_pri()).strong().size(12.5));
                                        });
                                        ui.label(RichText::new(*desc).color(text_sec()).size(11.0));
                                    });
                                }).response.interact(egui::Sense::click());
                            if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                            if r.clicked() { to_run = Some(tid.clone()); }
                            ui.add_space(6.0);
                        }
                        if !items.is_empty() {
                            ui.add_space(2.0);
                            if ui.add_sized([ui.available_width(), 30.0],
                                egui::Button::new(RichText::new("▶  Run all transforms")
                                    .color(Color32::WHITE).strong().size(12.0))
                                    .fill(accent_dark()).rounding(Rounding::same(5.0))).clicked()
                            {
                                run_all = true;
                            }
                        }
                    });
                });

                if let Some(tid) = to_run { self.dispatch(id, &tid); }
                if run_all {
                    // only in-process transforms — engine ops can be slow/heavy
                    let ids: Vec<String> = transforms::for_kind(kind).iter().map(|t| t.id.to_string()).collect();
                    for tid in ids { self.run_transform(id, &tid); }
                }
            });
    }

    fn log_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("graph_log")
            .resizable(true)
            .default_height(120.0)
            .frame(egui::Frame::none().fill(bg_output())
                .inner_margin(Margin::symmetric(16.0, 8.0))
                .stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("LOG").color(text_mut()).size(10.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("clear").color(text_mut()).size(10.5))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).clicked()
                        {
                            self.log.clear();
                        }
                    });
                });
                ui.add_space(2.0);
                ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (line, col) in &self.log {
                            ui.label(RichText::new(line).color(*col)
                                .font(FontId::new(12.0, FontFamily::Monospace)));
                        }
                    });
            });
    }

    fn canvas_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_canvas()).inner_margin(Margin::same(0.0)))
            .show(ctx, |ui| {
                self.canvas_rect = ui.available_rect_before_wrap();
                if self.needs_fit {
                    self.view.fit(&self.graph, self.canvas_rect);
                    self.needs_fit = false;
                }
                let action = canvas::draw(ui, &mut self.graph, &mut self.view, &mut self.sel);
                if let Some(id) = action.run_default {
                    self.run_default(id);
                }
                if let Some(ctxt) = action.context {
                    self.menu = Some(ctxt);
                }
            });
    }

    /// Floating right-click transform menu (the signature Maltego gesture).
    fn context_menu(&mut self, ctx: &egui::Context) {
        let Some((id, pos)) = self.menu else { return };
        let kind = match self.graph.entities.get(&id) { Some(e) => e.kind, None => { self.menu = None; return; } };
        let value = self.graph.entities.get(&id).map(|e| e.value.clone()).unwrap_or_default();

        let mut to_run: Option<String> = None;
        let mut run_all = false;
        let mut close = false;

        let area = egui::Area::new(egui::Id::new("ctx_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(bg_panel()).stroke(Stroke::new(1.0, accent_dark()))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(Margin::symmetric(6.0, 6.0))
                    .show(ui, |ui| {
                        ui.set_max_width(260.0);
                        ui.label(RichText::new(format!("{} {}", kind.icon(),
                            truncate(&value, 26))).color(text_sec()).size(11.0));
                        ui.separator();
                        ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                            for (tid, name, _desc, is_engine) in Self::menu_items(kind) {
                                let lbl = if is_engine { format!("⚙ {name}") } else { name.to_string() };
                                if ui.add(egui::Button::new(RichText::new(lbl).color(text_pri()).size(12.5))
                                    .fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).clicked()
                                {
                                    to_run = Some(tid);
                                    close = true;
                                }
                            }
                        });
                        ui.separator();
                        if ui.add(egui::Button::new(RichText::new("▶  Run all").color(accent()).size(12.0))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).clicked()
                        {
                            run_all = true; close = true;
                        }
                        if ui.add(egui::Button::new(RichText::new("⊘  Delete").color(c_err()).size(12.0))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).clicked()
                        {
                            self.graph.remove(id);
                            self.sel.set.remove(&id);
                            if self.sel.primary == Some(id) {
                                self.sel.primary = self.sel.set.iter().next().copied();
                            }
                            close = true;
                        }
                    });
            });

        // Dismiss when clicking outside the menu.
        if ctx.input(|i| i.pointer.any_pressed()) && !area.response.hovered() {
            close = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { close = true; }

        if let Some(tid) = to_run { self.dispatch(id, &tid); }
        if run_all {
            let ids: Vec<String> = transforms::for_kind(kind).iter().map(|t| t.id.to_string()).collect();
            for tid in ids { self.run_transform(id, &tid); }
        }
        if close { self.menu = None; }
    }

    /// Centre the view on an entity and select it.
    fn focus(&mut self, id: u64) {
        if let Some(e) = self.graph.entities.get(&id) {
            self.view.pan = -e.pos.to_vec2() * self.view.zoom;
            self.sel.select_one(id);
        }
    }

    fn request_shot(&mut self, ctx: &egui::Context, fmt: ExportFmt) {
        self.pending_shot = Some(fmt);
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        self.log("◦  capturing canvas…");
    }

    /// Crop the framebuffer screenshot to the canvas and save it.
    fn save_shot(&mut self, img: std::sync::Arc<egui::ColorImage>, fmt: ExportFmt, ppp: f32) {
        let [fw, fh] = img.size;
        let r = self.canvas_rect;
        let x0 = ((r.min.x * ppp).floor() as usize).min(fw);
        let y0 = ((r.min.y * ppp).floor() as usize).min(fh);
        let x1 = ((r.max.x * ppp).ceil() as usize).min(fw);
        let y1 = ((r.max.y * ppp).ceil() as usize).min(fh);
        let (cw, ch) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        if cw == 0 || ch == 0 { self.log("✗  empty capture"); return; }

        let mut rgba = Vec::with_capacity(cw * ch * 4);
        for y in y0..y1 {
            for x in x0..x1 {
                let px = img.pixels[y * fw + x];
                rgba.extend_from_slice(&[px.r(), px.g(), px.b(), 255]);
            }
        }
        let (cw, ch) = (cw as u32, ch as u32);
        let res = match fmt {
            ExportFmt::Png => super::export::save_png("graph.png", &rgba, cw, ch).map(|_| "graph.png"),
            ExportFmt::Pdf => super::export::save_pdf("graph.pdf", &rgba, cw, ch).map(|_| "graph.pdf"),
        };
        match res {
            Ok(p)  => self.log(format!("✓  exported {p} ({cw}×{ch})")),
            Err(e) => self.log(format!("✗  export failed: {e}")),
        }
    }

    fn export_csv(&mut self) {
        let mut s = String::from("id,kind,value,properties\n");
        for e in self.graph.entities.values() {
            let props = e.props.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; ");
            s.push_str(&format!("{},{},{},{}\n",
                e.id, e.kind.label(), csv_field(&e.value), csv_field(&props)));
        }
        match std::fs::write("entities.csv", s) {
            Ok(_)  => self.log("✓  exported entities.csv"),
            Err(e) => self.log(format!("✗  CSV export failed: {e}")),
        }
    }
}

/// Pull URLs / emails / IPs out of an operation's free-text output and turn them
/// into linked graph entities.
fn harvest(text: &str) -> Vec<transforms::NewItem> {
    use regex::Regex;
    use std::collections::HashSet;
    let url_re  = Regex::new(r"https?://[^\s'\042<>()]+").unwrap();
    let mail_re = Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").unwrap();
    let ip_re   = Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap();

    let mut items = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mk = |k: Kind, v: String, e: &str| transforms::NewItem {
        kind: k, value: v, edge: e.into(), props: Vec::new(),
    };

    for m in url_re.find_iter(text) {
        if items.len() >= 60 { break; }
        let v = m.as_str().trim_end_matches([',', '.', ')', '"', '\'']).to_string();
        if seen.insert(v.clone()) { items.push(mk(Kind::Website, v, "found")); }
    }
    for m in mail_re.find_iter(text) {
        if items.len() >= 90 { break; }
        let v = m.as_str().to_lowercase();
        if seen.insert(v.clone()) { items.push(mk(Kind::Email, v, "found")); }
    }
    for m in ip_re.find_iter(text) {
        if items.len() >= 110 { break; }
        let v = m.as_str().to_string();
        if v.split('.').all(|o| o.parse::<u8>().is_ok()) && seen.insert(v.clone()) {
            items.push(mk(Kind::Ip, v, "found"));
        }
    }
    items
}

/// Open a URL in the system browser (Linux/macOS/Windows).
fn open_url(url: &str) {
    let url = url.to_string();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", &url]).spawn();
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
    else { s.to_string() }
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else { s.to_string() }
}

fn toolbtn(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let r = ui.add(egui::Button::new(RichText::new(label).color(text_sec()).size(12.0))
        .fill(bg_input()).stroke(Stroke::new(1.0, border())).rounding(Rounding::same(5.0)));
    if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    r
}
