//! The graph workspace — the Maltego-style heart of the app. Wires the entity
//! palette, the canvas, the details/transform panel and the async transform
//! runner together.

use egui::{self, Color32, FontFamily, FontId, Margin, Pos2, RichText,
                   Rounding, ScrollArea, Stroke, TextEdit};
use std::sync::mpsc::{Receiver, Sender};

use super::canvas::{self, View};
use super::engine;
use super::i18n;
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
        Machine { name: "Domain Attack Surface", root: Kind::Domain,
            desc: "crt.sh + CertSpotter + DNS + WHOIS, then profile every IP & site",
            waves: vec![
                vec!["dom_resolve", "dom_crtsh", "dom_certspotter", "dom_dns", "dom_whois"],
                vec!["ip_ptr", "ip_geo", "web_headers", "web_files"],
            ] },
        Machine { name: "Email → Accounts", root: Kind::Email,
            desc: "username + holehe + gravatar, then hunt & GitHub the username",
            waves: vec![
                vec!["mail_user", "mail_domain", "mail_gravatar", "email_holehe"],
                vec!["user_hunt", "user_github"],
            ] },
        Machine { name: "IP Deep Profile", root: Kind::Ip,
            desc: "reverse DNS + reverse-IP + geo/ASN, then announced prefixes",
            waves: vec![
                vec!["ip_ptr", "ip_revip", "ip_geo"],
                vec!["asn_prefixes"],
            ] },
        Machine { name: "ASN → Netblocks", root: Kind::Asn,
            desc: "all prefixes announced by this ASN",
            waves: vec![ vec!["asn_prefixes"] ] },
        Machine { name: "Hash Triage", root: Kind::Hash,
            desc: "identify algorithm + dictionary lookup",
            waves: vec![ vec!["hash_id", "hash_lookup"] ] },
        Machine { name: "Domain Deep Recon", root: Kind::Domain,
            desc: "crt.sh + SecurityTrails + FullHunt + OTX + DNS/WHOIS, then profile every IP",
            waves: vec![
                vec!["dom_crtsh", "dom_securitytrails", "dom_fullhunt", "dom_otx", "dom_dns", "dom_whois", "dom_resolve"],
                vec!["ip_ptr", "ip_internetdb", "ip_greynoise", "ip_ipinfo"],
            ] },
        Machine { name: "Domain Tech & Surface", root: Kind::Domain,
            desc: "BuiltWith tech, OTX URLs, exposed files & headers, then fingerprint sites",
            waves: vec![
                vec!["dom_builtwith", "dom_otx_urls", "dom_website"],
                vec!["web_fetch", "web_headers", "web_files", "web_links"],
            ] },
        Machine { name: "IP Full Profile", root: Kind::Ip,
            desc: "PTR, geo/ASN, InternetDB, GreyNoise, IPinfo + announced prefixes",
            waves: vec![
                vec!["ip_ptr", "ip_geo", "ip_internetdb", "ip_greynoise", "ip_ipinfo", "ip_revip"],
                vec!["asn_prefixes"],
            ] },
        Machine { name: "Email Breach Sweep", root: Kind::Email,
            desc: "username + domain + gravatar + holehe + HIBP + IntelX, then hunt accounts",
            waves: vec![
                vec!["mail_user", "mail_domain", "mail_gravatar", "email_holehe", "email_hibp", "email_intelx"],
                vec!["user_hunt", "user_github", "dom_resolve"],
            ] },
        Machine { name: "Username 360", root: Kind::Username,
            desc: "hunt accounts + social URLs + GitHub/GitLab/Keybase/HN, then fingerprint",
            waves: vec![
                vec!["user_hunt", "user_socials", "user_github", "user_gitlab", "user_keybase", "user_hackernews"],
                vec!["social_fetch"],
            ] },
        Machine { name: "Phone Profile", root: Kind::Phone,
            desc: "normalize, country, NumVerify carrier/line-type + search links",
            waves: vec![ vec!["phone_format", "phone_info", "phone_numverify", "phone_pivots"] ] },
        Machine { name: "BTC Wallet Trace", root: Kind::BtcAddress,
            desc: "balance + transactions + explorer links",
            waves: vec![ vec!["btc_info", "btc_txs", "btc_pivots"] ] },
        Machine { name: "ETH Wallet Trace", root: Kind::EthAddress,
            desc: "balance + transactions + explorer links",
            waves: vec![ vec!["eth_info", "eth_txs", "eth_pivots"] ] },
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
    /// A file the user chose in the native picker, to set as `id`'s node image.
    ImagePicked(u64, String),
    /// The AI's raw reply (a JSON graph plan) or an error.
    AiGraph(Result<String, String>),
    /// A chat reply from the graph assistant.
    Chat(Result<String, String>),
    /// An agent "plan next steps" JSON reply.
    Plan(Result<String, String>),
    /// Instinct auto-triage result: set node `id`'s flag + log a note.
    Flag { id: u64, flag: u8, note: String },
    /// An imported Instinct rule-pack (Lisp source).
    RulesLoaded(String),
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
    video_path: String,
    filter:    String,
    menu:      Option<(u64, egui::Pos2)>,
    machine:   Option<MachineRun>,
    canvas_rect: egui::Rect,
    pending_shot: Option<ExportFmt>,
    show_table: bool,
    show_add: bool,
    show_analytics: bool,
    show_minimap: bool,
    undo: Vec<super::model::GraphData>,
    redo: Vec<super::model::GraphData>,
    recording: Option<RecState>,
    /// Loaded node-face images, keyed by entity id, plus the path each was loaded
    /// from (so a changed path reloads).
    node_tex:  std::collections::HashMap<u64, egui::TextureHandle>,
    node_src:  std::collections::HashMap<u64, String>,
    // AI graph builder
    show_ai:   bool,
    ai_prompt: String,
    ai_busy:   bool,
    // AI assistant chat (over the current graph)
    show_chat:  bool,
    chat:       Vec<(super::ai::ChatRole, String)>,
    chat_input: String,
    chat_busy:  bool,
    // Quick Intel plashka (phone HLR / email registration → graph)
    show_quick: bool,
    quick_input: String,
    // Lisp rule-based advisor ("smart graph", no AI)
    show_advisor: bool,
    advisor_edit: bool,
    advisor_auto: bool,
    advisor_badge: usize,
    advisor_rules: String,
    // session autosave + coverage tracking + command palette
    last_autosave: f64,
    autosave_sig:  (usize, usize),
    ran:           std::collections::HashMap<u64, std::collections::HashSet<String>>,
    show_coverage: bool,
    show_palette_cmd: bool,
    cmd_query:     String,
    node_risk:     std::collections::HashMap<u64, u8>,
    risk_t:        f64,
    graph_filter:  String,
    show_help:     bool,
}

/// Path of the autosaved session graph.
fn session_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("parasite").join("session.json")
}

/// Path of the saved camera position (pan + zoom).
fn view_path() -> std::path::PathBuf { session_path().with_file_name("session_view.json") }

/// Path of the user-editable advisor rules file.
fn advisor_rules_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("parasite").join("rules.lisp")
}

#[derive(Clone, Copy)]
enum ExportFmt { Png, Pdf }

