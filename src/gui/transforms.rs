//! Transforms — the heart of the tool. Each transform takes one entity and
//! produces related entities, exactly like Maltego. Everything runs in-process
//! (reqwest / scraper / regex / std::net), no external binary required.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use regex::Regex;
use url::Url;

use super::keys;
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
    TransformDef { id: "dom_permute",  name: "Typosquat Permutations", applies: Kind::Domain,
                   desc: "generate look-alike domains (typo/TLD swaps)" },
    TransformDef { id: "dom_emails",   name: "Harvest Emails",     applies: Kind::Domain,
                   desc: "fetch the site and extract email addresses" },
    TransformDef { id: "dom_hackertarget", name: "Subdomains (HackerTarget)", applies: Kind::Domain,
                   desc: "host search → subdomains + IPs (free API)" },
    TransformDef { id: "dom_hunter",   name: "Emails (Hunter.io)", applies: Kind::Domain,
                   desc: "domain email search — needs Hunter.io key" },
    TransformDef { id: "dom_vt",       name: "VirusTotal Report",  applies: Kind::Domain,
                   desc: "reputation & detections — needs VirusTotal key" },
    TransformDef { id: "dom_certspotter", name: "Subdomains (CertSpotter)", applies: Kind::Domain,
                   desc: "certificate transparency via api.certspotter.com" },
    TransformDef { id: "dom_otx",      name: "Passive DNS (OTX)",  applies: Kind::Domain,
                   desc: "AlienVault OTX passive DNS → subdomains & IPs" },
    TransformDef { id: "dom_urlscan",  name: "urlscan.io",         applies: Kind::Domain,
                   desc: "recent urlscan.io submissions for this domain" },
    TransformDef { id: "dom_subfinder", name: "subfinder (CLI)",    applies: Kind::Domain,
                   desc: "passive subdomain enum — runs the subfinder tool" },
    TransformDef { id: "dom_waybackurls", name: "waybackurls (CLI)", applies: Kind::Domain,
                   desc: "archived URLs — runs the waybackurls tool" },
    TransformDef { id: "dom_harvester", name: "theHarvester (CLI)", applies: Kind::Domain,
                   desc: "emails & hosts — runs the theHarvester tool" },
    TransformDef { id: "dom_pivots",   name: "Search Links",       applies: Kind::Domain,
                   desc: "Shodan, Censys, urlscan, SecurityTrails… (open in browser)" },
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
    TransformDef { id: "email_hibp",   name: "Breaches (HIBP)",    applies: Kind::Email,
                   desc: "Have I Been Pwned breaches — needs HIBP key" },
    TransformDef { id: "email_holehe", name: "holehe (account check)", applies: Kind::Email,
                   desc: "where this email is registered — runs the holehe CLI tool" },
    TransformDef { id: "email_pivots", name: "Search Links",       applies: Kind::Email,
                   desc: "HIBP, EmailRep, Hunter, IntelX, Google (open in browser)" },
    // ── Person ────────────────────────────────────────────────────────────────
    TransformDef { id: "person_user",  name: "To Username Guesses",applies: Kind::Person,
                   desc: "generate likely usernames from the name" },
    TransformDef { id: "person_pivots",name: "Search Links",       applies: Kind::Person,
                   desc: "Google, LinkedIn, Twitter, Pipl (open in browser)" },
    // ── Username ──────────────────────────────────────────────────────────────
    TransformDef { id: "user_hunt",    name: "Hunt Accounts",      applies: Kind::Username,
                   desc: "Sherlock-style search across 50 social networks" },
    TransformDef { id: "user_github",  name: "GitHub Profile",     applies: Kind::Username,
                   desc: "GitHub user: name, org, location, blog (free API)" },
    TransformDef { id: "user_maigret", name: "maigret (CLI)",      applies: Kind::Username,
                   desc: "deep account search — runs the maigret tool" },
    TransformDef { id: "user_sherlock", name: "sherlock (CLI)",    applies: Kind::Username,
                   desc: "the original sherlock tool, if installed" },
    TransformDef { id: "user_pivots",  name: "Search Links",       applies: Kind::Username,
                   desc: "search engines & people-search (open in browser)" },
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
    TransformDef { id: "ip_revip",     name: "Reverse IP (HackerTarget)", applies: Kind::Ip,
                   desc: "other domains hosted on this IP (free API)" },
    TransformDef { id: "ip_shodan",    name: "Shodan Host",        applies: Kind::Ip,
                   desc: "open ports, services, hostnames — needs Shodan key" },
    TransformDef { id: "ip_vt",        name: "VirusTotal Report",  applies: Kind::Ip,
                   desc: "reputation & detections — needs VirusTotal key" },
    TransformDef { id: "ip_abuse",     name: "AbuseIPDB Check",    applies: Kind::Ip,
                   desc: "abuse score & reports — needs AbuseIPDB key" },
    TransformDef { id: "ip_otx",       name: "Threat Intel (OTX)", applies: Kind::Ip,
                   desc: "AlienVault OTX pulses, ASN & country" },
    TransformDef { id: "ip_internetdb",name: "Shodan InternetDB",  applies: Kind::Ip,
                   desc: "free: open ports, vulns (CVE), hostnames, tags" },
    TransformDef { id: "ip_ipapi",     name: "Geo + ASN (ip-api)", applies: Kind::Ip,
                   desc: "free city/country/ISP/ASN + coordinates" },
    TransformDef { id: "ip_website",   name: "To Website",         applies: Kind::Ip,
                   desc: "build http:// website for this IP" },
    // ── ASN ───────────────────────────────────────────────────────────────────
    TransformDef { id: "asn_prefixes", name: "Announced Prefixes", applies: Kind::Asn,
                   desc: "netblocks announced by this ASN (RIPEstat)" },
    // ── Organization ──────────────────────────────────────────────────────────
    TransformDef { id: "org_domain",   name: "Guess Domain",       applies: Kind::Organization,
                   desc: "build a likely primary domain from the name" },
    TransformDef { id: "org_dork",     name: "Google Dorks",       applies: Kind::Organization,
                   desc: "search-engine dorks targeting this organization" },
    TransformDef { id: "org_pivots",   name: "Search Links",       applies: Kind::Organization,
                   desc: "LinkedIn, OpenCorporates, Crunchbase, Google (open in browser)" },
    // ── Phone ─────────────────────────────────────────────────────────────────
    TransformDef { id: "phone_info",   name: "Country / Region",   applies: Kind::Phone,
                   desc: "guess country from the calling code" },
    TransformDef { id: "phone_format", name: "Normalize",          applies: Kind::Phone,
                   desc: "strip to E.164-ish digits and validate length" },
    TransformDef { id: "phone_pivots", name: "Search Links",       applies: Kind::Phone,
                   desc: "Truecaller, sync.me, WhoCalld, Google (open in browser)" },
    // ── BTC address ───────────────────────────────────────────────────────────
    TransformDef { id: "btc_info",     name: "Balance & Activity", applies: Kind::BtcAddress,
                   desc: "balance, tx count & total received (blockchain.info)" },
    TransformDef { id: "btc_pivots",   name: "Explorer Links",     applies: Kind::BtcAddress,
                   desc: "Blockchair, Blockchain.com, OXT (open in browser)" },
    // ── MAC address ───────────────────────────────────────────────────────────
    TransformDef { id: "mac_vendor",   name: "Vendor Lookup",      applies: Kind::MacAddress,
                   desc: "OUI → hardware vendor (macvendors.com)" },
    // ── Coordinate / Location ─────────────────────────────────────────────────
    TransformDef { id: "coord_geocode",name: "Reverse Geocode",    applies: Kind::Coordinate,
                   desc: "lat,lon → address (OpenStreetMap Nominatim)" },
    TransformDef { id: "coord_pivots", name: "Map Links",          applies: Kind::Coordinate,
                   desc: "OpenStreetMap, Google Maps, Bing (open in browser)" },
    TransformDef { id: "loc_geocode",  name: "Geocode",            applies: Kind::Location,
                   desc: "place name → coordinates (Nominatim)" },
    TransformDef { id: "loc_pivots",   name: "Map Links",          applies: Kind::Location,
                   desc: "maps & search for this place (open in browser)" },
    // ── Document / OS / Service / Netblock / Port / Phrase ────────────────────
    TransformDef { id: "doc_pivots",   name: "Find Online",        applies: Kind::Document,
                   desc: "Google filetype search for this document" },
    TransformDef { id: "os_pivots",    name: "Known Vulns",        applies: Kind::OperatingSystem,
                   desc: "CVE / Exploit-DB / Vulners search links" },
    TransformDef { id: "service_pivots",name: "Known Vulns",       applies: Kind::Service,
                   desc: "CVE / Exploit-DB / Shodan search links" },
    TransformDef { id: "netblock_pivots",name: "Recon Links",      applies: Kind::Netblock,
                   desc: "Shodan, Censys, bgp.he.net (open in browser)" },
    TransformDef { id: "port_pivots",  name: "Service Info",       applies: Kind::Port,
                   desc: "Shodan & SpeedGuide for this port (open in browser)" },
    TransformDef { id: "phrase_search",name: "Search Engines",     applies: Kind::Phrase,
                   desc: "Google / Bing / DuckDuckGo (open in browser)" },
    // ── File ──────────────────────────────────────────────────────────────────
    TransformDef { id: "file_hash",    name: "Compute Hashes",     applies: Kind::File,
                   desc: "MD5 / SHA1 / SHA256 of a local file" },
    // ── CVE ───────────────────────────────────────────────────────────────────
    TransformDef { id: "cve_nvd",      name: "NVD Details",        applies: Kind::Cve,
                   desc: "description, CVSS score & severity from NVD" },
    // ── Hash ──────────────────────────────────────────────────────────────────
    TransformDef { id: "hash_id",      name: "Identify Algorithm", applies: Kind::Hash,
                   desc: "guess the hash type from length & charset" },
    TransformDef { id: "hash_lookup",  name: "Dictionary Lookup",  applies: Kind::Hash,
                   desc: "check against a built-in table of common hashes" },
    TransformDef { id: "hash_circl",   name: "CIRCL hashlookup",   applies: Kind::Hash,
                   desc: "is this a known file? (hashlookup.circl.lu)" },
    TransformDef { id: "hash_vt",      name: "VirusTotal File",    applies: Kind::Hash,
                   desc: "malware report for this file hash — needs VirusTotal key" },
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
        "dom_permute" => permute(&v, &mut o),
        "asn_prefixes" => asn_prefixes(&v, &mut o).await,
        "phone_info" => phone_info(&v, &mut o),
        "phone_format" => phone_format(&v, &mut o),
        "phone_pivots" => pivots(&v, "phone", &mut o),
        "org_domain" => org_domain(&v, &mut o),
        "org_dork"   => org_dork(&v, &mut o),
        "org_pivots" => pivots(&v, "org", &mut o),
        // ── per-kind transforms ──
        "btc_info"      => btc_info(&v, &mut o).await,
        "btc_pivots"    => pivots(&v, "btc", &mut o),
        "mac_vendor"    => mac_vendor(&v, &mut o).await,
        "coord_geocode" => coord_geocode(&v, &mut o).await,
        "coord_pivots"  => pivots(&v, "coord", &mut o),
        "loc_geocode"   => loc_geocode(&v, &mut o).await,
        "loc_pivots"    => pivots(&v, "location", &mut o),
        "doc_pivots"    => pivots(&v, "document", &mut o),
        "os_pivots"     => pivots(&v, "os", &mut o),
        "service_pivots"=> pivots(&v, "service", &mut o),
        "netblock_pivots"=> pivots(&v, "netblock", &mut o),
        "port_pivots"   => pivots(&v, "port", &mut o),
        "phrase_search" => pivots(&v, "phrase", &mut o),
        "file_hash"     => file_hash(&v, &mut o),
        "hash_lookup" => hash_lookup(&v, &mut o),
        // ── awesome-osint style search/pivot links ──
        "dom_pivots"    => pivots(&v, "domain", &mut o),
        "email_pivots"  => pivots(&v, "email",  &mut o),
        "user_pivots"   => pivots(&v, "user",   &mut o),
        "person_pivots" => pivots(&v, "person", &mut o),
        // ── keyless real APIs ──
        "user_github"      => github_user(&v, &mut o).await,
        "dom_hackertarget" => hackertarget_hosts(&v, &mut o).await,
        "ip_revip"         => hackertarget_revip(&v, &mut o).await,
        // ── keyed integrations ──
        "ip_shodan"  => shodan_host(&v, &mut o).await,
        "ip_vt"      => virustotal(&v, "ip", &mut o).await,
        "dom_vt"     => virustotal(&v, "domain", &mut o).await,
        "email_hibp" => hibp(&v, &mut o).await,
        "ip_abuse"   => abuseipdb(&v, &mut o).await,
        "dom_hunter" => hunter(&v, &mut o).await,
        // ── more real APIs ──
        "dom_certspotter" => certspotter(&v, &mut o).await,
        "dom_otx"    => otx_domain(&v, &mut o).await,
        "ip_otx"     => otx_ip(&v, &mut o).await,
        "ip_internetdb" => internetdb(&v, &mut o).await,
        "ip_ipapi"   => ipapi(&v, &mut o).await,
        "dom_urlscan" => urlscan(&v, &mut o).await,
        "hash_circl" => circl_hash(&v, &mut o).await,
        "cve_nvd"    => nvd_cve(&v, &mut o).await,
        "hash_vt"    => virustotal_file(&v, &mut o).await,
        // ── external GitHub CLI tools (optional) ──
        "email_holehe"  => holehe(&v, &mut o).await,
        "user_maigret"  => maigret(&v, &mut o).await,
        "dom_subfinder" => subfinder(&v, &mut o).await,
        "dom_harvester" => the_harvester(&v, &mut o).await,
        "user_sherlock" => sherlock_cli(&v, &mut o).await,
        "dom_waybackurls" => waybackurls_cli(&v, &mut o).await,
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
    // org looks like "AS15169 Google LLC" → split into ASN + Organization
    let org = get("org");
    if !org.is_empty() {
        let (asn, name) = org.split_once(' ').unwrap_or((org.as_str(), ""));
        if asn.starts_with("AS") { o.item(Kind::Asn, asn.to_string(), "ASN"); }
        if !name.is_empty() { o.item(Kind::Organization, name.to_string(), "operated by"); }
        if !asn.starts_with("AS") { o.item(Kind::Organization, org.clone(), "operated by"); }
    }
    // city/country → Location
    let city = get("city"); let region = get("region"); let country = get("country");
    let loc_label: Vec<String> = [city, region, country].into_iter().filter(|s| !s.is_empty()).collect();
    if !loc_label.is_empty() { o.item(Kind::Location, loc_label.join(", "), "located in"); }
    let host = get("hostname");
    if !host.is_empty() { o.item(Kind::Domain, host, "hostname"); }
    if j.get("city").is_none() && j.get("org").is_none() {
        o.log.push("◦  ipinfo returned no data (rate-limited?)".into());
    }
}

