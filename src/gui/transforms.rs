//! Transforms — the heart of the tool. Each transform takes one entity and
//! produces related entities, exactly like Maltego. Everything runs in-process
//! (reqwest / scraper / regex / std::net), no external binary required.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use regex::Regex;
use url::Url;

use super::model::Kind;
use super::sherlock;

/// A transform the user can run against an entity of `applies` kind.
#[derive(Clone, Copy)]
pub struct TransformDef {
    pub id:      &'static str,
    pub name:    &'static str,
    pub applies: Kind,
    pub desc:    &'static str,
}

/// One entity produced by a transform.
pub struct NewItem {
    pub kind:  Kind,
    pub value: String,
    pub edge:  String,
    pub props: Vec<(String, String)>,
}

/// The result of running a transform.
pub struct Outcome {
    pub items:  Vec<NewItem>,
    pub props:  Vec<(String, String)>, // properties to merge onto the source entity
    pub log:    Vec<String>,
}

impl Outcome {
    fn new() -> Self { Self { items: Vec::new(), props: Vec::new(), log: Vec::new() } }
    fn item(&mut self, kind: Kind, value: impl Into<String>, edge: impl Into<String>) {
        self.items.push(NewItem { kind, value: value.into(), edge: edge.into(), props: Vec::new() });
    }
}

pub const TRANSFORMS: &[TransformDef] = &[
    // ── Domain ────────────────────────────────────────────────────────────────
    TransformDef { id: "dom_website",  name: "To Website [https]", applies: Kind::Domain,
                   desc: "build the https:// website for this domain" },
    TransformDef { id: "dom_resolve",  name: "Resolve to IP",      applies: Kind::Domain,
                   desc: "DNS A-record lookup" },
    TransformDef { id: "dom_crtsh",    name: "Subdomains (crt.sh)",applies: Kind::Domain,
                   desc: "certificate-transparency subdomain enumeration via crt.sh" },
    TransformDef { id: "dom_subs",     name: "Subdomains (brute)", applies: Kind::Domain,
                   desc: "probe a built-in list of common subdomains" },
    TransformDef { id: "dom_dns",      name: "DNS Records",        applies: Kind::Domain,
                   desc: "MX, NS and TXT records (real DNS resolver)" },
    TransformDef { id: "dom_whois",    name: "WHOIS",              applies: Kind::Domain,
                   desc: "registrar, dates & contacts via whois (port 43)" },
    TransformDef { id: "dom_wayback",  name: "Wayback URLs",       applies: Kind::Domain,
                   desc: "known URLs from the Internet Archive (CDX)" },
    TransformDef { id: "dom_dork",     name: "Google Dorks",       applies: Kind::Domain,
                   desc: "generate useful search-engine dork queries" },
    TransformDef { id: "dom_emails",   name: "Harvest Emails",     applies: Kind::Domain,
                   desc: "fetch the site and extract email addresses" },
    // ── Website ───────────────────────────────────────────────────────────────
    TransformDef { id: "web_fetch",    name: "Fetch & Fingerprint",applies: Kind::Website,
                   desc: "HTTP GET: status, server, title, tech" },
    TransformDef { id: "web_links",    name: "Extract Links",      applies: Kind::Website,
                   desc: "parse <a>/<link> and split into pages & external domains" },
    TransformDef { id: "web_emails",   name: "Extract Emails",     applies: Kind::Website,
                   desc: "regex email addresses from the page" },
    TransformDef { id: "web_phones",   name: "Extract Phones",     applies: Kind::Website,
                   desc: "regex phone numbers from the page" },
    TransformDef { id: "web_domain",   name: "To Domain",          applies: Kind::Website,
                   desc: "extract the host as a Domain entity" },
    TransformDef { id: "web_files",    name: "Find Exposed Files", applies: Kind::Website,
                   desc: "probe for .env / .git / robots / backups" },
    TransformDef { id: "web_robots",   name: "robots.txt & Sitemap",applies: Kind::Website,
                   desc: "parse robots.txt for disallowed paths & sitemaps" },
    TransformDef { id: "web_headers",  name: "Security Headers",   applies: Kind::Website,
                   desc: "grade CSP / HSTS / X-Frame-Options etc." },
    // ── Email ─────────────────────────────────────────────────────────────────
    TransformDef { id: "mail_domain",  name: "To Domain",          applies: Kind::Email,
                   desc: "the domain part of the address" },
    TransformDef { id: "mail_user",    name: "To Username",        applies: Kind::Email,
                   desc: "the local part as a username to hunt" },
    TransformDef { id: "mail_gravatar",name: "Gravatar Profile",   applies: Kind::Email,
                   desc: "check for a Gravatar tied to this email" },
    // ── Person ────────────────────────────────────────────────────────────────
    TransformDef { id: "person_user",  name: "To Username Guesses",applies: Kind::Person,
                   desc: "generate likely usernames from the name" },
    // ── Username ──────────────────────────────────────────────────────────────
    TransformDef { id: "user_hunt",    name: "Hunt Accounts",      applies: Kind::Username,
                   desc: "Sherlock-style search across 50 social networks" },
    // ── Social ────────────────────────────────────────────────────────────────
    TransformDef { id: "social_fetch", name: "Fetch & Fingerprint",applies: Kind::Social,
                   desc: "HTTP GET the profile: status, title" },
    // ── IP ────────────────────────────────────────────────────────────────────
    TransformDef { id: "ip_ports",     name: "Scan Common Ports",  applies: Kind::Ip,
                   desc: "TCP-connect scan of common ports" },
    TransformDef { id: "ip_ptr",       name: "Reverse DNS (PTR)",  applies: Kind::Ip,
                   desc: "PTR record → hostname" },
    TransformDef { id: "ip_geo",       name: "Geo / ASN (ipinfo)", applies: Kind::Ip,
                   desc: "city, country, org & ASN via ipinfo.io" },
    TransformDef { id: "ip_website",   name: "To Website",         applies: Kind::Ip,
                   desc: "build http:// website for this IP" },
    // ── Phone ─────────────────────────────────────────────────────────────────
    TransformDef { id: "phone_info",   name: "Country / Region",   applies: Kind::Phone,
                   desc: "guess country from the calling code" },
    // ── Hash ──────────────────────────────────────────────────────────────────
    TransformDef { id: "hash_id",      name: "Identify Algorithm", applies: Kind::Hash,
                   desc: "guess the hash type from length & charset" },
    TransformDef { id: "hash_lookup",  name: "Dictionary Lookup",  applies: Kind::Hash,
                   desc: "check against a built-in table of common hashes" },
];