/// Recording renders to a constant frame rate driven by a virtual clock.
const REC_FPS: f64 = 24.0;
/// A captured frame handed to a background writer thread (encode + disk I/O off
/// the UI thread — that was the real bottleneck, not the frame count).
struct FrameJob { path: std::path::PathBuf, rgba: Vec<u8>, w: u32, h: u32 }
struct RecState {
    dir: std::path::PathBuf, idx: u32, start_time: f64, end_time: f64,
    /// wall-clock time recording started — for the safety timeout
    wall_start: f64, out: String,
    tx: Option<std::sync::mpsc::Sender<FrameJob>>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

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
            video_path: "graph.mp4".into(),
            filter: String::new(),
            menu: None,
            machine: None,
            canvas_rect: egui::Rect::NOTHING,
            pending_shot: None,
            show_table: false,
            show_add: false,
            show_analytics: false,
            show_minimap: true,
            undo: Vec::new(),
            redo: Vec::new(),
            recording: None,
            node_tex: std::collections::HashMap::new(),
            node_src: std::collections::HashMap::new(),
            show_ai: false,
            ai_prompt: String::new(),
            ai_busy: false,
            show_chat: false,
            chat: Vec::new(),
            chat_input: String::new(),
            chat_busy: false,
            show_quick: false,
            quick_input: String::new(),
            show_advisor: false,
            advisor_edit: false,
            advisor_auto: false,
            advisor_badge: 0,
            advisor_rules: std::fs::read_to_string(advisor_rules_path())
                .unwrap_or_else(|_| super::lisp::rules_default(i18n::lang()).to_string()),
            last_autosave: 0.0,
            autosave_sig: (0, 0),
            ran: std::collections::HashMap::new(),
            show_coverage: false,
            show_palette_cmd: false,
            cmd_query: String::new(),
            node_risk: std::collections::HashMap::new(),
            risk_t: 0.0,
            graph_filter: String::new(),
            show_help: false,
        };
        s.log("◦  add an entity from the palette, then double-click it to run a transform");
        // restore the autosaved session if one exists
        if let Ok(text) = std::fs::read_to_string(session_path()) {
            if let Ok(data) = serde_json::from_str::<super::model::GraphData>(&text) {
                if !data.entities.is_empty() {
                    s.graph = Graph::from_data(data);
                    s.log(format!("◦  restored autosaved session ({} entities)", s.graph.entities.len()));
                    // restore the camera too (else fit on the next frame)
                    match std::fs::read_to_string(view_path())
                        .ok().and_then(|t| serde_json::from_str::<(f32, f32, f32)>(&t).ok())
                    {
                        Some((px, py, z)) if z.is_finite() && z > 0.0 => {
                            s.view.pan = egui::Vec2::new(px, py); s.view.zoom = z;
                        }
                        _ => s.needs_fit = true,
                    }
                }
            }
        }
        // debug/demo: open a graph file at startup if PARASITE_OPEN is set
        if let Ok(path) = std::env::var("PARASITE_OPEN") {
            s.save_path = path;
            s.load_graph();
        }
        s
    }

    /// Autosave the session (called periodically from `ui`, only when changed).
    fn autosave(&mut self, now: f64) {
        if now - self.last_autosave < 4.0 { return; }
        self.last_autosave = now;
        let sig = (self.graph.entities.len(), self.graph.edges.len());
        if sig == self.autosave_sig { return; }
        self.autosave_sig = sig;
        let p = session_path();
        if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
        if let Ok(json) = serde_json::to_string(&self.graph.to_data()) {
            let _ = std::fs::write(&p, json);
        }
        // camera position
        let v = (self.view.pan.x, self.view.pan.y, self.view.zoom);
        if let Ok(json) = serde_json::to_string(&v) { let _ = std::fs::write(view_path(), json); }
    }


    pub fn recording(&self) -> bool { self.recording.is_some() }

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
        self.record();
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

    /// Open a native file picker (off the UI thread so egui never freezes) and,
    /// once the user chooses an image, set it as entity `id`'s node image.
    fn pick_image_file(&self, id: u64) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if let Some(path) = native_open_file() {
                let _ = tx.send(Msg::ImagePicked(id, path));
            }
        });
    }

    /// Load/refresh/evict the node-face image textures so they track each
    /// entity's `image` path. Cheap: only (re)loads when a path appears or changes.
    fn sync_textures(&mut self, ctx: &egui::Context) {
        // drop textures whose entity is gone or no longer has an image
        let live: std::collections::HashSet<u64> = self.graph.entities.iter()
            .filter(|(_, e)| e.image.is_some()).map(|(id, _)| *id).collect();
        self.node_tex.retain(|id, _| live.contains(id));
        self.node_src.retain(|id, _| live.contains(id));

        for (id, e) in &self.graph.entities {
            let Some(path) = &e.image else { continue };
            if self.node_src.get(id).map(|p| p == path).unwrap_or(false) { continue; }
            // (re)load
            match std::fs::read(path).ok().and_then(|b| image::load_from_memory(&b).ok()) {
                Some(img) => {
                    let img = img.thumbnail(256, 256).to_rgba8();
                    let size = [img.width() as usize, img.height() as usize];
                    let ci = egui::ColorImage::from_rgba_unmultiplied(size, &img);
                    let tex = ctx.load_texture(format!("node_img_{id}"), ci, egui::TextureOptions::LINEAR);
                    self.node_tex.insert(*id, tex);
                }
                None => { self.node_tex.remove(id); }
            }
            // remember the path either way so a broken path doesn't reload every frame
            self.node_src.insert(*id, path.clone());
        }
    }

    /// Ask the configured AI (Claude or Gemini) to design a graph for the prompt.
    /// `expand` includes the current graph as context so the model extends it.
    fn ai_build(&mut self, expand: bool) {
        let instruction = self.ai_prompt.trim().to_string();
        if instruction.is_empty() { self.status = "describe a target first".into(); return; }
        let Some(cfg) = super::ai::cfg() else {
            self.log("✗  AI: add a Claude or Gemini key in ⚙ Settings → API keys");
            self.status = "no AI key set".into();
            return;
        };
        let context = if expand {
            self.graph.entities.values()
                .map(|e| format!("- {} \"{}\"", i18n::kind_label(e.kind), e.value))
                .take(60).collect::<Vec<_>>().join("\n")
        } else { String::new() };
        let (system, user) = super::ai::graph_prompt(&instruction, &context);
        let tx = self.tx.clone();
        self.ai_busy = true;
        self.status = format!("asking {}…", cfg.label());
        self.log(format!("✦  {} building graph: {instruction}", cfg.label()));
        self.rt.spawn(async move {
            let res = super::ai::complete(&cfg, &system, &user).await;
            let _ = tx.send(Msg::AiGraph(res));
        });
    }

    /// Turn an AI JSON plan into entities + edges on the graph.
    fn apply_ai_plan(&mut self, text: &str) {
        let plan = match super::ai::parse_plan(text) {
            Ok(p) => p,
            Err(e) => { self.log(format!("✗  AI: {e}")); self.status = "ai parse failed".into(); return; }
        };
        if plan.entities.is_empty() { self.log("⚠  AI returned no entities"); return; }
        self.record();
        let center = self.view_center_world();
        let n = plan.entities.len().max(1) as f32;
        let mut ids: Vec<u64> = Vec::with_capacity(plan.entities.len());
        for (i, ent) in plan.entities.iter().enumerate() {
            let kind = super::ai::kind_from_label(&ent.kind);
            let ang = std::f32::consts::TAU * (i as f32) / n;
            let r = 90.0 + (i as f32 % 6.0) * 26.0;
            let pos = Pos2::new(center.x + ang.cos() * r, center.y + ang.sin() * r);
            let (id, _) = self.graph.upsert(kind, ent.value.trim(), pos);
            if let Some(e) = self.graph.entities.get_mut(&id) {
                if !ent.note.trim().is_empty() && e.note.is_empty() {
                    e.note = format!("✦ {}", ent.note.trim());
                }
            }
            ids.push(id);
        }
        let mut links = 0;
        for edge in &plan.edges {
            if let (Some(&a), Some(&b)) = (ids.get(edge.from), ids.get(edge.to)) {
                self.graph.link(a, b, edge.label.clone());
                links += 1;
            }
        }
        self.needs_fit = true;
        self.status = "ai graph ready".into();
        self.log(format!("✓  AI added {} entit(ies) + {} link(s)", ids.len(), links));
    }

    /// A compact text description of the current graph for the AI's context.
    fn graph_context(&self) -> String {
        let mut s = String::from("Current OSINT graph:\nEntities:\n");
        for e in self.graph.entities.values().take(120) {
            s.push_str(&format!("- [{}] {}{}\n", i18n::kind_label(e.kind), e.value,
                if e.note.is_empty() { String::new() } else { format!("  (note: {})", e.note) }));
        }
        if !self.graph.edges.is_empty() {
            s.push_str("Links:\n");
            for ed in self.graph.edges.iter().take(120) {
                let a = self.graph.entities.get(&ed.from).map(|e| e.value.as_str()).unwrap_or("?");
                let b = self.graph.entities.get(&ed.to).map(|e| e.value.as_str()).unwrap_or("?");
                s.push_str(&format!("- {a} —{}→ {b}\n", if ed.label.is_empty() { "" } else { &ed.label }));
            }
        }
        s
    }

    /// Transforms available for the kinds present in the graph (id — name).
    fn available_transforms(&self) -> String {
        let kinds: std::collections::HashSet<Kind> =
            self.graph.entities.values().map(|e| e.kind).collect();
        let mut s = String::new();
        for k in kinds {
            let ts = transforms::for_kind(k);
            if ts.is_empty() { continue; }
            s.push_str(&format!("{}: ", k.label()));
            s.push_str(&ts.iter().map(|t| t.id).collect::<Vec<_>>().join(", "));
            s.push('\n');
        }
        s
    }

    /// Send a chat message to the AI assistant (context = the current graph).
    fn send_chat(&mut self) {
        let msg = self.chat_input.trim().to_string();
        if msg.is_empty() || self.chat_busy { return; }
        let Some(cfg) = super::ai::cfg() else { self.status = "no AI key set".into(); return; };
        self.chat.push((super::ai::ChatRole::User, msg));
        self.chat_input.clear();
        let system = format!("You are an OSINT analyst assistant embedded in 'parasite', a Maltego-style \
            graph tool. You have FULL control of the graph — but you must NEVER export, save files, run \
            shell commands, or touch anything outside the in-app graph.\n\n\
            FLAGS colour a node: 0 = none, 1 = important (red), 2 = verified (green), 3 = target (orange). \
            NOTES are free text shown under a node. Use these to mark up the investigation.\n\n\
            To change the graph, append exactly ONE block at the very end of your reply:\n\
            <<<ACTIONS\n{{\"actions\":[ ... ]}}\nACTIONS>>>\n\
            Actions (one object each):\n\
            {{\"op\":\"add\",\"kind\":\"Email\",\"value\":\"x@y.com\"}}\n\
            {{\"op\":\"rename\",\"target\":\"<value>\",\"value\":\"<new>\"}}\n\
            {{\"op\":\"delete\",\"target\":\"<value>\"}}\n\
            {{\"op\":\"set_kind\",\"target\":\"<value>\",\"value\":\"Person\"}}\n\
            {{\"op\":\"link\",\"from\":\"<value>\",\"to\":\"<value>\",\"label\":\"owns\"}}\n\
            {{\"op\":\"unlink\",\"from\":\"<value>\",\"to\":\"<value>\"}}\n\
            {{\"op\":\"note\",\"target\":\"<value>\",\"value\":\"...\"}}\n\
            {{\"op\":\"flag\",\"target\":\"<value>\",\"flag\":3}}\n\
            {{\"op\":\"run\",\"target\":\"<value>\",\"transform\":\"<id>\"}}  (runs a transform to expand a node)\n\
            {{\"op\":\"layout\"}}  (re-arrange the graph)\n\
            'target'/'from'/'to'/value-of-rename must match an entity value verbatim. \
            Never claim a change without the block. Keep prose short.\n\n\
            Transforms you may 'run' (by id), for the kinds in the graph:\n{}\n\n{}",
            self.available_transforms(), self.graph_context());
        let history = self.chat.clone();
        let tx = self.tx.clone();
        self.chat_busy = true;
        self.rt.spawn(async move {
            let res = super::ai::chat(&cfg, &system, &history).await;
            let _ = tx.send(Msg::Chat(res));
        });
    }

    /// Add a single entity to the graph (used by Watch → graph).
    pub fn add_node(&mut self, kind: Kind, value: String) {
        self.add_entity(kind, value);
        self.needs_fit = true;
    }

    /// Snapshot the graph for saving as a case.
    pub fn export_case(&self) -> super::model::GraphData { self.graph.to_data() }

    /// Replace the graph with a loaded case snapshot.
    pub fn import_case(&mut self, data: super::model::GraphData) {
        self.record();
        self.graph = Graph::from_data(data);
        self.sel.clear();
        self.needs_fit = true;
        self.log("✓  case loaded into the graph");
    }

    /// Find an entity id by its value (case-insensitive, any kind).
    fn id_by_value(&self, value: &str) -> Option<u64> {
        let v = value.trim().to_lowercase();
        self.graph.entities.values()
            .find(|e| e.value.trim().to_lowercase() == v).map(|e| e.id)
    }

    /// Post a chat reply, executing any `<<<ACTIONS …>>>` block the agent emitted
    /// so it can actually modify the graph (rename, add, link, delete, …).
    fn apply_chat_actions(&mut self, raw: &str) {
        let (clean, actions) = super::ai::extract_actions(raw);
        if !clean.is_empty() {
            self.chat.push((super::ai::ChatRole::Assistant, clean));
        }
        if let Some(v) = actions {
            let report = self.run_actions(&v);
            if !report.is_empty() {
                self.chat.push((super::ai::ChatRole::Assistant, format!("⚙ {report}")));
            }
        }
    }

    /// Execute an actions JSON object on the graph. Returns a human summary.
    fn run_actions(&mut self, v: &serde_json::Value) -> String {
        let Some(arr) = v["actions"].as_array() else { return String::new() };
        if arr.is_empty() { return String::new() }
        self.record();
        let center = self.view_center_world();
        let mut done: Vec<String> = Vec::new();
        for a in arr {
            let op = a["op"].as_str().unwrap_or("");
            let target = a["target"].as_str().unwrap_or("").trim();
            let value = a["value"].as_str().unwrap_or("").trim();
            match op {
                "rename" => {
                    if let Some(id) = self.id_by_value(target) {
                        if let Some(e) = self.graph.entities.get_mut(&id) { e.value = value.to_string(); }
                        done.push(format!("renamed '{target}' → '{value}'"));
                    }
                }
                "add" => {
                    if !value.is_empty() {
                        let kind = super::ai::kind_from_label(a["kind"].as_str().unwrap_or("Phrase"));
                        let (_, created) = self.graph.upsert(kind, value, center);
                        if created { done.push(format!("added [{}] '{value}'", kind.label())); }
                    }
                }
                "delete" => {
                    if let Some(id) = self.id_by_value(target) {
                        self.graph.remove(id);
                        done.push(format!("deleted '{target}'"));
                    }
                }
                "link" => {
                    let from = a["from"].as_str().unwrap_or("").trim();
                    let to = a["to"].as_str().unwrap_or("").trim();
                    if let (Some(x), Some(y)) = (self.id_by_value(from), self.id_by_value(to)) {
                        self.graph.link(x, y, a["label"].as_str().unwrap_or("").to_string());
                        done.push(format!("linked '{from}' → '{to}'"));
                    }
                }
                "note" => {
                    if let Some(id) = self.id_by_value(target) {
                        if let Some(e) = self.graph.entities.get_mut(&id) { e.note = value.to_string(); }
                        done.push(format!("noted '{target}'"));
                    }
                }
                "flag" => {
                    if let Some(id) = self.id_by_value(target) {
                        let f = a["flag"].as_u64().unwrap_or(0).min(3) as u8;
                        if let Some(e) = self.graph.entities.get_mut(&id) { e.flag = f; }
                        done.push(format!("flagged '{target}' ({})",
                            ["none","important","verified","target"][f as usize]));
                    }
                }
                "unlink" => {
                    let from = a["from"].as_str().unwrap_or("").trim();
                    let to = a["to"].as_str().unwrap_or("").trim();
                    if let (Some(x), Some(y)) = (self.id_by_value(from), self.id_by_value(to)) {
                        let before = self.graph.edges.len();
                        self.graph.edges.retain(|e|
                            !((e.from == x && e.to == y) || (e.from == y && e.to == x)));
                        if self.graph.edges.len() < before { done.push(format!("unlinked '{from}' ✗ '{to}'")); }
                    }
                }
                "set_kind" => {
                    if let Some(id) = self.id_by_value(target) {
                        let k = super::ai::kind_from_label(value);
                        if let Some(e) = self.graph.entities.get_mut(&id) { e.kind = k; }
                        done.push(format!("retyped '{target}' → {}", k.label()));
                    }
                }
                "layout" => { canvas::auto_layout(&mut self.graph); done.push("re-laid out the graph".into()); }
                "run" => {
                    let tid = a["transform"].as_str().unwrap_or("").trim();
                    if let (Some(id), false) = (self.id_by_value(target), tid.is_empty()) {
                        self.run_transform(id, tid);
                        done.push(format!("ran '{tid}' on '{target}'"));
                    }
                }
                _ => {}
            }
        }
        if !done.is_empty() { self.needs_fit = true; self.log(format!("⚙  agent: {}", done.join("; "))); }
        done.join("; ")
    }

    /// Agent "plan next steps": ask the AI which entities to expand next; the
    /// suggested nodes get flagged (highlighted) and a plan is shown in the chat.
    fn agent_plan(&mut self) {
        if self.graph.entities.is_empty() { self.status = "add some entities first".into(); return; }
        let Some(cfg) = super::ai::cfg() else { self.status = "no AI key set".into(); return; };
        let system = "You are an OSINT investigation planner. Given the current graph, decide the \
            most valuable NEXT steps. Respond with STRICT JSON only: \
            {\"steps\":[{\"value\":\"<exact entity value from the graph>\",\"action\":\"what to do / which transform\",\"reason\":\"why\"}]} \
            Pick 3-6 steps, referencing entity values that appear verbatim in the graph. No markdown.".to_string();
        let user = self.graph_context();
        let tx = self.tx.clone();
        self.ai_busy = true;
        self.status = "planning next steps…".into();
        self.show_chat = true;
        self.rt.spawn(async move {
            let res = super::ai::complete(&cfg, &system, &user).await;
            let _ = tx.send(Msg::Plan(res));
        });
    }

    /// Apply an agent plan: flag the referenced entities and post the plan to chat.
    fn apply_plan(&mut self, text: &str) {
        let v: serde_json::Value = match super::ai::parse_json(text) {
            Ok(v) => v,
            Err(e) => { self.log(format!("✗  plan parse: {e}")); return; }
        };
        let mut out = String::from("◉ Suggested next steps:\n");
        let mut flagged = 0;
        if let Some(steps) = v["steps"].as_array() {
            for (i, st) in steps.iter().enumerate() {
                let value = st["value"].as_str().unwrap_or("").trim();
                let action = st["action"].as_str().unwrap_or("");
                let reason = st["reason"].as_str().unwrap_or("");
                out.push_str(&format!("{}. {value} — {action}\n   {reason}\n", i + 1));
                // highlight the matching node(s)
                let vl = value.to_lowercase();
                for e in self.graph.entities.values_mut() {
                    if e.value.to_lowercase() == vl { e.flag = 3; flagged += 1; }
                }
            }
        }
        self.chat.push((super::ai::ChatRole::Assistant, out));
        self.status = format!("✓  plan ready — {flagged} node(s) highlighted");
        self.log(format!("◉  agent flagged {flagged} node(s) for next steps"));
    }

    /// Bring a finished dossier onto the graph: a subject node, its online links
    /// as connected Social/Website nodes, and key facts copied as properties.
    pub fn ingest_dossier(&mut self, seed: super::dossier::DossierSeed) {
        self.record();
        let kind = if seed.is_org { Kind::Organization } else { Kind::Person };
        let pos = self.view_center_world();
        let (root, _) = self.graph.upsert(kind, &seed.title, pos);
        if let Some(e) = self.graph.entities.get_mut(&root) {
            if !seed.subtitle.is_empty() {
                e.props.push(("description".into(), seed.subtitle.clone()));
            }
            for (k, v) in &seed.facts {
                if !e.props.iter().any(|(ek, _)| ek == k) { e.props.push((k.clone(), v.clone())); }
            }
            e.flag = 3; // target
        }
        let n = seed.links.len().max(1) as f32;
        for (i, (label, url)) in seed.links.iter().enumerate() {
            let lk = url.to_lowercase();
            let k = if lk.contains("twitter.com") || lk.contains("instagram.com")
                || lk.contains("facebook.com") || lk.contains("github.com") { Kind::Social }
                else { Kind::Website };
            let ang = std::f32::consts::TAU * (i as f32) / n;
            let cp = Pos2::new(pos.x + ang.cos() * 160.0, pos.y + ang.sin() * 160.0);
            let (cid, _) = self.graph.upsert(k, url, cp);
            self.graph.link(root, cid, label.clone());
        }
        self.sel.select_one(root);
        self.needs_fit = true;
        self.log(format!("✓  dossier '{}' added to graph", seed.title));
    }

    /// Spawn an async transform against entity `source_id`.
    fn run_transform(&mut self, source_id: u64, transform_id: &str) {
        let Some(e) = self.graph.entities.get(&source_id) else { return };
        let value = e.value.clone();
        let tid = transform_id.to_string();
        let tx = self.tx.clone();
        self.ran.entry(source_id).or_default().insert(tid.clone()); // coverage tracking

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
        transforms::applies_to(tid, kind)
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
                Msg::Flag { id, flag, note } => {
                    if let Some(e) = self.graph.entities.get_mut(&id) {
                        e.flag = flag;
                        if !note.is_empty() && !e.props.iter().any(|(k, _)| k == "instinct") {
                            e.props.push(("instinct".into(), note.clone()));
                        }
                    }
                    if !note.is_empty() { self.log(format!("🚩  {note}")); }
                }
                Msg::RulesLoaded(src) => {
                    if src.trim().is_empty() { self.log("✗  rule-pack empty or unreadable"); }
                    else { self.advisor_rules = src; self.show_advisor = true;
                           self.log("✓  Instinct rule-pack imported"); }
                }
                Msg::ImagePicked(id, path) => {
                    if self.graph.entities.contains_key(&id) {
                        self.record();
                        if let Some(e) = self.graph.entities.get_mut(&id) { e.image = Some(path.clone()); }
                        self.log(format!("✓  image set: {path}"));
                    }
                }
                Msg::AiGraph(res) => {
                    self.ai_busy = false;
                    match res {
                        Ok(text) => self.apply_ai_plan(&text),
                        Err(e) => { self.status = "ai failed".into(); self.log(format!("✗  AI: {e}")); }
                    }
                }
                Msg::Chat(res) => {
                    self.chat_busy = false;
                    match res {
                        Ok(t) => self.apply_chat_actions(&t),
                        Err(e) => self.chat.push((super::ai::ChatRole::Assistant, format!("✗ {e}"))),
                    }
                }
                Msg::Plan(res) => {
                    self.ai_busy = false;
                    match res {
                        Ok(t) => self.apply_plan(&t),
                        Err(e) => { self.log(format!("✗  AI plan: {e}")); self.status = "plan failed".into(); }
                    }
                }
            }
        }
        if got { self.needs_fit = false; }
        if self.running > 0 { ctx.request_repaint(); }
    }

    fn apply_outcome(&mut self, source_id: u64, _transform: &str, outcome: transforms::Outcome) {
        if !outcome.items.is_empty() || !outcome.props.is_empty() { self.record(); }
        for line in outcome.log { self.log(line); }
        if !outcome.props.is_empty() {
            self.graph.merge_props(source_id, &outcome.props);
        }

        let mut origin = self.graph.entities.get(&source_id).map(|e| e.pos).unwrap_or_default();
        if !origin.x.is_finite() || !origin.y.is_finite() { origin = Pos2::ZERO; }
        let base_deg = self.graph.degree(source_id);
        let total = outcome.items.len().max(1);

        for (i, item) in outcome.items.into_iter().enumerate() {
            // Fan children out on a ring around the source (+ jitter so children
            // from repeated transforms never land on the exact same point).
            let ang = std::f32::consts::TAU * ((base_deg + i) as f32) / (total as f32 + base_deg as f32 + 1.0)
                + (i as f32 * 0.37);
            let radius = 150.0 + (i as f32 % 5.0) * 22.0;
            let pos = Pos2::new(origin.x + radius * ang.cos(), origin.y + radius * ang.sin());

            let (child, created) = self.graph.upsert(item.kind, &item.value, pos);
            if created && !item.props.is_empty() {
                self.graph.merge_props(child, &item.props);
            }
            self.graph.link(source_id, child, item.edge);
        }
    }

    /// Snapshot the graph for undo. Call before any structural mutation.
    fn record(&mut self) {
        self.undo.push(self.graph.to_data());
        if self.undo.len() > 80 { self.undo.remove(0); }
        self.redo.clear();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(self.graph.to_data());
            self.graph = Graph::from_data(prev);
            self.sel.clear();
            self.log("↶  undo");
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.graph.to_data());
            self.graph = Graph::from_data(next);
            self.sel.clear();
            self.log("↷  redo");
        }
    }

    fn delete_selected(&mut self) {
        if self.sel.set.is_empty() { return; }
        self.record();
        let ids: Vec<u64> = self.sel.set.iter().copied().collect();
        for id in &ids { self.graph.remove(*id); }
        let n = ids.len();
        self.sel.clear();
        self.log(format!("⊘  removed {n} entit{}", if n == 1 { "y" } else { "ies" }));
    }

    fn save_graph(&mut self) {
        let path = self.save_path.trim().to_string();
        // Maltego export
        if path.to_lowercase().ends_with(".mtgx") {
            match super::mtgx::export(&path, &self.graph) {
                Ok(_)  => self.log(format!("✓  exported {} entities to Maltego → {path}", self.graph.entities.len())),
                Err(e) => self.log(format!("✗  .mtgx export failed: {e}")),
            }
            return;
        }
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
        let lower = path.to_lowercase();
        // CSV batch import: "kind,value" or just values (kind guessed)
        if lower.ends_with(".csv") || lower.ends_with(".txt") {
            self.import_csv(&path);
            return;
        }
        // Maltego import
        if path.to_lowercase().ends_with(".mtgx") {
            self.record();
            match super::mtgx::import(&path) {
                Ok(g) => {
                    self.graph = g;
                    self.sel.clear();
                    self.needs_fit = true;
                    canvas::auto_layout(&mut self.graph);
                    self.log(format!("✓  imported {} entities from Maltego ← {path}", self.graph.entities.len()));
                }
                Err(e) => self.log(format!("✗  .mtgx import failed: {e}")),
            }
            return;
        }
        match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(data) => {
                    self.record();
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

    /// Import a CSV/TXT file: each line is `kind,value` or just a value (kind is
    /// guessed). Adds entities (and links them to nothing).
    fn import_csv(&mut self, path: &str) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t, Err(e) => { self.log(format!("✗  read failed: {e}")); return; }
        };
        self.record();
        let mut n = 0;
        let mut col = 0.0f32;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let (kind, value) = match line.split_once(',') {
                Some((k, v)) => (kind_from_name(k.trim()).unwrap_or_else(|| guess_kind(v.trim())), v.trim().to_string()),
                None => (guess_kind(line), line.to_string()),
            };
            let pos = Pos2::new((n as f32 % 10.0) * 140.0 - 630.0, (n as f32 / 10.0).floor() * 120.0 - 300.0);
            self.graph.add(kind, value, pos);
            n += 1; col += 1.0;
        }
        let _ = col;
        self.sel.clear();
        self.needs_fit = true;
        self.log(format!("✓  imported {n} entities from CSV ← {path}"));
    }

    // ── Rendering ──────────────────────────────────────────────────────────────

    pub fn ui(&mut self, ctx: &egui::Context) {
        self.drain_messages(ctx);
        self.sync_textures(ctx);
        self.machine_tick();
        self.autosave(ctx.input(|i| i.time));
        // Instinct auto-mode: keep the badge (warning count) fresh even when closed
        if self.advisor_auto {
            let facts = self.compute_facts();
            self.advisor_badge = super::lisp::advise_with(&self.advisor_rules, &facts)
                .iter().filter(|s| s.level == 2).count();
        } else { self.advisor_badge = 0; }
        // refresh node risk scores (throttled)
        let now = ctx.input(|i| i.time);
        if now - self.risk_t > 0.5 { self.node_risk = self.compute_risk(); self.risk_t = now; }
        if self.machine.is_some() { ctx.request_repaint(); }

        // Drive the canvas off a VIRTUAL clock while recording (one fixed step
        // per captured frame) so the animation is smooth in the output, then stop
        // once the reveal + tail is over.
        if let Some(rec) = &self.recording {
            let wall = ctx.input(|i| i.time);
            let elapsed = wall - rec.wall_start;
            let vt = rec.start_time + rec.idx as f64 / REC_FPS;
            canvas::set_render_time(Some(vt));
            // stop when: reveal done · OR backend isn't returning frames (early
            // abort) · OR a hard wall-clock cap so it can never run forever.
            let stalled = elapsed > 2.5 && rec.idx == 0;
            if vt >= rec.end_time || stalled || elapsed > 90.0 {
                self.finish_video();
                canvas::set_render_time(None);
            }
        } else {
            canvas::set_render_time(None);
        }

        // Pick up a requested screenshot (single export OR a video frame).
        if self.pending_shot.is_some() || self.recording.is_some() {
            let shot = ctx.input(|i| i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            }));
            if let Some(img) = shot {
                let ppp = ctx.pixels_per_point();
                if self.recording.is_some() {
                    self.save_frame(img, ppp);
                } else if let Some(fmt) = self.pending_shot.take() {
                    self.save_shot(img, fmt, ppp);
                }
            }
            // keep requesting frames while recording
            if self.recording.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                ctx.request_repaint();
            }
        }

        // Global shortcuts — but never while the user is typing in a text field
        // (otherwise Backspace/Delete would nuke the selected node).
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::F1) || i.key_pressed(egui::Key::Questionmark)) {
                self.show_help = !self.show_help;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                if let Some(ei) = self.sel.edge.take() {
                    if ei < self.graph.edges.len() { self.record(); self.graph.edges.remove(ei); self.log("⊘  link removed"); }
                } else if !self.sel.set.is_empty() {
                    self.delete_selected();
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F)) { self.needs_fit = true; }
            if ctx.input(|i| i.key_pressed(egui::Key::L) && !i.modifiers.ctrl) {
                self.record();
                canvas::auto_layout(&mut self.graph);
                self.needs_fit = true;
            }
            // undo / redo
            if ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z)) { self.undo(); }
            if ctx.input(|i| (i.modifiers.command && i.key_pressed(egui::Key::Y))
                || (i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z))) { self.redo(); }
            // run default transform on the selected node
            if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(id) = self.sel.primary { self.run_default(id); }
            }
            // select all
            if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
                let ids: Vec<u64> = self.graph.entities.keys().copied().collect();
                self.sel.edge = None;
                self.sel.set = ids.iter().copied().collect();
                self.sel.primary = ids.first().copied();
            }
        }

        // Ctrl+K toggles the command palette
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K)) {
            self.show_palette_cmd = !self.show_palette_cmd;
            self.cmd_query.clear();
        }

        self.toolbar(ctx);
        self.palette(ctx);
        self.instinct_panel(ctx);
        self.coverage_panel(ctx);
        self.chat_panel(ctx);
        self.details_panel(ctx);
        self.log_panel(ctx);
        self.canvas_panel(ctx);
        // hide floating windows while recording so they don't appear in the video
        if self.recording.is_none() {
            self.context_menu(ctx);
            self.table_window(ctx);
            self.add_window(ctx);
            self.ai_window(ctx);
            self.quick_window(ctx);
            self.command_palette(ctx);
            self.help_window(ctx);
            self.analytics_window(ctx);
        }
    }

    /// Graph analytics — degree centrality, connected components, density…
    fn analytics_window(&mut self, ctx: &egui::Context) {
        if !self.show_analytics { return; }
        let g = &self.graph;
        let n = g.entities.len();
        let e = g.edges.len();

        // undirected adjacency + degree
        let mut deg: std::collections::HashMap<u64, usize> = g.entities.keys().map(|&k| (k, 0)).collect();
        let mut adj: std::collections::HashMap<u64, Vec<u64>> = g.entities.keys().map(|&k| (k, Vec::new())).collect();
        for ed in &g.edges {
            if let Some(d) = deg.get_mut(&ed.from) { *d += 1; }
            if let Some(d) = deg.get_mut(&ed.to)   { *d += 1; }
            if adj.contains_key(&ed.from) && adj.contains_key(&ed.to) {
                adj.get_mut(&ed.from).unwrap().push(ed.to);
                adj.get_mut(&ed.to).unwrap().push(ed.from);
            }
        }
        // connected components (BFS)
        let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
        let mut comps = 0usize;
        let mut largest = 0usize;
        for &start in adj.keys() {
            if seen.contains(&start) { continue; }
            comps += 1;
            let mut size = 0;
            let mut q = std::collections::VecDeque::from([start]);
            seen.insert(start);
            while let Some(x) = q.pop_front() {
                size += 1;
                for &y in &adj[&x] { if seen.insert(y) { q.push_back(y); } }
            }
            largest = largest.max(size);
        }
        let density = if n > 1 { 2.0 * e as f64 / (n as f64 * (n as f64 - 1.0)) } else { 0.0 };
        let avg_deg = if n > 0 { 2.0 * e as f64 / n as f64 } else { 0.0 };
        let isolates: Vec<u64> = deg.iter().filter(|(_, &d)| d == 0).map(|(&k, _)| k).collect();

        // top by degree
        let mut top: Vec<(u64, usize)> = deg.iter().map(|(&k, &d)| (k, d)).collect();
        top.sort_by(|a, b| b.1.cmp(&a.1));
        top.truncate(8);

        // by-kind counts
        let mut by_kind: std::collections::HashMap<&'static str, usize> = std::collections::HashMap::new();
        for ent in g.entities.values() { *by_kind.entry(i18n::kind_label(ent.kind)).or_default() += 1; }
        let mut by_kind: Vec<(&str, usize)> = by_kind.into_iter().collect();
        by_kind.sort_by(|a, b| b.1.cmp(&a.1));

        let mut open = true;
        let mut focus: Option<u64> = None;
        let mut select_isolates = false;
        egui::Window::new(RichText::new("∑  Graph analytics").color(text_pri()).strong())
            .open(&mut open).default_width(340.0)
            .frame(egui::Frame::window(&ctx.style()).fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                let row = |ui: &mut egui::Ui, k: &str, v: String| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(k).color(text_sec()).size(12.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(v).color(text_pri()).strong().size(12.0));
                        });
                    });
                };
                row(ui, "Entities", n.to_string());
                row(ui, "Links", e.to_string());
                row(ui, "Density", format!("{:.3}", density));
                row(ui, "Avg degree", format!("{:.2}", avg_deg));
                row(ui, "Components (clusters)", comps.to_string());
                row(ui, "Largest cluster", largest.to_string());
                row(ui, "Isolates", isolates.len().to_string());
                if !isolates.is_empty() {
                    if ui.add(egui::Button::new(RichText::new("select isolates").color(accent()).size(11.0))
                        .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, border()))
                        .rounding(Rounding::same(4.0))).clicked() { select_isolates = true; }
                }

                ui.add_space(8.0); ui.separator();
                ui.label(RichText::new("MOST CONNECTED (degree centrality)").color(text_mut()).size(10.0).strong());
                ui.add_space(2.0);
                for (id, d) in &top {
                    if *d == 0 { continue; }
                    if let Some(ent) = self.graph.entities.get(id) {
                        let r = ui.add(egui::Label::new(RichText::new(format!("{}  {}  ·  {d}",
                            ent.kind.icon(), truncate(&ent.value, 26))).color(text_pri()).size(11.5))
                            .sense(egui::Sense::click()));
                        if r.clicked() { focus = Some(*id); }
                        if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                    }
                }

                ui.add_space(8.0); ui.separator();
                ui.label(RichText::new("BRIDGES (betweenness centrality)").color(text_mut()).size(10.0).strong());
                ui.add_space(2.0);
                if n <= 400 {
                    let bc = canvas::betweenness(&self.graph);
                    let mut bt: Vec<(u64, f64)> = bc.into_iter().filter(|(_, v)| *v > 0.0).collect();
                    bt.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    bt.truncate(6);
                    if bt.is_empty() {
                        ui.label(RichText::new("— (no bridging nodes)").color(text_mut()).size(11.0));
                    }
                    for (id, score) in &bt {
                        if let Some(ent) = self.graph.entities.get(id) {
                            let r = ui.add(egui::Label::new(RichText::new(format!("{}  {}  ·  {:.1}",
                                ent.kind.icon(), truncate(&ent.value, 24), score)).color(text_pri()).size(11.5))
                                .sense(egui::Sense::click()));
                            if r.clicked() { focus = Some(*id); }
                            if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                        }
                    }
                } else {
                    ui.label(RichText::new("(skipped — graph too large)").color(text_mut()).size(11.0));
                }

                ui.add_space(8.0); ui.separator();
                ui.label(RichText::new("BY TYPE").color(text_mut()).size(10.0).strong());
                ui.add_space(2.0);
                for (k, c) in &by_kind { row(ui, k, c.to_string()); }
            });

        if select_isolates {
            self.sel.clear();
            for id in isolates { self.sel.set.insert(id); self.sel.primary = Some(id); }
        }
        if let Some(id) = focus { self.focus(id); }
        if !open { self.show_analytics = false; }
    }

    /// Quick add-entity popup (handy everywhere; the only way to add in Focus).
    /// The graph AI assistant — a chat panel that knows the current graph, plus
    /// an agent "plan next steps" button that highlights nodes to expand.
    fn chat_panel(&mut self, ctx: &egui::Context) {
        if !self.show_chat { return; }
        let mut send = false;
        let mut plan = false;
        let mut clear = false;
        let mut quick: Option<&str> = None;
        egui::SidePanel::right("graph_chat")
            .resizable(true).default_width(360.0).width_range(280.0..=560.0)
            .frame(egui::Frame::none().fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                egui::TopBottomPanel::top("gchat_hdr")
                    .frame(egui::Frame::none().fill(bg_sidebar())
                        .inner_margin(Margin::symmetric(12.0, 7.0)).stroke(Stroke::new(1.0, border())))
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("❝ {}", i18n::tr("gr.chat_title")))
                                .color(text_pri()).strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if toolbtn(ui, "✗").clicked() { clear = true; }
                                let pl = if self.ai_busy { "◌" } else { "◉" };
                                if toolbtn(ui, pl).on_hover_text(i18n::tr("gr.chat_plan")).clicked() { plan = true; }
                            });
                        });
                    });
                egui::TopBottomPanel::bottom("gchat_in")
                    .frame(egui::Frame::none().fill(bg_input())
                        .inner_margin(Margin::symmetric(8.0, 7.0)).stroke(Stroke::new(1.0, border())))
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            let r = ui.add(TextEdit::singleline(&mut self.chat_input)
                                .desired_width(ui.available_width() - 52.0)
                                .hint_text(i18n::tr("gr.chat_ph")));
                            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            let click = ui.add_enabled(!self.chat_busy, egui::Button::new(
                                RichText::new("➤").color(Color32::WHITE).strong())
                                .fill(accent()).rounding(Rounding::same(corner()))).clicked();
                            if (enter || click) && !self.chat_input.trim().is_empty() { send = true; r.request_focus(); }
                        });
                    });
                // quick-action chips
                egui::TopBottomPanel::top("gchat_chips")
                    .frame(egui::Frame::none().inner_margin(Margin::symmetric(8.0, 5.0)))
                    .show_inside(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (icon, prompt) in [
                                ("✦ Summarize", "Summarize what this graph shows in a few sentences."),
                                ("◌ Find gaps", "What's missing or weakly connected? Suggest links."),
                                ("◎ Mark targets", "Flag the most important nodes as targets and add short notes."),
                                ("⊹ Tidy", "Re-layout and clean up the graph."),
                            ] {
                                if ui.add(egui::Button::new(RichText::new(icon).color(text_sec()).size(11.0))
                                    .fill(bg_item_hov()).stroke(Stroke::new(1.0, border()))
                                    .rounding(Rounding::same(12.0))).clicked() { quick = Some(prompt); }
                            }
                        });
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().inner_margin(Margin::symmetric(10.0, 8.0)))
                    .show_inside(ui, |ui| {
                        ScrollArea::vertical().auto_shrink([false; 2]).stick_to_bottom(true).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if self.chat.is_empty() {
                                ui.label(RichText::new(i18n::tr("gr.chat_hello")).color(text_mut()).size(11.5).italics());
                            }
                            for (role, text) in &self.chat {
                                let me = *role == super::ai::ChatRole::User;
                                let (fill, col) = if me { (bg_item_sel(), text_pri()) } else { (bg_item_hov(), text_sec()) };
                                egui::Frame::none().fill(fill).rounding(Rounding::same(8.0))
                                    .inner_margin(Margin::symmetric(10.0, 7.0)).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(RichText::new(if me { "you" } else { "ai" })
                                            .color(if me { accent() } else { c_info() }).size(9.5).strong());
                                        ui.label(RichText::new(text).color(col).size(12.5));
                                    });
                                ui.add_space(5.0);
                            }
                            if self.chat_busy || self.ai_busy {
                                ui.horizontal(|ui| {
                                    super::logo::widget_anim(ui, 13.0);
                                    ui.add_space(4.0);
                                    ui.label(RichText::new("thinking…").color(text_mut()).size(11.5).italics());
                                });
                            }
                        });
                    });
            });
        if clear { self.chat.clear(); }
        if plan && !self.ai_busy { self.agent_plan(); }
        if let Some(p) = quick { if !self.chat_busy { self.chat_input = p.to_string(); self.send_chat(); } }
        if send { self.send_chat(); }
        if self.chat_busy || self.ai_busy { ctx.request_repaint(); }
    }

    /// Build a self-contained HTML investigation report (graph + flags + Instinct
    /// findings) and write it to the home directory.
    fn export_report(&mut self) {
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let facts = self.compute_facts();
        let hints = super::lisp::advise_with(&self.advisor_rules, &facts);
        let mut h = String::new();
        h.push_str("<!doctype html><meta charset=utf-8><title>Parasite report</title>");
        h.push_str("<style>body{font:14px/1.5 -apple-system,Segoe UI,sans-serif;max-width:880px;margin:40px auto;padding:0 16px;color:#1d1d1f}\
h1{font-size:24px}h2{margin-top:28px;border-bottom:1px solid #e5e5e7;padding-bottom:4px}\
table{border-collapse:collapse;width:100%}td,th{border:1px solid #e5e5e7;padding:6px 9px;text-align:left;font-size:13px}\
.f1{color:#d6483e}.f2{color:#1fa868}.f3{color:#cf8e20}.tag{color:#86868b;font-size:12px}\
.hint{background:#f5f5f7;border-radius:10px;padding:10px 14px;margin:8px 0}.w{border-left:3px solid #cf8e20}</style>");
        h.push_str(&format!("<h1>🦠 Parasite — Investigation Report</h1><p class=tag>{} entities · {} links · {} components · {} cycles</p>",
            facts.nodes, facts.edges, facts.components, facts.cycles));

        if !hints.is_empty() {
            h.push_str("<h2>λ Instinct findings</h2>");
            for s in &hints {
                let cls = if s.level == 2 { " w" } else { "" };
                h.push_str(&format!("<div class='hint{cls}'><b>{}</b><br>{}</div>", esc(&s.title), esc(&s.detail)));
            }
        }

        h.push_str("<h2>Entities</h2><table><tr><th>Kind</th><th>Value</th><th>Flag</th><th>Notes / props</th></tr>");
        let mut ents: Vec<_> = self.graph.entities.values().collect();
        ents.sort_by_key(|e| e.kind.label());
        for e in &ents {
            let flag = match e.flag { 1 => "<span class=f1>● risk</span>", 2 => "<span class=f2>● ok</span>",
                                      3 => "<span class=f3>● target</span>", _ => "" };
            let props: String = e.props.iter().map(|(k, v)| format!("{}={}", esc(k), esc(v)))
                .collect::<Vec<_>>().join("; ");
            let note = if e.note.is_empty() { String::new() } else { format!("<i>{}</i> ", esc(&e.note)) };
            h.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{flag}</td><td>{note}<span class=tag>{props}</span></td></tr>",
                e.kind.label(), esc(&e.value)));
        }
        h.push_str("</table>");

        if !self.graph.edges.is_empty() {
            h.push_str("<h2>Links</h2><table><tr><th>From</th><th>→</th><th>To</th></tr>");
            for ed in &self.graph.edges {
                let f = self.graph.entities.get(&ed.from).map(|e| e.value.clone()).unwrap_or_default();
                let t = self.graph.entities.get(&ed.to).map(|e| e.value.clone()).unwrap_or_default();
                h.push_str(&format!("<tr><td>{}</td><td class=tag>{}</td><td>{}</td></tr>", esc(&f), esc(&ed.label), esc(&t)));
            }
            h.push_str("</table>");
        }
        h.push_str("<p class=tag style='margin-top:30px'>Generated by parasite · rule-based, no AI in this report.</p>");

        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir)
            .join(format!("parasite-report-{ts}.html"));
        match std::fs::write(&path, h) {
            Ok(_)  => { self.status = format!("✓  report saved: {}", path.display());
                        self.log(format!("✓  report → {}", path.display())); }
            Err(e) => self.status = format!("✗  report failed: {e}"),
        }
    }

    /// Deterministic per-node risk score 0-100: flags + Instinct findings + the
    /// reputation/liveness notes auto-triage wrote. No AI.
    fn compute_risk(&self) -> std::collections::HashMap<u64, u8> {
        use std::collections::HashMap;
        let mut risk: HashMap<u64, f32> = HashMap::new();
        for e in self.graph.entities.values() {
            let mut s = match e.flag { 1 => 60.0, 3 => 35.0, 2 => -25.0, _ => 0.0 };
            for (k, v) in &e.props {
                let vl = v.to_lowercase();
                if k == "instinct" || k.starts_with("greynoise") {
                    if vl.contains("malicious") { s += 35.0; }
                    if vl.contains("benign") { s -= 15.0; }
                    if vl.contains("noise") { s += 10.0; }
                }
                if vl.contains("unreachable") || vl.contains("http 4") || vl.contains("http 5") { s += 15.0; }
            }
            risk.insert(e.id, s);
        }
        // Instinct findings raise the risk of the nodes they target
        let facts = self.compute_facts();
        for sug in super::lisp::advise_with(&self.advisor_rules, &facts) {
            if sug.level >= 1 && !sug.select.is_empty() {
                let add = if sug.level == 2 { 22.0 } else { 9.0 };
                for id in self.advisor_targets(&sug.select) { *risk.entry(id).or_insert(0.0) += add; }
            }
        }
        risk.into_iter().map(|(id, v)| (id, v.clamp(0.0, 100.0) as u8)).collect()
    }

    /// Compute deterministic facts about the graph for the Lisp advisor.
    fn compute_facts(&self) -> super::lisp::Facts {
        use std::collections::HashMap;
        let mut f = super::lisp::Facts {
            nodes: self.graph.entities.len(),
            edges: self.graph.edges.len(),
            ..Default::default()
        };
        for e in self.graph.entities.values() {
            *f.kinds.entry(kind_tag(e.kind).to_string()).or_insert(0) += 1;
            if e.flag != 0 { f.flagged += 1; }
        }
        // degrees → isolated / leaves / hubs / max degree
        let mut deg: HashMap<u64, usize> = HashMap::new();
        for ed in &self.graph.edges {
            *deg.entry(ed.from).or_insert(0) += 1;
            *deg.entry(ed.to).or_insert(0) += 1;
        }
        for id in self.graph.entities.keys() {
            let d = deg.get(id).copied().unwrap_or(0);
            match d { 0 => f.isolated += 1, 1 => f.leaves += 1, _ => {} }
            if d >= 5 { f.hubs += 1; }
            f.max_degree = f.max_degree.max(d);
        }
        // connected components (union-find) → components + independent cycles
        let ids: Vec<u64> = self.graph.entities.keys().copied().collect();
        let idx: HashMap<u64, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let mut parent: Vec<usize> = (0..ids.len()).collect();
        fn find(p: &mut [usize], x: usize) -> usize {
            let mut r = x;
            while p[r] != r { r = p[r]; }
            let mut c = x;
            while p[c] != r { let n = p[c]; p[c] = r; c = n; }
            r
        }
        for ed in &self.graph.edges {
            if let (Some(&a), Some(&b)) = (idx.get(&ed.from), idx.get(&ed.to)) {
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                if ra != rb { parent[ra] = rb; }
            }
        }
        if !ids.is_empty() {
            let mut roots: std::collections::HashSet<usize> = std::collections::HashSet::new();
            for i in 0..ids.len() { let r = find(&mut parent, i); roots.insert(r); }
            f.components = roots.len();
            // independent cycles = E - N + components (clamped at 0)
            f.cycles = (self.graph.edges.len() + f.components).saturating_sub(self.graph.entities.len());
        }
        // duplicate values: same value string on more than one node
        let mut valcount: HashMap<String, usize> = HashMap::new();
        for e in self.graph.entities.values() {
            *valcount.entry(e.value.trim().to_lowercase()).or_insert(0) += 1;
        }
        f.duplicates = valcount.values().filter(|&&c| c > 1).count();
        // shared property values: key → largest group sharing one value
        let mut groups: HashMap<String, HashMap<String, usize>> = HashMap::new();
        for e in self.graph.entities.values() {
            for (k, v) in &e.props {
                if v.trim().is_empty() { continue; }
                *groups.entry(k.to_lowercase()).or_default()
                    .entry(v.to_lowercase()).or_insert(0) += 1;
            }
        }
        for (k, vals) in groups {
            let total: usize = vals.values().sum();
            if let Some(m) = vals.values().copied().max() { f.shared.insert(k.clone(), m); }
            f.props.insert(k, total);
        }
        f.distinct = f.kinds.len();
        f.avg_degree = if f.nodes > 0 { 2.0 * f.edges as f64 / f.nodes as f64 } else { 0.0 };

        // co-hosting: domains that share an IP with another domain
        {
            let mut ip_domains: HashMap<u64, Vec<u64>> = HashMap::new();
            for ed in &self.graph.edges {
                for (a, b) in [(ed.from, ed.to), (ed.to, ed.from)] {
                    if let (Some(ea), Some(eb)) = (self.graph.entities.get(&a), self.graph.entities.get(&b)) {
                        if ea.kind == Kind::Ip && eb.kind == Kind::Domain {
                            ip_domains.entry(a).or_default().push(b);
                        }
                    }
                }
            }
            let mut cohosted: std::collections::HashSet<u64> = std::collections::HashSet::new();
            for doms in ip_domains.values() {
                if doms.len() >= 2 { for d in doms { cohosted.insert(*d); } }
            }
            f.cohosted = cohosted.len();
        }
        // largest connected component
        if !ids.is_empty() {
            let mut size: HashMap<usize, usize> = HashMap::new();
            for i in 0..ids.len() { let r = find(&mut parent, i); *size.entry(r).or_insert(0) += 1; }
            f.biggest = size.values().copied().max().unwrap_or(0);
        }
        // coverage-awareness: how many nodes of a kind never ran a key check
        let key_checks = [("ip", "ip_greynoise"), ("email", "email_hibp"), ("domain", "dom_whois"),
                          ("domain", "dom_resolve"), ("ip", "ip_internetdb"), ("username", "user_hunt")];
        for (ktag, tid) in key_checks {
            let miss = self.graph.entities.values()
                .filter(|e| kind_tag(e.kind) == ktag && !self.ran.get(&e.id).map_or(false, |s| s.contains(tid)))
                .count();
            f.unrun.insert(format!("{ktag}|{tid}"), miss);
        }
        f
    }

    /// Node ids matching an advisor `select` string ("kind:ip", "isolated",
    /// "leaves", "hubs", "flagged", "shared:registrar").
    fn advisor_targets(&self, select: &str) -> Vec<u64> {
        use std::collections::HashMap;
        let mut deg: HashMap<u64, usize> = HashMap::new();
        for ed in &self.graph.edges {
            *deg.entry(ed.from).or_insert(0) += 1;
            *deg.entry(ed.to).or_insert(0) += 1;
        }
        let d = |id: u64| deg.get(&id).copied().unwrap_or(0);
        let (kind, arg) = select.split_once(':').unwrap_or((select, ""));
        let mut out = Vec::new();
        for (id, e) in &self.graph.entities {
            let hit = match kind {
                "kind"     => kind_tag(e.kind) == arg,
                "isolated" => d(*id) == 0,
                "leaves"   => d(*id) == 1,
                "hubs"     => d(*id) >= 5,
                "flagged"  => e.flag != 0,
                "shared"   => {
                    // nodes sharing the most-common value of property `arg`
                    e.props.iter().any(|(k, _)| k.eq_ignore_ascii_case(arg))
                }
                _ => false,
            };
            if hit { out.push(*id); }
        }
        out
    }

    /// λ Instinct — a docked panel that runs the (user-editable) Lisp rule brain
    /// over the graph and lists suggestions. Each can highlight the nodes it refers
    /// to and run its transform/machine in one click. Deterministic; no AI.
    fn instinct_panel(&mut self, ctx: &egui::Context) {
        if !self.show_advisor { return; }
        let facts = self.compute_facts();
        let hints = super::lisp::advise_with(&self.advisor_rules, &facts);
        let mut edit = self.advisor_edit;
        let mut close = false;
        let mut do_select: Option<String> = None;
        let mut do_run: Option<(String, String)> = None;
        let mut save_rules = false;
        let mut reset_rules = false;
        let mut export_pack = false;
        let mut import_pack = false;
        let mut triage = false;
        let mut auto = self.advisor_auto;
        let (warns, tips) = (hints.iter().filter(|s| s.level == 2).count(),
                             hints.iter().filter(|s| s.level == 1).count());

        egui::SidePanel::left("graph_instinct")
            .resizable(true).default_width(312.0).width_range(250.0..=460.0)
            .frame(egui::Frame::none().fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                // header
                egui::TopBottomPanel::top("instinct_hdr")
                    .frame(egui::Frame::none().fill(bg_sidebar())
                        .inner_margin(Margin::symmetric(12.0, 9.0)).stroke(Stroke::new(1.0, border())))
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("λ").color(accent()).strong().size(17.0));
                            ui.add_space(2.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(i18n::tr("gr.advisor")).color(text_pri()).strong().size(14.0));
                                ui.label(RichText::new(i18n::tr("gr.inst_sub")).color(text_mut()).size(9.5));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if toolbtn(ui, "✗").clicked() { close = true; }
                                if toolbtn(ui, if edit { "◂ hints" } else { "✎ rules" }).clicked() { edit = !edit; }
                                ui.checkbox(&mut auto, RichText::new("auto").color(text_sec()).size(10.5))
                                    .on_hover_text("keep watching the graph and badge new warnings");
                            });
                        });
                    });

                if edit {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::none().inner_margin(Margin::symmetric(10.0, 8.0)))
                        .show_inside(ui, |ui| {
                            ui.label(RichText::new(i18n::tr("gr.inst_rules")).color(text_mut()).size(10.0).strong());
                            ui.horizontal(|ui| {
                                if ui.add(egui::Button::new(RichText::new("▣ Save").color(Color32::WHITE).strong().size(11.0))
                                    .fill(accent()).rounding(Rounding::same(corner()))).clicked() { save_rules = true; }
                                if toolbtn(ui, "↺ Defaults").clicked() { reset_rules = true; }
                                if toolbtn(ui, &format!("▲ {}", i18n::tr("gr.inst_export"))).clicked() { export_pack = true; }
                                if toolbtn(ui, &format!("▼ {}", i18n::tr("gr.inst_import"))).clicked() { import_pack = true; }
                            });
                            ui.add_space(4.0);
                            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                                ui.add(TextEdit::multiline(&mut self.advisor_rules)
                                    .desired_width(f32::INFINITY).desired_rows(24)
                                    .font(FontId::new(11.5, FontFamily::Monospace)));
                            });
                        });
                } else {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::none().inner_margin(Margin::symmetric(10.0, 8.0)))
                        .show_inside(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("◦ {} {}", hints.len(), i18n::tr("gr.inst_hints"))).color(text_sec()).size(11.0));
                                if warns > 0 { ui.label(RichText::new(format!("⚠ {warns}")).color(c_warn()).size(11.0)); }
                                if tips > 0 { ui.label(RichText::new(format!("✦ {tips}")).color(c_info()).size(11.0)); }
                            });
                            ui.add_space(4.0);
                            if ui.add(egui::Button::new(RichText::new(format!("⚑ {}", i18n::tr("gr.inst_triage")))
                                .color(Color32::WHITE).strong().size(11.5))
                                .fill(accent()).rounding(Rounding::same(corner()))).on_hover_text(i18n::tr("gr.inst_triage_hint")).clicked()
                            { triage = true; }
                            ui.add_space(4.0);
                            if hints.is_empty() {
                                ui.label(RichText::new(i18n::tr("gr.advisor_none")).color(text_mut()).size(12.0).italics());
                            }
                            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                                for s in &hints {
                                    let (icon, col) = match s.level {
                                        2 => ("⚠", c_warn()), 1 => ("✦", c_info()), _ => ("›", accent()),
                                    };
                                    egui::Frame::none().fill(bg_item_hov()).rounding(Rounding::same(corner()))
                                        .stroke(Stroke::new(1.0, border())).inner_margin(Margin::symmetric(11.0, 8.0))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            // clickable title → highlight its nodes (doesn't overlap the buttons)
                                            let t = ui.add(egui::Label::new(RichText::new(format!("{icon} {}", s.title)).color(col).strong().size(12.5))
                                                .sense(egui::Sense::click()));
                                            if !s.select.is_empty() {
                                                if t.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                                                if t.clicked() { do_select = Some(s.select.clone()); }
                                            }
                                            ui.label(RichText::new(&s.detail).color(text_sec()).size(11.5));
                                            if !s.explain.is_empty() {
                                                ui.label(RichText::new(format!("∵ {}", s.explain))
                                                    .color(text_mut()).size(10.0).monospace())
                                                    .on_hover_text(i18n::tr("gr.inst_why"));
                                            }
                                            if !s.select.is_empty() || !s.action.is_empty() {
                                                ui.add_space(3.0);
                                                ui.horizontal(|ui| {
                                                    if !s.select.is_empty()
                                                        && ui.add(egui::Button::new(RichText::new("◎ Show").color(accent()).size(11.0))
                                                            .fill(bg_input()).stroke(Stroke::new(1.0, border()))
                                                            .rounding(Rounding::same(corner()))).clicked()
                                                    { do_select = Some(s.select.clone()); }
                                                    if !s.action.is_empty()
                                                        && ui.add(egui::Button::new(RichText::new("▶ Run").color(Color32::WHITE).strong().size(11.0))
                                                            .fill(accent()).rounding(Rounding::same(corner()))).clicked()
                                                    { do_run = Some((s.select.clone(), s.action.clone())); }
                                                });
                                            }
                                        });
                                    ui.add_space(6.0);
                                }
                            });
                        });
                }
            });

        self.advisor_edit = edit;
        self.advisor_auto = auto;
        if close { self.show_advisor = false; }
        if triage { self.instinct_triage(); }
        if export_pack { self.export_rulepack(); }
        if import_pack { self.import_rulepack(); }
        if reset_rules { self.advisor_rules = super::lisp::rules_default(i18n::lang()).to_string(); }
        if save_rules {
            let p = advisor_rules_path();
            if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
            let _ = std::fs::write(&p, &self.advisor_rules);
            self.status = format!("✓  Instinct rules saved to {}", p.display());
        }
        if let Some(sel) = do_select { self.advisor_highlight(&sel); }
        if let Some((sel, action)) = do_run { self.advisor_act(&sel, &action); }
    }

    /// Select (highlight) the nodes a suggestion refers to.
    fn advisor_highlight(&mut self, select: &str) {
        let ids = self.advisor_targets(select);
        self.sel.clear();
        for id in &ids { self.sel.set.insert(*id); }
        self.sel.primary = ids.first().copied();
        self.needs_fit = true;
        self.status = format!("◎  {} node(s) highlighted", ids.len());
    }

    /// Run a suggestion's action — `run:<tid>` on each selected node, or
    /// `machine:<Name>` from the first matching node.
    fn advisor_act(&mut self, select: &str, action: &str) {
        let ids = self.advisor_targets(select);
        if let Some(tid) = action.strip_prefix("run:") {
            for id in &ids { self.run_transform(*id, tid); }
            self.status = format!("▶  {tid} on {} node(s)", ids.len());
        } else if let Some(mname) = action.strip_prefix("machine:") {
            let ms = machines();
            if let (Some(root), Some(idx)) = (ids.first().copied(), ms.iter().position(|m| m.name == mname)) {
                self.start_machine(&ms[idx], root);
                self.status = format!("⚙  machine '{mname}' started");
            }
        }
        self.advisor_highlight(select);
    }

    /// Instinct auto-triage: actually check services (keyless) and set flags like a
    /// real assistant — red = bad/unreachable, green = benign/online, orange = noisy.
    /// No AI: deterministic checks (GreyNoise community for IPs, HTTP for hosts).
    fn instinct_triage(&mut self) {
        let targets: Vec<(u64, Kind, String)> = self.graph.entities.values()
            .filter(|e| matches!(e.kind, Kind::Ip | Kind::Website | Kind::Domain))
            .map(|e| (e.id, e.kind, e.value.clone())).collect();
        if targets.is_empty() {
            self.status = i18n::tr("gr.inst_notargets").into();
            return;
        }
        self.log(format!("🚩  Instinct auto-triage on {} target(s)", targets.len()));
        self.status = format!("Instinct: triaging {} target(s)…", targets.len());
        for (id, kind, value) in targets {
            let tx = self.tx.clone();
            self.rt.spawn(async move {
                let cli = super::net::builder().user_agent("parasite-instinct/1.0")
                    .timeout(std::time::Duration::from_secs(15)).build().expect("client");
                let (flag, note) = match kind {
                    Kind::Ip => triage_ip(&cli, &value).await,
                    _        => triage_host(&cli, &value).await,
                };
                if flag != 0 { let _ = tx.send(Msg::Flag { id, flag, note }); }
            });
        }
    }

    /// Export the current Instinct rules as a shareable pack file.
    fn export_rulepack(&mut self) {
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let path = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir)
            .join(format!("parasite-rulepack-{ts}.lisp"));
        match std::fs::write(&path, &self.advisor_rules) {
            Ok(_)  => { self.status = format!("✓  rule-pack saved: {}", path.display());
                        self.log(format!("✓  rule-pack → {}", path.display())); }
            Err(e) => self.status = format!("✗  export failed: {e}"),
        }
    }

    /// Pick a `.lisp` rule-pack and import it (native dialog, off the UI thread).
    fn import_rulepack(&self) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if let Some(path) = native_open_file() {
                let src = std::fs::read_to_string(&path).unwrap_or_default();
                let _ = tx.send(Msg::RulesLoaded(src));
            }
        });
    }

    /// Coverage Board — for every entity, which built-in checks have been run.
    /// Click an un-run check to run it; green ✓ = already done.
    fn coverage_panel(&mut self, ctx: &egui::Context) {
        if !self.show_coverage { return; }
        let mut close = false;
        let mut run: Option<(u64, String)> = None;
        let mut bulk = false;
        let mut ents: Vec<(u64, Kind, String)> = self.graph.entities.values()
            .map(|e| (e.id, e.kind, e.value.clone())).collect();
        ents.sort_by(|a, b| a.1.label().cmp(b.1.label()));

        egui::SidePanel::right("graph_coverage")
            .resizable(true).default_width(340.0).width_range(270.0..=560.0)
            .frame(egui::Frame::none().fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                egui::TopBottomPanel::top("cov_hdr")
                    .frame(egui::Frame::none().fill(bg_sidebar())
                        .inner_margin(Margin::symmetric(12.0, 9.0)).stroke(Stroke::new(1.0, border())))
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("▦ {}", i18n::tr("gr.coverage"))).color(text_pri()).strong().size(14.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if toolbtn(ui, "✗").clicked() { close = true; }
                            });
                        });
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::none().inner_margin(Margin::symmetric(10.0, 8.0)))
                    .show_inside(ui, |ui| {
                        if ents.is_empty() {
                            ui.label(RichText::new(i18n::tr("gr.cov_empty")).color(text_mut()).size(12.0).italics());
                        }
                        ui.label(RichText::new(i18n::tr("gr.cov_hint")).color(text_mut()).size(10.0));
                        ui.add_space(4.0);
                        if !ents.is_empty()
                            && ui.add(egui::Button::new(RichText::new(format!("▶▶ {}", i18n::tr("gr.cov_runall")))
                                .color(Color32::WHITE).strong().size(11.5))
                                .fill(accent()).rounding(Rounding::same(corner())))
                                .on_hover_text(i18n::tr("gr.cov_runall_hint")).clicked()
                        { bulk = true; }
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                            for (id, kind, value) in &ents {
                                let checks: Vec<&transforms::TransformDef> =
                                    transforms::TRANSFORMS.iter().filter(|t| t.applies == *kind).collect();
                                if checks.is_empty() { continue; }
                                let done = self.ran.get(id);
                                let n_done = checks.iter().filter(|t| done.map_or(false, |s| s.contains(t.id))).count();
                                egui::Frame::none().fill(bg_item_hov()).rounding(Rounding::same(corner()))
                                    .stroke(Stroke::new(1.0, border())).inner_margin(Margin::symmetric(10.0, 8.0))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(kind.icon()).color(kind.color()).size(13.0));
                                            ui.label(RichText::new(short_val(value, 30)).color(text_pri()).strong().size(12.0));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(RichText::new(format!("{n_done}/{}", checks.len()))
                                                    .color(if n_done == checks.len() { c_ok() } else { text_mut() }).size(10.5));
                                            });
                                        });
                                        ui.horizontal_wrapped(|ui| {
                                            for t in &checks {
                                                let ran = done.map_or(false, |s| s.contains(t.id));
                                                if ran {
                                                    ui.label(RichText::new(format!("✓ {}", t.name)).color(c_ok()).size(10.5));
                                                } else if ui.add(egui::Button::new(RichText::new(format!("▷ {}", t.name)).color(text_sec()).size(10.5))
                                                    .fill(bg_input()).stroke(Stroke::new(1.0, border())).rounding(Rounding::same(corner()))).clicked()
                                                { run = Some((*id, t.id.to_string())); }
                                            }
                                        });
                                    });
                                ui.add_space(6.0);
                            }
                        });
                    });
            });
        if close { self.show_coverage = false; }
        if let Some((id, tid)) = run { self.run_transform(id, &tid); }
        if bulk {
            // run every built-in check that hasn't been run yet, across the graph
            let mut jobs: Vec<(u64, String)> = Vec::new();
            for (id, kind, _) in &ents {
                for t in transforms::TRANSFORMS.iter().filter(|t| t.applies == *kind) {
                    if !self.ran.get(id).map_or(false, |s| s.contains(t.id)) {
                        jobs.push((*id, t.id.to_string()));
                    }
                }
            }
            let n = jobs.len();
            for (id, tid) in jobs { self.run_transform(id, &tid); }
            self.status = format!("▶▶ running {n} missing check(s)…");
        }
    }

    /// Command palette (Ctrl+K) — fuzzy-run any transform or machine on the
    /// selected node, fully from the keyboard.
    fn command_palette(&mut self, ctx: &egui::Context) {
        if !self.show_palette_cmd { return; }
        let mut close = false;
        let mut do_run: Option<(u64, String)> = None;
        let mut do_machine: Option<(u64, usize)> = None;
        let prim = self.sel.primary.or_else(|| self.graph.entities.keys().next().copied());
        let kind = prim.and_then(|id| self.graph.entities.get(&id)).map(|e| e.kind);
        let q = self.cmd_query.to_lowercase();

        egui::Window::new(RichText::new(format!("⌘  {}", i18n::tr("gr.cmd"))).color(text_pri()).strong())
            .collapsible(false).resizable(false).default_width(440.0)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 90.0])
            .frame(egui::Frame::window(&ctx.style()).fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                let r = ui.add(TextEdit::singleline(&mut self.cmd_query)
                    .desired_width(f32::INFINITY).hint_text(i18n::tr("gr.cmd_ph"))
                    .font(FontId::new(14.0, FontFamily::Proportional)));
                r.request_focus();
                let Some(kind) = kind else {
                    ui.label(RichText::new(i18n::tr("gr.cmd_nonode")).color(text_mut()).size(11.5).italics());
                    return;
                };
                let id = prim.unwrap();
                ui.add_space(4.0);
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let mut first = true;
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    // machines first
                    for (mi, m) in machines().iter().enumerate() {
                        if m.root != kind { continue; }
                        if !q.is_empty() && !m.name.to_lowercase().contains(&q) { continue; }
                        let lbl = format!("⚙ {}  ·  machine", m.name);
                        if ui.add(egui::Button::new(RichText::new(lbl).color(text_pri()).size(12.5))
                            .fill(bg_item_hov()).stroke(Stroke::new(1.0, border()))
                            .rounding(Rounding::same(corner()))).clicked() || (enter && first)
                        { do_machine = Some((id, mi)); }
                        first = false;
                    }
                    // transforms (built-in + pivots)
                    let mut shown = 0;
                    for t in transforms::for_kind(kind) {
                        if !q.is_empty() && !(t.name.to_lowercase().contains(&q) || t.id.contains(&q)) { continue; }
                        if shown > 40 { break; }
                        shown += 1;
                        let lbl = format!("› {}  ·  {}", t.name, t.id);
                        if ui.add(egui::Button::new(RichText::new(lbl).color(text_sec()).size(12.0))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).clicked() || (enter && first)
                        { do_run = Some((id, t.id.to_string())); }
                        first = false;
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) { close = true; }
            });
        if let Some((id, tid)) = do_run { self.run_transform(id, &tid); self.show_palette_cmd = false; self.cmd_query.clear(); }
        if let Some((id, mi)) = do_machine { let ms = machines(); self.start_machine(&ms[mi], id);
            self.show_palette_cmd = false; self.cmd_query.clear(); }
        if close { self.show_palette_cmd = false; }
    }

    /// Keyboard-shortcut cheat sheet (toggle with `?` / F1).
    fn help_window(&mut self, ctx: &egui::Context) {
        if !self.show_help { return; }
        let mut open = true;
        egui::Window::new(RichText::new(format!("?  {}", i18n::tr("gr.help"))).color(text_pri()).strong())
            .open(&mut open).collapsible(false).resizable(false).default_width(380.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -40.0])
            .frame(egui::Frame::window(&ctx.style()).fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                for (k, d) in [
                    ("Right-click node", "transform / operation menu"),
                    ("Double-click node", "run its default transform"),
                    ("Ctrl+K", "command palette (run any transform/machine)"),
                    ("F", "fit graph to view"),
                    ("Ctrl+Z / Ctrl+Y", "undo / redo"),
                    ("Delete / Backspace", "remove selected nodes/edge"),
                    ("Shift-drag", "marquee multi-select"),
                    ("Ctrl-drag node → node", "link two nodes"),
                    ("Scroll", "zoom (drag empty space = pan)"),
                    ("? / F1", "this help"),
                ] {
                    ui.horizontal(|ui| {
                        ui.add_sized([170.0, 18.0], egui::Label::new(
                            RichText::new(k).color(accent()).size(12.0).monospace()));
                        ui.label(RichText::new(d).color(text_sec()).size(12.0));
                    });
                    ui.add_space(2.0);
                }
            });
        if !open { self.show_help = false; }
    }

    /// Quick Intel plashka — paste a phone or email; it drops the entity on the
    /// graph and runs the HLR / registration checks (NumVerify, holehe, breaches…).
    fn quick_window(&mut self, ctx: &egui::Context) {
        if !self.show_quick { return; }
        let mut open = true;
        let mut run_phone = false;
        let mut run_email = false;
        egui::Window::new(RichText::new(format!("⚡  {}", i18n::tr("gr.intel_title"))).color(text_pri()).strong())
            .open(&mut open).collapsible(false).resizable(false).default_width(380.0)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
            .frame(egui::Frame::window(&ctx.style()).fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.label(RichText::new(i18n::tr("gr.intel_hint")).color(text_sec()).size(12.0));
                ui.add_space(6.0);
                ui.add(TextEdit::singleline(&mut self.quick_input).desired_width(f32::INFINITY)
                    .font(FontId::new(13.0, FontFamily::Monospace))
                    .hint_text("+1 555 0100  or  user@example.com"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new(format!("☎ {}", i18n::tr("gr.intel_phone")))
                        .color(Color32::WHITE).strong().size(12.5)).fill(accent()).rounding(Rounding::same(corner()))).clicked()
                    { run_phone = true; }
                    if ui.add(egui::Button::new(RichText::new(format!("✉ {}", i18n::tr("gr.intel_email")))
                        .color(Color32::WHITE).strong().size(12.5)).fill(accent()).rounding(Rounding::same(corner()))).clicked()
                    { run_email = true; }
                });
                ui.add_space(4.0);
                ui.label(RichText::new(i18n::tr("gr.intel_note")).color(text_mut()).size(10.5).italics());
            });
        if run_phone { self.quick_run(true); }
        if run_email { self.quick_run(false); }
        if !open { self.show_quick = false; }
    }

    /// Drop a phone/email entity and run its intel machine.
    fn quick_run(&mut self, phone: bool) {
        let v = self.quick_input.trim().to_string();
        if v.is_empty() { self.status = "enter a phone or email".into(); return; }
        let (kind, mname) = if phone { (Kind::Phone, "Phone Profile") }
                            else { (Kind::Email, "Email Breach Sweep") };
        let id = self.add_entity(kind, v.clone());
        let ms = machines();
        if let Some(idx) = ms.iter().position(|m| m.name == mname) {
            self.start_machine(&ms[idx], id);
            self.log(format!("⚡  Quick Intel on {v} → machine '{mname}'"));
        }
        self.show_quick = false;
        self.quick_input.clear();
        self.needs_fit = true;
    }

    /// The AI graph-builder window: describe a target, the model designs a graph.
    fn ai_window(&mut self, ctx: &egui::Context) {
        if !self.show_ai { return; }
        let mut open = true;
        let mut build = false;
        let mut expand = false;
        let provider = super::ai::cfg();
        egui::Window::new(RichText::new(format!("✦  {}", i18n::tr("gr.ai_title"))).color(text_pri()).strong())
            .open(&mut open).collapsible(false).resizable(false).default_width(420.0)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
            .frame(egui::Frame::window(&ctx.style()).fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.label(RichText::new(i18n::tr("gr.ai_hint")).color(text_sec()).size(12.0));
                ui.add_space(6.0);
                ui.add(TextEdit::multiline(&mut self.ai_prompt).desired_width(f32::INFINITY)
                    .desired_rows(3).hint_text(i18n::tr("gr.ai_ph")));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let enabled = provider.is_some() && !self.ai_busy;
                    let label = if self.ai_busy { format!("◌  {}", i18n::tr("gr.ai_working")) }
                                else { format!("✦  {}", i18n::tr("gr.ai_build")) };
                    if ui.add_enabled(enabled, egui::Button::new(RichText::new(label).color(Color32::WHITE).strong().size(13.0))
                        .fill(accent()).rounding(Rounding::same(corner()))).clicked() { build = true; }
                    if ui.add_enabled(enabled && !self.graph.entities.is_empty(),
                        egui::Button::new(RichText::new(format!("⊕  {}", i18n::tr("gr.ai_expand"))).color(accent()).size(12.0))
                        .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, accent_dark())).rounding(Rounding::same(corner()))).clicked()
                    { build = true; expand = true; }
                });
                ui.add_space(6.0);
                match &provider {
                    Some(c) => ui.label(RichText::new(format!("◈ {} {}", i18n::tr("gr.ai_using"), c.label()))
                        .color(c_ok()).size(11.0)),
                    None => ui.label(RichText::new(i18n::tr("gr.ai_nokey")).color(c_warn()).size(11.0).italics()),
                };
                ui.label(RichText::new(i18n::tr("gr.ai_warn")).color(text_mut()).size(10.5).italics());
            });
        if build { self.ai_build(expand); }
        if !open { self.show_ai = false; }
        if self.ai_busy { ctx.request_repaint(); }
    }

    fn add_window(&mut self, ctx: &egui::Context) {
        if !self.show_add { return; }
        let mut open = true;
        let mut do_add = false;
        egui::Window::new(RichText::new("+  New entity").color(text_pri()).strong())
            .open(&mut open).collapsible(false).resizable(false)
            .anchor(egui::Align2::LEFT_TOP, [16.0, 56.0])
            .default_width(240.0)
            .frame(egui::Frame::window(&ctx.style()).fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                egui::ComboBox::from_id_salt("add_kind_combo")
                    .selected_text(RichText::new(format!("{} {}", self.new_kind.icon(), i18n::kind_label(self.new_kind))).color(text_pri()))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for k in Kind::ALL {
                            ui.selectable_value(&mut self.new_kind, k,
                                RichText::new(format!("{}  {}", k.icon(), i18n::kind_label(k))).color(k.color()));
                        }
                    });
                ui.add_space(5.0);
                let te = ui.add(TextEdit::singleline(&mut self.new_value)
                    .hint_text(self.new_kind.default_value()).desired_width(f32::INFINITY));
                ui.add_space(5.0);
                if ui.add_sized([ui.available_width(), 28.0], egui::Button::new(
                    RichText::new("Add to graph").color(Color32::WHITE).strong())
                    .fill(accent()).rounding(Rounding::same(5.0))).clicked()
                    || (te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    do_add = true;
                }
            });
        if do_add {
            let v = if self.new_value.trim().is_empty() { self.new_kind.default_value().to_string() }
                    else { self.new_value.trim().to_string() };
            self.add_entity(self.new_kind, v);
            self.new_value.clear();
        }
        if !open { self.show_add = false; }
    }

    /// A sortable table view of every entity (Maltego's entity list).
    fn table_window(&mut self, ctx: &egui::Context) {
        if !self.show_table { return; }
        let mut open = true;
        let mut focus: Option<u64> = None;
        egui::Window::new(RichText::new("▤  Entities").color(text_pri()).strong())
            .open(&mut open)
            .default_width(560.0)
            .default_height(420.0)
            .frame(egui::Frame::window(&ctx.style()).fill(bg_panel()).stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                let mut rows: Vec<(u64, Kind, String, String)> = self.graph.entities.values().map(|e| {
                    let props = e.props.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ");
                    (e.id, e.kind, e.value.clone(), props)
                }).collect();
                rows.sort_by(|a, b| (a.1.label(), a.2.to_lowercase()).cmp(&(b.1.label(), b.2.to_lowercase())));

                ui.label(RichText::new(format!("{} entities · {} links",
                    rows.len(), self.graph.edges.len())).color(text_mut()).size(11.0));
                ui.add_space(4.0);
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("entity_table").num_columns(3).striped(true)
                        .spacing([14.0, 4.0]).show(ui, |ui| {
                            ui.label(RichText::new("Type").color(text_mut()).size(10.5).strong());
                            ui.label(RichText::new("Value").color(text_mut()).size(10.5).strong());
                            ui.label(RichText::new("Properties").color(text_mut()).size(10.5).strong());
                            ui.end_row();
                            for (id, kind, value, props) in &rows {
                                let r = ui.add(egui::Label::new(RichText::new(format!("{} {}", kind.icon(), i18n::kind_label(*kind)))
                                    .color(kind.color()).size(11.5)).sense(egui::Sense::click()));
                                if r.clicked() { focus = Some(*id); }
                                let rv = ui.add(egui::Label::new(RichText::new(value).color(text_pri()).size(11.5))
                                    .sense(egui::Sense::click()));
                                if rv.clicked() { focus = Some(*id); }
                                ui.label(RichText::new(props).color(text_sec()).size(11.0));
                                ui.end_row();
                            }
                        });
                });
            });
        if let Some(id) = focus { self.focus(id); }
        if !open { self.show_table = false; }
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        let tb = egui::TopBottomPanel::top("graph_toolbar")
            .frame(egui::Frame::none().fill(bg_panel())
                .inner_margin(Margin::symmetric(12.0, 7.0))
                .stroke(Stroke::new(1.0, border())))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if toolbtn(ui, &format!("+ {}", i18n::tr("gr.new"))).clicked() { self.show_add = !self.show_add; }
                    if toolbtn(ui, &format!("✦ {}", i18n::tr("gr.ai"))).clicked() { self.show_ai = !self.show_ai; }
                    if toolbtn(ui, &format!("❝ {}", i18n::tr("gr.chat"))).clicked() { self.show_chat = !self.show_chat; }
                    if toolbtn(ui, &format!("⚡ {}", i18n::tr("gr.intel"))).clicked() { self.show_quick = !self.show_quick; }
                    let lam = if self.advisor_auto && self.advisor_badge > 0 {
                        format!("λ {} ⚠{}", i18n::tr("gr.advisor"), self.advisor_badge)
                    } else { format!("λ {}", i18n::tr("gr.advisor")) };
                    if toolbtn(ui, &lam).clicked() { self.show_advisor = !self.show_advisor; }
                    if toolbtn(ui, &format!("▦ {}", i18n::tr("gr.coverage"))).clicked() { self.show_coverage = !self.show_coverage; }
                    if toolbtn(ui, &format!("▤ {}", i18n::tr("gr.report"))).clicked() { self.export_report(); }
                    ui.add_space(6.0);
                    let count = self.graph.entities.len();
                    let edges = self.graph.edges.len();
                    super::logo::widget(ui, 8.0);
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{count} · {edges}"))
                        .color(text_sec()).size(12.0))
                        .on_hover_text(format!("{count} entities · {edges} links"));
                    ui.add_space(12.0);

                    let mut did_layout = false;
                    ui.menu_button(RichText::new(format!("⊹ {} ▾", i18n::tr("gr.layout"))).color(text_sec()).size(12.0), |ui| {
                        if ui.button(i18n::tr("gr.lay_force")).clicked() { self.record(); canvas::auto_layout(&mut self.graph); did_layout = true; ui.close_menu(); }
                        if ui.button(i18n::tr("gr.lay_tree")).clicked() { self.record(); canvas::tree_layout(&mut self.graph); did_layout = true; ui.close_menu(); }
                        if ui.button(i18n::tr("gr.lay_radial")).clicked() { self.record(); canvas::radial_layout(&mut self.graph); did_layout = true; ui.close_menu(); }
                        if ui.button(i18n::tr("gr.lay_circle")).clicked() { self.record(); canvas::circle_layout(&mut self.graph); did_layout = true; ui.close_menu(); }
                        if ui.button(i18n::tr("gr.lay_spiral")).clicked() { self.record(); canvas::spiral_layout(&mut self.graph); did_layout = true; ui.close_menu(); }
                        if ui.button(i18n::tr("gr.lay_grid")).clicked() { self.record(); canvas::grid_layout(&mut self.graph); did_layout = true; ui.close_menu(); }
                        if ui.button(i18n::tr("gr.lay_cols")).clicked() { self.record(); canvas::columns_layout(&mut self.graph); did_layout = true; ui.close_menu(); }
                        if ui.button(i18n::tr("gr.lay_scatter")).clicked() { self.record(); canvas::scatter_layout(&mut self.graph); did_layout = true; ui.close_menu(); }
                    });
                    if did_layout { self.needs_fit = true; self.status = "layout applied".into(); }
                    if toolbtn(ui, &format!("⊕ {}", i18n::tr("gr.fit"))).clicked() {
                        self.needs_fit = true;
                    }
                    // graph filter — dims non-matching nodes
                    ui.add(egui::TextEdit::singleline(&mut self.graph_filter)
                        .desired_width(120.0).hint_text(i18n::tr("gr.filter")));
                    if !self.graph_filter.is_empty() && toolbtn(ui, "✗").clicked() { self.graph_filter.clear(); }
                    ui.add_enabled_ui(!self.undo.is_empty(), |ui| {
                        if toolbtn(ui, "↶").on_hover_text(i18n::tr("gr.undo")).clicked() { self.undo(); }
                    });
                    ui.add_enabled_ui(!self.redo.is_empty(), |ui| {
                        if toolbtn(ui, "↷").on_hover_text(i18n::tr("gr.redo")).clicked() { self.redo(); }
                    });
                    if toolbtn(ui, &format!("▤ {}", i18n::tr("gr.table"))).clicked() {
                        self.show_table = !self.show_table;
                    }
                    if toolbtn(ui, &format!("∑ {}", i18n::tr("gr.analytics"))).clicked() {
                        self.show_analytics = !self.show_analytics;
                    }
                    if toolbtn(ui, &format!("▦ {}", i18n::tr("gr.minimap"))).clicked() {
                        self.show_minimap = !self.show_minimap;
                    }
                    if toolbtn(ui, &format!("✗ {}", i18n::tr("gr.clear"))).clicked() {
                        self.record();
                        self.graph.clear();
                        self.sel.clear();
                        self.log("⊘  graph cleared");
                    }

                    // Link / unlink the two selected nodes (alt to Ctrl-drag).
                    if self.sel.set.len() == 2 {
                        let ids: Vec<u64> = self.sel.set.iter().copied().collect();
                        let (a, b) = (ids[0], ids[1]);
                        let linked = self.graph.edges.iter().any(|e|
                            (e.from == a && e.to == b) || (e.from == b && e.to == a));
                        if !linked {
                            if toolbtn(ui, &format!("❖ {}", i18n::tr("gr.link"))).clicked() {
                                self.record();
                                self.graph.link(a, b, "linked");
                                self.log("✓  linked");
                            }
                        } else if toolbtn(ui, &format!("✂ {}", i18n::tr("gr.unlink"))).clicked() {
                            self.record();
                            self.graph.edges.retain(|e|
                                !((e.from == a && e.to == b) || (e.from == b && e.to == a)));
                            self.log("⊘  unlinked");
                        }
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    if toolbtn(ui, &format!("▼ {}", i18n::tr("gr.open"))).clicked() { self.load_graph(); }
                    if toolbtn(ui, &format!("▲ {}", i18n::tr("gr.save"))).clicked() { self.save_graph(); }
                    if toolbtn(ui, "⇩ CSV").clicked()  { self.export_csv(); }
                    if toolbtn(ui, "⊞ PNG").clicked()  { self.request_shot(ctx, ExportFmt::Png); }
                    if toolbtn(ui, "⊞ PDF").clicked()  { self.request_shot(ctx, ExportFmt::Pdf); }
                    if self.recording.is_some() {
                        ui.label(RichText::new("● REC").color(c_err()).strong().size(12.0));
                        if toolbtn(ui, "■ stop").clicked() { self.finish_video(); }
                    } else {
                        if toolbtn(ui, &format!("► {}", i18n::tr("gr.video"))).clicked() { self.start_video(ctx); }
                        ui.add(TextEdit::singleline(&mut self.video_path)
                            .desired_width(96.0).font(FontId::new(12.0, FontFamily::Monospace))
                            .text_color(text_sec()));
                    }
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
        motif_bevel(ctx, tb.response.rect, true);
    }

    fn palette(&mut self, ctx: &egui::Context) {
        if !super::theme::variant().show_palette() { return; }
        let pal = egui::SidePanel::left("graph_palette")
            .resizable(true)
            .default_width(super::theme::variant().palette_width())
            .width_range(140.0..=340.0)
            .frame(egui::Frame::none().fill(bg_sidebar()).inner_margin(Margin::same(0.0)))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .inner_margin(Margin::symmetric(12.0, 10.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(i18n::tr("gr.entities")).color(text_mut()).size(10.0).strong());
                        ui.add_space(8.0);

                        // Kind selector
                        egui::ComboBox::from_id_salt("kind_combo")
                            .selected_text(RichText::new(format!("{} {}",
                                self.new_kind.icon(), i18n::kind_label(self.new_kind))).color(text_pri()))
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for k in Kind::ALL {
                                    ui.selectable_value(&mut self.new_kind, k,
                                        RichText::new(format!("{}  {}", k.icon(), i18n::kind_label(k)))
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
                            egui::Button::new(RichText::new(format!("+  {}", i18n::tr("dos.to_graph")))
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
                        // Maltego-style entity categories (id, i18n-key, kinds)
                        let cats: [(&str, &str, &[Kind]); 6] = [
                            ("infra", "pal.infra", &[Kind::Domain, Kind::Website, Kind::Ip, Kind::Netblock,
                                Kind::Asn, Kind::Service, Kind::Port, Kind::OperatingSystem]),
                            ("personal", "pal.personal", &[Kind::Person, Kind::Email, Kind::Phone, Kind::Username,
                                Kind::Social, Kind::Organization]),
                            ("locations", "pal.locations", &[Kind::Location, Kind::Coordinate]),
                            ("malware", "pal.malware", &[Kind::Cve, Kind::Hash, Kind::File, Kind::MacAddress, Kind::Document]),
                            ("crypto", "pal.crypto", &[Kind::BtcAddress, Kind::EthAddress, Kind::Transaction]),
                            ("other", "pal.other", &[Kind::Phrase]),
                        ];
                        for (id, key, kinds) in cats {
                            egui::CollapsingHeader::new(RichText::new(i18n::tr(key)).color(text_pri()).size(11.5).strong())
                                .id_salt(("pal_cat", id))
                                .default_open(id == "infra")
                                .show(ui, |ui| {
                                    for &k in kinds {
                                        let r = egui::Frame::none().fill(bg_item_hov()).rounding(Rounding::same(4.0))
                                            .inner_margin(Margin::symmetric(10.0, 6.0))
                                            .show(ui, |ui| {
                                                ui.set_min_width(ui.available_width());
                                                ui.horizontal(|ui| {
                                                    ui.label(RichText::new(k.icon()).color(k.color()).size(13.0));
                                                    ui.add_space(4.0);
                                                    ui.label(RichText::new(i18n::kind_label(k)).color(text_sec()).size(12.0));
                                                });
                                            }).response.interact(egui::Sense::click());
                                        if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                                        if r.clicked() { to_add = Some(k); }
                                        ui.add_space(3.0);
                                    }
                                });
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
        motif_bevel(ctx, pal.response.rect, true);
    }

    fn details_panel(&mut self, ctx: &egui::Context) {
        let det = egui::SidePanel::right("graph_details")
            .resizable(true)
            .default_width(super::theme::variant().details_width())
            .width_range(200.0..=460.0)
            .frame(egui::Frame::none().fill(bg_panel()).inner_margin(Margin::same(0.0)))
            .show(ctx, |ui| {
                let Some(id) = self.sel.primary else {
                    egui::Frame::none().inner_margin(Margin::symmetric(16.0, 16.0)).show(ui, |ui| {
                        if let Some(ei) = self.sel.edge {
                            if self.graph.edges.get(ei).is_some() {
                                ui.label(RichText::new("◗ Link selected").color(text_pri()).strong().size(13.0));
                                ui.add_space(6.0);
                                ui.label(RichText::new("LABEL").color(text_mut()).size(10.0).strong());
                                ui.add_space(2.0);
                                let mut lbl = self.graph.edges[ei].label.clone();
                                if ui.add(TextEdit::singleline(&mut lbl).desired_width(f32::INFINITY)
                                    .hint_text("link label")).changed()
                                {
                                    self.graph.edges[ei].label = lbl;
                                }
                                ui.add_space(8.0);
                                if ui.add(egui::Button::new(RichText::new("⊘  Delete link").color(c_err()).size(12.0))
                                    .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, border()))
                                    .rounding(Rounding::same(4.0))).clicked()
                                {
                                    self.record();
                                    self.graph.edges.remove(ei);
                                    self.sel.edge = None;
                                    self.log("⊘  link removed");
                                }
                                ui.add_space(4.0);
                                ui.label(RichText::new("(or press Delete)").color(text_mut()).size(11.0));
                                return;
                            }
                        }
                        ui.label(RichText::new(i18n::tr("gr.no_sel")).color(text_mut()).italics().size(12.5));
                        ui.add_space(6.0);
                        ui.label(RichText::new("Click a node to inspect it & run transforms.\n\
                                                Ctrl-drag from a node to another to link them.\n\
                                                Click a link, then Delete, to remove it.")
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
                        ui.label(RichText::new(i18n::kind_label(kind)).color(text_pri()).strong().size(15.0));
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new(i18n::tr("gr.value")).color(text_mut()).size(10.0).strong());
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
                        if ui.add(egui::Button::new(RichText::new(format!("↗  {}", i18n::tr("gr.open_browser"))).color(accent()).size(12.0))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, accent_dark()))
                            .rounding(Rounding::same(5.0))).clicked()
                        {
                            let url = if lo.starts_with("http") { value.clone() } else { format!("https://{value}") };
                            open_url(&url);
                        }
                    }
                });

                // Flags + note
                egui::Frame::none().inner_margin(Margin::symmetric(16.0, 8.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(i18n::tr("gr.flag")).color(text_mut()).size(10.0).strong());
                        ui.add_space(6.0);
                        let cur = self.graph.entities.get(&id).map(|e| e.flag).unwrap_or(0);
                        for (f, label, col) in [(0u8, "none", text_sec()),
                            (1, "⚑ important", c_err()), (2, "✓ verified", c_ok()), (3, "◎ target", c_warn())] {
                            let active = cur == f;
                            if ui.add(egui::Button::new(RichText::new(label).color(if active { Color32::WHITE } else { col }).size(11.0))
                                .fill(if active { col } else { Color32::TRANSPARENT })
                                .stroke(Stroke::new(1.0, border())).rounding(Rounding::same(4.0))).clicked()
                            {
                                self.record();
                                if let Some(e) = self.graph.entities.get_mut(&id) { e.flag = f; }
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new(i18n::tr("gr.note")).color(text_mut()).size(10.0).strong());
                    ui.add_space(2.0);
                    let mut note = self.graph.entities.get(&id).map(|e| e.note.clone()).unwrap_or_default();
                    if ui.add(TextEdit::multiline(&mut note).desired_width(f32::INFINITY).desired_rows(2)
                        .hint_text("free-text note…")).changed()
                    {
                        if let Some(e) = self.graph.entities.get_mut(&id) { e.note = note; }
                    }

                    // Node-face image (screenshot, photo, logo…) — chosen via a
                    // native file picker, no typing a path.
                    ui.add_space(8.0);
                    ui.label(RichText::new(i18n::tr("gr.image")).color(text_mut()).size(10.0).strong());
                    ui.add_space(3.0);
                    let cur = self.graph.entities.get(&id).and_then(|e| e.image.clone());
                    if let Some(p) = &cur {
                        let name = std::path::Path::new(p).file_name()
                            .map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone());
                        ui.label(RichText::new(format!("▣ {name}")).color(text_sec()).size(11.5));
                        ui.add_space(2.0);
                    }
                    ui.horizontal(|ui| {
                        let label = if cur.is_some() { i18n::tr("gr.change_image") } else { i18n::tr("gr.upload_image") };
                        if ui.add(egui::Button::new(RichText::new(format!("▤  {label}")).color(accent()).size(11.5))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, accent_dark()))
                            .rounding(Rounding::same(4.0))).clicked()
                        {
                            self.pick_image_file(id);
                        }
                        if cur.is_some() && ui.add(egui::Button::new(RichText::new(i18n::tr("gr.clear_image")).color(c_err()).size(11.5))
                            .fill(Color32::TRANSPARENT).stroke(Stroke::new(1.0, border()))
                            .rounding(Rounding::same(4.0))).clicked()
                        {
                            self.record();
                            if let Some(e) = self.graph.entities.get_mut(&id) { e.image = None; }
                        }
                    });
                });

                // Properties
                if !props.is_empty() {
                    egui::Frame::none().inner_margin(Margin::symmetric(16.0, 10.0)).show(ui, |ui| {
                        ui.label(RichText::new(i18n::tr("gr.props")).color(text_mut()).size(10.0).strong());
                        ui.add_space(4.0);
                        egui::Grid::new("props_grid").num_columns(2).spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                for (k, v) in &props {
                                    ui.label(RichText::new(k).color(text_sec()).size(11.5));
                                    // wrap long values so they never widen the panel
                                    ui.add(egui::Label::new(RichText::new(v).color(text_pri()).size(11.5)
                                        .font(FontId::new(11.5, FontFamily::Monospace))).wrap());
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
                            ui.label(RichText::new(format!("⚙ {}", i18n::tr("gr.machines"))).color(accent()).size(10.0).strong());
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
        motif_bevel(ctx, det.response.rect, true);
    }

    fn log_panel(&mut self, ctx: &egui::Context) {
        let lg = egui::TopBottomPanel::bottom("graph_log")
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
        motif_bevel(ctx, lg.response.rect, true);
    }

    fn canvas_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_canvas()).inner_margin(Margin::same(0.0)))
            .show(ctx, |ui| {
                self.canvas_rect = ui.available_rect_before_wrap();
                if self.needs_fit {
                    self.view.fit(&self.graph, self.canvas_rect);
                    // extra margin while recording so no node sits at the very edge
                    // (and the watermark never covers one)
                    if self.recording.is_some() { self.view.zoom *= 0.80; }
                    self.needs_fit = false;
                }
                let gf = self.graph_filter.trim().to_lowercase();
                let action = canvas::draw(ui, &mut self.graph, &mut self.view, &mut self.sel, &self.node_tex, &self.node_risk, &gf);
                if let Some(id) = action.run_default {
                    self.run_default(id);
                }
                if let Some(ctxt) = action.context {
                    self.menu = Some(ctxt);
                }
                if let Some((a, b)) = action.new_link {
                    let before = self.graph.edges.len();
                    self.record();
                    self.graph.link(a, b, "linked");
                    if self.graph.edges.len() > before { self.log("✓  linked"); } else { self.undo.pop(); }
                }
                if self.show_minimap && self.recording.is_none() {
                    canvas::draw_minimap(ui.painter(), &self.graph, &self.view, self.canvas_rect);
                }
                // theme-adaptive video watermark (baked into the recorded frames)
                if self.recording.is_some() {
                    let r = self.canvas_rect;
                    let pp = ui.painter();
                    // subtle themed backing panel
                    let panel = egui::Rect::from_min_size(
                        r.left_bottom() + egui::Vec2::new(14.0, -68.0), egui::Vec2::new(176.0, 54.0));
                    let bp = bg_panel();
                    pp.rect_filled(panel, Rounding::same(8.0),
                        Color32::from_rgba_unmultiplied(bp.r(), bp.g(), bp.b(), 200));
                    pp.rect_stroke(panel, Rounding::same(8.0), Stroke::new(1.0, accent_dark()));
                    super::logo::paint(pp, panel.left_center() + egui::Vec2::new(24.0, 0.0), 18.0);
                    pp.text(panel.left_top() + egui::Vec2::new(48.0, 12.0), egui::Align2::LEFT_TOP,
                        "parasite", FontId::new(20.0, FontFamily::Proportional), accent());
                    pp.text(panel.left_top() + egui::Vec2::new(48.0, 34.0), egui::Align2::LEFT_TOP,
                        "OSINT graph", FontId::new(11.5, FontFamily::Proportional), text_sec());
                }
            });
        // sunken Motif workspace bevel (retro design only)
        motif_bevel(ctx, self.canvas_rect, false);
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
                    .rounding(Rounding::same(corner()))
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

        // Dismiss only when a press lands geometrically OUTSIDE the menu rect.
        // (Using `hovered()` here was the bug: it's false while the pointer is
        // over a button, so pressing Run-all/Delete closed the menu on press,
        // before the button's click could complete.)
        let menu_rect = area.response.rect;
        let pressed_outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().map_or(true, |p| !menu_rect.contains(p))
        });
        if pressed_outside { close = true; }
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

    /// Crop the framebuffer screenshot to the canvas → (rgba, w, h).
    fn crop_canvas(&self, img: &egui::ColorImage, ppp: f32) -> Option<(Vec<u8>, u32, u32)> {
        let [fw, fh] = img.size;
        let r = self.canvas_rect;
        let x0 = ((r.min.x * ppp).floor() as usize).min(fw);
        let y0 = ((r.min.y * ppp).floor() as usize).min(fh);
        let x1 = ((r.max.x * ppp).ceil() as usize).min(fw);
        let y1 = ((r.max.y * ppp).ceil() as usize).min(fh);
        let (cw, ch) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        if cw == 0 || ch == 0 { return None; }
        let mut rgba = Vec::with_capacity(cw * ch * 4);
        for y in y0..y1 {
            for x in x0..x1 {
                let px = img.pixels[y * fw + x];
                rgba.extend_from_slice(&[px.r(), px.g(), px.b(), 255]);
            }
        }
        Some((rgba, cw as u32, ch as u32))
    }

    fn save_shot(&mut self, img: std::sync::Arc<egui::ColorImage>, fmt: ExportFmt, ppp: f32) {
        let Some((rgba, cw, ch)) = self.crop_canvas(&img, ppp) else { self.log("✗  empty capture"); return };
        let res = match fmt {
            ExportFmt::Png => super::export::save_png("graph.png", &rgba, cw, ch).map(|_| "graph.png"),
            ExportFmt::Pdf => super::export::save_pdf("graph.pdf", &rgba, cw, ch).map(|_| "graph.pdf"),
        };
        match res {
            Ok(p)  => self.log(format!("✓  exported {p} ({cw}×{ch})")),
            Err(e) => self.log(format!("✗  export failed: {e}")),
        }
    }

    /// Start recording an animated video of the graph (staggered reveal + logo).
    /// Records until every node's spawn animation has finished, then 2 more
    /// seconds — so big graphs are never cut off half-way.
    fn start_video(&mut self, ctx: &egui::Context) {
        if self.recording.is_some() { return; }
        if self.graph.entities.is_empty() { self.log("✗  add some entities first"); return; }
        // fail fast if ffmpeg is missing — otherwise we'd capture frames for
        // nothing and only learn at the very end.
        if std::process::Command::new("ffmpeg").arg("-version")
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status().map(|s| !s.success()).unwrap_or(true)
        {
            self.log("✗  ffmpeg not found — install it to export MP4 (`sudo pacman -S ffmpeg`)");
            return;
        }
        let dir = std::env::temp_dir().join("parasite_frames");
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::create_dir_all(&dir).is_err() { self.log("✗  cannot create temp dir"); return; }

        // staggered reveal on a VIRTUAL timeline starting at 0. Scale the stagger
        // down for big graphs so the whole reveal always fits in ~7s — otherwise a
        // 500-node graph would record for minutes.
        let lead = 0.15;
        let mut ids: Vec<u64> = self.graph.entities.keys().copied().collect();
        ids.sort_unstable();
        let n = ids.len();
        let stagger = (6.5 / n.max(1) as f64).min(0.07);
        for (i, id) in ids.iter().enumerate() {
            if let Some(e) = self.graph.entities.get_mut(id) { e.anim_start = Some(lead + i as f64 * stagger); }
        }
        // last node finishes at: lead + (n-1)*stagger + spawn_dur(0.45); + 1.2s tail
        let end_time = lead + (n.saturating_sub(1) as f64) * stagger + 0.45 + 1.2;

        let mut out = self.video_path.trim().to_string();
        if out.is_empty() { out = "graph.mp4".into(); }
        if !out.to_lowercase().ends_with(".mp4") { out.push_str(".mp4"); }

        // background frame-writer pool: encoding + disk I/O off the UI thread
        let (tx, rx) = std::sync::mpsc::channel::<FrameJob>();
        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
        let mut workers = Vec::new();
        for _ in 0..3 {
            let rx = rx.clone();
            workers.push(std::thread::spawn(move || loop {
                let job = { let g = rx.lock().unwrap(); g.recv() };
                match job {
                    Ok(j) => { let _ = super::export::save_png_fast(
                        j.path.to_str().unwrap_or("frame.png"), &j.rgba, j.w, j.h); }
                    Err(_) => break,
                }
            }));
        }

        self.needs_fit = true;
        let wall_start = ctx.input(|i| i.time);
        self.recording = Some(RecState { dir, idx: 0, start_time: 0.0, end_time, wall_start, out,
            tx: Some(tx), workers });
        self.log(format!("►  recording {} ({:.1}s @ {:.0}fps)…",
            self.recording.as_ref().unwrap().out, end_time, REC_FPS));
    }

    fn save_frame(&mut self, img: std::sync::Arc<egui::ColorImage>, ppp: f32) {
        let Some((rgba, w, h)) = self.crop_canvas(&img, ppp) else { return };
        let (dir, idx) = match &self.recording { Some(r) => (r.dir.clone(), r.idx), None => return };
        let path = dir.join(format!("f{idx:05}.png"));
        // hand the encode+write to a worker thread; the UI thread just crops
        if let Some(r) = self.recording.as_mut() {
            if let Some(tx) = &r.tx { let _ = tx.send(FrameJob { path, rgba, w, h }); }
            r.idx += 1;
        }
    }

    fn finish_video(&mut self) {
        let Some(mut rec) = self.recording.take() else { return };
        // close the channel and wait for the writer pool to flush every frame
        rec.tx = None;
        for h in rec.workers.drain(..) { let _ = h.join(); }
        if rec.idx == 0 {
            self.log("✗  no frames captured (screenshot unsupported on this backend?)");
            let _ = std::fs::remove_dir_all(&rec.dir);
            return;
        }
        // Frames are evenly spaced in virtual time → a plain constant-framerate
        // input. Smooth by construction; no concat / timestamp guessing needed.
        let pattern = rec.dir.join("f%05d.png");
        let status = std::process::Command::new("ffmpeg")
            .args(["-y", "-framerate", &format!("{REC_FPS}"), "-i", pattern.to_str().unwrap_or(""),
                   // cap huge canvases for fast, light encodes; keep even dims
                   "-vf", "scale='min(1920,iw)':-2:flags=bicubic",
                   "-c:v", "libx264", "-preset", "veryfast", "-crf", "21",
                   "-pix_fmt", "yuv420p", "-r", &format!("{REC_FPS}"),
                   "-movflags", "+faststart", &rec.out])
            .status();
        match status {
            Ok(s) if s.success() => self.log(format!("✓  saved {} ({} frames @ {:.0}fps)", rec.out, rec.idx, REC_FPS)),
            Ok(_) => self.log("✗  ffmpeg failed to encode"),
            Err(_) => self.log("✗  ffmpeg not installed — install it (e.g. `sudo pacman -S ffmpeg`)"),
        }
        let _ = std::fs::remove_dir_all(&rec.dir);
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
    let url_re  = Regex::new(r#"https?://[^\s'\x22<>()]+"#).unwrap();
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
    // route through the built-in ParasiteGoogle browser
    super::app_open(url);
}

/// Show a native "open file" dialog and return the chosen path. Tries the common
/// Linux dialog helpers (no extra dependency); falls back to macOS/Windows.
/// Blocking — call this on a worker thread, never the UI thread.
#[cfg(target_os = "linux")]
pub(crate) fn native_open_file() -> Option<String> {
    use std::process::Command;
    // zenity / qarma / yad: path on stdout; kdialog: getopenfilename
    let attempts: [(&str, Vec<&str>); 4] = [
        ("zenity", vec!["--file-selection", "--title=Choose an image",
            "--file-filter=Images | *.png *.jpg *.jpeg *.gif *.webp *.bmp", "--file-filter=All | *"]),
        ("qarma",  vec!["--file-selection", "--title=Choose an image"]),
        ("yad",    vec!["--file-selection", "--title=Choose an image"]),
        ("kdialog", vec!["--getopenfilename", ".", "Images (*.png *.jpg *.jpeg *.gif *.webp *.bmp)"]),
    ];
    for (bin, args) in attempts {
        if let Ok(out) = Command::new(bin).args(&args).output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() { return Some(p); }
            }
            // a successful spawn with non-zero usually means "user cancelled"
            if out.status.code().is_some() { return None; }
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn native_open_file() -> Option<String> {
    use std::process::Command;
    let script = "POSIX path of (choose file with prompt \"Choose an image\")";
    let out = Command::new("osascript").args(["-e", script]).output().ok()?;
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !p.is_empty() { Some(p) } else { None }
}

#[cfg(target_os = "windows")]
pub(crate) fn native_open_file() -> Option<String> {
    use std::process::Command;
    let ps = "Add-Type -AssemblyName System.Windows.Forms; \
        $d = New-Object System.Windows.Forms.OpenFileDialog; \
        $d.Filter = 'Images|*.png;*.jpg;*.jpeg;*.gif;*.webp;*.bmp|All|*.*'; \
        if ($d.ShowDialog() -eq 'OK') { [Console]::Out.Write($d.FileName) }";
    let out = Command::new("powershell").args(["-NoProfile", "-Command", ps]).output().ok()?;
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !p.is_empty() { Some(p) } else { None }
}

/// Platforms without a native desktop file dialog (e.g. Android) — no-op.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) fn native_open_file() -> Option<String> {
    None
}

fn kind_from_name(name: &str) -> Option<Kind> {
    let n = name.to_lowercase();
    Kind::ALL.into_iter().find(|k| k.label().to_lowercase() == n
        || format!("{:?}", k).to_lowercase() == n)
}

/// Guess an entity kind from a raw value.
fn guess_kind(v: &str) -> Kind {
    let v = v.trim();
    if v.contains('@') && v.contains('.') { return Kind::Email; }
    if v.starts_with("http://") || v.starts_with("https://") { return Kind::Website; }
    if v.parse::<std::net::Ipv4Addr>().is_ok() { return Kind::Ip; }
    if v.to_uppercase().starts_with("AS") && v[2..].chars().all(|c| c.is_ascii_digit()) && v.len() > 2 { return Kind::Asn; }
    if v.starts_with('+') || (v.chars().filter(|c| c.is_ascii_digit()).count() >= 8
        && v.chars().all(|c| c.is_ascii_digit() || " +-()".contains(c))) { return Kind::Phone; }
    let hexlen = v.chars().filter(|c| c.is_ascii_hexdigit()).count();
    if hexlen == v.len() && matches!(v.len(), 32 | 40 | 64 | 128) { return Kind::Hash; }
    if v.contains('.') && !v.contains(' ') && v.split('.').count() >= 2 { return Kind::Domain; }
    Kind::Phrase
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

/// Reputation-flag an IP via GreyNoise community (keyless). Returns (flag, note).
async fn triage_ip(cli: &reqwest::Client, ip: &str) -> (u8, String) {
    let url = format!("https://api.greynoise.io/v3/community/{}", ip.trim());
    match cli.get(&url).send().await {
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(j) => {
                let class = j["classification"].as_str().unwrap_or("");
                let noise = j["noise"].as_bool().unwrap_or(false);
                let riot  = j["riot"].as_bool().unwrap_or(false);
                if class == "malicious" { (1, format!("{ip}: malicious (GreyNoise)")) }
                else if class == "benign" || riot { (2, format!("{ip}: benign (GreyNoise)")) }
                else if noise { (3, format!("{ip}: internet noise (GreyNoise)")) }
                else { (0, String::new()) }
            }
            Err(_) => (0, String::new()),
        },
        Err(_) => (0, String::new()),
    }
}

/// Liveness-flag a domain/website via a plain HTTP request. Returns (flag, note).
async fn triage_host(cli: &reqwest::Client, v: &str) -> (u8, String) {
    let url = if v.contains("://") { v.to_string() } else { format!("https://{}", v.trim()) };
    match cli.get(&url).send().await {
        Ok(r) => {
            let c = r.status().as_u16();
            if c < 400 { (2, format!("{v}: online ({c})")) } else { (1, format!("{v}: HTTP {c}")) }
        }
        Err(_) => (1, format!("{v}: unreachable")),
    }
}

/// Draw a Motif/CDE 3D bevel frame around `rect` (raised = light top-left,
/// dark bottom-right). Only used by the Retro Unix design.
fn motif_bevel(ctx: &egui::Context, rect: egui::Rect, raised: bool) {
    if design() != Design::Maltego { return; }
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground,
        egui::Id::new(("bevel", rect.min.x as i32, rect.min.y as i32))));
    let light = Color32::from_rgb(240, 240, 240);
    let dark = Color32::from_rgb(108, 108, 108);
    let (tl, br) = if raised { (light, dark) } else { (dark, light) };
    let w = 2.0;
    p.line_segment([rect.left_top(), rect.right_top()], Stroke::new(w, tl));
    p.line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(w, tl));
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(w, br));
    p.line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(w, br));
}

/// Truncate a value for compact display.
fn short_val(s: &str, n: usize) -> String {
    if s.chars().count() > n { format!("{}…", s.chars().take(n - 1).collect::<String>()) } else { s.to_string() }
}

/// Short lowercase tag for a kind, used by the Lisp advisor's `(count-kind …)`.
fn kind_tag(k: Kind) -> &'static str {
    match k {
        Kind::Domain => "domain", Kind::Website => "website", Kind::Ip => "ip",
        Kind::Email => "email", Kind::Phone => "phone", Kind::Person => "person",
        Kind::Username => "username", Kind::Social => "social", Kind::Organization => "org",
        Kind::Location => "location", Kind::Asn => "asn", Kind::Cve => "cve",
        Kind::BtcAddress => "btc", Kind::EthAddress => "eth", Kind::Transaction => "tx",
        Kind::MacAddress => "mac", Kind::Coordinate => "coord", Kind::Document => "document",
        Kind::Service => "service", Kind::OperatingSystem => "os", Kind::File => "file",
        Kind::Hash => "hash", Kind::Port => "port", Kind::Netblock => "netblock",
        Kind::Phrase => "phrase",
    }
}