// ── Typosquat / look-alike domain permutations ─────────────────────────────────
fn permute(domain: &str, o: &mut Outcome) {
    let d = domain.trim().to_lowercase();
    let (name, tld) = d.rsplit_once('.').unwrap_or((d.as_str(), "com"));
    let mut out: Vec<String> = Vec::new();

    // character omission
    for i in 0..name.len() {
        let mut s: String = name.to_string();
        if s.is_char_boundary(i) && i < s.len() { s.remove(i); out.push(format!("{s}.{tld}")); }
    }
    // adjacent transposition
    let chars: Vec<char> = name.chars().collect();
    for i in 0..chars.len().saturating_sub(1) {
        let mut c = chars.clone();
        c.swap(i, i + 1);
        out.push(format!("{}.{tld}", c.into_iter().collect::<String>()));
    }
    // common double letters
    for i in 0..chars.len() {
        let mut c = chars.clone();
        c.insert(i, chars[i]);
        out.push(format!("{}.{tld}", c.into_iter().collect::<String>()));
    }
    // TLD swaps
    for alt in ["com", "net", "org", "io", "co", "info", "app", "dev"] {
        if alt != tld { out.push(format!("{name}.{alt}")); }
    }

    out.sort(); out.dedup();
    out.retain(|s| *s != d);
    o.log.push(format!("◦  {} permutation(s) generated", out.len()));
    for s in out.into_iter().take(80) {
        o.item(Kind::Domain, s, "typosquat");
    }
}

