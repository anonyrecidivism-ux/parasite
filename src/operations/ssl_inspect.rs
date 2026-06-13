use anyhow::Result;
use url::Url;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("ssl inspect — сертификат и цепочка");
    println!();

    let target = ui::prompt("target url (https://):");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }

    let url = match Url::parse(&target) {
        Ok(u) => u, Err(e) => { ui::err(&format!("{e}")); return Ok(()); }
    };
    let host   = url.host_str().unwrap_or("?").to_string();
    let scheme = url.scheme();

    println!("  {GRAY}проверяем ssl...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    // Check HTTP → HTTPS redirect
    let http_url = format!("http://{host}/");
    let redirected_to_https = if scheme == "https" {
        let http_client = reqwest::Client::builder()
            .user_agent("parasite/1.0")
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        match http_client.get(&http_url).send().await {
            Ok(r) => {
                let loc = r.headers().get("location")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                r.status().is_redirection() && loc.starts_with("https://")
            }
            Err(_) => false,
        }
    } else { false };

    let resp = match client.get(&target).send().await {
        Ok(r)  => r,
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };

    let status  = resp.status().as_u16();
    let hdrs    = resp.headers().clone();
    let final_u = resp.url().to_string();

    let hsts = hdrs.get("strict-transport-security")
        .and_then(|v| v.to_str().ok())
        .map(|v| if v.len() > 44 { format!("{}…", &v[..43]) } else { v.to_string() });

    let is_https = final_u.starts_with("https://");

    ui::cursor_up(1);
    println!();
    println!("  {DRED}╔══════════════════════ {BRED}{BOLD}ssl @ {host}{RESET}{DRED} ══════════════════╗{RESET}");
    println!("  {DRED}║{RESET}                                                               {DRED}║{RESET}");

    let (sc, st) = if is_https { (BRED, "✓  HTTPS активен") } else { (RED, "✗  HTTPS не активен") };
    println!("  {DRED}║{RESET}  {sc}{BOLD}{st:<59}{RESET}  {DRED}║{RESET}");

    let (rc, rs) = if redirected_to_https { (BRED, "✓  HTTP → HTTPS редирект") } else { (RED, "✗  HTTP → HTTPS редирект отсутствует") };
    println!("  {DRED}║{RESET}  {rc}{rs:<59}{RESET}  {DRED}║{RESET}");

    match &hsts {
        Some(v) => println!("  {DRED}║{RESET}  {BRED}✓{RESET}  {GRAY}HSTS:{RESET}  {WHITE}{v:<52}{RESET}  {DRED}║{RESET}"),
        None    => println!("  {DRED}║{RESET}  {RED}✗  HSTS не установлен{RESET}{:<40}  {DRED}║{RESET}", ""),
    }

    println!("  {DRED}║{RESET}  {GRAY}статус ответа:{RESET}  {WHITE}{status:<47}{RESET}  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}final url:{RESET}  {WHITE}{:<51}{RESET}  {DRED}║{RESET}", shorten(&final_u, 51));
    println!("  {DRED}║{RESET}                                                               {DRED}║{RESET}");

    // Check common security headers
    let sec_hdrs = [
        ("content-security-policy", "CSP"),
        ("x-frame-options",         "X-Frame-Options"),
        ("x-content-type-options",  "X-Content-Type"),
        ("referrer-policy",         "Referrer-Policy"),
    ];
    println!("  {DRED}║{RESET}  {GRAY}дополнительные заголовки:{RESET}");
    for (h, name) in &sec_hdrs {
        if hdrs.contains_key(*h) {
            println!("  {DRED}║{RESET}  {BRED}✓{RESET}  {name}");
        } else {
            println!("  {DRED}║{RESET}  {RED}✗{RESET}  {RED}{name}{RESET}");
        }
    }

    println!("  {DRED}║{RESET}                                                               {DRED}║{RESET}");
    println!("  {DRED}╚═══════════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