pub fn for_kind(kind: Kind) -> Vec<&'static TransformDef> {
    TRANSFORMS.iter().filter(|t| t.applies == kind).collect()
}

const SUBDOMAINS: &[&str] = &[
    "www", "mail", "ftp", "webmail", "smtp", "pop", "ns1", "ns2", "api", "dev",
    "staging", "test", "admin", "portal", "vpn", "remote", "blog", "shop", "app",
    "cdn", "static", "img", "assets", "m", "mobile", "secure", "git", "gitlab",
    "jenkins", "jira", "docs", "support", "status", "dashboard",
];

const PROBE_PATHS: &[&str] = &[
    "/.env", "/.git/HEAD", "/.git/config", "/robots.txt", "/sitemap.xml",
    "/.htaccess", "/backup.zip", "/backup.sql", "/db.sql", "/config.json",
    "/.aws/credentials", "/wp-config.php.bak", "/server-status", "/phpinfo.php",
];

const PORTS: &[(u16, &str)] = &[
    (21, "ftp"), (22, "ssh"), (23, "telnet"), (25, "smtp"), (53, "dns"),
    (80, "http"), (110, "pop3"), (143, "imap"), (443, "https"), (445, "smb"),
    (3306, "mysql"), (3389, "rdp"), (5432, "postgres"), (6379, "redis"),
    (8080, "http-alt"), (8443, "https-alt"), (9200, "elastic"),
];

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("parasite-graph/2.0")
        .timeout(Duration::from_secs(15))
        .danger_accept_invalid_certs(true)
        .gzip(true)
        .build()
        .expect("client")
}

fn host_of(value: &str) -> Option<String> {
    if let Ok(u) = Url::parse(value) {
        u.host_str().map(|h| h.to_string())
    } else {
        Url::parse(&format!("https://{value}")).ok().and_then(|u| u.host_str().map(|s| s.to_string()))
    }
}

fn resolve(host: &str) -> Vec<String> {
    let mut out = Vec::new();
    for port in [443u16, 80] {
        if let Ok(iter) = (host, port).to_socket_addrs() {
            for a in iter { out.push(a.ip().to_string()); }
        }
        if !out.is_empty() { break; }
    }
    out.sort();
    out.dedup();
    out
}