// ── ASN announced prefixes via RIPEstat ────────────────────────────────────────
async fn asn_prefixes(asn: &str, o: &mut Outcome) {
    let num: String = asn.chars().filter(|c| c.is_ascii_digit()).collect();
    if num.is_empty() { o.log.push("✗  not a valid ASN".into()); return; }
    let url = format!("https://stat.ripe.net/data/announced-prefixes/data.json?resource=AS{num}");
    o.log.push("◦  querying RIPEstat for announced prefixes…".into());
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default())
        .unwrap_or_default();
    let mut n = 0;
    if let Some(arr) = j.pointer("/data/prefixes").and_then(|p| p.as_array()) {
        for p in arr {
            if let Some(pfx) = p.get("prefix").and_then(|v| v.as_str()) {
                n += 1;
                if n <= 120 { o.item(Kind::Netblock, pfx.to_string(), "announced"); }
            }
        }
    }
    o.log.push(format!("✓  {n} prefix(es) announced by AS{num}"));
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
    // a browser-like UA so sites don't serve bots a misleading status/page
    let c = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0 Safari/537.36")
        .timeout(Duration::from_secs(15))
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_else(|_| client());

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

// ── helpers for the integrations ───────────────────────────────────────────────

/// minimal percent-encoding for query strings
fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn key(name: &str, o: &mut Outcome) -> Option<String> {
    let k = keys::get(name);
    if k.trim().is_empty() {
        o.log.push(format!("✗  set the {name} API key in ⚙ Settings to use this"));
        None
    } else { Some(k) }
}

