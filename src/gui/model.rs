//! Graph data model — entities (nodes) and edges, à la Maltego.

use eframe::egui::{Color32, Pos2, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::theme::*;

/// The kind of an entity. Drives icon, colour and which transforms apply.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Kind {
    Domain,
    Website,
    Ip,
    Email,
    Phone,
    Person,
    Username,
    Social,
    Organization,
    Location,
    Asn,
    Cve,
    File,
    Hash,
    Port,
    Netblock,
    Phrase,
}

impl Kind {
    pub const ALL: [Kind; 17] = [
        Kind::Domain, Kind::Website, Kind::Ip, Kind::Email, Kind::Phone,
        Kind::Person, Kind::Username, Kind::Social, Kind::Organization,
        Kind::Location, Kind::Asn, Kind::Cve, Kind::File, Kind::Hash,
        Kind::Port, Kind::Netblock, Kind::Phrase,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Domain       => "Domain",
            Kind::Website      => "Website",
            Kind::Ip           => "IP Address",
            Kind::Email        => "Email",
            Kind::Phone        => "Phone",
            Kind::Person       => "Person",
            Kind::Username     => "Username",
            Kind::Social       => "Social Profile",
            Kind::Organization => "Organization",
            Kind::Location     => "Location",
            Kind::Asn          => "ASN",
            Kind::Cve          => "CVE",
            Kind::File         => "File",
            Kind::Hash         => "Hash",
            Kind::Port         => "Port",
            Kind::Netblock     => "Netblock",
            Kind::Phrase       => "Phrase",
        }
    }

    /// A single glyph drawn inside the node disc. Chosen from DejaVuSans coverage.
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Domain       => "◈",
            Kind::Website      => "⊕",
            Kind::Ip           => "▤",
            Kind::Email        => "✉",
            Kind::Phone        => "☎",
            Kind::Person       => "☻",
            Kind::Username     => "@",
            Kind::Social       => "◉",
            Kind::Organization => "⬡",
            Kind::Location     => "◎",
            Kind::Asn          => "§",
            Kind::Cve          => "‼",
            Kind::File         => "▢",
            Kind::Hash         => "#",
            Kind::Port         => "⚑",
            Kind::Netblock     => "▦",
            Kind::Phrase       => "✎",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            Kind::Domain       => accent(),
            Kind::Website      => Color32::from_rgb(120, 155, 195),
            Kind::Ip           => Color32::from_rgb(95, 155, 108),
            Kind::Email        => Color32::from_rgb(205, 152, 60),
            Kind::Phone        => Color32::from_rgb(180, 130, 200),
            Kind::Person       => Color32::from_rgb(210, 120, 140),
            Kind::Username     => Color32::from_rgb(225, 170, 90),
            Kind::Social       => Color32::from_rgb(130, 170, 220),
            Kind::Organization => Color32::from_rgb(200, 160, 120),
            Kind::Location     => Color32::from_rgb(120, 200, 160),
            Kind::Asn          => Color32::from_rgb(160, 160, 210),
            Kind::Cve          => Color32::from_rgb(220, 90, 90),
            Kind::File         => Color32::from_rgb(150, 140, 120),
            Kind::Hash         => Color32::from_rgb(120, 180, 180),
            Kind::Port         => Color32::from_rgb(170, 170, 90),
            Kind::Netblock     => Color32::from_rgb(110, 140, 90),
            Kind::Phrase       => text_sec(),
        }
    }

    pub fn default_value(self) -> &'static str {
        match self {
            Kind::Domain       => "example.com",
            Kind::Website      => "https://example.com",
            Kind::Ip           => "93.184.216.34",
            Kind::Email        => "user@example.com",
            Kind::Phone        => "+1 555 0100",
            Kind::Person       => "John Doe",
            Kind::Username     => "johndoe",
            Kind::Social       => "https://github.com/torvalds",
            Kind::Organization => "Example Inc",
            Kind::Location     => "Berlin, DE",
            Kind::Asn          => "AS15169",
            Kind::Cve          => "CVE-2021-44228",
            Kind::File         => "/path/to/file",
            Kind::Hash         => "5f4dcc3b5aa765d61d8327deb882cf99",
            Kind::Port         => "80",
            Kind::Netblock     => "93.184.216.0/24",
            Kind::Phrase       => "note",
        }
    }
}

#[derive(Clone)]
pub struct Entity {
    pub id:     u64,
    pub kind:   Kind,
    pub value:  String,
    pub props:  Vec<(String, String)>,
    pub pos:    Pos2,
    pub vel:    Vec2,
    pub pinned: bool,
}

