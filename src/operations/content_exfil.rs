use anyhow::Result;
use regex::Regex;
use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;
use crate::ui::{self, color::*};

struct Pattern {
    name:    &'static str,
    re:      &'static str,
    sev:     &'static str,
}

const PATTERNS: &[Pattern] = &[
    Pattern { name: "AWS Access Key",    re: r"AKIA[0-9A-Z]{16}",                                           sev: "CRITICAL" },
    Pattern { name: "AWS Secret Key",    re: r#"(?i)aws.{0,20}secret.{0,20}['"][0-9a-zA-Z/+]{40}['"]"#,    sev: "CRITICAL" },
    Pattern { name: "JWT Token",         re: r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",        sev: "CRITICAL" },
    Pattern { name: "Private Key",       re: r"-----BEGIN.{0,20}PRIVATE KEY-----",                          sev: "CRITICAL" },
    Pattern { name: "Google API Key",    re: r"AIza[0-9A-Za-z\-_]{35}",                                     sev: "HIGH"     },
    Pattern { name: "Generic API Key",   re: r#"(?i)api[_-]?key\s*[:=]\s*['""]?[0-9a-zA-Z\-_]{20,64}['""]?"#, sev: "HIGH" },
    Pattern { name: "Bearer Token",      re: r"Bearer\s+[A-Za-z0-9\-_\.]+",                                 sev: "HIGH"     },
    Pattern { name: "GitHub Token",      re: r"gh[pousr]_[A-Za-z0-9_]{36}",                                 sev: "CRITICAL" },
    Pattern { name: "Slack Token",       re: r"xox[baprs]-[0-9A-Za-z\-]{10,48}",                           sev: "HIGH"     },
    Pattern { name: "Password in code",  re: r#"(?i)password\s*[:=]\s*['""][^\s'"]{6,}['""]"#,              sev: "HIGH"     },
    Pattern { name: "DB Connection",     re: r#"(?i)(mysql|postgres|mongodb)://[^\s<>"']+"#,                 sev: "CRITICAL" },
    Pattern { name: "IP Address",        re: r"\b(?:10|172\.(?:1[6-9]|2\d|3[01])|192\.168)\.\d{1,3}\.\d{1,3}\b", sev: "MEDIUM" },
    Pattern { name: "Email Address",     re: r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}",          sev: "LOW"      },
    Pattern { name: "Phone Number",      re: r"\+?[1-9][0-9]{7,14}",                                        sev: "LOW"      },
];

pub async fn run() -> Result<()> {
    ui::section("content exfiltration — умный сбор данных");
    println!();

    let target    = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }
    let pages_s   = ui::prompt_default("страниц для сканирования:", "20");
    let max_pages = pages_s.parse::<usize>().unwrap_or(20);

    let root = match Url::parse(&target) {
        Ok(u) => u, Err(e) => { ui::err(&format!("{e}")); return Ok(()); }
    };
    let own_host = root.host_str().unwrap_or("").to_string();

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let compiled: Vec<(&Pattern, Regex)> = PATTERNS.iter()
        .filter_map(|p| Regex::new(p.re).ok().map(|r| (p, r)))
        .collect();

    let mut visited: HashSet<String> = HashSet::new();
    let mut queue                    = vec![root.to_string()];
    let mut findings: Vec<(String, String, String)> = vec![]; // (name, match, page)
    let link_sel = Selector::parse("a[href],script[src]").unwrap();

    while !queue.is_empty() && visited.len() < max_pages {
        let url_str = queue.remove(0);
        if visited.contains(&url_str) { continue; }
        visited.insert(url_str.clone());

        let short = if url_str.len() > 60 { format!("{}…", &url_str[..59]) } else { url_str.clone() };
        println!("  {GRAY}  [{:>2}/{}] {short}{RESET}", visited.len(), max_pages);
        ui::flush();

        let html = match client.get(&url_str).send().await {
            Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
            _ => { ui::cursor_up(1); continue; }
        };
        ui::cursor_up(1);

        // Scan HTML + inline scripts
        for (pat, re) in &compiled {
            for m in re.find_iter(&html).take(5) {
                let matched = m.as_str().to_string();
                if matched.len() > 4 {
                    let key = format!("{}:{}", pat.name, &matched[..matched.len().min(40)]);
                    if !findings.iter().any(|f| f.0 == key) {
                        findings.push((key, matched, url_str.clone()));
                    }
                }
            }
        }

        // Queue more pages + JS files
        let doc = Html::parse_document(&html);
        for el in doc.select(&link_sel) {
            let href = el.value().attr("href")
                .or(el.value().attr("src"))
                .unwrap_or("");
            if let Ok(u) = root.join(href) {
                if u.host_str().unwrap_or("") == own_host && !visited.contains(&u.to_string()) {
                    queue.push(u.to_string());
                }
            }
        }
    }

    println!();
    println!("  {DRED}╔══════════════════ {BRED}{BOLD}content exfiltration{RESET}{DRED} ══════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}страниц:{RESET}  {WHITE}{}{RESET}   {RED}находок:{RESET}  {RED}{BOLD}{}{RESET}", visited.len(), findings.len());
    println!("  {DRED}║{RESET}");

    if findings.is_empty() {
        println!("  {DRED}║{RESET}  {BRED}✓{RESET}  чувствительных данных не обнаружено");
    } else {
        // Group by pattern
        for pat in PATTERNS {
            let group: Vec<_> = findings.iter().filter(|f| f.0.starts_with(pat.name)).collect();
            if group.is_empty() { continue; }

            let (sc, bullet) = match pat.sev {
                "CRITICAL" => (BRED, "██ CRITICAL"),
                "HIGH"     => (RED,  "▓▓ HIGH    "),
                "MEDIUM"   => (RED,  "▒▒ MEDIUM  "),
                _          => (GRAY, "░░ LOW     "),
            };
            println!("  {DRED}║{RESET}  {sc}{bullet}  {BOLD}{}{RESET}  ({} шт.)", pat.name, group.len());
            for (_, matched, page) in group.iter().take(3) {
                let m_short = if matched.len() > 55 { format!("{}…", &matched[..54]) } else { matched.clone() };
                let p_short = if page.len() > 50 { format!("{}…", &page[..49]) } else { page.clone() };
                println!("  {DRED}║{RESET}    {sc}{m_short}{RESET}");
                println!("  {DRED}║{RESET}    {GRAY}↳ {p_short}{RESET}");
            }
            println!("  {DRED}║{RESET}");
        }
    }

    println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}