/// Run a transform. Async because most transforms do HTTP. Blocking std::net
/// calls (DNS, TCP) are fine here: each transform runs on its own worker thread.
pub async fn run(id: &str, value: &str) -> Outcome {
    let mut o = Outcome::new();
    let v = value.trim().to_string();
    if v.is_empty() {
        o.log.push("✗  empty entity value".into());
        return o;
    }

    match id {
        "dom_website" => {
            o.item(Kind::Website, format!("https://{v}"), "website");
            o.log.push(format!("✓  https://{v}"));
        }
        "ip_website" => {
            o.item(Kind::Website, format!("http://{v}"), "website");
            o.log.push(format!("✓  http://{v}"));
        }
        "web_domain" | "mail_domain" => {
            let host = if id == "mail_domain" {
                v.split('@').nth(1).map(|s| s.to_string())
            } else {
                host_of(&v)
            };
            match host {
                Some(h) => { o.item(Kind::Domain, h.clone(), "domain"); o.log.push(format!("✓  {h}")); }
                None    => o.log.push("✗  could not parse a host".into()),
            }
        }
        "dom_resolve" => {
            let ips = resolve(&v);
            if ips.is_empty() { o.log.push(format!("✗  {v} did not resolve")); }
            for ip in ips {
                o.log.push(format!("✓  A {ip}"));
                o.item(Kind::Ip, ip, "resolves to");
            }
        }
        "dom_subs" => {
            o.log.push(format!("◦  probing {} common subdomains…", SUBDOMAINS.len()));
            for sub in SUBDOMAINS {
                let host = format!("{sub}.{v}");
                let ips = resolve(&host);
                if let Some(ip) = ips.first() {
                    o.log.push(format!("✓  {host} → {ip}"));
                    o.items.push(NewItem {
                        kind: Kind::Domain, value: host.clone(), edge: "subdomain".into(),
                        props: vec![("resolved".into(), ip.clone())],
                    });
                }
            }
            let found = o.items.len();
            o.log.push(format!("◦  {found} live subdomain(s)"));
        }
        "ip_ports" => {
            o.log.push(format!("◦  scanning {} ports on {v}…", PORTS.len()));
            for (port, name) in PORTS {
                let addr = format!("{v}:{port}");
                let open = addr.to_socket_addrs().ok()
                    .and_then(|mut it| it.next())
                    .map(|a| TcpStream::connect_timeout(&a, Duration::from_millis(900)).is_ok())
                    .unwrap_or(false);
                if open {
                    o.log.push(format!("✓  {port}/tcp open ({name})"));
                    o.items.push(NewItem {
                        kind: Kind::Port, value: format!("{port} ({name})"),
                        edge: "open port".into(), props: vec![("service".into(), name.to_string())],
                    });
                }
            }
            if o.items.is_empty() { o.log.push("◦  no common ports open".into()); }
        }
        "hash_id" => {
            let h = v.trim();
            let hex = h.chars().all(|c| c.is_ascii_hexdigit());
            let guess = match (h.len(), hex) {
                (32, true)  => "MD5 / NTLM",
                (40, true)  => "SHA-1",
                (56, true)  => "SHA-224",
                (64, true)  => "SHA-256",
                (96, true)  => "SHA-384",
                (128, true) => "SHA-512",
                _ if h.starts_with("$2a$") || h.starts_with("$2b$") => "bcrypt",
                _ if h.starts_with("$1$")  => "md5crypt",
                _ if h.starts_with("$6$")  => "sha512crypt",
                _ => "unknown",
            };
            o.log.push(format!("✓  {} chars, hex={} → {guess}", h.len(), hex));
            o.props.push(("algorithm".into(), guess.into()));
            o.item(Kind::Phrase, guess, "likely");
        }
        "web_fetch" | "social_fetch" => fetch_fingerprint(&v, &mut o).await,
        "dom_emails" => {
            let url = format!("https://{v}");
            extract_from_page(&url, "emails", &mut o).await;
        }
        "web_links"  => extract_from_page(&v, "links",  &mut o).await,
        "web_emails" => extract_from_page(&v, "emails", &mut o).await,
        "web_phones" => extract_from_page(&v, "phones", &mut o).await,
        "web_files"  => find_files(&v, &mut o).await,
        "dom_crtsh"  => crtsh(&v, &mut o).await,
        "dom_dns"    => dns_records(&v, &mut o).await,
        "ip_ptr"     => reverse_dns(&v, &mut o).await,
        "dom_whois"  => whois(&v, &mut o).await,
        "mail_user"  => {
            if let Some(u) = v.split('@').next() {
                o.item(Kind::Username, u.to_string(), "username");
                o.log.push(format!("✓  username candidate: {u}"));
            }
        }
        "mail_gravatar" => gravatar(&v, &mut o).await,
        "person_user" => {
            for u in username_guesses(&v) {
                o.log.push(format!("◦  guess: {u}"));
                o.item(Kind::Username, u, "alias");
            }
        }
        "user_hunt"  => hunt_accounts(&v, &mut o).await,
        "dom_wayback" => wayback(&v, &mut o).await,
        "dom_dork"   => dorks(&v, &mut o),
        "web_robots" => robots(&v, &mut o).await,
        "web_headers" => security_headers(&v, &mut o).await,
        "ip_geo"     => ip_geo(&v, &mut o).await,
        "phone_info" => phone_info(&v, &mut o),
        "hash_lookup" => hash_lookup(&v, &mut o),
        other => o.log.push(format!("✗  unknown transform '{other}'")),
    }

    o
}