#[derive(Clone)]
pub struct Edge {
    pub from:  u64,
    pub to:    u64,
    pub label: String,
}

#[derive(Default)]
pub struct Graph {
    pub entities: HashMap<u64, Entity>,
    pub edges:    Vec<Edge>,
    next_id:      u64,
}

impl Graph {
    pub fn new() -> Self {
        Self { entities: HashMap::new(), edges: Vec::new(), next_id: 1 }
    }

    /// Insert a new entity at `pos`. Returns its id.
    pub fn add(&mut self, kind: Kind, value: impl Into<String>, pos: Pos2) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entities.insert(id, Entity {
            id, kind, value: value.into(), props: Vec::new(),
            pos, vel: Vec2::ZERO, pinned: false,
        });
        id
    }

    /// Find an existing entity matching (kind, value), case-insensitive on value.
    pub fn find(&self, kind: Kind, value: &str) -> Option<u64> {
        let v = value.trim().to_lowercase();
        self.entities.values()
            .find(|e| e.kind == kind && e.value.trim().to_lowercase() == v)
            .map(|e| e.id)
    }

    /// Get-or-create an entity, placing new ones at `pos`. Returns (id, created).
    pub fn upsert(&mut self, kind: Kind, value: &str, pos: Pos2) -> (u64, bool) {
        if let Some(id) = self.find(kind, value) {
            (id, false)
        } else {
            (self.add(kind, value.to_string(), pos), true)
        }
    }

    pub fn link(&mut self, from: u64, to: u64, label: impl Into<String>) {
        if from == to { return; }
        if self.edges.iter().any(|e| e.from == from && e.to == to) { return; }
        self.edges.push(Edge { from, to, label: label.into() });
    }

    pub fn remove(&mut self, id: u64) {
        self.entities.remove(&id);
        self.edges.retain(|e| e.from != id && e.to != id);
    }

    pub fn clear(&mut self) {
        self.entities.clear();
        self.edges.clear();
        self.next_id = 1;
    }

    /// Count of edges touching `id` — used to fan new children out nicely.
    pub fn degree(&self, id: u64) -> usize {
        self.edges.iter().filter(|e| e.from == id || e.to == id).count()
    }

    /// Merge key/value properties onto an existing entity (de-duplicated by key).
    pub fn merge_props(&mut self, id: u64, props: &[(String, String)]) {
        if let Some(e) = self.entities.get_mut(&id) {
            for (k, v) in props {
                if let Some(slot) = e.props.iter_mut().find(|(ek, _)| ek == k) {
                    slot.1 = v.clone();
                } else {
                    e.props.push((k.clone(), v.clone()));
                }
            }
        }
    }

    // ── Serialisation ──────────────────────────────────────────────────────────

    pub fn to_data(&self) -> GraphData {
        GraphData {
            next_id: self.next_id,
            entities: self.entities.values().map(|e| EntityData {
                id: e.id, kind: e.kind, value: e.value.clone(), props: e.props.clone(),
                x: e.pos.x, y: e.pos.y, pinned: e.pinned,
            }).collect(),
            edges: self.edges.iter().map(|e| EdgeData {
                from: e.from, to: e.to, label: e.label.clone(),
            }).collect(),
        }
    }

    pub fn from_data(d: GraphData) -> Self {
        let mut entities = HashMap::new();
        for e in d.entities {
            entities.insert(e.id, Entity {
                id: e.id, kind: e.kind, value: e.value, props: e.props,
                pos: Pos2::new(e.x, e.y), vel: Vec2::ZERO, pinned: e.pinned,
            });
        }
        let edges = d.edges.into_iter()
            .map(|e| Edge { from: e.from, to: e.to, label: e.label })
            .collect();
        let next_id = d.next_id.max(entities.keys().copied().max().unwrap_or(0) + 1);
        Self { entities, edges, next_id }
    }
}

/// On-disk representation of a graph (JSON via serde).
#[derive(Serialize, Deserialize)]
pub struct GraphData {
    pub next_id:  u64,
    pub entities: Vec<EntityData>,
    pub edges:    Vec<EdgeData>,
}

#[derive(Serialize, Deserialize)]
pub struct EntityData {
    pub id:     u64,
    pub kind:   Kind,
    pub value:  String,
    pub props:  Vec<(String, String)>,
    pub x:      f32,
    pub y:      f32,
    pub pinned: bool,
}

#[derive(Serialize, Deserialize)]
pub struct EdgeData {
    pub from:  u64,
    pub to:    u64,
    pub label: String,
}