/// Search / pivot links — the awesome-osint approach: hand the analyst ready-made
/// queries on external services as Website entities they can open in a browser.
fn pivots(value: &str, kind: &str, o: &mut Outcome) {
    let v = value.trim();
    let e = enc(v);
    let links: &[(&str, String)] = &match kind {
        "domain" => vec![
            ("Shodan",         format!("https://www.shodan.io/search?query={e}")),
            ("Censys",         format!("https://search.censys.io/search?resource=hosts&q={e}")),
            ("urlscan.io",     format!("https://urlscan.io/domain/{e}")),
            ("SecurityTrails", format!("https://securitytrails.com/domain/{e}/dns")),
            ("VirusTotal",     format!("https://www.virustotal.com/gui/domain/{e}")),
            ("crt.sh",         format!("https://crt.sh/?q=%25.{e}")),
            ("Wayback",        format!("https://web.archive.org/web/*/{e}/*")),
        ],
        "email" => vec![
            ("HaveIBeenPwned", format!("https://haveibeenpwned.com/account/{e}")),
            ("EmailRep",       format!("https://emailrep.io/{e}")),
            ("Hunter",         format!("https://hunter.io/email-verifier/{e}")),
            ("IntelX",         format!("https://intelx.io/?s={e}")),
            ("Google",         format!("https://www.google.com/search?q=%22{e}%22")),
        ],
        "user" => vec![
            ("Google",     format!("https://www.google.com/search?q=%22{e}%22")),
            ("DuckDuckGo", format!("https://duckduckgo.com/?q=%22{e}%22")),
            ("WhatsMyName",format!("https://whatsmyname.app/")),
            ("Twitter",    format!("https://twitter.com/search?q={e}")),
            ("Reddit",     format!("https://www.reddit.com/search/?q={e}")),
            ("GitHub",     format!("https://github.com/search?q={e}&type=users")),
        ],
        "person" => vec![
            ("Google",   format!("https://www.google.com/search?q=%22{e}%22")),
            ("LinkedIn", format!("https://www.google.com/search?q=%22{e}%22+site:linkedin.com")),
            ("Twitter",  format!("https://twitter.com/search?q={e}")),
            ("Facebook", format!("https://www.facebook.com/search/people/?q={e}")),
            ("TruePeopleSearch", format!("https://www.truepeoplesearch.com/results?name={e}")),
        ],
        "org" => vec![
            ("Google",         format!("https://www.google.com/search?q=%22{e}%22")),
            ("LinkedIn",       format!("https://www.linkedin.com/search/results/companies/?keywords={e}")),
            ("OpenCorporates", format!("https://opencorporates.com/companies?q={e}")),
            ("Crunchbase",     format!("https://www.crunchbase.com/textsearch?q={e}")),
            ("Wikipedia",      format!("https://en.wikipedia.org/w/index.php?search={e}")),
        ],
        "phone" => vec![
            ("Google",      format!("https://www.google.com/search?q=%22{e}%22")),
            ("Truecaller",  format!("https://www.truecaller.com/search/global/{e}")),
            ("Sync.me",     format!("https://sync.me/search/?number={e}")),
            ("WhoCalld",    format!("https://whocalld.com/{e}")),
        ],
        "btc" => vec![
            ("Blockchair",     format!("https://blockchair.com/bitcoin/address/{e}")),
            ("Blockchain.com", format!("https://www.blockchain.com/explorer/addresses/btc/{e}")),
            ("OXT",            format!("https://oxt.me/address/{e}")),
            ("BitcoinAbuse",   format!("https://www.bitcoinabuse.com/reports/{e}")),
        ],
        "coord" => vec![
            ("OpenStreetMap", format!("https://www.openstreetmap.org/?mlat={}&mlon={}", lat(value), lon(value))),
            ("Google Maps",   format!("https://www.google.com/maps?q={e}")),
            ("Bing Maps",     format!("https://www.bing.com/maps?cp={e}")),
        ],
        "location" => vec![
            ("Google Maps", format!("https://www.google.com/maps/search/{e}")),
            ("OpenStreetMap", format!("https://www.openstreetmap.org/search?query={e}")),
            ("Wikipedia",   format!("https://en.wikipedia.org/w/index.php?search={e}")),
        ],
        "document" => vec![
            ("Google",  format!("https://www.google.com/search?q={e}")),
            ("Google (filetype)", format!("https://www.google.com/search?q=%22{e}%22+filetype:pdf")),
        ],
        "os" => vec![
            ("CVE search",   format!("https://www.cvedetails.com/version-search.php?search={e}")),
            ("Exploit-DB",   format!("https://www.exploit-db.com/search?text={e}")),
            ("Vulners",      format!("https://vulners.com/search?query={e}")),
        ],
        "service" => vec![
            ("CVE search",  format!("https://www.cvedetails.com/google-search-results.php?q={e}")),
            ("Exploit-DB",  format!("https://www.exploit-db.com/search?text={e}")),
            ("Shodan",      format!("https://www.shodan.io/search?query=product:{e}")),
        ],
        "netblock" => vec![
            ("Shodan",   format!("https://www.shodan.io/search?query=net:{e}")),
            ("Censys",   format!("https://search.censys.io/search?resource=hosts&q=ip:{e}")),
            ("bgp.he.net", format!("https://bgp.he.net/net/{e}")),
        ],
        "port" => vec![
            ("Shodan",      format!("https://www.shodan.io/search?query=port:{e}")),
            ("SpeedGuide",  format!("https://www.speedguide.net/port.php?port={e}")),
        ],
        "phrase" => vec![
            ("Google",     format!("https://www.google.com/search?q={e}")),
            ("Bing",       format!("https://www.bing.com/search?q={e}")),
            ("DuckDuckGo", format!("https://duckduckgo.com/?q={e}")),
        ],
        _ => vec![],
    };
    for (name, url) in links {
        o.log.push(format!("◦  {name}: {url}"));
        o.items.push(NewItem { kind: Kind::Website, value: url.clone(),
            edge: (*name).into(), props: vec![("service".into(), (*name).into())] });
    }
    o.log.push(format!("◦  {} search link(s) — open them from the details panel", links.len()));
}

fn org_domain(org: &str, o: &mut Outcome) {
    let slug: String = org.to_lowercase().chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .replace([' '], "");
    if slug.is_empty() { o.log.push("✗  empty organization name".into()); return; }
    for tld in ["com", "io", "net", "org", "co"] {
        let d = format!("{slug}.{tld}");
        o.log.push(format!("◦  candidate {d}"));
        o.item(Kind::Domain, d, "likely domain");
    }
}

fn org_dork(org: &str, o: &mut Outcome) {
    let templates = [
        "\"{o}\" filetype:pdf",
        "\"{o}\" (confidential OR internal OR proprietary)",
        "\"{o}\" site:linkedin.com/in",
        "\"{o}\" (email OR contact) @",
        "\"{o}\" site:github.com",
        "\"{o}\" intext:password filetype:xls",
    ];
    for t in templates {
        let q = t.replace("{o}", org);
        o.log.push(format!("◦  {q}"));
        o.item(Kind::Phrase, q, "dork");
    }
}

fn phone_format(phone: &str, o: &mut Outcome) {
    let mut digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    let plus = phone.trim_start().starts_with('+');
    if plus { digits = format!("+{digits}"); }
    let n = digits.chars().filter(|c| c.is_ascii_digit()).count();
    let valid = (8..=15).contains(&n);
    o.log.push(format!("{}  {digits}  ({n} digits)", if valid { "✓" } else { "⚠" }));
    o.props.push(("normalized".into(), digits.clone()));
    o.props.push(("valid_length".into(), valid.to_string()));
    if valid { o.item(Kind::Phrase, digits, "E.164"); }
}

fn lat(v: &str) -> String { v.split(',').next().unwrap_or("").trim().to_string() }
fn lon(v: &str) -> String { v.split(',').nth(1).unwrap_or("").trim().to_string() }

// ── BTC address (blockchain.info, free) ────────────────────────────────────────
async fn btc_info(addr: &str, o: &mut Outcome) {
    let url = format!("https://blockchain.info/rawaddr/{}?limit=0", enc(addr.trim()));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if !resp.status().is_success() { o.log.push(format!("✗  blockchain.info: HTTP {}", resp.status().as_u16())); return; }
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let sat = |k: &str| j.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) / 1e8;
    let n_tx = j.get("n_tx").and_then(|v| v.as_u64()).unwrap_or(0);
    o.log.push(format!("✓  balance {:.8} BTC · {} tx · received {:.8}", sat("final_balance"), n_tx, sat("total_received")));
    o.props.push(("balance_btc".into(), format!("{:.8}", sat("final_balance"))));
    o.props.push(("tx_count".into(), n_tx.to_string()));
    o.props.push(("total_received_btc".into(), format!("{:.8}", sat("total_received"))));
    o.item(Kind::Phrase, format!("{:.8} BTC ({n_tx} tx)", sat("final_balance")), "balance");
}

// ── MAC vendor (macvendors.com, free) ──────────────────────────────────────────
async fn mac_vendor(mac: &str, o: &mut Outcome) {
    let url = format!("https://api.macvendors.com/{}", enc(mac.trim()));
    match client().get(&url).send().await {
        Ok(r) if r.status().is_success() => {
            let v = r.text().await.unwrap_or_default();
            if v.trim().is_empty() { o.log.push("◦  vendor not found".into()); return; }
            o.log.push(format!("✓  vendor: {v}"));
            o.props.push(("vendor".into(), v.clone()));
            o.item(Kind::Organization, v, "vendor");
        }
        Ok(r) => o.log.push(format!("◦  macvendors: HTTP {} (unknown OUI?)", r.status().as_u16())),
        Err(e) => o.log.push(format!("✗  {e}")),
    }
}