// ── Wayback Machine known URLs ─────────────────────────────────────────────────
async fn wayback(domain: &str, o: &mut Outcome) {
    let url = format!("http://web.archive.org/cdx/search/cdx?url={domain}/*&output=json\
                       &fl=original&collapse=urlkey&limit=60");
    o.log.push("◦  querying the Internet Archive (CDX)…".into());
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let rows: Vec<Vec<String>> = serde_json::from_str(&resp.text().await.unwrap_or_default())
        .unwrap_or_default();
    let mut n = 0;
    for row in rows.into_iter().skip(1) {
        if let Some(u) = row.into_iter().next() {
            n += 1;
            o.item(Kind::Website, u, "archived");
        }
    }
    o.log.push(format!("✓  {n} archived URL(s)"));
}

// ── Search-engine dork generator ───────────────────────────────────────────────
fn dorks(domain: &str, o: &mut Outcome) {
    let templates = [
        "site:{d} ext:sql | ext:env | ext:log | ext:bak",
        "site:{d} intitle:\"index of\"",
        "site:{d} inurl:admin | inurl:login | inurl:dashboard",
        "site:{d} ext:pdf | ext:doc | ext:xls confidential",
        "site:pastebin.com {d}",
        "site:github.com {d} password",
        "site:{d} inurl:wp-content | inurl:wp-admin",
        "\"@{d}\" -site:{d}",
    ];
    for t in templates {
        let q = t.replace("{d}", domain);
        o.log.push(format!("◦  {q}"));
        o.item(Kind::Phrase, q, "dork");
    }
}

// ── robots.txt & sitemaps ──────────────────────────────────────────────────────
async fn robots(url: &str, o: &mut Outcome) {
    let url = ensure_scheme(url);
    let base = match Url::parse(&url) { Ok(u) => u, Err(e) => { o.log.push(format!("✗  {e}")); return; } };
    let robots_url = match base.join("/robots.txt") { Ok(u) => u, Err(_) => return };
    let body = match client().get(robots_url.clone()).send().await {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let mut paths = 0; let mut maps = 0;
    for line in body.lines() {
        let l = line.trim();
        if let Some(p) = l.strip_prefix("Disallow:").or_else(|| l.strip_prefix("Allow:")) {
            let p = p.trim();
            if p.is_empty() || p == "/" { continue; }
            if let Ok(full) = base.join(p) {
                if paths < 60 { o.item(Kind::Website, full.to_string(), "robots path"); paths += 1; }
            }
        } else if let Some(sm) = l.strip_prefix("Sitemap:") {
            o.item(Kind::Website, sm.trim().to_string(), "sitemap");
            maps += 1;
        }
    }
    o.log.push(format!("✓  {paths} disallowed path(s), {maps} sitemap(s)"));
    if paths == 0 && maps == 0 { o.log.push("◦  robots.txt empty or missing".into()); }
}

// ── Security headers grade ─────────────────────────────────────────────────────
async fn security_headers(url: &str, o: &mut Outcome) {
    let url = ensure_scheme(url);
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let h = resp.headers().clone();
    let checks = [
        ("Content-Security-Policy", "content-security-policy"),
        ("Strict-Transport-Security", "strict-transport-security"),
        ("X-Frame-Options", "x-frame-options"),
        ("X-Content-Type-Options", "x-content-type-options"),
        ("Referrer-Policy", "referrer-policy"),
        ("Permissions-Policy", "permissions-policy"),
    ];
    let mut present = 0;
    for (label, key) in checks {
        if h.contains_key(key) {
            present += 1;
            o.log.push(format!("✓  {label}"));
            o.props.push((label.into(), "present".into()));
        } else {
            o.log.push(format!("⚠  missing {label}"));
            o.props.push((label.into(), "MISSING".into()));
        }
    }
    let grade = match present {
        6 => "A", 5 => "B", 4 => "C", 3 => "D", 2 => "E", _ => "F",
    };
    o.log.push(format!("◦  grade {grade} ({present}/6)"));
    o.props.push(("Security Grade".into(), format!("{grade} ({present}/6)")));
    o.item(Kind::Phrase, format!("Security headers: {grade}"), "grade");
}

// ── IP geolocation / ASN via ipinfo.io ─────────────────────────────────────────
async fn ip_geo(ip: &str, o: &mut Outcome) {
    let url = format!("https://ipinfo.io/{ip}/json");
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default())
        .unwrap_or_default();
    let get = |k: &str| j.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    for (label, key) in [("city","city"),("region","region"),("country","country"),
                         ("org","org"),("hostname","hostname"),("loc","loc")] {
        let val = get(key);
        if !val.is_empty() {
            o.log.push(format!("✓  {label}: {val}"));
            o.props.push((label.into(), val));
        }
    }
    let org = get("org");
    if !org.is_empty() { o.item(Kind::Phrase, org, "ASN / org"); }
    let host = get("hostname");
    if !host.is_empty() { o.item(Kind::Domain, host, "hostname"); }
    if j.get("city").is_none() { o.log.push("◦  ipinfo returned no data (rate-limited?)".into()); }
}

