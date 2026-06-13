/// PROBE DEFENSES — сканировать защиту хоста
use anyhow::Result;
use url::Url;

use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("PROBE DEFENSES — сканирование защиты");
    println!();

    let target = ui::prompt("Target URL:");
    if target.is_empty() { ui::err("URL обязателен"); return Ok(()); }

    let url = match Url::parse(&target) {
        Ok(u) => u,
        Err(e) => { ui::err(&format!("{e}")); return Ok(()); }
    };

    let host   = url.host_str().unwrap_or("?").to_string();
    let scheme = url.scheme();

    println!();
    println!("  {GRAY}Зондируем защиту...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    // ── HTTP-заголовки безопасности ───────────────────────────────────────────
    let resp = match client.get(&target).send().await {
        Ok(r)  => r,
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };

    let status = resp.status().as_u16();
    let hdrs   = resp.headers().clone();
    ui::cursor_up(1);

    // Список security-заголовков с оценкой
    let checks: &[(&str, &str, &str)] = &[
        ("strict-transport-security", "HSTS",                   "Принудительный HTTPS"),
        ("content-security-policy",   "CSP",                    "Политика контента"),
        ("x-frame-options",           "X-Frame-Options",        "Защита от clickjacking"),
        ("x-content-type-options",    "X-Content-Type-Options", "MIME-sniffing защита"),
        ("x-xss-protection",          "X-XSS-Protection",       "XSS фильтр"),
        ("referrer-policy",           "Referrer-Policy",        "Политика Referrer"),
        ("permissions-policy",        "Permissions-Policy",     "Политика разрешений"),
        ("cross-origin-opener-policy","COOP",                   "Cross-Origin Opener"),
    ];

    let mut vuln_count = 0usize;

    println!();
    println!("  {DRED}╔══════════════════════ {BRED}{BOLD}SECURITY HEADERS @ {host}{RESET}{DRED} ══════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}HTTP {status}{RESET}{: <63}{DRED}║{RESET}", "");
    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");

    for (hdr, name, desc) in checks {
        if let Some(val) = hdrs.get(*hdr).and_then(|v| v.to_str().ok()) {
            let v = if val.len() > 36 { format!("{}…", &val[..35]) } else { val.to_string() };
            println!(
                "  {DRED}║{RESET}  {BRED}✓{RESET}  {WHITE}{name:<28}{RESET}  {GRAY}{v:<36}{RESET}  {DRED}║{RESET}"
            );
        } else {
            vuln_count += 1;
            println!(
                "  {DRED}║{RESET}  {RED}✗{RESET}  {RED}{name:<28}{RESET}  {GRAY}{desc:<36}{RESET}  {DRED}║{RESET}"
            );
        }
    }

    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");

    // Оценка
    let (grade, gc) = match vuln_count {
        0 => ("A+", BRED),
        1 => ("A",  BRED),
        2 => ("B",  RED),
        3 => ("C",  RED),
        4 => ("D",  DRED),
        _ => ("F",  DRED),
    };
    println!(
        "  {DRED}║{RESET}  {GRAY}Оценка безопасности:{RESET}  {gc}{BOLD}{grade}{RESET}  {GRAY}({vuln_count}/{} заголовков отсутствует){RESET}{: <20}{DRED}║{RESET}",
        checks.len(), ""
    );
    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");

    // ── robots.txt ────────────────────────────────────────────────────────────
    let robots_url = format!("{scheme}://{host}/robots.txt");
    println!("  {DRED}║{RESET}  {GRAY}Robots.txt:{RESET}  {WHITE}{robots_url}{RESET}{: <30}{DRED}║{RESET}", "");
    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");

    match client.get(&robots_url).send().await {
        Ok(r) if r.status().is_success() => {
            let body = r.text().await.unwrap_or_default();
            let disallow_count = body.lines()
                .filter(|l| l.trim_start().starts_with("Disallow:"))
                .count();
            let has_crawl_delay = body.lines()
                .any(|l| l.trim_start().starts_with("Crawl-delay:"));

            println!("  {DRED}║{RESET}  {BRED}✓{RESET}  Найден  |  {GRAY}{} Disallow правил{RESET}{: <40}{DRED}║{RESET}",
                disallow_count, "");

            if has_crawl_delay {
                println!("  {DRED}║{RESET}  {RED}⚠{RESET}  {RED}Crawl-delay указан — замедляем заражение{RESET}{: <28}{DRED}║{RESET}", "");
            }

            // Показать первые Disallow
            for line in body.lines().filter(|l| l.starts_with("Disallow:")).take(6) {
                let path = line.trim_start_matches("Disallow:").trim();
                println!("  {DRED}║{RESET}  {DRED}  ▸{RESET}  {GRAY}{path:<64}{RESET}  {DRED}║{RESET}");
            }
        }
        Ok(r) => {
            println!("  {DRED}║{RESET}  {RED}✗{RESET}  HTTP {}  — ограничений нет{: <38}{DRED}║{RESET}", r.status().as_u16(), "");
        }
        Err(_) => {
            println!("  {DRED}║{RESET}  {GRAY}⊘{RESET}  robots.txt недоступен{: <46}{DRED}║{RESET}", "");
        }
    }

    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");
    println!("  {DRED}╚══════════════════════════════════════════════════════════════════════╝{RESET}");
    Ok(())
}