// ── Geocoding via OpenStreetMap Nominatim (free) ───────────────────────────────
async fn coord_geocode(coord: &str, o: &mut Outcome) {
    let url = format!("https://nominatim.openstreetmap.org/reverse?format=json&lat={}&lon={}",
        enc(&lat(coord)), enc(&lon(coord)));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    if let Some(name) = j.get("display_name").and_then(|v| v.as_str()) {
        o.log.push(format!("✓  {name}"));
        o.props.push(("address".into(), name.into()));
        o.item(Kind::Location, name.to_string(), "located at");
    } else {
        o.log.push("◦  no address for these coordinates".into());
    }
}

async fn loc_geocode(place: &str, o: &mut Outcome) {
    let url = format!("https://nominatim.openstreetmap.org/search?format=json&limit=1&q={}", enc(place.trim()));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let arr: Vec<serde_json::Value> = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    if let Some(first) = arr.first() {
        let la = first.get("lat").and_then(|v| v.as_str()).unwrap_or("");
        let lo = first.get("lon").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(name) = first.get("display_name").and_then(|v| v.as_str()) {
            o.log.push(format!("✓  {name} → {la},{lo}"));
            o.props.push(("matched".into(), name.into()));
        }
        if !la.is_empty() { o.item(Kind::Coordinate, format!("{la}, {lo}"), "coordinates"); }
    } else {
        o.log.push("◦  place not found".into());
    }
}

// ── Local file hashing ─────────────────────────────────────────────────────────
fn file_hash(path: &str, o: &mut Outcome) {
    use sha2::{Digest as _, Sha256};
    let bytes = match std::fs::read(path.trim()) {
        Ok(b) => b, Err(e) => { o.log.push(format!("✗  cannot read file: {e}")); return; }
    };
    o.log.push(format!("◦  {} bytes", bytes.len()));
    let md5 = {
        use md5::{Digest as _, Md5};
        let mut h = Md5::new(); h.update(&bytes); hex(&h.finalize())
    };
    let sha1 = {
        use sha1::{Digest as _, Sha1};
        let mut h = Sha1::new(); h.update(&bytes); hex(&h.finalize())
    };
    let sha256 = { let mut h = Sha256::new(); h.update(&bytes); hex(&h.finalize()) };
    for (name, val) in [("md5", &md5), ("sha1", &sha1), ("sha256", &sha256)] {
        o.log.push(format!("✓  {name}: {val}"));
        o.props.push((name.into(), val.clone()));
        o.item(Kind::Hash, val.clone(), name);
    }
}

// ── GitHub user (free) ─────────────────────────────────────────────────────────
async fn github_user(user: &str, o: &mut Outcome) {
    let url = format!("https://api.github.com/users/{}", enc(user));
    let resp = match client().get(&url).header("Accept", "application/vnd.github+json").send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if !resp.status().is_success() {
        o.log.push(format!("✗  GitHub: HTTP {}", resp.status().as_u16())); return;
    }
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let s = |k: &str| j.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    o.log.push(format!("✓  GitHub user '{}'", s("login")));
    for (label, k) in [("name","name"),("bio","bio"),("company","company"),
                       ("location","location"),("blog","blog")] {
        let val = s(k);
        if !val.is_empty() { o.props.push((label.into(), val)); }
    }
    if let Some(n) = j.get("public_repos").and_then(|v| v.as_u64()) { o.props.push(("public_repos".into(), n.to_string())); }
    if let Some(n) = j.get("followers").and_then(|v| v.as_u64()) { o.props.push(("followers".into(), n.to_string())); }
    if !s("name").is_empty()     { o.item(Kind::Person, s("name"), "real name"); }
    if !s("company").is_empty()  { o.item(Kind::Organization, s("company").trim_start_matches('@').to_string(), "company"); }
    if !s("location").is_empty() { o.item(Kind::Location, s("location"), "location"); }
    if !s("blog").is_empty()     { o.item(Kind::Website, ensure_scheme(&s("blog")), "blog"); }
    if !s("email").is_empty()    { o.item(Kind::Email, s("email"), "public email"); }
    o.item(Kind::Social, format!("https://github.com/{}", s("login")), "github");
}

