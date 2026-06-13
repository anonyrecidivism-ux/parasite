//! Operations-as-transforms. The original 32 recon operations live in the
//! standalone `parasite` engine binary. Here we expose them as graph transforms:
//! each one runs the engine, streams its output into the log, and harvests any
//! URLs / emails / IPs it printed back onto the graph as linked entities.

use super::model::Kind;

/// One engine-backed operation, surfaced as a transform.
pub struct EngineOp {
    pub id:      u32,
    pub tid:     &'static str, // transform id, e.g. "op_6"
    pub name:    &'static str,
    pub desc:    &'static str,
    pub applies: Kind,
}

macro_rules! op {
    ($id:expr, $tid:expr, $name:expr, $kind:expr, $desc:expr) => {
        EngineOp { id: $id, tid: $tid, name: $name, applies: $kind, desc: $desc }
    };
}

pub const OPS: &[EngineOp] = &[
    // Website-targeted recon
    op!(1,  "op_1",  "Infect (full crawl)",   Kind::Website, "recursive site crawl (engine)"),
    op!(2,  "op_2",  "Feed (page data)",      Kind::Website, "absorb links, emails, phones, meta"),
    op!(3,  "op_3",  "Map Colony",            Kind::Website, "BFS site-structure map"),
    op!(4,  "op_4",  "Leech Resources",       Kind::Website, "download all page resources"),
    op!(6,  "op_6",  "Analyze Host",          Kind::Website, "biopsy: type, structure, security headers"),
    op!(7,  "op_7",  "Probe Defenses",        Kind::Website, "security headers + robots analysis"),
    op!(8,  "op_8",  "SSL Inspect",           Kind::Website, "certificate, expiry, chain"),
    op!(9,  "op_9",  "HTTP Methods",          Kind::Website, "allowed methods (GET/PUT/DELETE…)"),
    op!(10, "op_10", "Header Dump",           Kind::Website, "all response headers"),
    op!(11, "op_11", "CORS Probe",            Kind::Website, "CORS misconfiguration check"),
    op!(12, "op_12", "Shadow Crawl",          Kind::Website, "stealth crawl, fingerprint rotation"),
    op!(13, "op_13", "Backdoor Hunter",       Kind::Website, "hunt .env/.git/backups/config"),
    op!(14, "op_14", "Form Injector",         Kind::Website, "collect forms & fuzz vectors"),
    op!(16, "op_16", "Necrosis Check",        Kind::Website, "dead links ripe for hijack"),
    op!(17, "op_17", "Content Exfil",         Kind::Website, "API keys, tokens, secrets"),
    op!(20, "op_20", "Burrow (dir brute)",    Kind::Website, "directory brute-force"),
    op!(21, "op_21", "API Parasite",          Kind::Website, "build API map from JS"),
    op!(22, "op_22", "WebSocket Leech",       Kind::Website, "intercept WebSocket traffic"),
    op!(23, "op_23", "Symbiosis",             Kind::Website, "brute common API endpoints"),
    op!(24, "op_24", "Open Redirect",         Kind::Website, "test open-redirect vulns"),
    // Domain
    op!(19, "op_19", "Dormant Check",         Kind::Domain,  "host liveness check"),
    // Phrase utilities
    op!(15, "op_15", "DNA Mutation",          Kind::Phrase,  "mutate wordlist: SQL/XSS/bypass/path"),
    op!(18, "op_18", "Spawn Larvae",          Kind::Phrase,  "word variants: case/leet/affixes"),
    op!(25, "op_25", "Hash Generate",         Kind::Phrase,  "MD5/SHA1/SHA256/SHA512 of text"),
    op!(27, "op_27", "Encode",                Kind::Phrase,  "base64/hex/url/html encode"),
    op!(28, "op_28", "Decode",                Kind::Phrase,  "try all decodings"),
    op!(30, "op_30", "Score Targets",         Kind::Phrase,  "priority-score URLs"),
    // File
    op!(29, "op_29", "Checksum File",         Kind::File,    "hash a local file"),
];

pub fn for_kind(kind: Kind) -> Vec<&'static EngineOp> {
    OPS.iter().filter(|o| o.applies == kind).collect()
}

pub fn by_tid(tid: &str) -> Option<&'static EngineOp> {
    OPS.iter().find(|o| o.tid == tid)
}

/// Build the stdin the engine expects for an operation, using `value` as the
/// primary input (URL / word / path / host) and sensible defaults for the rest.
/// Mirrors the engine's per-operation prompt order.
pub fn stdin_body(op_id: u32, value: &str) -> String {
    let v = value.trim();
    match op_id {
        1  => format!("{v}\n8\n500\n4\n1000\n"),
        2 | 4 | 6 | 7 | 8 | 9 | 10 | 11 | 14 | 21 | 24 => format!("{v}\n"),
        3  => format!("{v}\n80\n3\n"),
        12 => format!("{v}\n200\n2500\n\n"),
        13 => format!("{v}\n20\n"),
        15 => format!("{v}\n\n5\n"),
        16 => format!("{v}\n15\n"),
        17 => format!("{v}\n20\n"),
        18 => format!("{v}\n\n"),
        19 => format!("{v}\n\n8\n"),
        20 => format!("{v}\n20\nall\n"),
        22 => format!("{v}\n30\n\n"),
        23 => format!("{v}\n15\n"),
        25 | 27 | 28 | 29 => format!("{v}\n"),
        30 => format!("{v}\n\n"),
        _  => format!("{v}\n"),
    }
}

/// The full stdin stream fed to `parasite --gui` (op id, body, then `0` to exit).
pub fn full_stdin(op_id: u32, value: &str) -> String {
    format!("{}\n{}\n0\n", op_id, stdin_body(op_id, value))
}

/// Locate the `parasite` engine binary next to this executable, else in target/.
pub fn find_engine() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("parasite");
            if p.exists() { return p.to_string_lossy().into_owned(); }
        }
    }
    for c in ["./target/release/parasite", "./target/debug/parasite"] {
        if std::path::Path::new(c).exists() { return c.to_string(); }
    }
    "parasite".to_string()
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut esc = false;
    for c in s.chars() {
        if esc {
            if c.is_ascii_alphabetic() { esc = false; }
        } else if c == '\x1b' {
            esc = true;
        } else {
            out.push(c);
        }
    }
    out
}

pub fn keep_line(s: &str) -> bool {
    if s.trim().is_empty() { return false; }
    if s.contains('\u{25B8}') { return false; }
    if s.contains("━━  enter") || s.contains("enter — продолжить") { return false; }
    true
}