// ── Phone country guess ────────────────────────────────────────────────────────
fn phone_info(phone: &str, o: &mut Outcome) {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
    let cc: &[(&str, &str)] = &[
        ("+1","US/Canada"),("+44","United Kingdom"),("+49","Germany"),("+33","France"),
        ("+7","Russia/Kazakhstan"),("+380","Ukraine"),("+86","China"),("+81","Japan"),
        ("+91","India"),("+39","Italy"),("+34","Spain"),("+55","Brazil"),("+61","Australia"),
        ("+48","Poland"),("+31","Netherlands"),("+90","Turkey"),("+82","South Korea"),
        ("+971","UAE"),("+972","Israel"),("+46","Sweden"),
    ];
    let mut found = false;
    for (code, country) in cc {
        if digits.starts_with(code) {
            o.log.push(format!("✓  {code} → {country}"));
            o.props.push(("country".into(), (*country).into()));
            o.item(Kind::Phrase, *country, "country");
            found = true;
            break;
        }
    }
    if !found { o.log.push("◦  unknown calling code".into()); }
}

// ── Tiny offline hash dictionary ───────────────────────────────────────────────
fn hash_lookup(hash: &str, o: &mut Outcome) {
    let h = hash.trim().to_lowercase();
    let table: &[(&str, &str)] = &[
        // MD5
        ("5f4dcc3b5aa765d61d8327deb882cf99", "password"),
        ("e10adc3949ba59abbe56e057f20f883e", "123456"),
        ("25d55ad283aa400af464c76d713c07ad", "12345678"),
        ("827ccb0eea8a706c4c34a16891f84e7b", "12345"),
        ("d8578edf8458ce06fbc5bb76a58c5ca4", "qwerty"),
        ("21232f297a57a5a743894a0e4a801fc3", "admin"),
        ("0d107d09f5bbe40cade3de5c71e9e9b7", "letmein"),
        // SHA-1
        ("5baa61e4c9b93f3f0682250b6cf8331b7ee68fd8", "password"),
        ("7c4a8d09ca3762af61e59520943dc26494f8941b", "123456"),
        // SHA-256
        ("5e884898da28047151d0e56f8dc6292773603d0d6aabbdd62a11ef721d1542d8", "password"),
    ];
    for (hh, plain) in table {
        if *hh == h {
            o.log.push(format!("✓  cracked → {plain}"));
            o.props.push(("plaintext".into(), (*plain).into()));
            o.item(Kind::Phrase, *plain, "plaintext");
            return;
        }
    }
    o.log.push("◦  not in the built-in dictionary".into());
}