// ── HackerTarget (free, rate-limited) ──────────────────────────────────────────
async fn hackertarget_hosts(domain: &str, o: &mut Outcome) {
    let url = format!("https://api.hackertarget.com/hostsearch/?q={}", enc(domain));
    let body = match client().get(&url).send().await {
        Ok(r) => r.text().await.unwrap_or_default(), Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if body.contains("API count exceeded") || body.contains("error") {
        o.log.push(format!("✗  HackerTarget: {}", body.lines().next().unwrap_or(""))); return;
    }
    let mut n = 0;
    for line in body.lines() {
        if let Some((host, ip)) = line.split_once(',') {
            n += 1;
            o.item(Kind::Domain, host.trim().to_string(), "subdomain");
            if ip.trim().parse::<std::net::Ipv4Addr>().is_ok() {
                o.item(Kind::Ip, ip.trim().to_string(), "resolves to");
            }
        }
    }
    o.log.push(format!("✓  {n} host(s) from HackerTarget"));
}

async fn hackertarget_revip(ip: &str, o: &mut Outcome) {
    let url = format!("https://api.hackertarget.com/reverseiplookup/?q={}", enc(ip));
    let body = match client().get(&url).send().await {
        Ok(r) => r.text().await.unwrap_or_default(), Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if body.contains("API count exceeded") || body.contains("error") || body.contains("No DNS") {
        o.log.push(format!("◦  HackerTarget: {}", body.lines().next().unwrap_or(""))); return;
    }
    let mut n = 0;
    for line in body.lines() {
        let h = line.trim();
        if !h.is_empty() { n += 1; o.item(Kind::Domain, h.to_string(), "hosted on"); }
    }
    o.log.push(format!("✓  {n} domain(s) on {ip}"));
}

// ── Shodan (key) ───────────────────────────────────────────────────────────────
async fn shodan_host(ip: &str, o: &mut Outcome) {
    let Some(k) = key("shodan", o) else { return };
    let url = format!("https://api.shodan.io/shodan/host/{}?key={}", enc(ip), enc(&k));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if !resp.status().is_success() { o.log.push(format!("✗  Shodan: HTTP {}", resp.status().as_u16())); return; }
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    for (label, k) in [("org","org"),("isp","isp"),("os","os"),("country","country_name")] {
        if let Some(val) = j.get(k).and_then(|v| v.as_str()) {
            o.props.push((label.into(), val.into()));
        }
    }
    if let Some(org) = j.get("org").and_then(|v| v.as_str()) { o.item(Kind::Organization, org.to_string(), "org"); }
    if let Some(hs) = j.get("hostnames").and_then(|v| v.as_array()) {
        for h in hs { if let Some(s) = h.as_str() { o.item(Kind::Domain, s.to_string(), "hostname"); } }
    }
    if let Some(ports) = j.get("ports").and_then(|v| v.as_array()) {
        for p in ports { if let Some(n) = p.as_u64() { o.item(Kind::Port, n.to_string(), "open port"); } }
        o.log.push(format!("✓  Shodan: {} open port(s)", ports.len()));
    } else { o.log.push("◦  Shodan returned no ports".into()); }
}

// ── VirusTotal (key) ───────────────────────────────────────────────────────────
async fn virustotal(value: &str, kind: &str, o: &mut Outcome) {
    let Some(k) = key("virustotal", o) else { return };
    let url = if kind == "ip" {
        format!("https://www.virustotal.com/api/v3/ip_addresses/{}", enc(value))
    } else {
        format!("https://www.virustotal.com/api/v3/domains/{}", enc(value))
    };
    let resp = match client().get(&url).header("x-apikey", k).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if !resp.status().is_success() { o.log.push(format!("✗  VirusTotal: HTTP {}", resp.status().as_u16())); return; }
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let a = j.pointer("/data/attributes");
    if let Some(stats) = a.and_then(|x| x.get("last_analysis_stats")) {
        let g = |k: &str| stats.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
        o.log.push(format!("✓  VT: {} malicious / {} suspicious / {} harmless",
            g("malicious"), g("suspicious"), g("harmless")));
        o.props.push(("vt_malicious".into(), g("malicious").to_string()));
        o.props.push(("vt_suspicious".into(), g("suspicious").to_string()));
    }
    if let Some(owner) = a.and_then(|x| x.get("as_owner")).and_then(|v| v.as_str()) {
        o.item(Kind::Organization, owner.to_string(), "AS owner");
    }
    if let Some(c) = a.and_then(|x| x.get("country")).and_then(|v| v.as_str()) {
        o.item(Kind::Location, c.to_string(), "country");
    }
}

// ── Have I Been Pwned (key) ────────────────────────────────────────────────────
async fn hibp(email: &str, o: &mut Outcome) {
    let Some(k) = key("hibp", o) else { return };
    let url = format!("https://haveibeenpwned.com/api/v3/breachedaccount/{}?truncateResponse=false", enc(email));
    let resp = match client().get(&url)
        .header("hibp-api-key", k).header("user-agent", "parasite-osint").send().await
    {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if resp.status().as_u16() == 404 { o.log.push("✓  no breaches found".into()); return; }
    if !resp.status().is_success() { o.log.push(format!("✗  HIBP: HTTP {}", resp.status().as_u16())); return; }
    let arr: Vec<serde_json::Value> = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    o.log.push(format!("⚠  {} breach(es)!", arr.len()));
    o.props.push(("breaches".into(), arr.len().to_string()));
    for b in arr {
        let name = b.get("Name").and_then(|v| v.as_str()).unwrap_or("?");
        let date = b.get("BreachDate").and_then(|v| v.as_str()).unwrap_or("");
        o.item(Kind::Phrase, format!("{name} ({date})"), "breached in");
    }
}

// ── AbuseIPDB (key) ────────────────────────────────────────────────────────────
async fn abuseipdb(ip: &str, o: &mut Outcome) {
    let Some(k) = key("abuseipdb", o) else { return };
    let url = format!("https://api.abuseipdb.com/api/v2/check?ipAddress={}&maxAgeInDays=90", enc(ip));
    let resp = match client().get(&url).header("Key", k).header("Accept", "application/json").send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if !resp.status().is_success() { o.log.push(format!("✗  AbuseIPDB: HTTP {}", resp.status().as_u16())); return; }
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let d = j.get("data");
    let g = |k: &str| d.and_then(|x| x.get(k));
    if let Some(score) = g("abuseConfidenceScore").and_then(|v| v.as_u64()) {
        o.log.push(format!("✓  abuse score {score}% ({} report(s))",
            g("totalReports").and_then(|v| v.as_u64()).unwrap_or(0)));
        o.props.push(("abuse_score".into(), format!("{score}%")));
    }
    if let Some(isp) = g("isp").and_then(|v| v.as_str()) { o.item(Kind::Organization, isp.to_string(), "ISP"); }
    if let Some(dom) = g("domain").and_then(|v| v.as_str()) { if !dom.is_empty() { o.item(Kind::Domain, dom.to_string(), "domain"); } }
    if let Some(cc) = g("countryCode").and_then(|v| v.as_str()) { o.item(Kind::Location, cc.to_string(), "country"); }
}

// ── Hunter.io (key) ────────────────────────────────────────────────────────────
async fn hunter(domain: &str, o: &mut Outcome) {
    let Some(k) = key("hunter", o) else { return };
    let url = format!("https://api.hunter.io/v2/domain-search?domain={}&api_key={}", enc(domain), enc(&k));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if !resp.status().is_success() { o.log.push(format!("✗  Hunter: HTTP {}", resp.status().as_u16())); return; }
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    if let Some(org) = j.pointer("/data/organization").and_then(|v| v.as_str()) {
        if !org.is_empty() { o.item(Kind::Organization, org.to_string(), "organization"); }
    }
    let mut n = 0;
    if let Some(emails) = j.pointer("/data/emails").and_then(|v| v.as_array()) {
        for e in emails {
            if let Some(addr) = e.get("value").and_then(|v| v.as_str()) {
                n += 1;
                o.item(Kind::Email, addr.to_string(), "hunter");
            }
        }
    }
    o.log.push(format!("✓  Hunter.io: {n} email(s)"));
}

// ── Shodan InternetDB (free, no key) ───────────────────────────────────────────
async fn internetdb(ip: &str, o: &mut Outcome) {
    let url = format!("https://internetdb.shodan.io/{}", enc(ip.trim()));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if resp.status().as_u16() == 404 { o.log.push("◦  no InternetDB data for this IP".into()); return; }
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let arr = |k: &str| j.get(k).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let ports = arr("ports");
    for p in &ports { if let Some(n) = p.as_u64() { o.item(Kind::Port, n.to_string(), "open port"); } }
    for h in arr("hostnames") { if let Some(s) = h.as_str() { o.item(Kind::Domain, s.to_string(), "hostname"); } }
    for vu in arr("vulns") { if let Some(s) = vu.as_str() { o.item(Kind::Cve, s.to_string(), "vulnerable to"); } }
    for t in arr("tags") { if let Some(s) = t.as_str() { o.item(Kind::Phrase, s.to_string(), "tag"); } }
    for c in arr("cpes").iter().take(10) { if let Some(s) = c.as_str() { o.item(Kind::Service, s.to_string(), "cpe"); } }
    o.log.push(format!("✓  {} port(s), {} vuln(s), {} hostname(s)",
        ports.len(), arr("vulns").len(), arr("hostnames").len()));
}

// ── ip-api.com (free, no key) ──────────────────────────────────────────────────
async fn ipapi(ip: &str, o: &mut Outcome) {
    let url = format!("http://ip-api.com/json/{}?fields=status,country,regionName,city,lat,lon,isp,org,as,query", enc(ip.trim()));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let s = |k: &str| j.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    if j.get("status").and_then(|v| v.as_str()) != Some("success") {
        o.log.push("◦  ip-api returned no data".into()); return;
    }
    let loc: Vec<String> = [s("city"), s("regionName"), s("country")].into_iter().filter(|x| !x.is_empty()).collect();
    if !loc.is_empty() { o.item(Kind::Location, loc.join(", "), "located in"); }
    if !s("org").is_empty()  { o.item(Kind::Organization, s("org"), "org"); }
    if !s("isp").is_empty() && s("isp") != s("org") { o.item(Kind::Organization, s("isp"), "ISP"); }
    let asn = s("as");
    if asn.starts_with("AS") { o.item(Kind::Asn, asn.split_whitespace().next().unwrap_or(&asn).to_string(), "ASN"); }
    if let (Some(la), Some(lo)) = (j.get("lat").and_then(|v| v.as_f64()), j.get("lon").and_then(|v| v.as_f64())) {
        o.item(Kind::Coordinate, format!("{la:.4}, {lo:.4}"), "geo");
        o.log.push(format!("✓  {} · {la:.4},{lo:.4}", loc.join(", ")));
    }
}

// ── AlienVault OTX (free) ──────────────────────────────────────────────────────
async fn otx_domain(domain: &str, o: &mut Outcome) {
    let url = format!("https://otx.alienvault.com/api/v1/indicators/domain/{}/passive_dns", enc(domain));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let mut n = 0;
    if let Some(arr) = j.get("passive_dns").and_then(|v| v.as_array()) {
        let mut seen: Vec<String> = Vec::new();
        for rec in arr {
            if let Some(h) = rec.get("hostname").and_then(|v| v.as_str()) {
                let h = h.to_lowercase();
                if h.ends_with(domain) && !seen.contains(&h) { seen.push(h.clone()); if seen.len() <= 150 { o.item(Kind::Domain, h, "passive dns"); } }
            }
            if let Some(a) = rec.get("address").and_then(|v| v.as_str()) {
                if a.parse::<std::net::Ipv4Addr>().is_ok() { o.item(Kind::Ip, a.to_string(), "resolved"); }
            }
            n += 1;
        }
    }
    o.log.push(format!("✓  {n} passive-DNS record(s) from OTX"));
}

async fn otx_ip(ip: &str, o: &mut Outcome) {
    let url = format!("https://otx.alienvault.com/api/v1/indicators/IPv4/{}/general", enc(ip));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let pulses = j.pointer("/pulse_info/count").and_then(|v| v.as_u64()).unwrap_or(0);
    o.log.push(format!("{}  {pulses} threat pulse(s)", if pulses > 0 { "⚠" } else { "✓" }));
    o.props.push(("otx_pulses".into(), pulses.to_string()));
    if let Some(asn) = j.get("asn").and_then(|v| v.as_str()) { o.item(Kind::Asn, asn.split_whitespace().next().unwrap_or(asn).to_string(), "ASN"); }
    if let Some(c) = j.get("country_name").and_then(|v| v.as_str()) { if !c.is_empty() { o.item(Kind::Location, c.to_string(), "country"); } }
}

async fn urlscan(domain: &str, o: &mut Outcome) {
    let url = format!("https://urlscan.io/api/v1/search/?q=domain:{}&size=25", enc(domain));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let mut n = 0;
    if let Some(arr) = j.get("results").and_then(|v| v.as_array()) {
        let mut seen: Vec<String> = Vec::new();
        for r in arr {
            if let Some(u) = r.pointer("/page/url").and_then(|v| v.as_str()) {
                if !seen.contains(&u.to_string()) { seen.push(u.to_string()); o.item(Kind::Website, u.to_string(), "urlscan"); n += 1; }
            }
            if let Some(ip) = r.pointer("/page/ip").and_then(|v| v.as_str()) {
                if ip.parse::<std::net::Ipv4Addr>().is_ok() { o.item(Kind::Ip, ip.to_string(), "hosted on"); }
            }
        }
    }
    o.log.push(format!("✓  {n} urlscan submission(s)"));
}

async fn circl_hash(hash: &str, o: &mut Outcome) {
    let h = hash.trim();
    let algo = match h.len() { 32 => "md5", 40 => "sha1", 64 => "sha256", _ => { o.log.push("◦  need an MD5/SHA1/SHA256 hash".into()); return; } };
    let url = format!("https://hashlookup.circl.lu/lookup/{algo}/{}", enc(h));
    let resp = match client().get(&url).header("Accept", "application/json").send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if resp.status().as_u16() == 404 { o.log.push("◦  hash not known to CIRCL".into()); return; }
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    if let Some(name) = j.get("FileName").and_then(|v| v.as_str()) {
        o.log.push(format!("✓  known file: {name}"));
        o.props.push(("filename".into(), name.into()));
        o.item(Kind::File, name.to_string(), "known as");
    }
    if let Some(src) = j.get("source").and_then(|v| v.as_str()) { o.props.push(("source".into(), src.into())); }
    o.props.push(("known".into(), "yes (CIRCL)".into()));
}

// ── more real APIs ─────────────────────────────────────────────────────────────
async fn certspotter(domain: &str, o: &mut Outcome) {
    let url = format!("https://api.certspotter.com/v1/issuances?domain={}&include_subdomains=true&expand=dns_names", enc(domain));
    o.log.push("◦  querying CertSpotter…".into());
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if !resp.status().is_success() { o.log.push(format!("✗  CertSpotter: HTTP {}", resp.status().as_u16())); return; }
    let arr: Vec<serde_json::Value> = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let mut seen: Vec<String> = Vec::new();
    for issuance in arr {
        if let Some(names) = issuance.get("dns_names").and_then(|v| v.as_array()) {
            for nm in names {
                if let Some(h) = nm.as_str() {
                    let h = h.trim_start_matches("*.").to_lowercase();
                    if h.ends_with(domain) && h != domain && !seen.contains(&h) {
                        seen.push(h.clone());
                        if seen.len() <= 200 { o.item(Kind::Domain, h, "subdomain"); }
                    }
                }
            }
        }
    }
    o.log.push(format!("✓  {} subdomain(s) from CertSpotter", seen.len()));
}

async fn nvd_cve(cve: &str, o: &mut Outcome) {
    let id = cve.trim().to_uppercase();
    let url = format!("https://services.nvd.nist.gov/rest/json/2.0/cves/2.0?cveId={}", enc(&id));
    let resp = match client().get(&url).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let Some(cve_obj) = j.pointer("/vulnerabilities/0/cve") else {
        o.log.push("◦  CVE not found in NVD".into()); return;
    };
    if let Some(desc) = cve_obj.pointer("/descriptions/0/value").and_then(|v| v.as_str()) {
        o.log.push(format!("✓  {desc}"));
        o.props.push(("description".into(), desc.chars().take(300).collect()));
    }
    // CVSS v3.1 then v2 fallback
    let metric = cve_obj.pointer("/metrics/cvssMetricV31/0/cvssData")
        .or_else(|| cve_obj.pointer("/metrics/cvssMetricV30/0/cvssData"))
        .or_else(|| cve_obj.pointer("/metrics/cvssMetricV2/0/cvssData"));
    if let Some(m) = metric {
        if let Some(score) = m.get("baseScore").and_then(|v| v.as_f64()) {
            let sev = m.get("baseSeverity").and_then(|v| v.as_str()).unwrap_or("");
            o.log.push(format!("⚠  CVSS {score} {sev}"));
            o.props.push(("cvss".into(), format!("{score} {sev}")));
            o.item(Kind::Phrase, format!("CVSS {score} {sev}"), "severity");
        }
    }
    if let Some(refs) = cve_obj.pointer("/references").and_then(|v| v.as_array()) {
        for r in refs.iter().take(10) {
            if let Some(u) = r.get("url").and_then(|v| v.as_str()) { o.item(Kind::Website, u.to_string(), "reference"); }
        }
    }
}

async fn virustotal_file(hash: &str, o: &mut Outcome) {
    let Some(k) = key("virustotal", o) else { return };
    let url = format!("https://www.virustotal.com/api/v3/files/{}", enc(hash.trim()));
    let resp = match client().get(&url).header("x-apikey", k).send().await {
        Ok(r) => r, Err(e) => { o.log.push(format!("✗  {e}")); return; }
    };
    if resp.status().as_u16() == 404 { o.log.push("◦  hash unknown to VirusTotal".into()); return; }
    if !resp.status().is_success() { o.log.push(format!("✗  VirusTotal: HTTP {}", resp.status().as_u16())); return; }
    let j: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap_or_default()).unwrap_or_default();
    let a = j.pointer("/data/attributes");
    if let Some(stats) = a.and_then(|x| x.get("last_analysis_stats")) {
        let mal = stats.get("malicious").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = stats.as_object().map(|m| m.values().filter_map(|v| v.as_u64()).sum::<u64>()).unwrap_or(0);
        o.log.push(format!("⚠  VT: {mal}/{total} engines flagged it"));
        o.props.push(("vt_detections".into(), format!("{mal}/{total}")));
        o.item(Kind::Phrase, format!("VT {mal}/{total} malicious"), "verdict");
    }
    if let Some(name) = a.and_then(|x| x.get("meaningful_name")).and_then(|v| v.as_str()) {
        o.item(Kind::File, name.to_string(), "filename");
    }
}

// ── external GitHub CLI tools (run if installed) ───────────────────────────────
/// Run a command, returning combined stdout+stderr, or None if the binary is
/// missing. Does not log on its own.
async fn try_tool(bin: &str, args: &[&str]) -> Option<String> {
    use tokio::process::Command;
    match Command::new(bin).args(args).output().await {
        Ok(out) => Some(format!("{}{}",
            String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))),
        Err(_) => None,
    }
}

async fn run_tool(bin: &str, args: &[&str], o: &mut Outcome) -> Option<String> {
    o.log.push(format!("◦  running {bin}…"));
    let out = try_tool(bin, args).await;
    if out.is_none() {
        o.log.push(format!("✗  '{bin}' not installed — install it (e.g. pipx install {bin}) to use this"));
    }
    out
}

async fn holehe(email: &str, o: &mut Outcome) {
    o.log.push("◦  running holehe…".into());
    // try the CLI, then `python3 -m holehe` as a fallback
    let mut out = try_tool("holehe", &["--only-used", "--no-color", email]).await;
    if out.is_none() {
        out = try_tool("python3", &["-m", "holehe", "--only-used", "--no-color", email]).await;
    }
    let Some(out) = out else {
        o.log.push("✗  holehe not found — install with: pipx install holehe".into());
        return;
    };
    let mut n = 0;
    for line in out.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("[+]") {
            let site = rest.trim();
            if !site.is_empty() {
                n += 1;
                o.item(Kind::Social, format!("https://{site}"), "registered");
                o.props.push(("holehe".into(), site.to_string()));
            }
        }
    }
    o.log.push(format!("✓  holehe: {n} site(s) where this email is used"));
}

