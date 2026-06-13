/// ANALYZE HOST — биопсия страницы
use anyhow::Result;
use std::time::Instant;
use url::Url;

use crate::parser::HtmlParser;
use crate::scoring::ScoringEngine;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("ANALYZE HOST — биопсия");
    println!();

    let target = ui::prompt("Target URL:");
    if target.is_empty() { ui::err("URL обязателен"); return Ok(()); }

    let url = match Url::parse(&target) {
        Ok(u) => u,
        Err(e) => { ui::err(&format!("Некорректный URL: {e}")); return Ok(()); }
    };

    println!();
    println!("  {GRAY}Анализируем...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(20))
        .gzip(true)
        .build()?;

    let t0   = Instant::now();
    let resp = match client.get(&target).send().await {
        Ok(r)  => r,
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };

    let status     = resp.status().as_u16();
    let ok         = resp.status().is_success();
    let hdrs       = resp.headers().clone();
    let html       = resp.text().await.unwrap_or_default();
    let elapsed_ms = t0.elapsed().as_millis();

    ui::cursor_up(1);

    let page_type  = HtmlParser::detect_page_type(&url, &html);
    let title      = HtmlParser::extract_title(&html).unwrap_or_else(|| "—".into());
    let links      = HtmlParser::extract_links(&html, &url);
    let score      = ScoringEngine::score(&url, 0);
    let ct         = hdrs.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("?");
    let server     = hdrs.get("server").and_then(|v| v.to_str().ok()).unwrap_or("—");
    let powered_by = hdrs.get("x-powered-by").and_then(|v| v.to_str().ok()).unwrap_or("—");
    let csp        = hdrs.get("content-security-policy").is_some();
    let hsts       = hdrs.get("strict-transport-security").is_some();
    let cors       = hdrs.get("access-control-allow-origin").and_then(|v| v.to_str().ok()).unwrap_or("—");

    let sc_color = if ok { BRED } else { RED };
    let type_icon = match format!("{}", page_type).as_str() {
        "API"     => "◉ API",
        "Docs"    => "◉ Docs",
        "Blog"    => "◉ Blog",
        "Profile" => "◉ Profile",
        "Product" => "◉ Product",
        "Catalog" => "◉ Catalog",
        _         => "◉ Page",
    };

    println!();
    println!("  {DRED}╔══════════════════════════ {BRED}{BOLD}ANALYSIS{RESET}{DRED} ══════════════════════════════╗{RESET}");
    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");
    println!("  {DRED}║{RESET}   {BRED}{BOLD}{type_icon:<24}{RESET}  {GRAY}Score:{RESET} {sc_color}{score}{RESET}{: <34}{DRED}║{RESET}", "");
    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");

    let show = |label: &str, value: &str| {
        let v = if value.len() > 52 { format!("{}…", &value[..51]) } else { value.to_string() };
        println!("  {DRED}║{RESET}   {GRAY}{label:<18}{RESET}  {WHITE}{v:<52}{RESET}  {DRED}║{RESET}");
    };

    show("URL",          &target);
    show("Title",        &title);
    show("HTTP Status",  &format!("{status} {}", if ok {"✓"} else {"✗"}));
    show("Response",     &format!("{elapsed_ms} ms  •  {} байт", html.len()));
    show("Content-Type", ct);
    show("Server",       server);
    show("X-Powered-By", powered_by);
    show("CORS Origin",  cors);
    show("Links found",  &links.len().to_string());
    show("CSP Header",   if csp { "Present ✓" } else { "Missing ✗" });
    show("HSTS Header",  if hsts { "Present ✓" } else { "Missing ✗" });

    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");
    println!("  {DRED}╚══════════════════════════════════════════════════════════════════════╝{RESET}");
    Ok(())
}