// ── crt.sh certificate-transparency subdomain enumeration ──────────────────────
async fn crtsh(domain: &str, o: &mut Outcome) {
    let url = format!("https://crt.sh/?q=%25.{domain}&output=json");
    o.log.push("◦  querying crt.sh certificate transparency logs…".into());
    let resp = match client().get(&url).send().await {
        Ok(r) => r,
        Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let text = resp.text().await.unwrap_or_default();
    let entries: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
    let mut seen: Vec<String> = Vec::new();
    for e in entries {
        if let Some(name) = e.get("name_value").and_then(|n| n.as_str()) {
            for line in name.split('\n') {
                let h = line.trim().trim_start_matches("*.").to_lowercase();
                if h.ends_with(domain) && h != domain && !seen.contains(&h) {
                    seen.push(h.clone());
                    if seen.len() <= 200 {
                        o.item(Kind::Domain, h, "subdomain");
                    }
                }
            }
        }
    }
    o.log.push(format!("✓  {} unique subdomain(s) from crt.sh", seen.len()));
}

// ── DNS records via a real resolver ────────────────────────────────────────────
async fn dns_records(domain: &str, o: &mut Outcome) {
    use hickory_resolver::TokioAsyncResolver;
    use hickory_resolver::config::{ResolverConfig, ResolverOpts};
    let resolver = TokioAsyncResolver::tokio(ResolverConfig::cloudflare(), ResolverOpts::default());

    if let Ok(mx) = resolver.mx_lookup(domain).await {
        for r in mx.iter() {
            let host = r.exchange().to_utf8();
            let host = host.trim_end_matches('.').to_string();
            o.log.push(format!("✓  MX {} (pref {})", host, r.preference()));
            o.item(Kind::Domain, host, "mail server");
        }
    }
    if let Ok(ns) = resolver.ns_lookup(domain).await {
        for r in ns.iter() {
            let host = r.0.to_utf8();
            let host = host.trim_end_matches('.').to_string();
            o.log.push(format!("✓  NS {host}"));
            o.item(Kind::Domain, host, "nameserver");
        }
    }
    if let Ok(txt) = resolver.txt_lookup(domain).await {
        for r in txt.iter() {
            let s = r.to_string();
            o.log.push(format!("✓  TXT {s}"));
            o.items.push(NewItem { kind: Kind::Phrase, value: s, edge: "TXT".into(), props: vec![] });
        }
    }
    if o.items.is_empty() { o.log.push("◦  no MX/NS/TXT records found".into()); }
}

async fn reverse_dns(ip: &str, o: &mut Outcome) {
    use hickory_resolver::TokioAsyncResolver;
    use hickory_resolver::config::{ResolverConfig, ResolverOpts};
    let addr: std::net::IpAddr = match ip.parse() {
        Ok(a) => a,
        Err(_) => { o.log.push("✗  not a valid IP".into()); return; }
    };
    let resolver = TokioAsyncResolver::tokio(ResolverConfig::cloudflare(), ResolverOpts::default());
    match resolver.reverse_lookup(addr).await {
        Ok(names) => {
            for n in names.iter() {
                let h = n.to_utf8();
                let h = h.trim_end_matches('.').to_string();
                o.log.push(format!("✓  PTR {h}"));
                o.item(Kind::Domain, h, "PTR");
            }
        }
        Err(_) => o.log.push("◦  no PTR record".into()),
    }
}

// ── WHOIS over port 43 (IANA referral chain) ───────────────────────────────────
async fn whois(domain: &str, o: &mut Outcome) {
    let domain = domain.trim().to_lowercase();
    let dom = domain.clone();
    o.log.push("◦  querying WHOIS (port 43)…".into());
    let res = tokio::task::spawn_blocking(move || whois_blocking(&dom)).await;
    let text = match res {
        Ok(Ok(t)) => t,
        Ok(Err(e)) => { o.log.push(format!("✗  whois failed: {e}")); return; }
        Err(e) => { o.log.push(format!("✗  whois task failed: {e}")); return; }
    };

    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('%') || l.starts_with('#') { continue; }
        let lo = l.to_lowercase();
        if lo.starts_with("registrar:") || lo.starts_with("creation date:")
            || lo.starts_with("registry expiry date:") || lo.starts_with("updated date:")
            || lo.starts_with("registrant organization:") || lo.starts_with("name server:")
        {
            o.log.push(format!("✓  {l}"));
            if let Some((k, val)) = l.split_once(':') {
                o.props.push((k.trim().to_string(), val.trim().to_string()));
            }
        }
        if lo.starts_with("name server:") {
            if let Some((_, ns)) = l.split_once(':') {
                let ns = ns.trim().to_lowercase();
                if !ns.is_empty() { o.item(Kind::Domain, ns, "nameserver"); }
            }
        }
    }
    // emails inside the whois record
    let re = Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap();
    let mut seen: Vec<String> = Vec::new();
    for m in re.find_iter(&text) {
        let e = m.as_str().to_lowercase();
        if !seen.contains(&e) && !e.contains("abuse") {
            seen.push(e.clone());
            o.item(Kind::Email, e, "whois contact");
        }
    }
    if o.props.is_empty() && o.items.is_empty() {
        o.log.push("◦  no structured WHOIS fields parsed".into());
    }
}