async fn maigret(username: &str, o: &mut Outcome) {
    o.log.push("◦  running maigret…".into());
    let mut out = try_tool("maigret", &[username, "--no-color", "--timeout", "10"]).await;
    if out.is_none() {
        out = try_tool("python3", &["-m", "maigret", username, "--no-color", "--timeout", "10"]).await;
    }
    let Some(out) = out else {
        o.log.push("✗  maigret not found — install with: pipx install maigret".into());
        return;
    };
    let re = Regex::new(r"https?://[^\s]+").unwrap();
    let mut seen: Vec<String> = Vec::new();
    for line in out.lines() {
        if line.contains("[+]") {
            if let Some(m) = re.find(line) {
                let u = m.as_str().to_string();
                if !seen.contains(&u) { seen.push(u.clone()); o.item(Kind::Social, u, "maigret"); }
            }
        }
    }
    o.log.push(format!("✓  maigret: {} profile(s)", seen.len()));
}

async fn sherlock_cli(username: &str, o: &mut Outcome) {
    let Some(out) = run_tool("sherlock", &["--print-found", "--no-color", "--timeout", "10", username], o).await else { return };
    let re = Regex::new(r"https?://[^\s]+").unwrap();
    let mut seen: Vec<String> = Vec::new();
    for line in out.lines() {
        if line.contains("[+]") {
            if let Some(m) = re.find(line) {
                let u = m.as_str().to_string();
                if !seen.contains(&u) { seen.push(u.clone()); o.item(Kind::Social, u, "sherlock"); }
            }
        }
    }
    o.log.push(format!("✓  sherlock: {} profile(s)", seen.len()));
}

