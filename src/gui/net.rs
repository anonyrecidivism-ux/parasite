//! Shared network policy: an optional proxy (HTTP or SOCKS5 — useful for users in
//! regions where some OSINT sites are blocked), the preferred search engine, and
//! an "block insecure HTTP" switch. All `reqwest` clients in the app build through
//! `net::builder()` so the proxy applies everywhere.

use std::sync::{OnceLock, RwLock};

#[derive(Clone, Default)]
struct NetCfg {
    proxy:      String,  // "" = direct; e.g. socks5://127.0.0.1:9050 or http://host:port
    search:     String,  // "duckduckgo" | "google"
    block_http: bool,
}

fn store() -> &'static RwLock<NetCfg> {
    static S: OnceLock<RwLock<NetCfg>> = OnceLock::new();
    S.get_or_init(|| RwLock::new(NetCfg { proxy: String::new(), search: "duckduckgo".into(), block_http: false }))
}

pub fn set(proxy: String, search: String, block_http: bool) {
    *store().write().unwrap() = NetCfg { proxy, search, block_http };
}

fn proxy() -> String { store().read().unwrap().proxy.clone() }
pub fn block_http() -> bool { store().read().unwrap().block_http }

/// A reqwest client builder pre-configured with the active proxy (if any).
pub fn builder() -> reqwest::ClientBuilder {
    let mut b = reqwest::Client::builder();
    let p = proxy();
    if !p.trim().is_empty() {
        if let Ok(px) = reqwest::Proxy::all(p.trim()) { b = b.proxy(px); }
    }
    b
}

/// Does this string already look like a URL (vs. a search query)?
pub fn looks_like_url(s: &str) -> bool {
    let s = s.trim();
    if s.contains("://") { return true; }
    if s.contains(' ') { return false; }
    // "example.com", "1.2.3.4", "localhost:8080"
    s.contains('.') || s.starts_with("localhost")
}

/// Turn a raw query into a search URL on the chosen engine.
pub fn search_url(query: &str) -> String {
    let q: String = query.trim().chars()
        .map(|c| if c == ' ' { '+' } else { c }).collect();
    match store().read().unwrap().search.as_str() {
        "google" => format!("https://www.google.com/search?q={q}"),
        _        => format!("https://duckduckgo.com/?q={q}"),
    }
}

/// Apply the navigation policy to a user-supplied string: resolve queries to a
/// search URL, and (when configured) refuse insecure HTTP. Returns the URL to
/// open, or None if it should be blocked.
pub fn resolve_nav(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() { return None; }
    let url = if looks_like_url(s) {
        if s.contains("://") { s.to_string() } else { format!("https://{s}") }
    } else {
        search_url(s)
    };
    if block_http() && url.starts_with("http://") { return None; }
    Some(url)
}