fn whois_blocking(domain: &str) -> std::io::Result<String> {
    let tld = domain.rsplit('.').next().unwrap_or("");
    // Step 1: ask IANA which whois server is authoritative for this TLD.
    let iana = whois_query("whois.iana.org", tld)?;
    let mut server = iana.lines()
        .find_map(|l| l.to_lowercase().strip_prefix("refer:").map(|s| s.trim().to_string()))
        .unwrap_or_default();
    if server.is_empty() {
        // sensible defaults for the most common TLDs
        server = match tld {
            "com" | "net" => "whois.verisign-grs.com".into(),
            "org"         => "whois.pir.org".into(),
            "io"          => "whois.nic.io".into(),
            _             => return Ok(iana),
        };
    }
    // Step 2: query the authoritative server for the full record.
    whois_query(&server, domain)
}

fn whois_query(server: &str, query: &str) -> std::io::Result<String> {
    let addr = (server, 43u16).to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no whois addr"))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(8))?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.write_all(format!("{query}\r\n").as_bytes())?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf)?;
    Ok(buf)
}

// ── Gravatar ───────────────────────────────────────────────────────────────────
async fn gravatar(email: &str, o: &mut Outcome) {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(email.trim().to_lowercase().as_bytes());
    let hash = hex(&hasher.finalize());
    let url = format!("https://www.gravatar.com/avatar/{hash}?d=404");
    match client().get(&url).send().await {
        Ok(r) if r.status().is_success() => {
            o.log.push("✓  Gravatar exists".into());
            o.item(Kind::Social, format!("https://gravatar.com/{hash}"), "gravatar");
            o.props.push(("gravatar".into(), format!("https://gravatar.com/{hash}")));
        }
        _ => o.log.push("◦  no Gravatar for this email".into()),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── Sherlock-style account hunt ────────────────────────────────────────────────
async fn hunt_accounts(username: &str, o: &mut Outcome) {
    use futures::stream::{self, StreamExt};
    let username = username.trim().to_string();
    if username.is_empty() || username.contains(char::is_whitespace) {
        o.log.push("✗  give a single-token username".into());
        return;
    }
    let sites = sherlock::sites();
    o.log.push(format!("◦  hunting '{username}' across {} sites…", sites.len()));
    let c = client();

    let tasks = sites.into_iter().map(|site| {
        let c = c.clone();
        let username = username.clone();
        async move {
            let found = sherlock::check(&c, &site, &username).await;
            (site.name.to_string(), found)
        }
    });
    let results: Vec<(String, Option<String>)> = stream::iter(tasks)
        .buffer_unordered(20)
        .collect()
        .await;

    let mut found = 0usize;
    for (name, url) in results {
        if let Some(url) = url {
            found += 1;
            o.log.push(format!("✓  {name}: {url}"));
            o.items.push(NewItem {
                kind: Kind::Social, value: url, edge: name.to_lowercase(),
                props: vec![("site".into(), name)],
            });
        }
    }
    o.log.push(format!("◦  {found} account(s) found"));
}

fn username_guesses(name: &str) -> Vec<String> {
    let parts: Vec<String> = name.split_whitespace()
        .map(|s| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect())
        .filter(|s: &String| !s.is_empty())
        .collect();
    let mut out = Vec::new();
    match parts.as_slice() {
        [single] => out.push(single.clone()),
        [first, last, ..] => {
            out.push(format!("{first}{last}"));
            out.push(format!("{first}.{last}"));
            out.push(format!("{first}_{last}"));
            out.push(format!("{}{}", first.chars().next().unwrap_or('x'), last));
            out.push(format!("{first}{}", last.chars().next().unwrap_or('x')));
        }
        _ => {}
    }
    out.dedup();
    out
}

async fn fetch_fingerprint(url: &str, o: &mut Outcome) {
    let url = ensure_scheme(url);
    let resp = match client().get(&url).send().await {
        Ok(r) => r,
        Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let status = resp.status();
    let headers = resp.headers().clone();
    let server = headers.get("server").and_then(|v| v.to_str().ok()).unwrap_or("-").to_string();
    let powered = headers.get("x-powered-by").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let ctype  = headers.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("-").to_string();
    let body = resp.text().await.unwrap_or_default();
    let title = extract_title(&body).unwrap_or_else(|| "-".into());

    o.log.push(format!("✓  HTTP {} · {} bytes", status.as_u16(), body.len()));
    o.log.push(format!("◦  server: {server}"));
    o.log.push(format!("◦  title:  {title}"));
    o.props.push(("status".into(), status.as_u16().to_string()));
    o.props.push(("server".into(), server));
    if !powered.is_empty() { o.props.push(("x-powered-by".into(), powered)); }
    o.props.push(("content-type".into(), ctype));
    o.props.push(("title".into(), title));
}

async fn extract_from_page(url: &str, what: &str, o: &mut Outcome) {
    let url = ensure_scheme(url);
    let base = match Url::parse(&url) { Ok(u) => u, Err(e) => { o.log.push(format!("✗  bad URL: {e}")); return; } };
    let body = match client().get(&url).send().await {
        Ok(r)  => r.text().await.unwrap_or_default(),
        Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    o.log.push(format!("✓  fetched {} bytes", body.len()));

    match what {
        "links" => {
            let doc = scraper::Html::parse_document(&body);
            let sel = scraper::Selector::parse("a[href], link[href]").unwrap();
            let mut pages = 0usize;
            let mut seen_pages: Vec<String> = Vec::new();
            let mut seen_domains: Vec<String> = Vec::new();
            let base_host = base.host_str().unwrap_or("").to_string();
            for el in doc.select(&sel) {
                let Some(href) = el.value().attr("href") else { continue };
                let Ok(mut u) = base.join(href) else { continue };
                u.set_fragment(None);
                if u.scheme() != "http" && u.scheme() != "https" { continue; }
                let host = u.host_str().unwrap_or("").to_string();
                if host == base_host {
                    let s = u.to_string();
                    if !seen_pages.contains(&s) && seen_pages.len() < 40 {
                        seen_pages.push(s.clone());
                        o.item(Kind::Website, s, "link");
                        pages += 1;
                    }
                } else if !host.is_empty() && !seen_domains.contains(&host) && seen_domains.len() < 20 {
                    seen_domains.push(host.clone());
                    o.item(Kind::Domain, host, "external");
                }
            }
            o.log.push(format!("◦  {pages} internal page(s), {} external domain(s)", seen_domains.len()));
        }
        "emails" => {
            let re = Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap();
            let mut seen: Vec<String> = Vec::new();
            for m in re.find_iter(&body) {
                let e = m.as_str().to_lowercase();
                if !seen.contains(&e) {
                    seen.push(e.clone());
                    o.item(Kind::Email, e, "found on");
                }
            }
            o.log.push(format!("◦  {} email(s)", seen.len()));
        }
        "phones" => {
            let re = Regex::new(r"[\+]?[0-9]{1,3}[\s\-\.]?[\(\s]?[0-9]{3}[\)\s\-\.]?[0-9]{3}[\s\-\.]?[0-9]{2,4}").unwrap();
            let mut seen: Vec<String> = Vec::new();
            for m in re.find_iter(&body) {
                let p = m.as_str().trim().to_string();
                if p.len() >= 7 && !seen.contains(&p) && seen.len() < 30 {
                    seen.push(p.clone());
                    o.item(Kind::Phone, p, "found on");
                }
            }
            o.log.push(format!("◦  {} phone(s)", seen.len()));
        }
        _ => {}
    }
}

async fn find_files(url: &str, o: &mut Outcome) {
    let url = ensure_scheme(url);
    let base = match Url::parse(&url) { Ok(u) => u, Err(e) => { o.log.push(format!("✗  bad URL: {e}")); return; } };
    let c = client();
    o.log.push(format!("◦  probing {} sensitive paths…", PROBE_PATHS.len()));
    for path in PROBE_PATHS {
        let Ok(u) = base.join(path) else { continue };
        match c.get(u.clone()).send().await {
            Ok(r) => {
                let code = r.status().as_u16();
                if code == 200 {
                    let len = r.content_length().unwrap_or(0);
                    o.log.push(format!("✓  200 {path}  ({len} bytes)"));
                    o.items.push(NewItem {
                        kind: Kind::File, value: u.to_string(), edge: "exposed".into(),
                        props: vec![("status".into(), "200".into())],
                    });
                } else if code == 403 {
                    o.log.push(format!("⚠  403 {path}  (exists, forbidden)"));
                }
            }
            Err(_) => {}
        }
    }
    if o.items.is_empty() { o.log.push("◦  nothing exposed".into()); }
}

fn ensure_scheme(v: &str) -> String {
    if v.starts_with("http://") || v.starts_with("https://") { v.to_string() }
    else { format!("https://{v}") }
}

fn extract_title(html: &str) -> Option<String> {
    let doc = scraper::Html::parse_document(html);
    let sel = scraper::Selector::parse("title").ok()?;
    let t = doc.select(&sel).next()?.text().collect::<String>().trim().to_string();
    if t.is_empty() { None } else { Some(t.chars().take(80).collect()) }
}