async fn waybackurls_cli(domain: &str, o: &mut Outcome) {
    let Some(out) = run_tool("waybackurls", &[domain], o).await else { return };
    let mut n = 0;
    for line in out.lines() {
        let u = line.trim();
        if u.starts_with("http") { n += 1; if n <= 200 { o.item(Kind::Website, u.to_string(), "archived"); } }
    }
    o.log.push(format!("✓  waybackurls: {n} URL(s)"));
}

async fn subfinder(domain: &str, o: &mut Outcome) {
    let Some(out) = run_tool("subfinder", &["-d", domain, "-silent"], o).await else { return };
    let mut n = 0;
    for line in out.lines() {
        let h = line.trim().to_lowercase();
        if h.ends_with(domain) && !h.is_empty() { n += 1; if n <= 300 { o.item(Kind::Domain, h, "subdomain"); } }
    }
    o.log.push(format!("✓  subfinder: {n} subdomain(s)"));
}

async fn the_harvester(domain: &str, o: &mut Outcome) {
    let Some(out) = run_tool("theHarvester", &["-d", domain, "-b", "duckduckgo,bing,crtsh"], o).await else { return };
    let mail_re = Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").unwrap();
    let host_re = Regex::new(r"[A-Za-z0-9._\-]+\.[A-Za-z0-9.\-]+").unwrap();
    let mut emails: Vec<String> = Vec::new();
    let mut hosts: Vec<String> = Vec::new();
    for m in mail_re.find_iter(&out) {
        let e = m.as_str().to_lowercase();
        if !emails.contains(&e) { emails.push(e.clone()); o.item(Kind::Email, e, "harvested"); }
    }
    for m in host_re.find_iter(&out) {
        let h = m.as_str().to_lowercase();
        if h.ends_with(domain) && h != domain && !hosts.contains(&h) && hosts.len() < 200 {
            hosts.push(h.clone()); o.item(Kind::Domain, h, "subdomain");
        }
    }
    o.log.push(format!("✓  theHarvester: {} email(s), {} host(s)", emails.len(), hosts.len()));
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
